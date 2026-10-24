//
//  MediaFolderProbe.swift
//  Argo
//
//  /var/mobile/media/ 폴더 안의 문서를 Argo 엔진으로 PDF 로 바꾸고,
//  같은 폴더에 있는 PC 변환 PDF 와 쪽별로 비교합니다.
//
//  동작 방식
//  ─────────
//  1. /var/mobile/media/ 를 훑어 변환 가능한 원본을 모읍니다.
//     (xlsx 는 제외합니다 — 탐침 목적 밖입니다.)
//  2. 원본과 이름이 같은 .pdf 가 있으면 PC 레퍼런스로 씁니다.
//  3. Argo 엔진으로 변환해 _argo.pdf 를 같은 폴더에 저장합니다.
//  4. 두 PDF 를 쪽별로 렌더링해 픽셀 diff 를 구합니다:
//       - 쪽 수가 다르면 즉시 기록합니다.
//       - 같으면 각 쪽을 300×300 gray 썸네일로 비교합니다.
//       - 밝기 차가 임계값(0.5%) 이상인 쪽을 [pageDiff] 로 기록합니다.
//       - Argo 쪽이 blank 인데 PC 쪽에 내용이 있으면 [blankBug] 로
//         따로 표시합니다 — 변환 버그 확정.
//  5. 모든 결과가 ArgoDiag 로 기록됩니다. AirDrop 으로 꺼내면 됩니다.
//
//  호출 방법
//  ─────────
//  앱 시작 시 또는 설정 화면에서 호출:
//      await MediaFolderProbe.run()
//
//  xlsx 를 제외한 이유: 넓은 시트의 쪽 나누기 방식이 PC 와 다를 수
//  있어서 픽셀 diff 가 무의미합니다.
//

import UIKit
import CoreGraphics

@available(tvOS 13.0, *)
enum MediaFolderProbe {

    // MARK: - 설정

    /// 탐침이 읽을 폴더
    static let mediaRoot = "/var/mobile/media"

    /// 변환 대상 확장자 (xlsx 제외)
    static let targetExtensions: Set<String> = ["hwp", "hwpx", "docx", "pptx"]

    /// 썸네일 크기 — 클수록 정밀하지만 느립니다
    private static let thumbSize = CGSize(width: 300, height: 300)

    /// 이 비율 이상 다르면 [pageDiff] 로 기록 (0~1)
    private static let diffThreshold: CGFloat = 0.005   // 0.5 %

    /// Argo 변환본 접미사
    static let argoSuffix = "_argo"

    // MARK: - 진입점

    /// 탐침을 실행합니다. 결과는 ArgoDiag 로그에 기록됩니다.
    static func run() async {
        ArgoDiag.log("probe", "MediaFolderProbe 시작 — \(mediaRoot)")

        guard FileManager.default.fileExists(atPath: mediaRoot) else {
            ArgoDiag.log("probe", "폴더 없음: \(mediaRoot)")
            return
        }

        let items: [String]
        do {
            items = try FileManager.default.contentsOfDirectory(atPath: mediaRoot)
        } catch {
            ArgoDiag.log("probe", "폴더 읽기 실패: \(error)")
            return
        }

        // 원본 파일 목록 (xlsx 제외)
        let originals = items.filter { name in
            let ext = (name as NSString).pathExtension.lowercased()
            guard targetExtensions.contains(ext) else { return false }
            // _argo.pdf 의 stem 을 원본으로 오인하지 않습니다
            return true
        }

        if originals.isEmpty {
            ArgoDiag.log("probe", "변환할 파일 없음")
            return
        }

        ArgoDiag.log("probe", "원본 \(originals.count)개 발견")

        for name in originals.sorted() {
            await probeOne(name: name, items: items)
        }

        ArgoDiag.log("probe", "MediaFolderProbe 완료")
    }

    // MARK: - 파일 하나 처리

    private static func probeOne(name: String, items: [String]) async {
        let stem = (name as NSString).deletingPathExtension
        let srcPath  = mediaRoot + "/" + name
        let pcPDF    = mediaRoot + "/" + stem + ".pdf"
        let argoPDF  = mediaRoot + "/" + stem + argoSuffix + ".pdf"

        ArgoDiag.log("probe", "── \(name) ──────────────────")

        // 1. Argo 변환 (기존에 없거나 원본보다 오래됐으면 재변환)
        let needsConvert: Bool = {
            guard FileManager.default.fileExists(atPath: argoPDF) else { return true }
            let srcMod  = modDate(srcPath)
            let argMod  = modDate(argoPDF)
            if let s = srcMod, let a = argMod { return s > a }
            return true
        }()

        if needsConvert {
            do {
                let tmpPath = try ArgoOfficeConverter.convertToPDF(at: srcPath)
                // tmp → mediaRoot/_argo.pdf 로 복사
                let fm = FileManager.default
                if fm.fileExists(atPath: argoPDF) { try? fm.removeItem(atPath: argoPDF) }
                try fm.copyItem(atPath: tmpPath, toPath: argoPDF)
                try? fm.removeItem(atPath: tmpPath)
                ArgoDiag.log("probe", "변환 완료 → \(stem)\(argoSuffix).pdf")
            } catch {
                ArgoDiag.log("probe", "변환 실패: \(error)")
                return
            }
        } else {
            ArgoDiag.log("probe", "기존 Argo PDF 재사용")
        }

        // 2. PC PDF 없으면 비교 생략
        guard FileManager.default.fileExists(atPath: pcPDF) else {
            ArgoDiag.log("probe", "PC PDF 없음 — 비교 생략 (\(stem).pdf 를 넣어 주세요)")
            return
        }

        // 3. 두 PDF 열기
        guard
            let pcDoc   = CGPDFDocument(URL(fileURLWithPath: pcPDF)   as CFURL),
            let argoDoc = CGPDFDocument(URL(fileURLWithPath: argoPDF)  as CFURL)
        else {
            ArgoDiag.log("probe", "PDF 열기 실패")
            return
        }

        let pcCount   = pcDoc.numberOfPages
        let argoCount = argoDoc.numberOfPages

        ArgoDiag.log("probe", "PC \(pcCount)쪽 / Argo \(argoCount)쪽")

        if pcCount != argoCount {
            ArgoDiag.log("probe", "[pageCountMismatch] PC=\(pcCount) Argo=\(argoCount) — 쪽 수 차이")
        }

        // 4. 쪽별 비교
        let compareCount = min(pcCount, argoCount)
        var diffPages: [Int] = []
        var blankBugPages: [Int] = []

        for pageIdx in 1...max(1, compareCount) {
            guard
                let pcPage   = pcDoc.page(at: pageIdx),
                let argoPage = argoDoc.page(at: pageIdx)
            else { continue }

            let pcImg   = render(page: pcPage,   size: thumbSize)
            let argoImg = render(page: argoPage, size: thumbSize)

            guard let pcI = pcImg, let argoI = argoImg else { continue }

            let pcBlank   = isBlank(pcI)
            let argoBlank = isBlank(argoI)
            let diffRatio = pixelDiff(pcI, argoI)

            // Argo blank, PC 에 내용 있으면 변환 버그 확정
            if argoBlank && !pcBlank {
                blankBugPages.append(pageIdx)
                ArgoDiag.log("probe", "[blankBug] 쪽 \(pageIdx) — Argo blank, PC 에 내용 있음")
            } else if diffRatio >= diffThreshold {
                diffPages.append(pageIdx)
                ArgoDiag.log("probe",
                    String(format: "[pageDiff] 쪽 %d — diff %.2f%%", pageIdx, diffRatio * 100))
            }
        }

        // 5. 요약
        if blankBugPages.isEmpty && diffPages.isEmpty {
            ArgoDiag.log("probe", "✅ \(name) — 두 PDF 동일 (비교 \(compareCount)쪽)")
        } else {
            if !blankBugPages.isEmpty {
                ArgoDiag.log("probe",
                    "❌ [blankBug] \(name) — 쪽 \(blankBugPages.map(String.init).joined(separator: ", "))")
            }
            if !diffPages.isEmpty {
                ArgoDiag.log("probe",
                    "⚠️ [pageDiff] \(name) — 쪽 \(diffPages.map(String.init).joined(separator: ", "))")
            }
        }
    }

    // MARK: - 렌더링

    /// PDF 한 쪽을 지정 크기 gray 이미지로 렌더링합니다.
    private static func render(page: CGPDFPage, size: CGSize) -> UIImage? {
        let box    = page.getBoxRect(.mediaBox)
        let scaleX = size.width  / max(box.width,  1)
        let scaleY = size.height / max(box.height, 1)
        let scale  = min(scaleX, scaleY)
        let w = Int(box.width  * scale)
        let h = Int(box.height * scale)
        guard w > 0, h > 0 else { return nil }

        var pixels = [UInt8](repeating: 255, count: w * h)
        guard let ctx = CGContext(
            data: &pixels, width: w, height: h,
            bitsPerComponent: 8, bytesPerRow: w,
            space: CGColorSpaceCreateDeviceGray(),
            bitmapInfo: CGImageAlphaInfo.none.rawValue
        ) else { return nil }

        ctx.setFillColor(gray: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        ctx.scaleBy(x: scale, y: scale)
        ctx.drawPDFPage(page)

        guard let cgImg = ctx.makeImage() else { return nil }
        return UIImage(cgImage: cgImg)
    }

    // MARK: - 픽셀 비교

    /// 두 이미지의 픽셀 평균 밝기 차이를 0~1 로 돌려줍니다.
    private static func pixelDiff(_ a: UIImage, _ b: UIImage) -> CGFloat {
        let w = 60, h = 60
        var pixA = [UInt8](repeating: 255, count: w * h)
        var pixB = [UInt8](repeating: 255, count: w * h)

        func draw(_ img: UIImage, into buf: inout [UInt8]) {
            guard let cg = img.cgImage,
                  let ctx = CGContext(
                    data: &buf, width: w, height: h,
                    bitsPerComponent: 8, bytesPerRow: w,
                    space: CGColorSpaceCreateDeviceGray(),
                    bitmapInfo: CGImageAlphaInfo.none.rawValue
                  ) else { return }
            ctx.setFillColor(gray: 1, alpha: 1)
            ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
            ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
        }

        draw(a, into: &pixA)
        draw(b, into: &pixB)

        var totalDiff: Int = 0
        for i in 0..<(w * h) {
            totalDiff += abs(Int(pixA[i]) - Int(pixB[i]))
        }
        return CGFloat(totalDiff) / CGFloat(w * h) / 255.0
    }

    /// 이미지가 거의 흰색(blank)인지 판정합니다.
    private static func isBlank(_ image: UIImage) -> Bool {
        guard let cg = image.cgImage else { return true }
        let w = 60, h = 60
        var pixels = [UInt8](repeating: 255, count: w * h)
        guard let ctx = CGContext(
            data: &pixels, width: w, height: h,
            bitsPerComponent: 8, bytesPerRow: w,
            space: CGColorSpaceCreateDeviceGray(),
            bitmapInfo: CGImageAlphaInfo.none.rawValue
        ) else { return false }
        ctx.setFillColor(gray: 1, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
        ctx.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
        let inked = pixels.reduce(0) { $1 < 250 ? $0 + 1 : $0 }
        return CGFloat(inked) / CGFloat(w * h) < 0.001
    }

    // MARK: - 유틸

    private static func modDate(_ path: String) -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date
    }
}
