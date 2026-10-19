//
//  PDFReaderModel.swift
//  Argo
//

import Foundation
import CoreGraphics
import UIKit
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
    let pageIndex: Int // 0-based
    let level: Int
}

final class PDFReaderModel: ObservableObject {
    let fullPath: String
    let fileName: String
    let fileHash: String

    private(set) var document: CGPDFDocument?

    @Published var isLocked: Bool = false
    @Published var loadError: String? = nil

    @Published var currentPageIndex: Int = 0 // 0-based (CGPDFDocument 호출 시 +1)
    @Published var totalPages: Int = 0

    @Published var displayModeOption: PDFDisplayModeOption = .singlePage
    @Published var fitPolicy: PDFFitPolicy = .fitPage
    @Published var theme: PDFThemeOption = .original
    @Published var brightness: Double = 1.0

    @Published var interactionMode: PDFInteractionMode = .reading
    @Published var zoomStepIndex: Int = 0 // 0 = Fit, 1 = 100%, 2 = 150%, 3 = 200%
    let zoomMultipliers: [CGFloat] = [1.0, 1.0, 1.5, 2.0]
    @Published var panOffset: CGSize = .zero

    @Published var primaryPageImage: UIImage? = nil
    @Published var secondaryPageImage: UIImage? = nil
    @Published var isRenderingPrimary: Bool = false

    @Published var continuousScrollJumpTarget: Int? = nil

    @Published var bookmarks: [Int] = []
    @Published var outlineEntries: [PDFOutlineEntry] = []

    @Published var resumeBannerText: String? = nil
    @Published var autoAdvanceEnabled: Bool = false

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

        renderCurrentPages()
    }

    // MARK: - Rendering Current View

    var currentRenderScaleMultiplier: CGFloat {
        var mult: CGFloat = 1.0
        if fitPolicy == .fitWidth {
            mult = 1.35
        }
        if interactionMode == .zoomPan {
            let z = zoomMultipliers[zoomStepIndex]
            mult = max(mult, z)
        }
        return mult
    }

    var currentDisplayScaleEffect: CGFloat {
        var scale: CGFloat = (fitPolicy == .fitWidth) ? 1.35 : 1.0
        if interactionMode == .zoomPan {
            scale *= zoomMultipliers[zoomStepIndex]
        }
        return scale
    }

    func renderCurrentPages() {
        guard let doc = document, !isLocked, totalPages > 0 else { return }

        if displayModeOption == .continuous {
            return
        }

        let mult = currentRenderScaleMultiplier
        let baseWidth: CGFloat = (displayModeOption == .twoUp) ? 960.0 : 1920.0
        let baseHeight: CGFloat = 1080.0
        let targetSize = CGSize(
            width: min(4096.0, baseWidth * mult),
            height: min(4096.0, baseHeight * mult)
        )

        isRenderingPrimary = (primaryPageImage == nil)

        let primaryIndex = currentPageIndex
        PDFPageRenderer.shared.renderPage(
            document: doc,
            docKey: fileHash,
            pageIndex: primaryIndex,
            targetSize: targetSize,
            slot: "primary"
        ) { [weak self] image in
            guard let self = self else { return }
            self.primaryPageImage = image
            self.isRenderingPrimary = false
        }

        if displayModeOption == .twoUp {
            let secondIndex = primaryIndex + 1
            if secondIndex < totalPages {
                PDFPageRenderer.shared.renderPage(
                    document: doc,
                    docKey: fileHash,
                    pageIndex: secondIndex,
                    targetSize: targetSize,
                    slot: "secondary"
                ) { [weak self] image in
                    self?.secondaryPageImage = image
                }
            } else {
                _ = PDFPageRenderer.shared.nextToken(for: "secondary")
                secondaryPageImage = nil
            }
        } else {
            _ = PDFPageRenderer.shared.nextToken(for: "secondary")
            secondaryPageImage = nil

            // 한 쪽 보기에서는 인접(±1) 페이지를 백그라운드에서 미리 캐시
            PDFPageRenderer.shared.prefetchAdjacent(
                document: doc,
                docKey: fileHash,
                around: primaryIndex,
                totalPages: totalPages,
                targetSize: targetSize
            )
        }
    }

    // MARK: - Navigation

    func goToPage(_ index: Int) {
        guard totalPages > 0 else { return }
        let clamped = max(0, min(totalPages - 1, index))
        panOffset = .zero
        currentPageIndex = clamped
        saveProgress(clamped)

        if displayModeOption == .continuous {
            continuousScrollJumpTarget = clamped
        } else {
            renderCurrentPages()
        }
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

    func updateVisibleContinuousPage(_ pageIndex: Int) {
        guard displayModeOption == .continuous,
              pageIndex >= 0,
              pageIndex < totalPages else { return }
        if currentPageIndex != pageIndex {
            currentPageIndex = pageIndex
            saveProgress(pageIndex)
        }
    }

    // MARK: - Zoom & Pan (Expanded Mode)

    func cycleZoomStep() {
        zoomStepIndex = (zoomStepIndex + 1) % zoomMultipliers.count
        if zoomStepIndex == 0 {
            panOffset = .zero
        } else {
            clampPanOffset()
        }
        renderCurrentPages()
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

    func pan(dx: CGFloat, dy: CGFloat) {
        // 리모컨 우측/하단 입력 시 문서의 우측/하단 영역을 볼 수 있도록 오프셋 이동
        panOffset = CGSize(
            width: panOffset.width - dx,
            height: panOffset.height - dy
        )
        clampPanOffset()
    }

    private func clampPanOffset() {
        let scale = currentDisplayScaleEffect
        let maxX = max(0, (1920.0 * max(1.0, scale) - 1920.0) / 2.0 + 360.0)
        let maxY = max(0, (1080.0 * max(1.0, scale) - 1080.0) / 2.0 + 360.0)
        panOffset.width = max(-maxX, min(maxX, panOffset.width))
        panOffset.height = max(-maxY, min(maxY, panOffset.height))
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

    // MARK: - Outline Extraction (Core Graphics Catalog Dictionary Walk)

    private func buildOutline(from doc: CGPDFDocument) {
        guard let catalog = doc.catalog else {
            outlineEntries = []
            return
        }

        var outlines: CGPDFDictionaryRef?
        guard CGPDFDictionaryGetDictionary(catalog, "Outlines", &outlines),
              let root = outlines else {
            outlineEntries = []
            return
        }

        // 페이지 딕셔너리 포인터 → 0-based 페이지 인덱스 맵 생성 (1회)
        var pageMap: [UnsafeRawPointer: Int] = [:]
        if doc.numberOfPages > 0 {
            for i in 1...doc.numberOfPages {
                if let p = doc.page(at: i), let d = p.dictionary {
                    pageMap[UnsafeRawPointer(d)] = i - 1
                }
            }
        }

        var entries: [PDFOutlineEntry] = []
        var firstRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(root, "First", &firstRef), let first = firstRef {
            walkOutline(node: first, depth: 0, pageMap: pageMap, into: &entries)
        }
        outlineEntries = entries
    }

    private func walkOutline(
        node: CGPDFDictionaryRef,
        depth: Int,
        pageMap: [UnsafeRawPointer: Int],
        into list: inout [PDFOutlineEntry]
    ) {
        guard depth < 16 else { return }

        var titleText = ""
        var titleRef: CGPDFStringRef?
        if CGPDFDictionaryGetString(node, "Title", &titleRef), let titleRaw = titleRef {
            if let cf = CGPDFStringCopyTextString(titleRaw) {
                titleText = (cf as String).trimmingCharacters(in: .whitespacesAndNewlines)
            }
        }

        // /Dest 배열 또는 /A 액션 내부의 /D 배열에서 첫 번째 원소(페이지 딕셔너리) 추출
        var targetPageIndex: Int? = nil

        var destArray: CGPDFArrayRef?
        if CGPDFDictionaryGetArray(node, "Dest", &destArray), let arr = destArray {
            if CGPDFArrayGetCount(arr) > 0 {
                var pageRef: CGPDFDictionaryRef?
                if CGPDFArrayGetDictionary(arr, 0, &pageRef), let pDict = pageRef {
                    targetPageIndex = pageMap[UnsafeRawPointer(pDict)]
                }
            }
        }

        if targetPageIndex == nil {
            var actionRef: CGPDFDictionaryRef?
            if CGPDFDictionaryGetDictionary(node, "A", &actionRef), let action = actionRef {
                var innerDest: CGPDFArrayRef?
                if CGPDFDictionaryGetArray(action, "D", &innerDest), let arr = innerDest {
                    if CGPDFArrayGetCount(arr) > 0 {
                        var pageRef: CGPDFDictionaryRef?
                        if CGPDFArrayGetDictionary(arr, 0, &pageRef), let pDict = pageRef {
                            targetPageIndex = pageMap[UnsafeRawPointer(pDict)]
                        }
                    }
                }
            }
        }

        // 해석에 성공한 항목만 추가 (실패 시 목록에서 제외)
        if !titleText.isEmpty, let validPageIndex = targetPageIndex {
            list.append(PDFOutlineEntry(title: titleText, pageIndex: validPageIndex, level: depth))
        }

        var childRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(node, "First", &childRef), let child = childRef {
            walkOutline(node: child, depth: depth + 1, pageMap: pageMap, into: &list)
        }

        var siblingRef: CGPDFDictionaryRef?
        if CGPDFDictionaryGetDictionary(node, "Next", &siblingRef), let sibling = siblingRef {
            walkOutline(node: sibling, depth: depth, pageMap: pageMap, into: &list)
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