//
//  ArgoPDFNight.swift
//  Argo
//
//  PDF 야간 모드.
//
//  전에는 쪽 전체에 '색 뒤집기 + 색상 180° 회전'을 걸었다. 그 결과
//  - 사진·그림까지 뒤집혀 음화(네거티브)가 됐고,
//  - 바탕은 순흑, 글자는 순백이라 눈이 부셨고,
//  - 원래 어두운 슬라이드는 거꾸로 하얗게 됐다.
//
//  다른 뷰어들이 푼 방법을 섞어 다시 짰다.
//  - Okular "Invert luma": 밝기만 뒤집고 색상·채도는 그대로 둔다.
//    RGB 를 뒤집는 대신 지각 색공간(OKLab)의 L 만 뒤집는다.
//  - pdf.js(Firefox) pageColors: 흰 종이 → 정한 바탕색, 검은 글자 → 정한
//    글자색으로 옮긴다. 순흑·순백을 쓰지 않는다.
//  - zathura recolor-keephue + recolor-reverse-video: 종이와 글자만 다시
//    칠하고, 쪽 안의 그림(이미지 XObject)은 원래 색으로 되돌려 놓는다.
//  - Apple Books·Dark Reader: 되돌린 그림은 조금 어둡게 해 눈부심을 줄인다.
//  - 이전 뷰어: 원래 어두운 쪽은 손대지 않는다.
//
//  색 변환은 3D LUT(CIColorCubeWithColorSpace) 하나로 GPU 에서 한다.
//  LUT 는 앱이 처음 야간 모드를 쓸 때 한 번 만든다.
//

import UIKit
import CoreImage
import CoreGraphics

// MARK: - 색 변환

@available(tvOS 13.0, *)
enum ArgoPDFNight {

    /// 종이(흰색)가 옮겨 갈 바탕색. #1E1E20 — 순흑보다 한 단 밝다.
    /// 순흑 바탕에 밝은 글자는 획이 번져 보인다(헐레이션).
    static let paperRGB: (Double, Double, Double) = (0.118, 0.118, 0.125)
    /// 글자(검은색)가 옮겨 갈 색. #E2E2DE — 순백보다 낮춰 눈부심을 줄인다.
    /// 바탕과의 대비는 약 13:1 로, 읽기에 충분하다.
    static let inkRGB: (Double, Double, Double) = (0.886, 0.886, 0.871)

    static var paperColor: UIColor {
        UIColor(red: CGFloat(paperRGB.0), green: CGFloat(paperRGB.1),
                blue: CGFloat(paperRGB.2), alpha: 1)
    }

    /// 중간 밝기를 글자 쪽으로 당기는 정도(1 이면 곧은 선).
    /// 빨강·파랑 글자가 바탕에 묻히지 않게 0.75 로 둔다.
    /// 빨강 #FF0000 → (214,24,16), 파랑 #0000FF → (96,143,247).
    private static let lightnessCurve = 0.75
    /// 어두운 바탕에서 원색이 떠 보이지 않게 채도를 조금 줄인다.
    private static let chromaScale = 0.9
    /// 원래 색으로 되돌린 그림을 얼마나 어둡게 할지 (0 = 그대로).
    private static let imageDim: CGFloat = 0.12
    /// 쪽 평균 밝기가 이보다 낮으면 원래 어두운 쪽으로 보고 손대지 않는다.
    private static let darkPageLuma: CGFloat = 0.38

    private static let cubeSize = 32
    private static let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!
    private static let ciContext = CIContext(options: [.workingColorSpace: CGColorSpace(name: CGColorSpace.linearSRGB)!,
                                                       .cacheIntermediates: false])

    /// 화면에 놓인 쪽 하나.
    struct Slot {
        /// 쪽이 그려진 자리. 점 단위, 위가 0.
        var frame: CGRect
        /// 쪽 안의 그림 자리. 점 단위, 위가 0.
        var images: [CGRect]
    }

    /// 낮 그림을 받아 밤 그림을 돌려준다.
    static func render(day: UIImage, slots: [Slot]) -> UIImage {
        guard let dayCG = day.cgImage, let nightCG = recolor(dayCG) else {
            ArgoDiag.log("night", "색 변환 실패 — 낮 그림 그대로")
            return day
        }
        let scale = day.scale
        let size = day.size

        // 1) 쪽마다: 원래 어두운 쪽인가, 되돌릴 그림은 어디인가.
        var keep: [CGRect] = []        // 원래 색 그대로 둘 자리 (어둡게 하지 않음)
        var photos: [CGRect] = []      // 원래 색으로 되돌리고 조금 어둡게 할 자리
        var darkPages = 0, paperLike = 0, tiny = 0

        for slot in slots {
            guard let page = stats(of: dayCG, rect: slot.frame, scale: scale) else { continue }
            if page.meanLuma < darkPageLuma {
                keep.append(slot.frame)
                darkPages += 1
                continue
            }
            let slotArea = max(1, slot.frame.width * slot.frame.height)
            for raw in slot.images {
                let r = raw.intersection(slot.frame)
                guard !r.isNull, r.width >= 2, r.height >= 2 else { continue }
                let coverage = r.width * r.height / slotArea
                // 아주 작은 그림(글머리표·선 조각)은 글과 같이 다시 칠한다.
                if coverage < 0.002 { tiny += 1; continue }
                guard let s = stats(of: dayCG, rect: r, scale: scale) else { continue }
                // 스캔한 글 쪽·흰 종이 무늬: 밝고 색이 거의 없다 → 종이로 본다.
                let isPaper = s.bright > 0.55 && s.chroma < 0.10
                // 쪽을 거의 덮는 밝은 배경 그림 → 종이로 본다.
                let isBackdrop = coverage > 0.6 && s.meanLuma > 0.62
                if isPaper || isBackdrop { paperLike += 1; continue }
                photos.append(r)
            }
        }

        ArgoDiag.log("night", "쪽 \(slots.count) · 어두운 쪽 \(darkPages) · 되돌린 그림 \(photos.count) "
                            + "· 종이로 본 그림 \(paperLike) · 작은 그림 \(tiny)")

        let nightImage = UIImage(cgImage: nightCG, scale: scale, orientation: .up)
        // 되돌릴 것이 없으면 그림 한 장을 더 만들지 않는다 (메모리).
        if keep.isEmpty && photos.isEmpty { return nightImage }

        // 2) 밤 그림 위에 원래 색 자리를 얹는다.
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = scale
        format.opaque = true
        format.preferredRange = .standard
        let full = CGRect(origin: .zero, size: size)
        return UIGraphicsImageRenderer(size: size, format: format).image { rc in
            let ctx = rc.cgContext
            nightImage.draw(in: full)

            if !keep.isEmpty {
                ctx.saveGState()
                ctx.addRects(keep)
                ctx.clip()
                day.draw(in: full)
                ctx.restoreGState()
            }
            if !photos.isEmpty {
                // 겹친 그림이 두 번 어두워지지 않게 한 번에 자른다.
                ctx.saveGState()
                ctx.addRects(photos)
                ctx.clip()
                day.draw(in: full)
                UIColor(white: 0, alpha: imageDim).setFill()
                ctx.fill(full)
                ctx.restoreGState()
            }
        }
    }

    // MARK: LUT

    private static func recolor(_ image: CGImage) -> CGImage? {
        let input = CIImage(cgImage: image)
        guard let cube = CIFilter(name: "CIColorCubeWithColorSpace") else { return nil }
        cube.setValue(input, forKey: kCIInputImageKey)
        cube.setValue(cubeSize, forKey: "inputCubeDimension")
        cube.setValue(cubeData, forKey: "inputCubeData")
        cube.setValue(sRGB, forKey: "inputColorSpace")
        guard let out = cube.outputImage else { return nil }
        return ciContext.createCGImage(out, from: input.extent, format: .RGBA8, colorSpace: sRGB)
    }

    /// r 이 가장 빨리, 그다음 g, b 순으로 도는 RGBA Float32 큐브.
    private static let cubeData: Data = {
        let n = cubeSize
        var data = [Float](repeating: 0, count: n * n * n * 4)
        let paper = oklab(linear(paperRGB))
        let ink = oklab(linear(inkRGB))
        var i = 0
        for b in 0..<n {
            for g in 0..<n {
                for r in 0..<n {
                    let c = (Double(r) / Double(n - 1), Double(g) / Double(n - 1), Double(b) / Double(n - 1))
                    let o = map(c, paper: paper, ink: ink)
                    data[i] = Float(o.0); data[i + 1] = Float(o.1); data[i + 2] = Float(o.2); data[i + 3] = 1
                    i += 4
                }
            }
        }
        return data.withUnsafeBufferPointer { Data(buffer: $0) }
    }()

    /// sRGB 한 색을 야간 색으로. 밝기(OKLab L)만 뒤집고 색상은 둔다.
    private static func map(_ c: (Double, Double, Double),
                            paper: (Double, Double, Double),
                            ink: (Double, Double, Double)) -> (Double, Double, Double) {
        let lab = oklab(linear(c))
        let L = min(1, max(0, lab.0))
        let t = 1 - L                                   // 0 = 흰 종이, 1 = 검은 글자
        let newL = paper.0 + (ink.0 - paper.0) * pow(t, lightnessCurve)
        // 바탕·글자색에 기운 색조가 있으면 그만큼 섞는다 (무채색이면 0).
        let tintA = L * paper.1 + t * ink.1
        let tintB = L * paper.2 + t * ink.2

        // 화면에 낼 수 없는 색이면 채도를 줄여 안으로 들인다 (색상·밝기는 지킨다).
        var k = chromaScale
        var rgb = linearRGB((newL, lab.1 * k + tintA, lab.2 * k + tintB))
        var tries = 0
        while !inGamut(rgb) && tries < 16 {
            k *= 0.85
            rgb = linearRGB((newL, lab.1 * k + tintA, lab.2 * k + tintB))
            tries += 1
        }
        return (encode(rgb.0), encode(rgb.1), encode(rgb.2))
    }

    private static func inGamut(_ c: (Double, Double, Double)) -> Bool {
        let e = 1e-4
        return c.0 >= -e && c.0 <= 1 + e && c.1 >= -e && c.1 <= 1 + e && c.2 >= -e && c.2 <= 1 + e
    }

    private static func decode(_ v: Double) -> Double {
        v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
    }

    private static func encode(_ v: Double) -> Double {
        let c = min(1, max(0, v))
        return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1 / 2.4) - 0.055
    }

    private static func linear(_ c: (Double, Double, Double)) -> (Double, Double, Double) {
        (decode(c.0), decode(c.1), decode(c.2))
    }

    /// 선형 sRGB → OKLab (Björn Ottosson).
    private static func oklab(_ c: (Double, Double, Double)) -> (Double, Double, Double) {
        let l = cbrt(0.4122214708 * c.0 + 0.5363325363 * c.1 + 0.0514459929 * c.2)
        let m = cbrt(0.2119034982 * c.0 + 0.6806995451 * c.1 + 0.1073969566 * c.2)
        let s = cbrt(0.0883024619 * c.0 + 0.2817188376 * c.1 + 0.6299787005 * c.2)
        return (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
                1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
                0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)
    }

    /// OKLab → 선형 sRGB.
    private static func linearRGB(_ lab: (Double, Double, Double)) -> (Double, Double, Double) {
        let l = pow(lab.0 + 0.3963377774 * lab.1 + 0.2158037573 * lab.2, 3)
        let m = pow(lab.0 - 0.1055613458 * lab.1 - 0.0638541728 * lab.2, 3)
        let s = pow(lab.0 - 0.0894841775 * lab.1 - 1.2914855480 * lab.2, 3)
        return ( 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
                -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
                -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)
    }

    // MARK: 밝기 재기

    struct Stats {
        /// 평균 밝기 0…1.
        var meanLuma: CGFloat
        /// 밝기 0.85 넘는 점의 비율.
        var bright: CGFloat
        /// 평균 채도(최대 성분 - 최소 성분) 0…1.
        var chroma: CGFloat
    }

    /// 점 단위 자리를 24×24 로 줄여 잰다.
    static func stats(of image: CGImage, rect: CGRect, scale: CGFloat) -> Stats? {
        let px = CGRect(x: rect.minX * scale, y: rect.minY * scale,
                        width: rect.width * scale, height: rect.height * scale)
            .integral
            .intersection(CGRect(x: 0, y: 0, width: image.width, height: image.height))
        guard !px.isNull, px.width >= 1, px.height >= 1,
              let crop = image.cropping(to: px) else { return nil }

        let w = 24, h = 24
        var pixels = [UInt8](repeating: 255, count: w * h * 4)
        let made: Bool = pixels.withUnsafeMutableBytes { buf in
            guard let ctx = CGContext(data: buf.baseAddress, width: w, height: h,
                                      bitsPerComponent: 8, bytesPerRow: w * 4, space: sRGB,
                                      bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return false }
            ctx.interpolationQuality = .medium
            ctx.draw(crop, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        guard made else { return nil }

        var luma: CGFloat = 0, bright = 0, chroma: CGFloat = 0
        for i in stride(from: 0, to: pixels.count, by: 4) {
            let r = CGFloat(pixels[i]) / 255, g = CGFloat(pixels[i + 1]) / 255, b = CGFloat(pixels[i + 2]) / 255
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b
            luma += y
            if y > 0.85 { bright += 1 }
            chroma += max(r, g, b) - min(r, g, b)
        }
        let count = CGFloat(w * h)
        return Stats(meanLuma: luma / count, bright: CGFloat(bright) / count, chroma: chroma / count)
    }
}

// MARK: - 쪽 안의 그림 찾기

/// 쪽의 내용 흐름을 훑어 그림(이미지 XObject·인라인 그림)이 놓인 자리를 찾는다.
///
/// zathura 가 mupdf 에게 받는 '쪽의 그림 목록'을 CoreGraphics 로 직접 만든다.
/// q / Q / cm 으로 좌표 변환을 따라가고, Do 에서 그림이면 단위 정사각형을
/// 지금 변환으로 옮긴 자리를 적는다. 폼 XObject 는 안으로 들어간다.
/// 마스크 그림(ImageMask)은 글자처럼 칠해지는 것이라 빼고 다시 칠한다.
@available(tvOS 13.0, *)
enum ArgoPDFImageScan {

    /// 쪽 기본 좌표계(PDF 사용자 공간)에서의 그림 자리.
    static func imageRects(in page: CGPDFPage) -> [CGRect] {
        guard let table = makeTable() else { return [] }
        defer { CGPDFOperatorTableRelease(table) }

        let state = ScanState()
        state.table = table
        if let pageDict = page.dictionary {
            var res: CGPDFDictionaryRef?
            if CGPDFDictionaryGetDictionary(pageDict, "Resources", &res), let res {
                state.resources.append(res)
            }
        }

        let stream: CGPDFContentStreamRef? = CGPDFContentStreamCreateWithPage(page)
        guard let stream else { return [] }
        defer { CGPDFContentStreamRelease(stream) }
        scan(stream, state: state)
        return state.rects
    }

    private final class ScanState {
        var ctm: CGAffineTransform = .identity
        var saved: [CGAffineTransform] = []
        var rects: [CGRect] = []
        var resources: [CGPDFDictionaryRef] = []
        var table: CGPDFOperatorTableRef?
        var depth = 0
        var forms = 0
    }

    private static let maxDepth = 8
    private static let maxForms = 300
    private static let maxRects = 256

    private static func scan(_ stream: CGPDFContentStreamRef, state: ScanState) {
        guard let table = state.table else { return }
        let info = Unmanaged.passUnretained(state).toOpaque()
        let scanner: CGPDFScannerRef? = CGPDFScannerCreate(stream, table, info)
        guard let scanner else { return }
        _ = CGPDFScannerScan(scanner)
        CGPDFScannerRelease(scanner)
    }

    private static func state(_ info: UnsafeMutableRawPointer?) -> ScanState? {
        guard let info else { return nil }
        return Unmanaged<ScanState>.fromOpaque(info).takeUnretainedValue()
    }

    private static func makeTable() -> CGPDFOperatorTableRef? {
        let made: CGPDFOperatorTableRef? = CGPDFOperatorTableCreate()
        guard let table = made else { return nil }

        CGPDFOperatorTableSetCallback(table, "q") { _, info in
            guard let st = ArgoPDFImageScan.state(info) else { return }
            st.saved.append(st.ctm)
        }
        CGPDFOperatorTableSetCallback(table, "Q") { _, info in
            guard let st = ArgoPDFImageScan.state(info) else { return }
            if let last = st.saved.popLast() { st.ctm = last }
        }
        CGPDFOperatorTableSetCallback(table, "cm") { scanner, info in
            guard let st = ArgoPDFImageScan.state(info) else { return }
            // 스택에서 꺼내므로 f e d c b a 순서로 나온다.
            var v = [CGPDFReal](repeating: 0, count: 6)
            for i in stride(from: 5, through: 0, by: -1) {
                var n: CGPDFReal = 0
                guard CGPDFScannerPopNumber(scanner, &n) else { return }
                v[i] = n
            }
            let m = CGAffineTransform(a: v[0], b: v[1], c: v[2], d: v[3], tx: v[4], ty: v[5])
            st.ctm = m.concatenating(st.ctm)
        }
        CGPDFOperatorTableSetCallback(table, "Do") { scanner, info in
            guard let st = ArgoPDFImageScan.state(info) else { return }
            ArgoPDFImageScan.handleDo(scanner, st)
        }
        // 인라인 그림(BI … ID … EI). 스캐너가 그림을 흐름으로 올려 준다.
        // 어느 연산자에 붙는지 문서마다 말이 달라 둘 다 받는다 —
        // 흐름을 꺼내지 못하면 아무것도 적지 않는다(다시 칠하는 쪽이 안전).
        let inline: CGPDFOperatorCallback = { scanner, info in
            guard let st = ArgoPDFImageScan.state(info) else { return }
            var stream: CGPDFStreamRef?
            guard CGPDFScannerPopStream(scanner, &stream), let stream,
                  let dict = CGPDFStreamGetDictionary(stream) else { return }
            var mask: CGPDFBoolean = 0
            if CGPDFDictionaryGetBoolean(dict, "IM", &mask), mask != 0 { return }
            if CGPDFDictionaryGetBoolean(dict, "ImageMask", &mask), mask != 0 { return }
            ArgoPDFImageScan.addUnitSquare(st)
        }
        CGPDFOperatorTableSetCallback(table, "BI", inline)
        CGPDFOperatorTableSetCallback(table, "EI", inline)
        return table
    }

    private static func addUnitSquare(_ st: ScanState) {
        guard st.rects.count < maxRects else { return }
        let r = CGRect(x: 0, y: 0, width: 1, height: 1).applying(st.ctm)
        guard r.width.isFinite, r.height.isFinite, r.width > 0, r.height > 0 else { return }
        if st.rects.last != r { st.rects.append(r) }
    }

    private static func handleDo(_ scanner: CGPDFScannerRef, _ st: ScanState) {
        var namePtr: UnsafePointer<CChar>?
        guard CGPDFScannerPopName(scanner, &namePtr), let name = namePtr else { return }
        let content = CGPDFScannerGetContentStream(scanner)
        guard let object = CGPDFContentStreamGetResource(content, "XObject", name) else { return }
        var streamRef: CGPDFStreamRef?
        guard CGPDFObjectGetValue(object, .stream, &streamRef), let stream = streamRef,
              let dict = CGPDFStreamGetDictionary(stream) else { return }
        var subPtr: UnsafePointer<CChar>?
        guard CGPDFDictionaryGetName(dict, "Subtype", &subPtr), let sub = subPtr else { return }

        switch String(cString: sub) {
        case "Image":
            var mask: CGPDFBoolean = 0
            if CGPDFDictionaryGetBoolean(dict, "ImageMask", &mask), mask != 0 { return }
            addUnitSquare(st)

        case "Form":
            guard st.depth < maxDepth, st.forms < maxForms else { return }
            st.forms += 1

            var matrix = CGAffineTransform.identity
            var arr: CGPDFArrayRef?
            if CGPDFDictionaryGetArray(dict, "Matrix", &arr), let arr, CGPDFArrayGetCount(arr) == 6 {
                var v = [CGPDFReal](repeating: 0, count: 6)
                var ok = true
                for i in 0..<6 {
                    var n: CGPDFReal = 0
                    if CGPDFArrayGetNumber(arr, i, &n) { v[i] = n } else { ok = false }
                }
                if ok { matrix = CGAffineTransform(a: v[0], b: v[1], c: v[2], d: v[3], tx: v[4], ty: v[5]) }
            }

            var own: CGPDFDictionaryRef?
            let hasOwn = CGPDFDictionaryGetDictionary(dict, "Resources", &own) && own != nil
            guard let resources = hasOwn ? own : st.resources.last else { return }

            let child: CGPDFContentStreamRef? = CGPDFContentStreamCreateWithStream(stream, resources, content)
            guard let child else { return }
            let savedCTM = st.ctm
            let savedDepth = st.saved.count
            st.ctm = matrix.concatenating(st.ctm)
            st.depth += 1
            st.resources.append(resources)
            scan(child, state: st)
            st.resources.removeLast()
            st.depth -= 1
            // 폼 안의 q/Q 짝이 틀어져도 바깥은 원래대로.
            if st.saved.count > savedDepth { st.saved.removeLast(st.saved.count - savedDepth) }
            st.ctm = savedCTM
            CGPDFContentStreamRelease(child)

        default:
            return
        }
    }
}
