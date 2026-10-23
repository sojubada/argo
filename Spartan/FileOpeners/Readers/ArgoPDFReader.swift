//
//  ArgoPDFReader.swift
//  Argo
//
//  PDF 읽기. 네이티브로 쪽을 다룬다.
//
//  PDFKit 은 tvOS SDK 에 없고, 인쇄 서식기는 바이너리에 있어도 호출하면
//  SIGSEGV 로 죽는다 (Lab ③ 가 확인). 남은 것은 CoreGraphics 인데,
//  PDF 를 읽고 쪽을 그리는 데는 그것으로 충분하다 —
//  CGPDFDocument · CGPDFPage · CGContextDrawPDFPage 전부 쓸 수 있다.
//
//  WebKit 으로 pdf.js 를 돌리던 길은 버린다. 그쪽은 길게 흘러가는 스크롤이라
//  '쪽'이 없어 가로 맞춤·세로 맞춤·쪽 넘기기가 성립하지 않았다.
//

import UIKit
import SwiftUI
import CoreGraphics

// MARK: - 맞춤 방식

enum PDFFitMode: Int, CaseIterable {
    /// 가로 꽉 참. 쪽이 화면보다 길어지면 위아래로 움직인다. 글 문서에 맞다.
    case fitWidth
    /// 세로 꽉 참. 쪽 하나가 통째로 보인다.
    case fitHeight
    /// 두 쪽을 나란히. 만화·그림책의 펼침면이다.
    ///
    /// 세로 쪽 둘을 옆에 놓으면 가로세로비가 1.4 안팎이 되어 16:9 화면을
    /// 거의 채운다. 세로 꽉 참으로 한 쪽만 보면 화면의 3분의 2가 빈다.
    case twoPage

    var label: String {
        switch self {
        case .fitWidth:  return "가로 꽉 참"
        case .fitHeight: return "세로 꽉 참"
        case .twoPage:   return "두 쪽"
        }
    }

    var symbol: String {
        switch self {
        case .fitWidth:  return "arrow.left.and.right"
        case .fitHeight: return "arrow.up.and.down"
        case .twoPage:   return "book"
        }
    }

    var next: PDFFitMode {
        switch self {
        case .fitWidth:  return .fitHeight
        case .fitHeight: return .twoPage
        case .twoPage:   return .fitWidth
        }
    }
}

// MARK: - 리모컨을 받는 캔버스

/// 문서 위에 올려 두는 유일한 포커스 대상.
///
/// 뷰컨트롤러에서 pressesBegan 을 받으면 방향키가 전부 포커스 이동에
/// 먹힌다 — 문서 뷰어에서 한 번 크게 당한 지점이다.
/// 입력은 포커스를 가진 뷰에 먼저 가므로, 포커스를 가진 쪽이 받아야 한다.
///
/// **UIButton 이 아니라 UIView 다.** 전에는 UIButton 이었는데, tvOS 는
/// 포커스를 받은 버튼에 흰색 포커스 판을 깔아 준다. 이 뷰는 화면 전체를
/// 덮으므로 그 판이 **문서를 통째로 가렸다** — PDF 가 흰 화면으로 나오던
/// 원인이다. 쪽은 멀쩡히 그려져 있었고 그 위가 흰색이었다.
/// 포커스가 툴바로 넘어가면 판이 사라져 문서가 드러났는데, 그게
/// "선택 버튼을 누르면 흰 화면이 없어진다"의 정체다.
///
/// 평범한 UIView 는 포커스를 받아도 아무것도 그리지 않는다. 그래서 이쪽이다.
@available(tvOS 13.0, *)
final class ArgoFocusCanvas: UIView {
    /// 방향키·재생 등. true 를 돌려주면 그 입력은 여기서 끝난다.
    var onPress: ((UIPress.PressType) -> Bool)?
    /// 가운데 선택 버튼.
    var onSelect: (() -> Void)?

    /// 포커스를 받아야 입력이 온다. UIView 는 기본이 false 다.
    override var canBecomeFocused: Bool { true }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
            ArgoDiag.log("canvas", "pressesBegan type=\(press.type.rawValue) focused=\(isFocused)")
            if press.type == .select {
                onSelect?()
                return
            }
            if let handled = onPress?(press.type), handled { return }
        }
        super.pressesBegan(presses, with: event)
    }
}

// MARK: - 본체

@available(tvOS 13.0, *)
final class ArgoPDFReaderController: UIViewController {

    var documentPath: String = ""
    var onClose: (() -> Void)?

    private var document: CGPDFDocument?
    private var pageCount = 0
    private var currentPage = 1
    /// 지난번에 고른 맞춤으로 연다. 저장만 하고 읽지 않아서 만화책을 열 때마다
    /// 두 쪽을 다시 눌러야 했다.
    private var fitMode: PDFFitMode = {
        let raw = UserDefaults.settings.object(forKey: "pdfFitMode") as? Int
        return PDFFitMode(rawValue: raw ?? 0) ?? .fitWidth
    }()

    /// 가로 맞춤에서 쪽이 화면보다 길 때의 세로 위치 (0 = 맨 위).
    private var scrollOffset: CGFloat = 0
    private var renderedPageHeight: CGFloat = 0

    private let scrollHost = UIScrollView()
    private let pageImage = UIImageView()
    private var canvas = ArgoFocusCanvas()

    private var toolbarSurface = ArgoSurfaceView.makeSurface()
    private var toolbar = UIStackView()
    private var statusLabel = UILabel()
    private var fitButton: ArgoBarButton!
    private var toolbarVisible = true
    /// 도구 모음이 입력을 받는 상태인가.
    ///
    /// 보이는 것과 포커스를 가지는 것을 나눈다. 열자마자 버튼이 포커스를
    /// 가져가면 화살표가 전부 버튼 사이를 오가고 쪽은 꼼짝도 하지 않는다 —
    /// 쪽 넘김이 안 되던 이유다.
    private var toolbarInteractive = false
    private var hideTimer: Timer?

    private var loadingView: UIView?
    private var pageProblemLabel: UILabel?

    // MARK: 화면

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = ArgoTheme.background()

        scrollHost.backgroundColor = .clear
        scrollHost.translatesAutoresizingMaskIntoConstraints = false
        scrollHost.isScrollEnabled = false     // 움직임은 우리가 직접 준다
        view.addSubview(scrollHost)

        pageImage.contentMode = .scaleAspectFit
        pageImage.backgroundColor = .white     // 종이
        // 이 뷰는 layoutPage() 가 프레임으로 직접 놓는다. false 로 두면
        // 오토레이아웃이 제약 없는 뷰를 0 크기로 되돌려 쪽이 사라진다.
        pageImage.translatesAutoresizingMaskIntoConstraints = true
        scrollHost.addSubview(pageImage)

        canvas.translatesAutoresizingMaskIntoConstraints = false
        canvas.backgroundColor = .clear
        canvas.onSelect = { [weak self] in self?.toggleToolbar() }
        canvas.onPress = { [weak self] type in
            self?.handlePress(type) ?? false
        }
        view.addSubview(canvas)

        buildToolbar()

        NSLayoutConstraint.activate([
            scrollHost.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            scrollHost.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            scrollHost.topAnchor.constraint(equalTo: view.topAnchor),
            scrollHost.bottomAnchor.constraint(equalTo: view.bottomAnchor),

            canvas.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            canvas.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            canvas.topAnchor.constraint(equalTo: view.topAnchor),
            canvas.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        let g = view.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            toolbarSurface.centerXAnchor.constraint(equalTo: g.centerXAnchor),
            toolbarSurface.bottomAnchor.constraint(equalTo: g.bottomAnchor,
                                                   constant: -ArgoTheme.gutter()),
            toolbar.leadingAnchor.constraint(equalTo: toolbarSurface.leadingAnchor,
                                             constant: ArgoTheme.gutter()),
            toolbar.trailingAnchor.constraint(equalTo: toolbarSurface.trailingAnchor,
                                              constant: -ArgoTheme.gutter()),
            toolbar.topAnchor.constraint(equalTo: toolbarSurface.topAnchor,
                                         constant: ArgoTheme.gutter() * 0.5),
            toolbar.bottomAnchor.constraint(equalTo: toolbarSurface.bottomAnchor,
                                            constant: -ArgoTheme.gutter() * 0.5),
        ])
        view.bringSubviewToFront(toolbarSurface)

        let menu = UITapGestureRecognizer(target: self, action: #selector(menuPressed))
        menu.allowedPressTypes = [NSNumber(value: UIPress.PressType.menu.rawValue)]
        view.addGestureRecognizer(menu)

        // 리모컨을 제스처로도 받는다.
        //
        // pressesBegan 은 **포커스를 가진 뷰**에만 온다. 캔버스가 포커스를
        // 못 받으면 입력이 아예 도착하지 않는데, 겉으로는 '페이지가 안 넘어간다'
        // 로만 보인다. 제스처는 포커스와 무관하게 온다 — menu 키가 이 길로
        // 혼자 살아 있었던 것이 증거다.
        pressGestures = [
            makePressGesture(.upArrow,    #selector(gUp)),
            makePressGesture(.downArrow,  #selector(gDown)),
            makePressGesture(.leftArrow,  #selector(gLeft)),
            makePressGesture(.rightArrow, #selector(gRight)),
            makePressGesture(.playPause,  #selector(gPlayPause)),
        ]
        // 선택은 따로 둔다 — 도구 모음이 살아 있을 때도 꺼지면 안 된다.
        _ = makePressGesture(.select, #selector(gSelect))

        ArgoDiag.begin("PDF 뷰어: \((documentPath as NSString).lastPathComponent)")
        ArgoDiag.log("load", "bounds=\(view.bounds) safeArea=\(view.safeAreaInsets)")

        openDocument()
    }

    // MARK: 제스처로 받는 리모컨

    private var pressGestures: [UIGestureRecognizer] = []

    private func makePressGesture(_ type: UIPress.PressType, _ action: Selector) -> UITapGestureRecognizer {
        let g = UITapGestureRecognizer(target: self, action: action)
        g.allowedPressTypes = [NSNumber(value: type.rawValue)]
        view.addGestureRecognizer(g)
        return g
    }

    @objc private func gSelect() {
        ArgoDiag.log("gesture", "select interactive=\(toolbarInteractive)")
        guard !toolbarInteractive else { return }
        toggleToolbar()
    }
    @objc private func gUp()        { ArgoDiag.log("gesture", "up");    _ = handlePress(.upArrow) }
    @objc private func gDown()      { ArgoDiag.log("gesture", "down");  _ = handlePress(.downArrow) }
    @objc private func gLeft()      { ArgoDiag.log("gesture", "left");  _ = handlePress(.leftArrow) }
    @objc private func gRight()     { ArgoDiag.log("gesture", "right"); _ = handlePress(.rightArrow) }
    @objc private func gPlayPause() { ArgoDiag.log("gesture", "playPause"); _ = handlePress(.playPause) }

    override func didUpdateFocus(in context: UIFocusUpdateContext,
                                 with coordinator: UIFocusAnimationCoordinator) {
        super.didUpdateFocus(in: context, with: coordinator)
        ArgoDiag.log("focus", "\(String(describing: context.previouslyFocusedItem.map { type(of: $0) })) → "
                            + "\(String(describing: context.nextFocusedItem.map { type(of: $0) }))")
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] {
        if pageProblemLabel?.isHidden == false { return [canvas] }
        return toolbarInteractive ? [toolbar] : [canvas]
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        ArgoDiag.log("layout", "bounds=\(view.bounds)")
        renderCurrentPage()
    }

    // MARK: 문서 열기

    private func openDocument() {
        guard let provider = CGDataProvider(url: URL(fileURLWithPath: documentPath) as CFURL),
              let doc = CGPDFDocument(provider) else {
            showFailure("이 PDF 를 열 수 없습니다. 파일이 손상되었거나 암호가 걸려 있습니다.")
            return
        }
        if doc.isEncrypted && !doc.unlockWithPassword("") {
            showFailure("암호가 걸린 PDF 입니다.")
            return
        }
        document = doc
        pageCount = doc.numberOfPages
        guard pageCount > 0 else {
            showFailure("쪽이 없는 PDF 입니다.")
            return
        }
        currentPage = 1
        renderCurrentPage()
        setToolbar(visible: true, interactive: false, animated: false)
    }

    // MARK: 쪽 그리기

    /// 지금 보여 줄 쪽들. 두 쪽 모드면 펼침면 한 쌍이다.
    ///
    /// 표지는 혼자 선다 — (1), (2,3), (4,5) … 실제 책이 그렇게 펼쳐지고,
    /// 스캔한 만화·그림책은 1쪽이 표지인 경우가 대부분이다.
    private func pagesForCurrentView() -> [Int] {
        guard fitMode == .twoPage else { return [currentPage] }
        if currentPage <= 1 { return [1] }
        let left = currentPage % 2 == 0 ? currentPage : currentPage - 1
        let right = left + 1
        return right <= pageCount ? [left, right] : [left]
    }

    /// 쪽(또는 펼침면)을 화면 크기에 맞춰 그린다.
    ///
    /// UIGraphicsImageRenderer 로 간다. 뒤집기는 내가 한다 — UIKit 컨텍스트는
    /// 위에서 아래로, PDF 는 아래에서 위로 간다.
    ///
    /// /Rotate 가 걸린 쪽이 있으므로 변환은 getDrawingTransform 에서 받아온다 —
    /// 직접 계산하면 90도 돌아간 스캔 문서가 눕는다.
    private func renderCurrentPage() {
        guard let doc = document, view.bounds.width > 0 else { return }

        let numbers = pagesForCurrentView()
        var pages: [(page: CGPDFPage, box: CGRect, boxKind: CGPDFBox, rotation: Int)] = []
        for n in numbers {
            guard let page = doc.page(at: n) else { continue }
            // cropBox 가 비어 있는 PDF 가 있다. 그때는 mediaBox 로 물러선다.
            var kind = CGPDFBox.cropBox
            var box = page.getBoxRect(.cropBox)
            if box.width <= 0 || box.height <= 0 {
                kind = .mediaBox
                box = page.getBoxRect(.mediaBox)
            }
            guard box.width > 0, box.height > 0 else { continue }

            let rotation = Int(page.rotationAngle)
            let turned = (rotation / 90) % 2 != 0
            let shown = turned ? CGRect(x: 0, y: 0, width: box.height, height: box.width) : box
            pages.append((page, shown, kind, rotation))
        }
        guard !pages.isEmpty else {
            showPageProblem("이 쪽의 크기를 읽을 수 없습니다.")
            return
        }

        // 쪽들을 같은 높이로 맞춰 옆으로 늘어놓았을 때의 전체 가로세로비.
        let aspect = pages.reduce(0) { $0 + $1.box.width / $1.box.height }
        let available = view.bounds.size

        // 점(point) 기준의 최종 크기.
        let size: CGSize
        switch fitMode {
        case .fitWidth:
            let w = available.width
            size = CGSize(width: w, height: (w / aspect).rounded())
        case .fitHeight:
            let h = available.height
            size = CGSize(width: (h * aspect).rounded(), height: h)
        case .twoPage:
            // 펼침면은 가로·세로 둘 다 넘지 않게 맞춘다.
            let h = min(available.height, available.width / aspect)
            size = CGSize(width: (h * aspect).rounded(), height: h.rounded())
        }
        guard size.width >= 1, size.height >= 1 else { return }
        ArgoDiag.log("render", "pages=\(numbers) fit=\(fitMode.rawValue) available=\(available) "
                             + "aspect=\(String(format: "%.3f", aspect)) size=\(size) "
                             + "renderScale=\(Self.renderScale(for: size))")

        let format = UIGraphicsImageRendererFormat.default()
        format.scale = Self.renderScale(for: size)
        format.opaque = true

        let image = UIGraphicsImageRenderer(size: size, format: format).image { rc in
            let ctx = rc.cgContext

            // 종이를 먼저 깐다. PDF 는 배경을 그리지 않아 안 깔면 비어 보인다.
            UIColor.white.setFill()
            ctx.fill(CGRect(origin: .zero, size: size))

            var x: CGFloat = 0
            for item in pages {
                let w = (item.box.width / item.box.height) * size.height
                let slot = CGRect(x: x, y: 0, width: w, height: size.height)
                ctx.saveGState()
                ctx.clip(to: slot)
                // 이 쪽의 자리로 원점을 옮긴 뒤, 그 안에서 뒤집는다.
                ctx.translateBy(x: slot.minX, y: slot.maxY)
                ctx.scaleBy(x: 1, y: -1)
                ctx.concatenate(item.page.getDrawingTransform(
                    item.boxKind,
                    rect: CGRect(origin: .zero, size: slot.size),
                    rotate: 0,
                    preserveAspectRatio: true))
                ctx.drawPDFPage(item.page)
                ctx.restoreGState()
                x += w
            }
        }

        pageImage.image = image
        renderedPageHeight = size.height
        layoutPage()
        updateStatus()

        if Self.isBlank(image) {
            let first = pages[0]
            let detail = String(format: "쪽 %.0f×%.0f · 회전 %d · %@ · 그린 크기 %.0f×%.0f @%.0fx",
                                first.box.width, first.box.height, first.rotation,
                                first.boxKind == .cropBox ? "cropBox" : "mediaBox",
                                size.width, size.height, format.scale)
            showPageProblem("이 쪽에서 그려진 것이 없습니다.\n"
                            + "원래 빈 쪽이거나, 이 기기가 그리지 못하는 내용입니다.\n\n"
                            + detail)
        } else {
            hidePageProblem()
        }
    }

    /// 몇 배로 그릴지. 4K 화면에서는 두 배로 그려야 글자가 또렷하다.
    ///
    /// 등배로만 그리면 4K 애플TV 에서도 1080p 품질로 보인다. 다만 가로 꽉 참의
    /// 긴 쪽은 두 배로 올리면 한 장이 수천만 화소가 되므로 화소 예산을 둔다 —
    /// 예산을 넘으면 그만큼만 올린다.
    private static func renderScale(for size: CGSize) -> CGFloat {
        let wanted = min(UIScreen.main.nativeScale, 2.0)
        guard wanted > 1 else { return 1 }

        let budget: CGFloat = 24_000_000      // 화소. 32비트면 약 96MB.
        let atWanted = size.width * size.height * wanted * wanted
        guard atWanted > budget else { return wanted }

        let fitted = (budget / (size.width * size.height)).squareRoot()
        return max(1, min(wanted, fitted))
    }

    /// 작게 줄여 어두운 점이 있는지 센다. 사람 눈으로는 못 하는 판정이다.
    private static func isBlank(_ image: UIImage) -> Bool {
        guard let cg = image.cgImage else { return true }

        let w = 60, h = 60
        var pixels = [UInt8](repeating: 255, count: w * h)
        guard let ctx = CGContext(data: &pixels, width: w, height: h,
                                  bitsPerComponent: 8, bytesPerRow: w,
                                  space: CGColorSpaceCreateDeviceGray(),
                                  bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return false }
        ctx.setFillColor(gray: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))

        let inked = pixels.reduce(0) { $1 < 250 ? $0 + 1 : $0 }
        return CGFloat(inked) / CGFloat(w * h) < 0.001
    }

    /// 쪽이 비었을 때 겹쳐 보여 주는 쪽지. 쪽을 넘기면 사라진다.
    private func showPageProblem(_ message: String) {
        if pageProblemLabel == nil {
            let label = UILabel()
            label.numberOfLines = 0
            label.textAlignment = .center
            label.font = ArgoTheme.fontBody()
            label.textColor = ArgoTheme.textPrimary()
            label.backgroundColor = ArgoTheme.surface()
            label.layer.cornerRadius = ArgoTheme.cornerRadius()
            label.layer.masksToBounds = true
            label.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(label)
            NSLayoutConstraint.activate([
                label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
                label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
                label.widthAnchor.constraint(equalToConstant: 900),
                label.heightAnchor.constraint(greaterThanOrEqualToConstant: 220),
            ])
            pageProblemLabel = label
        }
        pageProblemLabel?.text = message
        pageProblemLabel?.isHidden = false
        if let label = pageProblemLabel { view.bringSubviewToFront(label) }
        view.bringSubviewToFront(toolbarSurface)
    }

    private func hidePageProblem() {
        pageProblemLabel?.isHidden = true
    }

    /// 그린 쪽을 화면에 놓는다. 가로 맞춤에서 쪽이 길면 scrollOffset 만큼 올린다.
    private func layoutPage() {
        let available = view.bounds.size
        let w: CGFloat
        let h: CGFloat
        // 그린 그림의 가로세로비를 그대로 쓴다. 두 쪽이면 펼침면 전체의 비율이다.
        let imageAspect = (pageImage.image?.size.width ?? 1)
                        / max(1, pageImage.image?.size.height ?? 1)
        switch fitMode {
        case .fitWidth:
            w = available.width
            h = renderedPageHeight
        case .fitHeight, .twoPage:
            h = min(available.height, renderedPageHeight)
            w = imageAspect * h
        }

        let maxOffset = max(0, h - available.height)
        scrollOffset = min(max(0, scrollOffset), maxOffset)

        pageImage.frame = CGRect(x: (available.width - w) / 2,
                                 y: -scrollOffset,
                                 width: w, height: h)
        ArgoDiag.log("layoutPage", "available=\(available) image=\(pageImage.image?.size ?? .zero) "
                                 + "renderedH=\(renderedPageHeight) offset=\(scrollOffset) "
                                 + "frame=\(pageImage.frame)")
    }

    private var canScrollWithinPage: Bool {
        renderedPageHeight > view.bounds.height + 1 && fitMode == .fitWidth
    }

    // MARK: 리모컨

    private func handlePress(_ type: UIPress.PressType) -> Bool {
        // 네 방향 모두 여기서 받는다. 도구 모음은 선택 버튼으로만 간다 —
        // 도구 모음이 화면 아래에 있어서, 위쪽을 넘겨 주면 갈 곳이 없어
        // 아무 일도 일어나지 않는다.
        switch type {
        case .leftArrow:
            goToPage(previousViewStart())
            return true
        case .rightArrow:
            goToPage(nextViewStart())
            return true
        case .upArrow:
            // 쪽이 길면 쪽 안에서 올라가고, 맨 위면 앞 쪽으로 넘어간다.
            if canScrollWithinPage && scrollOffset > 0 {
                scrollBy(-view.bounds.height * 0.85)
            } else {
                goToPage(previousViewStart(), landAtBottom: true)
            }
            return true
        case .downArrow:
            let maxOffset = max(0, renderedPageHeight - view.bounds.height)
            if canScrollWithinPage && scrollOffset < maxOffset - 1 {
                scrollBy(view.bounds.height * 0.85)
            } else {
                goToPage(nextViewStart())
            }
            return true
        case .playPause:
            cycleFit()
            return true
        default:
            return false
        }
    }

    private func scrollBy(_ delta: CGFloat) {
        scrollOffset += delta
        layoutPage()
        updateStatus()
    }

    private func goToPage(_ page: Int, landAtBottom: Bool = false) {
        guard page >= 1, page <= pageCount else { return }
        currentPage = page
        scrollOffset = 0
        renderCurrentPage()
        if landAtBottom {
            // 앞 쪽으로 올라갈 때는 그 쪽의 아래쪽부터 보여 준다 —
            // 읽던 흐름이 끊기지 않게.
            scrollOffset = max(0, renderedPageHeight - view.bounds.height)
            layoutPage()
            updateStatus()
        }
    }

    @objc private func cycleFit() {
        fitMode = fitMode.next
        scrollOffset = 0
        // 두 쪽으로 들어가면 지금 쪽이 든 펼침면의 왼쪽으로 맞춘다.
        if fitMode == .twoPage, currentPage > 1, currentPage % 2 == 1 {
            currentPage -= 1
        }
        UserDefaults.settings.set(fitMode.rawValue, forKey: "pdfFitMode")
        renderCurrentPage()
        setToolbar(visible: true, interactive: false, animated: true)
    }

    // MARK: 도구 모음

    private func buildToolbar() {
        statusLabel.font = ArgoTheme.fontMonoBody()
        statusLabel.textColor = ArgoTheme.textSecondary()
        statusLabel.text = "여는 중…"

        let prev = ArgoBarButton.make(title: "◀ 이전")
        prev.addTarget(self, action: #selector(prevPage), for: .primaryActionTriggered)
        let next = ArgoBarButton.make(title: "다음 ▶")
        next.addTarget(self, action: #selector(nextPage), for: .primaryActionTriggered)

        fitButton = ArgoBarButton.make(title: fitMode.label)
        fitButton.addTarget(self, action: #selector(cycleFit), for: .primaryActionTriggered)

        let first = ArgoBarButton.make(title: "처음")
        first.addTarget(self, action: #selector(goFirst), for: .primaryActionTriggered)
        let last = ArgoBarButton.make(title: "끝")
        last.addTarget(self, action: #selector(goLast), for: .primaryActionTriggered)

        let close = ArgoBarButton.make(title: "닫기")
        close.addTarget(self, action: #selector(closeNow), for: .primaryActionTriggered)

        toolbar = UIStackView(arrangedSubviews: [prev, next, statusLabel,
                                                 fitButton, first, last, close])
        toolbar.axis = .horizontal
        toolbar.spacing = ArgoTheme.gutter()
        toolbar.alignment = .center
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
    }

    @objc private func prevPage() { goToPage(previousViewStart()) }
    @objc private func nextPage() { goToPage(nextViewStart()) }

    /// 다음으로 보여 줄 묶음의 첫 쪽. 두 쪽 모드면 한 번에 두 장씩 간다.
    private func nextViewStart() -> Int {
        guard fitMode == .twoPage else { return currentPage + 1 }
        let shown = pagesForCurrentView()
        return (shown.last ?? currentPage) + 1
    }

    private func previousViewStart() -> Int {
        guard fitMode == .twoPage else { return currentPage - 1 }
        let first = pagesForCurrentView().first ?? currentPage
        if first <= 1 { return 0 }          // 범위 밖 — goToPage 가 막는다
        if first == 2 { return 1 }          // 표지로
        return first - 2
    }
    @objc private func goFirst()  { goToPage(1) }
    @objc private func goLast()   { goToPage(pageCount) }

    private func updateStatus() {
        let shown = pagesForCurrentView()
        var text: String
        if shown.count > 1, let a = shown.first, let b = shown.last {
            text = "\(a)–\(b) / \(pageCount)"
        } else {
            text = "\(shown.first ?? currentPage) / \(pageCount)"
        }
        if canScrollWithinPage {
            let maxOffset = max(1, renderedPageHeight - view.bounds.height)
            let percent = Int((scrollOffset / maxOffset * 100).rounded())
            text += "  ·  \(min(100, max(0, percent)))%"
        }
        statusLabel.text = text
        fitButton.setTitle(fitMode.label, for: .normal)
    }

    /// 선택 버튼. 도구 모음을 띄우고 포커스를 그쪽으로 넘긴다.
    @objc private func toggleToolbar() {
        setToolbar(visible: true, interactive: !toolbarInteractive, animated: true)
    }

    private func setToolbar(visible: Bool, interactive: Bool, animated: Bool) {
        toolbarVisible = visible
        toolbarInteractive = visible && interactive
        hideTimer?.invalidate()

        // 꺼진 버튼은 포커스를 못 받는다. 이것이 캔버스가 화살표를 받는
        // 유일한 방법이다.
        for sub in toolbar.arrangedSubviews {
            (sub as? UIControl)?.isEnabled = toolbarInteractive
        }
        // 방향키 제스처가 도구 모음의 포커스 이동을 가로채지 않게 한다.
        for g in pressGestures { g.isEnabled = !toolbarInteractive }
        ArgoDiag.log("toolbar", "visible=\(visible) interactive=\(toolbarInteractive)")
        setNeedsFocusUpdate()
        updateFocusIfNeeded()

        let apply = {
            // 보이기만 할 때는 옅게 — 지금 눌리지 않는다는 뜻이다.
            let a: CGFloat = visible ? (self.toolbarInteractive ? 1.0 : 0.55) : 0
            self.toolbar.alpha = a
            self.toolbarSurface.alpha = a
        }
        if animated { UIView.animate(withDuration: 0.2, animations: apply) } else { apply() }

        if visible {
            let wait: TimeInterval = toolbarInteractive ? 12 : 5
            hideTimer = Timer.scheduledTimer(withTimeInterval: wait, repeats: false) { [weak self] _ in
                self?.setToolbar(visible: false, interactive: false, animated: true)
            }
        }
    }

    @objc private func menuPressed() {
        // 한 겹씩 벗긴다. 도구 모음 → 닫기.
        if toolbarInteractive {
            setToolbar(visible: false, interactive: false, animated: true)
        } else {
            closeNow()
        }
    }

    @objc private func closeNow() {
        hideTimer?.invalidate()
        onClose?()
    }

    // MARK: 실패

    private func showFailure(_ message: String) {
        let box = ArgoSurfaceView.makeSurface()

        let head = UILabel()
        head.text = "PDF 를 열 수 없습니다"
        head.font = ArgoTheme.fontTitle()
        head.textColor = ArgoTheme.textPrimary()
        head.textAlignment = .center
        head.translatesAutoresizingMaskIntoConstraints = false

        let why = UILabel()
        why.text = message
        why.numberOfLines = 0
        why.font = ArgoTheme.fontCaption()
        why.textColor = ArgoTheme.textSecondary()
        why.textAlignment = .center
        why.translatesAutoresizingMaskIntoConstraints = false

        let back = ArgoBarButton.make(title: "파일 목록으로")
        back.isPrimary = true
        back.addTarget(self, action: #selector(closeNow), for: .primaryActionTriggered)

        for v in [head, why, back] { box.addSubview(v) }
        let pad = ArgoTheme.screenMargin() * 0.7
        NSLayoutConstraint.activate([
            head.topAnchor.constraint(equalTo: box.topAnchor, constant: pad),
            head.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: pad),
            head.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -pad),

            why.topAnchor.constraint(equalTo: head.bottomAnchor, constant: 20),
            why.leadingAnchor.constraint(equalTo: box.leadingAnchor, constant: pad),
            why.trailingAnchor.constraint(equalTo: box.trailingAnchor, constant: -pad),

            back.topAnchor.constraint(equalTo: why.bottomAnchor, constant: 32),
            back.centerXAnchor.constraint(equalTo: box.centerXAnchor),
            back.bottomAnchor.constraint(equalTo: box.bottomAnchor, constant: -pad),

            box.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            box.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            box.widthAnchor.constraint(equalToConstant: 900),
        ])
        loadingView = box
        view.addSubview(box)
        view.bringSubviewToFront(box)
        setNeedsFocusUpdate()
        updateFocusIfNeeded()
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct ArgoPDFReaderView: UIViewControllerRepresentable {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    func makeUIViewController(context: Context) -> ArgoPDFReaderController {
        let vc = ArgoPDFReaderController()
        vc.documentPath = filePath.hasSuffix("/") ? filePath + fileName : filePath
        vc.onClose = { isPresented = false }
        return vc
    }

    func updateUIViewController(_ vc: ArgoPDFReaderController, context: Context) {}
}
