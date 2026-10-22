# 1단계 참고 코드

제미나이가 요청한 현재 `main` 소스입니다. **글자 그대로 복사하십시오** — `ContentView.swift` 는 공백과 탭이 섞여 있습니다.

경로를 하나 정정합니다. 설정 화면은 `Spartan/SettingsView.swift` 가 아니라 **`Spartan/ButtonActions/SettingsView.swift`** 입니다.

---

## 0. 먼저 — 쓰기는 거의 다 한 곳을 지나갑니다

열 개 파일에 검사를 흩뿌릴 필요가 없습니다. **`RootHelperActs` 가 root 권한 쓰기의 단일 관문입니다.** `SpareViews.swift:334` 에 있고, 메서드 여덟 개가 전부 `spawn(... root: true)` 를 부릅니다.

```swift
class RootHelperActs {
	class func rm(_ filePath: String)
	class func mv(_ filePath: String, _ fileDest: String)
	class func cp(_ filePath: String, _ fileDest: String)
	class func touch(_ filePath: String)
	class func mkdir(_ filePath: String)
	class func ln(_ filePath: String, _ fileDest: String)
	class func chmod(_ filePath: String, _ perms: Int)
	class func mvtemp(_ filePath: String)      // mv(tempPath, filePath) + rm(tempPath)
}
```

**→ 여기에 `FileGuard` 를 걸면 root 경로의 모든 파괴적 작업이 한 번에 막힙니다.**

다만 **root 경로만으로는 부족합니다.** 샌드박스 권한으로도 쓰는 곳이 있습니다. 전수 조사한 목록입니다.

### root 헬퍼 경유 (`RootHelperActs`) — 여기만 막으면 되는 것들

| 파일 | 줄 | 동작 |
|---|---|---|
| `ContentView.swift` | 508, 859, 1155, 1174 | `rm` |
| `ContentView.swift` | 1479 | `chmod` |
| `ContentView.swift` | 1805, 1840 | `mvtemp` |
| `ButtonActions/CreateFileView.swift` | 27 | `touch` |
| `ButtonActions/CreateDirectoryView.swift` | 27 | `mkdir` |
| `ButtonActions/CreateSymlinkView.swift` | 39 | `ln` |
| `ButtonActions/RenameFileView.swift` | 26 | `mv` |
| `ButtonActions/MoveFileView.swift` | 46, 49, 52, 54 | `mv` |
| `ButtonActions/SettingsView.swift` | 342 | `mvtemp` |
| `Trash/TrashManager.swift` | 74, 84, 87, 109, 123, 184 | `mkdir` `rm` `mv` `cp` `mvtemp` |
| `SpartanApp.swift` | 77 | `mkdir` (휴지통 폴더 생성 — 예외 처리 필요) |
| `FileOpeners/HexView.swift` | 114, 115 | `rm` `mvtemp` |
| `FileOpeners/TextView.swift` | 89 | `rm` |

### root 헬퍼를 안 거치는 쓰기 — 따로 막아야 합니다

| 파일 | 줄 | 동작 |
|---|---|---|
| `Trash/TrashManager.swift` | 72, 82, 103, 117 | `FileManager` 1차 시도 (실패하면 위의 root 폴백) |
| `FileOpeners/Compression/Compression.swift` | 211, 366 | `Zip.zipFiles` — **압축 만들기** |
| `FileOpeners/Compression/Compression.swift` | 250 | `Zip.unzipFile` — **압축 풀기, 덮어쓰기 켜짐** |
| `FileOpeners/Compression/Compression.swift` | 536 | `createDirectory` |
| `FileOpeners/DMGMountView.swift` | 53 | `createDirectory` |
| `FileOpeners/HexView.swift` | 113, 117 | `Data.write` — 파일 직접 덮어쓰기 |
| `FileOpeners/TextView.swift` | 88, 92 | 텍스트 직접 덮어쓰기 |
| `ContentView.swift` | 1801, 1839 | `Data.write` (임시 경로) |
| `ButtonActions/SettingsView.swift` | 338 | `Data.write` (임시 경로) |

`Compression.swift:250` 의 **압축 풀기는 특히 위험합니다.** `overwrite: true` 이고, zip 안의 경로가 `../../` 로 올라가면 밖으로 빠져나갑니다 (Zip Slip). 풀기 전에 **대상 경로 하나하나**를 검사해야 합니다.

### 이미 있는 비슷한 것

`SpareViews.swift` 끝부분에 이런 함수가 있습니다.

```swift
func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
	return ((fullPath.count < 19) || (fullPath.substring(toIndex: 19) != "/private/var/mobile"))
}
```

`PathSafety` 가 대체할 조잡한 선행 사례입니다. **앞 19글자를 자르는 방식이라 `/var/mobile` (심볼릭 링크)을 못 잡습니다.** 쓰는 곳을 찾아 같이 정리하십시오.

---
## 1. `Spartan/ButtonActions/SettingsView.swift` (전체 343행)

`showView` 배열은 현재 **5**입니다. 안전 모드 스위치와 작업 기록 보기를 넣으면 키워야 합니다.

```swift
//
//  SettingsView.swift
//  Spartan
//
//  Created by RealKGB on 4/6/23.
//

import SwiftUI

struct SettingsView: View {
    @State private var showView: [Bool] = [Bool](repeating: false, count: 5)
    @State private var descriptiveTitlesPre = UserDefaults.settings.bool(forKey: "descriptiveTitles")
    @State private var descriptiveTimestampsPre = UserDefaults.settings.bool(forKey: "verboseTimestamps")
    @State private var autoCompletePre = UserDefaults.settings.bool(forKey: "autoComplete")
    @State private var logWindowFontSizePre = UserDefaults.settings.integer(forKey: "logWindowFontSize")
    @State private var sheikahFontApplyPre = UserDefaults.settings.bool(forKey: "sheikahFontApply")
    @State private var dateFormatPre: String = UserDefaults.settings.string(forKey: "dateFormat") ?? ""
    
    @State var test = 0.0

    var body: some View {
		ScrollView {
			Text(NSLocalizedString("SETTINGS", comment: "But choose carefully because you'll stay in the job you pick for the rest of your life."))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 60)
				}
				.font(.system(size: 60))
                .sheet(isPresented: $showView[0], content: {
                    CreditsView()
                })
			
			StepperTV(value: $logWindowFontSizePre, isHorizontal: true) {
				UserDefaults.settings.set(logWindowFontSizePre, forKey: "logWindowFontSize")
				UserDefaults.settings.synchronize()
			}
			.padding(5)
			Text(NSLocalizedString("SETTINGS_LOGFONTSIZE", comment: "The same job the rest of your life?"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
				.font(.system(size: 25))
				
			Text(" ")
				
			descriptiveThings
			Text(" ")
			autoCompleteFileExtensions
			Text(" ")
			sheikahFont
			Text(" ")
			
			GeometryReader { geometry in
				HStack {
					Spacer()
					TextField("SETTINGS_DATEFORMAT", text: $dateFormatPre)
						.frame(maxWidth: geometry.size.width * 0.8)
					Spacer()
				}
			}
			Text(" ")
				.font(.system(size: 15))
			Text(NSLocalizedString("SETTINGS_DATEFORMATDESC1", comment: "Halls Of Science 4"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
				.font(.system(size: 25))
			Text(NSLocalizedString("SETTINGS_DATEFORMATDESC2", comment: "Halls Of Science... 5?"))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 25)
				}
				.font(.system(size: 25))
			Text(" ")
            
            /*Button(action: {
                cacheFolder("/")
            }) {
                Text(localizedString: "SETTINGS_CACHEALL")
            }
            Text(NSLocalizedString("SETTINGS_CACHEALLDESC", comment: "Halls Of Science... 5?"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 25)
                }
                .font(.system(size: 25))
            Text(" ")*/
			
			Button(action: {
				showView[2] = true
			}) {
				HStack {
					Image(systemName: "pencil")
					Text(NSLocalizedString("APPICON", comment: """
					LET'S GO BABY LOVE THE [[METS]] HIT A HOME RUN BABY
					1987 *CAN* HAPPEN AGAIN
					"""))
					.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
						view.scaledFont(name: "BotW Sheikah Regular", size: 25)
					}
				}
			}
			
			/*Button(action: {
				showView[1] = true
			}) {
				Text(LocalizedString("SETTINGS_WEBSERVERSTART"))
			}*/
			
			Button(action: { //WebKit probe
				showView[3] = true
			}) {
				HStack {
					Image(systemName: "stethoscope")
						.frame(width: 50, height: 50)
					Text("Lab ① WebKit 탐침 — 확인 완료")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
			}
			.fullScreenCover(isPresented: $showView[3]) {
				WebKitProbeView(isPresented: $showView[3])
					.edgesIgnoringSafeArea(.all)
			}

			Button(action: { //document render probe
				showView[4] = true
			}) {
				HStack {
					Image(systemName: "doc.viewfinder")
						.frame(width: 50, height: 50)
					Text("Lab ② 문서 렌더링 탐침 ← 지금 이것")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
			}
			.fullScreenCover(isPresented: $showView[4]) {
				WebDocProbeView(isPresented: $showView[4])
					.edgesIgnoringSafeArea(.all)
			}

			Button(action: { //info
				showView[0] = true
			}) {
				HStack {
					Image(systemName: "info.circle")
						.frame(width: 50, height: 50)
					Text(NSLocalizedString("CREDITS", comment: """
					"What's the difference?"
					"""))
					.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
						view.scaledFont(name: "BotW Sheikah Regular", size: 40)
					}
				}
			}
			
			Text(" ")
				.font(.system(size: 25))
			Text("Spartan v\((Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "Don't edit") (\((Bundle.main.infoDictionary?["CFBundleVersion"] as? String) ?? "Info.plists"))")
                .foregroundColor(.secondary)
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 20)
				}
				.font(.footnote)
        }
		.sheet(isPresented: $showView[1], content: {
			WebServerView()
		})
		.sheet(isPresented: $showView[2], content: {
			IconView()
		})
    }
    
    @ViewBuilder
    var descriptiveThings: some View {
        Button(action: {
            descriptiveTitlesPre.toggle()
            UserDefaults.settings.set(descriptiveTitlesPre, forKey: "descriptiveTitles")
            UserDefaults.settings.synchronize()
        }) {
            Text(NSLocalizedString("SETTINGS_TITLES", comment: "The same job the rest of your life?"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            Image(systemName: descriptiveTitlesPre ? "checkmark.square" : "square")
        }
        Text(NSLocalizedString("SETTINGS_TITLES_DESC", comment: "I didn't know that."))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 25)
            }
            .font(.system(size: 25))
		
		Text(" ")
        Button(action: {
            descriptiveTimestampsPre.toggle()
            UserDefaults.settings.set(descriptiveTimestampsPre, forKey: "verboseTimestamps")
            UserDefaults.settings.synchronize()
        }) {
            Text(NSLocalizedString("SETTINGS_TIMESTAMPS", comment: "What's the difference?"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            Image(systemName: descriptiveTimestampsPre ? "checkmark.square" : "square")
        }
        Text(NSLocalizedString("SETTINGS_TIMESTAMPS_DESC", comment: "You'll be happy to know that bees, as a species"))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 25)
            }
            .font(.system(size: 25))
    }
    
    @ViewBuilder
    var autoCompleteFileExtensions: some View {
        Button(action: {
            autoCompletePre.toggle()
            UserDefaults.settings.set(autoCompletePre, forKey: "autoComplete")
            UserDefaults.settings.synchronize()
        }) {
            Text(NSLocalizedString("SETTINGS_AUTOCOMPLETE", comment: "haven't had one day off in 27 million years."))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            Image(systemName: autoCompletePre ? "checkmark.square" : "square")
        }
        Text("""
        \(NSLocalizedString("SETTINGS_AUTOCOMPLETE_DESC_1", comment: "So you'll just work us to death?"))
        \(NSLocalizedString("SETTINGS_AUTOCOMPLETE_DESC_2", comment: "We'll sure try."))
        """)
            .multilineTextAlignment(.center)
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 25)
            }
            .font(.system(size: 25))
        Text(NSLocalizedString("SETTINGS_AUTOCOMPLETE_WARNING", comment: "Wow! That blew my mind!"))
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 25)
            }
            .font(.system(size: 25))
    }
    
    @ViewBuilder
    var sheikahFont: some View {
        Button(action: {
            sheikahFontApplyPre.toggle()
            UserDefaults.settings.set(sheikahFontApplyPre, forKey: "sheikahFontApply")
            UserDefaults.settings.synchronize()
            if(sheikahFontApplyPre) {
                buttonWidth *= 1.5
            } else {
                buttonWidth /= 1.5
            }
        }) {
            Text(NSLocalizedString("SETTINGS_SHEIKAH", comment: "YOU LOST IT WHEN YOU TRIED TO SEE TOO FAR"))
                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                }
            Image(systemName: sheikahFontApplyPre ? "checkmark.square" : "square")
        }
    }
}

struct IconView: View {
	var body: some View {
		VStack {
			Text(NSLocalizedString("APPICON_TITLE", comment: "But choose carefully because you'll stay in the job you pick for the rest of your life."))
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 60)
				}
				.font(.system(size: 60))
			VStack {
				HStack {
					IconButton(iconName: "Alpha", creator: "WhitetailAni")
					IconButton(iconName: "Megamind", creator: "WhitetailAni")
					IconButton(iconName: "Terabyte", creator: "WhitetailAni")
				}
				HStack {
                    IconButton(iconName: "Beta", creator: "WhitetailAni")
					IconButton(iconName: "Summit", creator: "Noel Berry? Unsure")
					IconButton(iconName: "Finda", creator: "JamesnLollify")
				}
			}
		}
	}
}

struct IconButton: View {
	@State var iconName: String
	@State var creator: String

	var body: some View {
		Button(action: {
			UIApplication.shared.setAlternateIconName(iconName) { error in
				if let error = error {
					print(error.localizedDescription)
				}
			}
		}) {
			VStack {
				Image(uiImage: UIImage(named: iconName)!)
					.resizable()
					.cornerRadius(10)
					.frame(width: 300, height: 180)
				Text(iconName)
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 30)
						}
				Text(creator)
					.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
						view.scaledFont(name: "BotW Sheikah Regular", size: 25).foregroundColor(.gray)
					}
					.foregroundColor(.gray)
			}
			.frame(width: 350, height: 300)
		}
	}
}

func cacheFolder(_ directory: String) {
    var masterFiles: [SpartanFile] = []
    do {
        let contents = try FileManager.default.contentsOfDirectory(atPath: directory)
        var files: [String]
        files = contents.map { file in
            let filePath = "/" + directory + "/" + file
            var isDirectory: ObjCBool = false
            FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory)
            return isDirectory.boolValue ? "\(file)/" : file
        }
        files.remove(at: files.firstIndex(of: metadataName!)!) //hide the metadata file from view
        for i in 0..<files.count {
            masterFiles.append(SpartanFile(name: files[i], fullPath: directory + files[i], isSelected: false, fileType: FileInfo.yandereDevFileType(file: directory + files[i]), isLoadingFile: false))
        }
    } catch {
        print(error)
    }
    let encoder = JSONEncoder()
    do {
        let encoded = try encoder.encode(masterFiles)
        try encoded.write(to: URL(fileURLWithPath: tempPath))
    } catch {
        print("failed to save cached metadata: \(error)")
    }
    RootHelperActs.mvtemp(directory + metadataName!)
}
```

---

## 2. `Spartan/Trash/TrashManager.swift` (전체 443행)

`FileManager` 를 먼저 시도하고 실패하면 `RootHelperActs` 로 넘어갑니다. **두 경로 다 막아야 합니다.**

```swift
//
//  TrashManager.swift
//  Argo
//

import Foundation
import Combine

struct TrashEntry: Codable, Identifiable, Hashable {
    let id: String
    let originalPath: String
    let originalName: String
    let deletedAt: Date
    let isDirectory: Bool
    let size: Int64

    var storedPath: String {
        if originalPath.isEmpty {
            return "\(TrashManager.trashRootPath)/\(originalName)"
        }
        return "\(TrashManager.itemsRootPath)/\(id)/\(originalName)"
    }

    var isLegacy: Bool {
        return originalPath.isEmpty
    }

    var deletedAtDescription: String {
        let formatter = DateFormatter()
        let format = UserDefaults.settings.string(forKey: "dateFormat") ?? "MM-dd-yyyy HH:mm"
        formatter.dateFormat = format.isEmpty ? "MM-dd-yyyy HH:mm" : format
        return formatter.string(from: deletedAt)
    }
}

enum TrashRestoreResult {
    case restored(String)
    case renamed(String)
    case failed(String)
}

private struct TrashIndexFile: Codable {
    var version: Int = 1
    var entries: [TrashEntry]
}

final class TrashManager: ObservableObject {
    static let shared = TrashManager()

    static let trashRootPath = "/private/var/mobile/Media/.Trash"
    static let indexFilePath = "/private/var/mobile/Media/.Trash/argo-trash-index.json"
    static let itemsRootPath = "/private/var/mobile/Media/.Trash/items"

    @Published private(set) var entries: [TrashEntry] = []

    var totalSize: Int64 {
        return entries.reduce(0) { $0 + $1.size }
    }

    private init() {
        ensureDirectories()
        reload()
    }

    // MARK: - Dual File Operations (FileManager First + RootHelperActs Fallback)

    private func robustMkdir(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return }
        if fileManager.fileExists(atPath: norm) { return }
        do {
            try fileManager.createDirectory(atPath: norm, withIntermediateDirectories: true, attributes: nil)
        } catch {
            RootHelperActs.mkdir(norm)
        }
    }

    private func robustRemove(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty, fileManager.fileExists(atPath: norm) else { return }
        do {
            try fileManager.removeItem(atPath: norm)
        } catch {
            RootHelperActs.rm(norm)
        }
        if fileManager.fileExists(atPath: norm) {
            RootHelperActs.rm(norm)
        }
    }

    private func robustMove(from src: String, to dst: String) -> Bool {
        let normSrc = normalizePath(src)
        let normDst = normalizePath(dst)
        guard !normSrc.isEmpty, !normDst.isEmpty else { return false }
        guard fileManager.fileExists(atPath: normSrc) else { return false }

        if fileManager.fileExists(atPath: normDst) {
            robustRemove(normDst)
        }

        // 1차 시도: FileManager 직접 이동
        do {
            try fileManager.moveItem(atPath: normSrc, toPath: normDst)
            if fileManager.fileExists(atPath: normDst) {
                return true
            }
        } catch {
            // 2차 시도: RootHelperActs.mv
            RootHelperActs.mv(normSrc, normDst)
            if fileManager.fileExists(atPath: normDst) {
                return true
            }
        }

        // 3차 시도: 복사 후 원본 삭제 (다른 볼륨/마운트 간 이동 대응)
        do {
            try fileManager.copyItem(atPath: normSrc, toPath: normDst)
            if fileManager.fileExists(atPath: normDst) {
                robustRemove(normSrc)
                return !fileManager.fileExists(atPath: normSrc)
            }
        } catch {
            RootHelperActs.cp(normSrc, normDst)
            if fileManager.fileExists(atPath: normDst) {
                robustRemove(normSrc)
                return !fileManager.fileExists(atPath: normSrc)
            }
        }

        return fileManager.fileExists(atPath: normDst)
    }

    private func ensureDirectories() {
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
    }

    func reload() {
        ensureDirectories()
        let indexed = loadIndexedEntries()
        let legacy = scanLegacyEntries()
        let combined = indexed + legacy
        entries = combined.sorted { $0.deletedAt > $1.deletedAt }
    }

    private func loadIndexedEntries() -> [TrashEntry] {
        guard fileManager.fileExists(atPath: Self.indexFilePath),
              let data = try? Data(contentsOf: URL(fileURLWithPath: Self.indexFilePath)) else {
            return []
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        guard let indexFile = try? decoder.decode(TrashIndexFile.self, from: data) else {
            return []
        }
        return indexFile.entries.filter { fileManager.fileExists(atPath: $0.storedPath) }
    }

    private func saveIndexedEntries(_ list: [TrashEntry]) {
        let nonLegacy = list.filter { !$0.isLegacy }
        let indexFile = TrashIndexFile(version: 1, entries: nonLegacy)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted]
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(indexFile) else { return }

        // 1차 시도: 직접 쓰기
        do {
            try data.write(to: URL(fileURLWithPath: Self.indexFilePath))
            return
        } catch {
            // 2차 시도: 앱 샌드박스 임시 경로 및 전역 tempPath 경유
            let sandboxTemp = (NSTemporaryDirectory() as NSString).appendingPathComponent("argo-trash-index.tmp")
            if let _ = try? data.write(to: URL(fileURLWithPath: sandboxTemp)) {
                robustRemove(Self.indexFilePath)
                _ = robustMove(from: sandboxTemp, to: Self.indexFilePath)
                robustRemove(sandboxTemp)
                if fileManager.fileExists(atPath: Self.indexFilePath) {
                    return
                }
            }
            if let _ = try? data.write(to: URL(fileURLWithPath: tempPath)) {
                robustRemove(Self.indexFilePath)
                RootHelperActs.mvtemp(Self.indexFilePath)
            }
        }
    }

    private func scanLegacyEntries() -> [TrashEntry] {
        guard let names = try? fileManager.contentsOfDirectory(atPath: Self.trashRootPath) else {
            return []
        }
        var result: [TrashEntry] = []
        for name in names {
            if name == "argo-trash-index.json" || name == "items" || name == "." || name == ".." {
                continue
            }
            let fullPath = "\(Self.trashRootPath)/\(name)"
            var isDir: ObjCBool = false
            guard fileManager.fileExists(atPath: fullPath, isDirectory: &isDir) else { continue }
            let attrs = try? fileManager.attributesOfItem(atPath: fullPath)
            let modDate = (attrs?[.modificationDate] as? Date) ?? Date(timeIntervalSince1970: 0)
            let itemSize = computeSize(atPath: fullPath, isDirectory: isDir.boolValue)
            let entry = TrashEntry(
                id: "legacy-\(name)",
                originalPath: "",
                originalName: name,
                deletedAt: modDate,
                isDirectory: isDir.boolValue,
                size: itemSize
            )
            result.append(entry)
        }
        return result
    }

    @discardableResult
    func moveToTrash(path: String) -> Bool {
        let normalized = normalizePath(path)
        guard !normalized.isEmpty else { return false }

        if normalized == Self.trashRootPath || normalized.hasPrefix(Self.trashRootPath + "/") {
            return false
        }

        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: normalized, isDirectory: &isDir) else {
            return false
        }

        ensureDirectories()

        let uuid = UUID().uuidString
        let originalName = (normalized as NSString).lastPathComponent
        guard !originalName.isEmpty, originalName != "/" else { return false }

        let itemSize = computeSize(atPath: normalized, isDirectory: isDir.boolValue)
        let uuidDir = "\(Self.itemsRootPath)/\(uuid)"
        let destPath = "\(uuidDir)/\(originalName)"

        robustMkdir(uuidDir)

        guard robustMove(from: normalized, to: destPath) else {
            robustRemove(uuidDir)
            return false
        }

        var currentIndexed = loadIndexedEntries()
        let newEntry = TrashEntry(
            id: uuid,
            originalPath: normalized,
            originalName: originalName,
            deletedAt: Date(),
            isDirectory: isDir.boolValue,
            size: itemSize
        )
        currentIndexed.append(newEntry)
        saveIndexedEntries(currentIndexed)
        reload()
        return true
    }

    func moveToTrash(paths: [String]) -> (moved: Int, failed: Int) {
        var movedCount = 0
        var failedCount = 0
        for p in paths {
            if moveToTrash(path: p) {
                movedCount += 1
            } else {
                failedCount += 1
            }
        }
        return (movedCount, failedCount)
    }

    func restore(_ entry: TrashEntry) -> TrashRestoreResult {
        if entry.isLegacy || entry.originalPath.isEmpty {
            return .failed("Unknown original location.")
        }

        let sourcePath = entry.storedPath
        guard fileManager.fileExists(atPath: sourcePath) else {
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return .failed("Item no longer exists in Trash.")
        }

        let targetOriginal = normalizePath(entry.originalPath)
        let parentDir = (targetOriginal as NSString).deletingLastPathComponent
        ensureDirectoryRecursive(parentDir)

        var finalPath = targetOriginal
        var didRename = false

        if fileManager.fileExists(atPath: finalPath) {
            finalPath = makeNonConflictingPath(for: targetOriginal, isDirectory: entry.isDirectory)
            didRename = true
        }

        guard robustMove(from: sourcePath, to: finalPath) else {
            return .failed("Failed to move item to \(finalPath)")
        }

        let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
        robustRemove(uuidDir)

        var currentIndexed = loadIndexedEntries()
        currentIndexed.removeAll { $0.id == entry.id }
        saveIndexedEntries(currentIndexed)
        reload()

        if didRename {
            return .renamed(finalPath)
        } else {
            return .restored(finalPath)
        }
    }

    func deleteForever(_ entry: TrashEntry) -> Bool {
        if entry.isLegacy {
            let target = entry.storedPath
            robustRemove(target)
            let ok = !fileManager.fileExists(atPath: target)
            reload()
            return ok
        } else {
            let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
            robustRemove(uuidDir)
            let ok = !fileManager.fileExists(atPath: uuidDir)
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return ok
        }
    }

    /// 일반 파일 브라우저에서 휴지통을 거치지 않고 즉시 영구 삭제할 때 사용하는 헬퍼
    @discardableResult
    func deletePathDirectly(_ path: String) -> Bool {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return false }
        robustRemove(norm)
        return !fileManager.fileExists(atPath: norm)
    }

    func emptyAll() -> Bool {
        robustRemove(Self.trashRootPath)
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
        saveIndexedEntries([])
        reload()
        return entries.isEmpty
    }

    // MARK: - Helpers

    private func normalizePath(_ path: String) -> String {
        var result = path.trimmingCharacters(in: .whitespacesAndNewlines)
        while result.count > 1 && result.hasSuffix("/") {
            result.removeLast()
        }
        return result
    }

    private func ensureDirectoryRecursive(_ dirPath: String) {
        let normalized = normalizePath(dirPath)
        guard !normalized.isEmpty, normalized != "/" else { return }
        if fileManager.fileExists(atPath: normalized) { return }

        let components = normalized.split(separator: "/").map(String.init)
        var current = ""
        for comp in components {
            current += "/" + comp
            if !fileManager.fileExists(atPath: current) {
                robustMkdir(current)
            }
        }
    }

    private func makeNonConflictingPath(for originalPath: String, isDirectory: Bool) -> String {
        let nsPath = originalPath as NSString
        let parent = nsPath.deletingLastPathComponent
        let fileName = nsPath.lastPathComponent

        let baseName: String
        let ext: String

        if isDirectory {
            baseName = fileName
            ext = ""
        } else {
            let nsFile = fileName as NSString
            ext = nsFile.pathExtension
            baseName = ext.isEmpty ? fileName : nsFile.deletingPathExtension
        }

        var counter = 1
        while true {
            let candidateName: String
            if ext.isEmpty {
                candidateName = "\(baseName) (\(counter))"
            } else {
                candidateName = "\(baseName) (\(counter)).\(ext)"
            }
            let candidatePath = (parent as NSString).appendingPathComponent(candidateName)
            if !fileManager.fileExists(atPath: candidatePath) {
                return candidatePath
            }
            counter += 1
        }
    }

    private func computeSize(atPath path: String, isDirectory: Bool) -> Int64 {
        if !isDirectory {
            let attrs = try? fileManager.attributesOfItem(atPath: path)
            if let num = attrs?[.size] as? NSNumber {
                return num.int64Value
            }
            if let s = attrs?[.size] as? Int64 {
                return s
            }
            return 0
        }

        var total: Int64 = 0
        if let enumerator = fileManager.enumerator(atPath: path) {
            for case let subPath as String in enumerator {
                let fullSubPath = (path as NSString).appendingPathComponent(subPath)
                let attrs = try? fileManager.attributesOfItem(atPath: fullSubPath)
                if let type = attrs?[.type] as? FileAttributeType, type == .typeRegular {
                    if let num = attrs?[.size] as? NSNumber {
                        total += num.int64Value
                    } else if let s = attrs?[.size] as? Int64 {
                        total += s
                    }
                }
            }
        }
        return total
    }
}```

---

## 3. `Spartan/ContentView.swift` (전체 1941행)

파일 전체를 따로 첨부합니다. 아래는 1단계에서 건드릴 구간입니다.

### 상단 @State 선언부 — 19~70행

**`showSubView` 는 현재 39입니다.** 새 시트를 쓰면 키우십시오. 안 키우면 앱이 켜지자마자 죽습니다.

```swift
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
```

### body 시작 — 배너·미리보기 오버레이를 붙일 곳 — 104~152행

폴더 진입 1.2초 배너와 일괄 삭제 미리보기를 여기 `ZStack` 으로 감싸면 됩니다. 들여쓰기가 공백 16칸입니다.

```swift
    @State private var nonexistentFile = "" //REALLY dont question it
    
    @State private var isImageSVG: [Bool] = [false, false]
    
    var body: some View {
        VStack {
            VStack {
                HStack { //input directory + refresh
                    TextField(NSLocalizedString("INPUT_DIRECTORY", comment: "According to all known laws of aviation"), text: $directory, onCommit: {
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
                        CreateDirectoryView(directoryPath: $directory, isPresented: $showSubView[6])
                    })
                    .sheet(isPresented: $showSubView[5], content: {
                        CreateFileView(filePath: $directory, isPresented: $showSubView[5])
                    })
                    .sheet(isPresented: $showSubView[20], content: {
                        CreateSymlinkView(symlinkPath: $directory, isPresented: $showSubView[20])
                    })
                }
```

### updateFiles() — 폴더가 바뀌는 지점 — 1751~1806행

배너를 띄울 자리입니다. 여기서 `directory` 가 확정됩니다.

```swift

    func updateFiles() { //credit to ethanrdoesmc for the updates to this. now we have a ds_store clone!
        guard fileManager.fileExists(atPath: directory + metadataName!) else {
            masterFiles = oldUpdate()
            print("file didnt exist")
            return
        }
        guard let encoded = fileManager.contents(atPath: directory + metadataName!) else {
            masterFiles = oldUpdate()
            print("file was broke")
            return
        } //two guards. one to make sure the file exists, and one to make sure it has data. if it doesn't exist (directory hasn't been cached yet) or is empty (no files in the folder, would be a waste of time to check it), fallback to the old method (which when a directory is empty, is extremely fast).
        let decoder = JSONDecoder()
        var decoded: [SpartanFile] = []
        do {
            masterFiles = []
            print(metadataName!)
            decoded = try decoder.decode([SpartanFile].self, from: encoded)
            let contents = try FileManager.default.contentsOfDirectory(atPath: directory)
            var files: [String]
            files = contents.map { file in
                let filePath = "/" + directory + "/" + file
                var isDirectory: ObjCBool = false
                FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory)
                return isDirectory.boolValue ? "\(file)/" : file
            }
            if let metadataIndex = files.firstIndex(of: metadataName!) {
                files.remove(at: metadataIndex)
            } //hide the metadata file from view
            for i in 0..<files.count {
                if let j = decoded.map({ $0.name }).firstIndex(of: files[i]) {
                    masterFiles.append(decoded[j])
                } else {
                    masterFiles.append(SpartanFile(name: files[i], fullPath: directory + files[i], isSelected: false, fileType: FileInfo.yandereDevFileType(file: directory + files[i]), isLoadingFile: false))
                }
            }
        } catch {
            masterFiles = oldUpdate()
            print(error)
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.5) { //removes files that no longer exist from the cache (so the filesize doesn't grow until you run out of disk space)
            var decoded2 = decoded
            for i in 0..<decoded.count {
                if !(masterFiles.contains(decoded[i])) {
                    decoded2.remove(at: i)
                }
            }
            let encoder = JSONEncoder()
            do {
                let encoded = try encoder.encode(decoded2)
                try encoded.write(to: URL(fileURLWithPath: tempPath))
            } catch {
                print("failed to update and/or save cached metadata: \(error)")
            }
            RootHelperActs.mvtemp(directory + metadataName!)
            multiSelect = false
```

### rm 호출 (메타데이터) — 500~515행

```swift
                            .contextMenu {
                                Button(action: {
                                    E.toggle()
                                }) {
                                    Text("Toggle Debug")
                                }
                                
                                Button(action: {
                                    RootHelperActs.rm(directory + metadataName!)
                                }) {
                                    Text(localizedString: "CACHE_CLEAR")
                                }
                                
                                Button("Dismiss", action: { } ) //the only way to exit a tvOS context menu is to press a button. nothing else
                            }
                        } else {
```

### 삭제 계열 호출부 (목록 쪽) — 818~870행

`deletePathDirectly` / `emptyAll` / `moveToTrash` / `RootHelperActs.rm` 가 한자리에 모여 있습니다.

```swift
                                        Text(NSLocalizedString("OPENIN", comment: "The bee, of course, flies anyway"))
                                            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                            }
                                    }
                                    
                                    if(directory == "/private/var/mobile/Media/.Trash/"){
                                        Button(action: {
                                            if TrashManager.shared.deletePathDirectly(masterFiles[index].fullPath) {
                                                updateFiles()
                                            }
                                        }) {
                                            Text(NSLocalizedString("DELETE", comment: "because bees don't care what humans think is impossible."))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                }
                                        }
                                    } else if (directory == "/private/var/mobile/Media/" && masterFiles[index].name == ".Trash/") {
                                        Button(action: {
                                            _ = TrashManager.shared.emptyAll()
                                            updateFiles()
                                        }) {
                                            Text(NSLocalizedString("TRASHYEET", comment: "Yellow, black. Yellow, black."))
                                                .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                    view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                }
                                        }
                                    } else {
                                        Button(action: {
                                            if TrashManager.shared.moveToTrash(path: masterFiles[index].fullPath) {
                                                updateFiles()
                                            }
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
                                                }
                                        }
                                    }
                                    
                                    Button(action: {
                                        showSubView[17] = true
```

### 삭제 계열 호출부 (상세 쪽) — 1148~1182행

위와 같은 동작의 두 번째 자리입니다. **교체 블록을 만들 때 둘을 구분하십시오.**

```swift
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
```

### chmod 호출부 — 권한 변경 — 1470~1490행

```swift
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
```

### mvtemp 호출부 — 임시파일 경유 쓰기 — 1795~1850행

```swift
                    decoded2.remove(at: i)
                }
            }
            let encoder = JSONEncoder()
            do {
                let encoded = try encoder.encode(decoded2)
                try encoded.write(to: URL(fileURLWithPath: tempPath))
            } catch {
                print("failed to update and/or save cached metadata: \(error)")
            }
            RootHelperActs.mvtemp(directory + metadataName!)
            multiSelect = false
            resetMultiSelectArrays()
        }
        if UserDefaults.settings.bool(forKey: "autoComplete") && !directory.hasSuffix("/") && FileInfo.isDirectory(filePath: directory) {
            directory = directory + "/"
        }
        if directory == "//" {
			directory = "/"
		}
        DispatchQueue.global().asyncAfter(deadline: .now() + 2) {
            cacheFolder(directory)
        }
    }
    
    func oldUpdate() -> [SpartanFile] {
        var new: [SpartanFile] = []
        do {
            let contents = try FileManager.default.contentsOfDirectory(atPath: directory)
            var files: [String]
            files = contents.map { file in
                let filePath = "/" + directory + "/" + file
                var isDirectory: ObjCBool = false
                FileManager.default.fileExists(atPath: filePath, isDirectory: &isDirectory)
                return isDirectory.boolValue ? "\(file)/" : file
            }
            if let metadataIndex = files.firstIndex(of: metadataName!) {
                files.remove(at: metadataIndex)
            } //hide the metadata file from view
            for i in 0..<contents.count {
                new.append(SpartanFile(name: files[i], fullPath: directory + files[i], isSelected: false, fileType: FileInfo.yandereDevFileType(file: directory + files[i]), isLoadingFile: false))
            }
            let encoder = JSONEncoder()
            let encoded = try encoder.encode(new)
            try encoded.write(to: URL(fileURLWithPath: tempPath))
            RootHelperActs.mvtemp(directory + metadataName!)
        } catch {
            print(error.localizedDescription)
        }
        return new
    }
    
    func goBack() {
        guard directory != "/" else {
            return
        }
```

---

## 4. 참고 — `RootHelperActs` 원문

`Spartan/SpareViews.swift` 334~366행입니다. **여기가 관문입니다.**

```swift
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
```

---

## 5. 제안

`FileGuard` 를 **`RootHelperActs` 안쪽**에 거십시오. 메서드 여덟 개 머리에 한 줄씩이면 root 경로가 전부 덮입니다. 호출부 열세 곳을 각각 고치는 것보다 빠뜨릴 여지가 적습니다.

그다음 root 를 안 거치는 일곱 곳(`TrashManager` 의 FileManager 1차 시도, `Compression`, `HexView`, `TextView`)을 따로 막으십시오.

막혔을 때 **무엇이 왜 막혔는지 화면에 띄워야 합니다.** 이전에 삭제가 조용히 실패해서 원인 찾는 데 오래 걸렸습니다.
