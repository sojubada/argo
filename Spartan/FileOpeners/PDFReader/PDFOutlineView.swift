//
//  PDFOutlineView.swift
//  Argo
//

import SwiftUI
import PDFKit

enum PDFNavigationTab: String, CaseIterable, Identifiable {
    case outline = "Outline"
    case bookmarks = "Bookmarks"

    var id: String { rawValue }
}

struct PDFOutlineView: View {
    @ObservedObject var model: PDFReaderModel
    var onSelectPage: (Int) -> Void
    var onClose: () -> Void

    @State private var selectedTab: PDFNavigationTab = .outline

    var body: some View {
        VStack(spacing: 20) {
            HStack(spacing: 24) {
                Text("Contents & Bookmarks")
                    .font(.title2)

                Spacer()

                ForEach(PDFNavigationTab.allCases) { tab in
                    Button(action: {
                        selectedTab = tab
                    }) {
                        HStack(spacing: 8) {
                            Image(systemName: tab == .outline ? "list.bullet.indent" : "bookmark.fill")
                            Text(tab.rawValue)
                        }
                    }
                }

                Button(action: {
                    onClose()
                }) {
                    HStack(spacing: 8) {
                        Image(systemName: "xmark")
                        Text("Close")
                    }
                }
            }

            if selectedTab == .outline {
                outlineList
            } else {
                bookmarkList
            }
        }
        .padding(40)
        .onAppear {
            if model.outlineEntries.isEmpty && !model.bookmarks.isEmpty {
                selectedTab = .bookmarks
            }
        }
        .onExitCommand {
            onClose()
        }
    }

    private var outlineList: some View {
        Group {
            if model.outlineEntries.isEmpty {
                Spacer()
                Text("This PDF has no table of contents.")
                    .font(.headline)
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                // 행당 버튼 하나 규칙 준수
                List(model.outlineEntries) { entry in
                    Button(action: {
                        onSelectPage(entry.pageIndex)
                    }) {
                        HStack {
                            Text(String(repeating: "    ", count: max(0, entry.level)) + entry.title)
                                .lineLimit(1)
                            Spacer()
                            Text("p. \(entry.pageIndex + 1)")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }
            }
        }
    }

    private var bookmarkList: some View {
        Group {
            if model.bookmarks.isEmpty {
                Spacer()
                Text("No bookmarked pages yet.")
                    .font(.headline)
                    .foregroundColor(.secondary)
                Spacer()
            } else {
                // 행당 버튼 하나 규칙 준수
                List(model.bookmarks, id: \.self) { pageIdx in
                    Button(action: {
                        onSelectPage(pageIdx)
                    }) {
                        HStack(spacing: 16) {
                            Image(systemName: "bookmark.fill")
                            Text("Page \(pageIdx + 1)")
                                .font(.headline)
                            Spacer()
                            if pageIdx == model.currentPageIndex {
                                Text("Current")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Thumbnail Grid View (Lazy Generation for Large PDFs)

struct PDFThumbnailGridView: View {
    @ObservedObject var model: PDFReaderModel
    var onSelectPage: (Int) -> Void
    var onClose: () -> Void

    private let columns: [GridItem] = [
        GridItem(.adaptive(minimum: 220, maximum: 280), spacing: 28)
    ]

    var body: some View {
        VStack(spacing: 20) {
            HStack {
                Text("Page Thumbnails (\(model.totalPages) pages)")
                    .font(.title2)
                Spacer()
                Button(action: {
                    onClose()
                }) {
                    HStack(spacing: 8) {
                        Image(systemName: "xmark")
                        Text("Close")
                    }
                }
            }

            ScrollView {
                LazyVGrid(columns: columns, spacing: 28) {
                    ForEach(0..<model.totalPages, id: \.self) { pageIdx in
                        Button(action: {
                            onSelectPage(pageIdx)
                        }) {
                            PDFThumbnailCell(
                                document: model.document,
                                pageIndex: pageIdx,
                                isCurrent: pageIdx == model.currentPageIndex
                            )
                        }
                    }
                }
                .padding(20)
            }
        }
        .padding(40)
        .onExitCommand {
            onClose()
        }
    }
}

private struct PDFThumbnailCell: View {
    let document: PDFDocument?
    let pageIndex: Int
    let isCurrent: Bool

    @State private var thumbnailImage: UIImage? = nil

    var body: some View {
        VStack(spacing: 8) {
            ZStack {
                Rectangle()
                    .fill(Color.secondary.opacity(0.2))
                    .frame(width: 180, height: 240)
                    .cornerRadius(8)

                if let image = thumbnailImage {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 172, height: 232)
                        .cornerRadius(6)
                } else {
                    ProgressView()
                }
            }

            Text("Page \(pageIndex + 1)\(isCurrent ? " •" : "")")
                .font(.caption)
                .foregroundColor(isCurrent ? .primary : .secondary)
        }
        .padding(8)
        .onAppear {
            loadThumbnailIfNeeded()
        }
    }

    private func loadThumbnailIfNeeded() {
        guard thumbnailImage == nil,
              let doc = document,
              let page = doc.page(at: pageIndex) else {
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let img = page.thumbnail(of: CGSize(width: 240, height: 320), for: .cropBox)
            DispatchQueue.main.async {
                self.thumbnailImage = img
            }
        }
    }
}