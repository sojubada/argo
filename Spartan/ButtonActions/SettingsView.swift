//
//  SettingsView.swift
//  Spartan
//
//  Created by RealKGB on 4/6/23.
//

import SwiftUI

struct SettingsView: View {
    // showView 배열 크기 5 -> 6 -> 7 -> 8 확장 (media 폴더 탐침 추가)
    @State private var showView: [Bool] = [Bool](repeating: false, count: 8)
    @State private var probeRunning = false
    @State private var probeStatus = ""
    @State private var descriptiveTitlesPre = UserDefaults.settings.bool(forKey: "descriptiveTitles")
    @State private var descriptiveTimestampsPre = UserDefaults.settings.bool(forKey: "verboseTimestamps")
    @State private var autoCompletePre = UserDefaults.settings.bool(forKey: "autoComplete")
    @State private var logWindowFontSizePre = UserDefaults.settings.integer(forKey: "logWindowFontSize")
    @State private var sheikahFontApplyPre = UserDefaults.settings.bool(forKey: "sheikahFontApply")
    @State private var dateFormatPre: String = UserDefaults.settings.string(forKey: "dateFormat") ?? ""
    
    // 안전모드 설정
    @State private var safeModeEnabledPre = UserDefaults.settings.bool(forKey: "safeModeEnabled")
    @State private var showSafeModeWarning = false
    
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
            
            // 안전 모드 스위치
            Button(action: {
                if safeModeEnabledPre {
                    showSafeModeWarning = true // 끌 때만 경고
                } else {
                    safeModeEnabledPre = true
                    UserDefaults.settings.set(true, forKey: "safeModeEnabled")
                }
            }) {
                HStack {
                    Image(systemName: safeModeEnabledPre ? "checkmark.shield.fill" : "shield.slash")
                        .foregroundColor(safeModeEnabledPre ? .green : .red)
                    Text("안전 모드 (시스템 쓰기 차단)")
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                        }
                }
            }
            .alert(isPresented: $showSafeModeWarning) {
                Alert(
                    title: Text("안전 모드를 끄시겠습니까?"),
                    message: Text("시스템 파일을 바꿀 수 있게 됩니다. 애플TV가 부팅하지 못할 수 있습니다."),
                    primaryButton: .destructive(Text("끄기"), action: {
                        safeModeEnabledPre = false
                        UserDefaults.settings.set(false, forKey: "safeModeEnabled")
                    }),
                    secondaryButton: .cancel(Text("유지"))
                )
            }
            
            Button(action: {
                showView[5] = true
            }) {
                HStack {
                    Image(systemName: "doc.text.magnifyingglass")
                    Text("작업 기록 보기")
                }
            }
            .sheet(isPresented: $showView[5]) {
                TextView(filePath: OperationLog.activeLogDirectory, fileName: OperationLog.fileName, isPresented: $showView[5])
            }
            Text(" ")
			
			GeometryReader { geometry in
				HStack {
					Spacer()
					ArgoTextField(NSLocalizedString("SETTINGS_DATEFORMAT", comment: ""), text: $dateFormatPre)
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
					Text("Lab ② 문서 렌더링 탐침 — 확인 완료")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
			}
			.fullScreenCover(isPresented: $showView[4]) {
				WebDocProbeView(isPresented: $showView[4])
					.edgesIgnoringSafeArea(.all)
			}

			Button(action: { //print pipeline probe
				showView[6] = true
			}) {
				HStack {
					Image(systemName: "printer")
						.frame(width: 50, height: 50)
					Text("Lab ③ 인쇄 파이프라인 탐침")
						.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
							view.scaledFont(name: "BotW Sheikah Regular", size: 40)
						}
				}
			}
			.fullScreenCover(isPresented: $showView[6]) {
				PrintProbeView(isPresented: $showView[6])
					.edgesIgnoringSafeArea(.all)
			}

			// Lab ④ — media 폴더 원본 vs PC PDF 비교 탐침
			//
			// 결과를 버튼 아래에 그대로 띄웁니다. 로그를 꺼내 봐야만 알 수
			// 있으면, 받아 본 로그가 묵은 사본인지 탐침이 안 돈 것인지
			// 구별하느라 한참을 돌게 됩니다.
			Button(action: {
				guard !probeRunning else { return }
				probeRunning = true
				probeStatus  = "실행 중…"
				Task {
					let summary = await MediaFolderProbe.run()
					await MainActor.run {
						probeRunning = false
						probeStatus  = summary
					}
				}
			}) {
				HStack(alignment: .top) {
					if probeRunning {
						ProgressView()
							.frame(width: 50, height: 50)
					} else {
						Image(systemName: "arrow.triangle.2.circlepath.doc.on.clipboard")
							.frame(width: 50, height: 50)
					}
					VStack(alignment: .leading, spacing: 8) {
						Text("Lab ④ media 폴더 PDF 비교 탐침 ← 지금 이것")
							.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
								view.scaledFont(name: "BotW Sheikah Regular", size: 40)
							}
						if !probeStatus.isEmpty {
							Text(probeStatus)
								.font(.system(size: 22))
								.foregroundColor(.secondary)
								.multilineTextAlignment(.leading)
								.fixedSize(horizontal: false, vertical: true)
						}
					}
				}
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
    // 캐시 파일 이름을 못 읽으면 할 일이 없다.
    guard let metaName = metadataName else { return }

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
        // 캐시 파일이 폴더에 있을 때만 뺀다.
        //
        // 전에는 files.firstIndex(of:)! 로 강제 언래핑했다. 캐시가 아직 없는
        // 폴더에서는 nil 이 나와 뒷줄 스레드에서 트랩이 났다 — 실행 직후 종료의
        // 원인이다. 원래 코드에서는 updateFiles 가 캐시 없는 폴더에서 먼저
        // return 해 여기까지 오지 않았고, 그 구조가 사라지면서 드러났다.
        if let metaIndex = files.firstIndex(of: metaName) {
            files.remove(at: metaIndex)
        }
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
    RootHelperActs.mvtemp(directory + metaName)
}
