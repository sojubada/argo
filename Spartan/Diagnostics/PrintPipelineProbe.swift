//
//  PrintPipelineProbe.swift
//  Argo
//
//  3단계의 바탕을 확인하는 탐침.
//
//  SDK 조회 결과(실행 39766805425): tvOS 17.5 SDK 는 인쇄 서식기를 전부 declare 한다.
//
//      PRESENT  UIPrintPageRenderer, UIPrintFormatter,
//               UISimpleTextPrintFormatter, UIMarkupTextPrintFormatter,
//               viewPrintFormatter, UIGraphicsPDFRenderer,
//               CGPDFDocument, CGContextDrawPDFPage
//      ABSENT   PDFKit, WebKit, QuickLook
//
//  declare 는 '있다'가 아니다. tvOS 에 인쇄 기능 자체가 없으니, 서식기가 헤더에만
//  있고 실제로는 아무것도 안 그릴 수 있다. 쪽 수가 0으로 나오거나, PDF 가 나와도
//  전부 빈 쪽일 수 있다. 그래서 '폴더가 있나'가 아니라 '잉크가 나오나'를 묻는다.
//
//  이것이 되면 3단계의 모양이 완전히 달라진다. 지금 뷰어는 HTML 을 길게 흘려보내
//  '쪽'이라는 개념이 없고, 그래서 가로 맞춤·세로 맞춤·두 장 보기·책장 넘기기가
//  애초에 성립하지 않는다. 쪽 나누기가 되면 그 네 가지가 전부 가능해진다.
//

import UIKit
import SwiftUI
import CoreGraphics

@available(tvOS 13.0, *)
final class PrintPipelineProbeController: UIViewController {

    var onClose: (() -> Void)?

    /// A4, 72dpi.
    private let pageSize = CGSize(width: 595.2, height: 841.8)
    private let pageMargin: CGFloat = 20

    private let logView = UITextView()
    private var logText = ""

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black

        logView.frame = view.bounds.insetBy(dx: 60, dy: 60)
        logView.backgroundColor = .black
        logView.textColor = .green
        logView.font = UIFont(name: "Menlo", size: 22) ?? .systemFont(ofSize: 22)
        logView.isEditable = false
        logView.isUserInteractionEnabled = true
        view.addSubview(logView)

        log("=== 인쇄 파이프라인 탐침 ===")
        log("질문: 서식기가 헤더에만 있는가, 실제로 잉크가 나오는가")
        log("")

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
            self?.runAll()
        }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses where press.type == .menu {
            onClose?()
            return
        }
        super.pressesBegan(presses, with: event)
    }

    private func log(_ s: String) {
        logText += s + "\n"
        logView.text = logText
        let end = NSRange(location: (logText as NSString).length - 1, length: 1)
        logView.scrollRangeToVisible(end)
        print("[PrintProbe] " + s)
    }

    // MARK: - 순서

    private func runAll() {
        step1_classesExist()
        step2_simpleTextFormatter()
        step3_markupFormatter()
        step4_readBackWithCGPDF()
        step5_realDocumentAsHTML()
        log("")
        log("=== 끝. 메뉴 버튼으로 닫습니다 ===")
    }

    // MARK: - 1. 클래스가 런타임에 실제로 있는가

    private func step1_classesExist() {
        log("[1] 런타임 클래스 확인")
        for name in ["UIPrintPageRenderer",
                     "UIPrintFormatter",
                     "UISimpleTextPrintFormatter",
                     "UIMarkupTextPrintFormatter",
                     "UIGraphicsPDFRenderer"] {
            let exists = NSClassFromString(name) != nil
            log("    \(exists ? "있음" : "없음  <-- 문제") \(name)")
        }
        // viewPrintFormatter 는 메서드다. 응답하는지 따로 본다.
        let responds = UIView.instancesRespond(to: Selector(("viewPrintFormatter")))
        log("    \(responds ? "응답함" : "응답 안 함  <-- 문제") UIView.viewPrintFormatter")
        log("")
    }

    // MARK: - 2. 글자만으로 쪽이 나뉘는가

    private func step2_simpleTextFormatter() {
        log("[2] UISimpleTextPrintFormatter -> 여러 쪽")

        // 한 쪽을 확실히 넘기도록 길게 만든다. 한글을 섞어 글꼴 문제도 같이 본다.
        var body = ""
        for i in 1...300 {
            body += "\(i)번째 줄입니다. The quick brown fox jumps over the lazy dog. 가나다라마바사아자차카타파하.\n"
        }
        let attributed = NSAttributedString(string: body, attributes: [
            .font: UIFont.systemFont(ofSize: 12),
            .foregroundColor: UIColor.black
        ])

        let formatter = UISimpleTextPrintFormatter(attributedText: attributed)
        guard let (data, pages) = paginate(formatter) else {
            log("    실패 — 쪽이 안 나왔습니다")
            log("")
            return
        }
        log("    쪽 수: \(pages)")
        log("    PDF 크기: \(data.count) 바이트")
        if pages <= 1 {
            log("    주의 — 300줄이 한 쪽이면 쪽 나누기가 안 되는 것입니다")
        }
        lastPDF = data
        log("")
    }

    // MARK: - 3. HTML 이 쪽으로 나뉘는가 (가장 중요)

    private var lastPDF: Data?

    private func step3_markupFormatter() {
        log("[3] UIMarkupTextPrintFormatter -> 여러 쪽  ★핵심★")
        log("    이것이 되면 WebKit 없이 HTML 을 쪽으로 나눌 수 있습니다.")
        log("    지금 뷰어의 JS 렌더러가 이미 HTML 을 만들고 있습니다.")

        var rows = ""
        for i in 1...120 {
            rows += "<tr><td>\(i)</td><td>항목 \(i)</td><td>설명 가나다 \(i)</td><td>\(i * 1234)</td></tr>"
        }
        let html = """
        <html><head><meta charset="utf-8">
        <style>
          body { font-family: -apple-system, sans-serif; font-size: 11pt; color: #000; }
          h1 { font-size: 18pt; }
          table { border-collapse: collapse; width: 100%; }
          td, th { border: 1px solid #888; padding: 4px 6px; }
        </style></head>
        <body>
          <h1>표 쪽 나누기 시험</h1>
          <p>표가 여러 쪽에 걸쳐 나뉘는지, 테두리와 한글이 살아 있는지 봅니다.</p>
          <table><tr><th>번호</th><th>이름</th><th>설명</th><th>값</th></tr>
          \(rows)
          </table>
        </body></html>
        """

        let formatter = UIMarkupTextPrintFormatter(markupText: html)
        guard let (data, pages) = paginate(formatter) else {
            log("    실패 — 쪽이 안 나왔습니다. HTML 경로는 못 씁니다.")
            log("")
            return
        }
        log("    쪽 수: \(pages)")
        log("    PDF 크기: \(data.count) 바이트")
        if pages <= 1 {
            log("    주의 — 120행이 한 쪽이면 표 쪽 나누기가 안 되는 것입니다")
        }
        lastPDF = data

        // 파일로 남겨 실제로 열어볼 수 있게 한다.
        let out = "/var/mobile/Media/argo-print-probe.pdf"
        if (try? data.write(to: URL(fileURLWithPath: out))) != nil {
            log("    저장: \(out)")
            log("    (탐색기에서 열어 눈으로 확인하십시오)")
        }
        log("")
    }

    // MARK: - 4. CGPDFDocument 로 읽어 잉크를 센다

    private func step4_readBackWithCGPDF() {
        log("[4] CGPDFDocument 로 되읽기 + 잉크 비율")
        log("    PDFKit 이 없으니 빈 쪽 판정은 CGPDF 로 해야 합니다.")

        guard let data = lastPDF else {
            log("    건너뜀 — 읽을 PDF 가 없습니다")
            log("")
            return
        }
        guard let provider = CGDataProvider(data: data as CFData),
              let doc = CGPDFDocument(provider) else {
            log("    실패 — CGPDFDocument 가 PDF 를 못 읽었습니다")
            log("")
            return
        }
        log("    CGPDF 쪽 수: \(doc.numberOfPages)")

        let limit = min(3, doc.numberOfPages)
        for i in 1...max(1, limit) {
            guard let page = doc.page(at: i) else { continue }
            let ratio = inkRatio(of: page)
            let verdict = ratio < 0.002 ? "빈 쪽" : "내용 있음"
            log(String(format: "    %d쪽: 잉크 %.4f  -> %@", i, ratio, verdict))
        }
        log("")
    }

    /// 쪽을 작게 흑백으로 그려 어두운 점의 비율을 센다.
    /// PDFEasy 의 pageInkRatio 와 같은 생각이지만 PDFPage.thumbnail 대신
    /// CGContextDrawPDFPage 를 쓴다 — PDFKit 이 없기 때문이다.
    private func inkRatio(of page: CGPDFPage) -> CGFloat {
        let box = page.getBoxRect(.cropBox)
        guard box.width > 0, box.height > 0 else { return 0 }

        let sampleLongSide: CGFloat = 100
        let scale = sampleLongSide / max(box.width, box.height)
        let w = max(1, Int((box.width * scale).rounded()))
        let h = max(1, Int((box.height * scale).rounded()))

        var pixels = [UInt8](repeating: 255, count: w * h)
        guard let ctx = CGContext(data: &pixels,
                                  width: w, height: h,
                                  bitsPerComponent: 8,
                                  bytesPerRow: w,
                                  space: CGColorSpaceCreateDeviceGray(),
                                  bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return 0 }

        // 흰색을 먼저 깐다. 투명한 쪽이 검은 쪽으로 읽히면 판정이 거꾸로 된다.
        ctx.setFillColor(gray: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))

        ctx.scaleBy(x: scale, y: scale)
        ctx.translateBy(x: -box.origin.x, y: -box.origin.y)
        ctx.drawPDFPage(page)

        let inkLevel: UInt8 = 250
        let inked = pixels.reduce(0) { $1 < inkLevel ? $0 + 1 : $0 }
        return CGFloat(inked) / CGFloat(w * h)
    }

    // MARK: - 5. 실제 문서를 HTML 로 바꿔 넣어 본다

    private func step5_realDocumentAsHTML() {
        log("[5] 실제 문서 경로 점검")
        log("    docx/xlsx 를 직접 서식기에 넣을 수는 없습니다 —")
        log("    UIMarkupTextPrintFormatter 는 HTML 만 받습니다.")
        log("    그래서 3단계의 경로는 이렇게 됩니다:")
        log("")
        log("      문서 -> (기존 JS 렌더러) -> HTML -> 서식기 -> 쪽이 있는 PDF")
        log("                                            -> CGPDF 로 그리기")
        log("")
        log("    JS 렌더러는 이미 돌고 있습니다. 바뀌는 것은 '그 뒤'입니다.")

        // 바탕이 되는 글꼴이 실제로 한글을 그리는지만 확인한다.
        let koreanHTML = "<html><body style='font-size:40pt'>한글 가나다 漢字 ABC 123</body></html>"
        let formatter = UIMarkupTextPrintFormatter(markupText: koreanHTML)
        if let (data, pages) = paginate(formatter),
           let provider = CGDataProvider(data: data as CFData),
           let doc = CGPDFDocument(provider),
           let page = doc.page(at: 1) {
            let ratio = inkRatio(of: page)
            log("")
            log(String(format: "    한글 한 줄: %d쪽, 잉크 %.4f", pages, ratio))
            if ratio < 0.002 {
                log("    주의 — 한글이 안 그려졌습니다. 글꼴 지정이 필요합니다.")
            } else {
                log("    한글이 그려집니다.")
            }
        } else {
            log("    한글 시험 실패")
        }
    }

    // MARK: - 공통

    /// 서식기를 쪽으로 나눠 PDF 로 만든다.
    /// paperRect·printableRect 는 읽기 전용이라 KVC 로 넣는다 —
    /// 맨 UIPrintPageRenderer 를 설정하는 문서화된 방법이다.
    private func paginate(_ formatter: UIPrintFormatter) -> (Data, Int)? {
        let renderer = UIPrintPageRenderer()
        renderer.addPrintFormatter(formatter, startingAtPageAt: 0)

        let paperRect = CGRect(origin: .zero, size: pageSize)
        let printableRect = paperRect.insetBy(dx: pageMargin, dy: pageMargin)
        renderer.setValue(NSValue(cgRect: paperRect), forKey: "paperRect")
        renderer.setValue(NSValue(cgRect: printableRect), forKey: "printableRect")

        // numberOfPages 를 읽는 것이 쪽 나누기를 일으킨다.
        let pageCount = renderer.numberOfPages
        guard pageCount > 0 else { return nil }

        let pdfRenderer = UIGraphicsPDFRenderer(bounds: paperRect)
        let data = pdfRenderer.pdfData { context in
            for i in 0..<pageCount {
                context.beginPage()
                renderer.drawPage(at: i, in: paperRect)
            }
        }
        guard !data.isEmpty else { return nil }
        return (data, pageCount)
    }
}

// MARK: - SwiftUI 껍데기

@available(tvOS 13.0, *)
struct PrintPipelineProbeView: UIViewControllerRepresentable {
    @Binding var isPresented: Bool

    func makeUIViewController(context: Context) -> PrintPipelineProbeController {
        let vc = PrintPipelineProbeController()
        vc.onClose = { isPresented = false }
        return vc
    }

    func updateUIViewController(_ vc: PrintPipelineProbeController, context: Context) {}
}
