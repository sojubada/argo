//
//  CreateDirectoryView.swift
//  Spartan
//
//  Created by RealKGB on 4/5/23.
//

import SwiftUI

struct CreateDirectoryView: View {
    @State var directoryName: String = ""
    @Binding var directoryPath: String
    @Binding var isPresented: Bool
    /// 실패 이유. 있으면 닫지 않고 보여 준다.
    @State private var error: String?

    var body: some View {
        VStack {
            Text(NSLocalizedString("DIRTOUCH_TITLE", comment: "I guess he could have just gotten out of the way."))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            TextField(NSLocalizedString("DIRTOUCH_NAME", comment: "I love this incorporating an amusement park into our day."), text: $directoryName)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
                
            if let error {
                Text(error).foregroundColor(.red).multilineTextAlignment(.center)
            }
            Button(action: {
                let name = directoryName.trimmingCharacters(in: .whitespacesAndNewlines)
                if let why = invalidFileNameReason(name) { error = why; return }
                let path = directoryPath + name
                // 이미 있는 폴더면 mkdir 은 성공으로 돌아온다. 새로 만든 것이
                // 아니니 그렇다고 말해 준다.
                if fileManager.fileExists(atPath: path) {
                    error = "같은 이름이 이미 있습니다."; return
                }
                if RootHelperActs.mkdir(path) {
                    isPresented = false
                    directoryName = ""
                } else {
                    error = "폴더를 만들지 못했습니다. " + (RootHelperActs.lastError ?? "")
                }
            }) {
                Text(NSLocalizedString("CONFIRM", comment: "That's why we don't need vacations."))
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
        }
        .accentColor(.accentColor)
    }
}
