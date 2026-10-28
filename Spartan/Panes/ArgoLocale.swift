//
//  ArgoLocale.swift
//  Argo
//
//  화면 언어.
//
//  처음에는 영어로 뜬다. 설정에서 한국어로 바꿀 수 있다.
//
//  우리가 쓴 문구는 한국어가 원문이다. L("…") 이 그 한국어를 열쇠로 삼아
//  고른 언어의 Localizable.strings 에서 찾고, 없으면 원문을 그대로 보여 준다.
//  원래 Spartan 의 문구는 영어 열쇠("DISMISS")라 NSLocalizedString 으로 간다.
//  두 길이 같은 언어를 보도록 Bundle.main 의 찾기를 여기로 돌린다.
//

import Foundation

enum ArgoLocale {
    /// 고를 수 있는 언어. 표에 하나 더 넣고 .lproj 를 더하면 늘어난다.
    static let supported: [(code: String, name: String)] = [
        ("en", "English"),
        ("ko", "한국어"),
    ]

    private static let key = "appLanguage"

    static var current: String {
        get {
            let v = UserDefaults.standard.string(forKey: key) ?? "en"
            return supported.contains(where: { $0.code == v }) ? v : "en"
        }
        set {
            UserDefaults.standard.set(newValue, forKey: key)
            // 다음 실행부터는 시스템이 고르는 언어도 같게 둔다.
            UserDefaults.standard.set([newValue], forKey: "AppleLanguages")
            bundleCache = [:]
        }
    }

    private static var bundleCache: [String: Bundle] = [:]

    static func bundle(_ code: String) -> Bundle? {
        if let b = bundleCache[code] { return b }
        guard let path = Bundle.main.path(forResource: code, ofType: "lproj"),
              let b = Bundle(path: path) else { return nil }
        bundleCache[code] = b
        return b
    }

    /// 앱이 뜰 때 한 번. 화면이 처음 그려지기 전에 불러야 한다.
    static func install() {
        UserDefaults.standard.set([current], forKey: "AppleLanguages")
        object_setClass(Bundle.main, ArgoLocalizedBundle.self)
    }

    /// 고른 언어의 표에서 찾는다. 없으면 영어 표, 그래도 없으면 열쇠 그대로.
    static func lookup(_ key: String, table: String?) -> String {
        let missing = "\u{1}__argo_missing__"
        if let b = bundle(current) {
            let v = b.localizedString(forKey: key, value: missing, table: table)
            if v != missing { return v }
        }
        if current != "en", let b = bundle("en") {
            // 한국어를 골랐을 때 영어 표로 내려가면 안 되는 것: 한국어가 원문인
            // 문구다. 열쇠에 한글이 있으면 원문을 그대로 쓴다.
            if key.unicodeScalars.contains(where: { (0xAC00...0xD7A3).contains($0.value) }) { return key }
            let v = b.localizedString(forKey: key, value: missing, table: table)
            if v != missing { return v }
        }
        return key
    }
}

/// Bundle.main 의 찾기만 바꾼다. NSLocalizedString 과 SwiftUI 의 문구가 모두 이리로 온다.
final class ArgoLocalizedBundle: Bundle {
    override func localizedString(forKey key: String, value: String?, table tableName: String?) -> String {
        let found = ArgoLocale.lookup(key, table: tableName)
        if found == key, let value, !value.isEmpty { return value }
        return found
    }
}

/// 한국어 원문 문구.
func L(_ korean: String) -> String {
    ArgoLocale.lookup(korean, table: nil)
}

/// 값이 끼는 문구. 원문의 {0} {1} … 자리에 값을 넣는다. 언어마다 자리 순서가
/// 달라도 되도록 번호로 가리킨다.
func LF(_ korean: String, _ args: Any...) -> String {
    var s = L(korean)
    for (i, a) in args.enumerated() {
        s = s.replacingOccurrences(of: "{\(i)}", with: "\(a)")
    }
    return s
}
