//
//  DocPipelineProbe.swift
//  Argo
//
//  COMPILE-ONLY PROBE. Nothing calls these functions.
//  Their only job is to prove, on the real tvOS SDK in CI, that every
//  API the document-to-PDF pipeline is planned around actually exists
//  before a line of the feature is specified. PDFKit taught us that
//  Apple's documentation is not evidence; a green build is.
//
//  Delete this file once DocumentPDFBuilder ships.
//

import Foundation
import UIKit
import CoreText
import CoreGraphics
import os

enum DocPipelineProbe {

    // MARK: - 1. CoreText: can we lay out and paginate attributed text?

    static func probeCoreTextPagination(_ text: String, pageSize: CGSize) -> Int {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: 28),
            .foregroundColor: UIColor.black,
            .paragraphStyle: NSParagraphStyle.default
        ]
        let attributed = NSAttributedString(string: text, attributes: attrs)
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)

        var pageCount = 0
        var location = 0
        let total = attributed.length
        let bounds = CGRect(origin: .zero, size: pageSize)

        while location < total && pageCount < 10_000 {
            let path = CGPath(rect: bounds, transform: nil)
            let frame = CTFramesetterCreateFrame(
                framesetter,
                CFRangeMake(location, 0),
                path,
                nil
            )
            let visible = CTFrameGetVisibleStringRange(frame)
            if visible.length <= 0 { break }
            location += visible.length
            pageCount += 1
        }
        return pageCount
    }

    static func probeSuggestedSize(_ attributed: NSAttributedString, width: CGFloat) -> CGSize {
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)
        var fitRange = CFRangeMake(0, 0)
        return CTFramesetterSuggestFrameSizeWithConstraints(
            framesetter,
            CFRangeMake(0, 0),
            nil,
            CGSize(width: width, height: .greatestFiniteMagnitude),
            &fitRange
        )
    }

    // MARK: - 2. Drawing a CoreText frame into a PDF page

    static func probeWritePaginatedPDF(text: String, to path: String) -> Bool {
        let pageSize = CGSize(width: 1280, height: 720)
        let margin: CGFloat = 60
        let textRect = CGRect(
            x: margin, y: margin,
            width: pageSize.width - margin * 2,
            height: pageSize.height - margin * 2
        )

        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 28)]
        )
        let framesetter = CTFramesetterCreateWithAttributedString(attributed)
        let renderer = UIGraphicsPDFRenderer(bounds: CGRect(origin: .zero, size: pageSize))

        do {
            try renderer.writePDF(to: URL(fileURLWithPath: path)) { ctx in
                var location = 0
                let total = attributed.length
                while location < total {
                    ctx.beginPage()
                    let cg = ctx.cgContext

                    // CoreText draws bottom-up; flip into UIKit's coordinate space.
                    cg.textMatrix = .identity
                    cg.translateBy(x: 0, y: pageSize.height)
                    cg.scaleBy(x: 1.0, y: -1.0)

                    let path = CGPath(rect: textRect, transform: nil)
                    let frame = CTFramesetterCreateFrame(
                        framesetter,
                        CFRangeMake(location, 0),
                        path,
                        nil
                    )
                    CTFrameDraw(frame, cg)

                    let visible = CTFrameGetVisibleStringRange(frame)
                    if visible.length <= 0 { break }
                    location += visible.length
                }
            }
            return true
        } catch {
            return false
        }
    }

    // MARK: - 3. Per-line drawing, for a table layout the framesetter can't do

    static func probeDrawSingleLine(_ text: String, in ctx: CGContext, at point: CGPoint) {
        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 24)]
        )
        let line = CTLineCreateWithAttributedString(attributed)
        ctx.textPosition = point
        CTLineDraw(line, ctx)
    }

    static func probeLineWidth(_ text: String) -> Double {
        let attributed = NSAttributedString(
            string: text,
            attributes: [.font: UIFont.systemFont(ofSize: 24)]
        )
        let line = CTLineCreateWithAttributedString(attributed)
        return CTLineGetTypographicBounds(line, nil, nil, nil)
    }

    // MARK: - 4. Korean font availability is a RUNTIME question, not a compile one

    static func probeKoreanFontNames() -> [String] {
        var found: [String] = []
        for family in UIFont.familyNames {
            for name in UIFont.fontNames(forFamilyName: family) {
                if name.contains("Gothic") || name.contains("Myungjo") || name.contains("Apple SD") {
                    found.append(name)
                }
            }
        }
        return found
    }

    // MARK: - 5. Logging, for the on-screen diagnostics pane

    static let log = Logger(subsystem: "com.sojubada.argo", category: "diagnostics")

    static func probeLogging() {
        log.info("probe: logger is available")
        os_log("probe: os_log is available")
    }

    // MARK: - 6. Temp file lifecycle, for the convert-then-delete flow

    static func probeTempLifecycle() -> Bool {
        let dir = (NSTemporaryDirectory() as NSString)
            .appendingPathComponent("argo_docpdf_\(UUID().uuidString)")
        let fm = FileManager.default
        guard (try? fm.createDirectory(atPath: dir, withIntermediateDirectories: true)) != nil else {
            return false
        }
        let file = (dir as NSString).appendingPathComponent("out.pdf")
        let ok = probeWritePaginatedPDF(text: "probe", to: file)
        let exists = fm.fileExists(atPath: file)
        try? fm.removeItem(atPath: dir)
        return ok && exists && !fm.fileExists(atPath: dir)
    }
}
