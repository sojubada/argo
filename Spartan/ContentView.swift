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
    /// ⌘L 경로로 이동, ⌘⇧F 걸러보기. showSubView 번호를 더 늘리지 않는다 —
    /// 번호로 시트를 고르는 방식은 겹쳐 쓰기 쉽다(25번이 그랬다).
    @State private var showGoToPath = false
    @State private var showFilterList = false

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

    /// WebDAV 가 떠 있는가. 화면을 통째로 덮는 창이다.
    ///
    /// 한때 칸을 나눠 반대 칸에 띄웠다. 받을 곳이 보인다는 점은 좋았지만,
    /// 원격 목록은 칸에 들어가기에는 넓어 단추 글자가 줄줄이 잘렸고, 칸이
    /// 바뀌는 사이 포커스가 어느 목록에도 없어 스페이스가 먹지 않았다.
    /// 다시 한 창으로 돌린다.
    @State var showWebDAV = false

    var directoryBinding: Binding<String> {
        Binding(get: { pane.directory }, set: { pane.directory = $0 })
    }

    /// 받은 것이 놓일 곳. 미디어 폴더를 기본으로 둔다.
    ///
    /// 보고 있던 폴더로 받으면 시스템 폴더 한가운데에 내려받는 수가 있다.
    /// 받을 곳은 늘 같은 자리여야 찾기도 쉽다.
    static let webdavDefaultTarget = "/var/mobile/Media/"

    /// WebDAV 를 연다.
    func openWebDAV() {
        showWebDAV = true
        ArgoDiag.log("webdav", "열림 — 받는 곳 \(Self.webdavDefaultTarget)")
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
    /// 오피스 문서를 PDF 로 바꾸는 중인가. 연타로 겹쳐 도는 것을 막는다.
    @State private var isConvertingOffice = false
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
                    // 위쪽 경로 칸을 뗐다.
                    //
                    // 칸마다 제 머리줄에 경로가 나오므로 같은 것을 두 번
                    // 보여 주고 있었다. 그리고 그 칸이 열려 있는 동안 누른
                    // ⌘⌫ 가 파일 삭제로 흘러들어, 글자를 지우려다 폴더가
                    // 지워지는 길이 되어 있었다.
                    //
                    // 그 줄에 따로 남아 있던 새로 읽기 단추도 툴바로 옮겼다.
                    // 거기 붙어 있던 새 파일·폴더·링크 시트는 freeSpace 쪽으로 옮겼다.
                // 툴바가 화면보다 넓어지면 부모 VStack 의 폭이 같이 커져
                // 화면 전체가 옆으로 밀려 잘린다. 가로 스크롤로 폭을 가둔다 —
                // 포커스가 화면 밖 버튼으로 가면 tvOS 가 알아서 스크롤한다.
                ScrollView(.horizontal, showsIndicators: false) {
                HStack {
                    // 자주 쓰는 차례: 전원 · 홈 · 새로 읽기 · 두 칸 · 즐겨찾기.
                    // Group 으로 묶은 것은 ViewBuilder 자식 수를 열 개 아래로 두려는 것이다.
                    Group {
                    Button(action: { //quit
                        quitArgo()
                    }) {
                        Image(systemName: "power")
                            .frame(width: 50, height: 50)
                    }

                    Button(action: { //홈 — 미디어 폴더로
                        pane.navigate(to: "/var/mobile/Media/")
                    }) {
                        Image(systemName: "house")
                            .frame(width: 50, height: 50)
                    }

                    Button(action: { //새로 읽기 — 두 칸 모두
                        refreshBothPanes()
                    }) {
                        Image(systemName: "arrow.clockwise")
                            .frame(width: 50, height: 50)
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
                    }

                    Button(action: { //webdav
                        openWebDAV()
                    }) {
                        Image(systemName: "globe")
                            .frame(width:50, height:50)
                    }
                    // 받는 곳은 미디어 폴더로 고정한다.
                    //
                    // 보고 있던 폴더로 받으면 시스템 폴더 한가운데로
                    // 내려받는 수가 있고, 나중에 어디에 받았는지 찾기도
                    // 어렵다. 늘 같은 자리면 둘 다 없어진다.
                    //
                    // showSubView[25] 는 쓰지 않는다 — 그 깃발에는
                    // WebServerView 가 걸려 있어, 한 깃발에 창이 둘이면
                    // 어느 쪽이 뜨는지가 두 .sheet 중 누가 이기느냐에
                    // 달리게 된다.
                    .fullScreenCover(isPresented: $showWebDAV, onDismiss: {
                        // 받은 것이 바로 보여야 한다. 받는 곳을 보고 있던
                        // 칸이면 그 칸을, 아니면 양쪽 다 새로 읽는다.
                        leftPane.updateFiles()
                        rightPane.updateFiles()
                    }, content: {
                        WebDAVRootView(
                            // 읽을 때는 늘 미디어 폴더 — 받는 곳이 된다.
                            // 쓸 때는 보던 칸을 그리로 옮긴다 — '받은 폴더
                            // 열기' 가 이 길로 온다. 하나로 묶어 두면 둘 중
                            // 하나는 못 하게 된다.
                            directory: Binding(
                                get: { Self.webdavDefaultTarget },
                                set: { pane.navigate(to: $0) }),
                            isPresented: $showWebDAV)
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
                        Image(systemName: (toolbarExpanded || multiSelect)
                              ? "chevron.left.circle" : "ellipsis.circle")
                            .frame(width: 50, height: 50)
                    }

                    // 펼침 영역도 자주 쓰는 차례로 둔다:
                    // 검색 · 새로 만들기 · 고르기(→ 이동·복사·압축·삭제) · 정렬 · 숨긴 항목
                    // · 반대 칸 명령 · 외장 드라이브 · 단축키 도움말 · 디버그.
                    Group {
                    // 들어내지 않고 접는다. 검색·드라이브 단추에 시트가 붙어 있어,
                    // 고르는 중에 들어내면 ⌘F 가 아무 일도 하지 않았다.
                    Group {
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
                                pane.navigate(to: newViewFilePath.hasSuffix("/")
                                              ? newViewFilePath : newViewFilePath + "/")
                            } else {
                                openFileAtFullPath(newViewFilePath)
                            }
                            didSearch = false
                        }
                    }, content: {
                        SearchView(directory: directoryBinding, isPresenting: $showSubView[19], selectedFile: $newViewFilePath, didSearch: $didSearch)
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    }) //my search function is very complex and i'm not sure how I wrote it. i don't fully understand it now and i didn't understand it then, either. I know it works and so im happy
                    //someday I'll figure it out. It's surprisingly good

                    Button(action: { //경로로 이동 (⌘L)
                        showGoToPath = true
                    }) {
                        Image(systemName: "arrow.right.to.line.circle")
                            .frame(width: 50, height: 50)
                    }

                    Button(action: { //걸러보기 (⌘⇧F). 켜져 있으면 채운 아이콘
                        showFilterList = true
                    }) {
                        Image(systemName: pane.filter.isEmpty
                              ? "line.3.horizontal.decrease.circle"
                              : "line.3.horizontal.decrease.circle.fill")
                            .frame(width: 50, height: 50)
                    }

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
                    }
                    .collapsed(multiSelect)

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

                    // 고르는 중에만 펼친다. 고른 것에 쓸 단추가 고르기 바로 뒤에 온다.
                    Group {
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
                            // 고른 표시까지 지운다. 전에는 모드만 껐다 — 동그라미는
                            // 사라지는데 파일은 고른 채로 남아, 다음 삭제·복사가
                            // 보이지 않는 선택을 대상으로 삼았다.
                            resetMultiSelectArrays()
                            withAnimation {
                                multiSelect = false
                            }
                            allWereSelected = false
                        }) {
                            Image(systemName: "xmark")
                                .frame(width:50, height:50)
                        }
                    }
                    .collapsed(!multiSelect)

                    Group {
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

                    Button(action: { //숨긴 항목
                        pane.showHidden.toggle()
                        UserDefaults.settings.set(pane.showHidden, forKey: "showHiddenFiles")
                        pane.applySortAndFilter()
                    }) {
                        Image(systemName: pane.showHidden ? "eye" : "eye.slash")
                            .frame(width: 50, height: 50)
                    }
                    }
                    .collapsed(multiSelect)

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

                    Group {
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

                    Button(action: { //단축키 도움말. tvOS에는 메뉴 막대가 없어 이 화면이 유일한 안내다
                        showSubView[39] = true
                    }) {
                        Image(systemName: "keyboard")
                            .frame(width: 50, height: 50)
                    }
                    }
                    .collapsed(multiSelect)

                    debugMenu
                        .frame(alignment: .leading)
                    freeSpace
                        .frame(alignment: .trailing)
                    .sheet(isPresented: $showSubView[6], onDismiss: refreshBothPanes, content: {
                        CreateDirectoryView(directoryPath: directoryBinding, isPresented: $showSubView[6])
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                    .sheet(isPresented: $showSubView[5], onDismiss: refreshBothPanes, content: {
                        CreateFileView(filePath: directoryBinding, isPresented: $showSubView[5])
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                    .sheet(isPresented: $showSubView[20], content: {
                        CreateSymlinkView(symlinkPath: directoryBinding, isPresented: $showSubView[20])
                            .onAppear { ArgoTextEntry.begin() }
                            .onDisappear { ArgoTextEntry.end() }
                    })
                        .sheet(isPresented: $showSubView[15], content: {
                            SpawnView(filePath: $newViewFilePath, fileName: $newViewFileName, isPresented: $showSubView[15])
                        })
                        .sheet(isPresented: $showSubView[4], content: {
                            TextView(filePath: newViewFilePath, fileName: newViewFileName, isPresented: $showSubView[4])
                        })
                        .sheet(isPresented: $showSubView[7], onDismiss: refreshBothPanes, content: {
                            // 바꾼 이름은 되돌리기에 남긴다. 기록 함수는 있었는데
                            // 이름 바꾸기 창이 부르지 않아 ⌘Z 가 듣지 않았다.
                            RenameFileView(fileName: $renameFileCurrentName, newFileName: $renameFileNewName, filePath: $newViewFilePath, isPresented: $showSubView[7],
                                           onRenamed: { from, to in transfer.recordRename(from: from, to: to) })
                                .onAppear { ArgoTextEntry.begin() }
                                .onDisappear { ArgoTextEntry.end() }
                        })
                        .sheet(isPresented: $showSubView[8], onDismiss: refreshBothPanes, content: {
                            MoveFileView(fileNames: $newViewArrayNames, filePath: $newViewFilePath, multiSelect: multiSelectBinding, isPresented: $showSubView[8],
                                         onMoved: { transfer.recordMoves($0) })
                        })
                        .sheet(isPresented: $showSubView[9], onDismiss: refreshBothPanes, content: {
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
                                                       presentTextEditor()
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
                    //
                    // 고르는 중이면 펼침 영역을 연다. 이동·복사·압축·삭제 단추가
                    // 이 안에 있어서, 접혀 있으면 골라 놓고도 할 일을 찾을 수
                    // 없었다(이슈 #2).
                    .collapsed(!(toolbarExpanded || multiSelect))
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
                                         onFocus: { if activeSide != .left { activeSide = .left } },
                                         menu: rowMenu(for: .left))
                            FilePaneView(model: rightPane,
                                         isActive: activeSide == .right,
                                         onOpen: { index in openInPane(.right, index: index) },
                                         onGoUp: { activeSide = .right; rightPane.goUp() },
                                         onFocus: { if activeSide != .right { activeSide = .right } },
                                         menu: rowMenu(for: .right))
                        }
                    } else {
                        FilePaneView(model: leftPane,
                                     // 한 칸일 때는 강조 테두리가 의미 없다 —
                                     // 어느 칸이 명령을 받는지 보여줄 상대가 없다.
                                     isActive: false,
                                     onOpen: { index in openInPane(.left, index: index) },
                                     onGoUp: { activeSide = .left; leftPane.goUp() },
                                     onFocus: { if activeSide != .left { activeSide = .left } },
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
                    
                    // 지우는 길은 TrashManager 하나로 모은다.
                    //
                    // 전에는 RootHelperActs.rm 을 불렀다. 그것은 루트 도우미
                    // 바이너리를 띄우는데, TrollStore 로 깔면 그 바이너리가
                    // 없어 아무 일도 일어나지 않는다. 그런데도 바로 아래에서
                    // masterFiles 에서 줄을 빼 버려서, 화면에서만 사라지고
                    // 파일은 디스크에 남았다. 정렬을 한 번 바꾸면 rawFiles
                    // 에서 다시 읽어 와 지운 파일이 되살아났다.
                    //
                    // 화면에서 줄을 빼는 것도 그만둔다. masterFiles 는
                    // rawFiles 를 걸러 만든 결과라, 결과만 건드리면 원본과
                    // 어긋난다. 지운 뒤 다시 읽는다.
                    if(directory == "/private/var/mobile/Media/.Trash/"){
                        ContextMenuButtonTV(stringKey: "DELETE", action: {
                            if TrashManager.shared.deletePathDirectly(masterFiles[newViewFileIndex].fullPath) {
                                updateFiles()
                            }
                            showSubView[1] = false
                        })
                    } else if(directory == "/private/var/mobile/Media/" && masterFiles[newViewFileIndex].name == ".Trash/"){
                        ContextMenuButtonTV(stringKey: "TRASHYEET", action: {
                            _ = TrashManager.shared.emptyAll()
                            updateFiles()
                            showSubView[1] = false
                        })
                    } else {
                        ContextMenuButtonTV(stringKey: "GOTOTRASH", action: {
                            if TrashManager.shared.moveToTrash(path: masterFiles[newViewFileIndex].fullPath) {
                                updateFiles()
                            }
                            showSubView[1] = false
                        })
                    }
                    if(deleteOverride) { //i dont think this can display but it's HERE ANYWAY
                        ContextMenuButtonTV(stringKey: "DELETE", action: {
                            if TrashManager.shared.deletePathDirectly(masterFiles[newViewFileIndex].fullPath) {
                                updateFiles()
                            }
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
                                    presentTextEditor()
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
                    // 폴더가 아니라 파일을 가리키며 열린 경우. 그 파일이 든
                    // 폴더를 띄우고 파일을 연다.
                    //
                    // 전에는 열고 나서 directory 를 직접 잘라 윗폴더로
                    // 옮겼는데, components 가 비면 removeLast() 가 그 자리에서
                    // 죽었다 — "/" 로 열리면 그랬다.
                    if FileInfo.yandereDevFileType(file: directory) != 0 {
                        multiSelect = false
                        openFileAtFullPath(directory, recordHistory: false)
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
                        // 폴더면 용량을 셀 수 있게 한다. 누를 때만 센다.
                        if FileInfo.isDirectory(filePath: directory + newViewFileName) {
                            FolderSizeRow(path: directory + newViewFileName)
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
            .onReceive(NotificationCenter.default.publisher(for: ArgoSelectToggle.notification)) { _ in
                handleSelectToggle()
            }
            .sheet(isPresented: $showSubView[39], content: {
                KeyboardHelpView(isPresented: $showSubView[39])
            })
            .sheet(isPresented: $showGoToPath) {
                GoToPathView(startDirectory: pane.directory,
                             isPresented: $showGoToPath,
                             onGo: { folder, file in
                                 pane.navigate(to: folder)
                                 // 파일을 쳤으면 그 파일만 보이게 거른다. 포커스는
                                 // 코드로 옮길 수 없지만, 한 줄만 남으면 바로 닿는다.
                                 // 다른 폴더로 가면 걸러보기는 저절로 풀린다.
                                 if let file {
                                     pane.filter = file
                                     pane.applySortAndFilter()
                                 }
                             })
            }
            .sheet(isPresented: $showFilterList) {
                FilterListView(initial: pane.filter,
                               isPresented: $showFilterList,
                               onApply: { text in
                                   pane.filter = text
                                   pane.applySortAndFilter()
                               })
            }
            
            // 진행률. 큰 파일을 복사하는 동안 멈춘 것처럼 보이면 안 된다.
            if transfer.isRunning {
                VStack(spacing: 16) {
                    Text(transfer.currentName)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    ProgressView(value: transfer.progress)
                        .frame(width: 600)
                    // 복사는 바이트로 보여 준다. 큰 파일 하나도 움직이는 것이 보여야 한다.
                    Text(transferCaption)
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Button(action: { transfer.cancel() }) {
                        Text(transfer.isCancelling ? "멈추는 중…" : "취소")
                    }
                    .disabled(transfer.isCancelling)
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
            
        } // End ZStack
        // 일괄 삭제 미리보기.
        //
        // 겹쳐 그리던 것을 진짜 모달로 바꿨다. ZStack 으로 덮기만 하면
        // 포커스 엔진은 그것을 가림막으로 보지 않는다 — 뒤의 파일 목록도
        // 여전히 포커스 대상이라, 한 칸일 때는 목록을 빠져나와 '삭제'
        // 단추까지 갈 기하적 경로가 없었다. 두 칸일 때 반대 칸을 거쳐
        // 좌우로 가면 닿았던 것이 그 증거다.
        .fullScreenCover(isPresented: $showBatchDeletePreview) {
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
        }
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

    private func presentOfficeReader() {
        let path = readerPath()

        // 변환기가 맡는 형식이면 기기 안에서 PDF 로 바꿔 PDF 뷰어로 엽니다.
        // 그 뷰어는 쪽 넘김·두 쪽·맞춤이 이미 제대로 돌아, 웹뷰에서 CSS 로
        // 수습하던 일들이 통째로 없어집니다.
        guard ArgoOfficeConverter.handles(path) else {
            // 변환기가 안 맡는 형식만 아직 웹 뷰어로 갑니다.
            //
            // .xls 는 번들된 SheetJS 가 읽습니다 — calamine 이 들어오면
            // 이 줄도 없어집니다. .doc 와 .ppt 는 웹 뷰어에도 그릴 것이
            // 없어 빈 화면만 나오므로, 믿을 수 없는 바이트를 스크립트
            // 엔진에 넘기지 않고 여기서 끝냅니다.
            if (path as NSString).pathExtension.lowercased() == "xls" {
                presentWebOfficeReader(path)
            } else {
                presentCannotOpen(path, reason: "아직 지원하지 않는 형식입니다.")
            }
            return
        }

        // 변환이 도는 동안 또 부르지 않는다.
        //
        // 큰 pptx 는 여섯 초가 넘게 걸리는데 그동안 화면이 멈춘 것처럼
        // 보여서, 리모컨을 몇 번 더 누르게 된다. 누른 만큼 변환이 겹쳐
        // 돌고 뷰어도 겹쳐 떴다 — 로그에 같은 파일이 연달아 세 번 변환된
        // 자국이 남아 있다.
        guard !isConvertingOffice else {
            ArgoDiag.log("convert", "이미 변환 중 — 다시 부르지 않는다")
            return
        }
        isConvertingOffice = true
        // 누른 줄에 도는 표시를 띄운다. 칸이 이미 그릴 줄 안다.
        setRowLoading(path, true)

        DispatchQueue.global(qos: .userInitiated).async {
            let converted = try? ArgoOfficeConverter.convertToPDF(at: path)
            DispatchQueue.main.async {
                isConvertingOffice = false
                setRowLoading(path, false)
                guard let pdf = converted else {
                    // 변환에 실패해도 웹 뷰어로 되돌리지 않습니다.
                    //
                    // 되돌림은 공격자가 고를 수 있는 길이었습니다 — 일부러
                    // 변환을 깨뜨리는 문서를 만들면, 살균하지 않고 DOM 에
                    // 꽂는 렌더러로 건너갈 수 있었습니다. 이 앱은 샌드박스
                    // 밖에서 / 를 읽고 씁니다. 못 여는 편이 낫습니다.
                    presentCannotOpen(path, reason: "문서를 읽지 못했습니다.")
                    return
                }
                let vc = ArgoPDFReaderController()
                vc.documentPath = pdf
                // 변환기가 요청대로 못 간 것이 있으면 그 줄을 같이 넘깁니다.
                vc.notice = ArgoOfficeConverter.lastUserNotice
                vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
                presentReader(vc)
            }
        }
    }

    /// 그 경로의 줄에 도는 표시를 켜거나 끕니다. 양쪽 칸을 다 봅니다 —
    /// 변환이 끝날 즈음 활성 칸이 바뀌어 있을 수 있습니다.
    private func setRowLoading(_ path: String, _ loading: Bool) {
        for model in [leftPane, rightPane] {
            if let i = model.masterFiles.firstIndex(where: { $0.fullPath == path }) {
                model.masterFiles[i].isLoadingFile = loading
            }
        }
    }

    /// 열 수 없다고 알립니다. 웹 뷰어로 되돌리지 않습니다.
    private func presentCannotOpen(_ path: String, reason: String) {
        let name = (path as NSString).lastPathComponent
        ArgoDiag.log("open", "거부 \(name) — \(reason)")
        guard let top = AirDropBridge.topViewController() else { return }
        let alert = UIAlertController(title: "이 문서는 열 수 없습니다",
                                      message: "\(name)\n\n\(reason)",
                                      preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: "확인", style: .default))
        top.present(alert, animated: true)
    }

    private func presentWebOfficeReader(_ path: String) {
        let vc = ArgoWebDocumentViewController()
        vc.documentPath = path
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        presentReader(vc)
    }

    /// 글·코드 편집기. 전체 화면으로 띄운다 — 시트로 띄우면 화면 가장자리가
    /// 잘려 줄 번호와 상태줄이 들어갈 자리가 없다.
    private func presentTextEditor() {
        let vc = ArgoCodeEditorController()
        vc.filePath = readerPath()
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        vc.onSaved = { refreshBothPanes() }
        presentReader(vc)
    }

    private func presentTextReader() {
        let vc = ArgoTextReaderController()
        vc.filePath = readerPath()
        vc.onClose = { [weak vc] in vc?.dismiss(animated: true) }
        vc.onEdit = { [weak vc] in
            vc?.dismiss(animated: true) {
                presentTextEditor()
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
                    // 오류의 종류는 코드로 가린다.
                    //
                    // 전에는 영어 메시지의 끝 33글자가 "don't have permission
                    // to view it." 인지 비교했다. 기기 말이 한국어면 그 문장이
                    // 아예 나오지 않아 권한 거부 알림이 한 번도 뜨지 않았고,
                    // 폴더는 조용히 안 열렸다.
                    let nsErr = error as NSError
                    let noPermission = nsErr.domain == NSCocoaErrorDomain
                        && (nsErr.code == NSFileReadNoPermissionError
                            || nsErr.code == NSFileReadUnknownError)
                    if noPermission {
                        alertTitle = NSLocalizedString("SHOW_DENIED", comment: "Here's the graduate.")
                        alertMsg = NSLocalizedString("INFO_DENIED", comment: "You're monsters!")
                    } else {
                        alertTitle = NSLocalizedString("SHOW_DENIED", comment: "Here's the graduate.")
                        alertMsg = error.localizedDescription
                    }
                    showAlert = true
                    denied = true
                }
                if(!denied){
                    if(isDirectPath) {
                        updateFiles()
                    } else {
                        // directory 에 직접 이어 붙이지 않는다.
                        //
                        // navigate 가 되돌아갈 기록을 남기는 유일한 통로인데,
                        // 여기서 directory 를 덮어쓰면 그 통로를 건너뛴다.
                        // 폴더를 아무리 타고 들어가도 backStack 이 비어 있어
                        // ⌘[ 가 듣지 않던 이유다.
                        pane.enter(fileToCheck[index])
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
                            pane.navigate(to: dest.hasSuffix("/") ? dest : dest + "/")
                        } else {
                            openFileAtFullPath(dest)
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
				presentOfficeReader()
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
        case .goBackOrUp:
            // ⌫. 기록이 있으면 뒤로, 없으면 위로 — 막 켰을 때도 키가 먹는다.
            if pane.canGoBack { pane.goBack() } else { pane.goUp() }
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
        // 이름 바꾸기·정보는 길게 눌러 뜨는 메뉴와 같은 동작을, 포커스된 줄을
        // 대상으로 부른다.
        //
        // 전에는 창만 열었다. 대상은 '지난번에 메뉴로 고른 줄' 로 남아 있어서,
        // ⌘⏎ 를 누르면 지금 보고 있는 파일이 아니라 지난번에 이름을 바꿨던
        // 파일의 이름 바꾸기 창이 떴다.
        case .rename:
            if let i = pane.lastFocusedIndex { rowMenu(for: activeSide).rename(i) }
            else { noFocusedRow() }
        case .fileInfo:
            if let i = pane.lastFocusedIndex { rowMenu(for: activeSide).info(i) }
            else { noFocusedRow() }
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
            // 위쪽 경로 칸을 뗐으므로 경로를 칠 작은 창을 띄운다.
            showGoToPath = true
        case .filterList:
            showFilterList = true
        }
    }

    /// ⌫ 로 삭제. 기존 일괄 삭제 미리보기를 거친다 — 되돌릴 수 없는 일이라 확인을 뺄 수 없다.
    func requestDeleteSelection() {
        // 고른 것이 없으면 포커스가 올라간 **줄** 을 지운다.
        //
        // 전에는 newViewFilePath 로 물러섰는데, 그 값은 폴더에 들어갈 때마다
        // '지금 폴더' 로 덮어써진다. 그래서 아무것도 고르지 않은 채 ⌘⌫ 를
        // 누르면 보고 있던 폴더가 통째로 지울 것이 됐다 — 경로 칸에 글자를
        // 치다가 지우려고 ⌫ 를 눌러도 같은 일이 났다.
        let targets = currentTargets
        guard !targets.isEmpty else {
            alertTitle = "삭제할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }

        // 보고 있는 폴더와 그 윗폴더는 지울 수 없다.
        //
        // 목록에 보이는 것을 지우는 자리이지, 발밑을 지우는 자리가 아니다.
        // 한 번 이렇게 지워지면 되돌릴 길이 없다.
        if let bad = targetUnderfoot(targets) {
            refuseUnderfoot(bad, verb: "지울")
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

    /// 줄을 대상으로 하는 단축키를 눌렀는데 포커스된 줄이 없을 때.
    func noFocusedRow() {
        alertTitle = "대상이 없습니다"
        alertMsg = "목록에서 줄을 하나 고른 뒤 다시 누르십시오."
        showAlert = true
    }

    /// 만들기·이름 바꾸기·이동·복사 창이 닫힐 때. 양쪽 칸을 새로 읽는다.
    ///
    /// 전에는 아무 칸도 새로 읽지 않아, 만든 폴더나 바꾼 이름이 직접 새로
    /// 고치기 전까지 목록에 나오지 않았다. 이동·복사는 반대 칸이 받는 곳일
    /// 수 있으므로 둘 다 읽는다.
    func refreshBothPanes() {
        leftPane.updateFiles()
        rightPane.updateFiles()
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

    /// 스페이스. 포커스가 올라간 줄을 고르거나 풉니다.
    ///
    /// 커서를 옮기며 여러 줄을 고를 수 있어야 하므로, 고른 뒤에도 포커스는
    /// 그대로 둡니다 — 고르기가 끝나면 복사든 삭제든 기존 명령이 고른 것을
    /// 그대로 받습니다.
    func handleSelectToggle() {
        guard pane.toggleSelectionAtFocus() else {
            ArgoDiag.log("select", "스페이스 — 포커스된 줄이 없다")
            return
        }
        ArgoDiag.log("select", "스페이스 — 고른 것 \(pane.multiSelectFiles.count)개")
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
            // 메뉴가 뜨면 포커스는 메뉴로 간다. 누른 줄을 대상으로 못박아 둔다.
            model.lastFocusedPath = file.fullPath
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
    /// 지우기·복사·잘라내기·반대 칸으로 보내기가 다룰 대상.
    ///
    /// 고른 것이 있으면 그것, 없으면 이 칸에서 마지막으로 포커스됐던 줄.
    ///
    /// 전에는 고른 것이 없을 때 newViewFilePath 로 물러섰다. 그 값은 폴더에
    /// 들어갈 때마다 '지금 폴더' 로 덮어써진다. 그래서 아무것도 고르지 않고
    /// ⌘⌫ 를 누르면 보던 폴더가 지울 것이 됐고, ⌘X 나 ⌘⌥X 를 누르면 보던
    /// 폴더가 통째로 잘라내지거나 반대 칸으로 옮겨질 참이었다. 세 곳이 같은
    /// 방식이었으므로 한 곳으로 모은다.
    var currentTargets: [String] {
        let selected = pane.selectedPaths
        if !selected.isEmpty { return selected }
        if let i = pane.lastFocusedIndex { return [pane.masterFiles[i].fullPath] }
        return []
    }

    /// 대상 중에 보던 폴더나 그 윗폴더가 있으면 그것을 돌려준다.
    ///
    /// 목록에 보이는 것을 다루는 자리이지 발밑을 다루는 자리가 아니다.
    /// 보던 폴더를 지우거나 옮기면 되돌릴 길이 없다.
    func targetUnderfoot(_ targets: [String]) -> String? {
        let here = Self.lexicalPath(pane.directory)
        return targets.first { target in
            let t = Self.lexicalPath(target)
            return t == here || here.hasPrefix(t + "/")
        }
    }

    /// 발밑을 다루려 할 때 알린다.
    func refuseUnderfoot(_ path: String, verb: String) {
        alertTitle = "이 폴더는 \(verb) 수 없습니다"
        alertMsg = (path as NSString).lastPathComponent
            + "\n\n지금 보고 있는 폴더이거나 그 윗폴더입니다. 윗폴더로 올라가서 하십시오."
        showAlert = true
        ArgoDiag.log("target", "거부(\(verb)) — 현재 폴더/윗폴더 \(path)")
    }

    func clipboardTake(mode: ClipboardMode) {
        let targets = currentTargets
        guard !targets.isEmpty else {
            alertTitle = "고른 항목이 없습니다"
            alertMsg = "파일을 길게 눌러 고르거나 여러 개 고르기를 켜십시오."
            showAlert = true
            return
        }
        if let bad = targetUnderfoot(targets) {
            refuseUnderfoot(bad, verb: mode == .move ? "잘라낼" : "복사할")
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
                     onFinish: { done, failed, cancelled in
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
                        } else if cancelled {
                            // 쓰다 만 것은 지웠다. 끝난 것은 남아 있다는 것을 알려야
                            // 반쯤 된 상태를 찾아 헤매지 않는다.
                            alertTitle = "취소했습니다"
                            alertMsg = "\(done)개는 끝났고, 하던 것은 지웠습니다."
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
        let sources = currentTargets
        guard !sources.isEmpty else {
            alertTitle = "복사할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }
        if let bad = targetUnderfoot(sources) {
            refuseUnderfoot(bad, verb: move ? "옮길" : "복사할")
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
                     onFinish: { done, failed, cancelled in
                        destinationPane.updateFiles()
                        if move { sourcePane.updateFiles() }
                        sourcePane.multiSelect = false
                        if !failed.isEmpty {
                            alertTitle = "일부를 옮기지 못했습니다"
                            alertMsg = "\(done)개 완료, \(failed.count)개 실패\n" + failed.prefix(5).joined(separator: "\n")
                            showAlert = true
                        } else if cancelled {
                            // 쓰다 만 것은 지웠다. 끝난 것은 남아 있다는 것을 알려야
                            // 반쯤 된 상태를 찾아 헤매지 않는다.
                            alertTitle = "취소했습니다"
                            alertMsg = "\(done)개는 끝났고, 하던 것은 지웠습니다."
                            showAlert = true
                        }
                     })
    }

    /// 진행 창 아래 줄. 복사는 바이트로, 이동은 개수로 보여 준다.
    /// body 안에 그대로 쓰면 타입 검사가 무거워져 따로 뺐다.
    var transferCaption: String {
        let count = "\(transfer.completed) / \(transfer.total)"
        guard transfer.bytesTotal > 0 else { return count }
        let done = ByteCountFormatter.string(fromByteCount: transfer.bytesDone, countStyle: .file)
        let all = ByteCountFormatter.string(fromByteCount: transfer.bytesTotal, countStyle: .file)
        return done + " / " + all + " · " + count + "개"
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

    
    /// 경로를 글자만 보고 다듬는다. 디스크를 건드리지 않는다.
    ///
    /// 휴지통 안인지 가릴 때 `resolvingSymlinksInPath` 를 쓰고 있었다.
    /// 그것은 링크를 **따라간다.** 휴지통 안에 바깥을 가리키는 심볼릭 링크가
    /// 들어 있으면 따라간 끝이 휴지통 밖이라 '휴지통 안이 아니다' 로
    /// 판정되고, 그러면 휴지통으로 옮기려 든다. 그런데 TrashManager 는
    /// 휴지통 안의 것을 휴지통으로 옮기기를 거부하므로, 그 링크는 지워지지도
    /// 옮겨지지도 않고 그대로 남는다.
    ///
    /// 여기서 알고 싶은 것은 '링크가 가리키는 곳' 이 아니라 '이 항목이 놓인
    /// 자리' 다. 그래서 따라가지 않고 글자만 다듬는다. /var 는 /private/var
    /// 로 가는 심링크라 두 철자가 같은 자리를 가리키므로 한쪽으로 모은다.
    static func lexicalPath(_ path: String) -> String {
        var p = (path as NSString).expandingTildeInPath
        while p.contains("//") { p = p.replacingOccurrences(of: "//", with: "/") }
        var parts: [String] = []
        for piece in p.split(separator: "/", omittingEmptySubsequences: true) {
            switch piece {
            case ".":  continue
            case "..": if !parts.isEmpty { parts.removeLast() }
            default:   parts.append(String(piece))
            }
        }
        var out = "/" + parts.joined(separator: "/")
        if out.hasPrefix("/var/") || out == "/var" { out = "/private" + out }
        return out
    }

    /// 다른 폴더에 있는 파일 하나를 연다.
    ///
    /// 그 파일이 든 폴더로 먼저 가고, 목록에서 제 자리를 찾아 연다.
    ///
    /// 전에는 지금 폴더의 masterFiles 끝에 그 파일을 끼워 넣고 마지막
    /// 인덱스로 열었다. 두 가지가 따라왔다. 여는 쪽은 경로를 `directory +
    /// 이름` 으로 다시 맞추므로, 대상이 다른 폴더에 있으면 있지도 않은
    /// 경로를 여는 꼴이 됐다. 그리고 끼워 넣은 줄이 목록에 그대로 남아,
    /// 누르면 엉뚱한 파일을 건드렸다.
    func openFileAtFullPath(_ path: String, recordHistory: Bool = true) {
        let parent = (path as NSString).deletingLastPathComponent
        let name   = (path as NSString).lastPathComponent
        pane.navigate(to: parent.hasSuffix("/") ? parent : parent + "/",
                      recordHistory: recordHistory)
        guard let idx = masterFiles.firstIndex(where: { $0.name == name }) else {
            // 폴더로는 갔으니 그 자리에서 찾을 수 있다. 조용히 넘어가지 않는다.
            ArgoDiag.log("open", "목록에서 못 찾음 \(name) — \(parent)")
            return
        }
        defaultAction(index: idx, isDirectPath: false)
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

        // 영구 삭제인지는 지울 파일 하나하나를 보고 정한다.
        //
        // 전에는 활성 칸의 directory 에 "/.Trash" 가 들어 있는지로 한꺼번에
        // 정했다. 두 가지가 틀렸다. 이름에 들어 있기만 하면 되므로
        // .Trash_Backup 같은 멀쩡한 폴더의 파일이 휴지통을 거치지 않고
        // 그 자리에서 사라졌고, 지울 것은 pendingDeletePaths 에 전체 경로로
        // 들어 있는데 판정은 '지금 활성인 칸'으로 해서, 미리보기가 떠 있는
        // 사이 포커스가 반대 칸으로 넘어가면 엉뚱한 기준으로 지웠다.
        let trashRoot = TrashManager.trashRootPath

        for path in targets {
            let norm = Self.lexicalPath(path)
            let insideTrash = norm == trashRoot || norm.hasPrefix(trashRoot + "/")
            if insideTrash {
                TrashManager.shared.deletePathDirectly(path)
            } else {
                TrashManager.shared.moveToTrash(path: path)
            }
        }

        pendingDeletePaths = []
        // 고른 것은 양쪽 칸에 걸쳐 있을 수 있다. 활성 칸만 비우면 반대 칸에
        // 지워진 파일이 골라진 채로 남는다.
        leftPane.multiSelect = false
        leftPane.resetMultiSelectArrays()
        rightPane.multiSelect = false
        rightPane.resetMultiSelectArrays()
        leftPane.updateFiles()
        rightPane.updateFiles()
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


extension View {
    /// 계층에서 빼지 않고 접는다. 폭 0 · 투명 · 포커스 불가.
    ///
    /// 툴바 단추에는 시트가 붙어 있다. if 로 단추를 들어내면 그 시트도 같이
    /// 사라져, 단축키로 띄우려 해도 아무 일이 없다.
    func collapsed(_ yes: Bool) -> some View {
        self.frame(maxWidth: yes ? 0 : nil)
            .opacity(yes ? 0 : 1)
            .disabled(yes)
    }
}
