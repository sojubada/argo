//
//  MediaFolderProbe.swift
//  Argo
//
//  media 폴더의 문서를 Argo 엔진으로 PDF 로 바꾸고, PC 변환본과 나란히
//  놓고 봅니다.
//
//  로그에 "몇 % 다르다" 만 남기면 pptx 차트가 제대로 그려졌는지 알 수가
//  없습니다. 그래서 눈으로 볼 산출물을 남깁니다.
//
//  남는 것 (원본 이름이 X 일 때, 같은 폴더에)
//  ─────────────────────────────────────────
//    X.pdf           PC 변환본. 사람이 넣어 둡니다 — 정답지.
//    X_argo.pdf      변환기가 내놓은 PDF 그대로.
//    X_screen.pdf    뷰어가 화면에 그리는 그대로. 쪽마다 한 장.
//    X_compare.pdf   쪽마다 세 패널을 나란히. 이것만 보면 됩니다.
//
//  세 패널로 나누는 이유
//  ───────────────────
//  뷰어는 cropBox 를 먼저 보고 /Rotate 를 적용합니다. mediaBox 로 그대로
//  그린 그림과 뷰어가 그린 그림이 다르면, 변환기가 박아 넣은 상자나 회전이
//  틀린 것입니다 — 내용은 PDF 안에 있는데 화면에만 안 보이는 경우입니다.
//  그 둘을 한 쪽에 같이 놓으면 어느 단계가 틀렸는지 눈으로 갈립니다.
//
//      ┌──────────┬──────────┬──────────┐
//      │ PC 변환본 │ Argo     │ Argo     │
//      │ (정답지)  │ mediaBox │ 화면     │
//      └──────────┴──────────┴──────────┘
//
//    1 있음 · 2 있음 · 3 없음 → 상자/회전 문제. 뷰어 쪽에서 고칩니다.
//    1 있음 · 2 없음 · 3 없음 → 변환기가 내용을 못 그린 것.
//    1 없음 · 2 없음 · 3 없음 → 원래 빈 쪽. 버그가 아닙니다.
//
//  xlsx 는 제외합니다. 넓은 시트를 쪽으로 나누는 방식이 PC 와 달라서
//  같은 쪽끼리 비교하는 것이 성립하지 않습니다.
//

import UIKit
import CoreGraphics

@available(tvOS 13.0, *)
enum MediaFolderProbe {

    // MARK: - 설정

    /// 탐침이 읽을 폴더 후보.
    ///
    /// tvOS 에서 /var 는 /private/var 로 가는 심링크입니다. 둘 다 같은 자리를
    /// 가리키지만, 파일 탐색기가 어느 쪽 이름으로 보여 주느냐에 따라 사용자가
    /// 적는 경로가 갈립니다. 대소문자도 갈립니다 — Media 는 시스템이 쓰는
    /// 이름이고 media 는 사용자가 새로 만든 폴더일 수 있습니다.
    /// 먼저 찾아지는 쪽을 씁니다.
    static let mediaRootCandidates = [
        "/var/mobile/media",
        "/private/var/mobile/media",
        "/var/mobile/Media/media",
        "/var/mobile/Media/Argo/media",
    ]

    /// 실제로 쓸 폴더. 후보 중 먼저 존재하는 것.
    static var mediaRoot: String {
        for path in mediaRootCandidates where FileManager.default.fileExists(atPath: path) {
            return path
        }
        return mediaRootCandidates[0]
    }

    /// 변환 대상 확장자 (xlsx 제외)
    static let targetExtensions: Set<String> = ["hwp", "hwpx", "docx", "pptx"]

    /// 화면 렌더본을 PDF 로 남길 때의 가로 길이(점).
    ///
    /// 뷰어는 @2x 까지 올리지만 여기서는 등배로 둡니다 — 158쪽 문서를 두 배로
    /// 담으면 파일이 수백 MB 가 되고, 내용이 있는지 보는 데 두 배가 필요하지
    /// 않습니다. 뷰어가 쓸 배율은 로그에 따로 적습니다.
    private static let screenPDFWidth: CGFloat = 1280

    /// 비교 PDF 한 패널의 가로 길이(점).
    ///
    /// 세 패널이 한 쪽에 들어가면서도, 쪽을 꺼내 AirDrop 으로 넘길 수 있는
    /// 크기로 남아야 합니다 — 150쪽이면 패널 하나의 화소 수가 그대로
    /// 파일 크기로 돌아옵니다.
    private static let panelWidth: CGFloat = 560

    /// 패널 하나의 세로 한계(점). 긴 쪽은 이 안에 맞춰 줄입니다.
    private static let panelMaxHeight: CGFloat = 860

    /// 이 비율 이상 다르면 [pageDiff] 로 기록 (0~1)
    private static let diffThreshold: CGFloat = 0.005   // 0.5 %

    /// 산출물에 담을 쪽 수 상한. 넘으면 앞에서부터 자르고 로그에 적습니다.
    private static let maxPages = 150

    static let argoSuffix    = "_argo"
    static let screenSuffix  = "_screen"
    static let compareSuffix = "_compare"

    // MARK: - 진입점

    /// 탐침을 실행합니다. 결과는 ArgoDiag 로그와 폴더의 PDF 들로 남습니다.
    static func run() async {
        ArgoDiag.log("probe", "MediaFolderProbe 시작")

        // 어느 후보가 실제로 있는지 먼저 정합니다. 한 번만 정하고 아래로
        // 넘깁니다 — 도중에 바뀔 자리가 아니고, 로그에 어디를 봤는지
        // 남아야 합니다.
        let root = mediaRoot
        guard FileManager.default.fileExists(atPath: root) else {
            ArgoDiag.log("probe", "폴더 없음 — 찾아본 자리: "
                                  + mediaRootCandidates.joined(separator: ", "))
            return
        }
        ArgoDiag.log("probe", "폴더 \(root)")

        let items: [String]
        do {
            items = try FileManager.default.contentsOfDirectory(atPath: root)
        } catch {
            ArgoDiag.log("probe", "폴더 읽기 실패: \(error)")
            return
        }

        // 변환할 원본만 고릅니다. 확장자로 거르므로 .pdf 는 (PC 레퍼런스든
        // 지난 산출물이든) 원본으로 잡히지 않습니다.
        let originals = items
            .filter { targetExtensions.contains(($0 as NSString).pathExtension.lowercased()) }
            .sorted()

        if originals.isEmpty {
            ArgoDiag.log("probe", "변환할 파일 없음 — "
                                  + "hwp·hwpx·docx·pptx 를 이 폴더에 넣어 주세요")
            return
        }

        ArgoDiag.log("probe", "원본 \(originals.count)개: "
                              + originals.joined(separator: ", "))

        for name in originals {
            await probeOne(name: name, root: root)
        }

        ArgoDiag.log("probe", "MediaFolderProbe 완료")
    }

    // MARK: - 파일 하나 처리

    private static func probeOne(name: String, root: String) async {
        let stem       = (name as NSString).deletingPathExtension
        let srcPath    = root + "/" + name
        let pcPDF      = root + "/" + stem + ".pdf"
        let argoPDF    = root + "/" + stem + argoSuffix    + ".pdf"
        let screenPDF  = root + "/" + stem + screenSuffix  + ".pdf"
        let comparePDF = root + "/" + stem + compareSuffix + ".pdf"

        ArgoDiag.log("probe", "── \(name) ──────────────────")

        // 1. Argo 변환. 있던 _argo.pdf 는 버리고 늘 다시 만듭니다.
        //
        // 전에는 원본보다 새로우면 재사용했는데, 그러면 새 빌드를 올려도
        // 지난 빌드가 만든 PDF 를 비교하게 됩니다 — 탐침이 재는 것이
        // 지금 들어 있는 변환기여야 하므로 캐시를 두면 안 됩니다.
        do {
            let tmpPath = try ArgoOfficeConverter.convertToPDF(at: srcPath)
            let fm = FileManager.default
            if fm.fileExists(atPath: argoPDF) { try? fm.removeItem(atPath: argoPDF) }
            try fm.copyItem(atPath: tmpPath, toPath: argoPDF)
            try? fm.removeItem(atPath: tmpPath)
            ArgoDiag.log("probe", "변환 완료 → \(stem)\(argoSuffix).pdf")
        } catch {
            ArgoDiag.log("probe", "변환 실패: \(error)")
            return
        }

        // 변환기가 사용자에게 알릴 것을 남겼으면 같이 적어 둡니다 —
        // 쪽을 나눴다든지 하는 알림이 쪽 수 차이의 설명이 됩니다.
        if let notice = ArgoOfficeConverter.lastUserNotice {
            ArgoDiag.log("probe", "변환기 알림: \(notice)")
        }

        guard let argoDoc = CGPDFDocument(URL(fileURLWithPath: argoPDF) as CFURL),
              argoDoc.numberOfPages > 0 else {
            ArgoDiag.log("probe", "Argo PDF 를 열 수 없습니다")
            return
        }

        // 2. 화면 렌더본. PC 변환본이 없어도 이것은 남깁니다 —
        //    정답지가 없더라도 무엇이 그려졌는지는 봐야 합니다.
        writeScreenPDF(doc: argoDoc, to: screenPDF, label: stem)

        // 3. PC 변환본이 있으면 나란히 비교합니다.
        guard FileManager.default.fileExists(atPath: pcPDF) else {
            ArgoDiag.log("probe", "PC 변환본 없음 — 비교 생략 "
                                  + "(\(stem).pdf 를 같은 폴더에 넣어 주세요)")
            return
        }
        guard let pcDoc = CGPDFDocument(URL(fileURLWithPath: pcPDF) as CFURL) else {
            ArgoDiag.log("probe", "PC 변환본을 열 수 없습니다")
            return
        }

        compare(pcDoc: pcDoc, argoDoc: argoDoc, to: comparePDF, label: stem)
    }

    // MARK: - 화면 렌더본

    /// 뷰어가 화면에 그리는 그대로를 쪽마다 한 장씩 담은 PDF 를 씁니다.
    private static func writeScreenPDF(doc: CGPDFDocument, to path: String, label: String) {
        let total = doc.numberOfPages
        let count = min(total, maxPages)
        if count < total {
            ArgoDiag.log("probe", "화면 렌더본은 앞 \(count)쪽만 담습니다 (전체 \(total)쪽)")
        }

        let bounds = CGRect(x: 0, y: 0, width: screenPDFWidth,
                            height: screenPDFWidth * 3 / 4)
        let renderer = UIGraphicsPDFRenderer(bounds: bounds)
        do {
            try renderer.writePDF(to: URL(fileURLWithPath: path)) { rc in
                let available = CGSize(width: screenPDFWidth, height: screenPDFWidth * 3 / 4)
                for n in 1...count {
                    guard let page = doc.page(at: n),
                          let shot = renderViewerPath(page: page, available: available)
                    else { continue }
                    // 첫 쪽의 상자·회전·배율을 한 번 적어 둡니다. 뒤에서 쪽마다
                    // 적으면 로그가 그것만으로 가득 찹니다.
                    if n == 1 { ArgoDiag.log("probe", "첫 쪽 \(shot.info)") }
                    rc.beginPage(withBounds: CGRect(origin: .zero, size: shot.image.size),
                                 pageInfo: [:])
                    shot.image.draw(in: CGRect(origin: .zero, size: shot.image.size))
                }
            }
            let size = fileSize(path)
            ArgoDiag.log("probe", "화면 렌더본 → \(label)\(screenSuffix).pdf "
                                  + "(\(count)쪽, \(size))")
        } catch {
            ArgoDiag.log("probe", "화면 렌더본 쓰기 실패: \(error)")
        }
    }

    // MARK: - 나란히 비교

    private static func compare(pcDoc: CGPDFDocument, argoDoc: CGPDFDocument,
                                to path: String, label: String) {
        let pcCount   = pcDoc.numberOfPages
        let argoCount = argoDoc.numberOfPages
        ArgoDiag.log("probe", "PC \(pcCount)쪽 / Argo \(argoCount)쪽")
        if pcCount != argoCount {
            ArgoDiag.log("probe", "[pageCountMismatch] PC=\(pcCount) Argo=\(argoCount)")
        }

        let compareCount = min(pcCount, argoCount)
        guard compareCount > 0 else {
            ArgoDiag.log("probe", "비교할 쪽이 없습니다")
            return
        }
        let count = min(compareCount, maxPages)
        if count < compareCount {
            ArgoDiag.log("probe", "비교본은 앞 \(count)쪽만 담습니다 (겹치는 \(compareCount)쪽)")
        }

        // 세 패널 + 여백 + 위쪽 제목줄.
        let gutter: CGFloat = 20, margin: CGFloat = 24, header: CGFloat = 64
        let pageW = margin * 2 + panelWidth * 3 + gutter * 2
        let pageH = margin * 2 + header + panelMaxHeight
        let bounds = CGRect(x: 0, y: 0, width: pageW, height: pageH)

        var boxBug:   [Int] = []    // 2 있음 · 3 없음 → 상자/회전 문제
        var blankBug: [Int] = []    // 1 있음 · 2 없음 → 변환기가 못 그림
        var diffPages: [Int] = []

        let renderer = UIGraphicsPDFRenderer(bounds: bounds)
        do {
            try renderer.writePDF(to: URL(fileURLWithPath: path)) { rc in
                for n in 1...count {
                    guard let pcPage   = pcDoc.page(at: n),
                          let argoPage = argoDoc.page(at: n) else { continue }

                    let panel = CGSize(width: panelWidth, height: panelMaxHeight)
                    let pcImg   = renderMediaBox(page: pcPage,   fitting: panel)
                    let rawImg  = renderMediaBox(page: argoPage, fitting: panel)
                    let shot    = renderViewerPath(page: argoPage, available: panel)
                    let viewImg = shot?.image

                    let pcBlank   = pcImg.map(isBlank)   ?? true
                    let rawBlank  = rawImg.map(isBlank)  ?? true
                    let viewBlank = viewImg.map(isBlank) ?? true

                    // 어느 단계에서 사라졌는지 가릅니다.
                    var verdict = ""
                    if pcBlank && rawBlank && viewBlank {
                        verdict = "원래 빈 쪽"
                    } else if !pcBlank && rawBlank {
                        blankBug.append(n)
                        verdict = "[blankBug] 변환기가 못 그림"
                        ArgoDiag.log("probe", "[blankBug] 쪽 \(n) — PC 에 내용 있음, Argo PDF 가 빔")
                    } else if !rawBlank && viewBlank {
                        boxBug.append(n)
                        verdict = "[boxBug] 상자·회전 문제"
                        // 어느 상자를 어떻게 읽었는지 같이 남깁니다 —
                        // 고칠 자리를 이 한 줄에서 찾게 됩니다.
                        ArgoDiag.log("probe", "[boxBug] 쪽 \(n) — PDF 에 내용 있는데 뷰어가 못 띄움"
                                              + (shot.map { " · " + $0.info } ?? ""))
                    }

                    var diff: CGFloat = 0
                    if let a = pcImg, let b = viewImg {
                        diff = pixelDiff(a, b)
                        if verdict.isEmpty && diff >= diffThreshold {
                            diffPages.append(n)
                            verdict = String(format: "[pageDiff] %.1f%%", diff * 100)
                        }
                    }

                    rc.beginPage()
                    let title = String(format: "%@  쪽 %d / %d   차이 %.1f%%   %@",
                                       label, n, count, diff * 100, verdict)
                    title.draw(in: CGRect(x: margin, y: margin,
                                          width: pageW - margin * 2, height: header),
                               withAttributes: [
                                   .font: UIFont.systemFont(ofSize: 30, weight: .medium),
                                   .foregroundColor: UIColor.black,
                               ])

                    let names = ["PC 변환본", "Argo mediaBox", "Argo 화면"]
                    let shots = [pcImg, rawImg, viewImg]
                    let blanks = [pcBlank, rawBlank, viewBlank]
                    for (i, shot) in shots.enumerated() {
                        let colX = margin + (panelWidth + gutter) * CGFloat(i)
                        let colY = margin + header
                        let frame = CGRect(x: colX, y: colY,
                                           width: panelWidth, height: panelMaxHeight)
                        drawPanel(shot, blank: blanks[i], name: names[i], in: frame)
                    }
                }
            }
            let size = fileSize(path)
            ArgoDiag.log("probe", "비교본 → \(label)\(compareSuffix).pdf (\(count)쪽, \(size))")
        } catch {
            ArgoDiag.log("probe", "비교본 쓰기 실패: \(error)")
            return
        }

        // 요약 한 줄. 로그만 훑어도 어느 파일이 성한지 보여야 합니다.
        if boxBug.isEmpty && blankBug.isEmpty && diffPages.isEmpty {
            ArgoDiag.log("probe", "결과 \(label) — 세 판본이 같습니다 (\(count)쪽 비교)")
        } else {
            if !blankBug.isEmpty {
                ArgoDiag.log("probe", "결과 \(label) [blankBug] 쪽 "
                                      + blankBug.map(String.init).joined(separator: ", "))
            }
            if !boxBug.isEmpty {
                ArgoDiag.log("probe", "결과 \(label) [boxBug] 쪽 "
                                      + boxBug.map(String.init).joined(separator: ", "))
            }
            if !diffPages.isEmpty {
                ArgoDiag.log("probe", "결과 \(label) [pageDiff] 쪽 "
                                      + diffPages.map(String.init).joined(separator: ", "))
            }
        }
    }

    /// 패널 하나를 테두리·이름표와 함께 그립니다.
    private static func drawPanel(_ image: UIImage?, blank: Bool,
                                  name: String, in frame: CGRect) {
        let label = CGRect(x: frame.minX, y: frame.minY, width: frame.width, height: 34)
        (blank ? name + " — 빈 쪽" : name).draw(
            in: label,
            withAttributes: [
                .font: UIFont.systemFont(ofSize: 24),
                .foregroundColor: blank ? UIColor.red : UIColor.darkGray,
            ])

        let art = CGRect(x: frame.minX, y: frame.minY + 38,
                         width: frame.width, height: frame.height - 38)
        UIColor.white.setFill()
        UIBezierPath(rect: art).fill()
        UIColor.lightGray.setStroke()
        let border = UIBezierPath(rect: art)
        border.lineWidth = 1
        border.stroke()

        guard let image else { return }
        // 칸 안에 가로세로비를 지켜 가운데 놓습니다.
        let fit = min(art.width / image.size.width, art.height / image.size.height)
        let w = image.size.width * fit, h = image.size.height * fit
        image.draw(in: CGRect(x: art.midX - w / 2, y: art.minY, width: w, height: h))
    }

    // MARK: - 렌더링

    /// 뷰어가 화면에 그리는 경로를 그대로 따라 한 쪽을 그립니다.
    ///
    /// ArgoPDFReader.renderCurrentPage 의 한 쪽·가로맞춤 경우와 같아야 합니다 —
    /// cropBox 우선, /Rotate 적용, 흰 종이 먼저, 칸 가운데 맞춤. 여기가
    /// 어긋나면 탐침이 뷰어와 다른 것을 재게 되고, 그러면 아무 소용이 없습니다.
    private static func renderViewerPath(page: CGPDFPage,
                                         available: CGSize) -> (image: UIImage, info: String)? {
        // cropBox 가 비어 있는 PDF 가 있습니다. 그때는 mediaBox 로 물러섭니다.
        var kind = CGPDFBox.cropBox
        var raw = page.getBoxRect(.cropBox)
        if raw.width <= 0 || raw.height <= 0 {
            kind = .mediaBox
            raw = page.getBoxRect(.mediaBox)
        }
        guard raw.width > 0, raw.height > 0 else { return nil }

        let rotation = Int(page.rotationAngle)
        let turned = (rotation / 90) % 2 != 0
        let shown = turned ? CGSize(width: raw.height, height: raw.width)
                           : CGSize(width: raw.width,  height: raw.height)

        // 가로맞춤: 가로를 꽉 채우고 세로는 비율대로.
        let aspect = shown.width / shown.height
        let size = CGSize(width: available.width,
                          height: (available.width / aspect).rounded())
        guard size.width >= 1, size.height >= 1 else { return nil }

        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1          // 산출물 크기 때문에 등배. 뷰어 배율은 따로 적습니다.
        format.opaque = true

        let image = UIGraphicsImageRenderer(size: size, format: format).image { rc in
            let ctx = rc.cgContext
            // 종이를 먼저 깝니다. PDF 는 배경을 그리지 않아 안 깔면 비어 보입니다.
            UIColor.white.setFill()
            ctx.fill(CGRect(origin: .zero, size: size))

            // UIKit 컨텍스트는 위에서 아래로, PDF 는 아래에서 위로 갑니다.
            ctx.translateBy(x: 0, y: size.height)
            ctx.scaleBy(x: 1, y: -1)

            // getDrawingTransform 은 쪽이 칸보다 작으면 키우지 않습니다 —
            // 슬라이드 PDF 가 화면에 안 꽉 차던 원인이라 배율을 직접 구합니다.
            let fit = min(size.width / shown.width, size.height / shown.height)
            ctx.translateBy(x: size.width / 2, y: size.height / 2)
            ctx.scaleBy(x: fit, y: fit)
            if rotation != 0 { ctx.rotate(by: -CGFloat(rotation) * .pi / 180) }
            ctx.translateBy(x: -raw.midX, y: -raw.midY)
            ctx.drawPDFPage(page)
        }

        let info = String(format: "%.0f×%.0f %@ 회전%d 뷰어배율 @%.0fx",
                          raw.width, raw.height,
                          kind == .cropBox ? "cropBox" : "mediaBox",
                          rotation, viewerScale(for: size))
        return (image, info)
    }

    /// mediaBox 를 그대로 칸에 맞춰 그립니다. 뷰어의 상자·회전 해석을 타지
    /// 않으므로, 내용이 PDF 안에 있는지만 봅니다.
    private static func renderMediaBox(page: CGPDFPage, fitting panel: CGSize) -> UIImage? {
        let box = page.getBoxRect(.mediaBox)
        guard box.width > 0, box.height > 0 else { return nil }

        let fit = min(panel.width / box.width, panel.height / box.height)
        let size = CGSize(width: (box.width * fit).rounded(),
                          height: (box.height * fit).rounded())
        guard size.width >= 1, size.height >= 1 else { return nil }

        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        format.opaque = true

        return UIGraphicsImageRenderer(size: size, format: format).image { rc in
            let ctx = rc.cgContext
            UIColor.white.setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
            ctx.translateBy(x: 0, y: size.height)
            ctx.scaleBy(x: 1, y: -1)
            ctx.scaleBy(x: fit, y: fit)
            ctx.translateBy(x: -box.minX, y: -box.minY)
            ctx.drawPDFPage(page)
        }
    }

    /// 뷰어라면 몇 배로 그릴지. ArgoPDFReader.renderScale 과 같은 셈입니다 —
    /// 탐침은 등배로 남기지만, 화면에서 몇 배로 가는지는 로그에 적습니다.
    private static func viewerScale(for size: CGSize) -> CGFloat {
        let wanted = min(UIScreen.main.nativeScale, 2.0)
        guard wanted > 1 else { return 1 }
        let budget: CGFloat = 24_000_000
        let atWanted = size.width * size.height * wanted * wanted
        guard atWanted > budget else { return wanted }
        let fitted = (budget / (size.width * size.height)).squareRoot()
        return max(1, min(wanted, fitted))
    }

    // MARK: - 픽셀 판정

    /// 작게 줄여 어두운 점이 있는지 셉니다. 뷰어의 isBlank 와 같은 셈입니다.
    private static func isBlank(_ image: UIImage) -> Bool {
        guard let gray = grayscale(image) else { return true }
        let inked = gray.reduce(0) { $1 < 250 ? $0 + 1 : $0 }
        return CGFloat(inked) / CGFloat(gray.count) < 0.001
    }

    /// 두 이미지의 평균 밝기 차이를 0~1 로 돌려줍니다.
    private static func pixelDiff(_ a: UIImage, _ b: UIImage) -> CGFloat {
        guard let ga = grayscale(a), let gb = grayscale(b), ga.count == gb.count else { return 0 }
        var total = 0
        for i in 0..<ga.count { total += abs(Int(ga[i]) - Int(gb[i])) }
        return CGFloat(total) / CGFloat(ga.count) / 255.0
    }

    /// 60×60 회색 표본. 가로세로비는 버립니다 — 같은 쪽끼리 견주는 용도입니다.
    private static func grayscale(_ image: UIImage) -> [UInt8]? {
        guard let cg = image.cgImage else { return nil }
        let w = 60, h = 60
        var pixels = [UInt8](repeating: 255, count: w * h)
        let ok: Bool = pixels.withUnsafeMutableBytes { raw -> Bool in
            guard let base = raw.baseAddress,
                  let ctx = CGContext(data: base, width: w, height: h,
                                      bitsPerComponent: 8, bytesPerRow: w,
                                      space: CGColorSpaceCreateDeviceGray(),
                                      bitmapInfo: CGImageAlphaInfo.none.rawValue)
            else { return false }
            ctx.setFillColor(gray: 1, alpha: 1)
            ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
            ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        return ok ? pixels : nil
    }

    // MARK: - 유틸

    private static func fileSize(_ path: String) -> String {
        let bytes = (try? FileManager.default.attributesOfItem(atPath: path))?[.size] as? Int ?? 0
        if bytes > 1_048_576 { return String(format: "%.1fMB", Double(bytes) / 1_048_576) }
        return String(format: "%.0fKB", Double(bytes) / 1024)
    }
}
