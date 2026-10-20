//
//  WebDocProbeView.swift
//  Argo
//

import SwiftUI
import UIKit

struct WebDocProbeView: UIViewControllerRepresentable {
    @Binding var isPresented: Bool

    func makeUIViewController(context: Context) -> ArgoWebDocProbeViewController {
        let controller = ArgoWebDocProbeViewController()
        controller.onClose = { isPresented = false }
        return controller
    }

    func updateUIViewController(_ uiViewController: ArgoWebDocProbeViewController,
                                context: Context) {
    }
}
