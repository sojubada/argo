//
//  CreateFileView.swift
//  Spartan
//
//  Created by RealKGB on 4/5/23.
//

import SwiftUI

struct CreateFileView: View {
    @State var fileName: String = ""
    @Binding var filePath: String
    @Binding var isPresented: Bool
    /// 실패 이유. 있으면 닫지 않고 보여 준다.
    @State private var error: String?

    var body: some View {
        VStack {
            Text(NSLocalizedString("TOUCH_TITLE", comment: "Everybody knows, sting someone, you die."))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            ArgoTextField(NSLocalizedString("TOUCH_NAME", comment: "Don't waste it on a squirrel."), text: $fileName)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
                
            if let error {
                Text(error).foregroundColor(.red).multilineTextAlignment(.center)
            }
            Button(action: {
                let name = fileName.trimmingCharacters(in: .whitespacesAndNewlines)
                if let why = invalidFileNameReason(name) { error = why; return }
                if RootHelperActs.touch(filePath + name) {
                    fileName = ""
                    isPresented = false
                } else {
                    error = "파일을 만들지 못했습니다. " + (RootHelperActs.lastError ?? "")
                }
            }) {
                Text(NSLocalizedString("CONFIRM", comment: "Such a hothead."))
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
        }
        .accentColor(.accentColor)
    }
}
