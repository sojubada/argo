//
//  WebKitProbeView.swift
//  Argo
//
//  Thin SwiftUI wrapper so the probe can be reached from Settings.
//  All of the work is in WebKitProbe.m; this just puts it on screen
//  and carries the Menu button back out again.
//

import SwiftUI
import UIKit

struct WebKitProbeView: UIViewControllerRepresentable {
    @Binding var isPresented: Bool

    func makeUIViewController(context: Context) -> ArgoWebKitProbeViewController {
        let controller = ArgoWebKitProbeViewController()
        controller.onClose = {
            isPresented = false
        }
        return controller
    }

    func updateUIViewController(_ uiViewController: ArgoWebKitProbeViewController,
                                context: Context) {
    }
}
