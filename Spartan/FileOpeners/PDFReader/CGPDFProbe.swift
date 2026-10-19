//
//  CGPDFProbe.swift
//  Argo
//
//  TEMPORARY. Compiled but never called.
//
//  PDFKit is absent from the tvOS SDK ("no such module 'PDFKit'"), despite
//  Apple's documentation listing it as tvOS 11+. This file exercises the
//  Core Graphics PDF API that the reader will be rewritten against, so that
//  CI confirms every symbol exists on tvOS before that work starts.
//
//  Delete once the reader is in place.
//

import Foundation
import CoreGraphics
import UIKit

enum CGPDFProbe {

    /// Open, count pages, and handle an encrypted document.
    static func probeOpen(path: String, password: String) -> Int {
        guard let doc = CGPDFDocument(URL(fileURLWithPath: path) as CFURL) else {
            return -1
        }
        if doc.isEncrypted && !doc.isUnlocked {
            _ = password.withCString { doc.unlockWithPassword($0) }
        }
        _ = doc.allowsCopying
        _ = doc.allowsPrinting
        return doc.numberOfPages
    }

    /// Page geometry and rotation.
    static func probeGeometry(doc: CGPDFDocument, pageNumber: Int) -> CGRect {
        guard let page = doc.page(at: pageNumber) else { return .zero }
        let crop = page.getBoxRect(.cropBox)
        let media = page.getBoxRect(.mediaBox)
        let rotation = page.rotationAngle
        _ = rotation
        _ = page.pageNumber
        return crop.isEmpty ? media : crop
    }

    /// Rasterise a page into a UIImage at a target size. This is what replaces
    /// PDFView: draw the page ourselves and show it in an Image.
    static func probeRender(doc: CGPDFDocument, pageNumber: Int, target: CGSize) -> UIImage? {
        guard let page = doc.page(at: pageNumber) else { return nil }
        var box = page.getBoxRect(.cropBox)
        if box.isEmpty { box = page.getBoxRect(.mediaBox) }
        guard box.width > 0, box.height > 0 else { return nil }

        let scale = min(target.width / box.width, target.height / box.height)
        let pixelSize = CGSize(width: box.width * scale, height: box.height * scale)

        let format = UIGraphicsImageRendererFormat.default()
        format.opaque = true
        let renderer = UIGraphicsImageRenderer(size: pixelSize, format: format)

        return renderer.image { ctx in
            let cg = ctx.cgContext
            cg.setFillColor(UIColor.white.cgColor)
            cg.fill(CGRect(origin: .zero, size: pixelSize))

            // PDF origin is bottom-left; UIKit's is top-left.
            cg.translateBy(x: 0, y: pixelSize.height)
            cg.scaleBy(x: scale, y: -scale)
            cg.translateBy(x: -box.origin.x, y: -box.origin.y)

            // Honours the page's own /Rotate entry.
            let transform = page.getDrawingTransform(.cropBox,
                                                     rect: box,
                                                     rotate: 0,
                                                     preserveAspectRatio: true)
            cg.concatenate(transform)
            cg.drawPDFPage(page)
        }
    }

    /// Walk the document catalog for an outline (table of contents). PDFOutline
    /// is part of PDFKit, so the tree has to be read out of the raw dictionaries.
    static func probeOutline(doc: CGPDFDocument) -> Int {
        guard let catalog = doc.catalog else { return 0 }

        var outlines: CGPDFDictionaryRef?
        guard CGPDFDictionaryGetDictionary(catalog, "Outlines", &outlines),
              let root = outlines else {
            return 0
        }

        var count = 0
        var firstRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(root, "First", &firstRef), let first = firstRef {
            count = walk(node: first, depth: 0)
        }
        return count
    }

    private static func walk(node: CGPDFDictionaryRef, depth: Int) -> Int {
        guard depth < 16 else { return 0 }
        var total = 1

        // Title is a PDF string.
        var titleRef: CGPDFStringRef?
        if CGPDFDictionaryGetString(node, "Title", &titleRef), let titleRaw = titleRef {
            if let cf = CGPDFStringCopyTextString(titleRaw) {
                _ = cf as String
            }
        }

        // Destination may be an array, a name, or sit behind an /A action.
        var destArray: CGPDFArrayRef?
        if CGPDFDictionaryGetArray(node, "Dest", &destArray), let arr = destArray {
            var pageRef: CGPDFDictionaryRef?
            if CGPDFArrayGetDictionary(arr, 0, &pageRef) {
                _ = pageRef
            }
            _ = CGPDFArrayGetCount(arr)
        }
        var actionRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(node, "A", &actionRef), let action = actionRef {
            var innerDest: CGPDFArrayRef?
            _ = CGPDFDictionaryGetArray(action, "D", &innerDest)
        }

        var childRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(node, "First", &childRef), let child = childRef {
            total += walk(node: child, depth: depth + 1)
        }
        var siblingRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(node, "Next", &siblingRef), let sibling = siblingRef {
            total += walk(node: sibling, depth: depth)
        }
        return total
    }
}
