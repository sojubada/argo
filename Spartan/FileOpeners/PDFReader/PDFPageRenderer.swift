//
//  PDFPageRenderer.swift
//  Argo
//

import Foundation
import CoreGraphics
import UIKit

final class PDFPageRenderer {
    static let shared = PDFPageRenderer()

    private let cache = NSCache<NSString, UIImage>()
    // Serial on purpose. renderDetached() carries no cancellation token, so a
    // fast scroll through a long document would otherwise pile up unbounded
    // concurrent rasterisations. Serialised, a stale renderPage() request is
    // dropped by its token check the moment it is dequeued.
    private let renderQueue = DispatchQueue(label: "com.sojubada.argo.pdfrenderer", qos: .userInitiated)
    private let lock = NSLock()
    private var activeTokens: [String: Int] = [:]

    private init() {
        // 현재 페이지 ±1 및 두 쪽 보기 대응을 위해 캐시 개수를 5개로 제한
        cache.countLimit = 5
    }

    func clearCache() {
        cache.removeAllObjects()
    }

    @discardableResult
    func nextToken(for slot: String) -> Int {
        lock.lock()
        defer { lock.unlock() }
        let next = (activeTokens[slot] ?? 0) + 1
        activeTokens[slot] = next
        return next
    }

    func isCurrent(token: Int, for slot: String) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return activeTokens[slot] == token
    }

    /// slot 별로 최신 요청만 유효하게 처리하는 비동기 렌더링
    func renderPage(
        document: CGPDFDocument,
        docKey: String,
        pageIndex: Int, // 0-based
        targetSize: CGSize,
        slot: String,
        useCache: Bool = true,
        completion: @escaping (UIImage?) -> Void
    ) {
        let token = nextToken(for: slot)
        let clampedTarget = Self.clampSize(targetSize, maxDimension: 4096)
        let cacheKey = "\(docKey)_p\(pageIndex)_\(Int(clampedTarget.width))x\(Int(clampedTarget.height))" as NSString

        if useCache, let cached = cache.object(forKey: cacheKey) {
            DispatchQueue.main.async { [weak self] in
                guard let self = self, self.isCurrent(token: token, for: slot) else { return }
                completion(cached)
            }
            return
        }

        renderQueue.async { [weak self] in
            guard let self = self else { return }
            guard self.isCurrent(token: token, for: slot) else { return }

            let image = Self.rasterizePage(
                document: document,
                pageNumber: pageIndex + 1, // CGPDFDocument는 1부터 시작
                targetSize: clampedTarget
            )

            guard self.isCurrent(token: token, for: slot) else { return }

            if useCache, let img = image {
                self.cache.setObject(img, forKey: cacheKey)
            }

            DispatchQueue.main.async { [weak self] in
                guard let self = self, self.isCurrent(token: token, for: slot) else { return }
                completion(image)
            }
        }
    }

    /// 현재 페이지 주변(±1)을 백그라운드에서 미리 캐시에 적재
    func prefetchAdjacent(
        document: CGPDFDocument,
        docKey: String,
        around pageIndex: Int,
        totalPages: Int,
        targetSize: CGSize
    ) {
        let clampedTarget = Self.clampSize(targetSize, maxDimension: 4096)
        let candidates = [pageIndex - 1, pageIndex + 1].filter { $0 >= 0 && $0 < totalPages }

        for idx in candidates {
            let cacheKey = "\(docKey)_p\(idx)_\(Int(clampedTarget.width))x\(Int(clampedTarget.height))" as NSString
            if cache.object(forKey: cacheKey) != nil {
                continue
            }
            renderQueue.async { [weak self] in
                guard let self = self else { return }
                if self.cache.object(forKey: cacheKey) != nil { return }
                if let img = Self.rasterizePage(document: document, pageNumber: idx + 1, targetSize: clampedTarget) {
                    self.cache.setObject(img, forKey: cacheKey)
                }
            }
        }
    }

    /// 썸네일·연속 스크롤 셀용 비동기 렌더링 (캐시 오염 방지를 위해 기본값 useCache = false)
    func renderDetached(
        document: CGPDFDocument,
        pageIndex: Int, // 0-based
        targetSize: CGSize,
        completion: @escaping (UIImage?) -> Void
    ) {
        let clampedTarget = Self.clampSize(targetSize, maxDimension: 2048)
        renderQueue.async {
            let image = Self.rasterizePage(
                document: document,
                pageNumber: pageIndex + 1,
                targetSize: clampedTarget
            )
            DispatchQueue.main.async {
                completion(image)
            }
        }
    }

    /// CGPDFProbe.probeRender 검증 문법을 그대로 따른 코어 래스터라이즈 함수
    static func rasterizePage(
        document: CGPDFDocument,
        pageNumber: Int, // 1-based
        targetSize: CGSize
    ) -> UIImage? {
        guard let page = document.page(at: pageNumber) else { return nil }
        var box = page.getBoxRect(.cropBox)
        if box.isEmpty {
            box = page.getBoxRect(.mediaBox)
        }
        guard box.width > 0, box.height > 0 else { return nil }

        let rotation = page.rotationAngle % 360
        let isSideways = (rotation == 90 || rotation == 270 || rotation == -90 || rotation == -270)
        let effectiveWidth = isSideways ? box.height : box.width
        let effectiveHeight = isSideways ? box.width : box.height

        let scale = min(targetSize.width / effectiveWidth, targetSize.height / effectiveHeight)
        guard scale > 0 else { return nil }

        let pixelSize = CGSize(
            width: max(1, floor(effectiveWidth * scale)),
            height: max(1, floor(effectiveHeight * scale))
        )

        let format = UIGraphicsImageRendererFormat.default()
        format.opaque = true
        format.scale = 1.0
        let renderer = UIGraphicsImageRenderer(size: pixelSize, format: format)

        return renderer.image { ctx in
            let cg = ctx.cgContext
            cg.setFillColor(UIColor.white.cgColor)
            cg.fill(CGRect(origin: .zero, size: pixelSize))

            // PDF 원점은 좌하단, UIKit 원점은 좌상단
            cg.translateBy(x: 0, y: pixelSize.height)
            cg.scaleBy(x: scale, y: -scale)

            let destRect = CGRect(x: 0, y: 0, width: effectiveWidth, height: effectiveHeight)
            let transform = page.getDrawingTransform(
                .cropBox,
                rect: destRect,
                rotate: 0,
                preserveAspectRatio: true
            )
            cg.concatenate(transform)
            cg.drawPDFPage(page)
        }
    }

    private static func clampSize(_ size: CGSize, maxDimension: CGFloat) -> CGSize {
        guard size.width > 0, size.height > 0 else {
            return CGSize(width: 1920, height: 1080)
        }
        let maxSide = max(size.width, size.height)
        if maxSide <= maxDimension {
            return size
        }
        let factor = maxDimension / maxSide
        return CGSize(width: floor(size.width * factor), height: floor(size.height * factor))
    }
}