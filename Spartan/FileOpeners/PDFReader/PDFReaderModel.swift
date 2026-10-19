//
//  PDFReaderModel.swift
//  Argo
//

import Foundation
import PDFKit
import SwiftUI
import Combine
import CryptoKit

enum PDFDisplayModeOption: String, CaseIterable, Identifiable {
    case singlePage = "Single Page"
    case continuous = "Continuous Scroll"
    case twoUp = "Two Pages (Book)"

    var id: String { rawValue }
}

enum PDFFitPolicy: String, CaseIterable, Identifiable {
    case fitPage = "Fit Entire Page"
    case fitWidth = "Fit Width"

    var id: String { rawValue }
}

enum PDFThemeOption: String, CaseIterable, Identifiable {
    case original = "Original"
    case night = "Night (Inverted)"
    case sepia = "Sepia"

    var id: String { rawValue }
}

enum PDFInteractionMode: Equatable {
    case reading
    case zoomPan
}

struct PDFOutlineEntry: Identifiable, Hashable {
    let id = UUID()
    let title: String
    let pageIndex: Int
    let level: Int
}

final class PDFReaderModel: ObservableObject {
    let fullPath: String
    let fileName: String
    let fileHash: String

    @Published private(set) var document: PDFDocument?
    @Published var isLocked: Bool = false
    @Published var loadError: String? = nil

    @Published var currentPageIndex: Int = 0
    @Published var totalPages: Int = 0

    @Published var displayModeOption: PDFDisplayModeOption = .singlePage
    @Published var fitPolicy: PDFFitPolicy = .fitPage
    @Published var theme: PDFThemeOption = .original
    @Published var brightness: Double = 1.0

    @Published var interactionMode: PDFInteractionMode = .reading
    @Published var zoomStepIndex: Int = 0 // 0 = Fit, 1 = 100%, 2 = 150%, 3 = 200%
    let zoomMultipliers: [CGFloat] = [1.0, 1.0, 1.5, 2.0]

    @Published var bookmarks: [Int] = []
    @Published var outlineEntries: [PDFOutlineEntry] = []

    @Published var resumeBannerText: String? = nil
    @Published var autoAdvanceEnabled: Bool = false

    weak var pdfView: PDFView?
    private var isRestoringProgress: Bool = false
    private var autoAdvanceTimer: Timer?

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

        loadSavedPreferences()
        loadDocument()
    }

    deinit {
        autoAdvanceTimer?.invalidate()
    }

    static func computeSHA256(for input: String) -> String {
        let data = Data(input.utf8)
        let digest = SHA256.hash(data: data)
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    // MARK: - Document Loading & Unlocking

    func loadDocument() {
        let url = URL(fileURLWithPath: fullPath)
        guard let doc = PDFDocument(url: url) else {
            loadError = "Unable to open this PDF file."
            return
        }
        self.document = doc
        if doc.isLocked {
            self.isLocked = true
        } else {
            self.isLocked = false
            setupAfterUnlock(doc)
        }
    }

    func unlock(password: String) -> Bool {
        guard let doc = document else { return false }
        if doc.unlock(withPassword: password) {
            isLocked = false
            setupAfterUnlock(doc)
            return true
        }
        return false
    }

    private func setupAfterUnlock(_ doc: PDFDocument) {
        totalPages = doc.pageCount
        loadBookmarks()
        buildOutline(from: doc)

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
    }

    // MARK: - Navigation

    func goToPage(_ index: Int) {
        guard let doc = document, totalPages > 0 else { return }
        let clamped = max(0, min(totalPages - 1, index))
        currentPageIndex = clamped
        if let page = doc.page(at: clamped), let view = pdfView {
            if view.currentPage != page {
                view.go(to: page)
            }
        }
        saveProgress(clamped)
    }

    func nextPage() {
        let step = (displayModeOption == .twoUp) ? 2 : 1
        if currentPageIndex + step < totalPages {
            goToPage(currentPageIndex + step)
        } else if currentPageIndex < totalPages - 1 {
            goToPage(totalPages - 1)
        }
    }

    func previousPage() {
        let step = (displayModeOption == .twoUp) ? 2 : 1
        if currentPageIndex - step >= 0 {
            goToPage(currentPageIndex - step)
        } else if currentPageIndex > 0 {
            goToPage(0)
        }
    }

    func onPDFViewPageChanged(_ newIndex: Int) {
        guard newIndex >= 0 && newIndex < totalPages else { return }
        if currentPageIndex != newIndex {
            currentPageIndex = newIndex
        }
        saveProgress(newIndex)
    }

    // MARK: - Zoom & Pan (Expanded Mode)

    func cycleZoomStep() {
        zoomStepIndex = (zoomStepIndex + 1) % zoomMultipliers.count
        applyScaleToPDFView()
    }

    var zoomStepLabel: String {
        switch zoomStepIndex {
        case 0: return "Fit"
        case 1: return "100%"
        case 2: return "150%"
        case 3: return "200%"
        default: return "Fit"
        }
    }

    func applyScaleToPDFView() {
        guard let view = pdfView else { return }
        if interactionMode == .reading || zoomStepIndex == 0 {
            if fitPolicy == .fitPage {
                view.autoScales = true
            } else {
                view.autoScales = false
                let baseScale = view.scaleFactorForSizeToFit
                if baseScale > 0 {
                    view.scaleFactor = baseScale * 1.35
                } else {
                    view.autoScales = true
                }
            }
        } else {
            view.autoScales = false
            let baseScale = max(view.scaleFactorForSizeToFit, 0.5)
            let multiplier = zoomMultipliers[zoomStepIndex]
            view.scaleFactor = baseScale * multiplier
        }
    }

    func pan(dx: CGFloat, dy: CGFloat) {
        guard let view = pdfView,
              let scrollView = Self.findScrollView(in: view) else { return }
        let current = scrollView.contentOffset
        let maxOffsetX = max(0, scrollView.contentSize.width - scrollView.bounds.width)
        let maxOffsetY = max(0, scrollView.contentSize.height - scrollView.bounds.height)
        let newX = max(0, min(maxOffsetX, current.x + dx))
        let newY = max(0, min(maxOffsetY, current.y + dy))
        scrollView.setContentOffset(CGPoint(x: newX, y: newY), animated: true)
    }

    private static func findScrollView(in view: UIView) -> UIScrollView? {
        if let scroll = view as? UIScrollView {
            return scroll
        }
        for sub in view.subviews {
            if let found = findScrollView(in: sub) {
                return found
            }
        }
        return nil
    }

    // MARK: - Auto Advance (Play/Pause)

    func toggleAutoAdvance() {
        autoAdvanceEnabled.toggle()
        autoAdvanceTimer?.invalidate()
        autoAdvanceTimer = nil

        if autoAdvanceEnabled {
            autoAdvanceTimer = Timer.scheduledTimer(withTimeInterval: 8.0, repeats: true) { [weak self] _ in
                guard let self = self else { return }
                if self.currentPageIndex < self.totalPages - 1 {
                    self.nextPage()
                } else {
                    self.autoAdvanceEnabled = false
                    self.autoAdvanceTimer?.invalidate()
                    self.autoAdvanceTimer = nil
                }
            }
        }
    }

    // MARK: - Progress & Bookmarks Persistence

    private var progressKey: String {
        "pdfProgress.\(fileHash)"
    }

    private var bookmarksKey: String {
        "pdfBookmarks.\(fileHash)"
    }

    private func saveProgress(_ pageIndex: Int) {
        guard !isRestoringProgress else { return }
        UserDefaults.settings.set(pageIndex, forKey: progressKey)
        UserDefaults.settings.synchronize()
    }

    private func loadBookmarks() {
        if let saved = UserDefaults.settings.array(forKey: bookmarksKey) as? [Int] {
            bookmarks = saved.filter { $0 >= 0 && $0 < totalPages }.sorted()
        } else {
            bookmarks = []
        }
    }

    private func persistBookmarks() {
        UserDefaults.settings.set(bookmarks, forKey: bookmarksKey)
        UserDefaults.settings.synchronize()
    }

    var isCurrentPageBookmarked: Bool {
        bookmarks.contains(currentPageIndex)
    }

    func toggleBookmarkForCurrentPage() {
        if let idx = bookmarks.firstIndex(of: currentPageIndex) {
            bookmarks.remove(at: idx)
        } else {
            bookmarks.append(currentPageIndex)
            bookmarks.sort()
        }
        persistBookmarks()
    }

    func removeBookmark(pageIndex: Int) {
        bookmarks.removeAll { $0 == pageIndex }
        persistBookmarks()
    }

    // MARK: - Preferences Persistence

    func savePreferences() {
        let defaults = UserDefaults.settings
        defaults.set(displayModeOption.rawValue, forKey: "pdfPref.displayMode")
        defaults.set(fitPolicy.rawValue, forKey: "pdfPref.fitPolicy")
        defaults.set(theme.rawValue, forKey: "pdfPref.theme")
        defaults.set(brightness, forKey: "pdfPref.brightness")
        defaults.synchronize()
    }

    private func loadSavedPreferences() {
        let defaults = UserDefaults.settings
        if let modeRaw = defaults.string(forKey: "pdfPref.displayMode"),
           let mode = PDFDisplayModeOption(rawValue: modeRaw) {
            displayModeOption = mode
        }
        if let fitRaw = defaults.string(forKey: "pdfPref.fitPolicy"),
           let fit = PDFFitPolicy(rawValue: fitRaw) {
            fitPolicy = fit
        }
        if let themeRaw = defaults.string(forKey: "pdfPref.theme"),
           let th = PDFThemeOption(rawValue: themeRaw) {
            theme = th
        }
        let savedBrightness = defaults.double(forKey: "pdfPref.brightness")
        if savedBrightness >= 0.3 && savedBrightness <= 1.0 {
            brightness = savedBrightness
        } else {
            brightness = 1.0
        }
    }

    // MARK: - Outline Extraction

    private func buildOutline(from doc: PDFDocument) {
        guard let root = doc.outlineRoot else {
            outlineEntries = []
            return
        }
        var list: [PDFOutlineEntry] = []
        traverseOutline(node: root, doc: doc, level: -1, into: &list)
        outlineEntries = list
    }

    private func traverseOutline(node: PDFOutline, doc: PDFDocument, level: Int, into list: inout [PDFOutlineEntry]) {
        if level >= 0 {
            let label = (node.label ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let pageIndex: Int
            if let destPage = node.destination?.page {
                pageIndex = doc.index(for: destPage)
            } else if let actionGoTo = node.action as? PDFActionGoTo,
                      let destPage = actionGoTo.destination.page {
                pageIndex = doc.index(for: destPage)
            } else {
                pageIndex = 0
            }
            if !label.isEmpty && pageIndex >= 0 && pageIndex < doc.pageCount {
                list.append(PDFOutlineEntry(title: label, pageIndex: pageIndex, level: level))
            }
        }
        let childCount = node.numberOfChildren
        for i in 0..<childCount {
            if let child = node.child(at: i) {
                traverseOutline(node: child, doc: doc, level: level + 1, into: &list)
            }
        }
    }

    // MARK: - Progress Helpers

    var progressFraction: Double {
        guard totalPages > 0 else { return 0 }
        return Double(currentPageIndex + 1) / Double(totalPages)
    }

    var remainingPercentageInt: Int {
        guard totalPages > 0 else { return 0 }
        let remainingPages = max(0, totalPages - (currentPageIndex + 1))
        return Int(round((Double(remainingPages) / Double(totalPages)) * 100.0))
    }
}