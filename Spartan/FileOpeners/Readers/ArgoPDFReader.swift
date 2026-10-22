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
    /// 가로 꽉 참. 쪽이 화면보다 길어지면 위아래로 움직인다.
    case fitWidth
    /// 세로 꽉 참. 쪽 하나가 통째로 보인다.
    case fitHeight

    var label: String {
        switch self {
        case .fitWidth:  return "가로 꽉 참"
        case .fitHeight: return "세로 꽉 참"
        }
    }

    var symbol: String {
        switch self {
        case .fitWidth:  return "arrow.left.and.right"
        case .fitHeight: return "arrow.up.and.down"
        }
    }

    var next: PDFFitMode {
        self == .fitWidth ? .fitHeight : .fitWidth
    }
}

// MARK: - 리모컨을 받는 캔버스

/// 쪽 위에 올려 두는 유일한 포커스 대상.
///
/// 뷰컨트롤러에서 pressesBegan 을 받으면 방향키가 전부 포커스 이동에
/// 먹힌다 — 문서 뷰어에서 한 번 크게 당한 지점이다.
/// 입력은 포커스를 가진 뷰에 먼저 가므로, 포커스를 가진 쪽이 받아야 한다.
@available(tvOS 13.0, *)
final class PDFCanvasButton: UIButton {
    var onPress: ((UIPress.PressType) -> Bool)?

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
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
    private var fitMode: PDFFitMode = .fitWidth

    /// 가로 맞춤에서 쪽이 화면보다 길 때의 세로 위치 (0 = 맨 위).
    private var scrollOffset: CGFloat = 0
    private var renderedPageHeight: CGFloat = 0

    private let scrollHost = UIScrollView()
    private let pageImage = UIImageView()
    private var canvas = PDFCanvasButton(type: .custom)

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
        canvas.addTarget(self, action: #selector(toggleToolbar),
                         for: .primaryActionTriggered)
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

        openDocument()
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] {
        if pageProblemLabel?.isHidden == false { return [canvas] }
        return toolbarInteractive ? [toolbar] : [canvas]
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
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

    /// 쪽 하나를 화면 크기에 맞춰 그린다.
    ///
    /// UIGraphicsImageRenderer 로 간다. 직접 만든 CGBitmapContext 에
    /// drawPDFPage 를 했더니 흰 종이만 나왔다 — 그림이 어디로 갔는지
    /// 알 길이 없었다. 이쪽은 UIKit 이 컨텍스트를 세워 주므로
    /// 좌표계와 색 공간을 내가 맞출 필요가 없다.
    ///
    /// 대신 뒤집기는 내가 한다. UIKit 컨텍스트는 위에서 아래로,
    /// PDF 는 아래에서 위로 간다.
    ///
    /// /Rotate 가 걸린 쪽이 있으므로 변환은 getDrawingTransform 에서 받아온다 —
    /// 직접 계산하면 90도 돌아간 스캔 문서가 눕는다.
    private func renderCurrentPage() {
        guard let doc = document,
              let page = doc.page(at: currentPage),
              view.bounds.width > 0 else { return }

        let box = page.getBoxRect(.cropBox)
        guard box.width > 0, box.height > 0 else {
            showPageProblem("이 쪽의 크기를 읽을 수 없습니다.")
            return
        }

        let rotation = Int(page.rotationAngle)
        let turned = (rotation / 90) % 2 != 0
        let pageW = turned ? box.height : box.width
        let pageH = turned ? box.width : box.height

        let available = view.bounds.size
        let scale: CGFloat
        switch fitMode {
        case .fitWidth:  scale = available.width / pageW
        case .fitHeight: scale = available.height / pageH
        }

        let size = CGSize(width: max(1, (pageW * scale).rounded()),
                          height: max(1, (pageH * scale).rounded()))

        let format = UIGraphicsImageRendererFormat.default()
        // 화면 밀도까지 곱하면 큰 쪽 하나가 수백 MB 가 된다. 등배로 충분하다.
        format.scale = 1
        format.opaque = true

        let image = UIGraphicsImageRenderer(size: size, format: format).image { rc in
            let ctx = rc.cgContext

            // 종이를 먼저 깐다. PDF 는 배경을 그리지 않아 안 깔면 비어 보인다.
            UIColor.white.setFill()
            ctx.fill(CGRect(origin: .zero, size: size))

            ctx.translateBy(x: 0, y: size.height)
            ctx.scaleBy(x: 1, y: -1)
            ctx.concatenate(page.getDrawingTransform(.cropBox,
                                                     rect: CGRect(origin: .zero, size: size),
                                                     rotate: 0,
                                                     preserveAspectRatio: true))
            ctx.drawPDFPage(page)
        }

        pageImage.image = image
        renderedPageHeight = size.height
        layoutPage()
        updateStatus()

        // 흰 화면을 그냥 보여 주지 않는다. 그리지 못한 것과 원래 빈 쪽은
        // 눈으로 구별할 수 없고, 아무 말이 없으면 고장으로 보인다.
        if Self.isBlank(image) {
            showPageProblem("이 쪽에서 그려진 것이 없습니다.\n"
                            + "원래 빈 쪽이거나, 이 기기가 그리지 못하는 내용입니다.")
        } else {
            hidePageProblem()
        }
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
        switch fitMode {
        case .fitWidth:
            w = available.width
            h = renderedPageHeight
        case .fitHeight:
            h = available.height
            w = (pageImage.image?.size.width ?? 1) / (pageImage.image?.size.height ?? 1) * h
        }

        let maxOffset = max(0, h - available.height)
        scrollOffset = min(max(0, scrollOffset), maxOffset)

        pageImage.frame = CGRect(x: (available.width - w) / 2,
                                 y: -scrollOffset,
                                 width: w, height: h)
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
            goToPage(currentPage - 1)
            return true
        case .rightArrow:
            goToPage(currentPage + 1)
            return true
        case .upArrow:
            // 쪽이 길면 쪽 안에서 올라가고, 맨 위면 앞 쪽으로 넘어간다.
            if canScrollWithinPage && scrollOffset > 0 {
                scrollBy(-view.bounds.height * 0.85)
            } else {
                goToPage(currentPage - 1, landAtBottom: true)
            }
            return true
        case .downArrow:
            let maxOffset = max(0, renderedPageHeight - view.bounds.height)
            if canScrollWithinPage && scrollOffset < maxOffset - 1 {
                scrollBy(view.bounds.height * 0.85)
            } else {
                goToPage(currentPage + 1)
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

    @objc private func prevPage() { goToPage(currentPage - 1) }
    @objc private func nextPage() { goToPage(currentPage + 1) }
    @objc private func goFirst()  { goToPage(1) }
    @objc private func goLast()   { goToPage(pageCount) }

    private func updateStatus() {
        var text = "\(currentPage) / \(pageCount)"
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
