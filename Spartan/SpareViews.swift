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
    }
}

struct UIKitProgressView: UIViewRepresentable {
    typealias UIViewType = UIProgressView
    
    @Binding var value: Double
    var total: Double
    
    func makeUIView(context: Context) -> UIProgressView {
        let progressView = UIProgressView()
        progressView.progressTintColor = .white
        progressView.trackTintColor = .gray
        return progressView
    }
    
    func updateUIView(_ uiView: UIProgressView, context: Context) {
        uiView.progress = Float(value / total)
    }
}

struct Checkbox: View {
	@Binding var value: Bool
	@State var label: String = ""
	let onCommit: () -> Void

	var body: some View {
		Button(action: {
			value.toggle()
			onCommit()
		}) {
			HStack {
				Image(systemName: value ? "checkmark.square.fill" : "square")
				Text(label)
			}
		}
	}
}

struct StepperTV: View {
    @Binding var value: Int
    @State var isHorizontal: Bool
    @State var valueToIncrementBy = -1
    let onCommit: () -> Void
    
    var body: some View {
        if(isHorizontal) {
            HStack {
                minus
                valueText
                plus
            }
        } else {
            VStack {
                plus
                valueText
                minus
            }
        }
    }
    
    var minus: some View {
        Button(action: {
			if valueToIncrementBy != -1 {
				value -= valueToIncrementBy
			} else {
				value -= 1
            }
            onCommit()
        }) {
            Image(systemName: "minus")
                .font(.system(size: 30))
                .frame(width: 30, height: 30)
        }
    }
    
    var valueText: some View {
        Text("\(value)")
            .if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) { view in
                view.scaledFont(name: "BotW Sheikah Regular", size: 40)
            }
            .font(.headline)
    }
    
    var plus: some View {
        Button(action: {
            if valueToIncrementBy != -1 {
				value += valueToIncrementBy
			} else {
				value += 1
            }
            onCommit()
        }) {
            Image(systemName: "plus")
                .font(.system(size: 30))
                .frame(width: 30, height: 30)
        }
    }
}

struct ScaledFont: ViewModifier {
    @Environment(\.sizeCategory) var sizeCategory
    var name: String
    var size: Double

    func body(content: Content) -> some View {
       let scaledSize = UIFontMetrics.default.scaledValue(for: size)
        return content.font(.custom(name, size: scaledSize))
    }
}

extension View {
    public func blending(color: Color) -> some View {
        modifier(ColorBlended(color: color))
    }
    
    func scaledFont(name: String, size: Double) -> some View {
        return self.modifier(ScaledFont(name: name, size: size))
    }
    
    @ViewBuilder
    func `if`<Content: View>(_ condition: Bool, content: (Self) -> Content) -> some View {
        if condition {
            content(self)
        } else {
            self
        }
    } //this is one of the best things I've ever written tbh
}

public struct ColorBlended: ViewModifier {
    fileprivate var color: Color
  
    public func body(content: Content) -> some View {
        VStack {
            ZStack {
                content
                color.blendMode(.sourceAtop)
            }
            .drawingGroup(opaque: false)
        }
    }
}

struct ContextMenuButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        let paddingInt: CGFloat = -7
        let opacityInt: CGFloat = 1.0
        
        configuration.label
            .padding(paddingInt)
            .opacity(opacityInt)
    }
}

struct UIKitTapGesture: UIViewRepresentable {
    let action: () -> Void
    
    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        let tapGesture = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.onTap))
        view.addGestureRecognizer(tapGesture)
        return view
    }
    
    func updateUIView(_ uiView: UIView, context: Context) {}
    
    func makeCoordinator() -> Coordinator {
        Coordinator(action: action)
    }
    
    class Coordinator: NSObject {
        let action: () -> Void
        
        init(action: @escaping () -> Void) {
            self.action = action
        }
        
        @objc func onTap() {
            action()
        }
    }
} //this comes close

struct UIKitTextView: UIViewRepresentable {
    @Binding var text: String
    @State var fontSize: CGFloat
    @Binding var isTapped: Bool

    func makeUIView(context: Context) -> UITextView {
        let textView = UITextView()
        let opacity = 0.25
        textView.delegate = context.coordinator
        textView.isUserInteractionEnabled = !text.isEmpty
        textView.isSelectable = true
        
        if #available(tvOS 15.0, *) {
            if let dynamicColor = UIColor(named: "systemBackground") {
                textView.backgroundColor = dynamicColor.withAlphaComponent(opacity)
            } else {
                textView.backgroundColor = UIColor.darkGray.withAlphaComponent(opacity)
            }
        } else {
            textView.backgroundColor = UIColor.darkGray.withAlphaComponent(opacity)
        }
        if(UserDefaults.settings.bool(forKey: "sheikahFontApply")) {
            textView.font = UIFont(name: "BotW Sheikah Regular", size: fontSize)
        } else {
            textView.font = UIFont(name: "SF Mono Regular", size: fontSize)
        }
        
        let tapGesture = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.textViewTapped))
        textView.addGestureRecognizer(tapGesture)
        
        if isTapped {
            textView.panGestureRecognizer.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.indirect.rawValue)]
        } else {
            textView.panGestureRecognizer.allowedTouchTypes = [0]
        }
        
        return textView
    }

    func updateUIView(_ uiView: UITextView, context: Context) {
        uiView.text = text
        uiView.isUserInteractionEnabled = !text.isEmpty
        if isTapped {
            uiView.panGestureRecognizer.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.indirect.rawValue)]
        } else {
            uiView.panGestureRecognizer.allowedTouchTypes = [0]
        }
    }
    
    func makeCoordinator() -> Coordinator {
        Coordinator(self, isTapped: $isTapped)
    }
    
    class Coordinator: NSObject, UITextViewDelegate {
        let parent: UIKitTextView
        @Binding var isTapped: Bool
    
        init(_ parent: UIKitTextView, isTapped: Binding<Bool>) {
            self.parent = parent
            _isTapped = isTapped
        }
        
        func textViewDidChange(_ textView: UITextView) {
            parent.text = textView.text
        }
        
        @objc func textViewTapped() {
            parent.isTapped.toggle()
        }
    }
}

extension Font {
    init(cgFont: CGFont, size: CGFloat) {
        let fontName = cgFont.postScriptName as String?
        self = Font.custom(fontName!, size: size) // Adjust the size as per your requirements
    }
}

extension Image {
    init(cgImage: CGImage) {
        self = Image(uiImage: UIImage(cgImage: cgImage))
    }
}

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

/// 파일 조작. 앱 안에서 먼저 하고, 루트 도우미는 실제로 있을 때만 쓴다.
///
/// 원래는 모든 조작을 루트 도우미(별도 실행 파일)를 띄워서 했다. 그런데
/// 배포되는 IPA 에는 그 도우미가 들어 있지 않고, 앱은 탈옥 분기로 컴파일돼
/// helperPath 가 /bin/RootHelper 로 남는다. 탈옥하지 않은 애플TV 에는 그런
/// 파일이 없다. 그래서 이름 바꾸기·새 폴더·새 파일·이동·복사·링크·권한
/// 바꾸기가 전부 없는 프로그램을 띄우고 아무 말 없이 끝났다.
///
/// 도우미가 하던 일은 FileManager 를 root 로 부르는 것뿐이다. TrollStore 로
/// 깐 앱은 /var/mobile 아래를 제 손으로 쓸 수 있으므로 — 휴지통과 탐침
/// 산출물이 이미 그렇게 쓰이고 있다 — 같은 일을 앱 안에서 먼저 한다.
///
/// 모든 함수가 성공 여부를 돌려준다. 작업 기록은 **실제로 일어났을 때만**
/// 남긴다. 전에는 '목적지가 있으면 성공' 으로 적어서, 이미 있던 파일 때문에
/// 아무 일도 안 한 이동이 성공으로 기록됐다.
class RootHelperActs {
    private static func checkAuth(action: String, paths: [String]) -> Bool {
        switch FileGuard.authorize(action: action, paths: paths) {
        case .success:
            return true
        case .failure(let err):
            print("[FileGuard Blocked] \(action) on \(paths) - \(err.localizedDescription)")
            return false
        }
    }

    /// 루트 도우미가 실제로 있는가. 없으면 띄울 이유가 없다.
    private static var helperAvailable: Bool {
        fileManager.isExecutableFile(atPath: helperPath)
    }

    /// 마지막 실패 이유. 사용자에게 보여 줄 한 줄이다.
    private(set) static var lastError: String?

    /// 앱 안에서 해 보고, 안 되면 도우미로 한 번 더. 둘 다 안 되면 false.
    private static func attempt(_ label: String,
                                path: String,
                                destination: String? = nil,
                                helperArgs: [String],
                                verify: () -> Bool,
                                _ body: () throws -> Void) -> Bool {
        lastError = nil
        do {
            try body()
            if verify() {
                OperationLog.record(action: label, path: path, destination: destination)
                return true
            }
            lastError = "작업이 끝났지만 결과를 확인하지 못했습니다."
        } catch {
            lastError = error.localizedDescription
        }
        guard helperAvailable else { return false }
        spawn(command: helperPath, args: helperArgs, env: [], root: true)
        if verify() {
            lastError = nil
            OperationLog.record(action: label + " (root)", path: path, destination: destination)
            return true
        }
        return false
    }

    private static func exists(_ p: String) -> Bool {
        if fileManager.fileExists(atPath: p) { return true }
        return (try? fileManager.destinationOfSymbolicLink(atPath: p)) != nil
    }

    @discardableResult
    class func rm(_ filePath: String) -> Bool {
        guard checkAuth(action: "rm", paths: [filePath]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        return attempt("DELETE", path: filePath, helperArgs: ["rm", filePath],
                       verify: { !exists(filePath) }) {
            try fileManager.removeItem(atPath: filePath)
        }
    }

    /// 옮긴다. 목적지가 이미 있으면 덮어쓰지 않고 실패한다 — 이름을 바꾸다가
    /// 같은 이름의 다른 파일을 지워 버리면 안 된다.
    @discardableResult
    class func mv(_ filePath: String, _ fileDest: String) -> Bool {
        guard checkAuth(action: "mv", paths: [filePath, fileDest]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        if exists(fileDest), lexical(fileDest) != lexical(filePath) {
            lastError = "같은 이름이 이미 있습니다."; return false
        }
        return attempt("MOVE", path: filePath, destination: fileDest,
                       helperArgs: ["mv", filePath, fileDest],
                       verify: { exists(fileDest) }) {
            try fileManager.moveItem(atPath: filePath, toPath: fileDest)
        }
    }

    @discardableResult
    class func cp(_ filePath: String, _ fileDest: String) -> Bool {
        guard checkAuth(action: "cp", paths: [filePath, fileDest]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        if exists(fileDest) {
            lastError = "같은 이름이 이미 있습니다."; return false
        }
        return attempt("COPY", path: filePath, destination: fileDest,
                       helperArgs: ["cp", filePath, fileDest],
                       verify: { exists(fileDest) }) {
            try fileManager.copyItem(atPath: filePath, toPath: fileDest)
        }
    }

    /// 빈 파일을 만든다. 이미 있으면 건드리지 않는다 — createFile 은 있는
    /// 파일을 빈 파일로 덮어쓰기 때문이다.
    @discardableResult
    class func touch(_ filePath: String) -> Bool {
        guard checkAuth(action: "touch", paths: [filePath]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        if exists(filePath) { lastError = "같은 이름이 이미 있습니다."; return false }
        return attempt("TOUCH", path: filePath, helperArgs: ["tf", filePath],
                       verify: { exists(filePath) }) {
            guard fileManager.createFile(atPath: filePath, contents: nil) else {
                throw CocoaError(.fileWriteUnknown)
            }
        }
    }

    @discardableResult
    class func mkdir(_ filePath: String) -> Bool {
        guard checkAuth(action: "mkdir", paths: [filePath]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        var isDir: ObjCBool = false
        if fileManager.fileExists(atPath: filePath, isDirectory: &isDir) {
            if isDir.boolValue { return true }
            lastError = "같은 이름의 파일이 있습니다."; return false
        }
        return attempt("MKDIR", path: filePath, helperArgs: ["td", filePath],
                       verify: { fileManager.fileExists(atPath: filePath) }) {
            try fileManager.createDirectory(atPath: filePath,
                                            withIntermediateDirectories: true)
        }
    }

    /// `filePath` 자리에 `fileDest` 를 가리키는 링크를 만든다.
    @discardableResult
    class func ln(_ filePath: String, _ fileDest: String) -> Bool {
        guard checkAuth(action: "ln", paths: [filePath, fileDest]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        if exists(filePath) { lastError = "같은 이름이 이미 있습니다."; return false }
        return attempt("SYMLINK", path: filePath, destination: fileDest,
                       helperArgs: ["ts", filePath, fileDest],
                       verify: { exists(filePath) }) {
            try fileManager.createSymbolicLink(atPath: filePath,
                                               withDestinationPath: fileDest)
        }
    }

    @discardableResult
    class func chmod(_ filePath: String, _ perms: Int) -> Bool {
        guard checkAuth(action: "chmod", paths: [filePath]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        return attempt("CHMOD", path: "\(filePath) to \(perms)",
                       helperArgs: ["ch", filePath, String(perms)],
                       verify: {
                           let now = (try? fileManager.attributesOfItem(atPath: filePath))?[.posixPermissions] as? NSNumber
                           return now?.intValue == perms
                       }) {
            try fileManager.setAttributes([.posixPermissions: NSNumber(value: perms)],
                                          ofItemAtPath: filePath)
        }
    }

    /// tempPath 에 써 둔 것을 `filePath` 자리에 놓는다. 있으면 바꿔 끼운다.
    ///
    /// 편집기·압축이 "원본을 지우고 → 임시 파일을 옮긴다" 순서로 불렀다.
    /// 그러면 옮기기가 실패한 순간 원본은 이미 없다. 여기서는 바꿔 끼우기를
    /// 한 번에 하므로, 실패해도 원본이 남는다.
    @discardableResult
    class func mvtemp(_ filePath: String) -> Bool {
        guard checkAuth(action: "mvtemp", paths: [filePath]) else {
            lastError = "안전장치가 막았습니다."; return false
        }
        defer { try? fileManager.removeItem(atPath: tempPath) }
        let ok = attempt("REPLACE", path: filePath,
                         helperArgs: ["mv", tempPath, filePath],
                         verify: { fileManager.fileExists(atPath: filePath) }) {
            if fileManager.fileExists(atPath: filePath) {
                _ = try fileManager.replaceItemAt(URL(fileURLWithPath: filePath),
                                                  withItemAt: URL(fileURLWithPath: tempPath))
            } else {
                try fileManager.moveItem(atPath: tempPath, toPath: filePath)
            }
        }
        return ok
    }

    /// 경로 글자만 다듬는다. 같은 자리인지 견줄 때 쓴다.
    private static func lexical(_ p: String) -> String {
        var s = (p as NSString).standardizingPath
        if s.hasPrefix("/var/") { s = "/private" + s }
        return s.hasSuffix("/") && s.count > 1 ? String(s.dropLast()) : s
    }
}

#if compiler(>=6.0)
extension String: @retroactive Error { }
#else
extension String: Error { }
#endif

func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
	return ((fullPath.count < 19) || (fullPath.substring(toIndex: 19) != "/private/var/mobile"))
}


struct UIWebViewTV: UIViewRepresentable {
    let bounds: CGRect
    let file: String

    func makeUIView(context: Context) -> UIView {
		return ObjCFunctions.initWebView(bounds, file: URL(fileURLWithPath: file)) as! UIView
    }

    func updateUIView(_ uiView: UIView, context: Context) { }
}

extension String {
	var length: Int {
		return count
	}
	
	subscript (i: Int) -> String {
		return self[i ..< i + 1]
	}
	
	func substring(fromIndex: Int) -> String {
		return self[min(fromIndex, length) ..< length]
	}
	
	func substring(toIndex: Int) -> String {
		return self[0 ..< max(0, toIndex)]
	}
	
	subscript (r: Range<Int>) -> String {
		let range = Range(uncheckedBounds: (lower: max(0, min(length, r.lowerBound)), upper: min(length, max(0, r.upperBound))))
		let start = index(startIndex, offsetBy: range.lowerBound)
		let end = index(start, offsetBy: range.upperBound - range.lowerBound)
		return String(self[start ..< end])
	}
}
//https://stackoverflow.com/a/26775912

func stringArrayToString(inputArray: [String]) -> String {
	return inputArray.joined(separator: "\n")
}

extension Text {
    init(localizedString: String) {
        self = Text(LocalizedString(localizedString))
        return
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
            .font(.system(size: 80))
    }
}
