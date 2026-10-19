//
//  PDFKitContainer.swift
//  Argo
//

import SwiftUI
import PDFKit
import UIKit

struct PDFKitContainer: UIViewRepresentable {
    @ObservedObject var model: PDFReaderModel

    func makeCoordinator() -> Coordinator {
        Coordinator(model: model)
    }

    func makeUIView(context: Context) -> PDFView {
        let pdfView = PDFView()
        pdfView.isUserInteractionEnabled = false
        pdfView.displaysPageBreaks = true
        pdfView.autoScales = true

        model.pdfView = pdfView

        if let doc = model.document, !model.isLocked {
            pdfView.document = doc
            applyConfiguration(to: pdfView)
            if let page = doc.page(at: model.currentPageIndex) {
                pdfView.go(to: page)
            }
        }

        NotificationCenter.default.addObserver(
            context.coordinator,
            selector: #selector(Coordinator.handlePageChanged(_:)),
            name: Notification.Name.PDFViewPageChanged,
            object: pdfView
        )

        return pdfView
    }

    func updateUIView(_ pdfView: PDFView, context: Context) {
        if model.pdfView !== pdfView {
            model.pdfView = pdfView
        }

        if let doc = model.document, !model.isLocked {
            if pdfView.document !== doc {
                pdfView.document = doc
                if let page = doc.page(at: model.currentPageIndex) {
                    pdfView.go(to: page)
                }
            }
            applyConfiguration(to: pdfView)

            if let targetPage = doc.page(at: model.currentPageIndex),
               pdfView.currentPage != targetPage {
                pdfView.go(to: targetPage)
            }
        }
    }

    static func dismantleUIView(_ pdfView: PDFView, coordinator: Coordinator) {
        NotificationCenter.default.removeObserver(
            coordinator,
            name: Notification.Name.PDFViewPageChanged,
            object: pdfView
        )
    }

    private func applyConfiguration(to pdfView: PDFView) {
        // 보기 모드 설정
        switch model.displayModeOption {
        case .singlePage:
            if pdfView.displayMode != .singlePage {
                pdfView.displayMode = .singlePage
            }
            if pdfView.displayDirection != .horizontal {
                pdfView.displayDirection = .horizontal
            }
            pdfView.displaysAsBook = false
        case .continuous:
            if pdfView.displayMode != .singlePageContinuous {
                pdfView.displayMode = .singlePageContinuous
            }
            if pdfView.displayDirection != .vertical {
                pdfView.displayDirection = .vertical
            }
            pdfView.displaysAsBook = false
        case .twoUp:
            if pdfView.displayMode != .twoUp {
                pdfView.displayMode = .twoUp
            }
            if pdfView.displayDirection != .horizontal {
                pdfView.displayDirection = .horizontal
            }
            pdfView.displaysAsBook = true
        }

        // 테마 배경색 설정
        switch model.theme {
        case .original:
            pdfView.backgroundColor = UIColor(white: 0.15, alpha: 1.0)
        case .night:
            // Night 모드에서는 뷰 전체에 colorInvert가 걸리므로 밝은 배경을 주면 반전 후 어두운 배경이 됨
            pdfView.backgroundColor = UIColor(white: 0.90, alpha: 1.0)
        case .sepia:
            pdfView.backgroundColor = UIColor(red: 0.24, green: 0.21, blue: 0.16, alpha: 1.0)
        }

        // 확대/맞춤 배율 적용
        model.applyScaleToPDFView()
    }

    final class Coordinator: NSObject {
        var model: PDFReaderModel

        init(model: PDFReaderModel) {
            self.model = model
        }

        @objc func handlePageChanged(_ notification: Notification) {
            guard let pdfView = notification.object as? PDFView,
                  let doc = pdfView.document,
                  let currentPage = pdfView.currentPage else {
                return
            }
            let pageIndex = doc.index(for: currentPage)
            if pageIndex != NSNotFound {
                DispatchQueue.main.async { [weak self] in
                    self?.model.onPDFViewPageChanged(pageIndex)
                }
            }
        }
    }
}