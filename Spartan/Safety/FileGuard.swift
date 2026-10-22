//
//  FileGuard.swift
//  Argo
//

import Foundation

enum GuardError: Error {
    case forbidden(String)
    case safeModeActive(String)
}

enum FileGuard {
    static func authorize(action: String, paths: [String]) -> Result<Void, GuardError> {
        let safeMode = UserDefaults.settings.bool(forKey: "safeModeEnabled")
        
        for path in paths {
            let risk = PathSafety.risk(of: path)
            if risk == .forbidden {
                return .failure(.forbidden("Blocked: '\(URL(fileURLWithPath: path).lastPathComponent)' is in a forbidden system area."))
            }
            if safeMode && risk == .caution {
                return .failure(.safeModeActive("Safe Mode: Cannot modify '\(URL(fileURLWithPath: path).lastPathComponent)' outside safe zones."))
            }
        }
        return .success(())
    }
}