//
//  FileGuard.swift
//  Argo
//

import Foundation

enum GuardError: LocalizedError {
    case forbidden(String)
    case safeModeActive(String)
    
    var errorDescription: String? {
        switch self {
        case .forbidden(let msg), .safeModeActive(let msg):
            return msg
        }
    }
}

enum FileGuard {
    static func authorize(action: String, paths: [String]) -> Result<Void, GuardError> {
        let safeMode = UserDefaults.settings.bool(forKey: "safeModeEnabled")
        
        for path in paths {
            let risk = PathSafety.risk(of: path)
            if risk == .forbidden {
                return .failure(.forbidden("시스템 폴더 — 쓰기가 차단됩니다: '\(URL(fileURLWithPath: path).lastPathComponent)'"))
            }
            if safeMode && risk == .caution {
                return .failure(.safeModeActive("안전 모드 — 시스템에 영향을 줄 수 있습니다: '\(URL(fileURLWithPath: path).lastPathComponent)'"))
            }
        }
        return .success(())
    }
}