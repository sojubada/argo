//
//  PathSafety.swift
//  Argo
//

import Foundation
import SwiftUI

enum PathRisk {
    case safe
    case caution
    case forbidden

    var badgeSymbol: String? {
        switch self {
        case .safe: return nil
        case .caution: return "exclamationmark.triangle.fill"
        case .forbidden: return "lock.fill"
        }
    }

    var badgeColor: Color {
        switch self {
        case .safe: return .primary
        case .caution: return .orange
        case .forbidden: return .red
        }
    }
}

enum PathSafety {
    private static let forbiddenPrefixes = [
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/Library",
        "/Applications",
        "/private/var/db",
        "/private/var/containers",
        "/private/var/installd",
        "/private/var/MobileSoftwareUpdate",
        "/dev",
        "/cores",
        "/private/var/preferences",
        "/private/var/keybags"
    ]

    private static let safePrefixes = [
        "/private/var/mobile/Media",
        "/private/var/mobile/Documents",
        "/private/var/mobile/Downloads",
        NSTemporaryDirectory().trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    ]

    static func risk(of path: String) -> PathRisk {
        // 루트 자체는 절대 금지
        if path == "/" || path == "/private" || path == "/private/var" {
            return .forbidden
        }

        let normalized = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        let testPath = normalized.hasSuffix("/") ? normalized : normalized + "/"

        // 앱 자신의 번들 보호
        let bundlePath = Bundle.main.bundlePath
        if testPath.hasPrefix(bundlePath + "/") || bundlePath.hasPrefix(testPath) {
            return .forbidden
        }

        for f in forbiddenPrefixes {
            let fPath = f + "/"
            if testPath.hasPrefix(fPath) { return .forbidden }
        }

        for s in safePrefixes {
            let sPath = s + "/"
            if testPath.hasPrefix(sPath) { return .safe }
        }

        return .caution
    }

    static func reason(for path: String) -> String {
        let r = risk(of: path)
        switch r {
        case .safe: return "Safe directory."
        case .caution: return "User or system area. Proceed with caution."
        case .forbidden: return "Core system or app bundle area. Writing is blocked."
        }
    }

    static func isWriteAllowed(_ path: String) -> Bool {
        let r = risk(of: path)
        if r == .forbidden { return false }
        if r == .caution {
            return !UserDefaults.settings.bool(forKey: "safeModeEnabled") // 안전모드면 주의구역 쓰기 불가
        }
        return true
    }
    
    // UI 스크롤 성능을 위한 빠른 상속 판정
    static func fastRisk(parentRisk: PathRisk, fileName: String) -> PathRisk {
        if parentRisk == .safe || parentRisk == .forbidden {
            return parentRisk // 자식은 부모의 속성을 물려받음
        }
        return .caution
    }
}