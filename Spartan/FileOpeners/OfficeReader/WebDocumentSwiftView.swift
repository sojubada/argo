//
//  WebDocumentSwiftView.swift
//  Argo
//
//  Puts the WebKit document viewer on screen from SwiftUI.
//

import SwiftUI
import UIKit

struct WebDocumentSwiftView: UIViewControllerRepresentable {
    let filePath: String
    let fileName: String
    @Binding var isPresented: Bool

    private var fullPath: String {
        if filePath.hasSuffix("/") { return filePath + fileName }
        if filePath.isEmpty { return fileName }
        return filePath + "/" + fileName
    }

    func makeUIViewController(context: Context) -> ArgoWebDocumentViewController {
        let controller = ArgoWebDocumentViewController()
        controller.documentPath = fullPath
        controller.onClose = { isPresented = false }
        return controller
    }

    func updateUIViewController(_ uiViewController: ArgoWebDocumentViewController,
                                context: Context) {
    }
}
