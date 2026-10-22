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
        // 테마 하나에서 옵니다. 기본 .orange / .red 는 TV 바탕에서
        // 너무 쨍해 눈이 피곤합니다.
        switch self {
        case .safe: return Theme.textSecondary
        case .caution: return Theme.riskCaution
        case .forbidden: return Theme.riskForbidden
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
    ].map { $0.lowercased() }

    private static let safePrefixes = [
        "/private/var/mobile/Media",
        "/private/var/mobile/Documents",
        "/private/var/mobile/Downloads",
        (NSTemporaryDirectory() as NSString).standardizingPath
    ].map { $0.lowercased() }

    static func risk(of path: String) -> PathRisk {
        let lowerPath = path.lowercased()
        if lowerPath == "/" || lowerPath == "/private" || lowerPath == "/private/var" {
            return .forbidden
        }

        let normalized = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        let testPath = (normalized.hasSuffix("/") ? normalized : normalized + "/").lowercased()

        let bundlePath = Bundle.main.bundlePath.lowercased()
        if testPath.hasPrefix(bundlePath + "/") || testPath == bundlePath {
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
    static func fastRisk(parentRisk: PathRisk, fullPath: String, isSymlink: Bool) -> PathRisk {
        if isSymlink {
            return risk(of: fullPath)
        }
        if parentRisk == .safe {
            return .safe
        }
        return risk(of: fullPath)
    }
}