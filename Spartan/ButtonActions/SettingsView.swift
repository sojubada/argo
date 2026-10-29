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

    // 화면 언어
    @State private var languagePre = ArgoLocale.current
    @State private var showLanguageRestart = false

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
			
			// 화면 언어. 두 언어로 같이 적는다 — 읽지 못하는 언어로 바뀌어도
			// 되돌릴 단추를 찾을 수 있어야 한다.
			Button(action: {
				let codes = ArgoLocale.supported.map { $0.code }
				let i = codes.firstIndex(of: languagePre) ?? 0
				languagePre = codes[(i + 1) % codes.count]
				ArgoLocale.current = languagePre
				showLanguageRestart = true
			}) {
				HStack {
					Image(systemName: "globe")
					Text("Language · 언어: " + (ArgoLocale.supported.first(where: { $0.code == languagePre })?.name ?? languagePre))
				}
			}
			.alert(isPresented: $showLanguageRestart) {
				Alert(
					title: Text(L("다시 켜면 모든 화면에 적용됩니다")),
					message: Text(L("지금 끄고 다시 켜시겠습니까?")),
					primaryButton: .default(Text(L("지금 끄기")), action: {
						let app = UIApplication.shared
						let suspend = Selector(("suspend"))
						if app.responds(to: suspend) { _ = app.perform(suspend) }
						DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { exit(0) }
					}),
					secondaryButton: .cancel(Text(L("나중에")))
				)
			}
			Text(" ")

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
            
            // 안전 모드 스위치. 안전판에서는 끌 수 없다.
            if ArgoEdition.isSafe {
                HStack {
                    Image(systemName: "lock.shield.fill")
                        .foregroundColor(.green)
                    Text(L("안전판 — 안전 모드가 늘 켜져 있습니다"))
                }
                Text(L("시스템 잠금이 걸린 항목이 안에 있는 폴더는 지울 수 없습니다. 끄려면 모험가판을 설치하십시오."))
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else {
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
                    Text(L("안전 모드 (시스템 쓰기 차단)"))
                        .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                            view.scaledFont(name: "BotW Sheikah Regular", size: 40)
                        }
                }
            }
            .alert(isPresented: $showSafeModeWarning) {
                Alert(
                    title: Text(L("안전 모드를 끄시겠습니까?")),
                    message: Text(L("시스템 파일을 바꿀 수 있게 됩니다. 애플TV가 부팅하지 못할 수 있습니다.")),
                    primaryButton: .destructive(Text(L("끄기")), action: {
                        safeModeEnabledPre = false
                        UserDefaults.settings.set(false, forKey: "safeModeEnabled")
                    }),
                    secondaryButton: .cancel(Text(L("유지")))
                )
            }
            Text(LF("{0} — 안전 모드를 끌 수 있습니다. 켜 두면 잠긴 항목이 든 폴더는 지울 수 없습니다.", ArgoEdition.name))
                .font(.caption)
                .foregroundColor(.secondary)
            }
            
            Button(action: {
                showView[5] = true
            }) {
                HStack {
                    Image(systemName: "doc.text.magnifyingglass")
                    Text(L("작업 기록 보기"))
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
					Image(systemName: "paintpalette")
					Text(L("테마 · 아이콘"))
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
					Text(L("Lab ① WebKit 탐침 — 확인 완료"))
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
					Text(L("Lab ② 문서 렌더링 탐침 — 확인 완료"))
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
					Text(L("Lab ③ 인쇄 파이프라인 탐침"))
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
				probeStatus  = L("실행 중…")
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
						Text(L("Lab ④ media 폴더 PDF 비교 탐침 ← 지금 이것"))
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

/// 테마 · 아이콘 고르기.
///
/// 고르면 화면 색(바탕·포인트 색)과 앱 아이콘이 같이 바뀐다. 아이콘은 그 자리에서
/// 바뀌고, 화면 색은 다시 켜야 모든 화면에 들어간다 — 이미 그려 둔 화면까지
/// 다시 칠할 길이 없다.
struct IconView: View {
	@State private var current = ArgoTheme.themeIndex()
	@State private var showRestart = false
	@State private var iconError: String?

	var body: some View {
		ScrollView {
			VStack(spacing: 30) {
				Text(L("테마 · 아이콘"))
					.font(.system(size: 56, weight: .semibold))
				Text(L("화면 바탕과 포인트 색, 앱 아이콘이 함께 바뀝니다."))
					.foregroundColor(.secondary)
				ForEach(0..<2, id: \.self) { row in
					HStack(spacing: 40) {
						ForEach(0..<3, id: \.self) { col in
							themeButton(ArgoThemeChoice.all[row * 3 + col])
						}
					}
				}
				if let iconError {
					Text(iconError).font(.caption).foregroundColor(.orange)
				}
			}
			.padding(40)
		}
		.alert(isPresented: $showRestart) {
			Alert(
				title: Text(L("다시 켜면 모든 화면에 적용됩니다")),
				message: Text(L("지금 끄고 다시 켜시겠습니까?")),
				primaryButton: .default(Text(L("지금 끄기")), action: {
					let app = UIApplication.shared
					let suspend = Selector(("suspend"))
					if app.responds(to: suspend) { _ = app.perform(suspend) }
					DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { exit(0) }
				}),
				secondaryButton: .cancel(Text(L("나중에")))
			)
		}
	}

	private func themeButton(_ t: ArgoThemeChoice) -> some View {
		Button(action: { choose(t) }) {
			VStack(spacing: 10) {
				Image("ThemePreview-" + t.id)
					.resizable()
					.aspectRatio(400.0 / 240.0, contentMode: .fit)
					.frame(width: 400)
					.cornerRadius(14)
				HStack(spacing: 8) {
					if current == t.index {
						Image(systemName: "checkmark.circle.fill")
					}
					Text(t.title)
					Circle().fill(t.swatch).frame(width: 22, height: 22)
				}
			}
			.padding(12)
		}
		.buttonStyle(.card)
	}

	private func choose(_ t: ArgoThemeChoice) {
		guard t.index != current else { return }
		UserDefaults.settings.set(t.index, forKey: "argoTheme")
		current = t.index
		iconError = nil
		// 기본 테마는 주 아이콘으로 되돌린다.
		guard UIApplication.shared.supportsAlternateIcons else {
			showRestart = true
			return
		}
		UIApplication.shared.setAlternateIconName(t.iconName) { error in
			DispatchQueue.main.async {
				if let error {
					iconError = L("아이콘을 바꾸지 못했습니다: ") + error.localizedDescription
				}
				showRestart = true
			}
		}
	}
}

struct ArgoThemeChoice: Identifiable {
	let index: Int
	let id: String          // 미리보기·아이콘 이름의 뒷부분
	let title: String
	let swatch: Color

	/// 다른 아이콘 이름. 기본(Midnight)은 nil — 주 아이콘이다.
	var iconName: String? { index == 0 ? nil : "Theme" + id }

	static let all: [ArgoThemeChoice] = [
		ArgoThemeChoice(index: 0, id: "Midnight", title: L("한밤"), swatch: Color(red: 0.29, green: 0.69, blue: 0.78)),
		ArgoThemeChoice(index: 1, id: "Ocean", title: L("바다"), swatch: Color(red: 0.38, green: 0.66, blue: 1.00)),
		ArgoThemeChoice(index: 2, id: "Forest", title: L("숲"), swatch: Color(red: 0.40, green: 0.82, blue: 0.55)),
		ArgoThemeChoice(index: 3, id: "Ember", title: L("노을"), swatch: Color(red: 1.00, green: 0.58, blue: 0.32)),
		ArgoThemeChoice(index: 4, id: "Orchid", title: L("보랏빛"), swatch: Color(red: 0.78, green: 0.58, blue: 1.00)),
		ArgoThemeChoice(index: 5, id: "Gold", title: L("황금빛"), swatch: Color(red: 0.95, green: 0.78, blue: 0.36)),
	]
}
