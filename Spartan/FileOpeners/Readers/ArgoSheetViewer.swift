//
//  ArgoSheetViewer.swift
//  엑셀(.xlsx) 셀 단위 뷰어 — Rust argosheet 가 "무엇을 어디에" 그릴지 넘기고,
//  여기서는 그 목록을 순서대로 CoreGraphics 로 그린다. 보기 전용.
//
//  처음 안은 제미나이(P2)가 썼고, 검수에서 다음을 고쳐 다시 짰다.
//  - 컴파일 오류(`private varlastMoveTime`), ArgoFocusCanvas 중복 선언(기존 것을 쓴다)
//  - 책 포인터를 타일 작업이 쓰는 도중 as_close 로 풀어 버리는 문제(해제 후 사용)
//    → ArgoSheetBook 이 참조 횟수로 붙들고, 마지막 참조가 사라질 때만 닫는다.
//  - 타일을 시트 256pt × 확대율 × 화면 배율로 잡아 한 장이 4MB, 96장×4구역이면
//    1.5GB → 화면 256pt 고정(한 장 ≈1MB), 캐시도 구역 크기에 맞춰 줄였다.
//  - text_offset + text_len 더하기 넘침(Swift 는 넘치면 앱이 죽는다)
//  - 축 배열이 비었을 때 배열 범위 밖 접근
//  - 메인 스레드에서 파일 열기 → 백그라운드에서 열고, 실패하면 PDF 경로로 돌아간다.
//
//  P6 사용성 개선:
//  - 3m 거리 가독성을 위한 글자 굵기 단계(sheetTextWeight 0/1/2 + 한글 대체 글꼴용 CTStrokeWidth 보강) 및 최소 14pt 보장(sheetMinFont)
//  - 블루투스 키보드 단축키 전체 지원(키 누르고 있기 타이머 반복, Ctrl/Cmd+화살표 데이터 끝 점프, 셀 주소 이동, 찾기)
//  - 다크모드(sheetDarkMode: drawCommands 단일 경로 색 변환)
//  - 하단 시트 탭 스트립(ArgoSheetTabStrip + 엑셀 시트 탭 색상 + 전환 시 중앙 토스트)
//  - 행·열 머리글(ArgoSheetHeaderView + 숨긴 행·열 경계 이중선 + 좌상단 현재 주소)
//  - 커서 중심 십자 강조(가로·세로 띠 CALayer zPosition 300, 커서 칸은 끊어서 두 번 어두워지지 않게 처리, 타일 재렌더링 없음)
//

import UIKit
import CoreText
import CoreImage
import ImageIO

// MARK: - 책 손잡이

/// Rust 쪽 ASBook 을 붙드는 상자.
///
/// 타일은 백그라운드 큐에서 그려지므로, 뷰어를 닫는 순간에도 그리는 중일 수 있다.
/// 포인터를 뷰컨트롤러가 직접 닫으면 그리던 쪽이 풀린 메모리를 읽어 앱이 죽는다.
/// 타일 작업이 이 상자를 강하게 잡고 있으므로, 마지막 작업이 끝난 뒤에야 닫힌다.
final class ArgoSheetBook {
    let ptr: OpaquePointer

    private init(ptr: OpaquePointer) { self.ptr = ptr }

    /// 파일을 연다. 실패하면 (nil, 이유). 무거우니 메인 스레드에서 부르지 않는다.
    static func open(path: String) -> (ArgoSheetBook?, String) {
        as_set_timezone_offset(Int32(TimeZone.current.secondsFromGMT()))
        let p: OpaquePointer? = path.withCString { as_open($0) }
        guard let p = p else {
            // as_last_note 는 스레드마다 따로라서 연 스레드에서 바로 읽는다.
            let note = as_last_note().map { String(cString: $0) } ?? ""
            return (nil, note)
        }
        return (ArgoSheetBook(ptr: p), "")
    }

    deinit { as_close(ptr) }

    var sheetCount: Int { Int(as_sheet_count(ptr)) }

    func sheetInfo(_ index: Int) -> ASSheetInfo? {
        var info = ASSheetInfo()
        return as_sheet_info(ptr, UInt32(index), &info) ? info : nil
    }

    func cellInfo(sheet: Int, row: Int, col: Int) -> ASCellInfo? {
        var info = ASCellInfo()
        return as_cell_info(ptr, UInt32(sheet), UInt32(row), UInt32(col), &info) ? info : nil
    }

    /// P6: 시트 탭 색상이 지정되어 있으면 UIColor 로 돌려준다.
    func sheetTabColor(_ index: Int) -> UIColor? {
        var c = ASColor()
        guard as_sheet_tab_color(ptr, UInt32(index), &c) else { return nil }
        return UIColor(red: CGFloat(c.r) / 255.0,
                       green: CGFloat(c.g) / 255.0,
                       blue: CGFloat(c.b) / 255.0,
                       alpha: CGFloat(c.a) / 255.0)
    }
}

/// C 고정 길이 글자 배열(튜플)을 String 으로. 0 이 없으면 배열 끝까지.
func argoSheetString<T>(_ tuple: T) -> String {
    withUnsafeBytes(of: tuple) { raw -> String in
        let bytes = raw.bindMemory(to: UInt8.self)
        let n = bytes.firstIndex(of: 0) ?? bytes.count
        return String(decoding: bytes.prefix(n), as: UTF8.self)
    }
}

// MARK: - 타일 뷰

/// 시트의 한 구역(틀 고정 모서리·위·왼쪽·본문)을 타일로 그린다.
///
/// 타일 한 장은 화면 256pt 정사각. 그 안에 들어가는 시트 영역은 256/확대율 pt.
/// 백그라운드 큐 두 개에서 CGContext 로 그린 뒤 CALayer.contents 에 넣는다.
final class ArgoSheetTileView: UIView {
    private let book: ArgoSheetBook
    private let sheetIndex: UInt32
    /// 이 구역이 맡는 시트 좌표의 시작점(시트 pt).
    private(set) var sheetOriginPt: CGPoint
    private(set) var zoom: CGFloat
    private(set) var palette: ArgoSheetPalette
    private var generation: UInt64 = 0

    static let tileScreenPt: CGFloat = 256
    /// 이 구역에 둘 타일 수 상한. 보이는 칸 수 + 여유. (한 장 ≈ 1MB @2x)
    private var maxCachedTiles = 48

    private struct TileKey: Hashable {
        let tx: Int
        let ty: Int
        let gen: UInt64
    }

    private var tileLayers: [TileKey: CALayer] = [:]
    private var inFlight: Set<TileKey> = []
    private var lru: [TileKey] = []

    // P6: 커서 중심 십자 강조용 얇은 색상 레이어 (비트맵 없음 -> 메모리 0)
    // 커서 칸 자체를 덮지 않고, 가로·세로 띠가 교차하며 두 번 어두워지지 않도록
    // 가로 띠는 커서 왼쪽(hLeft)과 오른쪽(hRight) 두 조각, 세로 띠는 위쪽(vTop)과 아래쪽(vBottom) 두 조각으로 나눈다.
    private let crossHLeft = CALayer()
    private let crossHRight = CALayer()
    private let crossVTop = CALayer()
    private let crossVBottom = CALayer()

    private static let renderQueues: [DispatchQueue] = [
        DispatchQueue(label: "argo.sheet.tile0", qos: .userInitiated),
        DispatchQueue(label: "argo.sheet.tile1", qos: .userInitiated),
    ]

    init(book: ArgoSheetBook, sheetIndex: UInt32, sheetOriginPt: CGPoint, zoom: CGFloat, palette: ArgoSheetPalette) {
        self.book = book
        self.sheetIndex = sheetIndex
        self.sheetOriginPt = sheetOriginPt
        self.zoom = zoom
        self.palette = palette
        super.init(frame: .zero)
        backgroundColor = palette.canvasBackground
        clipsToBounds = true
        isUserInteractionEnabled = false

        // 순서: 타일(기본 0) -> 십자 강조(300) -> 차트·그림(400) -> 커서(500)
        for cl in [crossHLeft, crossHRight, crossVTop, crossVBottom] {
            cl.zPosition = 300
            cl.isHidden = true
            layer.addSublayer(cl)
        }
        applyCrossHairColor()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    private func applyCrossHairColor() {
        let cg = palette.crossHairColor.cgColor
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for cl in [crossHLeft, crossHRight, crossVTop, crossVBottom] {
            cl.backgroundColor = cg
        }
        CATransaction.commit()
    }

    /// 확대율이나 시작점, 팔레트(다크모드·글자굵기·최소글자크기)가 바뀌면 있던 타일을 모두 버린다.
    func reset(zoom: CGFloat, sheetOriginPt: CGPoint, palette: ArgoSheetPalette? = nil) {
        self.zoom = zoom
        self.sheetOriginPt = sheetOriginPt
        if let p = palette {
            self.palette = p
            self.backgroundColor = p.canvasBackground
            applyCrossHairColor()
        }
        generation &+= 1
        for (_, l) in tileLayers { l.removeFromSuperlayer() }
        tileLayers.removeAll()
        inFlight.removeAll()
        lru.removeAll()
        rebuildOverlays()
    }

    /// P6: 커서 이동 시 타일을 다시 그리지 않고 십자 강조 레이어의 프레임만 즉시 갱신한다.
    /// - Parameters:
    ///   - cursorSheetRect: 커서(병합 포함)의 시트 pt 사각형. 십자 강조가 꺼져 있으면 nil.
    func updateCrossHair(cursorSheetRect: CGRect?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }

        guard let cr = cursorSheetRect, bounds.width > 0, bounds.height > 0 else {
            for cl in [crossHLeft, crossHRight, crossVTop, crossVBottom] { cl.isHidden = true }
            return
        }

        // 시트 pt -> 이 타일 뷰의 로컬 화면 pt 변환
        let localMinX = (cr.minX - sheetOriginPt.x) * zoom
        let localMaxX = (cr.maxX - sheetOriginPt.x) * zoom
        let localMinY = (cr.minY - sheetOriginPt.y) * zoom
        let localMaxY = (cr.maxY - sheetOriginPt.y) * zoom

        let W = bounds.width
        let H = bounds.height

        // 1. 가로 띠: 이 구역의 세로 범위에 커서 행이 들어올 때만 표시하되, 커서 칸(localMinX ..< localMaxX)은 비운다.
        let rowVisibleHere = (localMaxY > 0 && localMinY < H)
        if rowVisibleHere {
            let y1 = max(0, localMinY)
            let y2 = min(H, localMaxY)
            let h = max(0, y2 - y1)

            // 왼쪽 조각: 0 ..< min(W, localMinX)
            let leftEnd = min(W, max(0, localMinX))
            if leftEnd > 0.5 && h > 0.5 {
                crossHLeft.frame = CGRect(x: 0, y: y1, width: leftEnd, height: h)
                crossHLeft.isHidden = false
            } else {
                crossHLeft.isHidden = true
            }

            // 오른쪽 조각: max(0, localMaxX) ..< W
            let rightStart = max(0, min(W, localMaxX))
            if W - rightStart > 0.5 && h > 0.5 {
                crossHRight.frame = CGRect(x: rightStart, y: y1, width: W - rightStart, height: h)
                crossHRight.isHidden = false
            } else {
                crossHRight.isHidden = true
            }
        } else {
            crossHLeft.isHidden = true
            crossHRight.isHidden = true
        }

        // 2. 세로 띠: 이 구역의 가로 범위에 커서 열이 들어올 때만 표시하되, 커서 행(localMinY ..< localMaxY)은 비운다.
        let colVisibleHere = (localMaxX > 0 && localMinX < W)
        if colVisibleHere {
            let x1 = max(0, localMinX)
            let x2 = min(W, localMaxX)
            let w = max(0, x2 - x1)

            // 위쪽 조각: 0 ..< min(H, localMinY)
            let topEnd = min(H, max(0, localMinY))
            if topEnd > 0.5 && w > 0.5 {
                crossVTop.frame = CGRect(x: x1, y: 0, width: w, height: topEnd)
                crossVTop.isHidden = false
            } else {
                crossVTop.isHidden = true
            }

            // 아래쪽 조각: max(0, localMaxY) ..< H
            let botStart = max(0, min(H, localMaxY))
            if H - botStart > 0.5 && w > 0.5 {
                crossVBottom.frame = CGRect(x: x1, y: botStart, width: w, height: H - botStart)
                crossVBottom.isHidden = false
            } else {
                crossVBottom.isHidden = true
            }
        } else {
            crossVTop.isHidden = true
            crossVBottom.isHidden = true
        }
    }

    /// visible: 이 뷰 좌표(화면 pt)에서 지금 보이는 사각형.
    func updateVisible(_ visible: CGRect) {
        let v = visible.intersection(bounds)
        guard !v.isNull, v.width > 0, v.height > 0 else { return }
        let T = Self.tileScreenPt
        let minTx = max(0, Int(floor(v.minX / T)))
        let maxTx = max(minTx, Int(floor((v.maxX - 0.01) / T)))
        let minTy = max(0, Int(floor(v.minY / T)))
        let maxTy = max(minTy, Int(floor((v.maxY - 0.01) / T)))
        // 보이는 장수의 두 배 + 8 까지는 들고 있는다(되돌아갈 때 다시 안 그리게).
        maxCachedTiles = max(24, (maxTx - minTx + 1) * (maxTy - minTy + 1) * 2 + 8)

        var active = Set<TileKey>()
        for ty in minTy...maxTy {
            for tx in minTx...maxTx {
                let key = TileKey(tx: tx, ty: ty, gen: generation)
                active.insert(key)
                if tileLayers[key] != nil {
                    touch(key)
                } else if !inFlight.contains(key) {
                    render(key)
                }
            }
        }
        evict(keeping: active)
        updateOverlays(v)
    }

    private func touch(_ key: TileKey) {
        if let i = lru.firstIndex(of: key) { lru.remove(at: i) }
        lru.append(key)
    }

    private func evict(keeping active: Set<TileKey>) {
        var i = 0
        while tileLayers.count > maxCachedTiles && i < lru.count {
            let k = lru[i]
            if active.contains(k) { i += 1; continue }
            lru.remove(at: i)
            tileLayers.removeValue(forKey: k)?.removeFromSuperlayer()
        }
    }

    private func render(_ key: TileKey) {
        inFlight.insert(key)
        let book = self.book            // 강하게 잡는다 — 그리는 동안 닫히지 않게
        let sheet = sheetIndex
        let z = zoom
        let pal = palette
        let T = Self.tileScreenPt
        let sheetTile = T / z
        let sx = sheetOriginPt.x + CGFloat(key.tx) * sheetTile
        let sy = sheetOriginPt.y + CGFloat(key.ty) * sheetTile
        let scale = UIScreen.main.scale
        let q = Self.renderQueues[abs(key.tx &+ key.ty) % 2]
        q.async { [weak self] in
            let image = Self.drawTile(book: book, sheet: sheet,
                                      sheetRect: CGRect(x: sx, y: sy, width: sheetTile, height: sheetTile),
                                      zoom: z, screenScale: scale, palette: pal)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.inFlight.remove(key)
                guard self.generation == key.gen, let image = image else { return }
                let layer = CALayer()
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                layer.frame = CGRect(x: CGFloat(key.tx) * T, y: CGFloat(key.ty) * T, width: T, height: T)
                layer.contentsScale = scale
                layer.contents = image
                self.layer.addSublayer(layer)
                CATransaction.commit()
                self.tileLayers[key] = layer
                self.lru.append(key)
                self.evict(keeping: [key])
            }
        }
    }

    // MARK: P5 오버레이(차트·그림·도형)
    //
    // 타일 위 별도 층(zPosition 400, 커서 500 아래)에 올린다. 시작 셀이 속한 틀 고정 구역 한 곳만
    // 그린다(여러 구역에 겹쳐 그리면 스크롤할 때 둘로 갈라져 보인다). 보이는 곳 근처만 그리고,
    // 멀어지면 버린다 — 그림 수백 장짜리 파일에서도 메모리가 늘지 않게.

    enum Quadrant { case corner, top, left, body }
    private var quadrant: Quadrant = .body
    private var freezeRows: UInt32 = 0
    private var freezeCols: UInt32 = 0

    private struct OverlayItem {
        let index: UInt32
        let info: ASOverlayInfo
        let frame: CGRect       // 이 뷰 좌표(화면 pt)
    }
    private struct OverlayKey: Hashable { let index: UInt32; let gen: UInt64 }
    private var overlayItems: [OverlayItem] = []
    private var overlayLayers: [UInt32: CALayer] = [:]
    private var overlayInFlight: Set<OverlayKey> = []
    private var overlayGen: UInt64 = 0
    /// 한 시트에서 다룰 오버레이 상한(악성 파일 대비).
    private static let maxOverlays = 1000   // Rust MAX_DRAWINGS_PER_SHEET 와 같게
    /// 오버레이 한 장의 긴 변 픽셀 상한(2048² × 4 = 16MB).
    private static let overlayMaxPx: CGFloat = 2048
    private static let overlayQueue = DispatchQueue(label: "argo.sheet.overlay", qos: .userInitiated)

    func configureOverlays(quadrant: Quadrant, freezeRows: Int, freezeCols: Int) {
        self.quadrant = quadrant
        self.freezeRows = UInt32(clamping: max(0, freezeRows))
        self.freezeCols = UInt32(clamping: max(0, freezeCols))
        rebuildOverlays()
    }

    private func ownsOverlay(row: UInt32, col: UInt32) -> Bool {
        let fr = row <= freezeRows, fc = col <= freezeCols
        switch quadrant {
        case .corner: return fr && fc
        case .top:    return fr && !fc
        case .left:   return !fr && fc
        case .body:   return !fr && !fc
        }
    }

    private func rebuildOverlays() {
        overlayGen &+= 1
        for (_, l) in overlayLayers { l.removeFromSuperlayer() }
        overlayLayers.removeAll()
        overlayInFlight.removeAll()
        overlayItems.removeAll()
        let n = min(Int(as_sheet_overlay_count(book.ptr, sheetIndex)), Self.maxOverlays)
        guard n > 0 else { return }
        for i in 0..<n {
            var oi = ASOverlayInfo()
            guard as_sheet_overlay_info(book.ptr, sheetIndex, UInt32(i), &oi),
                  ownsOverlay(row: oi.start_row, col: oi.start_col) else { continue }
            let f = oi.frame_pt
            guard f.x.isFinite, f.y.isFinite, f.w.isFinite, f.h.isFinite,
                  f.w > 0.5, f.h > 0.5, f.w < 1e6, f.h < 1e6 else { continue }
            let r = CGRect(x: (CGFloat(f.x) - sheetOriginPt.x) * zoom,
                           y: (CGFloat(f.y) - sheetOriginPt.y) * zoom,
                           width: CGFloat(f.w) * zoom, height: CGFloat(f.h) * zoom)
            overlayItems.append(OverlayItem(index: UInt32(i), info: oi, frame: r))
        }
    }

    private func updateOverlays(_ v: CGRect) {
        guard !overlayItems.isEmpty else { return }
        let near = v.insetBy(dx: -v.width, dy: -v.height)
        let far = v.insetBy(dx: -3 * v.width, dy: -3 * v.height)
        for item in overlayItems {
            if item.frame.intersects(near) {
                if overlayLayers[item.index] == nil,
                   !overlayInFlight.contains(OverlayKey(index: item.index, gen: overlayGen)) {
                    renderOverlay(item)
                }
            } else if !item.frame.intersects(far), let l = overlayLayers.removeValue(forKey: item.index) {
                l.removeFromSuperlayer()
            }
        }
    }

    private func renderOverlay(_ item: OverlayItem) {
        let key = OverlayKey(index: item.index, gen: overlayGen)
        overlayInFlight.insert(key)
        let book = self.book            // 강하게 잡는다 — 그리는 동안 닫히지 않게
        let sheet = sheetIndex
        let scale = UIScreen.main.scale
        let info = item.info
        let size = item.frame.size
        let chartPal = palette.forChartOverlay()
        Self.overlayQueue.async { [weak self] in
            let image = Self.drawOverlay(book: book, sheet: sheet, index: item.index, info: info,
                                         screenSize: size, screenScale: scale, palette: chartPal)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.overlayInFlight.remove(key)
                guard self.overlayGen == key.gen, self.overlayLayers[item.index] == nil,
                      let image = image else { return }
                let layer = CALayer()
                CATransaction.begin()
                CATransaction.setDisableActions(true)
                layer.bounds = CGRect(origin: .zero, size: item.frame.size)
                layer.position = CGPoint(x: item.frame.midX, y: item.frame.midY)
                layer.contents = image
                layer.contentsGravity = .resize
                layer.contentsScale = scale
                layer.zPosition = 400
                if info.kind == 1 || info.kind == 2 {
                    // a:srcRect 자르기. 음수(여백 늘리기)는 무시한다.
                    func c(_ x: Float) -> CGFloat { x.isFinite ? CGFloat(min(max(x, 0), 1)) : 0 }
                    let l = c(info.crop_left), t = c(info.crop_top)
                    let w = max(0.01, 1 - l - c(info.crop_right)), h = max(0.01, 1 - t - c(info.crop_bottom))
                    if l > 0 || t > 0 || w < 1 || h < 1 {
                        layer.contentsRect = CGRect(x: l, y: t, width: w, height: h)
                    }
                }
                let rot = CGFloat(info.rotation_deg)
                if rot.isFinite, rot != 0 {
                    layer.setAffineTransform(CGAffineTransform(rotationAngle: rot * .pi / 180))
                }
                self.layer.addSublayer(layer)
                CATransaction.commit()
                self.overlayLayers[item.index] = layer
            }
        }
    }

    /// 백그라운드. 차트·도형은 그리기 목록(틀 기준 0..w, 0..h pt), 그림은 원본 바이트, EMF 는 RGBA.
    private static func drawOverlay(book: ArgoSheetBook, sheet: UInt32, index: UInt32, info: ASOverlayInfo,
                                    screenSize: CGSize, screenScale: CGFloat, palette: ArgoSheetPalette) -> CGImage? {
        switch info.kind {
        case 0, 3:
            guard let dl = as_sheet_overlay_drawlist(book.ptr, sheet, index) else { return nil }
            defer { as_drawlist_free(dl) }
            let fw = CGFloat(info.frame_pt.w), fh = CGFloat(info.frame_pt.h)
            let w0 = screenSize.width * screenScale, h0 = screenSize.height * screenScale
            guard fw > 0, fh > 0, w0 > 0, h0 > 0 else { return nil }
            let k = min(1, overlayMaxPx / max(w0, h0))
            let pw = max(1, Int(ceil(w0 * k))), ph = max(1, Int(ceil(h0 * k)))
            guard let ctx = CGContext(data: nil, width: pw, height: ph, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            ctx.clear(CGRect(x: 0, y: 0, width: pw, height: ph))
            ctx.translateBy(x: 0, y: CGFloat(ph))
            ctx.scaleBy(x: CGFloat(pw) / fw, y: -CGFloat(ph) / fh)
            drawCommands(ctx, dl, palette)
            return ctx.makeImage()
        case 1:
            // 그림(PNG·JPEG)은 다크모드에서도 원본 색 그대로 둔다.
            var len: UInt32 = 0
            guard let p = as_sheet_overlay_bytes(book.ptr, sheet, index, &len), len > 0 else { return nil }
            let data = Data(bytes: p, count: Int(len))  // 복사해 둔다
            guard let src = CGImageSourceCreateWithData(data as CFData, nil),
                  CGImageSourceGetCount(src) > 0 else { return nil }
            // 압축 폭탄 방지: 원본 픽셀 수가 지나치면 열지 않는다.
            if let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any] {
                let w = (props[kCGImagePropertyPixelWidth] as? Int) ?? 0
                let h = (props[kCGImagePropertyPixelHeight] as? Int) ?? 0
                if w <= 0 || h <= 0 || w > 30_000 || h > 30_000 || w * h > 100_000_000 { return nil }
            }
            let opts: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceShouldCacheImmediately: true,
                kCGImageSourceThumbnailMaxPixelSize: Int(overlayMaxPx),
            ]
            return CGImageSourceCreateThumbnailAtIndex(src, 0, opts as CFDictionary)
        case 2:
            // EMF 래스터 그림도 다크모드에서 원본 색 그대로 둔다.
            let w = Int(info.pixel_width), h = Int(info.pixel_height)
            guard w > 0, h > 0, w <= 4096, h <= 4096 else { return nil }
            var len: UInt32 = 0
            guard let p = as_sheet_overlay_bytes(book.ptr, sheet, index, &len),
                  Int(len) == w * h * 4 else { return nil }
            let data = Data(bytes: p, count: Int(len))
            guard let provider = CGDataProvider(data: data as CFData) else { return nil }
            return CGImage(width: w, height: h, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: w * 4,
                           space: CGColorSpaceCreateDeviceRGB(),
                           bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                           provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        default:
            return nil
        }
    }

    // MARK: 그리기 (백그라운드)

    private static func drawTile(book: ArgoSheetBook, sheet: UInt32, sheetRect: CGRect,
                                 zoom: CGFloat, screenScale: CGFloat, palette: ArgoSheetPalette) -> CGImage? {
        guard let dl = as_tile(book.ptr, sheet, Float(sheetRect.minX), Float(sheetRect.minY),
                               Float(sheetRect.width), Float(sheetRect.height), Float(zoom)) else {
            return nil
        }
        defer { as_drawlist_free(dl) }

        let px = Int(ceil(tileScreenPt * screenScale))
        guard px > 0, let ctx = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8,
                                          bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
            return nil
        }
        // P6 다크모드: 빈 칸 바탕을 팔레트 배경색(밝은 #FFFFFF / 어두운 #1C1E22)으로 칠한다.
        ctx.setFillColor(palette.canvasBackground.cgColor)
        ctx.fill(CGRect(x: 0, y: 0, width: px, height: px))

        // CG 의 좌하단 원점을 좌상단으로 뒤집고, 시트 pt 를 그대로 쓰게 맞춘다.
        ctx.translateBy(x: 0, y: CGFloat(px))
        ctx.scaleBy(x: screenScale * zoom, y: -screenScale * zoom)
        ctx.translateBy(x: -sheetRect.minX, y: -sheetRect.minY)

        drawCommands(ctx, dl, palette)
        return ctx.makeImage()
    }

    /// 그리기 목록 하나를 그린다(타일·차트·도형 공용). ctx 는 이미 시트 pt 좌표로 맞춰져 있어야 한다.
    /// P6: 다크모드 색상 변환과 글자 굵기·최소 크기는 모두 이 경로의 `palette` 인자 하나로 통일한다.
    static func drawCommands(_ ctx: CGContext, _ dl: OpaquePointer, _ palette: ArgoSheetPalette = .lightDefault) {
        let count = Int(as_drawlist_count(dl))
        guard count > 0, let cmdPtr = as_drawlist_commands(dl) else { return }
        var strLen: UInt32 = 0
        let strPtr = as_drawlist_strings(dl, &strLen)

        for cmd in UnsafeBufferPointer(start: cmdPtr, count: count) {
            switch cmd.cmd_type {
            case 0: // Fill
                ctx.setFillColor(palette.fillColor(cmd.color1))
                ctx.fill(cgRect(cmd.rect))
            case 1: // Gradient (color1 → color2, 위→아래)
                drawGradient(ctx, cgRect(cmd.rect), palette.fillColor(cmd.color1), palette.fillColor(cmd.color2))
            case 2: // Pattern: 바탕(color2) 위에 무늬 색(color1)을 무늬 밀도만큼 섞어 칠한다.
                ctx.setFillColor(palette.fillColor(cmd.color2))
                ctx.fill(cgRect(cmd.rect))
                ctx.setFillColor(palette.fillColor(cmd.color1, alphaOverride: patternDensity(cmd.sub_type)))
                ctx.fill(cgRect(cmd.rect))
            case 3: // Line (rect = x1,y1,x2,y2)
                drawLine(ctx, cmd, palette)
            case 4: // Text
                // 오프셋+길이를 Int 로 더한다(UInt32 끼리 더하면 넘칠 때 앱이 죽는다).
                let off = Int(cmd.text_offset), len = Int(cmd.text_len)
                guard let base = strPtr, len > 0, off >= 0, off + len <= Int(strLen) else { continue }
                let raw = UnsafeRawPointer(base).advanced(by: off)
                let text = String(decoding: UnsafeRawBufferPointer(start: raw, count: len), as: UTF8.self)
                drawText(ctx, text, cmd, palette)
            case 5: // Marker: 0 = 메모(오른쪽 위 빨간 삼각형), 1 = 자동 필터 단추
                let r = cgRect(cmd.rect)
                // 메모 표시(Marker)는 다크모드에서도 원본 색 그대로 유지
                ctx.setFillColor(cgColor(cmd.color1))
                if cmd.sub_type == 1 {
                    // 표·자동 필터 머리글 단추(보기 전용): 옅은 상자 + 아래 화살촉
                    if palette.isDark {
                        ctx.setFillColor(CGColor(srgbRed: 0.22, green: 0.25, blue: 0.30, alpha: 0.95))
                        ctx.fill(r)
                        ctx.setStrokeColor(CGColor(srgbRed: 0.45, green: 0.49, blue: 0.56, alpha: 1))
                        ctx.setLineWidth(0.5)
                        ctx.stroke(r)
                        ctx.setFillColor(CGColor(srgbRed: 0.82, green: 0.85, blue: 0.90, alpha: 1))
                    } else {
                        ctx.setFillColor(CGColor(srgbRed: 0.94, green: 0.95, blue: 0.97, alpha: 0.95))
                        ctx.fill(r)
                        ctx.setStrokeColor(CGColor(srgbRed: 0.65, green: 0.68, blue: 0.73, alpha: 1))
                        ctx.setLineWidth(0.5)
                        ctx.stroke(r)
                        ctx.setFillColor(CGColor(srgbRed: 0.25, green: 0.28, blue: 0.32, alpha: 1))
                    }
                    ctx.move(to: CGPoint(x: r.minX + r.width * 0.25, y: r.minY + r.height * 0.4))
                    ctx.addLine(to: CGPoint(x: r.maxX - r.width * 0.25, y: r.minY + r.height * 0.4))
                    ctx.addLine(to: CGPoint(x: r.midX, y: r.maxY - r.height * 0.3))
                } else {
                    ctx.move(to: CGPoint(x: r.minX, y: r.minY))
                    ctx.addLine(to: CGPoint(x: r.maxX, y: r.minY))
                    ctx.addLine(to: CGPoint(x: r.maxX, y: r.maxY))
                }
                ctx.closePath()
                ctx.fillPath()
            case 6: // 조건부 서식 데이터 막대 (다크모드에서도 막대 색 그대로 유지)
                let clip = cgRect(cmd.clip_rect), bar = cgRect(cmd.rect)
                ctx.saveGState()
                ctx.clip(to: clip)
                if bar.width > 0.2, bar.height > 0.2 {
                    ctx.setFillColor(cgColor(cmd.color1, alpha: 0.85))
                    ctx.fill(bar)
                }
                if cmd.align_vert == 1 {
                    let ax = CGFloat(cmd.stroke_width)
                    ctx.setStrokeColor(palette.lineColor(cmd.color2, isGrid: false))
                    ctx.setLineWidth(0.75)
                    ctx.setLineDash(phase: 0, lengths: [1.5, 1.5])
                    ctx.move(to: CGPoint(x: ax, y: clip.minY + 1))
                    ctx.addLine(to: CGPoint(x: ax, y: clip.maxY - 1))
                    ctx.strokePath()
                }
                ctx.restoreGState()
            case 7: // 조건부 서식 아이콘 (다크모드에서도 아이콘 색 그대로 유지)
                drawIcon(ctx, cmd)
            default:
                break
            }
        }
    }

    /// 아이콘 집합을 도형으로 그린다(그림 파일 없음). 화살표 계열은 단계에 따라 돌린다.
    private static func drawIcon(_ ctx: CGContext, _ cmd: ASDrawCmd) {
        let box = cgRect(cmd.rect)
        guard box.width > 2, box.height > 2 else { return }
        ctx.saveGState()
        defer { ctx.restoreGState() }
        ctx.clip(to: cgRect(cmd.clip_rect))
        ctx.setFillColor(cgColor(cmd.color1))
        let step = Int(cmd.align_horiz)

        switch cmd.sub_type {
        case 2:
            ctx.fillEllipse(in: box.insetBy(dx: 0.8, dy: 0.8))
            return
        case 3:
            if step == 0 {          // 마름모
                ctx.move(to: CGPoint(x: box.midX, y: box.minY))
                ctx.addLine(to: CGPoint(x: box.maxX, y: box.midY))
                ctx.addLine(to: CGPoint(x: box.midX, y: box.maxY))
                ctx.addLine(to: CGPoint(x: box.minX, y: box.midY))
            } else if step == 1 {   // 세모
                ctx.move(to: CGPoint(x: box.midX, y: box.minY + 0.5))
                ctx.addLine(to: CGPoint(x: box.maxX, y: box.maxY - 0.5))
                ctx.addLine(to: CGPoint(x: box.minX, y: box.maxY - 0.5))
            } else {
                ctx.fillEllipse(in: box.insetBy(dx: 0.8, dy: 0.8))
                return
            }
            ctx.closePath()
            ctx.fillPath()
            return
        default:
            break
        }

        let angles: [CGFloat]
        switch cmd.sub_type {
        case 7: angles = [180, 135, 45, 0]
        case 8: angles = [180, 135, 90, 45, 0]
        default: angles = [180, 90, 0]
        }
        let deg = angles[min(max(step, 0), angles.count - 1)]
        let r = min(box.width, box.height) * 0.46
        ctx.translateBy(x: box.midX, y: box.midY)
        ctx.rotate(by: deg * .pi / 180)
        let headW = r * 0.92, baseY = -r * 0.10, shaftW = r * 0.42
        ctx.move(to: CGPoint(x: 0, y: -r))
        ctx.addLine(to: CGPoint(x: headW, y: baseY))
        ctx.addLine(to: CGPoint(x: shaftW, y: baseY))
        ctx.addLine(to: CGPoint(x: shaftW, y: r))
        ctx.addLine(to: CGPoint(x: -shaftW, y: r))
        ctx.addLine(to: CGPoint(x: -shaftW, y: baseY))
        ctx.addLine(to: CGPoint(x: -headW, y: baseY))
        ctx.closePath()
        ctx.fillPath()
    }

    private static func cgRect(_ r: ASRect) -> CGRect {
        CGRect(x: CGFloat(r.x), y: CGFloat(r.y), width: CGFloat(r.w), height: CGFloat(r.h))
    }

    private static func cgColor(_ c: ASColor, alpha: CGFloat? = nil) -> CGColor {
        CGColor(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255, blue: CGFloat(c.b) / 255,
                alpha: alpha ?? CGFloat(c.a) / 255)
    }

    /// 엑셀 무늬(patternType) 번호 → 앞색 비율.
    private static func patternDensity(_ id: UInt8) -> CGFloat {
        switch id {
        case 2: return 0.50   // mediumGray
        case 3: return 0.75   // darkGray
        case 4: return 0.25   // lightGray
        case 17: return 0.125 // gray125
        case 18: return 0.0625 // gray0625
        default: return 0.40  // 줄무늬·격자무늬류
        }
    }

    private static func drawGradient(_ ctx: CGContext, _ r: CGRect, _ a: CGColor, _ b: CGColor) {
        guard r.width > 0, r.height > 0,
              let g = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                                 colors: [a, b] as CFArray, locations: [0, 1]) else { return }
        ctx.saveGState()
        ctx.clip(to: r)
        ctx.drawLinearGradient(g, start: CGPoint(x: r.minX, y: r.minY), end: CGPoint(x: r.minX, y: r.maxY), options: [])
        ctx.restoreGState()
    }

    /// 선 13종(+격자선 0). 두께는 Rust 가 stroke_width 로 준다.
    private static func drawLine(_ ctx: CGContext, _ cmd: ASDrawCmd, _ palette: ArgoSheetPalette) {
        let p1 = CGPoint(x: CGFloat(cmd.rect.x), y: CGFloat(cmd.rect.y))
        let p2 = CGPoint(x: CGFloat(cmd.rect.w), y: CGFloat(cmd.rect.h))
        let w = CGFloat(max(cmd.stroke_width, 0.25))
        ctx.saveGState()
        ctx.setStrokeColor(palette.lineColor(cmd.color1, isGrid: cmd.sub_type == 0))
        ctx.setLineCap(.butt)
        if cmd.sub_type == 12 { // double: 가는 선 두 줄, 사이 1pt
            let dx = p2.x - p1.x, dy = p2.y - p1.y
            let len = max(hypot(dx, dy), 0.001)
            let nx = -dy / len, ny = dx / len
            ctx.setLineWidth(0.6)
            for s in [-1.0, 1.0] as [CGFloat] {
                ctx.move(to: CGPoint(x: p1.x + nx * s, y: p1.y + ny * s))
                ctx.addLine(to: CGPoint(x: p2.x + nx * s, y: p2.y + ny * s))
            }
            ctx.strokePath()
            ctx.restoreGState()
            return
        }
        ctx.setLineWidth(w)
        let dash: [CGFloat]
        switch cmd.sub_type {
        case 1: dash = [0.75, 0.75]                        // hair
        case 2: dash = [1, 1]                              // dotted
        case 3, 7: dash = [3, 1, 1, 1, 1, 1]               // dashDotDot / mediumDashDotDot
        case 4, 9: dash = [3, 1, 1, 1]                     // dashDot / mediumDashDot
        case 5: dash = [3, 1]                              // dashed
        case 8: dash = [4, 1, 2, 1]                        // slantDashDot
        case 10: dash = [4, 2]                             // mediumDashed
        default: dash = []                                 // grid, thin, medium, thick
        }
        if !dash.isEmpty { ctx.setLineDash(phase: 0, lengths: dash.map { $0 * max(1, w) }) }
        ctx.move(to: p1)
        ctx.addLine(to: p2)
        ctx.strokePath()
        ctx.restoreGState()
    }

    // 글자 속성 비트 (argosheet.h AS_TEXT_FLAG_*)
    private static let flagBold: UInt8 = 1 << 0
    private static let flagItalic: UInt8 = 1 << 1
    private static let flagUnderline: UInt8 = 1 << 2
    private static let flagStrike: UInt8 = 1 << 3
    private static let flagWrap: UInt8 = 1 << 4
    private static let flagShrink: UInt8 = 1 << 5

    /// P6 글자 굵기:
    /// - textWeight == 0(보통): 기존과 동일
    /// - textWeight == 1(굵게, 기본) 및 2(아주 굵게): 보통 글자는 .semibold, 원래 굵은 글자는 .heavy
    /// - 한글 대체 글꼴은 weight trait 가 무시될 수 있으므로 attributed() 에서 음수 kCTStrokeWidthAttributeName 을 함께 부여한다.
    private static func makeFont(size: CGFloat, flags: UInt8, textWeight: Int) -> CTFont {
        let isBold = (flags & flagBold) != 0
        let isItalic = (flags & flagItalic) != 0

        if textWeight >= 1 {
            let uiWeight: UIFont.Weight = isBold ? .heavy : .semibold
            var uiFont = UIFont.systemFont(ofSize: size, weight: uiWeight)
            if isItalic, let desc = uiFont.fontDescriptor.withSymbolicTraits(.traitItalic) {
                uiFont = UIFont(descriptor: desc, size: size)
            }
            return uiFont as CTFont
        }

        var font = CTFontCreateUIFontForLanguage(.system, size, nil)
            ?? CTFontCreateWithName("Helvetica" as CFString, size, nil)
        var traits: CTFontSymbolicTraits = []
        if isBold { traits.insert(.traitBold) }
        if isItalic { traits.insert(.traitItalic) }
        if !traits.isEmpty, let f = CTFontCreateCopyWithSymbolicTraits(font, size, nil, traits, traits) {
            font = f
        }
        return font
    }

    /// P6: 글자 굵기 획(kCTStrokeWidthAttributeName) + 최소 14pt 보장(sheetMinFont) + 다크모드 4.5:1 대비 보정 포함.
    private static func drawText(_ ctx: CGContext, _ rawText: String, _ cmd: ASDrawCmd, _ palette: ArgoSheetPalette) {
        var box = cgRect(cmd.rect)
        guard box.width > 0, box.height > 0, !rawText.isEmpty else { return }
        ctx.saveGState()
        ctx.clip(to: cgRect(cmd.clip_rect))
        defer { ctx.restoreGState() }

        let flags = cmd.text_flags
        let align = cmd.align_horiz
        let indent = CGFloat(cmd.indent) * 7
        if indent > 0 {
            if align == 2 { box.size.width -= indent } else if align == 0 || align == 3 {
                box.origin.x += indent; box.size.width -= indent
            }
            guard box.width > 0 else { return }
        }

        var size = palette.effectiveFontSize(max(CGFloat(cmd.font_size_pt), 6))
        let color = palette.textColor(cmd.color1)
        let rotation = Int(cmd.rotation)
        let isBold = (flags & flagBold) != 0
        let strokePercent = palette.strokeWidthPercent(isCellBold: isBold)

        func attributed(_ s: String, _ font: CTFont) -> NSAttributedString {
            var dict: [NSAttributedString.Key: Any] = [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
            ]
            // 한글 대체 글꼴에서도 3m 거리 가독성과 엑셀 원본 굵게/보통 차이를 유지하기 위해 음수 획(채우기+외곽선)을 적용한다.
            if strokePercent != 0 {
                dict[NSAttributedString.Key(kCTStrokeWidthAttributeName as String)] = NSNumber(value: Double(strokePercent))
                dict[NSAttributedString.Key(kCTStrokeColorAttributeName as String)] = color
            }
            return NSAttributedString(string: s, attributes: dict)
        }
        func lineWidth(_ line: CTLine) -> CGFloat { CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) }

        var text = rawText
        var font = makeFont(size: size, flags: flags, textWeight: palette.textWeight)

        // 세로쓰기(255): 글자를 한 자씩 위에서 아래로.
        if rotation == 255 {
            text = text.map { String($0) }.joined(separator: "\n")
        }
        let wrap = flags & flagWrap != 0 || rotation == 255

        // 셀에 맞춤: 한 줄 폭이 넘치면 글자를 줄인다(최소 4pt).
        if flags & flagShrink != 0 && !wrap {
            let w = lineWidth(CTLineCreateWithAttributedString(attributed(text, font)))
            if w > box.width, w > 0 {
                size = max(4, size * box.width / w)
                font = makeFont(size: size, flags: flags, textWeight: palette.textWeight)
            }
        }
        // 채우기(fill): 셀 폭이 찰 때까지 글자를 되풀이.
        if align == 3, !wrap {
            let w = lineWidth(CTLineCreateWithAttributedString(attributed(text, font)))
            if w > 0 && w < box.width {
                text = String(repeating: text, count: min(500, Int(box.width / w)))
            }
        }

        let angle: CGFloat
        if rotation >= 1 && rotation <= 90 { angle = CGFloat(rotation) * .pi / 180 }
        else if rotation > 90 && rotation <= 180 { angle = -CGFloat(rotation - 90) * .pi / 180 }
        else { angle = 0 }
        let layoutWidth = angle == 0 ? box.width : (abs(angle) > .pi / 4 ? box.height : box.width)

        var lines: [CTLine] = []
        for para in text.components(separatedBy: "\n") {
            let attr = attributed(para, font)
            if !wrap || para.isEmpty {
                lines.append(CTLineCreateWithAttributedString(attr))
                continue
            }
            let typesetter = CTTypesetterCreateWithAttributedString(attr)
            var start = 0
            let total = (para as NSString).length
            var guardN = 0
            while start < total && guardN < 2000 {
                var n = CTTypesetterSuggestLineBreak(typesetter, start, Double(max(layoutWidth, 1)))
                if n <= 0 { n = 1 }
                lines.append(CTTypesetterCreateLine(typesetter, CFRange(location: start, length: n)))
                start += n
                guardN += 1
            }
        }
        guard !lines.isEmpty else { return }

        let ascent = CTFontGetAscent(font), descent = CTFontGetDescent(font)
        let lineH = ascent + descent + CTFontGetLeading(font) + size * 0.1
        let blockH = lineH * CGFloat(lines.count)
        let maxLineW = lines.map(lineWidth).max() ?? 0

        ctx.textMatrix = CGAffineTransform(scaleX: 1, y: -1)

        if angle != 0 {
            ctx.translateBy(x: box.midX, y: box.midY)
            ctx.rotate(by: -angle)
            var y = -blockH / 2 + ascent
            for line in lines {
                ctx.textPosition = CGPoint(x: -lineWidth(line) / 2, y: y)
                CTLineDraw(line, ctx)
                y += lineH
            }
            return
        }

        let topY: CGFloat
        switch cmd.align_vert {
        case 0: topY = box.minY
        case 2: topY = box.maxY - blockH
        default: topY = box.minY + (box.height - blockH) / 2
        }
        var y = topY + ascent
        for (i, line) in lines.enumerated() {
            let w = lineWidth(line)
            var x: CGFloat
            switch align {
            case 1, 6: x = box.minX + (box.width - w) / 2
            case 2: x = box.maxX - w
            default: x = box.minX
            }
            var drawLine = line
            if (align == 4 && i < lines.count - 1) || align == 5,
               w > box.width * 0.5,
               let j = CTLineCreateJustifiedLine(line, 1.0, Double(box.width)) {
                drawLine = j
                x = box.minX
            }
            ctx.textPosition = CGPoint(x: x, y: y)
            CTLineDraw(drawLine, ctx)
            let dw = lineWidth(drawLine)
            if flags & flagUnderline != 0 {
                ctx.setStrokeColor(color)
                ctx.setLineWidth(max(0.5, size / 16))
                ctx.move(to: CGPoint(x: x, y: y + descent * 0.5))
                ctx.addLine(to: CGPoint(x: x + dw, y: y + descent * 0.5))
                ctx.strokePath()
            }
            if flags & flagStrike != 0 {
                ctx.setStrokeColor(color)
                ctx.setLineWidth(max(0.5, size / 16))
                ctx.move(to: CGPoint(x: x, y: y - ascent * 0.32))
                ctx.addLine(to: CGPoint(x: x + dw, y: y - ascent * 0.32))
                ctx.strokePath()
            }
            y += lineH
        }
        _ = maxLineW
    }
}

// MARK: - 뷰컨트롤러

/// 전체 화면 엑셀 뷰어. 위: 주소·값·수식 막대 / 가운데: 머리글 + 틀 고정 4구역 / 아래: 시트 탭·확대율.
///
/// 입력은 화면 전체 ArgoFocusCanvas 하나가 받는다(PDF 뷰어·사진 앨범과 같다).
@available(tvOS 13.0, *)
final class ArgoSheetViewerController: UIViewController {

    /// 이 뷰어가 맡을 파일인지. 설정에서 끄면 예전 PDF 변환으로 간다.
    static func shouldHandle(_ path: String) -> Bool {
        let on = UserDefaults.settings.object(forKey: "excelCellViewer") as? Bool ?? true
        let ext = (path as NSString).pathExtension.lowercased()
        return on && (ext == "xlsx" || ext == "xlsm")
    }

    var onClose: (() -> Void)?
    /// 열지 못했을 때. 받는 쪽이 PDF 경로로 다시 연다.
    var onOpenFailed: ((String) -> Void)?

    private let path: String
    private var book: ArgoSheetBook?
    /// 화면이 다 뜨기 전에 실패하면 dismiss 가 무시되어 뷰어가 남는다. 다 뜬 뒤에 알린다.
    private var appeared = false
    private var pendingFailure: String?
    private var visibleSheets: [Int] = []
    private var sheetIndex = 0
    private var info = ASSheetInfo()

    // P6: 확대 단계 [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0], 기본 1.75, sheetZoom 저장 연동
    private let zoomLevels: [CGFloat] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0]
    private var zoom: CGFloat = 1.75

    // P6: 사용자 설정 상태 (UserDefaults.settings 연동)
    private var isDarkMode: Bool = false
    private var showHeaders: Bool = true
    private var showCross: Bool = true
    private var textWeight: Int = 1 // 0=보통, 1=굵게(기본), 2=아주 굵게
    private var minFont14: Bool = false

    private var currentPalette: ArgoSheetPalette {
        ArgoSheetPalette(isDark: isDarkMode, isChartOverlay: false, textWeight: textWeight, minFont14: minFont14, zoom: zoom)
    }

    private var colOffsets: [CGFloat] = [0]   // 시트 pt 누적, 길이 = 열 수 + 1
    private var rowOffsets: [CGFloat] = [0]
    private var freezeCols = 0, freezeRows = 0

    private var cursorRow = 1, cursorCol = 1
    private var cursorInfo: ASCellInfo?

    // P6: 찾기 상태
    private var searchQuery: String = ""
    private var searchHits: [ASHit] = []
    private var searchHitIndex: Int = -1

    // 화면
    private let topBar = UIView()
    private let addrLabel = UILabel()
    private let valueLabel = UILabel()
    private let formulaLabel = UILabel()
    private let bottomBar = UIView()
    private let sheetLabel = UILabel() // 기존 속성 보존(숨김 처리 후 tabStrip 표시)
    private let tabStrip = ArgoSheetTabStrip()
    private let zoomLabel = UILabel()
    private let statusLabel = UILabel()
    private let spinner = UIActivityIndicatorView(style: .large)

    // P6: 시트 전환 및 찾기 배너 토스트 (화면 중앙 및 상단 중앙)
    private let centerToastLabel = UILabel()
    private let topToastLabel = UILabel()

    private let grid = UIView()
    // P6: 행·열 머리글 뷰 3종(좌상단 모서리 주소 칸, 상단 열 머리글, 좌측 행 머리글)
    private let cornerHeaderView = ArgoSheetHeaderView(mode: .corner)
    private let colHeaderView = ArgoSheetHeaderView(mode: .columns)
    private let rowHeaderView = ArgoSheetHeaderView(mode: .rows)

    private let cornerHost = UIView(), topHost = UIView(), leftHost = UIView(), bodyHost = UIView()
    private var cornerTiles, topTiles, leftTiles, bodyTiles: ArgoSheetTileView?
    private let dividerH = UIView(), dividerV = UIView()
    private var bodyOffset = CGPoint.zero     // 본문 스크롤 위치(화면 pt)
    private let cursorLayer = CALayer()
    private let canvas = ArgoFocusCanvas()

    // 누르고 있기 반복 (리모컨 및 키보드 화살표 공용)
    private var repeatTimer: Timer?
    private var repeatDir: (Int, Int) = (0, 0)
    private var repeatCount = 0
    private var activeHeldKeyCode: UIKeyboardHIDUsage?

    init(path: String) {
        self.path = path
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
        loadSavedSettings()
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    deinit { repeatTimer?.invalidate() }

    override var preferredFocusEnvironments: [UIFocusEnvironment] { [canvas] }

    private func loadSavedSettings() {
        let ud = UserDefaults.settings
        isDarkMode = ud.object(forKey: "sheetDarkMode") as? Bool ?? false
        showHeaders = ud.object(forKey: "sheetHeaders") as? Bool ?? true
        showCross = ud.object(forKey: "sheetCross") as? Bool ?? true
        let w = ud.object(forKey: "sheetTextWeight") as? Int ?? 1
        textWeight = min(max(w, 0), 2)
        minFont14 = ud.object(forKey: "sheetMinFont") as? Bool ?? false

        if let savedZoom = ud.object(forKey: "sheetZoom") as? Double, savedZoom >= 1.0, savedZoom <= 3.0 {
            // 가장 가까운 zoomLevels 단계로 맞춘다
            let target = CGFloat(savedZoom)
            zoom = zoomLevels.min(by: { abs($0 - target) < abs($1 - target) }) ?? 1.75
        } else {
            zoom = 1.75
        }
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.11, green: 0.12, blue: 0.15, alpha: 1)
        buildBars()
        buildGrid()
        buildToasts()
        applyChromeTheme()

        canvas.backgroundColor = .clear
        canvas.onKey = { [weak self] key in self?.handleKeyDown(key) ?? false }
        canvas.onKeyUp = { [weak self] key in self?.handleKeyUp(key) }
        canvas.onSelect = { [weak self] in self?.showCellSheet() }
        canvas.onPress = { [weak self] type in self?.handlePress(type) ?? false }
        canvas.onRelease = { [weak self] _ in self?.stopRepeat() }
        view.addSubview(canvas)

        openBook()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        appeared = true
        if let note = pendingFailure {
            pendingFailure = nil
            onOpenFailed?(note)
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        canvas.frame = view.bounds
        layoutAll()
    }

    // MARK: 열기

    private func openBook() {
        spinner.startAnimating()
        statusLabel.text = L("여는 중…")
        let path = self.path
        let t0 = CACurrentMediaTime()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let (book, note) = ArgoSheetBook.open(path: path)
            DispatchQueue.main.async {
                guard let self = self else { return }
                self.spinner.stopAnimating()
                guard let book = book, book.sheetCount > 0 else {
                    ArgoDiag.log("sheet", "열기 실패 → PDF 로: \(note)")
                    if self.appeared { self.onOpenFailed?(note) } else { self.pendingFailure = note }
                    return
                }
                ArgoDiag.log("sheet", String(format: "열기 %.2f초, 시트 %d개", CACurrentMediaTime() - t0, book.sheetCount))
                self.book = book
                self.visibleSheets = (0..<book.sheetCount).filter { (book.sheetInfo($0)?.state ?? 1) == 0 }
                if self.visibleSheets.isEmpty { self.visibleSheets = [0] }
                self.statusLabel.text = nil
                self.loadSheet(self.visibleSheets[0], showBanner: false)
            }
        }
    }

    // MARK: 화면 짜기

    private func buildBars() {
        let barColor = UIColor(red: 0.15, green: 0.17, blue: 0.21, alpha: 1)
        topBar.backgroundColor = barColor
        bottomBar.backgroundColor = barColor
        view.addSubview(topBar)
        view.addSubview(bottomBar)

        // P6: 정보줄 글자 확대 (주소 30pt bold, 값 30pt semibold, 수식 26pt medium)
        addrLabel.font = .monospacedSystemFont(ofSize: 30, weight: .bold)
        addrLabel.textColor = .white
        addrLabel.textAlignment = .center
        addrLabel.backgroundColor = UIColor(red: 0.06, green: 0.49, blue: 0.25, alpha: 1)
        addrLabel.layer.cornerRadius = 8
        addrLabel.clipsToBounds = true
        valueLabel.font = .systemFont(ofSize: 30, weight: .semibold)
        valueLabel.textColor = .white
        formulaLabel.font = .monospacedSystemFont(ofSize: 26, weight: .medium)
        formulaLabel.textColor = UIColor(white: 0.78, alpha: 1)
        [addrLabel, valueLabel, formulaLabel].forEach { topBar.addSubview($0) }

        sheetLabel.font = .systemFont(ofSize: 22, weight: .medium)
        sheetLabel.textColor = .white
        sheetLabel.isHidden = true // P6: ArgoSheetTabStrip 으로 대체하되 기존 속성은 유지
        zoomLabel.font = .monospacedSystemFont(ofSize: 22, weight: .bold)
        zoomLabel.textColor = .systemYellow
        zoomLabel.textAlignment = .right
        [sheetLabel, tabStrip, zoomLabel].forEach { bottomBar.addSubview($0) }

        statusLabel.font = .systemFont(ofSize: 28, weight: .medium)
        statusLabel.textColor = .white
        statusLabel.textAlignment = .center
        statusLabel.numberOfLines = 0
        spinner.color = .white
        view.addSubview(statusLabel)
        view.addSubview(spinner)
    }

    private func buildGrid() {
        grid.clipsToBounds = true
        grid.backgroundColor = .white
        view.addSubview(grid)
        for h in [cornerHost, topHost, leftHost, bodyHost] {
            h.clipsToBounds = true
            h.backgroundColor = .white
            h.isUserInteractionEnabled = false
            grid.addSubview(h)
        }
        for hdr in [cornerHeaderView, colHeaderView, rowHeaderView] {
            hdr.isUserInteractionEnabled = false
            grid.addSubview(hdr)
        }
        for d in [dividerH, dividerV] {
            d.backgroundColor = UIColor(white: 0.45, alpha: 1)
            grid.addSubview(d)
        }
        cursorLayer.borderColor = UIColor(red: 0.06, green: 0.62, blue: 0.30, alpha: 1).cgColor
        cursorLayer.borderWidth = 4
        cursorLayer.zPosition = 500
    }

    private func buildToasts() {
        // 시트 전환 시 중앙 40pt 토스트
        centerToastLabel.font = .systemFont(ofSize: 40, weight: .bold)
        centerToastLabel.textColor = .white
        centerToastLabel.backgroundColor = UIColor(white: 0.08, alpha: 0.88)
        centerToastLabel.textAlignment = .center
        centerToastLabel.layer.cornerRadius = 16
        centerToastLabel.clipsToBounds = true
        centerToastLabel.alpha = 0
        centerToastLabel.isUserInteractionEnabled = false
        view.addSubview(centerToastLabel)

        // 찾기 결과("3 / 17", "찾지 못함") 상단 중앙 배너
        topToastLabel.font = .monospacedSystemFont(ofSize: 28, weight: .bold)
        topToastLabel.textColor = .white
        topToastLabel.backgroundColor = UIColor(red: 0.06, green: 0.49, blue: 0.25, alpha: 0.94)
        topToastLabel.textAlignment = .center
        topToastLabel.layer.cornerRadius = 12
        topToastLabel.clipsToBounds = true
        topToastLabel.alpha = 0
        topToastLabel.isUserInteractionEnabled = false
        view.addSubview(topToastLabel)
    }

    private func applyChromeTheme() {
        let pal = currentPalette
        topBar.backgroundColor = pal.barBackground
        bottomBar.backgroundColor = pal.barBackground
        addrLabel.backgroundColor = pal.addrBadgeBackground
        valueLabel.textColor = pal.primaryText
        formulaLabel.textColor = pal.secondaryText
        grid.backgroundColor = pal.canvasBackground
        for h in [cornerHost, topHost, leftHost, bodyHost] {
            h.backgroundColor = pal.canvasBackground
        }
        dividerH.backgroundColor = pal.dividerColor
        dividerV.backgroundColor = pal.dividerColor
        cursorLayer.borderColor = pal.cursorBorderColor
        for hdr in [cornerHeaderView, colHeaderView, rowHeaderView] {
            hdr.palette = pal
            hdr.setNeedsDisplay()
        }
        updateBottomBar()
    }

    // P6: topBar 높이 76 → 92, bottomBar 높이 60 → 72
    private let topBarH: CGFloat = 92
    private let bottomBarH: CGFloat = 72

    private func layoutAll() {
        let W = view.bounds.width, H = view.bounds.height
        guard W > 0, H > 0 else { return }
        let side: CGFloat = 60
        topBar.frame = CGRect(x: 0, y: 0, width: W, height: topBarH)
        addrLabel.frame = CGRect(x: side, y: 18, width: 196, height: 56)
        valueLabel.frame = CGRect(x: side + 216, y: 18, width: W * 0.37, height: 56)
        let fx = valueLabel.frame.maxX + 20
        formulaLabel.frame = CGRect(x: fx, y: 18, width: max(0, W - fx - side), height: 56)

        bottomBar.frame = CGRect(x: 0, y: H - bottomBarH, width: W, height: bottomBarH)
        let zoomW: CGFloat = 260
        sheetLabel.frame = CGRect(x: side, y: 12, width: W - side * 2 - zoomW - 20, height: 48)
        tabStrip.frame = sheetLabel.frame
        zoomLabel.frame = CGRect(x: W - side - zoomW, y: 12, width: zoomW, height: 48)

        statusLabel.frame = CGRect(x: side, y: H / 2 + 40, width: W - side * 2, height: 120)
        spinner.center = CGPoint(x: W / 2, y: H / 2)

        grid.frame = CGRect(x: 0, y: topBarH, width: W, height: H - topBarH - bottomBarH)
        layoutQuadrants()
    }

    /// 행·열 머리글 크기(화면 pt). 숨김 상태면 (0, 0).
    private var headerInsets: (colH: CGFloat, rowW: CGFloat) {
        guard showHeaders else { return (0, 0) }
        let m = ArgoSheetHeaderView.metrics(zoom: zoom, maxRow: max(1, rowCount))
        return (m.colHeaderH, m.rowHeaderW)
    }

    /// 틀 고정 경계(화면 pt). 머리글을 뺀 본문 가용 영역의 45% 를 넘지 않게 자른다.
    private var freezeSize: CGSize {
        let hi = headerInsets
        let availW = max(0, grid.bounds.width - hi.rowW)
        let availH = max(0, grid.bounds.height - hi.colH)
        let fw = colOffsets[min(freezeCols, colOffsets.count - 1)] * zoom
        let fh = rowOffsets[min(freezeRows, rowOffsets.count - 1)] * zoom
        return CGSize(width: min(fw, availW * 0.45), height: min(fh, availH * 0.45))
    }

    private var freezePt: CGPoint {
        CGPoint(x: colOffsets[min(freezeCols, colOffsets.count - 1)],
                y: rowOffsets[min(freezeRows, rowOffsets.count - 1)])
    }

    private func layoutQuadrants() {
        let G = grid.bounds
        guard G.width > 0, G.height > 0, book != nil else { return }
        let hi = headerInsets
        let hdrW = hi.rowW
        let hdrH = hi.colH
        let contentW = max(0, G.width - hdrW)
        let contentH = max(0, G.height - hdrH)

        let f = freezeSize

        // 머리글 3구역 배치
        cornerHeaderView.isHidden = !showHeaders
        colHeaderView.isHidden = !showHeaders
        rowHeaderView.isHidden = !showHeaders
        if showHeaders {
            cornerHeaderView.frame = CGRect(x: 0, y: 0, width: hdrW, height: hdrH)
            colHeaderView.frame = CGRect(x: hdrW, y: 0, width: contentW, height: hdrH)
            rowHeaderView.frame = CGRect(x: 0, y: hdrH, width: hdrW, height: contentH)
        }

        // 틀 고정 4개 host 를 머리글 너비(hdrW)·높이(hdrH)만큼 오른쪽·아래로 민다.
        cornerHost.frame = CGRect(x: hdrW, y: hdrH, width: f.width, height: f.height)
        topHost.frame = CGRect(x: hdrW + f.width, y: hdrH, width: contentW - f.width, height: f.height)
        leftHost.frame = CGRect(x: hdrW, y: hdrH + f.height, width: f.width, height: contentH - f.height)
        bodyHost.frame = CGRect(x: hdrW + f.width, y: hdrH + f.height, width: contentW - f.width, height: contentH - f.height)

        dividerH.isHidden = f.height <= 0
        dividerV.isHidden = f.width <= 0
        dividerH.frame = CGRect(x: hdrW, y: hdrH + f.height - 1, width: contentW, height: 2)
        dividerV.frame = CGRect(x: hdrW + f.width - 1, y: hdrH, width: 2, height: contentH)

        let totalW = (colOffsets.last ?? 0) * zoom, totalH = (rowOffsets.last ?? 0) * zoom
        let fp = freezePt
        let bodyW = max(0, totalW - fp.x * zoom), bodyH = max(0, totalH - fp.y * zoom)
        // 본문이 화면보다 작으면 스크롤하지 않는다.
        bodyOffset.x = max(0, min(bodyOffset.x, max(0, bodyW - bodyHost.bounds.width)))
        bodyOffset.y = max(0, min(bodyOffset.y, max(0, bodyH - bodyHost.bounds.height)))

        cornerTiles?.frame = CGRect(x: 0, y: 0, width: fp.x * zoom, height: fp.y * zoom)
        topTiles?.frame = CGRect(x: -bodyOffset.x, y: 0, width: bodyW, height: fp.y * zoom)
        leftTiles?.frame = CGRect(x: 0, y: -bodyOffset.y, width: fp.x * zoom, height: bodyH)
        bodyTiles?.frame = CGRect(x: -bodyOffset.x, y: -bodyOffset.y, width: bodyW, height: bodyH)

        refreshTiles()
        refreshHeaders()
        placeCursor()
    }

    private func refreshTiles() {
        for (host, tiles) in [(cornerHost, cornerTiles), (topHost, topTiles), (leftHost, leftTiles), (bodyHost, bodyTiles)] {
            guard let tiles = tiles, host.bounds.width > 0, host.bounds.height > 0 else { continue }
            tiles.updateVisible(host.convert(host.bounds, to: tiles))
        }
    }

    private func refreshHeaders() {
        guard showHeaders else { return }
        let f = freezeSize
        let fp = freezePt
        let pal = currentPalette

        let sr = Int(cursorInfo?.merge_start_row ?? UInt32(cursorRow))
        let er = Int(cursorInfo?.merge_end_row ?? UInt32(cursorRow))
        let sc = Int(cursorInfo?.merge_start_col ?? UInt32(cursorCol))
        let ec = Int(cursorInfo?.merge_end_col ?? UInt32(cursorCol))

        cornerHeaderView.palette = pal
        cornerHeaderView.zoom = zoom
        cornerHeaderView.cornerText = cursorInfo.map { argoSheetString($0.address) } ?? ""
        cornerHeaderView.setNeedsDisplay()

        colHeaderView.palette = pal
        colHeaderView.zoom = zoom
        colHeaderView.offsets = colOffsets
        colHeaderView.freezeCount = freezeCols
        colHeaderView.freezeSpanScreen = f.width
        colHeaderView.freezeOriginPt = fp.x
        colHeaderView.scrollOffsetScreen = bodyOffset.x
        colHeaderView.activeRange = min(sc, ec)...max(sc, ec)
        colHeaderView.setNeedsDisplay()

        rowHeaderView.palette = pal
        rowHeaderView.zoom = zoom
        rowHeaderView.offsets = rowOffsets
        rowHeaderView.freezeCount = freezeRows
        rowHeaderView.freezeSpanScreen = f.height
        rowHeaderView.freezeOriginPt = fp.y
        rowHeaderView.scrollOffsetScreen = bodyOffset.y
        rowHeaderView.activeRange = min(sr, er)...max(sr, er)
        rowHeaderView.setNeedsDisplay()
    }

    // MARK: 시트

    private func loadSheet(_ index: Int, showBanner: Bool = true) {
        guard let book = book, let si = book.sheetInfo(index) else { return }
        sheetIndex = index
        info = si
        stopRepeat()

        var cPtr: UnsafePointer<Float>?, rPtr: UnsafePointer<Float>?
        var nc: UInt32 = 0, nr: UInt32 = 0
        colOffsets = [0]
        rowOffsets = [0]
        if as_sheet_axes(book.ptr, UInt32(index), &cPtr, &nc, &rPtr, &nr), let c = cPtr, let r = rPtr {
            let safeCols = min(Int(nc), 16_384)
            let safeRows = min(Int(nr), 1_048_576)
            var a: CGFloat = 0
            colOffsets.reserveCapacity(safeCols + 1)
            for i in 0..<safeCols { a += CGFloat(max(0, c[i])); colOffsets.append(a) }
            a = 0
            rowOffsets.reserveCapacity(safeRows + 1)
            for i in 0..<safeRows { a += CGFloat(max(0, r[i])); rowOffsets.append(a) }
        }
        freezeCols = si.is_frozen ? Int(si.freeze_cols) : 0
        freezeRows = si.is_frozen ? Int(si.freeze_rows) : 0
        bodyOffset = .zero

        [cornerTiles, topTiles, leftTiles, bodyTiles].forEach { $0?.removeFromSuperview() }
        let fp = freezePt
        let s = UInt32(index)
        let pal = currentPalette
        cornerTiles = ArgoSheetTileView(book: book, sheetIndex: s, sheetOriginPt: .zero, zoom: zoom, palette: pal)
        topTiles = ArgoSheetTileView(book: book, sheetIndex: s, sheetOriginPt: CGPoint(x: fp.x, y: 0), zoom: zoom, palette: pal)
        leftTiles = ArgoSheetTileView(book: book, sheetIndex: s, sheetOriginPt: CGPoint(x: 0, y: fp.y), zoom: zoom, palette: pal)
        bodyTiles = ArgoSheetTileView(book: book, sheetIndex: s, sheetOriginPt: fp, zoom: zoom, palette: pal)
        cornerTiles?.configureOverlays(quadrant: .corner, freezeRows: freezeRows, freezeCols: freezeCols)
        topTiles?.configureOverlays(quadrant: .top, freezeRows: freezeRows, freezeCols: freezeCols)
        leftTiles?.configureOverlays(quadrant: .left, freezeRows: freezeRows, freezeCols: freezeCols)
        bodyTiles?.configureOverlays(quadrant: .body, freezeRows: freezeRows, freezeCols: freezeCols)
        cornerHost.addSubview(cornerTiles!)
        topHost.addSubview(topTiles!)
        leftHost.addSubview(leftTiles!)
        bodyHost.addSubview(bodyTiles!)

        // 틀 고정이 있으면 고정 영역 바로 다음 칸에서 시작한다.
        cursorRow = max(1, Int(si.is_frozen ? si.top_left_row : 1))
        cursorCol = max(1, Int(si.is_frozen ? si.top_left_col : 1))
        updateBottomBar()
        layoutQuadrants()
        syncCursor(ensureVisible: true)

        if showBanner {
            let pos = (visibleSheets.firstIndex(of: index) ?? 0) + 1
            let sName = argoSheetString(si.name)
            showCenterSheetToast("\(pos) / \(visibleSheets.count) · \(sName)")
        }
    }

    private func updateBottomBar() {
        guard let book = book else { return }
        let names = visibleSheets.map { i -> String in
            let n = book.sheetInfo(i).map { argoSheetString($0.name) } ?? "?"
            return i == sheetIndex ? "[\(n)]" : n
        }
        sheetLabel.text = names.joined(separator: "  ·  ")

        let tabItems = visibleSheets.map { i -> ArgoSheetTabStrip.TabItem in
            let n = book.sheetInfo(i).map { argoSheetString($0.name) } ?? "?"
            return ArgoSheetTabStrip.TabItem(index: i, name: n, tabColor: book.sheetTabColor(i))
        }
        tabStrip.update(tabs: tabItems, selectedIndex: sheetIndex, palette: currentPalette)
        zoomLabel.text = L("확대") + " \(Int((zoom * 100).rounded()))%"
    }

    private func showCenterSheetToast(_ text: String) {
        centerToastLabel.text = "  \(text)  "
        let fit = centerToastLabel.sizeThatFits(CGSize(width: view.bounds.width - 160, height: 90))
        let w = min(max(fit.width + 48, 280), view.bounds.width - 120)
        let h: CGFloat = 78
        centerToastLabel.frame = CGRect(x: (view.bounds.width - w) / 2,
                                        y: (view.bounds.height - h) / 2,
                                        width: w, height: h)
        centerToastLabel.layer.removeAllAnimations()
        centerToastLabel.alpha = 1.0
        UIView.animate(withDuration: 0.35, delay: 0.85, options: [.curveEaseOut]) {
            self.centerToastLabel.alpha = 0
        }
    }

    private func showTopFindToast(_ text: String) {
        topToastLabel.text = "  \(text)  "
        let fit = topToastLabel.sizeThatFits(CGSize(width: 600, height: 60))
        let w = min(max(fit.width + 36, 180), 640)
        let h: CGFloat = 52
        topToastLabel.frame = CGRect(x: (view.bounds.width - w) / 2,
                                     y: topBarH + 14,
                                     width: w, height: h)
        topToastLabel.layer.removeAllAnimations()
        topToastLabel.alpha = 1.0
        UIView.animate(withDuration: 0.3, delay: 1.3, options: [.curveEaseOut]) {
            self.topToastLabel.alpha = 0
        }
    }

    // MARK: 커서 및 키보드/리모컨 입력

    private var rowCount: Int { rowOffsets.count - 1 }
    private var colCount: Int { colOffsets.count - 1 }

    private func handlePress(_ type: UIPress.PressType) -> Bool {
        switch type {
        case .upArrow:    startMove(-1, 0); return true
        case .downArrow:  startMove(1, 0); return true
        case .leftArrow:  startMove(0, -1); return true
        case .rightArrow: startMove(0, 1); return true
        case .playPause:  nextSheet(); return true
        case .menu:       close(); return true
        default:          return false
        }
    }

    /// P6: 블루투스 키보드 키 누름 처리 (ArgoSheetKeyMap 과 단일 표 공유)
    private func handleKeyDown(_ key: UIKey) -> Bool {
        guard let action = ArgoSheetKeyMap.match(key: key) else { return false }
        switch action {
        case .moveUp:
            activeHeldKeyCode = key.keyCode
            startMove(-1, 0)
        case .moveDown:
            activeHeldKeyCode = key.keyCode
            startMove(1, 0)
        case .moveLeft:
            activeHeldKeyCode = key.keyCode
            startMove(0, -1)
        case .moveRight:
            activeHeldKeyCode = key.keyCode
            startMove(0, 1)
        case .jumpEdgeUp:
            stopRepeat()
            moveCursorToDataEdge(dr: -1, dc: 0)
        case .jumpEdgeDown:
            stopRepeat()
            moveCursorToDataEdge(dr: 1, dc: 0)
        case .jumpEdgeLeft:
            stopRepeat()
            moveCursorToDataEdge(dr: 0, dc: -1)
        case .jumpEdgeRight:
            stopRepeat()
            moveCursorToDataEdge(dr: 0, dc: 1)
        case .pageUp:
            stopRepeat()
            moveCursorByPage(dRowPages: -1, dColPages: 0)
        case .pageDown:
            stopRepeat()
            moveCursorByPage(dRowPages: 1, dColPages: 0)
        case .pageLeft:
            stopRepeat()
            moveCursorByPage(dRowPages: 0, dColPages: -1)
        case .pageRight:
            stopRepeat()
            moveCursorByPage(dRowPages: 0, dColPages: 1)
        case .rowStart:
            stopRepeat()
            cursorCol = 1
            syncCursor(ensureVisible: true)
        case .sheetStart:
            stopRepeat()
            cursorRow = 1
            cursorCol = 1
            syncCursor(ensureVisible: true)
        case .sheetEnd:
            stopRepeat()
            cursorRow = max(1, min(Int(info.row_count), rowCount))
            cursorCol = max(1, min(Int(info.col_count), colCount))
            syncCursor(ensureVisible: true)
        case .prevSheet:
            stopRepeat()
            prevSheet()
        case .nextSheet:
            stopRepeat()
            nextSheet()
        case .goToCell(let initialText):
            stopRepeat()
            promptGoToCell(prefill: initialText)
        case .zoomIn:
            stopRepeat()
            stepZoom(delta: 1)
        case .zoomOut:
            stopRepeat()
            stepZoom(delta: -1)
        case .zoom100:
            stopRepeat()
            setZoom(1.0)
        case .findPrompt:
            stopRepeat()
            promptFind()
        case .findNext:
            stopRepeat()
            stepFindResult(forward: true)
        case .findPrev:
            stopRepeat()
            stepFindResult(forward: false)
        case .quickMenu:
            stopRepeat()
            showCellSheet()
        case .toggleDarkMode:
            stopRepeat()
            toggleDarkMode()
        case .toggleHeaders:
            stopRepeat()
            toggleHeaders()
        case .toggleCross:
            stopRepeat()
            toggleCross()
        case .showHelp:
            stopRepeat()
            showHelpModal()
        case .closeOrEscape:
            stopRepeat()
            if let presented = presentedViewController {
                presented.dismiss(animated: true)
            } else {
                close()
            }
        }
        return true
    }

    private func handleKeyUp(_ key: UIKey) {
        if activeHeldKeyCode == key.keyCode {
            activeHeldKeyCode = nil
            stopRepeat()
        }
    }

    private func startMove(_ dr: Int, _ dc: Int) {
        guard book != nil else { return }
        moveCursor(dr, dc)
        repeatTimer?.invalidate()
        repeatDir = (dr, dc)
        repeatCount = 0
        // 0.4초 누르고 있으면 반복, 오래 누를수록 빨라진다.
        repeatTimer = Timer.scheduledTimer(withTimeInterval: 0.4, repeats: false) { [weak self] _ in
            self?.repeatTick()
        }
    }

    private func repeatTick() {
        repeatCount += 1
        let step = repeatCount > 30 ? 5 : (repeatCount > 12 ? 2 : 1)
        moveCursor(repeatDir.0 * step, repeatDir.1 * step)
        let interval = repeatCount > 12 ? 0.05 : 0.09
        repeatTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: false) { [weak self] _ in
            self?.repeatTick()
        }
    }

    private func stopRepeat() {
        repeatTimer?.invalidate()
        repeatTimer = nil
    }

    private func moveCursor(_ dr: Int, _ dc: Int) {
        guard rowCount > 0, colCount > 0 else { return }
        var r = cursorRow, c = cursorCol
        // 병합 칸이면 병합의 바깥 경계에서 출발한다.
        if let ci = cursorInfo {
            if dr > 0 { r = Int(ci.merge_end_row) } else if dr < 0 { r = Int(ci.merge_start_row) }
            if dc > 0 { c = Int(ci.merge_end_col) } else if dc < 0 { c = Int(ci.merge_start_col) }
        }
        r = min(max(1, r + dr), rowCount)
        c = min(max(1, c + dc), colCount)
        // 숨은 행·열(높이·너비 0)은 건너뛴다. 끝까지 숨었으면 제자리.
        let rs = dr >= 0 ? 1 : -1, cs = dc >= 0 ? 1 : -1
        var guardN = 0
        let maxRGuard = min(rowCount, 1_048_576)
        while rowOffsets[r] - rowOffsets[r - 1] <= 0, guardN < maxRGuard {
            let n = r + rs
            if n < 1 || n > rowCount { break }
            r = n; guardN += 1
        }
        guardN = 0
        let maxCGuard = min(colCount, 16_384)
        while colOffsets[c] - colOffsets[c - 1] <= 0, guardN < maxCGuard {
            let n = c + cs
            if n < 1 || n > colCount { break }
            c = n; guardN += 1
        }
        cursorRow = r
        cursorCol = c
        syncCursor(ensureVisible: true)
    }

    /// P6: 엑셀 Ctrl+화살표 규칙(빈 칸 / 찬 칸 경계까지 이동).
    /// 다음 칸이 비었는지는 `as_cell_info`의 `kind != 0` 여부로 판정하며, 상한은 현재 시트 `rowCount`/`colCount`로 제한한다.
    private func moveCursorToDataEdge(dr: Int, dc: Int) {
        guard let book = book, rowCount > 0, colCount > 0 else { return }
        let maxR = min(rowCount, 1_048_576)
        let maxC = min(colCount, 16_384)

        func isFilled(_ r: Int, _ c: Int) -> Bool {
            guard let ci = book.cellInfo(sheet: sheetIndex, row: r, col: c) else { return false }
            return ci.kind != 0
        }

        var r = cursorRow
        var c = cursorCol
        if let ci = cursorInfo {
            if dr > 0 { r = Int(ci.merge_end_row) } else if dr < 0 { r = Int(ci.merge_start_row) }
            if dc > 0 { c = Int(ci.merge_end_col) } else if dc < 0 { c = Int(ci.merge_start_col) }
        }

        let nr = r + dr
        let nc = c + dc
        if nr < 1 || nr > maxR || nc < 1 || nc > maxC { return }

        let curFilled = isFilled(cursorRow, cursorCol)
        let nextFilled = isFilled(nr, nc)

        let limit = dr != 0 ? maxR : maxC
        var steps = 0

        if curFilled && nextFilled {
            // 찬 칸 구간 끝(마지막 찬 칸)까지 전진
            r = nr; c = nc
            while steps < limit {
                steps += 1
                let tr = r + dr, tc = c + dc
                if tr < 1 || tr > maxR || tc < 1 || tc > maxC { break }
                if !isFilled(tr, tc) { break }
                r = tr; c = tc
            }
        } else {
            // 빈 칸을 건너뛰어 첫 번째 찬 칸(없으면 시트 끝)까지 전진
            r = nr; c = nc
            while steps < limit && !isFilled(r, c) {
                steps += 1
                let tr = r + dr, tc = c + dc
                if tr < 1 || tr > maxR || tc < 1 || tc > maxC { break }
                r = tr; c = tc
            }
        }

        cursorRow = min(max(1, r), maxR)
        cursorCol = min(max(1, c), maxC)
        syncCursor(ensureVisible: true)
    }

    /// P6: 한 화면 위·아래·왼쪽·오른쪽 이동
    private func moveCursorByPage(dRowPages: Int, dColPages: Int) {
        guard rowCount > 0, colCount > 0 else { return }
        if dRowPages != 0 {
            let pagePt = max(60, bodyHost.bounds.height / max(zoom, 0.25))
            let curY = rowOffsets[min(cursorRow - 1, rowOffsets.count - 1)]
            let targetY = max(0, curY + CGFloat(dRowPages) * pagePt)
            let newR = ArgoSheetHeaderView.binarySearchIndex(offsets: rowOffsets, targetPt: targetY)
            cursorRow = min(max(1, newR), rowCount)
        }
        if dColPages != 0 {
            let pagePt = max(120, bodyHost.bounds.width / max(zoom, 0.25))
            let curX = colOffsets[min(cursorCol - 1, colOffsets.count - 1)]
            let targetX = max(0, curX + CGFloat(dColPages) * pagePt)
            let newC = ArgoSheetHeaderView.binarySearchIndex(offsets: colOffsets, targetPt: targetX)
            cursorCol = min(max(1, newC), colCount)
        }
        syncCursor(ensureVisible: true)
    }

    private func syncCursor(ensureVisible: Bool) {
        guard let book = book, let ci = book.cellInfo(sheet: sheetIndex, row: cursorRow, col: cursorCol) else { return }
        cursorInfo = ci
        cursorRow = Int(ci.merge_start_row)
        cursorCol = Int(ci.merge_start_col)

        addrLabel.text = argoSheetString(ci.address)
        let v = argoSheetString(ci.display_text)
        valueLabel.text = v.isEmpty ? L("(빈 칸)") : v
        let f = argoSheetString(ci.formula)
        formulaLabel.text = f.isEmpty ? "" : "=" + f

        if ensureVisible { scrollToCursor() }
        refreshHeaders()
        placeCursor()
    }

    /// 커서 칸의 시트 pt 사각형.
    private func cursorSheetRect() -> CGRect? {
        guard let ci = cursorInfo, rowCount > 0, colCount > 0 else { return nil }
        let sr = min(max(1, Int(ci.merge_start_row)), rowCount)
        let sc = min(max(1, Int(ci.merge_start_col)), colCount)
        let er = min(max(sr, Int(ci.merge_end_row)), rowCount)
        let ec = min(max(sc, Int(ci.merge_end_col)), colCount)
        return CGRect(x: colOffsets[sc - 1], y: rowOffsets[sr - 1],
                      width: colOffsets[ec] - colOffsets[sc - 1], height: rowOffsets[er] - rowOffsets[sr - 1])
    }

    private func scrollToCursor() {
        guard let r = cursorSheetRect() else { return }
        let fp = freezePt
        let inFrozenCol = cursorCol <= freezeCols, inFrozenRow = cursorRow <= freezeRows
        let vw = bodyHost.bounds.width, vh = bodyHost.bounds.height
        var o = bodyOffset
        if !inFrozenCol, vw > 0 {
            let minX = (r.minX - fp.x) * zoom, maxX = (r.maxX - fp.x) * zoom
            if minX < o.x { o.x = max(0, minX - 24) } else if maxX > o.x + vw { o.x = maxX - vw + 24 }
        }
        if !inFrozenRow, vh > 0 {
            let minY = (r.minY - fp.y) * zoom, maxY = (r.maxY - fp.y) * zoom
            if minY < o.y { o.y = max(0, minY - 24) } else if maxY > o.y + vh { o.y = maxY - vh + 24 }
        }
        if o != bodyOffset {
            bodyOffset = o
            layoutQuadrants()
        }
    }

    private func placeCursor() {
        guard let r = cursorSheetRect() else {
            cursorLayer.removeFromSuperlayer()
            for t in [cornerTiles, topTiles, leftTiles, bodyTiles] { t?.updateCrossHair(cursorSheetRect: nil) }
            return
        }
        let fp = freezePt
        let inFrozenCol = cursorCol <= freezeCols, inFrozenRow = cursorRow <= freezeRows
        let host: ArgoSheetTileView?
        let origin: CGPoint
        switch (inFrozenRow, inFrozenCol) {
        case (true, true):  host = cornerTiles; origin = .zero
        case (true, false): host = topTiles; origin = CGPoint(x: fp.x, y: 0)
        case (false, true): host = leftTiles; origin = CGPoint(x: 0, y: fp.y)
        default:            host = bodyTiles; origin = fp
        }
        guard let h = host else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if cursorLayer.superlayer !== h.layer {
            cursorLayer.removeFromSuperlayer()
            h.layer.addSublayer(cursorLayer)
        }
        cursorLayer.frame = CGRect(x: (r.minX - origin.x) * zoom, y: (r.minY - origin.y) * zoom,
                                   width: r.width * zoom, height: r.height * zoom).insetBy(dx: -2, dy: -2)
        CATransaction.commit()

        // P6: 4개 구역의 십자 강조 띠 프레임만 갱신(타일 재렌더링 없음)
        let crossRect: CGRect? = showCross ? r : nil
        for t in [cornerTiles, topTiles, leftTiles, bodyTiles] {
            t?.updateCrossHair(cursorSheetRect: crossRect)
        }
    }

    // MARK: 시트 전환 · 확대/축소 · 설정 토글

    private func nextSheet() {
        guard visibleSheets.count > 1, let i = visibleSheets.firstIndex(of: sheetIndex) else { return }
        loadSheet(visibleSheets[(i + 1) % visibleSheets.count], showBanner: true)
    }

    private func prevSheet() {
        guard visibleSheets.count > 1, let i = visibleSheets.firstIndex(of: sheetIndex) else { return }
        let prevIdx = (i - 1 + visibleSheets.count) % visibleSheets.count
        loadSheet(visibleSheets[prevIdx], showBanner: true)
    }

    private func stepZoom(delta: Int) {
        let curIdx = zoomLevels.enumerated().min(by: { abs($0.element - zoom) < abs($1.element - zoom) })?.offset ?? 3
        let nextIdx = min(max(0, curIdx + delta), zoomLevels.count - 1)
        setZoom(zoomLevels[nextIdx])
    }

    private func setZoom(_ z: CGFloat) {
        guard z != zoom, book != nil else { return }
        zoom = z
        UserDefaults.settings.set(Double(z), forKey: "sheetZoom")
        reloadTilesWithCurrentPalette()
    }

    private func reloadTilesWithCurrentPalette() {
        let fp = freezePt
        let pal = currentPalette
        cornerTiles?.reset(zoom: zoom, sheetOriginPt: .zero, palette: pal)
        topTiles?.reset(zoom: zoom, sheetOriginPt: CGPoint(x: fp.x, y: 0), palette: pal)
        leftTiles?.reset(zoom: zoom, sheetOriginPt: CGPoint(x: 0, y: fp.y), palette: pal)
        bodyTiles?.reset(zoom: zoom, sheetOriginPt: fp, palette: pal)
        applyChromeTheme()
        layoutQuadrants()
        syncCursor(ensureVisible: true)
    }

    private func toggleDarkMode() {
        isDarkMode.toggle()
        UserDefaults.settings.set(isDarkMode, forKey: "sheetDarkMode")
        reloadTilesWithCurrentPalette()
    }

    private func toggleHeaders() {
        showHeaders.toggle()
        UserDefaults.settings.set(showHeaders, forKey: "sheetHeaders")
        layoutQuadrants()
        syncCursor(ensureVisible: true)
    }

    private func toggleCross() {
        showCross.toggle()
        UserDefaults.settings.set(showCross, forKey: "sheetCross")
        placeCursor()
    }

    private func setTextWeight(_ w: Int) {
        let clamped = min(max(w, 0), 2)
        guard clamped != textWeight else { return }
        textWeight = clamped
        UserDefaults.settings.set(clamped, forKey: "sheetTextWeight")
        reloadTilesWithCurrentPalette()
    }

    private func toggleMinFont14() {
        minFont14.toggle()
        UserDefaults.settings.set(minFont14, forKey: "sheetMinFont")
        reloadTilesWithCurrentPalette()
    }

    // MARK: 퀵 메뉴 · 서브 메뉴 · 셀 주소 이동 · 찾기 · 단축키 설명서

    /// 선택 버튼(리모컨 Select, 키보드 Return, M): 셀 정보와 P6 퀵 메뉴 전체.
    private func showCellSheet() {
        stopRepeat()
        guard book != nil else { return }
        var msg = ""
        if let ci = cursorInfo {
            let v = argoSheetString(ci.display_text)
            let f = argoSheetString(ci.formula)
            let fmt = argoSheetString(ci.format_code)
            let memo = argoSheetString(ci.comment_text)
            msg = L("값") + ": " + (v.isEmpty ? L("(빈 칸)") : v)
            if !f.isEmpty { msg += "\n" + L("수식") + ": =" + f }
            if !fmt.isEmpty && fmt != "General" { msg += "\n" + L("서식") + ": " + fmt }
            let author = argoSheetString(ci.comment_author)
            if !memo.isEmpty {
                msg += "\n\n" + L("메모") + (author.isEmpty ? "" : " (\(author))") + "\n" + memo
            }
            let link = argoSheetString(ci.link_target)
            if !link.isEmpty { msg += "\n\n" + L("링크") + ": " + link }
        }
        let title = cursorInfo.map { argoSheetString($0.address) } ?? ""
        let a = UIAlertController(title: title, message: msg, preferredStyle: .alert)

        // 1. [링크로 이동 / 링크 QR 보기(있을 때)]
        if let ci = cursorInfo {
            let link = argoSheetString(ci.link_target)
            if !link.isEmpty {
                if ci.link_is_internal {
                    a.addAction(UIAlertAction(title: L("링크로 이동"), style: .default) { [weak self] _ in self?.jump(to: link) })
                } else {
                    a.addAction(UIAlertAction(title: L("링크 QR 보기"), style: .default) { [weak self] _ in self?.showQR(link) })
                }
            }
        }

        // 2. 셀 주소로 이동
        a.addAction(UIAlertAction(title: L("셀 주소로 이동"), style: .default) { [weak self] _ in
            self?.promptGoToCell(prefill: "")
        })

        // 3. 찾기
        a.addAction(UIAlertAction(title: L("찾기"), style: .default) { [weak self] _ in
            self?.promptFind()
        })

        // 4. 시트 목록…
        if visibleSheets.count > 1 {
            a.addAction(UIAlertAction(title: L("시트 목록…"), style: .default) { [weak self] _ in
                self?.showSheetListPicker()
            })
        }

        // 5. 확대 n% · 축소 n%
        if let i = zoomLevels.firstIndex(of: zoom) {
            if i + 1 < zoomLevels.count {
                let z = zoomLevels[i + 1]
                a.addAction(UIAlertAction(title: L("확대") + " \(Int((z * 100).rounded()))%", style: .default) { [weak self] _ in self?.setZoom(z) })
            }
            if i > 0 {
                let z = zoomLevels[i - 1]
                a.addAction(UIAlertAction(title: L("축소") + " \(Int((z * 100).rounded()))%", style: .default) { [weak self] _ in self?.setZoom(z) })
            }
        }

        // 6. 다크모드 켜기/끄기
        let darkTitle = isDarkMode ? L("다크모드 끄기") : L("다크모드 켜기")
        a.addAction(UIAlertAction(title: darkTitle, style: .default) { [weak self] _ in
            self?.toggleDarkMode()
        })

        // 7. 머리글 보이기/숨기기
        let hdrTitle = showHeaders ? L("머리글 숨기기") : L("머리글 보이기")
        a.addAction(UIAlertAction(title: hdrTitle, style: .default) { [weak self] _ in
            self?.toggleHeaders()
        })

        // 8. 십자 강조 켜기/끄기
        let crossTitle = showCross ? L("십자 강조 끄기") : L("십자 강조 켜기")
        a.addAction(UIAlertAction(title: crossTitle, style: .default) { [weak self] _ in
            self?.toggleCross()
        })

        // 9. 글자 굵기…
        a.addAction(UIAlertAction(title: L("글자 굵기…"), style: .default) { [weak self] _ in
            self?.showTextWeightPicker()
        })

        // 10. 단축키 설명서
        a.addAction(UIAlertAction(title: L("단축키 설명서"), style: .default) { [weak self] _ in
            self?.showHelpModal()
        })

        // 11. 닫기
        a.addAction(UIAlertAction(title: L("닫기"), style: .cancel))
        present(a, animated: true)
    }

    private func showSheetListPicker() {
        guard let book = book else { return }
        let a = UIAlertController(title: L("시트 목록"), message: nil, preferredStyle: .alert)
        for idx in visibleSheets {
            let name = book.sheetInfo(idx).map { argoSheetString($0.name) } ?? "?"
            let title = (idx == sheetIndex ? "✓ " : "") + name
            a.addAction(UIAlertAction(title: title, style: .default) { [weak self] _ in
                self?.loadSheet(idx, showBanner: true)
            })
        }
        a.addAction(UIAlertAction(title: L("취소"), style: .cancel))
        present(a, animated: true)
    }

    private func showTextWeightPicker() {
        let a = UIAlertController(title: L("글자 굵기"), message: nil, preferredStyle: .alert)
        let options: [(Int, String)] = [
            (0, L("보통")),
            (1, L("굵게")),
            (2, L("아주 굵게"))
        ]
        for (w, label) in options {
            let prefix = (textWeight == w) ? "✓ " : ""
            a.addAction(UIAlertAction(title: prefix + label, style: .default) { [weak self] _ in
                self?.setTextWeight(w)
            })
        }
        let minFontTitle = (minFont14 ? "✓ " : "") + L("작은 글자 14pt 이상으로 키우기")
        a.addAction(UIAlertAction(title: minFontTitle, style: .default) { [weak self] _ in
            self?.toggleMinFont14()
        })
        a.addAction(UIAlertAction(title: L("취소"), style: .cancel))
        present(a, animated: true)
    }

    private func showHelpModal() {
        let vc = ArgoSheetHelpController(palette: currentPalette)
        present(vc, animated: true)
    }

    /// P6: 셀 주소 입력창 ("B12", "12"(현재 열의 12행), "C"(현재 행의 C열), "시트이름!B3", "'시트 이름'!B3")
    private func promptGoToCell(prefill: String) {
        let a = UIAlertController(title: L("셀 주소로 이동"),
                                  message: L("예: B12, 12(행만), C(열만), 시트!B3"),
                                  preferredStyle: .alert)
        a.addTextField { tf in
            tf.placeholder = "B12"
            tf.text = prefill
            tf.autocapitalizationType = .allCharacters
            tf.autocorrectionType = .no
        }
        a.addAction(UIAlertAction(title: L("이동"), style: .default) { [weak self, weak a] _ in
            guard let self = self, let raw = a?.textFields?.first?.text?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !raw.isEmpty else { return }
            if !self.jumpWithShorthand(raw) {
                self.showTopFindToast(L("잘못된 셀 주소입니다"))
            }
        })
        a.addAction(UIAlertAction(title: L("취소"), style: .cancel))
        present(a, animated: true)
    }

    /// 숫자만("12" -> 현재 열의 12행) 또는 영문자만("C" -> 현재 행의 C열)까지 확장해 jump(to:)로 연결한다.
    @discardableResult
    private func jumpWithShorthand(_ input: String) -> Bool {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return false }

        if !trimmed.contains("!") {
            if trimmed.allSatisfy({ $0.isNumber }) {
                let colName = ArgoSheetHeaderView.columnLetters(for: cursorCol)
                return jump(to: "\(colName)\(trimmed)")
            }
            if trimmed.allSatisfy({ $0.isASCII && $0.isLetter }) {
                return jump(to: "\(trimmed.uppercased())\(cursorRow)")
            }
        }
        return jump(to: trimmed)
    }

    /// P6: 찾기 입력창 -> `as_find(book, text, hits, 500)` 실행 후 현재 커서 뒤의 첫 결과로 이동
    private func promptFind() {
        let a = UIAlertController(title: L("찾기"), message: nil, preferredStyle: .alert)
        a.addTextField { [weak self] tf in
            tf.placeholder = L("찾을 글자")
            tf.text = self?.searchQuery ?? ""
        }
        a.addAction(UIAlertAction(title: L("찾기"), style: .default) { [weak self, weak a] _ in
            guard let self = self,
                  let q = a?.textFields?.first?.text?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !q.isEmpty else { return }
            self.executeFind(query: q)
        })
        a.addAction(UIAlertAction(title: L("취소"), style: .cancel))
        present(a, animated: true)
    }

    private func executeFind(query: String) {
        guard let book = book else { return }
        searchQuery = query
        var rawHits = [ASHit](repeating: ASHit(sheet: 0, row: 0, col: 0), count: 500)
        let count: UInt32 = query.withCString { cStr in
            rawHits.withUnsafeMutableBufferPointer { buf in
                as_find(book.ptr, cStr, buf.baseAddress, 500)
            }
        }
        let n = min(Int(count), 500)
        guard n > 0 else {
            searchHits.removeAll()
            searchHitIndex = -1
            showTopFindToast(L("찾지 못함"))
            return
        }
        searchHits = Array(rawHits.prefix(n))

        // 현재 (시트, 행, 열) 뒤의 첫 번째 결과를 찾고, 없으면 0번 결과로 간다.
        let curTuple = (UInt32(sheetIndex), UInt32(cursorRow), UInt32(cursorCol))
        if let nextIdx = searchHits.firstIndex(where: { ($0.sheet, $0.row, $0.col) > curTuple }) {
            searchHitIndex = nextIdx
        } else {
            searchHitIndex = 0
        }
        jumpToSearchHit(at: searchHitIndex)
    }

    private func stepFindResult(forward: Bool) {
        guard !searchHits.isEmpty else {
            promptFind()
            return
        }
        if forward {
            searchHitIndex = (searchHitIndex + 1) % searchHits.count
        } else {
            searchHitIndex = (searchHitIndex - 1 + searchHits.count) % searchHits.count
        }
        jumpToSearchHit(at: searchHitIndex)
    }

    private func jumpToSearchHit(at idx: Int) {
        guard idx >= 0, idx < searchHits.count else { return }
        let hit = searchHits[idx]
        let destSheet = Int(hit.sheet)
        if destSheet != sheetIndex {
            if !visibleSheets.contains(destSheet) {
                visibleSheets.append(destSheet)
                visibleSheets.sort()
            }
            loadSheet(destSheet, showBanner: false)
        }
        cursorRow = min(max(1, Int(hit.row)), max(1, rowCount))
        cursorCol = min(max(1, Int(hit.col)), max(1, colCount))
        syncCursor(ensureVisible: true)
        showTopFindToast("\(idx + 1) / \(searchHits.count)")
    }

    /// 내부 링크 "'시트 이름'!B3" · "시트!B3" · "B3"(같은 시트) 로 간다. 성공 여부를 돌려준다.
    @discardableResult
    private func jump(to target: String) -> Bool {
        guard let book = book else { return false }
        var sheetName = "", cell = target
        if let bang = target.range(of: "!", options: .backwards) {
            sheetName = String(target[..<bang.lowerBound])
            cell = String(target[bang.upperBound...])
            if sheetName.hasPrefix("'") && sheetName.hasSuffix("'") && sheetName.count >= 2 {
                sheetName = String(sheetName.dropFirst().dropLast()).replacingOccurrences(of: "''", with: "'")
            }
        }
        var destSheet = sheetIndex
        if !sheetName.isEmpty {
            guard let i = (0..<book.sheetCount).first(where: {
                book.sheetInfo($0).map { argoSheetString($0.name).caseInsensitiveCompare(sheetName) == .orderedSame } ?? false
            }) else { return false }
            destSheet = i
        }
        // 주소 "B3" 또는 "$B$3" (범위면 앞 칸)
        let first = cell.split(separator: ":").first.map(String.init) ?? cell
        var letters = 0, digits = ""
        for ch in first.uppercased() where ch != "$" {
            if let a = ch.asciiValue, a >= 65, a <= 90, digits.isEmpty {
                letters = letters * 26 + Int(a - 64)
                if letters > 16384 { return false }
            } else if ch.isNumber {
                digits.append(ch)
            } else {
                return false
            }
        }
        guard let r = Int(digits.prefix(7)), r >= 1, r <= 1_048_576, letters >= 1 else {
            return false
        }
        if destSheet != sheetIndex {
            if !visibleSheets.contains(destSheet) { visibleSheets.append(destSheet); visibleSheets.sort() }
            loadSheet(destSheet, showBanner: true)
        }
        cursorRow = min(r, max(1, rowCount))
        cursorCol = min(letters, max(1, colCount))
        syncCursor(ensureVisible: true)
        return true
    }

    /// 바깥 링크: TV 에는 브라우저가 없으니 QR 로 보여 준다(휴대폰으로 찍어서 연다).
    private func showQR(_ link: String) {
        let vc = UIViewController()
        vc.view.backgroundColor = UIColor(white: 0, alpha: 0.85)
        let iv = UIImageView()
        iv.contentMode = .scaleAspectFit
        iv.layer.magnificationFilter = .nearest
        if let f = CIFilter(name: "CIQRCodeGenerator") {
            f.setValue(Data(link.utf8), forKey: "inputMessage")
            f.setValue("M", forKey: "inputCorrectionLevel")
            if let out = f.outputImage?.transformed(by: CGAffineTransform(scaleX: 12, y: 12)),
               let cg = CIContext().createCGImage(out, from: out.extent) {
                iv.image = UIImage(cgImage: cg)
            }
        }
        let label = UILabel()
        label.text = link
        label.textColor = .white
        label.font = .systemFont(ofSize: 26)
        label.textAlignment = .center
        label.numberOfLines = 2
        vc.view.addSubview(iv)
        vc.view.addSubview(label)
        iv.translatesAutoresizingMaskIntoConstraints = false
        label.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            iv.centerXAnchor.constraint(equalTo: vc.view.centerXAnchor),
            iv.centerYAnchor.constraint(equalTo: vc.view.centerYAnchor, constant: -40),
            iv.widthAnchor.constraint(equalToConstant: 560),
            iv.heightAnchor.constraint(equalToConstant: 560),
            label.topAnchor.constraint(equalTo: iv.bottomAnchor, constant: 30),
            label.leadingAnchor.constraint(equalTo: vc.view.leadingAnchor, constant: 80),
            label.trailingAnchor.constraint(equalTo: vc.view.trailingAnchor, constant: -80),
        ])
        // 포커스를 받는 투명 판을 깐다. 포커스 대상이 없으면 입력이 아래 엑셀 화면으로 가서
        // 메뉴 버튼이 QR 이 아니라 뷰어 전체를 닫는다.
        let catcher = ArgoFocusCanvas()
        catcher.frame = vc.view.bounds
        catcher.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        catcher.backgroundColor = .clear
        catcher.onSelect = { [weak vc] in vc?.dismiss(animated: true) }
        catcher.onPress = { [weak vc] _ in vc?.dismiss(animated: true); return true }
        catcher.onKey = { [weak vc] _ in vc?.dismiss(animated: true); return true }
        vc.view.addSubview(catcher)
        vc.modalPresentationStyle = .overFullScreen
        present(vc, animated: true)
    }

    private func close() {
        stopRepeat()
        if let onClose = onClose { onClose() } else { dismiss(animated: true) }
    }
}

