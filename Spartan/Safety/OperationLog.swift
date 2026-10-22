//
//  OperationLog.swift
//  Argo
//

import Foundation

enum OperationLog {
    static let primaryDir = "/private/var/mobile/Media/"
    static let fileName = "argo-operations.log"

    static var fallbackDir: String {
        let doc = NSSearchPathForDirectoriesInDomains(.documentDirectory, .userDomainMask, true).first ?? NSTemporaryDirectory()
        return doc.hasSuffix("/") ? doc : (doc + "/")
    }

    static var logPath: String {
        return primaryDir + fileName
    }

    static var fallbackLogPath: String {
        return fallbackDir + fileName
    }

    /// SettingsView의 TextView가 열어야 할 실제 로그 폴더 경로 (M5 해결)
    static var activeLogDirectory: String {
        if FileManager.default.fileExists(atPath: logPath) {
            return primaryDir
        }
        if FileManager.default.fileExists(atPath: fallbackLogPath) {
            return fallbackDir
        }
        return primaryDir
    }

    static func record(action: String, path: String, destination: String? = nil) {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        let timestamp = formatter.string(from: Date())

        var logLine = "\(timestamp)  \(action)  \(path)"
        if let dest = destination {
            logLine += "  ->  \(dest)"
        }
        logLine += "\n"

        guard let data = logLine.data(using: .utf8) else { return }

        if appendData(data, toPath: logPath) {
            return
        }
        _ = appendData(data, toPath: fallbackLogPath)
    }

    private static func appendData(_ data: Data, toPath targetPath: String) -> Bool {
        if !FileManager.default.fileExists(atPath: targetPath) {
            guard FileManager.default.createFile(atPath: targetPath, contents: nil, attributes: nil) else {
                return false
            }
        }
        guard let handle = FileHandle(forWritingAtPath: targetPath) else {
            return false
        }
        handle.seekToEndOfFile()
        handle.write(data)
        handle.closeFile()
        return true
    }
}