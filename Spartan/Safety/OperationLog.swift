//
//  OperationLog.swift
//  Argo
//

import Foundation

enum OperationLog {
    static let primaryDir = "/private/var/mobile/Media/"
    static let fileName = "argo-operations.log"

    /// 로그 상한. 넘으면 뒤쪽 절반만 남긴다.
    /// 애플TV 저장공간은 넉넉하지 않고, 파일을 많이 다루면 이 파일만 끝없이 자란다.
    private static let maxLogBytes = 1_048_576

    /// 기본 경로에 한 번 실패하면 세션 내내 폴백만 쓴다.
    /// 매번 createFile을 다시 시도하면 작업마다 헛된 디스크 접근이 쌓인다.
    private static var primaryUnusable = false

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

        if !primaryUnusable {
            if appendData(data, toPath: logPath) {
                return
            }
            primaryUnusable = true
        }
        _ = appendData(data, toPath: fallbackLogPath)
    }

    /// 상한을 넘으면 뒤쪽 절반만 남기고 줄 경계에서 자른다.
    private static func truncateIfOversized(_ targetPath: String) {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: targetPath),
              let size = attrs[.size] as? Int, size > maxLogBytes else { return }
        guard let handle = FileHandle(forReadingAtPath: targetPath) else { return }
        handle.seek(toFileOffset: UInt64(size - maxLogBytes / 2))
        let tail = handle.readDataToEndOfFile()
        handle.closeFile()

        // 잘린 첫 줄은 버려서 반쪽 줄이 남지 않게 한다.
        var kept = tail
        if let nl = tail.firstIndex(of: 0x0A), nl + 1 < tail.count {
            kept = tail.subdata(in: (nl + 1)..<tail.count)
        }
        let header = "--- 이전 기록은 용량 상한으로 잘렸습니다 ---\n".data(using: .utf8) ?? Data()
        try? (header + kept).write(to: URL(fileURLWithPath: targetPath), options: .atomic)
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
        truncateIfOversized(targetPath)
        return true
    }
}