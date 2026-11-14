//
//  ArgoSheetChrome.swift
//  엑셀 뷰어(ArgoSheetViewer)의 독립 UI·색상·단축키 부품 모음.
//  컨트롤러의 private 속성에 의존하지 않으며, 다크모드 색 변환·머리글·시트 탭·단축키 표·설명서를 담당한다.
//

import UIKit
import CoreText

// MARK: - 1. ArgoSheetPalette (밝은·어두운 색표 및 HSL/WCAG 대비 변환)

struct ArgoSheetPalette: Equatable {
    let isDark: Bool
    let isChartOverlay: Bool
    let textWeight: Int   // 0: 보통, 1: 굵게(기본), 2: 아주 굵게
    let minFont14: Bool   // 화면 글자 크기(font_size_pt * zoom)가 14pt 미만이면 14pt로 보정
    let zoom: CGFloat

    static let lightDefault = ArgoSheetPalette(
        isDark: false,
        isChartOverlay: false,
        textWeight: 1,
        minFont14: false,
        zoom: 1.75
    )

    func forChartOverlay() -> ArgoSheetPalette {
        ArgoSheetPalette(
            isDark: isDark,
            isChartOverlay: true,
            textWeight: textWeight,
            minFont14: minFont14,
            zoom: zoom
        )
    }

    // MARK: 크롬·바탕 색상

    /// 빈 칸 및 타일·그리드 기본 바탕색 (밝은: #FFFFFF, 다크: #1C1E22, 다크 차트: #23262B)
    var canvasBackground: UIColor {
        if !isDark { return .white }
        return isChartOverlay
            ? UIColor(red: 0x23 / 255.0, green: 0x26 / 255.0, blue: 0x2B / 255.0, alpha: 1)
            : UIColor(red: 0x1C / 255.0, green: 0x1E / 255.0, blue: 0x22 / 255.0, alpha: 1)
    }

    var barBackground: UIColor {
        isDark
            ? UIColor(red: 0x14 / 255.0, green: 0x16 / 255.0, blue: 0x19 / 255.0, alpha: 1)
            : UIColor(red: 0.15, green: 0.17, blue: 0.21, alpha: 1)
    }

    var addrBadgeBackground: UIColor {
        isDark
            ? UIColor(red: 0x05 / 255.0, green: 0x96 / 255.0, blue: 0x69 / 255.0, alpha: 1)
            : UIColor(red: 0.06, green: 0.49, blue: 0.25, alpha: 1)
    }

    var primaryText: UIColor {
        isDark
            ? UIColor(red: 0xE6 / 255.0, green: 0xE8 / 255.0, blue: 0xEB / 255.0, alpha: 1)
            : .white
    }

    var secondaryText: UIColor {
        isDark
            ? UIColor(red: 0x9C / 255.0, green: 0xA3 / 255.0, blue: 0xAF / 255.0, alpha: 1)
            : UIColor(white: 0.78, alpha: 1)
    }

    var dividerColor: UIColor {
        isDark
            ? UIColor(red: 0x4B / 255.0, green: 0x55 / 255.0, blue: 0x63 / 255.0, alpha: 1)
            : UIColor(white: 0.45, alpha: 1)
    }

    var cursorBorderColor: CGColor {
        isDark
            ? CGColor(srgbRed: 0x10 / 255.0, green: 0xB9 / 255.0, blue: 0x81 / 255.0, alpha: 1)
            : CGColor(srgbRed: 0.06, green: 0.62, blue: 0.30, alpha: 1)
    }

    /// 커서 중심 십자 강조 반투명 한 장 (밝은: 검정 0.08, 다크: 흰색 0.07)
    var crossHairColor: UIColor {
        isDark
            ? UIColor(white: 1.0, alpha: 0.07)
            : UIColor(white: 0.0, alpha: 0.08)
    }

    // 머리글 색표
    var headerBackground: UIColor {
        isDark
            ? UIColor(red: 0x2B / 255.0, green: 0x2F / 255.0, blue: 0x36 / 255.0, alpha: 1)
            : UIColor(red: 0xF3 / 255.0, green: 0xF4 / 255.0, blue: 0xF6 / 255.0, alpha: 1)
    }

    var headerText: UIColor {
        isDark
            ? UIColor(red: 0xC9 / 255.0, green: 0xCE / 255.0, blue: 0xD6 / 255.0, alpha: 1)
            : UIColor(red: 0x37 / 255.0, green: 0x41 / 255.0, blue: 0x51 / 255.0, alpha: 1)
    }

    var headerBorder: UIColor {
        isDark
            ? UIColor(red: 0x3A / 255.0, green: 0x3E / 255.0, blue: 0x45 / 255.0, alpha: 1)
            : UIColor(red: 0xD1 / 255.0, green: 0xD5 / 255.0, blue: 0xDB / 255.0, alpha: 1)
    }

    var headerActiveBackground: UIColor {
        isDark
            ? UIColor(red: 0x14 / 255.0, green: 0x53 / 255.0, blue: 0x2D / 255.0, alpha: 1)
            : UIColor(red: 0xD1 / 255.0, green: 0xFA / 255.0, blue: 0xE5 / 255.0, alpha: 1)
    }

    var headerActiveText: UIColor {
        isDark
            ? UIColor(red: 0xBB / 255.0, green: 0xF7 / 255.0, blue: 0xD0 / 255.0, alpha: 1)
            : UIColor(red: 0x06 / 255.0, green: 0x5F / 255.0, blue: 0x46 / 255.0, alpha: 1)
    }

    var headerActiveAccentBand: UIColor {
        UIColor(red: 0x10 / 255.0, green: 0xB9 / 255.0, blue: 0x81 / 255.0, alpha: 1)
    }

    // MARK: 글자 크기·굵기 보조

    func effectiveFontSize(_ rawPt: CGFloat) -> CGFloat {
        guard minFont14, zoom > 0 else { return rawPt }
        let minSheetPt = 14.0 / zoom
        return max(rawPt, minSheetPt)
    }

    /// 한글 시스템 대체 글꼴에서 굵기 지정이 무시되는 현상을 보완하기 위한 음수 획 두께(%).
    /// 엑셀 원본의 보통/굵게 차이가 그대로 남도록 원래 굵은 셀에는 획을 더 준다.
    func strokeWidthPercent(isCellBold: Bool) -> CGFloat {
        switch textWeight {
        case 0:
            return 0
        case 2:
            return isCellBold ? -4.2 : -3.0
        default: // 1 (기본)
            return isCellBold ? -2.5 : -1.5
        }
    }

    // MARK: drawCommands 전용 색 변환

    /// 채우기(Fill·Gradient·Pattern) 색 변환.
    /// - 다크모드 일반 셀: HSL 로 바꿔 색상은 두고 밝기 `L' = 0.10 + 0.30 * L`, 채도 `* 0.85`.
    ///   단, 타일 기본 흰색 배경 채우기(#FFFFFF)는 `#1C1E22`로 맞춘다.
    /// - 다크모드 차트 오버레이: 거의 흰색(`L > 0.92`)인 배경만 `#23262B`로 바꾸고 차트 계열 색은 그대로 둔다.
    func fillColor(_ c: ASColor, alphaOverride: CGFloat? = nil) -> CGColor {
        let a = alphaOverride ?? (CGFloat(c.a) / 255.0)
        let r = CGFloat(c.r) / 255.0
        let g = CGFloat(c.g) / 255.0
        let b = CGFloat(c.b) / 255.0
        guard isDark else {
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        let (h, s, l) = Self.rgbToHsl(r, g, b)
        if isChartOverlay {
            if l > 0.92 {
                return CGColor(srgbRed: 0x23 / 255.0, green: 0x26 / 255.0, blue: 0x2B / 255.0, alpha: a)
            }
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        // 순수 흰색 바탕 칠하기 명령은 #1C1E22 로 통일
        if c.r == 255 && c.g == 255 && c.b == 255 {
            return CGColor(srgbRed: 0x1C / 255.0, green: 0x1E / 255.0, blue: 0x22 / 255.0, alpha: a)
        }

        let newL = 0.10 + 0.30 * l
        let newS = s * 0.85
        let (nr, ng, nb) = Self.hslToRgb(h, newS, newL)
        return CGColor(srgbRed: nr, green: ng, blue: nb, alpha: a)
    }

    /// 글자(Text) 색 변환.
    /// - 다크모드 차트: 거의 검정(`L < 0.25`)은 `#D0D4DA`, 나머지는 그대로.
    /// - 다크모드 일반 셀: 어두운 바탕(`#1C1E22`)과 WCAG 명도 대비 4.5:1 이상이 될 때까지 색상(H)은 유지한 채 밝기(L)만 올린다.
    ///   검정 글자는 `#E6E8EB` 부근이 된다.
    func textColor(_ c: ASColor) -> CGColor {
        let a = CGFloat(c.a) / 255.0
        let r = CGFloat(c.r) / 255.0
        let g = CGFloat(c.g) / 255.0
        let b = CGFloat(c.b) / 255.0
        guard isDark else {
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        let (h, s, l) = Self.rgbToHsl(r, g, b)
        if isChartOverlay {
            if l < 0.25 {
                return CGColor(srgbRed: 0xD0 / 255.0, green: 0xD4 / 255.0, blue: 0xDA / 255.0, alpha: a)
            }
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        // 무채색 어두운 글자(검정 등)는 #E6E8EB 로 즉시 매핑
        if s < 0.08 && l < 0.20 {
            return CGColor(srgbRed: 0xE6 / 255.0, green: 0xE8 / 255.0, blue: 0xEB / 255.0, alpha: a)
        }

        // 배경(#1C1E22, 상대 휘도 약 0.0116) 대비 4.5:1 이상이 되도록 밝기(L)만 단계적으로 끌어올린다.
        let bgLum: CGFloat = 0.0116
        var curL = l
        var (nr, ng, nb) = (r, g, b)
        var guardStep = 0
        while Self.contrastRatio(Self.relativeLuminance(nr, ng, nb), bgLum) < 4.5 && curL < 0.94 && guardStep < 24 {
            curL = min(0.94, curL + 0.05)
            (nr, ng, nb) = Self.hslToRgb(h, s, curL)
            guardStep += 1
        }
        return CGColor(srgbRed: nr, green: ng, blue: nb, alpha: a)
    }

    /// 격자선 및 테두리(Line) 색 변환.
    /// - 다크모드 격자선: `#3A3E45`
    /// - 다크모드 테두리: `L' = 1.0 - 0.7 * L` (검정 테두리 -> 밝은 회색)
    /// - 다크모드 차트: 거의 검정(`L < 0.25`)인 축선은 `#D0D4DA`, 밝은 눈금선은 `#3A3E45`, 차트 계열 선은 그대로.
    func lineColor(_ c: ASColor, isGrid: Bool) -> CGColor {
        let a = CGFloat(c.a) / 255.0
        let r = CGFloat(c.r) / 255.0
        let g = CGFloat(c.g) / 255.0
        let b = CGFloat(c.b) / 255.0
        guard isDark else {
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        if isGrid {
            return CGColor(srgbRed: 0x3A / 255.0, green: 0x3E / 255.0, blue: 0x45 / 255.0, alpha: a)
        }

        let (h, s, l) = Self.rgbToHsl(r, g, b)
        if isChartOverlay {
            if s < 0.12 {
                if l < 0.25 {
                    return CGColor(srgbRed: 0xD0 / 255.0, green: 0xD4 / 255.0, blue: 0xDA / 255.0, alpha: a)
                } else if l > 0.80 {
                    return CGColor(srgbRed: 0x3A / 255.0, green: 0x3E / 255.0, blue: 0x45 / 255.0, alpha: a)
                }
            }
            return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }

        let newL = (1.0 - 0.7 * l).clamp(0.0, 1.0)
        let (nr, ng, nb) = Self.hslToRgb(h, s, newL)
        return CGColor(srgbRed: nr, green: ng, blue: nb, alpha: a)
    }

    // MARK: 색 공간 수학 헬퍼

    static func rgbToHsl(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> (CGFloat, CGFloat, CGFloat) {
        let maxV = max(r, max(g, b))
        let minV = min(r, min(g, b))
        let l = (maxV + minV) * 0.5
        if abs(maxV - minV) < 1e-6 {
            return (0, 0, l)
        }
        let d = maxV - minV
        let s = l > 0.5 ? d / (2.0 - maxV - minV) : d / (maxV + minV)
        var h: CGFloat
        if maxV == r {
            h = (g - b) / d + (g < b ? 6.0 : 0.0)
        } else if maxV == g {
            h = (b - r) / d + 2.0
        } else {
            h = (r - g) / d + 4.0
        }
        h /= 6.0
        return (h, s, l)
    }

    static func hslToRgb(_ h: CGFloat, _ s: CGFloat, _ l: CGFloat) -> (CGFloat, CGFloat, CGFloat) {
        if s < 1e-6 {
            return (l, l, l)
        }
        let q = l < 0.5 ? l * (1.0 + s) : l + s - l * s
        let p = 2.0 * l - q
        func hue2rgb(_ tIn: CGFloat) -> CGFloat {
            var t = tIn
            if t < 0 { t += 1 }
            if t > 1 { t -= 1 }
            if t < 1.0 / 6.0 { return p + (q - p) * 6.0 * t }
            if t < 0.5 { return q }
            if t < 2.0 / 3.0 { return p + (q - p) * (2.0 / 3.0 - t) * 6.0 }
            return p
        }
        return (hue2rgb(h + 1.0 / 3.0), hue2rgb(h), hue2rgb(h - 1.0 / 3.0))
    }

    static func relativeLuminance(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> CGFloat {
        func lin(_ c: CGFloat) -> CGFloat {
            c <= 0.03928 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    static func contrastRatio(_ l1: CGFloat, _ l2: CGFloat) -> CGFloat {
        let hi = max(l1, l2)
        let lo = min(l1, l2)
        return (hi + 0.05) / (lo + 0.05)
    }
}

private extension CGFloat {
    func clamp(_ lo: CGFloat, _ hi: CGFloat) -> CGFloat {
        Swift.min(Swift.max(self, lo), hi)
    }
}

// MARK: - 2. ArgoSheetHeaderView (행·열 머리글 및 좌상단 모서리 주소 칸)

final class ArgoSheetHeaderView: UIView {
    enum Mode {
        case corner
        case columns
        case rows
    }

    let mode: Mode
    var palette: ArgoSheetPalette = .lightDefault
    var zoom: CGFloat = 1.75
    var offsets: [CGFloat] = [0]          // 시트 pt 누적 배열 (길이 = 행/열 개수 + 1)
    var freezeCount: Int = 0
    var freezeSpanScreen: CGFloat = 0     // 화면 pt 기준 고정 영역 너비/높이
    var freezeOriginPt: CGFloat = 0       // 시트 pt 기준 고정 영역 끝 좌표
    var scrollOffsetScreen: CGFloat = 0   // 본문 스크롤 위치(화면 pt)
    var activeRange: ClosedRange<Int> = 1...1
    var cornerText: String = "A1"

    init(mode: Mode) {
        self.mode = mode
        super.init(frame: .zero)
        isOpaque = true
        contentMode = .redraw
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    /// 글자 크기(15 * zoom, 18~30pt 제한)와 열 머리글 높이, 행 머리글 너비를 산출한다.
    static func metrics(zoom: CGFloat, maxRow: Int) -> (fontSize: CGFloat, colHeaderH: CGFloat, rowHeaderW: CGFloat) {
        let fontSize = (15.0 * zoom).clamp(18.0, 30.0)
        let colH = round(fontSize * 1.6)
        let digits = max(3, String(max(1, maxRow)).count)
        let charW = round(fontSize * 0.64)
        let rowW = max(68.0, CGFloat(digits) * charW + 26.0)
        return (fontSize, colH, rowW)
    }

    /// 1-based 열 번호 -> 엑셀 열 이름 ("A", "Z", "AA")
    static func columnLetters(for col1Based: Int) -> String {
        guard col1Based >= 1 else { return "A" }
        var n = col1Based
        var chars: [Character] = []
        while n > 0 {
            let rem = (n - 1) % 26
            if let scalar = UnicodeScalar(65 + rem) {
                chars.append(Character(scalar))
            }
            n = (n - 1) / 26
        }
        return String(chars.reversed())
    }

    /// 시트 누적 pt 배열에서 `targetPt`가 속한 1-based 행·열 번호를 이분 탐색한다.
    static func binarySearchIndex(offsets: [CGFloat], targetPt: CGFloat) -> Int {
        let count = offsets.count - 1
        guard count >= 1 else { return 1 }
        var lo = 1
        var hi = count
        while lo < hi {
            let mid = (lo + hi) / 2
            if offsets[mid] <= targetPt {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        return min(max(1, lo), count)
    }

    override func draw(_ rect: CGRect) {
        guard let ctx = UIGraphicsGetCurrentContext() else { return }
        let B = bounds
        guard B.width > 0, B.height > 0 else { return }

        let (fontSize, _, _) = Self.metrics(zoom: zoom, maxRow: max(1, offsets.count - 1))

        // 바탕 칠하기
        ctx.setFillColor(palette.headerBackground.cgColor)
        ctx.fill(B)

        switch mode {
        case .corner:
            drawCorner(ctx: ctx, bounds: B, fontSize: fontSize)
        case .columns:
            drawAxisCells(ctx: ctx, bounds: B, fontSize: fontSize, isColumns: true)
        case .rows:
            drawAxisCells(ctx: ctx, bounds: B, fontSize: fontSize, isColumns: false)
        }
    }

    private func drawCorner(ctx: CGContext, bounds: CGRect, fontSize: CGFloat) {
        let fitFont = UIFont.monospacedSystemFont(ofSize: max(16, fontSize * 0.88), weight: .bold)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: fitFont,
            .foregroundColor: palette.headerText,
        ]
        let ns = cornerText as NSString
        let sz = ns.size(withAttributes: attrs)
        let pt = CGPoint(x: bounds.midX - sz.width * 0.5, y: bounds.midY - sz.height * 0.5)
        ns.draw(at: pt, withAttributes: attrs)

        // 오른쪽·아래 경계선
        ctx.setStrokeColor(palette.headerBorder.cgColor)
        ctx.setLineWidth(1.0)
        ctx.move(to: CGPoint(x: bounds.maxX - 0.5, y: 0))
        ctx.addLine(to: CGPoint(x: bounds.maxX - 0.5, y: bounds.maxY))
        ctx.move(to: CGPoint(x: 0, y: bounds.maxY - 0.5))
        ctx.addLine(to: CGPoint(x: bounds.maxX, y: bounds.maxY - 0.5))
        ctx.strokePath()
    }

    /// 보이는 범위만 이분 탐색으로 찾아 그린다(열 16,384개를 뷰로 만들지 않는다).
    private func drawAxisCells(ctx: CGContext, bounds: CGRect, fontSize: CGFloat, isColumns: Bool) {
        let totalCount = offsets.count - 1
        guard totalCount >= 1, zoom > 0 else { return }

        let normalFont = UIFont.systemFont(ofSize: fontSize, weight: .semibold)
        let activeFont = UIFont.systemFont(ofSize: fontSize, weight: .bold)
        let spanLimit = isColumns ? bounds.width : bounds.height

        // 1. 고정 구간(1 ..< freezeCount+1) 그리기
        if freezeCount > 0 && freezeSpanScreen > 0 {
            ctx.saveGState()
            let freezeClip = isColumns
                ? CGRect(x: 0, y: 0, width: freezeSpanScreen, height: bounds.height)
                : CGRect(x: 0, y: 0, width: bounds.width, height: freezeSpanScreen)
            ctx.clip(to: freezeClip)

            let endF = min(freezeCount, totalCount)
            for idx in 1...endF {
                let startScreen = offsets[idx - 1] * zoom
                let endScreen = offsets[idx] * zoom
                if startScreen > freezeSpanScreen { break }
                drawSingleHeaderCell(
                    ctx: ctx,
                    index: idx,
                    startPos: startScreen,
                    endPos: endScreen,
                    bounds: bounds,
                    isColumns: isColumns,
                    normalFont: normalFont,
                    activeFont: activeFont
                )
            }
            ctx.restoreGState()
        }

        // 2. 스크롤되는 본문 구간(이분 탐색으로 시작·끝 인덱스만 추출)
        ctx.saveGState()
        let bodyClip = isColumns
            ? CGRect(x: freezeSpanScreen, y: 0, width: max(0, bounds.width - freezeSpanScreen), height: bounds.height)
            : CGRect(x: 0, y: freezeSpanScreen, width: bounds.width, height: max(0, bounds.height - freezeSpanScreen))
        ctx.clip(to: bodyClip)

        let startSheetPt = freezeOriginPt + scrollOffsetScreen / zoom
        let endSheetPt = freezeOriginPt + ( (spanLimit - freezeSpanScreen) + scrollOffsetScreen ) / zoom

        let firstIdx = max(freezeCount + 1, Self.binarySearchIndex(offsets: offsets, targetPt: startSheetPt) - 1)
        let lastIdx = min(totalCount, Self.binarySearchIndex(offsets: offsets, targetPt: endSheetPt) + 1)

        if firstIdx <= lastIdx {
            for idx in firstIdx...lastIdx {
                let startScreen = freezeSpanScreen + (offsets[idx - 1] - freezeOriginPt) * zoom - scrollOffsetScreen
                let endScreen = freezeSpanScreen + (offsets[idx] - freezeOriginPt) * zoom - scrollOffsetScreen
                if endScreen < freezeSpanScreen - 2 { continue }
                if startScreen > spanLimit + 2 { break }
                drawSingleHeaderCell(
                    ctx: ctx,
                    index: idx,
                    startPos: startScreen,
                    endPos: endScreen,
                    bounds: bounds,
                    isColumns: isColumns,
                    normalFont: normalFont,
                    activeFont: activeFont
                )
            }
        }
        ctx.restoreGState()

        // 바깥쪽 가장자리 경계선
        ctx.setStrokeColor(palette.headerBorder.cgColor)
        ctx.setLineWidth(1.0)
        if isColumns {
            ctx.move(to: CGPoint(x: 0, y: bounds.maxY - 0.5))
            ctx.addLine(to: CGPoint(x: bounds.maxX, y: bounds.maxY - 0.5))
        } else {
            ctx.move(to: CGPoint(x: bounds.maxX - 0.5, y: 0))
            ctx.addLine(to: CGPoint(x: bounds.maxX - 0.5, y: bounds.maxY))
        }
        ctx.strokePath()
    }

    private func drawSingleHeaderCell(
        ctx: CGContext,
        index: Int,
        startPos: CGFloat,
        endPos: CGFloat,
        bounds: CGRect,
        isColumns: Bool,
        normalFont: UIFont,
        activeFont: UIFont
    ) {
        let span = endPos - startPos
        // 숨긴 행·열(높이·너비 0)은 경계에 2pt 이중선을 그려 번호가 건너뛴 것이 보이게 한다.
        if span <= 0.2 {
            ctx.setStrokeColor(palette.headerActiveAccentBand.cgColor)
            ctx.setLineWidth(1.2)
            for delta in [-1.5, 1.5] as [CGFloat] {
                if isColumns {
                    ctx.move(to: CGPoint(x: startPos + delta, y: 2))
                    ctx.addLine(to: CGPoint(x: startPos + delta, y: bounds.height - 2))
                } else {
                    ctx.move(to: CGPoint(x: 2, y: startPos + delta))
                    ctx.addLine(to: CGPoint(x: bounds.width - 2, y: startPos + delta))
                }
            }
            ctx.strokePath()
            return
        }

        let cellRect = isColumns
            ? CGRect(x: startPos, y: 0, width: span, height: bounds.height)
            : CGRect(x: 0, y: startPos, width: bounds.width, height: span)

        let isActive = activeRange.contains(index)
        if isActive {
            ctx.setFillColor(palette.headerActiveBackground.cgColor)
            ctx.fill(cellRect)

            // 안쪽 가장자리 3pt 녹색 띠 (#10B981)
            ctx.setFillColor(palette.headerActiveAccentBand.cgColor)
            let bandRect = isColumns
                ? CGRect(x: cellRect.minX, y: cellRect.maxY - 3, width: cellRect.width, height: 3)
                : CGRect(x: cellRect.maxX - 3, y: cellRect.minY, width: 3, height: cellRect.height)
            ctx.fill(bandRect)
        }

        // 칸 경계선
        ctx.setStrokeColor(palette.headerBorder.cgColor)
        ctx.setLineWidth(1.0)
        if isColumns {
            ctx.move(to: CGPoint(x: cellRect.maxX - 0.5, y: 0))
            ctx.addLine(to: CGPoint(x: cellRect.maxX - 0.5, y: cellRect.maxY))
        } else {
            ctx.move(to: CGPoint(x: 0, y: cellRect.maxY - 0.5))
            ctx.addLine(to: CGPoint(x: cellRect.maxX, y: cellRect.maxY - 0.5))
        }
        ctx.strokePath()

        // 텍스트 출력
        let label = isColumns ? Self.columnLetters(for: index) : "\(index)"
        let font = isActive ? activeFont : normalFont
        let textColor = isActive ? palette.headerActiveText : palette.headerText
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: textColor,
        ]
        let ns = label as NSString
        let sz = ns.size(withAttributes: attrs)
        if sz.width <= cellRect.width + 2 && sz.height <= cellRect.height + 2 {
            let pt = CGPoint(x: cellRect.midX - sz.width * 0.5, y: cellRect.midY - sz.height * 0.5)
            ns.draw(at: pt, withAttributes: attrs)
        }
    }
}

// MARK: - 3. ArgoSheetTabStrip (하단 시트 탭 줄 — 포커스 받지 않는 그림 뷰)

final class ArgoSheetTabStrip: UIView {
    struct TabItem: Equatable {
        let index: Int
        let name: String
        let tabColor: UIColor?
    }

    private var tabs: [TabItem] = []
    private var selectedIndex: Int = 0
    private var palette: ArgoSheetPalette = .lightDefault

    override var canBecomeFocused: Bool { false }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        backgroundColor = .clear
        contentMode = .redraw
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    func update(tabs: [TabItem], selectedIndex: Int, palette: ArgoSheetPalette) {
        self.tabs = tabs
        self.selectedIndex = selectedIndex
        self.palette = palette
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        guard let ctx = UIGraphicsGetCurrentContext(), !tabs.isEmpty else { return }
        let B = bounds
        guard B.width > 0, B.height > 0 else { return }

        let activeFont = UIFont.systemFont(ofSize: 26, weight: .bold)
        let normalFont = UIFont.systemFont(ofSize: 22, weight: .medium)
        let spacing: CGFloat = 10
        let padH: CGFloat = 22

        // 1. 각 탭의 폭과 누적 X 좌표 계산
        var itemRects: [CGRect] = []
        var curX: CGFloat = 0
        var selectedRect: CGRect = .zero

        for item in tabs {
            let isSel = (item.index == selectedIndex)
            let font = isSel ? activeFont : normalFont
            let sz = (item.name as NSString).size(withAttributes: [.font: font])
            let w = max(84, ceil(sz.width) + padH * 2)
            let r = CGRect(x: curX, y: 2, width: w, height: B.height - 4)
            itemRects.append(r)
            if isSel { selectedRect = r }
            curX += w + spacing
        }

        // 2. 탭이 많아 넘치면 현재 선택된 탭이 항상 화면 안에 보이도록 가로 오프셋을 조정한다.
        var shiftX: CGFloat = 0
        let totalW = max(0, curX - spacing)
        if totalW > B.width {
            let targetCenter = selectedRect.midX - B.width * 0.5
            shiftX = targetCenter.clamp(0, totalW - B.width)
        }

        ctx.saveGState()
        ctx.clip(to: B)

        let defaultAccent = UIColor(red: 0x10 / 255.0, green: 0xB9 / 255.0, blue: 0x81 / 255.0, alpha: 1)

        for (i, item) in tabs.enumerated() {
            let r = itemRects[i].offsetBy(dx: -shiftX, dy: 0)
            if r.maxX < 0 || r.minX > B.width { continue }

            let isSel = (item.index == selectedIndex)
            let path = UIBezierPath(roundedRect: r, cornerRadius: 8)

            // 탭 바탕색: 지금 시트는 흰 바탕(다크: #2E3238), 다른 탭은 막대 색보다 조금 밝은 바탕
            if isSel {
                let bg = palette.isDark
                    ? UIColor(red: 0x2E / 255.0, green: 0x32 / 255.0, blue: 0x38 / 255.0, alpha: 1)
                    : .white
                ctx.setFillColor(bg.cgColor)
            } else {
                let bg = palette.isDark
                    ? UIColor(red: 0x20 / 255.0, green: 0x23 / 255.0, blue: 0x29 / 255.0, alpha: 1)
                    : UIColor(red: 0.22, green: 0.25, blue: 0.30, alpha: 1)
                ctx.setFillColor(bg.cgColor)
            }
            ctx.addPath(path.cgPath)
            ctx.fillPath()

            // 상단 또는 하단 컬러 띠
            ctx.saveGState()
            ctx.addPath(path.cgPath)
            ctx.clip()
            if isSel {
                // 지금 시트 탭: 위쪽 5pt 띠 (엑셀 탭 색이 있으면 그 색, 없으면 녹색 #10B981)
                let bandColor = item.tabColor ?? defaultAccent
                ctx.setFillColor(bandColor.cgColor)
                ctx.fill(CGRect(x: r.minX, y: r.minY, width: r.width, height: 5))
            } else if let tc = item.tabColor {
                // 다른 탭에 엑셀 탭 색이 있으면 아래쪽 4pt 띠로 표시
                ctx.setFillColor(tc.cgColor)
                ctx.fill(CGRect(x: r.minX, y: r.maxY - 4, width: r.width, height: 4))
            }
            ctx.restoreGState()

            // 탭 이름 텍스트
            let font = isSel ? activeFont : normalFont
            let textColor: UIColor = isSel
                ? (palette.isDark ? .white : UIColor(red: 0.08, green: 0.10, blue: 0.13, alpha: 1))
                : UIColor(white: 0.78, alpha: 1)
            let attrs: [NSAttributedString.Key: Any] = [
                .font: font,
                .foregroundColor: textColor,
            ]
            let ns = item.name as NSString
            let sz = ns.size(withAttributes: attrs)
            let ty = r.midY - sz.height * 0.5 + (isSel ? 1.5 : 0)
            ns.draw(at: CGPoint(x: r.midX - sz.width * 0.5, y: ty), withAttributes: attrs)
        }

        ctx.restoreGState()
    }
}

// MARK: - 4. ArgoSheetKeyMap (키보드 단축키 판정 및 설명서 단일 표)

enum ArgoSheetKeyAction: Equatable {
    case moveUp, moveDown, moveLeft, moveRight
    case jumpEdgeUp, jumpEdgeDown, jumpEdgeLeft, jumpEdgeRight
    case pageUp, pageDown, pageLeft, pageRight
    case rowStart, sheetStart, sheetEnd
    case prevSheet, nextSheet
    case goToCell(String)
    case zoomIn, zoomOut, zoom100
    case findPrompt, findNext, findPrev
    case quickMenu
    case toggleDarkMode, toggleHeaders, toggleCross
    case showHelp
    case closeOrEscape
}

enum ArgoSheetKeyMap {
    struct HelpRow {
        let actionTitle: String
        let keysText: String
    }

    /// 단축키 설명서 왼쪽 칸(리모컨)
    static var remoteEntries: [HelpRow] {
        [
            HelpRow(actionTitle: L("한 칸 이동 (누르고 있으면 가속)"), keysText: L("방향키 ←↑→↓")),
            HelpRow(actionTitle: L("셀 정보 · 퀵 메뉴"), keysText: L("선택(가운데)")),
            HelpRow(actionTitle: L("다음 시트 전환"), keysText: L("재생 / 일시정지")),
            HelpRow(actionTitle: L("닫기"), keysText: L("Menu")),
        ]
    }

    /// 단축키 설명서 오른쪽 칸(키보드) — 아래 `match(key:)`가 실제로 처리하는 키와 100% 동일한 표
    static var keyboardEntries: [HelpRow] {
        [
            HelpRow(actionTitle: L("한 칸 이동"), keysText: "← ↑ → ↓"),
            HelpRow(actionTitle: L("데이터 끝으로 이동"), keysText: "⌃/⌘ + ←↑→↓"),
            HelpRow(actionTitle: L("한 화면 위 · 아래"), keysText: "PageUp/Down, ⌥↑/↓, ⇧Space/Space"),
            HelpRow(actionTitle: L("한 화면 왼쪽 · 오른쪽"), keysText: "⌥PageUp/Down, ⌥←/→"),
            HelpRow(actionTitle: L("행의 처음"), keysText: "Home, ⌘⌥←"),
            HelpRow(actionTitle: L("A1 / 마지막 칸"), keysText: "⌃Home·⌘⌥↑ / ⌃End·⌘⌥↓"),
            HelpRow(actionTitle: L("이전 · 다음 시트"), keysText: "⌃PageUp/Down, [ / ], ⇧Tab/Tab"),
            HelpRow(actionTitle: L("셀 주소 입력 이동"), keysText: "1~9, ⌃G, ⌘L"),
            HelpRow(actionTitle: L("확대 / 축소 / 100%"), keysText: "+ = ⌘+ / - ⌘- / 0"),
            HelpRow(actionTitle: L("찾기 / 다음·이전 결과"), keysText: "⌃F·⌘F / F3·⌘G, ⇧F3·⌘⇧G"),
            HelpRow(actionTitle: L("셀 정보 · 퀵 메뉴"), keysText: "Return, M"),
            HelpRow(actionTitle: L("다크모드 / 머리글 / 십자"), keysText: "D / H / X"),
            HelpRow(actionTitle: L("단축키 설명서 / 닫기"), keysText: "? / F1 / Esc"),
        ]
    }

    /// UIKey 입력을 분석해 동작(`ArgoSheetKeyAction`)으로 변환한다.
    static func match(key: UIKey) -> ArgoSheetKeyAction? {
        let mods = key.modifierFlags
        let hasCmd = mods.contains(.command)
        let hasCtrl = mods.contains(.control)
        let hasOpt = mods.contains(.alternate)
        let hasShift = mods.contains(.shift)
        let code = key.keyCode
        let chars = key.charactersIgnoringModifiers

        // 1. 화살표 및 조합
        switch code {
        case .keyboardUpArrow:
            if hasCmd && hasOpt { return .sheetStart }
            if hasCmd || hasCtrl { return .jumpEdgeUp }
            if hasOpt { return .pageUp }
            return .moveUp
        case .keyboardDownArrow:
            if hasCmd && hasOpt { return .sheetEnd }
            if hasCmd || hasCtrl { return .jumpEdgeDown }
            if hasOpt { return .pageDown }
            return .moveDown
        case .keyboardLeftArrow:
            if hasCmd && hasOpt { return .rowStart }
            if hasCmd || hasCtrl { return .jumpEdgeLeft }
            if hasOpt { return .pageLeft }
            return .moveLeft
        case .keyboardRightArrow:
            if hasCmd || hasCtrl { return .jumpEdgeRight }
            if hasOpt { return .pageRight }
            return .moveRight
        default:
            break
        }

        // 2. PageUp / PageDown / Home / End / Space / Tab / Esc / Return / F1 / F3
        switch code {
        case .keyboardPageUp:
            if hasCtrl { return .prevSheet }
            if hasOpt { return .pageLeft }
            return .pageUp
        case .keyboardPageDown:
            if hasCtrl { return .nextSheet }
            if hasOpt { return .pageRight }
            return .pageDown
        case .keyboardHome:
            if hasCtrl || hasCmd { return .sheetStart }
            return .rowStart
        case .keyboardEnd:
            if hasCtrl || hasCmd { return .sheetEnd }
            return nil
        case .keyboardSpacebar:
            return hasShift ? .pageUp : .pageDown
        case .keyboardTab:
            return hasShift ? .prevSheet : .nextSheet
        case .keyboardReturnOrEnter, .keypadEnter:
            return .quickMenu
        case .keyboardEscape:
            return .closeOrEscape
        case .keyboardF1:
            return .showHelp
        case .keyboardF3:
            return hasShift ? .findPrev : .findNext
        default:
            break
        }

        // 3. Cmd / Ctrl 조합 단축키 (F, G, L, +, -)
        if hasCmd || hasCtrl {
            switch code {
            case .keyboardF:
                return .findPrompt
            case .keyboardG:
                if hasCtrl && !hasCmd { return .goToCell("") }
                return hasShift ? .findPrev : .findNext
            case .keyboardL:
                if hasCmd { return .goToCell("") }
            case .keyboardEqualSign, .keypadPlus:
                return .zoomIn
            case .keyboardHyphen, .keypadHyphen:
                return .zoomOut
            case .keyboard0, .keypad0:
                return .zoom100
            default:
                break
            }
            if chars == "+" || chars == "=" { return .zoomIn }
            if chars == "-" { return .zoomOut }
            return nil
        }

        // 4. 단독 키 (숫자 1~9, 0, +, =, -, [, ], M, D, H, X, ?, /)
        // ⇧+숫자는 기호(!@#…)라 주소 입력으로 받지 않는다.
        if hasShift, [UIKeyboardHIDUsage.keyboard1, .keyboard2, .keyboard3, .keyboard4, .keyboard5,
                      .keyboard6, .keyboard7, .keyboard8, .keyboard9, .keyboard0].contains(code) {
            return nil
        }
        switch code {
        case .keyboard1, .keypad1: return .goToCell("1")
        case .keyboard2, .keypad2: return .goToCell("2")
        case .keyboard3, .keypad3: return .goToCell("3")
        case .keyboard4, .keypad4: return .goToCell("4")
        case .keyboard5, .keypad5: return .goToCell("5")
        case .keyboard6, .keypad6: return .goToCell("6")
        case .keyboard7, .keypad7: return .goToCell("7")
        case .keyboard8, .keypad8: return .goToCell("8")
        case .keyboard9, .keypad9: return .goToCell("9")
        case .keyboard0, .keypad0: return .zoom100
        case .keyboardEqualSign, .keypadPlus: return .zoomIn
        case .keyboardHyphen, .keypadHyphen: return .zoomOut
        case .keyboardOpenBracket: return .prevSheet
        case .keyboardCloseBracket: return .nextSheet
        case .keyboardM: return .quickMenu
        case .keyboardD: return .toggleDarkMode
        case .keyboardH: return .toggleHeaders
        case .keyboardX: return .toggleCross
        case .keyboardSlash: return .showHelp // '/' 와 '?'(Shift+/) 모두 포함
        default:
            break
        }

        if chars == "?" || chars == "/" { return .showHelp }
        if chars == "[" { return .prevSheet }
        if chars == "]" { return .nextSheet }
        if chars == "+" || chars == "=" { return .zoomIn }
        if chars == "-" { return .zoomOut }
        return nil
    }
}

// MARK: - 5. ArgoSheetHelpController (단축키 설명서 — overFullScreen 모달)

final class ArgoSheetHelpController: UIViewController {
    private let palette: ArgoSheetPalette
    /// 포커스를 받는 투명 판. 이 화면에 포커스 대상이 없으면 입력이 아래 엑셀 화면으로 가서
    /// 메뉴 버튼이 설명서가 아니라 뷰어 전체를 닫는다.
    private let catcher = ArgoFocusCanvas()

    init(palette: ArgoSheetPalette) {
        self.palette = palette
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .overFullScreen
        modalTransitionStyle = .crossDissolve
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(white: 0, alpha: 0.86)

        catcher.frame = view.bounds
        catcher.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        catcher.backgroundColor = .clear
        catcher.onSelect = { [weak self] in self?.dismiss(animated: true) }
        catcher.onPress = { [weak self] _ in self?.dismiss(animated: true); return true }
        catcher.onKey = { [weak self] _ in self?.dismiss(animated: true); return true }
        view.addSubview(catcher)

        let card = UIView()
        card.backgroundColor = UIColor(red: 0.13, green: 0.15, blue: 0.19, alpha: 0.98)
        card.layer.cornerRadius = 22
        card.layer.borderColor = UIColor(white: 0.32, alpha: 1).cgColor
        card.layer.borderWidth = 1.5
        card.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(card)

        let titleLabel = UILabel()
        titleLabel.text = L("단축키 설명서")
        titleLabel.font = .systemFont(ofSize: 34, weight: .bold)
        titleLabel.textColor = .white
        titleLabel.translatesAutoresizingMaskIntoConstraints = false

        let hintLabel = UILabel()
        hintLabel.text = L("리모컨 Menu · Esc · 아무 키나 누르면 닫힙니다")
        hintLabel.font = .systemFont(ofSize: 24, weight: .medium)
        hintLabel.textColor = .systemYellow
        hintLabel.textAlignment = .right
        hintLabel.translatesAutoresizingMaskIntoConstraints = false

        card.addSubview(titleLabel)
        card.addSubview(hintLabel)

        // 왼쪽 칸: 리모컨 + 키보드 전반부 / 오른쪽 칸: 키보드 후반부 (2단 분할로 1080p 화면 넘침 방지)
        let kbAll = ArgoSheetKeyMap.keyboardEntries
        let splitIdx = (kbAll.count + 1) / 2
        let kbLeft = Array(kbAll.prefix(splitIdx))
        let kbRight = Array(kbAll.suffix(from: splitIdx))

        let leftStack = buildColumnStack(
            sections: [
                (L("리모컨"), ArgoSheetKeyMap.remoteEntries),
                (L("키보드 (이동 · 시트)"), kbLeft),
            ]
        )
        let rightStack = buildColumnStack(
            sections: [
                (L("키보드 (검색 · 보기 · 메뉴)"), kbRight),
            ]
        )

        card.addSubview(leftStack)
        card.addSubview(rightStack)

        NSLayoutConstraint.activate([
            card.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            card.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            card.widthAnchor.constraint(equalToConstant: 1740),
            card.heightAnchor.constraint(equalToConstant: 940),

            titleLabel.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 48),
            titleLabel.topAnchor.constraint(equalTo: card.topAnchor, constant: 32),

            hintLabel.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -48),
            hintLabel.centerYAnchor.constraint(equalTo: titleLabel.centerYAnchor),

            leftStack.leadingAnchor.constraint(equalTo: card.leadingAnchor, constant: 48),
            leftStack.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 28),
            leftStack.widthAnchor.constraint(equalToConstant: 790),

            rightStack.trailingAnchor.constraint(equalTo: card.trailingAnchor, constant: -48),
            rightStack.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 28),
            rightStack.widthAnchor.constraint(equalToConstant: 790),
        ])
    }

    private func buildColumnStack(sections: [(String, [ArgoSheetKeyMap.HelpRow])]) -> UIStackView {
        let col = UIStackView()
        col.axis = .vertical
        col.spacing = 14
        col.translatesAutoresizingMaskIntoConstraints = false

        for (secTitle, rows) in sections {
            let hdr = UILabel()
            hdr.text = secTitle
            hdr.font = .systemFont(ofSize: 28, weight: .bold)
            hdr.textColor = UIColor(red: 0x34 / 255.0, green: 0xD3 / 255.0, blue: 0x99 / 255.0, alpha: 1)
            col.addArrangedSubview(hdr)

            for r in rows {
                let rowView = UIStackView()
                rowView.axis = .horizontal
                rowView.alignment = .center
                rowView.distribution = .fill
                rowView.spacing = 12

                let actLbl = UILabel()
                actLbl.text = r.actionTitle
                actLbl.font = .systemFont(ofSize: 26, weight: .regular)
                actLbl.textColor = UIColor(white: 0.92, alpha: 1)
                actLbl.setContentHuggingPriority(.defaultLow, for: .horizontal)

                let keyLbl = UILabel()
                keyLbl.text = r.keysText
                keyLbl.font = .monospacedSystemFont(ofSize: 26, weight: .semibold)
                keyLbl.textColor = .white
                keyLbl.textAlignment = .right
                keyLbl.setContentHuggingPriority(.required, for: .horizontal)

                rowView.addArrangedSubview(actLbl)
                rowView.addArrangedSubview(keyLbl)
                col.addArrangedSubview(rowView)
            }

            let spacer = UIView()
            spacer.translatesAutoresizingMaskIntoConstraints = false
            spacer.heightAnchor.constraint(equalToConstant: 8).isActive = true
            col.addArrangedSubview(spacer)
        }
        return col
    }

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [catcher] }

    /// 리모컨 Menu · 선택 · Esc · 아무 키로 즉시 닫힌다(catcher 가 못 받았을 때의 예비).
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        dismiss(animated: true)
    }
}

