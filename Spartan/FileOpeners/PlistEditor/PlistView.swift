//
//  PlistView.swift
//  Spartan
//
//  Created by RealKGB on 4/10/23.
//

import SwiftUI

struct PlistView: View {
	@State var filePath: String
	@State var fileName: String
	
	@State var plistDict: [PlistKey] = []
	
	@State var editingSubView = false
	@State var subViewToShow: PlistKeyType = .bool
	
	@State var addKeyToPlist = false
	
	@State private var errorShow = false
	@State private var errorString = ""
	
	init(filePath: String, fileName: String) {
        _filePath = State(initialValue: filePath)
        _fileName = State(initialValue: fileName)
        
        var tempDict: [String: Any] = [:]
        
        if let rawData = fileManager.contents(atPath: filePath + fileName) {
			do {
				if let dictionary = try PropertyListSerialization.propertyList(from: rawData, format: nil) as? [String: Any] {
					tempDict = dictionary
				} else {
					print("1288")
					_plistDict = State(initialValue: [PlistKey(key: "The plist file specified does not have a dictionary as its root", value: "While these are valid plist files, they are not yet supported by Spartan.", type: .dict), PlistKey(key: "Check for an update to Spartan. If you're already up-to-date, wait for an update that lists support for these types of plist files", value: "Error ID 1288", type: .dict)])
					return
				}
			} catch {
				print("1394")
				_plistDict = State(initialValue: [PlistKey(key: "The file specified is cannot be read", value: "It may be corrupted, or be the wrong file.", type: .dict), PlistKey(key: "Select the proper file and then try again", value: "Error ID 1394", type: .dict)])
			return
			}
		} else {
			print("1395")
			_plistDict = State(initialValue: [PlistKey(key: "The file specified is cannot be read", value: "It may be corrupted, or be the wrong file.", type: .dict), PlistKey(key: "Select the proper file and then try again", value: "Error ID 1395", type: .dict)])
			return
		}
		
		let temp = PlistFormatter.swiftDictToPlistKeyArray(tempDict)
		_plistDict = State(initialValue: temp) //THIS STUPID LINE OF CODE TOOK TWO MONTHS TO FIGURE OUT
		//I FORGOT TO INITIALIZE IT AND **I DIDNT NOTICE**, THATS WHY IT WASNT WORKING
		//I WAS SENDING THE DATA TO NOWHERE
		//KILL ME
	}
	
	var body: some View {
		VStack {
			HStack {
				Button(action: {
					addKeyToPlist = true
				}) {
					Image(systemName: "plus")
				}
				Spacer()
				Text(UserDefaults.settings.bool(forKey: "verboseTimestamps") ? filePath + fileName : fileName)
					.if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
						view.scaledFont(name: "BotW Sheikah Regular", size: 40)
					}
					.font(.system(size: 40))
					.multilineTextAlignment(.center)
					.padding(-10)
					.focusable(true)
				Spacer()
				Button(action: {
					writeDictToPlist(PlistFormatter.plistKeyArrayToSwiftDict(plistDict))
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
			List(plistDict.indices, id: \.self) { index in
				Button(action: {
					if(plistDict[index].type == .bool) {
						let bool = plistDict[index].value as! Bool
						if bool {
							plistDict[index].value = false
						} else {
							plistDict[index].value = true
						}
					} else {
						editingSubView = true
					}
				}) {
					HStack {
						if (plistDict[index].type == .bool) {
							Image(systemName: plistDict[index].value as! Bool ? "checkmark.square" : "square")
						}
						Text(PlistFormatter.formatPlistKeyForDisplay(plistDict[index]))
					}
				}
				.sheet(isPresented: $editingSubView, content: {
					PlistDictEditor(keyToEdit: $plistDict[index], isPresented: $editingSubView, selectedKeyType: plistDict[index].type.stringRepresentation())
				})
			}
		}
		.sheet(isPresented: $addKeyToPlist, content: {
			PlistAddDictView(plistDict: $plistDict, isPresented: $addKeyToPlist)
		})
	}
	
	func writeDictToPlist(_ dict: [String: Any]) {
		let nsdict = dict as NSDictionary
		let fullPath = filePath + fileName
		if filePathIsNotMobileWritable(fullPath) {
			do {
				try nsdict.write(to: URL(fileURLWithPath: tempPath))
			} catch {
				print("Failed to save file: \(error.localizedDescription)")
				errorString = "Failed to save file: \(error.localizedDescription)"
				errorShow = true
				// 임시 파일도 못 썼다. 여기서 멈춘다.
				//
				// 전에는 catch 가 돌아가지 않고 아래로 흘러, 임시 파일을 못
				// 쓴 채로 원본을 지우고 빈 자리에 아무것도 옮기지 못했다.
				return
			}
			// 원본을 먼저 지우지 않는다. mvtemp 가 바꿔 끼운다.
			if !RootHelperActs.mvtemp(fullPath) {
				errorString = L("저장하지 못했습니다. 원본은 그대로입니다.")
				errorShow = true
			}
		} else {
			do {
				try nsdict.write(to: URL(fileURLWithPath: fullPath))
			} catch {
				print("Failed to save file: \(error.localizedDescription)")
				errorString = "Failed to save file: \(error.localizedDescription)"
				errorShow = true
			}
		}
	}
}
