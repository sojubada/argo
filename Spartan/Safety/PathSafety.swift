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
        "/var/db",
        "/var/containers",
        "/var/installd",
        "/var/MobileSoftwareUpdate",
        "/dev",
        "/cores",
        "/var/preferences",
        "/var/keybags"
    ].map { canonical($0) }

    /// 사용자의 집. 원본 Spartan 이 쓰기 가능으로 본 범위와 같다 —
    /// filePathIsNotMobileWritable 이 /private/var/mobile 아래를 전부 허용했다.
    ///
    /// 전에는 Media·Documents·Downloads 셋만 넣었는데, 그러면 Library 나
    /// 사용자가 만든 폴더가 전부 '주의'가 되고 안전 모드가 기본 켜짐이라
    /// 자기 파일도 못 지운다. 벽돌을 막는 것은 위의 금지 목록이지
    /// 집 안을 좁히는 것이 아니다.
    private static let safePrefixes = [
        "/var/mobile"
        // NSTemporaryDirectory() 를 넣어 두었다가 뺐다. 컨테이너가 없는
        // (no-container) 앱에서 무엇을 돌려주는지 확인한 적이 없고,
        // 컨테이너 tmp 라면 /var/mobile 규칙으로 이미 통과한다.
        // 확인하지 않은 값을 안전 목록에 넣는 것은 그 자체로 구멍이다.
    ].map { canonical($0) }

    /// 경로를 비교할 수 있는 한 가지 모양으로 만든다.
    ///
    /// resolvingSymlinksInPath() 는 **/private 접두사를 떼어낸다** (문서화된 동작).
    /// 그래서 목록을 /private/var/... 로 적어 두면 어느 것과도 안 맞는다 —
    /// 안전 구역이 전부 '주의'로 떨어지고, 안전 모드가 모든 쓰기를 막았다.
    /// 삭제가 안 되던 이유다.
    private static func canonical(_ path: String) -> String {
        var p = path
        if p.hasPrefix("/private/") { p = String(p.dropFirst("/private".count)) }
        while p.count > 1 && p.hasSuffix("/") { p.removeLast() }
        return p.lowercased()
    }

    static func risk(of path: String) -> PathRisk {
        let lowerPath = path.lowercased()
        // 최상위와 /var 자체는 쓰기 금지. 안을 둘러보는 것은 된다.
        let bare = canonical(lowerPath)
        if bare == "/" || bare == "" || bare == "/var" || bare == "/private" {
            return .forbidden
        }

        let normalized = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
        let testPath = canonical(normalized) + "/"

        let bundlePath = canonical(Bundle.main.bundlePath)
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