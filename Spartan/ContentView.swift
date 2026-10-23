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
    @StateObject private var selection = PaneSelection()

    /// 이름은 그대로 두고 저장소만 클래스로 옮겼다. 쓰고 바로 읽어도 새 값이 보인다.
    var activeSide: PaneSide {
        get { selection.active }
        nonmutating set { selection.active = newValue }
    }
    @StateObject var transfer = PaneTransfer()
    @StateObject var clipboard = ArgoClipboard()
    @StateObject var airdrop = AirDropState()

    /// 알림은 한 번만 뜨고 닫히면 지워진다.
    var airdropFailureBinding: Binding<Bool> {
        Binding(get: { airdrop.lastFailure != nil },
                set: { if !$0 { airdrop.lastFailure = nil } })
    }
    /// 두 칸을 함께 보일지. 끄면 한 칸만 나와 기존 동작과 같다.
    @AppStorage("dualPaneEnabled") var dualPaneEnabled = false
    /// 툴바에 나머지 아이콘을 펼칠지. 기본은 접어 둔다 —
    /// 25개가 한 줄에 늘어서면 무엇이 무엇인지 알 수 없다.
    @State private var toolbarExpanded = false

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
    var multiSelectBinding: Binding<Bool> {
        Binding(get: { pane.multiSelect }, set: { pane.multiSelect = $0 })
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
    
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 41)
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
    //keyboardHelpShow = 39
    //textReaderShow = 40
    
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
    /// 지금 지우려는 것들의 전체 경로.
    ///
    /// 전에는 미리보기가 multiSelectFiles(고른 항목 이름)를 보고, 실제 삭제도
    /// 같은 배열을 돌았다. 길게 눌러 한 개를 지우면 그 배열이 비어 있어
    /// 미리보기에는 파일이 보이는데 눌러도 아무 일이 없었다.
    @State private var pendingDeletePaths: [String] = []
    
    var body: some View {
        ZStack {
            // 화면 바탕. 순검정이 아닌 짙은 차콜 — 순검정은 OLED에서
            // 패널과 바탕의 경계가 사라집니다.
            Theme.background
                .edgesIgnoringSafeArea(.all)

            VStack {
                VStack {
                    HStack { //input directory + refresh
                    TextField(NSLocalizedString("INPUT_DIRECTORY", comment: "According to all known laws of aviation"), text: directoryBinding, onEditingChanged: { editing in
                        // 글자를 받는 동안 단축키를 막는다. 안 막으면 파일명에
                        // 'r' 을 넣는 순간 새로 읽기가 돈다.
                        if editing { ArgoTextEntry.begin() }
                        else { ArgoTextEntry.end() }
                    }, onCommit: {
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
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                    .sheet(isPresented: $showSubView[5], content: {
                        CreateFileView(filePath: directoryBinding, isPresented: $showSubView[5])
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                    .sheet(isPresented: $showSubView[20], content: {
                        CreateSymlinkView(symlinkPath: directoryBinding, isPresented: $showSubView[20])
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                }
                // 툴바가 화면보다 넓어지면 부모 VStack 의 폭이 같이 커져
                // 화면 전체가 옆으로 밀려 잘린다. 가로 스크롤로 폭을 가둔다 —
                // 포커스가 화면 밖 버튼으로 가면 tvOS 가 알아서 스크롤한다.
                ScrollView(.horizontal, showsIndicators: false) {
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

                    Button(action: { //홈 — 미디어 폴더로
                        pane.navigate(to: "/var/mobile/Media/")
                    }) {
                        Image(systemName: "house")
                            .frame(width: 50, height: 50)
                    }

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

                    // AirDrop. 쓸 수 없는 기기에서는 아예 보이지 않는다 —
                    // 눌러도 안 되는 버튼을 두는 것보다 없는 편이 낫다.
                    if airdrop.isAvailable {
                        Button(action: {
                            airdrop.toggle()
                        }) {
                            HStack(spacing: 6) {
                                Image(systemName: "dot.radiowaves.left.and.right")
                                if airdrop.isReceiving {
                                    // 남은 시간을 같이 보여 준다. 받는 중이면
                                    // 저절로 늘어나므로 멈춘 것처럼 보이지 않는다.
                                    Text(airdrop.countdownLabel)
                                        .font(.caption)
                                }
                            }
                            .frame(height: 50)
                            .padding(.horizontal, airdrop.isReceiving ? 12 : 0)
                            .frame(minWidth: 50)
                            .foregroundColor(airdrop.isReceiving ? Theme.airdropOn : nil)
                        }
                        .alert(isPresented: airdropFailureBinding) {
                            Alert(title: Text("AirDrop"),
                                  message: Text(airdrop.lastFailure ?? ""),
                                  dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: ""))))
                        }
                    }

                    Button(action: { //settings
                        showSubView[18] = true
                    }) {
                        Image(systemName: "gear")
                            .frame(width:50, height:50)
                    }
                    .sheet(isPresented: $showSubView[18], content: {
                        SettingsView()
                    })

                    // 붙여넣을 것이 있을 때만 나타난다. 평소에 자리를 차지하지 않고,
                    // 나타나면 그 자체가 '뭔가 담겨 있다'는 표시가 된다.
                    if !clipboard.isEmpty {
                        Button(action: {
                            clipboardPaste()
                        }) {
                            HStack(spacing: 6) {
                                Image(systemName: clipboard.mode == .move
                                      ? "scissors" : "doc.on.clipboard")
                                Text("\(clipboard.count)")
                            }
                            .frame(height: 50)
                            .padding(.horizontal, 10)
                        }

                        Button(action: {
                            clipboard.clear()
                        }) {
                            Image(systemName: "xmark.circle")
                                .frame(width: 50, height: 50)
                        }
                    }

                    Button(action: { //더보기 — 나머지 아이콘 펼치기
                        withAnimation { toolbarExpanded.toggle() }
                    }) {
                        Image(systemName: toolbarExpanded
                              ? "chevron.left.circle" : "ellipsis.circle")
                            .frame(width: 50, height: 50)
                    }

                    Group {
                    Button(action: { //숨긴 항목
                        pane.showHidden.toggle()
                        UserDefaults.settings.set(pane.showHidden, forKey: "showHiddenFiles")
                        pane.applySortAndFilter()
                    }) {
                        Image(systemName: pane.showHidden ? "eye" : "eye.slash")
                            .frame(width: 50, height: 50)
                    }

                    Button(action: { //정렬 기준 돌리기. 같은 기준을 다시 누르면 오르내림이 뒤집힌다
                        let keys = SortKey.allCases
                        if let i = keys.firstIndex(of: pane.sort) {
                            if pane.ascending {
                                pane.ascending = false
                                pane.applySortAndFilter()
                            } else {
                                setSort(keys[(i + 1) % keys.count])
                            }
                        }
                    }) {
                        HStack(spacing: 4) {
                            Image(systemName: pane.sort.symbol)
                            Image(systemName: pane.ascending ? "chevron.up" : "chevron.down")
                        }
                        .frame(width: 70, height: 50)
                    }


                    Button(action: { //단축키 도움말. tvOS에는 메뉴 막대가 없어 이 화면이 유일한 안내다
                        showSubView[39] = true
                    }) {
                        Image(systemName: "keyboard")
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
                                    .onAppear { ArgoTextEntry.begin() }
                                    .onDisappear { ArgoTextEntry.end() }
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
                                // 삭제는 한 길로만 간다. 전에는 여기서 바로
                                // 미리보기를 띄우면서 지울 목록을 채우지 않아,
                                // '지울 것 0개' 가 뜨고 눌러도 아무 일이 없었다.
                                requestDeleteSelection()
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
                                .onAppear { ArgoTextEntry.begin() }
                                .onDisappear { ArgoTextEntry.end() }
                        })
                        .sheet(isPresented: $showSubView[8], content: {
                            MoveFileView(fileNames: $newViewArrayNames, filePath: $newViewFilePath, multiSelect: multiSelectBinding, isPresented: $showSubView[8])
                        })
                        .sheet(isPresented: $showSubView[9], content: {
                            CopyFileView(fileNames: $newViewArrayNames, filePath: $newViewFilePath, multiSelect: multiSelectBinding, isPresented: $showSubView[9])
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
                            // 네이티브 CGPDFDocument 뷰어. 쪽이 있고 맞춤이 된다.
                            ArgoPDFReaderView(filePath: newViewFilePath,
                                              fileName: newViewFileName,
                                              isPresented: $showSubView[36])
                                // 고유 크기가 없는 Representable 이다. 이걸 안 주면
                                // SwiftUI 가 0에 가까운 크기를 주고 시트 가운데에 앉힌다 —
                                // 쪽이 아래쪽에 손톱만 하게 나오던 원인이다.
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                                .edgesIgnoringSafeArea(.all)
                        })
                        .sheet(isPresented: $showSubView[40], content: {
                            // 읽기 전용. 편집은 여기서 기존 TextView 로 넘어간다.
                            ArgoTextReaderView(filePath: newViewFilePath,
                                               fileName: newViewFileName,
                                               isPresented: $showSubView[40],
                                               onEdit: {
                                                   DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                                                       showSubView[4] = true
                                                   }
                                               })
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                                .edgesIgnoringSafeArea(.all)
                        })
                        .sheet(isPresented: $showSubView[38], content: {
                            WebDocumentSwiftView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[38])
                        })
                    }
                    // 접혀 있으면 폭 0 · 투명 · 포커스 불가. 계층에서 빼지 않는 이유는
                    // 이 버튼들에 시트가 붙어 있어서다 — if 로 들어내면 단축키로 띄우는
                    // 시트까지 같이 사라진다.
                    .frame(maxWidth: toolbarExpanded ? nil : 0)
                    .opacity(toolbarExpanded ? 1 : 0)
                    .disabled(!toolbarExpanded)
                }
                .padding(.horizontal, 8)
                }
                // 툴바를 하나의 포커스 구역으로 묶는다. 칸에서 좌우 끝으로 가면
                // 더 갈 곳이 없어 포커스가 이 구역으로 올라온다.
                .focusSection()
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
                                         menu: rowMenu(for: .left))
                            FilePaneView(model: rightPane,
                                         isActive: activeSide == .right,
                                         onOpen: { index in openInPane(.right, index: index) },
                                         onGoUp: { activeSide = .right; rightPane.goUp() },
                                         onFocus: { activeSide = .right },
                                         menu: rowMenu(for: .right))
                        }
                    } else {
                        FilePaneView(model: leftPane,
                                     // 한 칸일 때는 강조 테두리가 의미 없다 —
                                     // 어느 칸이 명령을 받는지 보여줄 상대가 없다.
                                     isActive: false,
                                     onOpen: { index in openInPane(.left, index: index) },
                                     onGoUp: { activeSide = .left; leftPane.goUp() },
                                     onFocus: { activeSide = .left },
                                     menu: rowMenu(for: .left))
                    }
                }
                .alert(isPresented: $showAlert) {
                    Alert(
                        title: Text(alertTitle),
                        message: Text(alertMsg),
                        dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "You're sky freaks! I love it! I love it!")))
                    )
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
                        // 0.5초 지연 전에 고정한다. masterFiles 는 활성 칸을 가리키는
                        // 계산 속성이라, 지연 중에 칸이 넘어가면 반대 칸의 파일을 읽는다.
                        let selectedFile = masterFiles[newViewFileIndex]
                        fileInfo = FileInfo.getFileInfo(filePath: selectedFile.fullPath)
                        showSubView[1] = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                            newViewFileName = selectedFile.name
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
            .onReceive(NotificationCenter.default.publisher(for: ArgoCommand.notification)) { note in
                if let raw = note.object as? String {
                    handleKeyCommand(raw)
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: ArgoTypeAhead.notification)) { note in
                if let prefix = note.object as? String {
                    handleTypeAhead(prefix)
                }
            }
            .sheet(isPresented: $showSubView[39], content: {
                KeyboardHelpView(isPresented: $showSubView[39])
            })
            
            // 진행률. 큰 파일을 복사하는 동안 멈춘 것처럼 보이면 안 된다.
            if transfer.isRunning {
                VStack(spacing: 16) {
                    Text(transfer.currentName)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    ProgressView(value: transfer.progress)
                        .frame(width: 600)
                    Text("\(transfer.completed) / \(transfer.total)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Button(action: { transfer.cancel() }) {
                        Text("취소")
                    }
                }
                .padding(40)
                .background(Color.black.opacity(0.85))
                .cornerRadius(16)
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
                    files: pendingDeletePaths.map { ($0 as NSString).lastPathComponent },
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
    
    // MARK: - 뷰어 띄우기

    /// 뷰어는 SwiftUI 시트가 아니라 UIKit 으로 직접 띄운다.
    ///
    /// 시트 안의 UIViewControllerRepresentable 은 포커스를 받지 못한다.
    /// 진단 로그에서 didUpdateFocus 가 한 번도 오지 않았고 pressesBegan 도
    /// 없었다 — 포커스가 뷰어 안으로 들어온 적이 아예 없다는 뜻이다.
    /// 포커스된 항목이 없으면 방향키와 선택 버튼은 포커스 엔진이 삼켜서
    /// 제스처로도 오지 않는다. menu 키만 살아 있던 것이 그 증거다.
    ///
    /// 직접 present 하면 그 뷰컨트롤러가 자기 포커스 환경의 주인이 되고,
    /// preferredFocusEnvironments 가 제대로 불린다.
    private func presentReader(_ vc: UIViewController) {
        guard let top = AirDropBridge.topViewController() else {
            ArgoDiag.log("present", "최상위 뷰컨트롤러를 못 찾았다")
            return
        }
        vc.modalPresentationStyle = .fullScreen
        top.present(vc, animated: true)
    }

    private func readerPath() -> String {
        newViewFilePath.hasSuffix("/") ? newViewFilePath + newViewFileName : newViewFilePath
    }

    private func presentPDFReader() {
        let vc = ArgoPDFReaderController()
        vc.documentPath = readerPath()
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        presentReader(vc)
    }

    private func presentTextReader() {
        let vc = ArgoTextReaderController()
        vc.filePath = readerPath()
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        vc.onEdit = { [weak vc] in
            vc?.dismiss(animated: true) {
                showSubView[4] = true
            }
        }
        presentReader(vc)
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
                // 이 폴더를 읽을 수 있었는지는 '지금 이 호출'의 결과로만 판단한다.
                //
                // 전에는 공유 깃발 showAlert 를 봤다. 그 깃발은 원래 권한 오류
                // 전용이었는데, FileGuard 차단 알림을 같은 깃발에 연결하면서
                // 폴더를 읽을 때마다 도는 캐시 쓰기가 주의 폴더에서 막히면
                // 깃발이 서고, 그 뒤로 모든 폴더 진입이 조용히 무시됐다.
                var denied = false
                do {
                    try fileManager.contentsOfDirectory(atPath: directory + fileToCheck[index])
                } catch {
					let err = error.localizedDescription
					if err.substring(fromIndex: err.count - 33) == "don’t have permission to view it." { //substrings are horrible parts of swift
						//switched to using the String extension. so much nicer.
                        alertTitle = NSLocalizedString("SHOW_DENIED", comment: "Here's the graduate.")
                        alertMsg = NSLocalizedString("INFO_DENIED", comment: "You're monsters!")
                        showAlert = true
                        denied = true
                    }
                }
                if(!denied){
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
                // 편집기가 아니라 읽기 뷰어로 연다. 줄이 접히고 글자 크기를
                // 바꿀 수 있다 — 로그는 읽는 것이 목적이지 고치는 것이 아니다.
                presentTextReader()
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
				presentPDFReader()
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

    // MARK: - 키보드 명령

    /// 리스폰더 체인에서 올라온 단축키를 처리한다.
    /// 전부 리모컨으로도 되는 일이어야 한다 — 단축키 전용 기능은 만들지 않는다.
    func handleKeyCommand(_ raw: String) {
        guard let command = ArgoCommand(rawValue: raw) else { return }
        switch command {
        case .refresh:           pane.updateFiles()
        case .goUp:              pane.goUp()
        case .goBack:            pane.goBack()
        case .goForward:         pane.goForward()
        case .goHome:            pane.navigate(to: "/var/mobile/Media/")
        case .goRoot:            pane.navigate(to: "/")

        case .toggleDualPane:
            withAnimation { dualPaneEnabled.toggle() }
            if dualPaneEnabled && rightPane.masterFiles.isEmpty {
                rightPane.updateFiles()
            }
        case .switchPane:
            // 시트가 떠 있으면 칸을 넘기지 않는다. 모달 안에서는 칸 전환이
            // 의미가 없고, 시트가 읽는 값이 넘어가는 순간 어긋난다.
            guard dualPaneEnabled, !showSubView.contains(true) else { return }
            activeSide = activeSide == .left ? .right : .left
        case .clipboardCopy:     clipboardTake(mode: .copy)
        case .clipboardCut:      clipboardTake(mode: .move)
        case .clipboardPaste:    clipboardPaste()
        case .copyToOtherPane:   copySelectionToOtherPane(move: false)
        case .moveToOtherPane:   copySelectionToOtherPane(move: true)
        case .syncPanes:         syncOtherPaneToActive()

        case .toggleMultiSelect:
            withAnimation { pane.multiSelect.toggle() }
            if !pane.multiSelect { pane.resetMultiSelectArrays() }
        case .selectAll:
            pane.multiSelect = true
            pane.setAllSelected(!pane.allWereSelected)

        // 삭제는 전용 시트가 없다. 선택이 있으면 미리보기를 거쳐 휴지통으로 보낸다.
        case .deleteSelection:   requestDeleteSelection()
        case .rename:            showSubView[7] = true   // renameFileShow
        case .fileInfo:          showSubView[3] = true   // fileInfoShow
        case .newFolder:         showSubView[6] = true   // createDirectoryShow
        case .newFile:           showSubView[5] = true   // createFileShow
        case .compress:          showSubView[14] = true  // compFileShow
        case .extractArchive:    showSubView[34] = true  // uncompFileShow
        case .search:            showSubView[19] = true  // searchShow
        case .showHelp:          showSubView[39] = true  // keyboardHelpShow

        case .toggleHidden:
            pane.showHidden.toggle()
            UserDefaults.settings.set(pane.showHidden, forKey: "showHiddenFiles")
            pane.applySortAndFilter()
        case .sortByName:        setSort(.name)
        case .sortBySize:        setSort(.size)
        case .sortByDate:        setSort(.date)
        case .sortByKind:        setSort(.kind)
        case .reverseSort:
            pane.ascending.toggle()
            pane.applySortAndFilter()
        case .undo:
            undoLastAction()

        case .focusPathField:
            // 경로 칸은 TextField라 포커스를 코드로 줄 수 없다.
            // 대신 경로를 알려주는 찾기 시트를 연다.
            showSubView[19] = true
        }
    }

    /// ⌫ 로 삭제. 기존 일괄 삭제 미리보기를 거친다 — 되돌릴 수 없는 일이라 확인을 뺄 수 없다.
    func requestDeleteSelection() {
        let targets = pane.selectedPaths.isEmpty
            ? (newViewFilePath.isEmpty ? [] : [newViewFilePath])
            : pane.selectedPaths
        guard !targets.isEmpty else {
            alertTitle = "삭제할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }
        // 관문을 먼저 지난다. 막히면 왜 막혔는지 말해 준다 —
        // 조용히 아무 일도 안 일어나는 것이 가장 나쁘다.
        if case .failure(let err) = FileGuard.authorize(action: "batch_delete", paths: targets) {
            alertTitle = "삭제가 차단되었습니다"
            alertMsg = err.localizedDescription
            showAlert = true
            return
        }
        pendingDeletePaths = targets
        cautionCountForBatch = targets.filter { PathSafety.risk(of: $0) != .safe }.count
        showBatchDeletePreview = true
    }

    /// 같은 기준을 다시 누르면 오르내림이 뒤집힌다. 파인더와 같다.
    func setSort(_ key: SortKey) {
        if pane.sort == key {
            pane.ascending.toggle()
        } else {
            pane.sort = key
            pane.ascending = true
        }
        pane.applySortAndFilter()
    }

    /// 글자를 치면 그 글자로 시작하는 첫 항목을 고른다.
    func handleTypeAhead(_ prefix: String) {
        guard let index = pane.indexOfFirstMatch(prefix: prefix) else { return }
        newViewFilePath = pane.masterFiles[index].fullPath
        newViewFileName = pane.masterFiles[index].name
    }

    /// 길게 눌렀을 때의 동작 묶음. 누른 칸을 먼저 활성으로 바꾸고
    /// 그 칸의 항목을 대상으로 삼는다.
    func rowMenu(for side: PaneSide) -> PaneRowMenu {
        let model = side == .left ? leftPane : rightPane

        func focusItem(_ index: Int) -> String? {
            activeSide = side
            guard model.masterFiles.indices.contains(index) else { return nil }
            let file = model.masterFiles[index]
            newViewFilePath = file.fullPath
            newViewFileName = file.name
            newViewFileIndex = index
            return file.fullPath
        }

        return PaneRowMenu(
            copy: { index in
                guard let path = focusItem(index) else { return }
                clipboard.take([path], mode: .copy)
            },
            cut: { index in
                guard let path = focusItem(index) else { return }
                clipboard.take([path], mode: .move)
            },
            paste: {
                activeSide = side
                clipboardPaste()
            },
            canPaste: { !clipboard.isEmpty },
            pasteLabel: { "여기에 붙여넣기 — " + clipboard.summary },
            hasOtherPane: { dualPaneEnabled },
            sendToOtherPane: { index, move in
                guard focusItem(index) != nil else { return }
                copySelectionToOtherPane(move: move)
            },
            rename: { index in
                guard focusItem(index) != nil else { return }
                renameFileCurrentName = model.masterFiles[index].name
                renameFileNewName = model.masterFiles[index].name
                newViewFilePath = model.directory
                showSubView[7] = true
            },
            info: { index in
                guard let path = focusItem(index) else { return }
                fileInfo = FileInfo.getFileInfo(filePath: path)
                showSubView[3] = true
            },
            delete: { index in
                guard focusItem(index) != nil else { return }
                requestDeleteSelection()
            },
            more: { index in
                guard focusItem(index) != nil else { return }
                showSubView[1] = true
            },
            canAirDrop: { index in
                guard AirDropBridge.isAvailable,
                      model.masterFiles.indices.contains(index) else { return false }
                // 폴더는 tvOS 에서 동작이 불확실하다. 압축해서 보내면 된다.
                return !model.masterFiles[index].name.hasSuffix("/")
            },
            airDrop: { index in
                guard let path = focusItem(index) else { return }
                AirDropBridge.send(filePath: path) { error in
                    if let error = error {
                        alertTitle = "AirDrop"
                        alertMsg = error.localizedDescription
                        showAlert = true
                    }
                }
            }
        )
    }

    // MARK: - 클립보드

    /// 지금 대상이 되는 항목들. 고른 것이 있으면 그것, 없으면 마지막으로 만진 파일.
    var currentTargets: [String] {
        let selected = pane.selectedPaths
        if !selected.isEmpty { return selected }
        return newViewFilePath.isEmpty ? [] : [newViewFilePath]
    }

    func clipboardTake(mode: ClipboardMode) {
        let targets = currentTargets
        guard !targets.isEmpty else {
            alertTitle = "고른 항목이 없습니다"
            alertMsg = "파일을 길게 눌러 고르거나 여러 개 고르기를 켜십시오."
            showAlert = true
            return
        }
        clipboard.take(targets, mode: mode)
        pane.multiSelect = false
        pane.resetMultiSelectArrays()
    }

    /// 지금 보고 있는 폴더에 붙여넣는다. 목적지를 입력할 일이 없다.
    func clipboardPaste() {
        guard !clipboard.isEmpty else { return }
        let sources = clipboard.paths
        let move = clipboard.mode == .move
        let destination = pane.directory
        let destinationPane = pane

        transfer.run(sources: sources,
                     destination: destination,
                     move: move,
                     onBlocked: { reason in
                        alertTitle = move ? "이동이 차단되었습니다" : "복사가 차단되었습니다"
                        alertMsg = reason
                        showAlert = true
                     },
                     onFinish: { done, failed in
                        destinationPane.updateFiles()
                        // 잘라내기는 원본이 사라졌으니 양쪽 다 다시 읽는다.
                        if move {
                            leftPane.updateFiles()
                            rightPane.updateFiles()
                            clipboard.clear()
                        }
                        if !failed.isEmpty {
                            alertTitle = "일부를 옮기지 못했습니다"
                            alertMsg = "\(done)개 완료, \(failed.count)개 실패\n" + failed.prefix(5).joined(separator: "\n")
                            showAlert = true
                        }
                     })
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

        // 큰 파일을 복사하는 동안 화면이 멈추면 안 된다. 뒷줄에서 돌리고 진행률만 올린다.
        let destination = otherPane.directory
        let destinationPane = otherPane
        let sourcePane = pane
        transfer.run(sources: sources,
                     destination: destination,
                     move: move,
                     onBlocked: { reason in
                        alertTitle = move ? "이동이 차단되었습니다" : "복사가 차단되었습니다"
                        alertMsg = reason
                        showAlert = true
                     },
                     onFinish: { done, failed in
                        destinationPane.updateFiles()
                        if move { sourcePane.updateFiles() }
                        sourcePane.multiSelect = false
                        if !failed.isEmpty {
                            alertTitle = "일부를 옮기지 못했습니다"
                            alertMsg = "\(done)개 완료, \(failed.count)개 실패\n" + failed.prefix(5).joined(separator: "\n")
                            showAlert = true
                        }
                     })
    }

    /// 마지막 이동·이름 변경을 되돌린다.
    func undoLastAction() {
        guard transfer.canUndo else {
            alertTitle = "되돌릴 작업이 없습니다"
            alertMsg = "이동과 이름 바꾸기만 되돌릴 수 있습니다."
            showAlert = true
            return
        }
        transfer.undoLast(onBlocked: { reason in
            alertTitle = "되돌리기가 차단되었습니다"
            alertMsg = reason
            showAlert = true
        }, onFinish: { ok in
            leftPane.updateFiles()
            rightPane.updateFiles()
            if !ok {
                alertTitle = "되돌리지 못했습니다"
                alertMsg = "원래 자리에 같은 이름이 있거나 경로가 사라졌습니다."
                showAlert = true
            }
        })
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
        // 미리보기가 보여준 그 목록을 그대로 지운다.
        let targets = pendingDeletePaths
        guard !targets.isEmpty else { return }

        let inTrash = PathSafety.risk(of: directory) != .forbidden
            && directory.contains("/.Trash")

        for path in targets {
            if inTrash {
                TrashManager.shared.deletePathDirectly(path)
            } else {
                TrashManager.shared.moveToTrash(path: path)
            }
        }

        pendingDeletePaths = []
        multiSelect = false
        resetMultiSelectArrays()
        updateFiles()
    }
}

struct SpartanFile: Hashable, Encodable, Decodable {
    var name: String
    var fullPath: String
    var isSelected: Bool
    var fileType: Double
    var isLoadingFile: Bool

    // 정렬용. 폴더를 읽을 때 한 번만 채운다 — 비교할 때마다 디스크를 치면
    // 파일 300개 폴더에서 attributesOfItem 이 O(n log n)번 돈다.
    //
    // 선택형인 이유는 이미 디스크에 깔린 캐시 때문이다. 필수 항목으로 두면
    // 예전 캐시의 JSON 에 키가 없어 디코딩이 통째로 실패하고, 모든 폴더가
    // 한 번씩 다시 훑긴다. 선택형이면 조용히 nil 로 들어온 뒤 채워진다.
    var size: Int64? = nil
    var modified: Date? = nil
} // i am so glad I switched to this instead of having like five different arrays
