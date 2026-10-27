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
    
    var body: some View {
        VStack{
            Text(NSLocalizedString("MOVE_TITLE", comment: "- We are!"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 60)
                }
            TextField(NSLocalizedString("DEST_PATH", comment: "- Bee-men."), text: $newFilePath, onEditingChanged: { (isEditing) in
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
                TextField(NSLocalizedString("NEW_FILENAME", comment: "- Amen!") + NSLocalizedString("OPTIONAL", comment: "Hallelujah!"), text: $newFileName)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
        
            if let error {
                Text(error).foregroundColor(.red).multilineTextAlignment(.center)
            }
            Button(action: {
                let r = runFileBatch(names: fileNames, from: filePath, to: newFilePath,
                                     rename: newFileName) { RootHelperActs.mv($0, $1) }
                if r.failed == 0 {
                    multiSelect = false
                    isPresented = false
                } else {
                    error = "\(fileNames.count)개 중 \(r.failed)개를 옮기지 못했습니다.\n" + (r.reason ?? "")
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
