# 검증 B — PDF·텍스트 뷰어와 포커스 모델

커밋 `6869fc9`.

**이 문서의 ①번이 지금 가장 급한 문제입니다.**

---

## 답해 주셔야 할 질문

### Q1. PDF 가 왜 흰 화면으로 나옵니까?  ★★최우선★★

증상 (실기기, build 130):

- PDF 를 열면 **쪽 수가 읽힙니다** — 상태줄에 `1 / 1` 이 나옴
- **화면이 완전히 흽니다** — 글자도 선도 없음
- 상태줄에 `· 0%` 가 같이 나옴

이 세 가지를 코드와 맞춰 보면 이렇게 좁혀집니다.

1. `CGPDFDocument` 가 문서를 **열었습니다.** 못 열면 오류 화면이 뜨게 되어 있고,
   오류 화면은 어두운 패널입니다 — 흰 화면이 아닙니다
2. `· 0%` 는 `canScrollWithinPage` 가 참일 때만 나옵니다. 그 값은
   `renderedPageHeight > 화면 높이` 인데, `renderedPageHeight` 는
   **그리기가 끝난 뒤에** 대입됩니다
3. 따라서 **그리기 호출까지 갔고, 비트맵도 만들어졌는데, 잉크가 없습니다**

제가 한 조치는 **추측에 기반합니다.** 직접 만든 `CGBitmapContext` 에
`drawPDFPage` 하던 것을 `UIGraphicsImageRenderer` 로 바꿨습니다.
가장 널리 쓰이는 경로라서 바꾼 것이지, 원인을 알아서 바꾼 것이 아닙니다.

**봐 주실 것:**

- 아래 `renderCurrentPage` 에서 **좌표계 뒤집기와 `getDrawingTransform` 의
  조합**이 맞습니까? `UIGraphicsImageRenderer` 의 컨텍스트는 위에서 아래로
  갑니다. 그래서 `translateBy` + `scaleBy(1, -1)` 로 뒤집은 뒤
  `getDrawingTransform` 을 적용했습니다. **이 순서가 맞습니까?**
  혹시 `getDrawingTransform` 이 이미 뒤집기를 포함하고 있어 **두 번
  뒤집히는** 것은 아닙니까?
- 바꾸기 전의 `CGBitmapContext` 방식(뒤집기 없음)은 아래에 **함께 실어
  두었습니다.** 둘 중 어느 쪽이 맞습니까, 아니면 둘 다 틀립니까?
- `drawPDFPage` 가 조용히 아무것도 안 그리는 다른 조건이 있습니까?
  (암호·권한 제한·색 공간·누락된 폰트 등)

### Q2. 포커스 모델이 세 뷰어에서 맞습니까?  ★

툴바를 화면 **아래**로 옮긴 뒤에도 세 뷰어 모두 이 코드를 갖고 있었습니다.

```objc
// 도구 모음이 떠 있으면 위쪽은 거기로 가는 길이다.
if (type == UIPressTypeUpArrow && self.toolbarVisible) { return NO; }
```

툴바가 아래에 있으니 위쪽은 갈 곳이 없어 **아무 일도 일어나지 않았고**,
열자마자 활성화된 툴바가 **첫 포커스를 가져가** 화살표가 버튼 사이만
오갔습니다. 이것이 "리모컨으로 쪽 넘김이 안 된다"의 원인이었습니다.

지금은 **보이는 것과 입력을 받는 것을 나눴습니다.**

| 상태 | 툴바 | 포커스 | 화살표 |
| --- | --- | --- | --- |
| 열었을 때 | 보임 (옅게, 55%) | 캔버스 | 문서를 움직임 |
| 선택 버튼 | 보임 (진하게) | 툴바 | 버튼 사이 이동 |
| 메뉴 / 시간 초과 | 숨김 | 캔버스 | 문서를 움직임 |

**같은 수정을 세 곳에 했습니다. 한 곳이 틀리면 세 곳이 틀립니다.**

- 꺼진 버튼(`isEnabled = NO`)이 정말 포커스를 못 받습니까?
- `preferredFocusEnvironments` 를 바꾼 뒤 `setNeedsFocusUpdate` +
  `updateFocusIfNeeded` 를 부르는 것으로 충분합니까?
- 툴바 전체(`UIStackView`)를 포커스 환경으로 돌려주는 것이 맞습니까,
  아니면 그 안의 버튼 하나를 지목해야 합니까?

### Q3. 텍스트 뷰어가 실제로 스크롤됩니까?

`UITextView` 를 `isUserInteractionEnabled = false` 로 두고
`setContentOffset(_:animated:)` 으로만 움직입니다. 리모컨 입력은 위에 덮은
캔버스 버튼이 받습니다.

- 상호작용이 꺼진 텍스트 뷰에서 `contentSize` 가 제대로 계산됩니까?
- `attributedText` 를 넣은 직후 `contentSize` 를 읽으면 0 입니까?
  (`updateStatus` 가 그 시점에 불립니다)

### Q4. 8MB 상한과 인코딩 판정이 맞습니까?

`FileHandle.readData(ofLength:)` 로 앞 8MB 만 읽습니다. UTF-8 이 아니면
`NSString.stringEncoding(for:...)` 로 찍고, 그래도 안 되면 `isoLatin1` 로
손실을 감수하고 보여 줍니다. CP949 한글 로그가 이 경로로 읽힙니까?

**8MB 경계에서 멀티바이트 글자가 잘리면** 그 줄이 깨집니다. 이것을
다뤄야 합니까?

---

## 1. PDF 뷰어 — 지금 코드

### `Spartan/FileOpeners/Readers/ArgoPDFReader.swift`  (593줄)

```swift
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

        // cropBox 가 비어 있는 PDF 가 있다. 그때는 mediaBox 로 물러선다.
        var usedBox = CGPDFBox.cropBox
        var box = page.getBoxRect(.cropBox)
        if box.width <= 0 || box.height <= 0 {
            usedBox = .mediaBox
            box = page.getBoxRect(.mediaBox)
        }
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
            ctx.concatenate(page.getDrawingTransform(usedBox,
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
            // 왜 비었는지 숫자를 같이 남긴다. '비어 보인다' 만으로는
            // 안 그려진 것인지 원래 빈 쪽인지 구별할 수 없다.
            let detail = String(format: "쪽 %.0f×%.0f · 회전 %d · %@ · 그린 크기 %.0f×%.0f",
                                box.width, box.height, rotation,
                                usedBox == .cropBox ? "cropBox" : "mediaBox",
                                size.width, size.height)
            showPageProblem("이 쪽에서 그려진 것이 없습니다.\n"
                            + "원래 빈 쪽이거나, 이 기기가 그리지 못하는 내용입니다.\n\n"
                            + detail)
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
```

---

## 2. 바꾸기 전의 렌더링 — 비교용

이쪽이 **흰 화면을 냈던** 코드입니다. 둘을 비교해 주십시오.

```swift
// 직접 만든 비트맵 컨텍스트. 원점이 왼쪽 아래라 PDF 와 같으므로
// 뒤집기를 하지 않았다.
let pixelScale = min(UIScreen.main.scale, 2.0)
let targetW = max(1, Int((pageW * scale * pixelScale).rounded()))
let targetH = max(1, Int((pageH * scale * pixelScale).rounded()))

guard let ctx = CGContext(data: nil,
                          width: targetW, height: targetH,
                          bitsPerComponent: 8,
                          bytesPerRow: 0,
                          space: CGColorSpaceCreateDeviceRGB(),
                          bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return }

ctx.setFillColor(gray: 1, alpha: 1)
ctx.fill(CGRect(x: 0, y: 0, width: targetW, height: targetH))

let target = CGRect(x: 0, y: 0, width: targetW, height: targetH)
ctx.concatenate(page.getDrawingTransform(.cropBox, rect: target,
                                         rotate: 0, preserveAspectRatio: true))
ctx.drawPDFPage(page)

guard let cg = ctx.makeImage() else { return }
pageImage.image = UIImage(cgImage: cg)
```

**이 코드에서 제가 못 찾은 결함이 있습니까?**

함께 고친 것이 하나 더 있습니다. `pageImage` 는 프레임으로 직접 놓는데
`translatesAutoresizingMaskIntoConstraints = false` 였고 제약이 하나도
없었습니다. 오토레이아웃이 그런 뷰를 0 크기로 되돌릴 수 있습니다.
지금은 `true` 입니다. **이것이 흰 화면의 원인일 수 있습니까?**
(제 생각에는 아닙니다 — 0 크기면 어두운 바탕이 보여야지 흰색이 아닙니다)

---

## 3. 텍스트·로그·plist 뷰어

### `Spartan/FileOpeners/Readers/ArgoTextReader.swift`  (363줄)

```swift
//
//  ArgoTextReader.swift
//  Argo
//
//  텍스트·로그·plist·json 읽기.
//
//  기존 TextView 는 편집기다. 줄마다 TextField 를 한 행씩 놓아서, 화면보다
//  긴 줄은 그냥 잘렸다 — 로그는 한 줄이 화면 몇 배라 사실상 읽을 수 없었다.
//
//  이것은 읽기 전용이고 UITextView 하나에 전부 흘려 넣는다. 줄이 접히고,
//  글꼴 크기를 바꿀 수 있고, 리모컨으로 한 화면씩 넘어간다.
//  고치려면 편집기를 따로 열면 된다.
//

import UIKit
import SwiftUI

@available(tvOS 13.0, *)
final class ArgoTextReaderController: UIViewController {

    var filePath: String = ""
    var onClose: (() -> Void)?
    /// 편집기로 넘길 때 부른다.
    var onEdit: (() -> Void)?

    /// 한 번에 읽어 들일 상한. 100MB 로그를 통째로 메모리에 올리면 죽는다.
    private static let maxBytes = 8 * 1024 * 1024

    private let textView = UITextView()
    private var canvas = PDFCanvasButton(type: .custom)

    private var toolbarSurface = ArgoSurfaceView.makeSurface()
    private var toolbar = UIStackView()
    private var statusLabel = UILabel()
    private var toolbarVisible = true
    /// 보이는 것과 입력을 받는 것은 다르다. 열자마자 버튼이 포커스를
    /// 가져가면 화살표가 버튼 사이만 오가고 글은 꼼짝도 하지 않는다.
    private var toolbarInteractive = false
    private var hideTimer: Timer?

    private var fontSize: CGFloat = 28
    private var truncated = false
    private var lineCount = 0

    private var ext: String { (filePath as NSString).pathExtension.lowercased() }

    /// 로그·코드·설정 파일은 고정폭이 읽기 쉽다. 줄이 세로로 정렬된다.
    private var wantsMonospace: Bool {
        ["log", "json", "plist", "xml", "yml", "yaml", "conf", "cfg",
         "ini", "sh", "swift", "m", "h", "c", "py", "js", "css", "csv"].contains(ext)
    }

    // MARK: 화면

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = ArgoTheme.background()

        fontSize = CGFloat(UserDefaults.settings.object(forKey: "readerFontSize") as? Double ?? 28)

        textView.translatesAutoresizingMaskIntoConstraints = false
        // isEditable 은 tvOS 에 없다 — 이 플랫폼의 UITextView 는 애초에
        // 편집을 하지 않는다. 선택도 끈다.
        textView.isSelectable = false
        textView.isScrollEnabled = true
        // 리모컨 입력은 캔버스가 받는다. 텍스트 뷰가 가져가면 방향키가 엉킨다.
        textView.isUserInteractionEnabled = false
        textView.backgroundColor = .clear
        textView.textColor = ArgoTheme.textPrimary()
        textView.textContainerInset = UIEdgeInsets(top: ArgoTheme.screenMargin(),
                                                   left: ArgoTheme.screenMargin(),
                                                   bottom: ArgoTheme.screenMargin() * 2,
                                                   right: ArgoTheme.screenMargin())
        view.addSubview(textView)

        canvas.translatesAutoresizingMaskIntoConstraints = false
        canvas.addTarget(self, action: #selector(toggleToolbar), for: .primaryActionTriggered)
        canvas.onPress = { [weak self] type in self?.handlePress(type) ?? false }
        view.addSubview(canvas)

        buildToolbar()

        NSLayoutConstraint.activate([
            textView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            textView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            textView.topAnchor.constraint(equalTo: view.topAnchor),
            textView.bottomAnchor.constraint(equalTo: view.bottomAnchor),

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

        loadFile()
        setToolbar(visible: true, interactive: false, animated: false)
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] {
        toolbarInteractive ? [toolbar] : [canvas]
    }

    // MARK: 읽기

    private func loadFile() {
        let url = URL(fileURLWithPath: filePath)
        guard let handle = try? FileHandle(forReadingFrom: url) else {
            apply(text: "파일을 열 수 없습니다.\n\(filePath)")
            return
        }
        defer { try? handle.close() }

        let data = handle.readData(ofLength: Self.maxBytes)
        let size = (try? FileManager.default.attributesOfItem(atPath: filePath)[.size] as? Int) ?? 0
        truncated = (size ?? 0) > Self.maxBytes

        // plist 는 바이너리일 수 있다. 그때는 읽을 수 있는 모양으로 바꾼다.
        if ext == "plist", let pretty = prettyPlist(data) {
            apply(text: pretty)
            return
        }
        if ext == "json", let pretty = prettyJSON(data) {
            apply(text: pretty)
            return
        }

        if let text = decode(data) {
            apply(text: text)
        } else {
            apply(text: "글자로 읽을 수 없는 파일입니다. 헥스 편집기로 열어 보십시오.")
        }
    }

    /// 인코딩을 찍어서 읽는다. UTF-8 이 아닌 한글 파일(CP949)이 흔하다.
    private func decode(_ data: Data) -> String? {
        if let s = String(data: data, encoding: .utf8) { return s }
        let detected = NSString.stringEncoding(for: data, encodingOptions: nil,
                                               convertedString: nil, usedLossyConversion: nil)
        if detected != 0, let s = String(data: data, encoding: .init(rawValue: detected)) { return s }
        // 마지막으로 손실을 감수하고라도 보여 준다. 안 보이는 것보다 낫다.
        return String(data: data, encoding: .isoLatin1)
    }

    /// 바이너리 plist 도 읽을 수 있게 XML 모양으로 편다.
    private func prettyPlist(_ data: Data) -> String? {
        guard let obj = try? PropertyListSerialization.propertyList(
                from: data, options: [], format: nil) else { return nil }
        guard let xml = try? PropertyListSerialization.data(
                fromPropertyList: obj, format: .xml, options: 0) else { return nil }
        return String(data: xml, encoding: .utf8)
    }

    private func prettyJSON(_ data: Data) -> String? {
        guard let obj = try? JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]),
              let pretty = try? JSONSerialization.data(withJSONObject: obj,
                                                       options: [.prettyPrinted, .sortedKeys]),
              let s = String(data: pretty, encoding: .utf8) else { return nil }
        return s
    }

    private func apply(text: String) {
        var body = text
        if truncated {
            body += "\n\n--- 여기까지만 읽었습니다 (8MB 상한) ---"
        }
        lineCount = body.reduce(into: 1) { n, c in if c == "\n" { n += 1 } }
        textView.attributedText = styled(body)
        updateStatus()
    }

    /// 거실에서 읽을 수 있게 줄 간격을 넓힌다. 기본값은 TV 에서 너무 빽빽하다.
    private func styled(_ body: String) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = fontSize * 0.35
        paragraph.paragraphSpacing = fontSize * 0.2
        paragraph.lineBreakMode = .byWordWrapping

        let font: UIFont = wantsMonospace
            ? (UIFont(name: "SF Mono Regular", size: fontSize)
               ?? UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular))
            : UIFont.systemFont(ofSize: fontSize)

        return NSAttributedString(string: body, attributes: [
            .font: font,
            .foregroundColor: ArgoTheme.textPrimary(),
            .paragraphStyle: paragraph
        ])
    }

    // MARK: 리모컨

    private func handlePress(_ type: UIPress.PressType) -> Bool {
        // 네 방향 모두 여기서 받는다. 도구 모음은 선택 버튼으로만 간다 —
        // 화면 아래에 있어서 위쪽을 넘겨 줘 봐야 갈 곳이 없다.
        let page = textView.bounds.height * 0.85
        switch type {
        case .upArrow:    scrollBy(-page);      return true
        case .downArrow:  scrollBy(page);       return true
        // 좌우는 크게 건너뛴다. 긴 로그에서 한 화면씩 가면 끝이 없다.
        case .leftArrow:  scrollBy(-page * 10); return true
        case .rightArrow: scrollBy(page * 10);  return true
        case .playPause:  cycleFontSize();      return true
        default:          return false
        }
    }

    private func scrollBy(_ delta: CGFloat) {
        let maxY = max(0, textView.contentSize.height - textView.bounds.height)
        var y = textView.contentOffset.y + delta
        y = min(max(0, y), maxY)
        textView.setContentOffset(CGPoint(x: 0, y: y), animated: true)
        updateStatus()
    }

    @objc private func cycleFontSize() {
        let sizes: [CGFloat] = [22, 26, 28, 32, 38, 44]
        let i = sizes.firstIndex(where: { $0 >= fontSize }) ?? 0
        fontSize = sizes[(i + 1) % sizes.count]
        UserDefaults.settings.set(Double(fontSize), forKey: "readerFontSize")
        textView.attributedText = styled(textView.text ?? "")
        setToolbar(visible: true, interactive: false, animated: true)
        updateStatus()
    }

    // MARK: 도구 모음

    private func buildToolbar() {
        statusLabel.font = ArgoTheme.fontMonoBody()
        statusLabel.textColor = ArgoTheme.textSecondary()

        let top = ArgoBarButton.make(title: "처음")
        top.addTarget(self, action: #selector(goTop), for: .primaryActionTriggered)
        let bottom = ArgoBarButton.make(title: "끝")
        bottom.addTarget(self, action: #selector(goBottom), for: .primaryActionTriggered)
        let size = ArgoBarButton.make(title: "글자 크기")
        size.addTarget(self, action: #selector(cycleFontSize), for: .primaryActionTriggered)
        let edit = ArgoBarButton.make(title: "편집")
        edit.addTarget(self, action: #selector(editNow), for: .primaryActionTriggered)
        let close = ArgoBarButton.make(title: "닫기")
        close.addTarget(self, action: #selector(closeNow), for: .primaryActionTriggered)

        toolbar = UIStackView(arrangedSubviews: [top, bottom, statusLabel, size, edit, close])
        toolbar.axis = .horizontal
        toolbar.spacing = ArgoTheme.gutter()
        toolbar.alignment = .center
        toolbar.translatesAutoresizingMaskIntoConstraints = false

        view.addSubview(toolbarSurface)
        toolbarSurface.addSubview(toolbar)
    }

    @objc private func goTop() {
        textView.setContentOffset(.zero, animated: true)
        updateStatus()
    }

    @objc private func goBottom() {
        let maxY = max(0, textView.contentSize.height - textView.bounds.height)
        textView.setContentOffset(CGPoint(x: 0, y: maxY), animated: true)
        updateStatus()
    }

    private func updateStatus() {
        let maxY = max(1, textView.contentSize.height - textView.bounds.height)
        let percent = Int((textView.contentOffset.y / maxY * 100).rounded())
        statusLabel.text = "\(min(100, max(0, percent)))%  ·  \(lineCount)줄  ·  \(Int(fontSize))pt"
    }

    /// 선택 버튼. 도구 모음을 띄우고 포커스를 넘긴다.
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
        if toolbarInteractive {
            setToolbar(visible: false, interactive: false, animated: true)
        } else {
            closeNow()
        }
    }

    @objc private func editNow() {
        hideTimer?.invalidate()
        onEdit?()
    }

    @objc private func closeNow() {
        hideTimer?.invalidate()
        onClose?()
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct ArgoTextReaderView: UIViewControllerRepresentable {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool
    /// 편집기로 넘어갈 때.
    var onEdit: (() -> Void)? = nil

    func makeUIViewController(context: Context) -> ArgoTextReaderController {
        let vc = ArgoTextReaderController()
        vc.filePath = filePath.hasSuffix("/") ? filePath + fileName : filePath
        vc.onClose = { isPresented = false }
        vc.onEdit = {
            isPresented = false
            onEdit?()
        }
        return vc
    }

    func updateUIViewController(_ vc: ArgoTextReaderController, context: Context) {}
}
```

**볼 곳**

- 로그·코드·json 은 고정폭, 산문은 비례폭으로 가릅니다
- 바이너리 plist 를 XML 로 펴서 보여 줍니다
- 좌우 화살표는 **열 화면씩** 건너뜁니다 — 긴 로그에서 한 화면씩은 끝이 없습니다

---

## 4. 기존 문서 뷰어의 포커스 수정 (docx·xlsx·pptx·hwp)

같은 수정을 ObjC 쪽에도 했습니다.

### `Spartan/FileOpeners/OfficeReader/WebDocumentView.m (툴바 상태)`  (53줄)

```objc
    [self.toolbarSurface addSubview:self.toolbar];
}

- (void)setToolbarVisible:(BOOL)visible animated:(BOOL)animated {
    [self setToolbarVisible:visible interactive:NO animated:animated];
}

- (void)setToolbarVisible:(BOOL)visible
              interactive:(BOOL)interactive
                 animated:(BOOL)animated {
    self.toolbarVisible = visible;
    self.toolbarInteractive = visible && interactive;
    [self.hideTimer invalidate];

    // 꺼진 버튼은 포커스를 못 받습니다. 이것이 캔버스가 화살표를 받는
    // 유일한 방법입니다.
    for (UIView *sub in self.toolbar.arrangedSubviews) {
        if ([sub isKindOfClass:[UIControl class]]) {
            ((UIControl *)sub).enabled = self.toolbarInteractive;
        }
    }
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];

    void (^apply)(void) = ^{
        // 보이기만 할 때는 옅게 — 지금 눌리지 않는다는 뜻입니다.
        CGFloat a = visible ? (self.toolbarInteractive ? 1.0 : 0.55) : 0.0;
        self.toolbar.alpha = a;
        self.toolbarSurface.alpha = a;
    };
    if (animated) { [UIView animateWithDuration:0.2 animations:apply]; } else { apply(); }

    if (visible) {
        NSTimeInterval wait = self.toolbarInteractive ? 12.0 : 5.0;
        self.hideTimer = [NSTimer scheduledTimerWithTimeInterval:wait repeats:NO
                                                           block:^(NSTimer *t) {
            [self setToolbarVisible:NO interactive:NO animated:YES];
        }];
    }
}

/// 선택 버튼. 도구 모음을 띄우고 포커스를 그쪽으로 넘깁니다.
- (void)toggleToolbar {
    [self setToolbarVisible:YES interactive:!self.toolbarInteractive animated:YES];
}

- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments {
    // 패널이 열리면 포커스가 그 안에 머물러야 합니다. 뒤에 있는
    // 캔버스로 빠지면 방향키가 문서를 움직여 패널을 쓸 수 없습니다.
    if (self.errorView) { return @[ self.errorView ]; }
    if (self.panelView) { return @[ self.panelView ]; }
    if (self.toolbarInteractive) { return @[ self.toolbar ]; }
    return @[ self.canvas ];
```

### `WebDocumentView.m (포커스 대상)`  (10줄)

```objc
- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments {
    // 패널이 열리면 포커스가 그 안에 머물러야 합니다. 뒤에 있는
    // 캔버스로 빠지면 방향키가 문서를 움직여 패널을 쓸 수 없습니다.
    if (self.errorView) { return @[ self.errorView ]; }
    if (self.panelView) { return @[ self.panelView ]; }
    if (self.toolbarInteractive) { return @[ self.toolbar ]; }
    return @[ self.canvas ];
}

#pragma mark - 설정 패널
```

### `WebDocumentView.m (리모컨)`  (30줄)

```objc
- (BOOL)handlePress:(UIPressType)type {
    // 네 방향 모두 여기서 받습니다. 도구 모음은 선택 버튼으로만 갑니다 —
    // 도구 모음을 화면 아래로 옮기면서, 위쪽을 넘겨 줘도 갈 곳이 없어
    // 아무 일도 일어나지 않게 됐습니다. 쪽 넘김이 안 되던 이유입니다.
    switch (type) {
        case UIPressTypeUpArrow:    [self scrollBy:-420]; return YES;
        case UIPressTypeDownArrow:  [self scrollBy:420];  return YES;
        case UIPressTypeLeftArrow:  [self goPrev];        return YES;
        case UIPressTypeRightArrow: [self goNext];        return YES;
        case UIPressTypePlayPause:  [self toggleNight];   return YES;
        default: return NO;
    }
}

- (void)didSwipe:(UISwipeGestureRecognizer *)g {
    [self scrollBy:(g.direction == UISwipeGestureRecognizerDirectionUp) ? 420 : -420];
}

- (void)scrollBy:(int)dy {
    [self js:[NSString stringWithFormat:@"argoNav.scroll(%d);argoNav.label()", dy]
         then:^(id r) { [self showStatus:r]; }];
}
- (void)goNext { [self js:@"argoNav.next();argoNav.label()" then:^(id r){ [self showStatus:r]; }]; }
- (void)goPrev { [self js:@"argoNav.prev();argoNav.label()" then:^(id r){ [self showStatus:r]; }]; }

- (void)toggleNight {
    self.isNight = !self.isNight;
    self.webHost.backgroundColor = self.isNight
        ? [ArgoTheme background]
        : [ArgoTheme canvasBackground];
```

**볼 곳**

- 세 뷰어의 상태 전이가 **같은지** — 하나만 다르면 거기서 버그가 납니다
- 사진에서 docx 는 `1 / 2 쪽` 이 보였는데 넘어가지 않았습니다.
  JS 쪽 `nav.go()` 는 `u[k].scrollIntoView(true)` 를 부릅니다.
  이것이 WKWebView 안에서 동작합니까?
