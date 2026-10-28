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

    /// 읽기 모드로 시작한다.
    ///
    /// 줄마다 TextField 를 쓰면 한 줄에 한 행이라 긴 줄이 그냥 잘린다.
    /// 로그는 한 줄이 화면 몇 배로 길어서 사실상 읽을 수가 없었다.
    /// 읽기 모드는 Text 로 그려 줄을 접는다. 편집이 필요하면 바꾸면 된다.
    @State private var readOnly = true
    
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
					readOnly.toggle()
				}) {
					HStack(spacing: 8) {
						Image(systemName: readOnly ? "doc.plaintext" : "pencil")
						Text(readOnly ? L("읽기 — 줄 접힘") : L("편집 — 한 줄씩"))
					}
				}
				Text(LF("{0}줄", fileContents.count))
					.font(.caption)
					.foregroundColor(.secondary)
			}

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
						if case .failure(let guardErr) = FileGuard.authorize(action: "write_text", paths: [fullPath], notifyOnBlock: false) {
							errorString = guardErr.localizedDescription
							errorShow = true
							return
						}
						if filePathIsNotMobileWritable(fullPath) {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: tempPath), atomically: true, encoding: encoding)
							// 원본을 먼저 지우지 않는다. mvtemp 가 한 번에 바꿔 끼우므로,
							// 실패해도 원본이 남는다. 전에는 지운 뒤에 옮기다 실패하면
							// 편집한 것도 원본도 다 없어졌다.
							if !RootHelperActs.mvtemp(fullPath) {
								errorString = L("저장하지 못했습니다. 원본은 그대로입니다.\n") + (RootHelperActs.lastError ?? "")
								errorShow = true
								return
							}
						} else {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: fullPath), atomically: true, encoding: encoding)
							OperationLog.record(action: "EDIT_TEXT", path: fullPath)
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
					if readOnly {
						// 줄을 접어서 보여준다. fixedSize 가 없으면 List 가
						// 행 높이를 한 줄로 잡아 접힌 부분이 잘려 보인다.
						Text(fileContents[indexB])
							.lineLimit(nil)
							.fixedSize(horizontal: false, vertical: true)
							.frame(maxWidth: .infinity, alignment: .leading)
							.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
								view.scaledFont(name: "BotW Sheikah Regular", size: 40)
							}
					} else {
						TextField(NSLocalizedString("LINEADD_DESCRIPTION", comment: "- You going to the funeral?"), text: $fileContents[indexB])
							.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
								view.scaledFont(name: "BotW Sheikah Regular", size: 40)
							}
					}
				}
			}
        }
    }
}
