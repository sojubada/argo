//
//  KeyboardProbe.swift
//  Argo
//
//  COMPILE-ONLY PROBE. Nothing calls this.
//
//  The SDK headers mention UIKeyCommand, keyboardShortcut and
//  onKeyPress, but a header mentioning a name is not the same as tvOS
//  offering it — PDFKit is in Apple's documentation for tvOS and is not
//  in the SDK. This file is here to make the compiler settle which of
//  them Argo can actually be built on, since the answer decides whether
//  shortcuts are written in SwiftUI or through the responder chain.
//
//  Delete once the keyboard layer ships.
//

import SwiftUI
import UIKit

// MARK: - The UIKit responder-chain route

final class KeyboardProbeResponder: UIViewController {

    override var keyCommands: [UIKeyCommand]? {
        return [
            UIKeyCommand(title: "Copy",
                         action: #selector(probeCopy),
                         input: "c",
                         modifierFlags: .command),
            UIKeyCommand(title: "Paste",
                         action: #selector(probePaste),
                         input: "v",
                         modifierFlags: .command),
            UIKeyCommand(title: "Select All",
                         action: #selector(probeSelectAll),
                         input: "a",
                         modifierFlags: .command),
            UIKeyCommand(title: "Compress",
                         action: #selector(probeCompress),
                         input: "z",
                         modifierFlags: [.command, .shift]),
            UIKeyCommand(title: "Parent Folder",
                         action: #selector(probeUp),
                         input: UIKeyInputUpArrow,
                         modifierFlags: .command),
        ]
    }

    @objc private func probeCopy() {}
    @objc private func probePaste() {}
    @objc private func probeSelectAll() {}
    @objc private func probeCompress() {}
    @objc private func probeUp() {}

    // Raw key events, for type-ahead: typing letters to jump to a file.
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
            guard let key = press.key else { continue }
            let characters = key.charactersIgnoringModifiers
            let hasCommand = key.modifierFlags.contains(.command)
            let hasShift = key.modifierFlags.contains(.shift)
            let isSpace = key.keyCode == UIKeyboardHIDUsage.keyboardSpacebar
            let isReturn = key.keyCode == UIKeyboardHIDUsage.keyboardReturnOrEnter
            let isDelete = key.keyCode == UIKeyboardHIDUsage.keyboardDeleteOrBackspace
            _ = (characters, hasCommand, hasShift, isSpace, isReturn, isDelete)
        }
        super.pressesBegan(presses, with: event)
    }
}

// MARK: - The SwiftUI route, if tvOS allows it

struct KeyboardProbeView: View {
    var body: some View {
        VStack {
            Button("probe") {}
            #if os(tvOS)
            // Left deliberately bare: whether .keyboardShortcut compiles
            // for tvOS is exactly what is being asked.
            #endif
        }
    }
}

#if compiler(>=5.9)
@available(tvOS 17.0, *)
struct KeyboardProbeModernView: View {
    var body: some View {
        Color.clear
            .focusable()
    }
}
#endif
