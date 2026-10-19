//
//  CGPDFProbe4.swift
//  Argo
//
//  TEMPORARY. Compiled but never called.
//
//  Stage 4 wants a highlighter, an annotated export, and text search. PDFKit
//  is absent on tvOS, so none of PDFSelection / findString / PDFAnnotation
//  exist. This file checks what Core Graphics alone can actually do, before
//  any of it is specified:
//
//    A. write a new PDF (for "save an annotated copy")
//    B. scan a page's content stream (the only route to text, hence to search
//       and to snapping a highlight onto words)
//
//  Delete once stage 4 is in.
//

import Foundation
import CoreGraphics
import UIKit

enum CGPDFProbe4 {

    // MARK: - A. Writing a PDF

    /// UIKit's PDF renderer. If this compiles, an annotated copy can be made by
    /// redrawing each source page and painting the highlights over it.
    static func probeWriteWithUIKit(source: CGPDFDocument, to path: String) -> Bool {
        guard let first = source.page(at: 1) else { return false }
        var box = first.getBoxRect(.cropBox)
        if box.isEmpty { box = first.getBoxRect(.mediaBox) }

        let renderer = UIGraphicsPDFRenderer(bounds: box)
        do {
            try renderer.writePDF(to: URL(fileURLWithPath: path)) { ctx in
                for i in 1...source.numberOfPages {
                    guard let page = source.page(at: i) else { continue }
                    var pageBox = page.getBoxRect(.cropBox)
                    if pageBox.isEmpty { pageBox = page.getBoxRect(.mediaBox) }
                    ctx.beginPage(withBounds: pageBox, pageInfo: [:])

                    let cg = ctx.cgContext
                    cg.saveGState()
                    cg.translateBy(x: 0, y: pageBox.height)
                    cg.scaleBy(x: 1, y: -1)
                    cg.drawPDFPage(page)
                    cg.restoreGState()

                    // A highlight: multiply blend over the drawn page.
                    cg.saveGState()
                    cg.setBlendMode(.multiply)
                    cg.setFillColor(UIColor.yellow.withAlphaComponent(0.45).cgColor)
                    cg.fill(CGRect(x: 72, y: 72, width: 180, height: 16))
                    cg.restoreGState()
                }
            }
            return true
        } catch {
            return false
        }
    }

    /// The Core Graphics route to the same thing, in case the UIKit one is not
    /// available or we need the media box per page.
    static func probeWriteWithCoreGraphics(source: CGPDFDocument, to path: String) -> Bool {
        let url = URL(fileURLWithPath: path) as CFURL
        guard let consumer = CGDataConsumer(url: url) else { return false }
        guard let page1 = source.page(at: 1) else { return false }
        var box = page1.getBoxRect(.mediaBox)
        guard let ctx = CGContext(consumer: consumer, mediaBox: &box, nil) else { return false }

        for i in 1...source.numberOfPages {
            guard let page = source.page(at: i) else { continue }
            var pageBox = page.getBoxRect(.mediaBox)
            ctx.beginPage(mediaBox: &pageBox)
            ctx.drawPDFPage(page)
            ctx.setBlendMode(.multiply)
            ctx.setFillColor(UIColor.yellow.withAlphaComponent(0.45).cgColor)
            ctx.fill(CGRect(x: 72, y: 72, width: 180, height: 16))
            ctx.endPage()
        }
        ctx.closePDF()
        return true
    }

    // MARK: - B. Reading text out of a page

    /// Content-stream scanning. This is the ONLY way to reach text without
    /// PDFKit: parse the operators ourselves and resolve glyph codes through
    /// the font's encoding. Checking here whether the machinery even exists.
    static func probeScanText(page: CGPDFPage) -> Int {
        guard let table = CGPDFOperatorTableCreate() else { return -1 }

        // Tj - show a string
        CGPDFOperatorTableSetCallback(table, "Tj") { scanner, info in
            var str: CGPDFStringRef?
            if CGPDFScannerPopString(scanner, &str), let s = str {
                if let text = CGPDFStringCopyTextString(s) {
                    _ = text as String
                }
                _ = CGPDFStringGetLength(s)
                _ = CGPDFStringGetBytePtr(s)
            }
            if let info = info {
                info.assumingMemoryBound(to: Int.self).pointee += 1
            }
        }

        // TJ - show an array of strings and kerning numbers
        CGPDFOperatorTableSetCallback(table, "TJ") { scanner, _ in
            var arr: CGPDFArrayRef?
            if CGPDFScannerPopArray(scanner, &arr), let a = arr {
                for i in 0..<CGPDFArrayGetCount(a) {
                    var s: CGPDFStringRef?
                    if CGPDFArrayGetString(a, i, &s), let str = s {
                        _ = CGPDFStringCopyTextString(str)
                    }
                    var num: CGPDFReal = 0
                    _ = CGPDFArrayGetNumber(a, i, &num)
                }
            }
        }

        // Td / Tm - text position, needed to know WHERE a word sits
        CGPDFOperatorTableSetCallback(table, "Td") { scanner, _ in
            var ty: CGPDFReal = 0
            var tx: CGPDFReal = 0
            _ = CGPDFScannerPopNumber(scanner, &ty)
            _ = CGPDFScannerPopNumber(scanner, &tx)
        }
        CGPDFOperatorTableSetCallback(table, "Tm") { scanner, _ in
            var f: CGPDFReal = 0, e: CGPDFReal = 0, d: CGPDFReal = 0
            var c: CGPDFReal = 0, b: CGPDFReal = 0, a: CGPDFReal = 0
            _ = CGPDFScannerPopNumber(scanner, &f)
            _ = CGPDFScannerPopNumber(scanner, &e)
            _ = CGPDFScannerPopNumber(scanner, &d)
            _ = CGPDFScannerPopNumber(scanner, &c)
            _ = CGPDFScannerPopNumber(scanner, &b)
            _ = CGPDFScannerPopNumber(scanner, &a)
        }

        // Tf - the font in force, whose encoding decides what the bytes mean
        CGPDFOperatorTableSetCallback(table, "Tf") { scanner, _ in
            var size: CGPDFReal = 0
            _ = CGPDFScannerPopNumber(scanner, &size)
            var name: UnsafePointer<Int8>?
            _ = CGPDFScannerPopName(scanner, &name)
        }

        var counter = 0
        let stream = CGPDFContentStreamCreateWithPage(page)
        let scanner = withUnsafeMutablePointer(to: &counter) { ptr -> CGPDFScannerRef in
            return CGPDFScannerCreate(stream, table, ptr)
        }
        CGPDFScannerScan(scanner)
        CGPDFScannerRelease(scanner)
        CGPDFContentStreamRelease(stream)
        CGPDFOperatorTableRelease(table)
        return counter
    }

    /// Reaching the font dictionaries, where a ToUnicode CMap would live.
    /// Without one, glyph codes cannot be turned into readable text.
    static func probeFontResources(page: CGPDFPage) -> Int {
        guard let pageDict = page.dictionary else { return -1 }
        var resources: CGPDFDictionaryRef?
        guard CGPDFDictionaryGetDictionary(pageDict, "Resources", &resources),
              let res = resources else { return 0 }
        var fonts: CGPDFDictionaryRef?
        guard CGPDFDictionaryGetDictionary(res, "Font", &fonts), let f = fonts else { return 0 }

        var found = 0
        CGPDFDictionaryApplyFunction(f, { _, value, info in
            var fontDict: CGPDFDictionaryRef?
            if CGPDFObjectGetValue(value, .dictionary, &fontDict), let fd = fontDict {
                var toUnicode: CGPDFStreamRef?
                if CGPDFDictionaryGetStream(fd, "ToUnicode", &toUnicode), let st = toUnicode {
                    var format: CGPDFDataFormat = .raw
                    if let data = CGPDFStreamCopyData(st, &format) {
                        _ = CFDataGetLength(data)
                    }
                }
            }
            if let info = info {
                info.assumingMemoryBound(to: Int.self).pointee += 1
            }
        }, &found)
        return found
    }
}
