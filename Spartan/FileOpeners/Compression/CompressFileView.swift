//
//  ZipFileView.swift
//  Spartan
//
//  Created by RealKGB on 4/15/23.
//

import SwiftUI
import Zip
import libarchiveBridge

let compTypes = ["zip", "gzip", "bzip2", "xz", "lzma", "lz4", "zstd", "tar"]
let tarCompTypes = ["gzip", "bzip2", "xz", "lzma"]

struct CompressFileView: View {
    @Binding var isPresented: Bool
    @State var directory: String
    @State var fileNames: [String]
    @State var multipleFiles: Bool
    
    @State private var selectedCompType: CompressionType = .zip
    @State private var selectedTarCompType: TarCompressionType = .none
    @State private var compPassword: String = ""
    
    @State var tarCompShow = false
    
    @State private var destPath = ""
    
    @State var error2: CompressionError?
    @State var errorShow = false
    @StateObject private var runner = CompressionRunner()
    
    var body: some View {
        VStack {
            if runner.isRunning {
                CompressionProgressView(runner: runner, fileName: destPath)
            } else {
            Text(localizedString: "COMP_TITLE")
                .font(.system(size: 60))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 25)
                }
            
            Picker(LocalizedString("COMP_TYPE"), selection: $selectedCompType) {
                Text("zip").tag(CompressionType.zip)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                    }
                if !multipleFiles {
                    Text("gzip").tag(CompressionType.gz).disabled(multipleFiles)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("bzip2").tag(CompressionType.bz2).disabled(multipleFiles)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("xz").tag(CompressionType.xz).disabled(multipleFiles)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("lzma").tag(CompressionType.lzma).disabled(multipleFiles)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("lz4").tag(CompressionType.lz4).disabled(multipleFiles)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                }
                Text("zstd").tag(CompressionType.zstd)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                    }
                Text("tar").tag(CompressionType.tar)
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                    }
            }
            
            if tarCompShow {
                Picker(LocalizedString("COMP_TARTYPE"), selection: $selectedTarCompType) {
                    Text("none").tag(TarCompressionType.none)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("gzip").tag(TarCompressionType.gz)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("bzip2").tag(TarCompressionType.bz2)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("xz").tag(TarCompressionType.xz)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                    Text("lzma").tag(TarCompressionType.lzma)
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 35)
                        }
                }
            }
            
            ArgoTextField(NSLocalizedString("COMP_DESTNAME", comment: ""), text: $destPath, onCommit: {
                let ext = selectedCompType.stringRepresentation() + selectedTarCompType.stringRepresentation()
                if destPath.suffix(ext.count) != ext {
                    destPath += ext
                }
            })
            
            if error2 != nil {
                Text("")
                    .onAppear {
                        errorShow = true
                    }
                    .alert(isPresented: $errorShow, content: {
                        Alert(
                            title: Text(NSLocalizedString("ERROR", comment: "")),
                            message: Text(error2?.explanation() ?? LocalizedString("ERROR_EXPLANATION")),
                            dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "")))
                        )
                    })
            }
            
            Button(action: {
                // 뒷줄에서 돈다. 메인에서 돌리면 10GB 파일에 화면이 멈춘다.
                let multiple = multipleFiles
                let names = fileNames
                let dir = directory
                let dest = directory + destPath
                let comp = selectedCompType
                let tarComp = selectedTarCompType
                runner.run(L("압축하는 중"), work: {
                    if multiple {
                        try Compression.compressFiles(listOfFiles: names, directory: dir, destination: dest, compType: comp, tarCompType: tarComp)
                    } else {
                        try Compression.compressFile(filePath: names[0], directory: dir, destination: dest, compType: comp, tarCompType: tarComp)
                    }
                }, onFinish: {
                    isPresented = false
                })
            }) {
                Text(localizedString: "CONFIRM")
            }
            
            if selectedCompType == .tar {
                Text("")
                    .onAppear {
                        withAnimation {
                            tarCompShow = true
                        }
                    }
            } else {
                Text("")
                    .onAppear {
                        withAnimation {
                            tarCompShow = false
                        }
                    }
            }
            }
        }
        .onReceive(runner.$failure.compactMap { $0 }) { err in
            error2 = err as? CompressionError
        }
    }
}

struct UncompressFileView: View {
    @Binding var isPresented: Bool
    @State var filePath: String
    @State var fileName: String
    @State private var extractFilePath: String = ""
    
    @State var errorShow = false
    @State var error2: CompressionError?
    @StateObject private var runner = CompressionRunner()
    
    var body: some View {
        VStack {
            if runner.isRunning {
                CompressionProgressView(runner: runner, fileName: fileName)
            } else {
            Text(localizedString: "UNCOMP_TITLE")
                .font(.system(size: 60))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 60)
                }
            ArgoTextField(NSLocalizedString("UNCOMP_DIR", comment: ""), text: $extractFilePath)
                .onAppear {
                    extractFilePath = filePath
                }
            
            if error2 != nil {
                Text("")
                    .onAppear {
                        errorShow = true
                    }
                    .alert(isPresented: $errorShow, content: {
                        Alert(
                            title: Text(NSLocalizedString("ERROR", comment: "")),
                            message: Text(error2?.explanation() ?? LocalizedString("ERROR_EXPLANATION")),
                            dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "")))
                        )
                    })
            }
            
            Button(action: {
                // 뒷줄에서 돈다. 공간 검사는 FileGuard 가 헤더를 훑으며 이미 한다.
                let archive = filePath + fileName
                let dest = extractFilePath
                runner.run(L("압축을 푸는 중"), work: {
                    try Compression.uncompressFile(filePath: archive,
                                                   destination: dest,
                                                   compType: Compression.archiveType(filePath: archive))
                }, onFinish: {
                    isPresented = false
                })
            }) {
                Text("CONFIRM")
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
            }
            .disabled(extractFilePath == "")
            }
        }
        .onReceive(runner.$failure.compactMap { $0 }) { err in
            error2 = err as? CompressionError
        }
    }
}
