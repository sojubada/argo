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
    private var hideTimer: Timer?

    private var loadingView: UIView?

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
        pageImage.translatesAutoresizingMaskIntoConstraints = false
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

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [canvas] }

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
        setToolbarVisible(true, animated: false)
    }

    // MARK: 쪽 그리기

    /// 쪽 하나를 화면 크기에 맞춰 비트맵으로 그린다.
    ///
    /// /Rotate 가 걸린 쪽이 있으므로 getDrawingTransform 으로 받아온다 —
    /// 직접 계산하면 90도 돌아간 스캔 문서가 눕는다.
    private func renderCurrentPage() {
        guard let doc = document,
              let page = doc.page(at: currentPage),
              view.bounds.width > 0 else { return }

        let box = page.getBoxRect(.cropBox)
        guard box.width > 0, box.height > 0 else { return }

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

        // 화면 밀도까지 곱하면 4K 에서 한 장이 수백 MB 가 된다. 2배까지만.
        let pixelScale = min(UIScreen.main.scale, 2.0)
        let targetW = max(1, Int((pageW * scale * pixelScale).rounded()))
        let targetH = max(1, Int((pageH * scale * pixelScale).rounded()))

        guard let ctx = CGContext(data: nil,
                                  width: targetW, height: targetH,
                                  bitsPerComponent: 8,
                                  bytesPerRow: 0,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return }

        // 종이를 먼저 깐다. PDF 는 배경을 그리지 않아 안 깔면 검게 나온다.
        ctx.setFillColor(gray: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: targetW, height: targetH))

        let target = CGRect(x: 0, y: 0, width: targetW, height: targetH)
        ctx.concatenate(page.getDrawingTransform(.cropBox, rect: target,
                                                 rotate: 0, preserveAspectRatio: true))
        ctx.drawPDFPage(page)

        guard let cg = ctx.makeImage() else { return }
        pageImage.image = UIImage(cgImage: cg)

        renderedPageHeight = pageH * scale
        layoutPage()
        updateStatus()
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
        // 도구 모음이 떠 있으면 위쪽은 거기로 가는 길이다.
        if type == .upArrow && toolbarVisible { return false }

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
        setToolbarVisible(true, animated: true)
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

    @objc private func toggleToolbar() {
        setToolbarVisible(!toolbarVisible, animated: true)
    }

    private func setToolbarVisible(_ visible: Bool, animated: Bool) {
        toolbarVisible = visible
        hideTimer?.invalidate()
        // 꺼진 버튼은 포커스를 못 받는다. 도구 모음이 내려가면 캔버스만 남아
        // 모든 방향키가 쪽 넘기기로 간다.
        for sub in toolbar.arrangedSubviews {
            (sub as? UIControl)?.isEnabled = visible
        }
        if !visible { setNeedsFocusUpdate(); updateFocusIfNeeded() }

        let apply = {
            self.toolbar.alpha = visible ? 1 : 0
            self.toolbarSurface.alpha = visible ? 1 : 0
        }
        if animated { UIView.animate(withDuration: 0.2, animations: apply) } else { apply() }

        if visible {
            hideTimer = Timer.scheduledTimer(withTimeInterval: 8, repeats: false) { [weak self] _ in
                self?.setToolbarVisible(false, animated: true)
            }
        }
    }

    @objc private func menuPressed() {
        // 한 겹씩 벗긴다. 도구 모음 → 닫기.
        if toolbarVisible { setToolbarVisible(false, animated: true) } else { closeNow() }
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
