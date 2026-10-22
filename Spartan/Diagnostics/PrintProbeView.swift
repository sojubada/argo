//
//  PrintProbeView.swift
//  Argo
//

import SwiftUI

@available(tvOS 13.0, *)
struct PrintProbeView: UIViewControllerRepresentable {
    @Binding var isPresented: Bool

    func makeUIViewController(context: Context) -> ArgoPrintProbeViewController {
        let vc = ArgoPrintProbeViewController()
        vc.onClose = { isPresented = false }
        return vc
    }

    func updateUIViewController(_ vc: ArgoPrintProbeViewController, context: Context) {}
}
