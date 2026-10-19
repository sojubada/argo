# Argo 기존 코드 컨텍스트 (Gemini 작업용)

저장소: https://github.com/sojubada/argo (public, 브랜치 main)
이 문서는 `GEMINI_TASK_SPEC.md` 1단계(휴지통) 작업에 필요한 기존 코드입니다.

**중요 — 치환 블록 작성 시 주의**
아래 발췌는 실제 파일에서 그대로 떠온 것이며 **들여쓰기가 원본 그대로**입니다.
`ContentView.swift`는 **스페이스와 탭이 섞여 있습니다.** 특히 `SpareViews.swift`의
`RootHelperActs`는 탭 들여쓰기입니다. 치환 블록의 "찾을 내용"은 아래에서
**복사해서** 쓰십시오. 직접 타이핑하면 공백이 어긋나 치환이 실패합니다.

행 번호는 현재 main 기준입니다.


## ContentView.swift — 1~100행 (상태 선언부, showSubView 인덱스 주석)

```swift
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
import Foundation
import AVKit
import AVFoundation
import MobileCoreServices
import ApplicationsWrapper
import AssetCatalogWrapper
import SVGWrapper

struct ContentView: View {
    @State var directory: String
    @State private var fileInfo: [String] = []
    @State var deleteOverride = false
    @State var E = false
    @State var E2 = false
    @State var masterFiles: [SpartanFile] = []
    @State var isRootless: Bool
    
    @State var scaleFactor: CGFloat
    @State var buttonCalc = false
    
    @State var didSearch = false
    
    @State var showAlert = false
    @State var alertTitle = ""
    @State var alertMsg = ""
    
    @State var multiSelect = false
    @State var allWereSelected = false
    @State var multiSelectFiles: [String] = []
    
    @State var newViewFilePath: String = ""
    @State var newViewArrayNames: [String] = [""]
    @State var newViewFileName: String = ""
    @State private var newViewFileIndex = 0
    
    @State var renameFileCurrentName: String = ""
    @State var renameFileNewName: String = ""
    
    @State var filePerms = 420
    
    @State var didChangeDir = false
    
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 36)
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
    
    @Binding var globalAVPlayer: AVPlayer //this is because Spartan has the ability to play music without the AudioPlayerView being shown. It took about a week to get working properly and I'm proud of it
    @State var isGlobalAVPlayerPlaying = false
    @State var callback = true
    
    //@Binding var webServer: GCDWebUploader
    
    @State private var isLoadingView = false
    @State var blankString: [String] = [""] //dont question it
    @State private var nonexistentFile = "" //REALLY dont question it
```


## ContentView.swift — 255~300행 (툴바: 마운트/WebDAV/즐겨찾기/설정 버튼. 휴지통 버튼을 여기 추가)

```swift
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
                                FavoritesView(directory: $directory, showView: $showSubView[16])
                            })
                            .sheet(isPresented: $showSubView[17], content: {
                                AddToFavoritesView(filePath: newViewFilePath, displayName: newViewFileName, showView: $showSubView[17])
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
```


## ContentView.swift — 330~365행 (다중선택 툴바의 휴지통 버튼 — 치환 대상 1)

```swift
                                newViewFilePath = directory
                                newViewArrayNames = multiSelectFiles
                            }) {
                                Image(systemName: "doc.zipper")
                                    .frame(width:50, height:50)
                            }
                        
                            Button(action: {
                                if(directory == "/private/var/mobile/Media/.Trash/"){
                                    for file in multiSelectFiles {
                                        RootHelperActs.rm(directory + file)
                                        updateFiles()
                                    }
                                } else {
                                    for file in multiSelectFiles {
                                        RootHelperActs.mv(directory + file, "/private/var/mobile/Media/.Trash/" + file)
                                        updateFiles()
                                    }
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
```


## ContentView.swift — 770~820행 (컨텍스트 메뉴의 삭제/휴지통 분기 — 치환 대상 2)

```swift
                                        newViewFilePath = directory
                                        newViewArrayNames = [masterFiles[index].name]
                                        newViewFileIndex = index
                                        showSubView[2] = true
                                    }) {
                                        Text(NSLocalizedString("OPENIN", comment: "The bee, of course, flies anyway"))
                                            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                            }
                                    }
                                    
                                    if(directory == "/private/var/mobile/Media/.Trash/"){
                                        Button(action: {
                                            RootHelperActs.rm(masterFiles[index].fullPath)
                                            masterFiles.remove(at: index)
                                        }) {
                                            Text(NSLocalizedString("DELETE", comment: "because bees don't care what humans think is impossible."))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                }
                                        }
                                    } else if (directory == "/private/var/mobile/Media/" && masterFiles[index].name == ".Trash/") {
                                        Button(action: {
                                            RootHelperActs.rm("/private/var/mobile/Media/.Trash/")
                                            RootHelperActs.mkdir("/private/var/mobile/Media/.Trash/")
                                            updateFiles()
                                        }) {
                                            Text(NSLocalizedString("TRASHYEET", comment: "Yellow, black. Yellow, black."))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                }
                                        }
                                    } else {
                                        Button(action: {
                                            RootHelperActs.mv(masterFiles[index].fullPath, ("/private/var/mobile/Media/.Trash/" + masterFiles[index].name))
                                            masterFiles.remove(at: index)
                                        }) {
                                            Text(NSLocalizedString("GOTOTRASH", comment: "Yellow, black. Yellow, black."))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                }
                                        }
                                    }
                                    if(deleteOverride) { //this never activates, but I leave it in just in case I ever change how this works
                                        Button(action: {
                                            RootHelperActs.rm(masterFiles[index].fullPath)
                                            masterFiles.remove(at: index)
                                        }) {
                                            Text(NSLocalizedString("DELETE", comment: "Ooh, black and yellow!"))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
```


## ContentView.swift — 1090~1130행 (tvOS13용 메뉴의 삭제/휴지통 분기 — 치환 대상 3)

```swift
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
                            do {
                                try fileManager.removeItem(atPath: "/private/var/mobile/Media/.Trash/")
                            } catch {
                                print("Error emptying Trash: \(error)")
                            }
                            do {
                                try fileManager.createDirectory(atPath: "/private/var/mobile/Media/.Trash/", withIntermediateDirectories: true, attributes: nil)
                            } catch {
                                print("Error emptying Trash: \(error)")
                            }
                            showSubView[1] = false
                        })
                    } else {
                        ContextMenuButtonTV(stringKey: "GOTOTRASH", action: {
                            RootHelperActs.mv(masterFiles[newViewFileIndex].fullPath, ("/private/var/mobile/Media/.Trash/" + masterFiles[newViewFileIndex].name))
                            masterFiles.remove(at: newViewFileIndex)
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
```


## ContentView.swift — 1580~1600행 (defaultAction 진입부, switch fileType 시작)

```swift
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
```


## ContentView.swift — 1845~1870행 (파일 끝: SpartanFile 구조체 등)

```swift
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
}

struct SpartanFile: Hashable, Encodable, Decodable {
    var name: String
    var fullPath: String
    var isSelected: Bool
    var fileType: Double
    var isLoadingFile: Bool
} // i am so glad I switched to this instead of having like five different arrays
```


## SpareViews.swift — 1~30행 (전역 fileManager, tempPath 등)

```swift
//
//  SpareView.swift
//  Spartan
//
//  Created by RealKGB on 4/10/23.
//

import SwiftUI
import UIKit
import Foundation
import ApplicationsWrapper

//this is for various global stuff that doesn't belong to a specific view. frameworks, view modifiers, view elements, UIViewRepresentables, etc

let fileManager = FileManager.default
let appsManager = ApplicationsManager(allApps: LSApplicationWorkspace.default().allApplications())
let tempPath = "/var/mobile/Media/temp"
let paddingInt: CGFloat = -7
let opacityInt: CGFloat = 1.0
var buttonWidth: CGFloat = 500
var buttonHeight: CGFloat = 30
let metadataName = UserDefaults.settings.string(forKey: "tvapothecary")

struct SpareView: View {
    var body: some View {
        VStack {
            Text("Take wrong turns. Talk to strangers. Open unmarked doors. And if you see a group of people in a field, go find out what they are doing.")
            Text("")
            Text("Except when that wrong turn leads you to a nonexistent file.")
        }
```


## SpareViews.swift — 300~375행 (helperPath, isJB, LocalizedString, RootHelperActs 전체)

```swift
func removeLastChar(_ string: String) -> String {
    return string.substring(toIndex: string.count - 1)
}

var helperPath: String = "/bin/RootHelper"
var isJB = true

func LocalizedString(_ key: String) -> String {
	return NSLocalizedString(key, comment: "")
} //this was not added until much later in the project, which is why it's not used much

func nop() { }

struct null: View {
	var body: some View { EmptyView() }
}

extension Data {
    init?(fromHexEncodedString hexString: String) {
        var hexString = hexString
        let length = hexString.count / 2
        var data = Data(capacity: length)
        
        for _ in 0..<length {
            guard let byte = UInt8(hexString.prefix(2), radix: 16) else {
                return nil
            }
            data.append(byte)
            hexString = String(hexString.dropFirst(2))
        }
        self = data
    }
}

class RootHelperActs {
	class func rm(_ filePath: String) {
		spawn(command: helperPath, args: ["rm", filePath], env: [], root: true)
	}
	
	class func mv(_ filePath: String, _ fileDest: String) {
		spawn(command: helperPath, args: ["mv", filePath, fileDest], env: [], root: true)
	}
	
	class func cp(_ filePath: String, _ fileDest: String) {
		spawn(command: helperPath, args: ["cp", filePath, fileDest], env: [], root: true)
	}
	
	class func touch(_ filePath: String) {
		spawn(command: helperPath, args: ["tf", filePath], env: [], root: true)
	}
	
	class func mkdir(_ filePath: String) {
		spawn(command: helperPath, args: ["td", filePath], env: [], root: true)
	}
	
	class func ln(_ filePath: String, _ fileDest: String) {
		spawn(command: helperPath, args: ["ts", filePath, fileDest], env: [], root: true)
	}
	
	class func chmod(_ filePath: String, _ perms: Int) {
		spawn(command: helperPath, args: ["ch", filePath, String(perms)], env: [], root: true)
	}
    
    class func mvtemp(_ filePath: String) {
        self.mv(tempPath, filePath)
        self.rm(tempPath)
    }
}

#if compiler(>=6.0)
extension String: @retroactive Error { }
#else
extension String: Error { }
#endif

func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
```


## SpareViews.swift — 425~470행 (sw_vers / tvOS 열거형)

```swift
    }
}
//taken from Hemlock

enum tvOS: Comparable {
    case yager
    case archer
    case satellite
    case sydney
    case dawn
}
var sw_vers: tvOS = .yager

func randomString(length: Int) -> String {
    let letters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    return String((0..<length).map{ _ in letters.randomElement()! })
}

struct ContextMenuButtonTV: View {
    @State var stringKey: String
    let action: () -> Void
    
    var body: some View {
        Button(action: {
            action()
        }) {
            Text(LocalizedString(stringKey))
                .frame(width: buttonWidth, height: buttonHeight)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
                .if(sw_vers <= .archer) { view in
                    view.font(.system(size: 40))
                }
        }
        .padding(paddingInt)
        .opacity(opacityInt)
    }
}

struct NotJailbrokenView: View {
    var body: some View {
        Text(localizedString: "NOT_JB")
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 80)
            }
```


## SpartanApp.swift — 60~116행 (앱 시작 시 .Trash 생성, UserDefaults suite 정의)

```swift
        } else if #available(tvOS 16.0, *) {
            sw_vers = .sydney
        } else if #available(tvOS 15.0, *) {
            sw_vers = .satellite
        } else if #available(tvOS 14.0, *) {
            sw_vers = .archer
        }
        
        #if TROLLSTORE
            isJB = false
            helperPath = Bundle.main.path(forResource: "RootHelper", ofType: nil)
            print("We are running on Trollstore...")
        #else
            print("We are jailbroken!")
        #endif
        
        if(!(FileManager.default.fileExists(atPath: "/private/var/mobile/Media/.Trash"))){
            RootHelperActs.mkdir("/private/var/mobile/Media/.Trash")
        } else {
            print("Trash already exists")
        }
        
		if(FileManager.default.isReadableFile(atPath: "/private/var/mobile/")){ //shows app data directory if sandbox exists
            //displayView(pathToLoad: "/private/var/mobile/Documents/")
            //displayView(pathToLoad: Bundle.main.bundlePath)
            //displayView(pathToLoad: "/private/var/containers/Bundle/Application/2A65A51A-4061-4143-B622-FA0E57C0C3EE/trillstore.app/")
            displayView(pathToLoad: "/private/var/mobile/")
        } else {
            displayView(pathToLoad: "/Developer/")
        }
        
        return true
    }
    
    func displayView(pathToLoad: String) {
        let isRootless = {
			if(FileManager.default.fileExists(atPath: "/private/var/jb/")) { //rootless tvos :woeis:
                return true
            }
            return false
        }()
        let hostingController = UIHostingController(rootView: ContentView(directory: pathToLoad, isRootless: isRootless, scaleFactor: UIScreen.main.nativeBounds.height/1080, globalAVPlayer: $player /*, webServer: $server*/))
            window = UIWindow(frame: UIScreen.main.bounds)
            window?.rootViewController = hostingController
            window?.makeKeyAndVisible()
    }
}

extension UserDefaults {
    static var favorites: UserDefaults {
        return UserDefaults(suiteName: "com.whitetailani.Spartan.favorites") ?? UserDefaults.standard
    }
    static var settings: UserDefaults {
        return UserDefaults(suiteName: "com.whitetailani.Spartan.settings") ?? UserDefaults.standard
    }
    // so it turns out the text editor did NOT need its own userdefaults. i never used it. no idea why i created it.
}
```


## ButtonActions/FavoritesView.swift — 전체 (2단계에서 교체 대상)

```swift
//
//  FavoritesView.swift
//  Spartan
//
//  Created by RealKGB on 4/6/23.
//

import SwiftUI

struct FavoritesView: View {

    @State var favoritesDisplayName: [String] = (UserDefaults.favorites.stringArray(forKey: "favoritesDisplayName") ?? ["Documents", "Applications", "UserApplications", "Trash"])
    @State var favoritesFilePath: [String] = (UserDefaults.favorites.stringArray(forKey: "favoritesFilePath") ?? ["/private/var/mobile/Documents/", "/Applications/", "/private/var/containers/Bundle/Application/", "/private/var/mobile/Media/.Trash/"]) //change to app bundle, app data, and group app data if jailed
    
    @Binding var directory: String
    @Binding var showView: Bool
    @State private var addToFavoritesShow = false
    @State private var blank: String = ""

    var body: some View {
        Text(NSLocalizedString("FAVORITES_TITLE", comment: "- Catches that little strand of honey"))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
        
        List(favoritesDisplayName.indices, id: \.self) { index in
            Button(action: {
                directory = favoritesFilePath[index]
                showView = false
            }) {
                HStack {
                    if(favoritesDisplayName.firstIndex(of: favoritesDisplayName[index])! < 4){
                        switch favoritesDisplayName.firstIndex(of: favoritesDisplayName[index])! {
                        case 0:
                            Image(systemName: "doc.text")
                        case 1:
                            Image(systemName: "applescript")
                        case 2:
                            Image(systemName: "person.crop.circle")
                        case 3:
                            Image(systemName: "trash")
                        case 4:
                            Image(systemName: "questionmark.square.dashed")
                        default:
                            Image(systemName: "questionmark")
                        }
                    }
                    Text(favoritesDisplayName[index])
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                        }
                }
            }
        }
        
        Button(action: {
            addToFavoritesShow = true
        }) {
            Text(NSLocalizedString("FAVORITESADD", comment: "that hangs after you pour it."))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
        }
        .sheet(isPresented: $addToFavoritesShow){
            AddToFavoritesView(filePath: directory, displayName: URL(fileURLWithPath: directory).lastPathComponent, showView: $addToFavoritesShow)
        }
    }
}

```


## ButtonActions/AddToFavoritesView.swift — 전체 (저장 버그가 있는 파일)

```swift
//
//  AddToFavoritesView.swift
//  Spartan
//
//  Created by RealKGB on 4/7/23.
//  The majority of this document is just strings. All it does is take the [String] containing your favorited files and add a value. Of course since I'm using a String for filepath I have to do cleanup on it... but I'm used to that.
//

import SwiftUI

struct AddToFavoritesView: View {
    
    @State var filePath: String
    @State var displayName: String
    @State var favoritesDisplayName: [String] = (UserDefaults.favorites.stringArray(forKey: "favoritesDisplayName") ?? ["Documents", "Applications", "UserApplications", "Trash"])
    @State var favoritesFilePath: [String] = (UserDefaults.favorites.stringArray(forKey: "favoritesFilePath") ?? ["/private/var/mobile/Documents/", "/Applications/", "/private/var/containers/Bundle/Application/", "/private/var/mobile/Media/.Trash/"])
    @Binding var showView: Bool

    var body: some View {
        Text(NSLocalizedString("FAVORITESADD", comment: "Saves us millions."))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
        TextField(NSLocalizedString("FAVORITESADD_DISPLAYNAME", comment: "Can anyone work on the Krelman?"), text: $displayName)
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
        TextField(NSLocalizedString("FAVORITESADD_FILEPATH", comment: "Of course. Most bee jobs are small ones."), text: $filePath, onEditingChanged: { (isEditing) in
            if !isEditing {
                if(!(filePath.hasSuffix("/")) && UserDefaults.settings.bool(forKey: "autoComplete")){
                    filePath = filePath + "/"
                }
            }
        })
        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
        }
        Button(action: {
            favoritesDisplayName.append(displayName)
            favoritesFilePath.append(filePath)
            UserDefaults.favorites.set(["Documents", "Applications", "UserApplications", "Trash"], forKey: "favoritesDisplayName")
            UserDefaults.favorites.set(["/private/var/mobile/Documents/", "/Applications/", "/private/var/containers/Bundle/Application/", "/private/var/mobile/Media/.Trash/"], forKey: "favoritesFilePath")
            UserDefaults.favorites.synchronize()
            showView = false
        }) {
            Text(NSLocalizedString("CONFIRM", comment: "But bees know that every small job, if it's done well, means a lot."))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
        }
    }
}

```


## WebDAV/WebDAVView.swift — 전체 (tvOS 툴바 / editMode / onExitCommand / 단일 sheet 패턴의 참고 구현)

```swift
//
//  WebDAVView.swift
//  Argo
//
//  Remote browsing over WebDAV: connection list, connection editor and the
//  remote file browser (download, view, install IPAs through TrollStore).
//

import SwiftUI
import AVKit

// MARK: - Root

struct WebDAVRootView: View {

    @Binding var directory: String
    @Binding var isPresented: Bool

    @ObservedObject private var store = WebDAVStore.shared

    /// 0 = connection list, 1 = connection editor, 2 = remote browser
    @State private var screen = 0
    @State private var workingServer = WebDAVServer()
    @State private var isNewServer = false
    /// When on, selecting a connection opens its settings instead of browsing it.
    @State private var editMode = false

    var body: some View {
        Group {
            if screen == 2 {
                WebDAVBrowserView(server: workingServer,
                                  directory: $directory,
                                  isPresented: $isPresented,
                                  onBack: { screen = 0 })
            } else if screen == 1 {
                WebDAVServerEditView(server: $workingServer,
                                     isNew: isNewServer,
                                     onSave: {
                                         store.upsert(workingServer)
                                         screen = 0
                                         editMode = false
                                     },
                                     onDelete: {
                                         store.remove(workingServer)
                                         screen = 0
                                         editMode = false
                                     },
                                     onCancel: {
                                         screen = 0
                                         editMode = false
                                     })
            } else {
                connectionList
            }
        }
    }

    private var connectionList: some View {
        VStack {
            Text("WebDAV")
                .font(.title)

            if store.servers.isEmpty {
                Text("No saved connections yet.")
                    .foregroundColor(.secondary)
                    .padding(.top, 20)
            } else {
                // One button per row: tvOS focuses a whole row at once, so a row
                // holding two buttons would always fire the trailing one.
                List(store.servers) { server in
                    Button(action: {
                        workingServer = server
                        if editMode {
                            isNewServer = false
                            screen = 1
                        } else {
                            screen = 2
                        }
                    }) {
                        HStack {
                            Image(systemName: editMode ? "gear" : "globe")
                            VStack(alignment: .leading) {
                                Text(server.displayName)
                                Text(server.urlString)
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            }

            Text(editMode
                 ? "Edit mode: choose a connection to change its settings."
                 : "Choose a connection to browse it.")
                .font(.caption)
                .foregroundColor(.secondary)

            HStack {
                Button(action: {
                    workingServer = WebDAVServer()
                    isNewServer = true
                    screen = 1
                }) {
                    HStack {
                        Image(systemName: "plus")
                        Text("Add connection")
                    }
                }

                if !store.servers.isEmpty {
                    Button(action: { editMode.toggle() }) {
                        HStack {
                            Image(systemName: "gear")
                            Text(editMode ? "Done" : "Edit connections")
                        }
                    }
                }

                Button(action: {
                    directory = WebDAVCache.directory + "/"
                    isPresented = false
                }) {
                    HStack {
                        Image(systemName: "folder")
                        Text("Open download folder")
                    }
                }
            }

            Text("Downloads are staged in \(WebDAVCache.directory) - \(ByteCountFormatter.string(fromByteCount: WebDAVCache.usedBytes, countStyle: .file)) used")
                .font(.caption)
                .foregroundColor(.secondary)
            Text(IPAInstaller.routeDescription)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }
}

// MARK: - Connection editor

struct WebDAVServerEditView: View {

    @Binding var server: WebDAVServer
    var isNew: Bool
    var onSave: () -> Void
    var onDelete: () -> Void
    var onCancel: () -> Void

    @State private var status = ""
    @State private var testing = false

    var body: some View {
        VStack(alignment: .leading) {
            Text(isNew ? "New connection" : "Edit connection")
                .font(.title)

            TextField("Name (optional)", text: $server.name)
            TextField("https://host:port/path", text: $server.urlString)
            TextField("Username", text: $server.username)
            SecureField("Password", text: $server.password)
            Toggle("Allow untrusted certificates", isOn: $server.allowInsecureTLS)

            if !status.isEmpty {
                Text(status)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack {
                Button(action: test) {
                    Text(testing ? "Testing..." : "Test")
                }
                Button(action: {
                    if server.isValid {
                        onSave()
                    } else {
                        status = "Enter a valid http:// or https:// URL first."
                    }
                }) {
                    Text("Save")
                }
                Button(action: onCancel) {
                    Text("Cancel")
                }
                if !isNew {
                    Button(action: onDelete) {
                        Text("Delete")
                            .foregroundColor(.red)
                    }
                }
            }
        }
        .padding()
    }

    private func test() {
        guard server.isValid else {
            status = "Enter a valid http:// or https:// URL first."
            return
        }
        testing = true
        status = ""
        let client = WebDAVClient(server: server)
        client.testConnection { result in
            testing = false
            switch result {
            case .success:
                status = "Connected."
            case .failure(let error):
                status = error.localizedDescription
            }
            _ = client // keep the client alive until the callback runs
        }
    }
}

// MARK: - Remote browser

struct WebDAVBrowserView: View {

    let server: WebDAVServer
    @Binding var directory: String
    @Binding var isPresented: Bool
    var onBack: () -> Void

    @State private var client: WebDAVClient
    @State private var path: String
    @State private var items: [WebDAVItem] = []
    @State private var loading = false
    @State private var message = ""

    /// Where "copy" puts things: whatever folder the local browser is sitting on.
    @State private var destination: String

    @State private var detailItem: WebDAVItem?
    @State private var multiSelect = false
    @State private var picked: Set<String> = []

    @State private var transferring = false
    @State private var progress: Double = 0
    @State private var transferLabel = ""

    // Viewers. SwiftUI on tvOS only honours one .sheet per view, so a single
    // sheet switches on viewerKind instead of stacking five modifiers.
    @State private var localPath = ""
    @State private var localName = ""
    @State private var showViewer = false
    @State private var viewerKind = 0   // 0 hex, 1 audio, 2 video, 3 image, 4 text
    @State private var isSVG = false
    @State private var player = AVPlayer()

    init(server: WebDAVServer,
         directory: Binding<String>,
         isPresented: Binding<Bool>,
         onBack: @escaping () -> Void) {
        self.server = server
        self._directory = directory
        self._isPresented = isPresented
        self.onBack = onBack
        self._client = State(initialValue: WebDAVClient(server: server))
        self._path = State(initialValue: server.rootPath)
        self._destination = State(initialValue: directory.wrappedValue)
    }

    private var atRoot: Bool {
        WebDAVClient.normalize(path) == WebDAVClient.normalize(server.rootPath)
    }

    var body: some View {
        VStack {
            header

            if transferring {
                Text("\(transferLabel)  \(Int(progress * 100))%")
                    .font(.headline)
            } else if loading {
                Text("Loading...")
                    .foregroundColor(.secondary)
            }

            if !message.isEmpty {
                Text(message)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            if detailItem != nil {
                detailPanel
            } else {
                listPanel
            }
        }
        .onAppear { reload() }
        // The remote's Menu button walks back up the tree, the way it does in
        // the local browser, instead of closing the whole sheet.
        .onExitCommand { goBack() }
        .sheet(isPresented: $showViewer) {
            viewerContent
        }
    }

    private var viewerContent: some View {
        Group {
            if viewerKind == 4 {
                TextView(filePath: localPath, fileName: localName, isPresented: $showViewer)
            } else if viewerKind == 3 {
                ImageView(imagePath: localPath, imageName: localName, isSVG: isSVG)
            } else if viewerKind == 2 {
                VideoPlayerView(videoPath: $localPath, videoName: $localName,
                                isPresented: $showViewer, player: player)
            } else if viewerKind == 1 {
                AudioPlayerView(callback: false, audioPath: $localPath, audioName: $localName,
                                player: player, isPresented: $showViewer)
            } else {
                HexView(filePath: localPath, fileName: localName, isPresented: $showViewer)
            }
        }
    }

    // MARK: Header

    private var header: some View {
        VStack {
            Text(path)
                .font(.headline)
            Text("\(server.displayName)   copy to \(destination)")
                .font(.caption)
                .foregroundColor(.secondary)

            HStack {
                Button(action: { goBack() }) {
                    Image(systemName: "chevron.left")
                        .frame(width: 50, height: 50)
                }

                Button(action: { reload() }) {
                    Image(systemName: "arrow.clockwise")
                        .frame(width: 50, height: 50)
                }

                Button(action: {
                    if multiSelect {
                        multiSelect = false
                        picked.removeAll()
                    } else {
                        detailItem = nil
                        multiSelect = true
                    }
                }) {
                    Image(systemName: multiSelect ? "checkmark.circle.fill" : "checkmark.circle")
                        .frame(width: 50, height: 50)
                }

                if multiSelect {
                    Button(action: {
                        let files = items.filter { !$0.isDirectory }
                        if picked.count == files.count {
                            picked.removeAll()
                        } else {
                            picked = Set(files.map { $0.path })
                        }
                    }) {
                        Image(systemName: "square.stack")
                            .frame(width: 50, height: 50)
                    }
                }

                Button(action: { copyPicked() }) {
                    ZStack {
                        Image(systemName: "doc.on.doc")
                            .frame(width: 50, height: 50)
                        Image(systemName: "arrow.down")
                            .resizable()
                            .frame(width: 15, height: 13)
                            .offset(x: -4, y: 11.75)
                    }
                }
                .disabled(transferring || picked.isEmpty)

                Button(action: {
                    directory = destination.hasSuffix("/") ? destination : destination + "/"
                    isPresented = false
                }) {
                    Image(systemName: "folder")
                        .frame(width: 50, height: 50)
                }

                Button(action: { onBack() }) {
                    Image(systemName: "globe")
                        .frame(width: 50, height: 50)
                }

                Button(action: { isPresented = false }) {
                    Image(systemName: "xmark")
                        .frame(width: 50, height: 50)
                }
            }
        }
    }

    // MARK: Listing

    private var listPanel: some View {
        List(items) { item in
            Button(action: { open(item) }) {
                HStack {
                    if multiSelect && !item.isDirectory {
                        Image(systemName: picked.contains(item.path) ? "checkmark.circle.fill" : "circle")
                    }
                    Image(systemName: icon(for: item))
                    VStack(alignment: .leading) {
                        Text(item.name)
                        if !item.isDirectory {
                            Text("\(item.sizeDescription)   \(item.modifiedDescription)")
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                }
            }
        }
    }

    private func icon(for item: WebDAVItem) -> String {
        if item.isDirectory { return "folder" }
        if item.isIPA { return "square.and.arrow.down" }
        return "doc"
    }

    // MARK: Detail

    private var detailPanel: some View {
        VStack(alignment: .leading) {
            if let item = detailItem {
                Text(item.name)
                    .font(.headline)
                Text("\(item.sizeDescription)   \(item.modifiedDescription)")
                    .font(.caption)
                    .foregroundColor(.secondary)

                if item.isIPA {
                    Button(action: { downloadThenInstall(item) }) {
                        HStack {
                            Image(systemName: "square.and.arrow.down")
                            Text("Install with TrollStore")
                        }
                    }
                    .disabled(transferring)
                }

                Button(action: { downloadThenView(item) }) {
                    HStack {
                        Image(systemName: "eye")
                        Text("View")
                    }
                }
                .disabled(transferring)

                Button(action: { copyOne(item) }) {
                    HStack {
                        Image(systemName: "doc.on.doc")
                        Text("Copy to \(destination)")
                    }
                }
                .disabled(transferring)

                Button(action: { detailItem = nil }) {
                    Text("Back")
                }
            }
        }
        .padding()
    }

    // MARK: Navigation

    private func goBack() {
        if detailItem != nil {
            detailItem = nil
        } else if multiSelect {
            multiSelect = false
            picked.removeAll()
        } else if !atRoot {
            path = WebDAVClient.parent(of: path)
            reload()
        } else {
            onBack()
        }
    }

    private func open(_ item: WebDAVItem) {
        if item.isDirectory {
            path = item.path
            picked.removeAll()
            reload()
        } else if multiSelect {
            if picked.contains(item.path) {
                picked.remove(item.path)
            } else {
                picked.insert(item.path)
            }
        } else {
            detailItem = item
        }
    }

    private func reload() {
        loading = true
        message = ""
        client.list(path: path) { result in
            loading = false
            switch result {
            case .success(let listed):
                items = listed
                if listed.isEmpty { message = "This folder is empty." }
            case .failure(let error):
                items = []
                message = error.localizedDescription
            }
        }
    }

    // MARK: Transfers

    private func localTarget(for item: WebDAVItem) -> String {
        return (destination as NSString).appendingPathComponent(item.name)
    }

    private func download(_ item: WebDAVItem, to target: String,
                          label: String, completion: @escaping (String?) -> Void) {
        transferring = true
        progress = 0
        transferLabel = label
        message = ""
        client.download(item: item, to: target, progress: { fraction in
            progress = fraction
        }, completion: { result in
            switch result {
            case .success(let saved):
                completion(saved)
            case .failure(let error):
                transferring = false
                message = error.localizedDescription
                completion(nil)
            }
        })
    }

    private func copyOne(_ item: WebDAVItem) {
        download(item, to: localTarget(for: item), label: "Copying \(item.name)") { saved in
            transferring = false
            if let saved = saved { message = "Copied to \(saved)" }
        }
    }

    private func copyPicked() {
        let queue = items.filter { picked.contains($0.path) && !$0.isDirectory }
        guard !queue.isEmpty else {
            message = "Select some files first."
            return
        }
        copyStep(queue, 0)
    }

    private func copyStep(_ queue: [WebDAVItem], _ index: Int) {
        if index >= queue.count {
            transferring = false
            message = "Copied \(queue.count) file(s) to \(destination)"
            picked.removeAll()
            multiSelect = false
            return
        }
        let item = queue[index]
        download(item, to: localTarget(for: item),
                 label: "Copying \(item.name)  (\(index + 1)/\(queue.count))") { saved in
            if saved == nil { return }   // download() already reported the failure
            copyStep(queue, index + 1)
        }
    }

    private func downloadThenView(_ item: WebDAVItem) {
        download(item, to: WebDAVCache.localPath(for: item),
                 label: "Opening \(item.name)") { local in
            transferring = false
            guard let local = local else { return }
            localPath = local
            localName = item.name
            present(local)
        }
    }

    private func present(_ local: String) {
        let type = FileInfo.yandereDevFileType(file: local)
        isSVG = false
        if type == 1 {
            viewerKind = 1
        } else if type == 2 {
            viewerKind = 2
        } else if type == 3 {
            viewerKind = 3
        } else if type == 12 {
            isSVG = true
            viewerKind = 3
        } else if type == 4 {
            viewerKind = 4
        } else {
            viewerKind = 0
        }
        showViewer = true
    }

    private func downloadThenInstall(_ item: WebDAVItem) {
        download(item, to: WebDAVCache.localPath(for: item),
                 label: "Downloading \(item.name)") { local in
            guard let local = local else { return }
            transferring = true
            transferLabel = "Installing \(item.name)"
            progress = 1
            IPAInstaller.install(localPath: local,
                                 remoteFallbackURL: client.url(forPath: item.path)) { result in
                transferring = false
                switch result {
                case .success(let log):
                    message = "Installed. \(log)"
                case .handedOff:
                    message = "Handed the IPA to TrollStore."
                case .failure(let error):
                    message = error
                }
            }
        }
    }
}

```


## WebDAV/WebDAVStore.swift — 전체 (ObservableObject + UserDefaults JSON 저장 패턴)

```swift
//
//  WebDAVStore.swift
//  Argo
//
//  Persistence for saved WebDAV connections.
//

import Foundation
import Combine

final class WebDAVStore: ObservableObject {

    static let shared = WebDAVStore()

    private static let key = "servers"

    @Published var servers: [WebDAVServer] = [] {
        didSet { save() }
    }

    private init() {
        load()
    }

    private func load() {
        guard let data = UserDefaults.webdav.data(forKey: WebDAVStore.key) else { return }
        if let decoded = try? JSONDecoder().decode([WebDAVServer].self, from: data) {
            // Assigning here would re-enter save(); that is harmless but wasteful,
            // so write the backing value directly.
            servers = decoded
        }
    }

    private func save() {
        guard let data = try? JSONEncoder().encode(servers) else { return }
        UserDefaults.webdav.set(data, forKey: WebDAVStore.key)
        UserDefaults.webdav.synchronize()
    }

    func upsert(_ server: WebDAVServer) {
        if let index = servers.firstIndex(where: { $0.id == server.id }) {
            servers[index] = server
        } else {
            servers.append(server)
        }
    }

    func remove(_ server: WebDAVServer) {
        servers.removeAll { $0.id == server.id }
    }
}

```


## FileInfo.swift — 1~60행 (yandereDevFileType: 3단계 PDF 판정 추가 위치)

```swift
//
//  FileTypes.swift
//  Spartan
//
//  Created by WhitetailAni on 11/20/23.
//

import Foundation
import AVFoundation
import libarchiveBridge
import Zip

class FileInfo {
    class func yandereDevFileType(file: String) -> Double { //I tried using unified file types but they all returned nil so I have to use this awful yandere dev shit
        //im sorry
        
        //FUTURE ME WANTS YOU TO KNOW I AM TALKING ABOUT THE IF ELSE STACK POST NOT THE PEDO STUFF
        
        //also apparently i look like yandere dev. thanks ethan
        //i tried to update this to be faster using async dispatch, but it was like 50 times slower and locked up half the time. you can see the attempt below, maybe someday i'll fix it
        
        if (isSymlink(filePath: file)) {
            return 8 //symlink
        } else if (isDirectory(filePath: file)) {
            return 0 //directory
        } else if (isVideo(filePath: file)) { //video has to come first as otherwise video files detect as audio (since they are audio files as well)
            return 2 //video file
        } else if (isAudio(filePath: file)) {
            return 1 //audio file
        } else if (isImage(filePath: file)) {
            return 3 //image
        } else if (isPlist(filePath: file) != 0) {
            return isPlist(filePath: file)
            //5.1 = xml plist
            //5.9 = bplist
        } else if (isHTML(filePath: file)) {
            return 14 //html
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".dmg"])) {
            return 13 //dmg
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".svg"])) {
            return 12 //svg
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".ttf", ".otf", ".ttc", ".pfb", ".pfa"])) {
            return 11 //a font (badly)
        } else if(isCar(filePath: file)) {
            return 10 //asset catalog
        } else if (fileManager.isExecutableFile(atPath: file)) { //executables detect as utf32 lol
            return 7 //executable
        } else if (isArchive(filePath: file)) {
            return 6 //archive
        } else if (isText(filePath: file)) { //these must be flipped because otherwise xml plist detects as text
            return 4 //text file
        } else if (doesFileHaveFileExtension(filePath: file, extensions: [".deb"])) {
            return 9 //deb
        } else {
            return 69 //unknown
        }
    }
    
    class func yandereDevFileType2(file: String) -> Double {
        //this is my attempt to make filetypes faster.
```


## 전체 파일이 더 필요할 때

저장소가 공개되어 있으므로 아래 주소에서 원본 전체를 받을 수 있습니다.

| 파일 | raw 주소 |
|---|---|
| ContentView.swift | https://raw.githubusercontent.com/sojubada/argo/main/Spartan/ContentView.swift |
| SpareViews.swift | https://raw.githubusercontent.com/sojubada/argo/main/Spartan/SpareViews.swift |
| SpartanApp.swift | https://raw.githubusercontent.com/sojubada/argo/main/Spartan/SpartanApp.swift |
| FileInfo.swift | https://raw.githubusercontent.com/sojubada/argo/main/Spartan/FileInfo.swift |
| WebDAVView.swift | https://raw.githubusercontent.com/sojubada/argo/main/Spartan/WebDAV/WebDAVView.swift |
| 저장소 전체 | https://github.com/sojubada/argo |

다만 **치환 블록의 "찾을 내용"은 이 문서의 발췌에서 복사**하십시오.
웹에서 받아온 내용을 중간에 옮겨 적는 과정에서 공백이 바뀌면 치환이 실패합니다.

## 치환 블록을 안전하게 만드는 요령

1. 아래 발췌에서 바꿀 구간을 **그대로 복사**
2. 앞뒤로 2~3줄을 더 포함시켜 파일 안에서 유일해지게 만들 것
   (예: `RootHelperActs.mv(` 한 줄만 쓰면 파일 안 여러 곳과 겹칩니다)
3. 들여쓰기를 손대지 말 것
4. 한글 주석이 들어간 줄은 되도록 "찾을 내용"에 넣지 말 것

치환이 애매하다고 판단되면, 억지로 만들지 말고
"이 구간은 치환 블록을 만들기 어렵습니다. 아래 내용으로 교체해 주십시오"라고
적고 바꿀 코드만 제시하십시오. 검수자가 직접 적용합니다.
