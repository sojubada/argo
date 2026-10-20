//
//  PDFReaderModel.swift
//  Argo
//
//  Simplified PDF Reader Model:
//  - Single Page / Two Pages (Book)
//  - Day (Original) / Night (Inverted)
//  - Page Flip Animation State & Resume Progress
//

import Foundation
import CoreGraphics
import UIKit
import SwiftUI
import Combine
import CryptoKit

enum PDFBookLayoutMode: String, CaseIterable, Identifiable {
    case singlePage = "1 Page"
    case twoPagesBook = "2 Pages (Book)"

    var id: String { rawValue }
}

final class PDFReaderModel: ObservableObject {
    let fullPath: String
    let fileName: String
    let fileHash: String

    private(set) var document: CGPDFDocument?

    @Published var isLocked: Bool = false
    @Published var loadError: String? = nil

    @Published var currentPageIndex: Int = 0 // 0-based
    @Published var totalPages: Int = 0

    @Published var layoutMode: PDFBookLayoutMode = .twoPagesBook
    @Published var isNightMode: Bool = false

    @Published var primaryPageImage: UIImage? = nil
    @Published var secondaryPageImage: UIImage? = nil
    @Published var isRendering: Bool = false

    /// 책장 넘기기 효과 방향 (-1: 이전 페이지로 넘김, 0: 정지, 1: 다음 페이지로 넘김)
    @Published var pageTurnDirection: Int = 0
    @Published var pageTurnAngle: Double = 0.0

    @Published var resumeBannerText: String? = nil

    private var isRestoringProgress: Bool = false

    init(filePath: String, fileName: String) {
        let combined: String
        if filePath.hasSuffix("/") {
            combined = filePath + fileName
        } else if filePath.isEmpty {
            combined = fileName
        } else {
            combined = filePath + "/" + fileName
        }
        self.fullPath = combined
        self.fileName = fileName
        self.fileHash = Self.computeSHA256(for: combined)

        loadPreferences()
        loadDocument()
    }

    static func computeSHA256(for input: String) -> String {
        let data = Data(input.utf8)
        let digest = SHA256.hash(data: data)
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: - Document Loading

    func loadDocument() {
        let cfURL = URL(fileURLWithPath: fullPath) as CFURL
        guard let doc = CGPDFDocument(cfURL) else {
            loadError = "Unable to open this PDF file."
            return
        }
        self.document = doc

        if doc.isEncrypted && !doc.isUnlocked {
            self.isLocked = true
        } else {
            self.isLocked = false
            setupAfterUnlock(doc)
        }
    }

    func unlock(password: String) -> Bool {
        guard let doc = document else { return false }
        let unlocked = password.withCString { cStr in
            doc.unlockWithPassword(cStr)
        }
        if unlocked || doc.isUnlocked {
            isLocked = false
            setupAfterUnlock(doc)
            return true
        }
        return false
    }

    private func setupAfterUnlock(_ doc: CGPDFDocument) {
        let count = doc.numberOfPages
        guard count > 0 else {
            loadError = "This PDF contains no pages."
            return
        }
        totalPages = count
        PDFPageRenderer.shared.clearCache()

        let savedPage = UserDefaults.settings.integer(forKey: progressKey)
        if savedPage > 0 && savedPage < totalPages {
            isRestoringProgress = true
            currentPageIndex = savedPage
            resumeBannerText = "Resuming from page \(savedPage + 1)"
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
                self?.resumeBannerText = nil
            }
            isRestoringProgress = false
        } else {
            currentPageIndex = 0
        }

        renderCurrentPages()
    }

    // MARK: - Rendering

    func renderCurrentPages() {
        guard let doc = document, !isLocked, totalPages > 0 else { return }

        let targetSize = (layoutMode == .twoPagesBook)
            ? CGSize(width: 940, height: 1040)
            : CGSize(width: 1840, height: 1040)

        if primaryPageImage == nil {
            isRendering = true
        }

        let primaryIdx = currentPageIndex
        PDFPageRenderer.shared.renderPage(
            document: doc,
            docKey: fileHash,
            pageIndex: primaryIdx,
            targetSize: targetSize,
            slot: "primary"
        ) { [weak self] img in
            guard let self = self else { return }
            self.primaryPageImage = img
            self.isRendering = false
        }

        if layoutMode == .twoPagesBook {
            let secondIdx = primaryIdx + 1
            if secondIdx < totalPages {
                PDFPageRenderer.shared.renderPage(
                    document: doc,
                    docKey: fileHash,
                    pageIndex: secondIdx,
                    targetSize: targetSize,
                    slot: "secondary"
                ) { [weak self] img in
                    self?.secondaryPageImage = img
                }
            } else {
                _ = PDFPageRenderer.shared.nextToken(for: "secondary")
                secondaryPageImage = nil
            }
        } else {
            _ = PDFPageRenderer.shared.nextToken(for: "secondary")
            secondaryPageImage = nil
        }

        PDFPageRenderer.shared.prefetchAdjacent(
            document: doc,
            docKey: fileHash,
            around: primaryIdx,
            totalPages: totalPages,
            targetSize: targetSize
        )
    }

    // MARK: - Navigation & Page Flip Effect

    var pageStep: Int {
        return (layoutMode == .twoPagesBook) ? 2 : 1
    }

    var canGoPrevious: Bool {
        return currentPageIndex > 0
    }

    var canGoNext: Bool {
        return currentPageIndex + pageStep < totalPages
    }

    func nextPageWithCurl() {
        guard canGoNext else { return }
        let nextIdx = min(totalPages - 1, currentPageIndex + pageStep)
        performPageTurn(to: nextIdx, direction: 1)
    }

    func previousPageWithCurl() {
        guard canGoPrevious else { return }
        let prevIdx = max(0, currentPageIndex - pageStep)
        performPageTurn(to: prevIdx, direction: -1)
    }

    func jumpToPage(_ index: Int) {
        guard totalPages > 0 else { return }
        let clamped = max(0, min(totalPages - 1, index))
        guard clamped != currentPageIndex else { return }
        let dir = (clamped > currentPageIndex) ? 1 : -1
        performPageTurn(to: clamped, direction: dir)
    }

    private func performPageTurn(to targetIndex: Int, direction: Int) {
        pageTurnDirection = direction
        withAnimation(.easeIn(duration: 0.14)) {
            pageTurnAngle = (direction > 0) ? -55.0 : 55.0
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.14) { [weak self] in
            guard let self = self else { return }
            self.currentPageIndex = targetIndex
            self.saveProgress(targetIndex)
            self.renderCurrentPages()
            self.pageTurnAngle = (direction > 0) ? 55.0 : -55.0

            withAnimation(.easeOut(duration: 0.18)) {
                self.pageTurnAngle = 0.0
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) { [weak self] in
                self?.pageTurnDirection = 0
            }
        }
    }

    // MARK: - Toggles & Persistence

    func toggleLayoutMode() {
        layoutMode = (layoutMode == .singlePage) ? .twoPagesBook : .singlePage
        savePreferences()
        renderCurrentPages()
    }

    func toggleNightMode() {
        isNightMode.toggle()
        savePreferences()
    }

    private var progressKey: String {
        "pdfProgress.\(fileHash)"
    }

    private func saveProgress(_ pageIndex: Int) {
        guard !isRestoringProgress else { return }
        UserDefaults.settings.set(pageIndex, forKey: progressKey)
        UserDefaults.settings.synchronize()
    }

    private func savePreferences() {
        let defaults = UserDefaults.settings
        defaults.set(layoutMode.rawValue, forKey: "pdfSimple.layoutMode")
        defaults.set(isNightMode, forKey: "pdfSimple.isNightMode")
        defaults.synchronize()
    }

    private func loadPreferences() {
        let defaults = UserDefaults.settings
        if let raw = defaults.string(forKey: "pdfSimple.layoutMode"),
           let mode = PDFBookLayoutMode(rawValue: raw) {
            layoutMode = mode
        }
        isNightMode = defaults.bool(forKey: "pdfSimple.isNightMode")
    }

    var pageStatusText: String {
        guard totalPages > 0 else { return "0 / 0" }
        if layoutMode == .twoPagesBook && currentPageIndex + 1 < totalPages {
            return "Pages \(currentPageIndex + 1)–\(currentPageIndex + 2) / \(totalPages)"
        }
        return "Page \(currentPageIndex + 1) / \(totalPages)"
    }
}