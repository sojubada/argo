//
//  RenameFileView.swift
//  Spartan
//
//  Created by RealKGB on 4/5/23.
//

import SwiftUI

struct RenameFileView: View {
    @Binding var fileName: String
    @Binding var newFileName: String
    @Binding var filePath: String
    @Binding var isPresented: Bool
    /// 바꾼 뒤 부른다. (원래 경로, 새 경로) — 되돌리기 기록에 쓴다.
    var onRenamed: ((String, String) -> Void)? = nil

    /// 실패한 이유. 있으면 창을 닫지 않고 보여 준다.
    ///
    /// 전에는 결과를 보지 않고 바로 닫았다. 실패해도 창은 닫히고 목록은
    /// 그대로라, 이름이 왜 안 바뀌었는지 알 길이 없었다.
    @State private var error: String?

    var body: some View {
        Text(NSLocalizedString("RENAME_TITLE", comment: "Boy, quite a bit of pomp..."))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 60)
            }
        ArgoTextField(NSLocalizedString("RENAME_NAME", comment: "under the circumstances."), text: $newFileName)
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
        if let error {
            Text(error)
                .foregroundColor(.red)
                .multilineTextAlignment(.center)
        }
        Button(NSLocalizedString("CONFIRM", comment: "Well, Adam, today we are men.")) {
            commit()
        }
    }

    private func commit() {
        // 폴더 이름은 끝에 '/' 가 붙어 온다. 비교와 검사는 그것을 떼고 한다.
        func bare(_ n: String) -> String {
            var t = n.trimmingCharacters(in: .whitespacesAndNewlines)
            while t.count > 1 && t.hasSuffix("/") { t.removeLast() }
            return t
        }
        let old = bare(fileName)
        let new = bare(newFileName)
        if new == old { isPresented = false; return }   // 바뀐 것이 없다
        if let why = invalidFileNameReason(new) { error = why; return }

        let from = filePath + old
        let to = filePath + new
        if RootHelperActs.mv(from, to) {
            onRenamed?(from, to)
            isPresented = false
        } else {
            error = "이름을 바꾸지 못했습니다. " + (RootHelperActs.lastError ?? "")
        }
    }
}
