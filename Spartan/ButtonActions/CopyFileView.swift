//
//  CopyFileView.swift
//  Spartan
//
//  Created by RealKGB on 4/5/23.
//

import SwiftUI

struct CopyFileView: View {
    @Binding var fileNames: [String]
    @Binding var filePath: String
    @Binding var multiSelect: Bool
    @State var newFileName: String = ""
    @State var newFilePath: String = ""
    @Binding var isPresented: Bool
    /// 실패 이유. 있으면 닫지 않고 보여 준다.
    @State private var error: String?
    
    var body: some View {
        if(fileNames.count == 0) {
            Text("Please select at least one file")
        } else {
            VStack {
                Text(NSLocalizedString("COPY_TITLE", comment: "please welcome Dean Buzzwell."))
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 60)
                    }
                TextField(NSLocalizedString("DEST_PATH", comment: "Welcome, New Hive City graduating class of..."), text: $newFilePath, onEditingChanged: { (isEditing) in
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
                    TextField(NSLocalizedString("NEW_FILENAME", comment: "...9:15.") + NSLocalizedString("OPTIONAL", comment: "That concludes our ceremonies."), text: $newFileName)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
                }
            
                if let error {
                    Text(error).foregroundColor(.red).multilineTextAlignment(.center)
                }
                Button(action: {
                    // 같은 폴더에 같은 이름으로 복사하면 원본과 부딪친다. 새
                    // 이름을 안 적었으면 겹치지 않는 이름으로 비켜 쓴다.
                    let r = runFileBatch(names: fileNames, from: filePath, to: newFilePath,
                                         rename: newFileName) { src, dst in
                        let target = fileManager.fileExists(atPath: dst)
                            ? PaneTransfer.nonClashingPath(dst) : dst
                        return copyFile(path: src, newPath: target)
                    }
                    if r.failed == 0 {
                        multiSelect = false
                        isPresented = false
                    } else {
                        error = "\(fileNames.count)개 중 \(r.failed)개를 복사하지 못했습니다.\n" + (r.reason ?? "")
                    }
                }) {
                    Text(NSLocalizedString("CONFIRM", comment: "And begins your career at Honex Industries!"))
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
    
    /// 복사. 반드시 RootHelperActs 를 거친다.
    ///
    /// 전에는 도우미를 직접 띄웠다. 그러면 FileGuard 를 한 번도 지나지 않아,
    /// 안전 모드에서 막혀야 할 시스템 폴더로의 복사도 그대로 시도됐다.
    @discardableResult
    func copyFile(path: String, newPath: String) -> Bool {
        RootHelperActs.cp(path, newPath)
    }
}
