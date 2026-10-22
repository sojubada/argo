//
//  OperationLog.swift
//  Argo
//

import Foundation

enum OperationLog {
    static let logPath = "/private/var/mobile/Media/argo-operations.log"
    static let fallbackLogPath = (NSSearchPathForDirectoriesInDomains(.documentDirectory, .userDomainMask, true).first ?? NSTemporaryDirectory()) + "/argo-operations.log"

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

        var targetPath = logPath
        if !FileManager.default.fileExists(atPath: targetPath) {
            FileManager.default.createFile(atPath: targetPath, contents: nil, attributes: nil)
        }
        
        if let handle = FileHandle(forWritingAtPath: targetPath) {
            handle.seekToEndOfFile()
            handle.write(data)
            handle.closeFile()
        } else {
            // 폴백
            if !FileManager.default.fileExists(atPath: fallbackLogPath) {
                FileManager.default.createFile(atPath: fallbackLogPath, contents: nil, attributes: nil)
            }
            if let fHandle = FileHandle(forWritingAtPath: fallbackLogPath) {
                fHandle.seekToEndOfFile()
                fHandle.write(data)
                fHandle.closeFile()
            }
        }
    }
}