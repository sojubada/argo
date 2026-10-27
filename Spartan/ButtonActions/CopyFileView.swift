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
            
                Button(action: {
                    if(multiSelect){
                        if(fileNames.count > 1){
                            for fileName in fileNames {
                                copyFile(path: filePath + fileName, newPath: newFilePath + fileName)
                            }
                        } else {
                            copyFile(path: filePath + fileNames[0], newPath: newFilePath + newFileName)
                        }
                    } else if(newFileName == ""){
                        copyFile(path: filePath + fileNames[0], newPath: newFilePath + fileNames[0])
                    } else {
                        copyFile(path: filePath + fileNames[0], newPath: newFilePath + newFileName)
                    }
                    fileNames[0] = ""
                    multiSelect = false
                    isPresented = false
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
