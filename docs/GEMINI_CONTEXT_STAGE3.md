# Argo 3단계(PDF 뷰어) 추가 컨텍스트

`GEMINI_CONTEXT.md`에 더해, 3단계에서 손대야 할 지점의 **현재 코드**입니다.
1·2단계가 반영된 상태이며 행 번호도 현재 main 기준입니다.

**치환 블록의 "찾을 내용"은 반드시 아래에서 복사하십시오.**
`ContentView.swift`는 스페이스와 탭이 섞여 있습니다 — 특히 `case 14:` 주변은
탭 들여쓰기입니다.

## 이미 반영된 사항

- 배포 타깃 tvOS 16.0 (pbxproj 6곳 + Info.plist MinimumOSVersion)
- `showSubView` 크기 38, 인덱스 36 = pdfViewShow (미사용), 37 = trashViewShow (사용 중)
- `Spartan/Trash/` 휴지통, `Spartan/Favorites/FavoritesStore.swift` 즐겨찾기


## ContentView.swift — 735~760행 (컨텍스트 메뉴 '열기' 분기 ①, case 14 주변)

```swift
                                                Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
											case 13:
												Image(systemName: "externaldrive")
												Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
											case 14:
												Image(systemName: "safari")
												Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
                                            default:
                                                Image(systemName: "doc")
                                                Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
                                            }
                                        }
                                    }
                                }
```


## ContentView.swift — 1039~1064행 (컨텍스트 메뉴 '열기' 분기 ②, case 14 주변)

```swift
												Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
											case 13:
												Image(systemName: "externaldrive")
												Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
											case 14:
												Image(systemName: "safari")
												Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
                                            default:
                                                Image(systemName: "doc")
                                                Text(masterFiles[index].name)
                                                    .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                                                        view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                                                    }
                                            }
                                        }
                                    }
                                }
```


## ContentView.swift — 1600~1690행 (defaultAction의 switch fileType 전체)

```swift
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
            default:
                masterFiles[index].isLoadingFile = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                    showSubView[22] = true
                }
            }
        }
    }

    func updateFiles() { //credit to ethanrdoesmc for the updates to this. now we have a ds_store clone!
```


## ContentView.swift — 440~470행 (파일 뷰어 .sheet 들이 붙어 있는 구간. PDF 시트를 여기 추가)

```swift
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
                            DMGMountView(filePath: newViewFilePath, fileName: newViewFileName, directory: $directory, isPresented: $showSubView[32])
                        })
                        .sheet(isPresented: $showSubView[33], content: {
                            HTMLView(filePath: newViewFilePath, fileName: newViewFileName)
```


## ContentView.swift — 50~95행 (showSubView 선언 + 인덱스 주석, 현재 상태)

```swift
    @State var filePerms = 420
    
    @State var didChangeDir = false
    
    @State private var showSubView: [Bool] = [Bool](repeating: false, count: 38)
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
    
    @Binding var globalAVPlayer: AVPlayer //this is because Spartan has the ability to play music without the AudioPlayerView being shown. It took about a week to get working properly and I'm proud of it
    @State var isGlobalAVPlayerPlaying = false
```


## FileInfo.swift — 1~60행 (yandereDevFileType 판정 체인. PDF 분기를 isText 앞에 추가)

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


## FileInfo.swift — 145~235행 (isText / isHTML 등 판정 헬퍼 구현 스타일)

```swift
        }
    
        if let image = UIImage(contentsOfFile: filePath) {
            return image.size.width > 0 && image.size.height > 0
        }
        return false
    }
    class func isText(filePath: String) -> Bool {
        guard let data = fileManager.contents(atPath: filePath) else {
            return false
        }
    
        return data.allSatisfy { Character(UnicodeScalar($0)).isASCII } || String(data: data, encoding: .utf8) != nil || String(data: data, encoding: .utf16) != nil || String(data: data, encoding: .utf32) != nil
    }
    class func isPlist(filePath: String) -> Double {
        guard let data = fileManager.contents(atPath: filePath) else {
            return 0
        }
        
        if data.count > 5 {
            if let header = String(data: data.subdata(in: 0..<5), encoding: .utf8) {
                if header == "<?xml" {
                    return 5.1
                } else if header == "bplis" {
                    return 5.9
                }
            }
        }
        return 0
    }
    class func isSymlink(filePath: String) -> Bool {
        let fileURL = URL(fileURLWithPath: filePath)
        
        do {
            let resourceValues = try fileURL.resourceValues(forKeys: [.isSymbolicLinkKey])
            if let isSymbolicLink = resourceValues.isSymbolicLink {
                return isSymbolicLink
            }
        } catch {
            print("Error: \(error)")
        }
        return false
    }
    class func isCar(filePath: String) -> Bool {
        guard let data = fileManager.contents(atPath: filePath) else {
            return false
        }
        if data.count > 8 {
            if let header = String(data: data.subdata(in: 0..<8), encoding: .utf8) {
                return header == "BOMStore"
            }
        }
        return false
    }
    class func isHTML(filePath: String) -> Bool {
        guard let data = fileManager.contents(atPath: filePath) else {
            return false
        }
        if data.count > 15 {
            if let header = String(data: data.subdata(in: 0..<15), encoding: .utf8) {
                return header == "<!DOCTYPE html>"
            }
        }
        return false
    }
    class func isArchive(filePath: String) -> Bool {
        guard let fileCString = filePath.cString(using: .utf8) else {
            return false
        }
        
        guard let fileData = fileManager.contents(atPath: filePath) else {
            return false
        }
        if fileData.count > 2 {
            if fileData.subdata(in: 0..<2) == Data(fromHexEncodedString: "504B") {
                return true
            }
        }

        let archive = archive_read_new()
        var entry = archive_entry_new()
        
        archive_read_support_format_all(archive)
        archive_read_support_filter_all(archive)
        archive_read_support_filter_lzma(archive)

        guard archive_read_open_filename(archive, fileCString, 10240) == ARCHIVE_OK else {
            archive_read_close(archive)
            archive_read_free(archive)
            return false
        }
```


## FileOpeners/TextView.swift — 전체 (파일 뷰어 기본 구조 참고)

```swift
//
//  TextView.swift
//  Spartan
//
//  Created by RealKGB on 4/18/23.
//

import SwiftUI

struct TextView: View {
    @State var filePath: String
    @State var fileName: String
    @Binding var isPresented: Bool
    
    @State private var fileContents: [String] = []
    @State private var textEditorShow = false
    @State private var textEditorString = ""
    @State private var textEditorIndex = 0
    @State private var textEditorOldIndex = 0
    @State private var encoding: String.Encoding = .utf8
    @State private var index: Int = 0
    
    @State private var errorShow = false
    @State private var errorString = ""
    
    init(filePath: String, fileName: String, isPresented: Binding<Bool>) {
		_filePath = State(initialValue: filePath)
		_fileName = State(initialValue: fileName)
		_isPresented = isPresented
		
		do {
            let url = URL(fileURLWithPath: filePath + fileName)
            let fileData = try Data(contentsOf: url)
            let detectedEncoding = NSString.stringEncoding(for: fileData, encodingOptions: nil, convertedString: nil, usedLossyConversion: nil)
            if detectedEncoding != 0 {
                _encoding = State(initialValue: String.Encoding(rawValue: detectedEncoding))
            }
            let fileString = String(data: fileData, encoding: encoding)
            _fileContents = State(initialValue: fileString?.components(separatedBy: "\n") ?? ["An error occurred while trying to read the text file."])
        } catch {
            print("Error loading file: \(error.localizedDescription)")
        }
    }
    
    var body: some View {
        VStack {
			Text(UserDefaults.settings.bool(forKey: "verboseTimestamps") ? filePath + fileName : fileName)
				.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
					view.scaledFont(name: "BotW Sheikah Regular", size: 40)
				}
				.font(.system(size: 40))
				.multilineTextAlignment(.center)
			HStack {
				Button(action: {
					if(index == 0) {
						fileContents.insert("", at: 1)
					} else {
						fileContents.insert("", at: index)
					}
				}) {
					Text("\(NSLocalizedString("LINEADD", comment: "- Hear about Frankie?")) \(index)")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
				Button(action: {
					fileContents.remove(at: index)
				}) {
					Text("\(NSLocalizedString("LINEREMOVE", comment: "- Yeah.")) \(index)")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
				VStack {
					Text(LocalizedString("LINESELECTED"))
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 25)
						}
						.font(.system(size: 25))
						.multilineTextAlignment(.center)
					StepperTV(value: $index, isHorizontal: true) { }
				}
				Spacer()
				Button(action: {
					do {
						let fullPath = filePath + fileName
						if filePathIsNotMobileWritable(fullPath) {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: tempPath), atomically: true, encoding: encoding)
							RootHelperActs.rm(fullPath)
							RootHelperActs.mvtemp(fullPath)
						} else {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: fullPath), atomically: true, encoding: encoding)
						}
						isPresented = false
					} catch {
						print("Failed to save file: \(error.localizedDescription)")
						errorString = "Failed to save file: \(error.localizedDescription)"
						errorShow = true
					}
				}) {
					Image(systemName: "square.and.arrow.down")
				}
				.alert(isPresented: $errorShow, content: {
					Alert(
						title: Text(NSLocalizedString("ERROR", comment: "")),
						message: Text(errorString),
						dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: "")))
					)
				})
				
			}
			List(fileContents.indices, id: \.self) { indexB in
				HStack {
					if(indexB == index) {
						Text(String(indexB))
							.foregroundColor(.blue)
							.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
								view.scaledFont(name: "BotW Sheikah Regular", size: 40)
							}
					} else {
						Text(String(indexB))
							.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
								view.scaledFont(name: "BotW Sheikah Regular", size: 40)
							}
					}
					TextField(NSLocalizedString("LINEADD_DESCRIPTION", comment: "- You going to the funeral?"), text: $fileContents[indexB])
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
			}
        }
    }
}

```


## 3단계 작업 메모

- PDF 파일 타입 값은 **15** (0~14 사용 중, 69 = unknown)
- 판정은 `isText`보다 **앞에** 둘 것 (PDF 선두가 텍스트로 오판됨)
- 확장자 `.pdf` 또는 선두 5바이트 `%PDF-` 둘 다 인정
- `defaultAction`의 `switch fileType`에 `case 15: showSubView[36] = true`
- 컨텍스트 메뉴 '열기' 분기 두 곳에도 `case 15` 추가 (`case 14` 바로 뒤, 같은 모양)
- `.sheet(isPresented: $showSubView[36])` 로 PDF 뷰어 연결
- `showSubView` 크기는 이미 38이므로 **더 늘릴 필요 없음**
