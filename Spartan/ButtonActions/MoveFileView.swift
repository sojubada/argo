//
//  MoveFileView.swift
//  Spartan
//
//  Created by RealKGB on 4/5/23.
//

import SwiftUI

struct MoveFileView: View {
    @Binding var fileNames: [String]
    @Binding var filePath: String
    @Binding var multiSelect: Bool
    @State var newFileName: String = ""
    @State var newFilePath: String = ""
    @Binding var isPresented: Bool
    /// 실패 이유. 있으면 닫지 않고 보여 준다.
    @State private var error: String?
    /// 옮긴 것을 (원래 자리, 새 자리) 로 알린다. 되돌리기(⌘Z)에 쓴다.
    /// 전에는 이 창으로 옮긴 것은 기록이 남지 않아 되돌릴 수 없었다.
    var onMoved: (([(from: String, to: String)]) -> Void)? = nil
    
    var body: some View {
        VStack{
            Text(NSLocalizedString("MOVE_TITLE", comment: "- We are!"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 60)
                }
            ArgoTextField(NSLocalizedString("DEST_PATH", comment: "- Bee-men."), text: $newFilePath, onEditingChanged: { (isEditing) in
                if !isEditing {
                    if(!(newFilePath.hasSuffix("/"))){
                        newFilePath = newFilePath + "/"
                    }
                }
            })
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
            
            if(fileNames.count == 1){
                ArgoTextField(NSLocalizedString("NEW_FILENAME", comment: "- Amen!") + NSLocalizedString("OPTIONAL", comment: "Hallelujah!"), text: $newFileName)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
        
            if let error {
                Text(error).foregroundColor(.red).multilineTextAlignment(.center)
            }
            Button(action: {
                var moved: [(from: String, to: String)] = []
                let r = runFileBatch(names: fileNames, from: filePath, to: newFilePath,
                                     rename: newFileName) { from, to in
                    let ok = RootHelperActs.mv(from, to)
                    if ok { moved.append((from: from, to: to)) }
                    return ok
                }
                onMoved?(moved)
                if r.failed == 0 {
                    multiSelect = false
                    isPresented = false
                } else {
                    error = LF("{0}개 중 {1}개를 옮기지 못했습니다.\n", fileNames.count, r.failed) + (r.reason ?? "")
                }
            }) {
                Text(NSLocalizedString("CONFIRM", comment: "Students, faculty, distinguished bees,"))
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
        }
        .onAppear {
            newFilePath = filePath
            if(!multiSelect){
                newFileName = fileNames[0]
            }
        }
    }
}
