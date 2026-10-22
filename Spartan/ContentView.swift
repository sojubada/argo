//
//  ContentView.swift
//  Spartan
//
//  Created by RealKGB on 4/3/23.
//
//  This is the biggest file in Spartan. It's also the worst since it has the most stuff going on, and not all of it was coded well. Perks of learning as you go.
//  Some of it is unused leftovers, I just don't know what all isn't used so it's left as-is.
//

import SwiftUI
import UIKit
import Foundation
import AVKit
import AVFoundation
import MobileCoreServices
import ApplicationsWrapper
import AssetCatalogWrapper
import SVGWrapper

struct ContentView: View {
    // 칸 둘. 왼쪽/오른쪽이 각자 폴더와 선택을 들고 있다.
    @StateObject var leftPane: FilePaneModel
    @StateObject var rightPane: FilePaneModel
    @State var activeSide: PaneSide = .left
    /// 두 칸을 함께 보일지. 끄면 한 칸만 나와 기존 동작과 같다.
    @AppStorage("dualPaneEnabled") var dualPaneEnabled = false

    /// 활성 칸. 툴바의 모든 동작은 이쪽을 대상으로 한다.
    var pane: FilePaneModel { activeSide == .left ? leftPane : rightPane }
    /// 반대 칸. 붙여넣기가 가는 곳이다.
    var otherPane: FilePaneModel { activeSide == .left ? rightPane : leftPane }

    // 아래 네 개는 기존 코드가 쓰던 이름 그대로다. 안쪽만 활성 칸을 가리킨다.
    // 구조체이지만 pane이 클래스 참조라 nonmutating set이 성립한다.
    var directory: String {
        get { pane.directory }
        nonmutating set { pane.directory = newValue }
    }
    var masterFiles: [SpartanFile] {
        get { pane.masterFiles }
        nonmutating set { pane.masterFiles = newValue }
    }
    var multiSelect: Bool {
        get { pane.multiSelect }
        nonmutating set { pane.multiSelect = newValue }
    }
    var multiSelectFiles: [String] {
        get { pane.multiSelectFiles }
        nonmutating set { pane.multiSelectFiles = newValue }
    }
    var allWereSelected: Bool {
        get { pane.allWereSelected }
        nonmutating set { pane.allWereSelected = newValue }
    }
    var currentDirRisk: PathRisk { pane.currentDirRisk }

    var directoryBinding: Binding<String> {
        Binding(get: { pane.directory }, set: { pane.directory = $0 })
    }
    var multiSelectFilesBinding: Binding<[String]> {
        Binding(get: { pane.multiSelectFiles }, set: { pane.multiSelectFiles = $0 })
    }

    init(directory: String = "/var/mobile/Media/",
         isRootless: Bool = false,
         scaleFactor: CGFloat = 1,
         globalAVPlayer: Binding<AVPlayer>) {
        _leftPane = StateObject(wrappedValue: FilePaneModel(directory: directory))
        _rightPane = StateObject(wrappedValue: FilePaneModel(directory: directory))
        _isRootless = State(initialValue: isRootless)
        _scaleFactor = State(initialValue: scaleFactor)
        _globalAVPlayer = globalAVPlayer
    }

    @State private var fileInfo: [String] = []
    @State var deleteOverride = false
    @State var E = false
    @State var E2 = false
    @State var isRootless: Bool
    
    @State var scaleFactor: CGFloat
    @State var buttonCalc = false
    
    @State var didSearch = false
    
    @State var showAlert = false
    @State var alertTitle = ""
    @State var alertMsg = ""
    
    @State var newViewFilePath: String = ""
    @State var newViewArrayNames: [String] = [""]
    @State var newViewFileName: String = ""
    @State private var newViewFileIndex = 0
    
    @State var renameFileCurrentName: String = ""
    @State var renameFileNewName: String = ""
    
    @State var filePerms = 420
    
    @State var didChangeDir = false
    
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 39)
    //createFileSelectShow = 0
    //contextMenuShow = 1
    //openInMenu = 2
    //fileInfoShow = 3
    //textShow = 4
    //createFileShow = 5
    //createDirectoryShow = 6
    //renameFileShow = 7
    //moveFileShow = 8
    //copyFileShow = 9
    //audioPlayerShow = 10
    //videoPlayerShow = 11
    //imageShow = 12
    //plistShow = 13
    //compFileShow = 14
    //spawnShow = 15 - only works jb
    //favoritesShow = 16
    //addToFavoritesShow = 17
    //settingsShow = 18
    //searchShow = 19
    //symlinkShow = 20
    //mountPointsShow = 21
    //hexShow = 22
    //dpkgViewShow = 23 - only works jb
    //dpkgDebViewShow = 24 - only works jb
    //webDAVShow = 25 (was webServerShow)
    //fileNotFoundView = 26
    //filePermsEdit = 27
    //carView = 28
    //tvOS13serverShow = 29  # some things are like this due to tvOS 13 (no context menu) limitations. I would drop 13 if I could but I'm not abandoning 13.4.8 people.
    //fontViewShow = 30
    //choosing between opening an image as an SVG or not an SVG = 31
    //dmgMountViewShow = 32 - only works jb
    //htmlViewShow = 33
    //uncompFileShow = 34
    //notJailbrokenView = 35
    //pdfViewShow = 36
    //trashViewShow = 37
    //officeDocViewShow = 38
    //batchDeletePreviewShow = 39 (안전장치 오버레이 대신 시트/상태 활용)
    
    @Binding var globalAVPlayer: AVPlayer 
    @State var isGlobalAVPlayerPlaying = false
    @State var callback = true
    
    @State private var isLoadingView = false
    @State var blankString: [String] = [""] 
    @State private var nonexistentFile = "" 
    
    @State private var isImageSVG: [Bool] = [false, false]
    
    // Safety Variables — 배너는 칸 모델이 들고 있다 (어느 칸이 시스템 폴더인지 보여야 하므로)
    var showSafetyBanner: Bool { pane.showBanner }
    var safetyBannerRisk: PathRisk { pane.bannerRisk }
    var safetyBannerMsg: String { pane.bannerMsg }
    @State private var showBatchDeletePreview = false
    @State private var cautionCountForBatch = 0
    
    var body: some View {
        ZStack {
            VStack {
                VStack {
                    HStack { //input directory + refresh
                    TextField(NSLocalizedString("INPUT_DIRECTORY", comment: "According to all known laws of aviation"), text: directoryBinding, onCommit: {
                        if directory == "" {
                            directory = "/"
                        }
                        directPathTypeCheckNewViewFileVariableSetter()
                        defaultAction(index: 0, isDirectPath: true)
                        
                        let regex = try! NSRegularExpression(pattern: "^[a-zA-Z0-9_\\-\\./]+$")
                        if regex.firstMatch(in: directory, options: [], range: NSRange(location: 0, length: directory.utf16.count)) == nil {
                            directory = "/"
                        }
                    })
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
                    .onAppear {
                        if(!buttonCalc) {
                            buttonWidth *= scaleFactor
                            buttonHeight *= scaleFactor
                            if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) {
                                buttonWidth *= 1.5
                            }
                            buttonCalc = true
                        }
                    }
                    
                    Button(action: {
                        updateFiles()
                    }) {
                        Image(systemName: "arrow.clockwise")
                    }
                    .sheet(isPresented: $showSubView[6], content: {
                        CreateDirectoryView(directoryPath: directoryBinding, isPresented: $showSubView[6])
                    })
                    .sheet(isPresented: $showSubView[5], content: {
                        CreateFileView(filePath: directoryBinding, isPresented: $showSubView[5])
                    })
                    .sheet(isPresented: $showSubView[20], content: {
                        CreateSymlinkView(symlinkPath: directoryBinding, isPresented: $showSubView[20])
                    })
                }
                HStack {
                    Button(action: { //quit
                        quitArgo()
                    }) {
                        Image(systemName: "power")
                            .frame(width: 50, height: 50)
                    }

                    Button(action: { //두 칸 켜고 끄기
                        withAnimation { dualPaneEnabled.toggle() }
                        if dualPaneEnabled && rightPane.masterFiles.isEmpty {
                            rightPane.updateFiles()
                        }
                    }) {
                        Image(systemName: dualPaneEnabled
                              ? "rectangle.split.2x1.fill"
                              : "rectangle.split.2x1")
                            .frame(width: 50, height: 50)
                    }

                    if dualPaneEnabled {
                        Button(action: { //반대 칸으로 복사
                            copySelectionToOtherPane(move: false)
                        }) {
                            Image(systemName: activeSide == .left
                                  ? "arrow.right.doc.on.clipboard"
                                  : "arrow.left.circle")
                                .frame(width: 50, height: 50)
                        }

                        Button(action: { //반대 칸으로 이동
                            copySelectionToOtherPane(move: true)
                        }) {
                            Image(systemName: "arrow.left.arrow.right")
                                .frame(width: 50, height: 50)
                        }

                        Button(action: { //반대 칸을 이 폴더로 맞추기
                            syncOtherPaneToActive()
                        }) {
                            Image(systemName: "equal.square")
                                .frame(width: 50, height: 50)
                        }
                    }

                    debugMenu
                        .frame(alignment: .leading)
                    HStack {
                        Button(action: {
                            if(multiSelect) {
                                multiSelectFiles = masterFiles.map { $0.name }
                                allWereSelected.toggle()
                                if(allWereSelected) {
                                    iterateOverFileWasSelected(boolToIterate: true)
                                } else {
                                    iterateOverFileWasSelected(boolToIterate: false)
                                }
                            } else {
                                resetMultiSelectArrays()
                                withAnimation {
                                    multiSelect = true
                                }
                            }
                        }) {
                            if (multiSelect) { //multiselect is a robust feature. the issue I had the most trouble with was with updateFiles, since the number of files would change but my multiselect array was too long. so I had to work out resizing the array.
                            //i gave up on that, and I just reset it to [] and then the length of masterFiles. it works.
                                if (allWereSelected) {
                                    Image(systemName: "checkmark.circle")
                                        .frame(width: 50, height: 50)
                                } else {
                                    Image(systemName: "circle")
                                        .frame(width: 50, height: 50)
                                }
                            } else {
                                Image(systemName: "checkmark.circle")
                                    .frame(width: 50, height: 50)
                            }
                        }
                        
                        if (!multiSelect) {
                            Button(action: {
                                showSubView[19] = true
                                newViewFilePath = directory
                            }) {
                                Image(systemName: "magnifyingglass")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[19], onDismiss: {
                                if(didSearch) {
                                    if(FileInfo.isDirectory(filePath: newViewFilePath)) {
                                        directory = newViewFilePath
                                    } else {
                                        directory = URL(fileURLWithPath: newViewFilePath).deletingLastPathComponent().path + "/"
                                        masterFiles.append(SpartanFile(name: URL(fileURLWithPath: newViewFilePath).lastPathComponent, fullPath: newViewFilePath, isSelected: false, fileType: FileInfo.yandereDevFileType(file: newViewFilePath), isLoadingFile: false))
                                        defaultAction(index: masterFiles.count-1, isDirectPath: false)
                                    }
                                    didSearch = false
                                }
                            }, content: {
                                SearchView(directory: directoryBinding, isPresenting: $showSubView[19], selectedFile: $newViewFilePath, didSearch: $didSearch)
                            }) //my search function is very complex and i'm not sure how I wrote it. i don't fully understand it now and i didn't understand it then, either. I know it works and so im happy
                            //someday I'll figure it out. It's surprisingly good
                    
                            Button(action: { //new file/directory/symlink
                                showSubView[0] = true
                            }) {
                                if #available(tvOS 14.0, *){
                                    Image(systemName: "doc.badge.plus")
                                        .frame(width:50, height:50)
                                } else {
                                    Image(systemName: "doc")
                                        .frame(width:50, height:50)
                                }
                            }
                            
                            Button(action: {
                                showSubView[21] = true
                            }) {
                                Image(systemName: "externaldrive")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[21], onDismiss: {
                                if didChangeDir {
                                    updateFiles()
                                    resetShowSubView()
                                    didChangeDir = false
                                }
                            }, content: {
                                MountPointsView(directory: directoryBinding, isPresented: $showSubView[21], didChangeDir: $didChangeDir)
                            })

                            Button(action: { //webdav
                                showSubView[25] = true
                            }) {
                                Image(systemName: "globe")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[25], onDismiss: {
                                updateFiles()
                            }, content: {
                                WebDAVRootView(directory: directoryBinding, isPresented: $showSubView[25])
                            })
                            
                            /*if #available(tvOS 14.0, *) {
                                Button(action: {
                                    showSubView[25] = true
                                }) {
                                    Image(systemName: "server.rack")
                                        .frame(width:50, height:50)
                                }
                                .contextMenu {
                                    serverButton
                                }
                            } else {
                                Button(action: {
                                    showSubView[29] = true
                                }) {
                                    Image(systemName: "server.rack")
                                        .frame(width:50, height:50)
                                }
                                .sheet(isPresented: $showSubView[29], content: {
                                    serverButton
                                })
                            }*/
                            //this is the webserver button
                            
                            Button(action: { //favorites
                                showSubView[16] = true
                            }) {
                                Image(systemName: "star")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[16], onDismiss: {
                                updateFiles() //FavoritesView *can* change directory, but without an updateFiles the changes won't be reflected. it took me awhile to figure this bug out.
                            }, content: {
                                FavoritesView(directory: directoryBinding, showView: $showSubView[16])
                            })
                            .sheet(isPresented: $showSubView[17], content: {
                                AddToFavoritesView(filePath: newViewFilePath, displayName: newViewFileName, showView: $showSubView[17])
                            })
                        
                            Button(action: { //trash
                                showSubView[37] = true
                            }) {
                                Image(systemName: "trash")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[37], onDismiss: {
                                updateFiles()
                            }, content: {
                                TrashView(isPresented: $showSubView[37], onChanged: {
                                    updateFiles()
                                })
                            })

                            Button(action: { //settings
                                showSubView[18] = true
                            }) {
                                Image(systemName: "gear")
                                    .frame(width:50, height:50)
                            }
                            .sheet(isPresented: $showSubView[18], content: {
                                SettingsView()
                            })
                        } else {
                            Button(action: {
                                showSubView[8] = true
                                newViewFilePath = directory
                                newViewArrayNames = multiSelectFiles
                                resetMultiSelectArrays()
                            }) {
                                ZStack {
                                    Image(systemName: "doc.on.doc")
                                        .frame(width: 50, height: 50)
                                    Image(systemName: "arrow.right")
                                        .resizable()
                                        .frame(width: 15, height: 13)
                                        .offset(x: -4, y: 11.75) //i do not know how i came up with these values. i am not going to touch them. hopefully it doesn't break on non-1080p (i don't have this)
                                }
                            }
                    
                            Button(action: {
                                showSubView[9] = true
                                newViewFilePath = directory
                                newViewArrayNames = multiSelectFiles
                                resetMultiSelectArrays()
                            }) {
                                ZStack {
                                    Image(systemName: "doc.on.clipboard")
                                        .frame(width:50, height:50)
                                    Image(systemName: "arrow.right")
                                        .resizable()
                                        .frame(width:15, height:13)
                                        .offset(x:-3.75, y:11.75)
                                }
                            }
                        
                            Button(action: {
                                showSubView[14] = true
                                newViewFilePath = directory
                                newViewArrayNames = multiSelectFiles
                            }) {
                                Image(systemName: "doc.zipper")
                                    .frame(width:50, height:50)
                            }
                        
                            Button(action: {
                                if multiSelectFiles.isEmpty { return }
                                // 보안 검사 먼저 수행 (다중 선택)
                                let fullPaths = multiSelectFiles.map { directory + $0 }
                                switch FileGuard.authorize(action: "batch_delete", paths: fullPaths) {
                                case .success:
                                    cautionCountForBatch = fullPaths.filter { PathSafety.risk(of: $0) == .caution }.count
                                    showBatchDeletePreview = true // 승인 후 팝업 띄움
                                case .failure(let err):
                                    alertTitle = "작업 차단됨"
                                    alertMsg = err.localizedDescription
                                    showAlert = true
                                }
                            }) {
                                ZStack {
                                    if(directory == "/private/var/mobile/Media/.Trash/"){
                                        Image(systemName: "trash")
                                            .frame(width:50, height:50)
                                            .foregroundColor(.red)
                                    } else {
                                        Image(systemName: "trash")
                                            .frame(width:50, height:50)
                                    }
                                }
                            }
                        
                            Button(action: {
                                withAnimation {
                                    multiSelect = false
                                }
                                allWereSelected = false
                            }) {
                                Image(systemName: "xmark")
                                    .frame(width:50, height:50)
                            }
                        }
                    }
                    .frame(alignment: .center)
                    
                    freeSpace
                        .frame(alignment: .trailing)
                        .sheet(isPresented: $showSubView[15], content: {
                            SpawnView(filePath: $newViewFilePath, fileName: $newViewFileName, isPresented: $showSubView[15])
                        })
                        .sheet(isPresented: $showSubView[4], content: {
                            TextView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[4])
                        })
                        .sheet(isPresented: $showSubView[7], content: {
                            RenameFileView(fileName: $renameFileCurrentName, newFileName: $renameFileNewName, filePath: $newViewFilePath, isPresented: $showSubView[7])
                        })
                        .sheet(isPresented: $showSubView[8], content: {
                            MoveFileView(fileNames: $newViewArrayNames, filePath: $newViewFilePath, multiSelect: $multiSelect, isPresented: $showSubView[8])
                        })
                        .sheet(isPresented: $showSubView[9], content: {
                            CopyFileView(fileNames: $newViewArrayNames, filePath: $newViewFilePath, multiSelect: $multiSelect, isPresented: $showSubView[9])
                        })
                        .sheet(isPresented: $showSubView[11], content: {
                            VideoPlayerView(videoPath: $newViewFilePath, videoName: $newViewFileName, isPresented: $showSubView[11], player: globalAVPlayer)
                        })
                        .sheet(isPresented: $showSubView[10], content: {
                            if(callback){
                                AudioPlayerView(callback: callback, audioPath: $newViewFilePath, audioName: $newViewFileName, player: globalAVPlayer, isPresented: $showSubView[10])
                            } else {
                                AudioPlayerView(callback: callback, audioPath: $blankString[0], audioName: $blankString[0], player: globalAVPlayer, isPresented: $showSubView[10])
                            }
                        })
                        .sheet(isPresented: $showSubView[12], onDismiss: {
                            isImageSVG[0] = false
                        }, content: {
                            ImageView(imagePath: newViewFilePath, imageName: newViewFileName, isSVG: isImageSVG[0])
                        })
                        .sheet(isPresented: $showSubView[13], content: {
                            PlistView(filePath: newViewFilePath, fileName: newViewFileName)
                        })
                        .sheet(isPresented: $showSubView[14], onDismiss: {
                            withAnimation {
                                multiSelect = false
                            }
                            resetMultiSelectArrays()
                            updateFiles()
                        }, content: {
                            CompressFileView(isPresented: $showSubView[14], directory: directory, fileNames: multiSelectFiles, multipleFiles: (multiSelectFiles.count > 1))
                        })
                        .sheet(isPresented: $showSubView[34], onDismiss: {
                            withAnimation {
                                multiSelect = false
                            }
                            resetMultiSelectArrays()
                            updateFiles()
                        }, content: {
                            UncompressFileView(isPresented: $showSubView[34], filePath: newViewFilePath, fileName: newViewFileName)
                        })
                        .sheet(isPresented: $showSubView[22], content: {
                            HexView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[22])
                                .onAppear {
                                    isLoadingView = false
                                } //it can take a long time for my current hex editor implementation to load everything, so a loading circle is displayed. i typically avoid it for this reason.
                                //i will later improve it, I just don't like working with Data
                        })
                        .sheet(isPresented: $showSubView[23], content: {
                            DpkgView(debPath: $newViewFilePath, debName: $newViewFileName, isPresented: $showSubView[23], isRootless: $isRootless)
                        })
                        .sheet(isPresented: $showSubView[24], content: {
                            DpkgBuilderView(debInputDir: $newViewFilePath, debInputName: $newViewFileName, isPresented: $showSubView[24], isRootless: $isRootless)
                        }) //i created these in case anything with nitoTV breaks. having filza deb installer has saved my butt more times than I can count
                        .sheet(isPresented: $showSubView[25], content: {
                            WebServerView() //this will probably never work
                        })
                        .sheet(isPresented: $showSubView[28], content: {
                            CarView(filePath: $newViewFilePath, fileName: $newViewFileName)
                        })
                        .sheet(isPresented: $showSubView[30], content: {
                            FontView(filePath: $newViewFilePath, fileName: $newViewFileName) //i was bored i think
                        })
                        .sheet(isPresented: $showSubView[32], content: {
                            DMGMountView(filePath: newViewFilePath, fileName: newViewFileName, directory: directoryBinding, isPresented: $showSubView[32])
                        })
                        .sheet(isPresented: $showSubView[33], content: {
                            HTMLView(filePath: newViewFilePath, fileName: newViewFileName)
                        })
                        .sheet(isPresented: $showSubView[36], content: {
                            WebDocumentSwiftView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[36])
                        })
                        .sheet(isPresented: $showSubView[38], content: {
                            WebDocumentSwiftView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[38])
                        })
                }
                HStack {
                    // 칸 둘. 활성 칸만 테두리가 켜지고, 툴바의 모든 명령은 그쪽으로 간다.
                    // 칸 사이 좌우 이동은 tvOS 포커스 엔진이 알아서 한다 — 손대면 싸운다.
                    if dualPaneEnabled {
                        HStack(spacing: 12) {
                            FilePaneView(model: leftPane,
                                         isActive: activeSide == .left,
                                         onOpen: { index in openInPane(.left, index: index) },
                                         onGoUp: { activeSide = .left; leftPane.goUp() },
                                         onFocus: { activeSide = .left },
                                         onContextMenu: { index in openContextMenu(.left, index: index) })
                            FilePaneView(model: rightPane,
                                         isActive: activeSide == .right,
                                         onOpen: { index in openInPane(.right, index: index) },
                                         onGoUp: { activeSide = .right; rightPane.goUp() },
                                         onFocus: { activeSide = .right },
                                         onContextMenu: { index in openContextMenu(.right, index: index) })
                        }
                    } else {
                        FilePaneView(model: leftPane,
                                     isActive: true,
                                     onOpen: { index in openInPane(.left, index: index) },
                                     onGoUp: { activeSide = .left; leftPane.goUp() },
                                     onFocus: { activeSide = .left },
                                     onContextMenu: { index in openContextMenu(.left, index: index) })
                    }
                        .alert(isPresented: $showAlert) {
                            Alert(
                                title: Text(alertTitle),
                                message: Text(alertMsg),
                                dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "You're sky freaks! I love it! I love it!")))
                            )
                        }
                }
                .sheet(isPresented: $showSubView[35]) {
                    NotJailbrokenView()
                }
                .sheet(isPresented: $showSubView[1]) {
					ContextMenuButtonTV(stringKey: "OPEN", action: {
						defaultAction(index: newViewFileIndex, isDirectPath: false)
                        showSubView[1] = false
					}) //i got tired of having the same copy pasted button thing so I made it a wrapper struct with only the parameters I need. cut down on filesize a lot
					
					ContextMenuButtonTV(stringKey: "INFO", action: {
                        fileInfo = FileInfo.getFileInfo(filePath: masterFiles[newViewFileIndex].fullPath)
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            newViewFileName = masterFiles[newViewFileIndex].name
                            showSubView[3] = true
                        }
					})
					
					ContextMenuButtonTV(stringKey: "RENAME", action: {
                        newViewFilePath = directory
                        renameFileCurrentName = masterFiles[newViewFileIndex].name
                        renameFileNewName = masterFiles[newViewFileIndex].name
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            showSubView[7] = true
                        }
                    })
                    
                    ContextMenuButtonTV(stringKey: "OPENIN", action: {
                        newViewFilePath = directory
                        newViewArrayNames = [masterFiles[newViewFileIndex].name]
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            showSubView[2] = true
                        }
                    })
                    
                    if(directory == "/private/var/mobile/Media/.Trash/"){
                        ContextMenuButtonTV(stringKey: "DELETE", action: {
                            RootHelperActs.rm(masterFiles[newViewFileIndex].fullPath)
                            masterFiles.remove(at: newViewFileIndex)
                            showSubView[1] = false
                        })
                    } else if(directory == "/private/var/mobile/Media/" && masterFiles[newViewFileIndex].name == ".Trash/"){
                        ContextMenuButtonTV(stringKey: "TRASHYEET", action: {
                            _ = TrashManager.shared.emptyAll()
                            showSubView[1] = false
                        })
                    } else {
                        ContextMenuButtonTV(stringKey: "GOTOTRASH", action: {
                            if TrashManager.shared.moveToTrash(path: masterFiles[newViewFileIndex].fullPath) {
                                masterFiles.remove(at: newViewFileIndex)
                            }
                            showSubView[1] = false
                        })
                    }
                    if(deleteOverride) { //i dont think this can display but it's HERE ANYWAY
                        ContextMenuButtonTV(stringKey: "DELETE", action: {
                            RootHelperActs.rm(masterFiles[newViewFileIndex].fullPath)
                            masterFiles.remove(at: newViewFileIndex)
                            showSubView[1] = false
                        })
                    }
                    
                    ContextMenuButtonTV(stringKey: "FAVORITESADD", action: {
                        newViewFilePath = directory
						newViewFileName = masterFiles[newViewFileIndex].name
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            showSubView[17] = true
                        }
                        UserDefaults.favorites.synchronize()
                    })//I'm sticking to consistent setups that I've practiced
                    
                    ContextMenuButtonTV(stringKey: "MOVETO", action: {
                        newViewFilePath = directory
                        newViewArrayNames = [masterFiles[newViewFileIndex].name]
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            showSubView[8] = true
                        }
                    })
                    
                    
                    ContextMenuButtonTV(stringKey: "COPYTO", action: {
                        newViewFilePath = directory
                        newViewArrayNames = [masterFiles[newViewFileIndex].name]
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            showSubView[9] = true
                        }
                    })
                    
                    
                    ContextMenuButtonTV(stringKey: "DISMISS", action: {
                        showSubView[1] = false
                    })
                }
                .sheet(isPresented: $showSubView[2]) {
                    HStack {
                        VStack {
                            ContextMenuButtonTV(stringKey: "OPEN_DIRECTORY", action: {
                                directory = directory + masterFiles[newViewFileIndex].name
                                updateFiles()
                                showSubView[2] = false
                            })
                            
                            ContextMenuButtonTV(stringKey: "OPEN_AUDIO", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[10] = true
									callback = true
								}
							})
							
							
							ContextMenuButtonTV(stringKey: "OPEN_VIDEO", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[11] = true
								}
							})
							
							
							ContextMenuButtonTV(stringKey: "OPEN_IMAGE", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								isImageSVG[1] = true
								showSubView[31] = true
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[2] = false
								}
							})
							.onDisappear {
								if isImageSVG[1] {
									DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
										showSubView[12] = true
									}
								}
							}
							.alert(isPresented: $showSubView[31], content: {
								Alert(
									title: Text(""),
									message: Text(NSLocalizedString("INFO_DENIED", comment: "You're monsters!")),
                                    primaryButton: .default(Text(localizedString: "OPEN_IMAGERGBA"), action: {
										isImageSVG[0] = false
									}),
                                    secondaryButton: .default(Text(localizedString: "OPEN_IMAGESVG"), action: {
										isImageSVG[0] = true
									})
								)
							})
                            
                            ContextMenuButtonTV(stringKey: "OPEN_TEXT", action: {
                                newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
                                showSubView[2] = false
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    showSubView[4] = true
                                }
                            })
                            
                            ContextMenuButtonTV(stringKey: "OPEN_HEX", action: {
                                newViewFilePath = directory
                                newViewFileName = masterFiles[newViewFileIndex].name
                                showSubView[2] = false
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    showSubView[22] = true
                                }
                            })
                            
                            ContextMenuButtonTV(stringKey: "OPEN_DMG", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    if isJB {
                                        showSubView[32] = true
                                    } else {
                                        showSubView[35] = true
                                    }
								}
							})
                            
                            ContextMenuButtonTV(stringKey: "OPEN_COMP", action: {
                                newViewFilePath = directory
                                newViewArrayNames = [masterFiles[newViewFileIndex].name]
                                showSubView[2] = false
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    showSubView[14] = true
                                }
                            })
                            
                            ContextMenuButtonTV(stringKey: "OPEN_UNCOMP", action: {
                                newViewFilePath = directory
                                newViewFileName = masterFiles[newViewFileIndex].name
                                showSubView[2] = false
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    showSubView[34] = true
                                }
                            })
                        }
                    
						VStack {
                            ContextMenuButtonTV(stringKey: "FAVORITESADD", action: {
                                newViewFilePath = directory
                                newViewFileName = masterFiles[newViewFileIndex].name
                                showSubView[2] = false
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    showSubView[17] = true
                                }
                            })
                            
							ContextMenuButtonTV(stringKey: "OPEN_PLIST", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[13] = true
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_FONT", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[30] = true
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_DPKG", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    if isJB {
                                        showSubView[23] = true
                                    } else {
                                        showSubView[35] = true
                                    }
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_DPKGDEB", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    if isJB {
                                        showSubView[24] = true
                                    } else {
                                        showSubView[35] = true
                                    }
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_HTML", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[33] = true
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_CAR", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
									showSubView[34] = true
								}
							})
							
							ContextMenuButtonTV(stringKey: "OPEN_SPAWN", action: {
								newViewFilePath = directory
								newViewFileName = masterFiles[newViewFileIndex].name
								showSubView[2] = false
								DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                    if isJB {
                                        showSubView[15] = true
                                    } else {
                                        showSubView[35] = true
                                    }
								}
							})
							
							ContextMenuButtonTV(stringKey: "DISMISS", action: {
								showSubView[2] = false
							})
                        }
                    }
                }
                .sheet(isPresented: $E2) {
                    EThree(directory: directoryBinding, files: Binding<[String]>(get: { self.masterFiles.map { $0.name } }, set: { newNames in
                        for (index, newName) in newNames.enumerated() {
                            self.masterFiles[index].name = newName
                        }
                    }), multiSelectFiles: multiSelectFilesBinding, fileWasSelected: Binding<[Bool]>(get: { self.masterFiles.map { $0.isSelected } }, set: { selected in
                        for (index, selected) in selected.enumerated() {
                            self.masterFiles[index].isSelected = selected
                        }
                    }), showSubView: $showSubView)
                }
                .navigationBarHidden(true)
                .onAppear {
                    resetShowSubView()
                    updateFiles()
                    if(FileInfo.yandereDevFileType(file: directory) != 0) {
                        directPathTypeCheckNewViewFileVariableSetter()
                        multiSelect = false
                        defaultAction(index: 0, isDirectPath: true)
                        var components = directory.split(separator: "/")
                        components.removeLast()
                        directory = "/" + components.joined(separator: "/") + "/"
                    }
                }
                .sheet(isPresented: $showSubView[3]) { //file info
                    VStack {
                        Text(NSLocalizedString("SHOW_INFO", comment: "A perfect report card, all B's."))
                            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                view.scaledFont(name: "BotW Sheikah Regular", size: 60)
                            }
                            .font(.system(size: 60))
                        ForEach(fileInfo, id: \.self) { infoPiece in
                            Text(infoPiece)
                            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                            }
                        }
                        Button(action: {
                            showSubView[3] = false
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                showSubView[27] = true
                            }
                        }) {
                            Text(NSLocalizedString("PERMSEDIT", comment: "I dont know why I added the sheikah function it took two hours"))
                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                }
                        }
                        Button(action: {
                            showSubView[3] = false
                        }) {
                            Text(NSLocalizedString("DISMISS", comment: "Very proud."))
                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                }
                        }
                        .onAppear {
                            fileInfo = FileInfo.getFileInfo(filePath: directory + newViewFileName) //turns out this is pretty simple. it just asks FileManager for file attributes and formats them
                        }
                    }
                }
                .sheet(isPresented: $showSubView[27], onDismiss: {
                    showSubView[3] = true
                }, content: {
                    TextField(NSLocalizedString("PERMSEDIT", comment: "This should have been added a long time ago"), value: $filePerms, formatter: NumberFormatter(), onCommit: {
                        RootHelperActs.chmod(masterFiles[newViewFileIndex].fullPath, filePerms)
                        showSubView[27] = false
                    })
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
                    .keyboardType(.numberPad)
                    .textContentType(.oneTimeCode) //0-9 only!
                    .onAppear {
						do {
							filePerms = try fileManager.attributesOfItem(atPath: masterFiles[newViewFileIndex].fullPath)[.posixPermissions] as? Int ?? 000 //this used to be a 000 for some reason??????? i changed it to 640 like other files on the tvOS filesystem. truly amazing.
							//im unsmart and forgot what this does. if it can't read the file perms it sets them to 000 since it can't read the file... and 000 means no perms to read the file........
                        } catch {
							filePerms = 000
						}//it also used a try! for some reason, so that's changed as well.
                    }
                })
				/*
                welcome to the SheetStack. how anything and everything is presented: a bool in showSubView, and a sheet.
				there's a lot. most of them are pretty easy to comprehend, if there's anything out of the ordinary I'll explain it
                
                 they were split apart due to a silly tvOS 14.1-14.4(?) bug. you can find them attached to:
                 1. the button they're called by
                 2. freeSpace by the top of the file
                */
                .sheet(isPresented: $showSubView[0], content: {
                    VStack {
                        ContextMenuButtonTV(stringKey: "CREATE_FILE") {
                            showSubView[0] = false
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                showSubView[5] = true
                            }
                        }
                        
                        ContextMenuButtonTV(stringKey: "CREATE_DIR") {
                            showSubView[0] = false
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                showSubView[6] = true
                            }
                        }
                        
                        ContextMenuButtonTV(stringKey: "CREATE_SYM") {
                            showSubView[0] = false
                            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                                showSubView[20] = true
                            }
                        }
                    }
                })
                .alert(isPresented: $showSubView[26]) {
                    Alert(
                        title: Text(NSLocalizedString("SHOW_NOTFOUND", comment: "")),
                        message: Text(NSLocalizedString("SHOW_OLDDEST", comment: "") + nonexistentFile),
                        dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "")))
                    )
                }
                .accentColor(.accentColor)
            }
        }
        .onPlayPauseCommand {
			callback = false
			showSubView[10] = true
		} //this lets you access the music player from anywhere. it does create a new instance of the view, but it's accessing the same AV player always, so it never loses its place. it doesn't break when you open it without a music file loaded, too - that took far too long
        .onExitCommand {
                if(directory == "/"){
                    ObjCFunctions.suspendNow() 
                } else {
                    goBack()
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: .fileGuardBlocked)) { note in
                if let reason = note.object as? String {
                    alertTitle = "작업 차단됨"
                    alertMsg = reason
                    showAlert = true
                }
            }
            
            // 배너
            if showSafetyBanner {
                VStack {
                    PathSafetyBanner(risk: safetyBannerRisk, message: safetyBannerMsg)
                        .transition(.move(edge: .top).combined(with: .opacity))
                    Spacer()
                }
                .zIndex(10)
            }
            
            // 일괄 삭제 미리보기
            if showBatchDeletePreview {
                BatchDeletePreview(
                    files: multiSelectFiles,
                    cautionCount: cautionCountForBatch,
                    onConfirm: {
                        showBatchDeletePreview = false
                        executeBatchDelete()
                    },
                    onCancel: {
                        showBatchDeletePreview = false
                    }
                )
                .zIndex(20)
            }
        } // End ZStack
    }
    
    @ViewBuilder
    var serverButton: some View {
        if #unavailable(tvOS 16.0) { //i... don't know why this is here? the webserver stuff can't be loaded by the UI, so i'm ignoring it for now. i'm curious what this was done for
        //I REMEMBER WHY I WAS USING A DIFFERENT SF SYMBOL THAT DOESNT EXIST ON >=16.0
            Button(action: {
                showSubView[29] = false
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                    showSubView[25] = true
                }
            }) {
                Text(NSLocalizedString("SERVERHEAD", comment: ""))
                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                    }
                    .frame(width: buttonWidth, height: buttonHeight)
            }
            .padding(paddingInt)
            .opacity(opacityInt)
        }
    
        Button(action: { //mount points
            showSubView[21] = true
        }) {
            Text(NSLocalizedString("MOUNTPOINTS", comment: ""))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
                .frame(width: buttonWidth, height: buttonHeight)
        }
        .padding(paddingInt)
        .opacity(opacityInt)
        
        Button(action: {
            //start webserver headless
        }) {
            Text(NSLocalizedString("SERVERHEADLESS", comment: ""))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
                .frame(width: buttonWidth, height: buttonHeight)
        }
        .padding(paddingInt)
        .opacity(opacityInt)
    }
    
    @ViewBuilder
    var freeSpace: some View { //this is hardcoded for now, returning mount points wasnt working
		//future me wants you to know it works now but im not changing it.
        let (doubleValue, stringValue) = freeSpace(path: "/")
        Text(NSLocalizedString("FREE_SPACE", comment: "E") + String(format: "%.2f", doubleValue) + " " + stringValue)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 32)
                }
    }
    
    // The Menu button walks up one folder at a time, so leaving from
    // somewhere deep means pressing it over and over. Suspending first
    // gives the same animation as the Home button before the process
    // goes; if that private selector is ever gone, exit still works.
    func quitArgo() {
        let app = UIApplication.shared
        let suspend = Selector(("suspend"))
        guard app.responds(to: suspend) else {
            exit(0)
        }
        // perform() hands back an Unmanaged<AnyObject>; discard it
        // explicitly or the if-branch looks like a value to infer from.
        _ = app.perform(suspend)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) {
            exit(0)
        }
    }

    @ViewBuilder
    var debugMenu: some View {
        VStack {
			if E { //i need e
				Button(action: {
					E2 = true
				}) {
					Image(systemName: "ant")
						.frame(width: 50, height: 50)
				}
			}
		}
		.frame(maxWidth: .infinity, alignment: .leading)
    }
    
    func defaultAction(index: Int, isDirectPath: Bool) { //this function was created to allow tvOS 13 support. it is probably the most important function in this entire file manager. without it, nothing aside from directory listing works
        var fileToCheck: [String] = masterFiles.map { $0.name }
        if(isDirectPath) {
            fileToCheck = [""]
        }
    
        if (multiSelect) {
            if(masterFiles[index].isSelected){
                let searchedIndex = multiSelectFiles.firstIndex(of: masterFiles[index].name)
                multiSelectFiles.remove(at: searchedIndex!)
                masterFiles[index].isSelected = false
            } else {
                masterFiles[index].isSelected = true
                multiSelectFiles.append(masterFiles[index].name)
            }
        } else {
            multiSelect = false
            newViewFilePath = directory
            newViewFileName = fileToCheck[index]
            let fileType = masterFiles[index].fileType
            switch fileType {
            case 0:
                do {
                    try fileManager.contentsOfDirectory(atPath: directory + fileToCheck[index])
                } catch {
					let err = error.localizedDescription
					if err.substring(fromIndex: err.count - 33) == "don’t have permission to view it." { //substrings are horrible parts of swift
						//switched to using the String extension. so much nicer.
                        alertTitle = NSLocalizedString("SHOW_DENIED", comment: "Here's the graduate.")
                        alertMsg = NSLocalizedString("INFO_DENIED", comment: "You're monsters!")
                        showAlert = true
                    }
                }
                if(!showAlert){
                    if(isDirectPath) {
                        updateFiles()
                    } else {
                        directory = directory + fileToCheck[index]
                        updateFiles()
                    }
                }
            case 1:
                showSubView[10] = true
                callback = true
            case 2:
                showSubView[11] = true
            case 3:
                showSubView[12] = true
            case 4:
                showSubView[4] = true
            case 5:
                showSubView[13] = true
            case 6:
                showSubView[34] = true
            case 7:
                if isJB {
                    showSubView[15] = true
                } else {
                    showSubView[35] = true
                }
            case 8:
                do {
                    let dest = try FileInfo.readSymlinkDestination(path: directory + fileToCheck[index])
                    if directory != dest { //1.74/2gb memory used if you have a symlink that resolves to itself. spartan continually tries to resolve to it and it infinite loops until jetsam kills it due to too much memory usage
                        if FileInfo.isDirectory(filePath: dest) {
                            directory = dest
                            updateFiles()
                        } else {
                            masterFiles.append(SpartanFile(name: URL(fileURLWithPath: dest).lastPathComponent, fullPath: dest, isSelected: false, fileType: FileInfo.yandereDevFileType(file: dest), isLoadingFile: false))
                            defaultAction(index: masterFiles.count-1, isDirectPath: false)
                        }
                    }
                } catch {
                    alertTitle = LocalizedString("BADSYMLINK")
                    alertMsg = LocalizedString("BADSYMLINK_MSG")
                    showAlert = true
                    print("Please create a valid symlink. (Error ID 167)")
                }
            case 9:
                if isJB {
                    showSubView[23] = true
                } else {
                    showSubView[35] = true
                }
            case 10:
                showSubView[28] = true
            case 11:
                showSubView[30] = true
			case 12:
				isImageSVG[0] = true
				showSubView[12] = true
			case 13:
                if isJB {
                    showSubView[32] = true
                } else {
                    showSubView[35] = true
                }
			case 14:
				showSubView[33] = true
			case 15:
				showSubView[36] = true
			case 16:
				showSubView[38] = true
            default:
                masterFiles[index].isLoadingFile = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                    showSubView[22] = true
                }
            }
        }
    }

    
    
    
    // 아래 다섯 개는 칸 모델로 옮겼다. 호출부가 많아 이름만 남겨 넘긴다.
    func updateFiles() {
        pane.updateFiles()
    }

    /// 한 칸 올라가기. 리스트 맨 위의 ".." 과 메뉴 버튼이 쓴다.
    func goBack() {
        pane.goUp()
    }

    func resetMultiSelectArrays() {
        pane.resetMultiSelectArrays()
    }

    func iterateOverFileWasSelected(boolToIterate: Bool) {
        pane.setAllSelected(boolToIterate)
    }

    // MARK: - 두 칸 라우팅

    /// 칸에서 항목을 눌렀을 때. 활성 칸을 먼저 바꿔야 defaultAction이 올바른 칸을 본다.
    /// directory·masterFiles가 활성 칸을 가리키는 계산 속성이기 때문이다.
    func openInPane(_ side: PaneSide, index: Int) {
        activeSide = side
        let model = side == .left ? leftPane : rightPane
        guard model.masterFiles.indices.contains(index) else { return }

        // 여러 개 고르는 중이면 열지 않고 선택만 토글한다.
        if model.multiSelect {
            model.toggleSelection(at: index)
            return
        }
        defaultAction(index: index, isDirectPath: false)
    }

    /// 길게 눌러 뜨는 메뉴. 기존 동작 시트(showSubView[1])를 그대로 쓴다.
    func openContextMenu(_ side: PaneSide, index: Int) {
        activeSide = side
        let model = side == .left ? leftPane : rightPane
        guard model.masterFiles.indices.contains(index) else { return }
        newViewFilePath = model.masterFiles[index].fullPath
        newViewFileName = model.masterFiles[index].name
        showSubView[1] = true
    }

    /// 활성 칸의 선택을 반대 칸으로 복사한다. 두 칸의 존재 이유다.
    /// 쓰기는 반드시 FileGuard를 지난다 — 관문을 우회하면 안전장치가 무의미해진다.
    func copySelectionToOtherPane(move: Bool) {
        let sources = pane.selectedPaths.isEmpty
            ? (newViewFilePath.isEmpty ? [] : [newViewFilePath])
            : pane.selectedPaths
        guard !sources.isEmpty else {
            alertTitle = "복사할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }

        let destination = otherPane.directory
        switch FileGuard.authorize(action: move ? "move" : "copy",
                                   paths: sources.map { destination + ($0 as NSString).lastPathComponent }) {
        case .failure(let err):
            alertTitle = move ? "이동이 차단되었습니다" : "복사가 차단되었습니다"
            alertMsg = err.localizedDescription
            showAlert = true
            return
        case .success:
            break
        }

        for source in sources {
            let name = (source as NSString).lastPathComponent
            let target = destination + name
            if move {
                RootHelperActs.mv(source, target)
            } else {
                RootHelperActs.cp(source, target)
            }
        }

        otherPane.updateFiles()
        if move { pane.updateFiles() }
        pane.multiSelect = false
    }

    /// 활성 칸을 반대 칸과 같은 폴더로 보낸다. 양쪽을 맞춰 볼 때 쓴다.
    func syncOtherPaneToActive() {
        otherPane.navigate(to: pane.directory)
    }

    func freeSpace(path: String) -> (double: Double, string: String) {
        do {
            let systemAttributes = try fileManager.attributesOfFileSystem(forPath: path)
            let freeSpace = systemAttributes[.systemFreeSize] as? NSNumber
            if let freeSpace = freeSpace {
                return convertBytes(bytes: freeSpace.doubleValue)
            } else {
                return (0, "?")
            }
        } catch {
            print("Error: \(error)")
            return (0, "?")
        }
    }
    func convertBytes(bytes: Double) -> (Double, String) {
        let units = ["bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB", "BB"] //i added support for anything > gigabytes because i could. deal with it. maybe someday i'll add support for mounting external storage, so it could come in handy. you never know
        //or maybe dosdude1 will just chuck a bunch of storage in an atv
        var remainingBytes = Double(bytes)
        var i = 0
        
        while remainingBytes >= 1024 && i < units.count - 1 {
            remainingBytes /= 1024
            i += 1
        }
        
        return (remainingBytes, units[i])
    }

    
    func directPathTypeCheckNewViewFileVariableSetter() { //this function name is very very silly and im leaving it. I think it's used once and it doesn't even work properly. but it uses a feature that literally no one should be using
        if(FileInfo.yandereDevFileType(file: directory) != 0){
            newViewFilePath = String(directory.prefix(through: directory.lastIndex(of: "/")!))
            let inProgressFileName = directory.split(separator: "/")
            newViewFileName = String(inProgressFileName.last ?? "")
        }
    }
    func resetShowSubView() {
        for i in 0..<showSubView.count {
            showSubView[i] = false
        }
    }
    
    func defineBundleID(_ plistDict: NSDictionary) -> String {
        if (directory == "/private/var/mobile/Containers/Shared/AppGroup/") {
            return trimGroupBundleID(plistDict["MCMMetadataIdentifier"] as? String ?? "group.com.apple.mail") ?? "com.whitetailani.Spartan"
        } else {
            return plistDict["MCMMetadataIdentifier"] as? String ?? "com.whitetailani.Spartan"
        }
    }
    
    func trimGroupBundleID(_ string: String) -> String? {
        let components = string.components(separatedBy: ".")
        guard components.count >= 3 else {
            return nil
        }
        return components.suffix(3).joined(separator: ".")
    }
    
    // 일괄 삭제 미리보기에서 확인 시 호출되는 함수
    func executeBatchDelete() {
        if(directory == "/private/var/mobile/Media/.Trash/"){
            for file in multiSelectFiles {
                TrashManager.shared.deletePathDirectly(directory + file)
            }
            updateFiles()
        } else {
            for file in multiSelectFiles {
                TrashManager.shared.moveToTrash(path: directory + file)
            }
            updateFiles()
        }
        multiSelect = false
        resetMultiSelectArrays()
    }
}

struct SpartanFile: Hashable, Encodable, Decodable {
    var name: String
    var fullPath: String
    var isSelected: Bool
    var fileType: Double
    var isLoadingFile: Bool
} // i am so glad I switched to this instead of having like five different arrays
