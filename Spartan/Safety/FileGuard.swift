//
//  FileGuard.swift
//  Argo
//

import Foundation
import libarchiveBridge

extension Notification.Name {
    static let fileGuardBlocked = Notification.Name("ArgoFileGuardBlockedNotification")
}

enum GuardError: LocalizedError {
    case forbidden(String)
    case safeModeActive(String)

    var errorDescription: String? {
        switch self {
        case .forbidden(let msg), .safeModeActive(let msg):
            return msg
        }
    }

    var isSafeMode: Bool {
        if case .safeModeActive = self { return true }
        return false
    }
}

enum FileGuard {
    private static let lock = NSLock()
    private static var pendingMessages: [String] = []
    private static var pendingCount = 0
    private static var flushWorkItem: DispatchWorkItem?
    private static var firstBlockTime: Date?

    /// 쌓아둘 메시지 상한. 건수는 따로 세므로 요약은 정확하다.
    private static let messageCap = 20
    /// 디바운스 간격. 연달아 막힐 때 알림을 한 번으로 묶는다.
    private static let debounceInterval: TimeInterval = 0.25
    /// 첫 차단으로부터 이 시간이 지나면 더 기다리지 않고 보낸다.
    /// 디바운스가 호출마다 리셋되므로, 이것이 없으면 긴 일괄 작업 내내 아무 알림도 뜨지 않는다.
    private static let maxDeferral: TimeInterval = 2.0

    /// 모든 파괴적 쓰기 작업(FileManager 및 RootHelperActs)의 단일 승인 관문
    static func authorize(action: String, paths: [String], notifyOnBlock: Bool = true) -> Result<Void, GuardError> {
        let safeMode = UserDefaults.settings.bool(forKey: "safeModeEnabled")

        for path in paths {
            // 심볼릭 링크 자체를 지우거나 옮길 때는 링크가 가리키는 대상이 아니라 '링크가 놓인 부모 폴더' 기준으로 판정
            let targetForCheck = effectivePathForWriteCheck(path)
            let risk = PathSafety.risk(of: targetForCheck)
            let itemName = (path as NSString).lastPathComponent

            if risk == .forbidden {
                let err = GuardError.forbidden("시스템 폴더 — 쓰기가 차단됩니다: '\(itemName)'")
                if notifyOnBlock {
                    reportBlocked(err.localizedDescription)
                }
                return .failure(err)
            }
            if safeMode && risk == .caution {
                let err = GuardError.safeModeActive("안전 모드 — 안전 구역 밖의 쓰기가 차단됩니다: '\(itemName)'")
                if notifyOnBlock {
                    reportBlocked(err.localizedDescription)
                }
                return .failure(err)
            }
        }
        return .success(())
    }

    /// 백그라운드/다중 호출 시 메인 스레드에서 1회로 묶어 알림 발송
    static func reportBlocked(_ message: String) {
        lock.lock()
        pendingCount += 1
        if pendingMessages.count < messageCap {
            pendingMessages.append(message)
        }

        let now = Date()
        let start = firstBlockTime ?? now
        if firstBlockTime == nil { firstBlockTime = now }

        // 첫 차단으로부터 maxDeferral이 지났으면 더 미루지 않는다.
        let elapsed = now.timeIntervalSince(start)
        let delay = min(debounceInterval, max(0, maxDeferral - elapsed))

        flushWorkItem?.cancel()

        let work = DispatchWorkItem {
            lock.lock()
            let msgs = pendingMessages
            let count = pendingCount
            pendingMessages.removeAll()
            pendingCount = 0
            firstBlockTime = nil
            lock.unlock()

            guard let first = msgs.first else { return }
            let finalMessage: String
            if count == 1 {
                finalMessage = first
            } else {
                finalMessage = "\(count)개 작업이 차단되었습니다.\n(\(first))"
            }
            NotificationCenter.default.post(name: .fileGuardBlocked, object: finalMessage)
        }
        flushWorkItem = work
        lock.unlock()

        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    /// 심볼릭 링크 자체에 대한 작업일 경우 링크 대상을 따라가지 않고 부모 경로를 검사하도록 보정
    private static func effectivePathForWriteCheck(_ rawPath: String) -> String {
        var trimmed = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        while trimmed.count > 1 && trimmed.hasSuffix("/") {
            trimmed.removeLast()
        }
        if let attrs = try? FileManager.default.attributesOfItem(atPath: trimmed),
           let type = attrs[.type] as? FileAttributeType,
           type == .typeSymbolicLink {
            let parent = (trimmed as NSString).deletingLastPathComponent
            let resolvedParent = URL(fileURLWithPath: parent).resolvingSymlinksInPath().path
            return (resolvedParent as NSString).appendingPathComponent((trimmed as NSString).lastPathComponent)
        }
        return trimmed
    }

    // MARK: - S4: Zip Slip & Malicious Symlink Pre-Check (libarchive)

    /// 압축을 풀기 전에 한 바이트도 디스크에 쓰지 않고 헤더만 순회하여 Zip Slip 및 외부 심볼릭 링크를 차단
    static func validateArchiveBeforeExtract(archivePath: String, destinationDir: String) -> Result<Void, GuardError> {
        switch authorize(action: "extract", paths: [destinationDir], notifyOnBlock: true) {
        case .failure(let err):
            return .failure(err)
        case .success:
            break
        }

        let resolvedDest = URL(fileURLWithPath: destinationDir).resolvingSymlinksInPath().path
        let destPrefix = resolvedDest.hasSuffix("/") ? resolvedDest : (resolvedDest + "/")

        guard let cPath = archivePath.cString(using: .utf8) else {
            return refuseUncheckable("경로를 읽을 수 없습니다")
        }

        guard let archive = archive_read_new() else {
            return refuseUncheckable("검사기를 준비할 수 없습니다")
        }
        defer {
            archive_read_close(archive)
            archive_read_free(archive)
        }

        archive_read_support_format_all(archive)
        archive_read_support_filter_all(archive)

        guard archive_read_open_filename(archive, cPath, 10240) == ARCHIVE_OK else {
            return refuseUncheckable("압축 파일을 열 수 없습니다")
        }

        var entry: OpaquePointer? = nil
        var readStatus = archive_read_next_header(archive, &entry)
        while readStatus == ARCHIVE_OK {
            guard let currentEntry = entry else {
                archive_read_data_skip(archive)
                continue
            }

            if let rawNamePtr = archive_entry_pathname(currentEntry) {
                let entryName = String(cString: rawNamePtr)

                // 1. 절대 경로 항목 차단
                if entryName.hasPrefix("/") {
                    let msg = "압축 해제 차단 — 절대 경로 항목이 포함되어 있습니다: \(entryName)"
                    reportBlocked(msg)
                    return .failure(.forbidden(msg))
                }

                // 2. 상대 경로(..) 정규화 후 목적지 폴더 이탈 여부 검사
                let combined = (destPrefix as NSString).appendingPathComponent(entryName)
                let standardized = (combined as NSString).standardizingPath
                let standardizedWithSlash = standardized.hasSuffix("/") ? standardized : (standardized + "/")

                if standardized != resolvedDest && !standardizedWithSlash.hasPrefix(destPrefix) {
                    let msg = "압축 해제 차단 (Zip Slip) — 대상 폴더 밖으로 벗어나는 경로입니다: \(entryName)"
                    reportBlocked(msg)
                    return .failure(.forbidden(msg))
                }

                // 3. 링크 항목이 목적지 밖을 가리키는지 검사.
                //    심볼릭 링크(AE_IFLNK)와 하드링크는 libarchive에서 서로 다른 필드로 들어오므로
                //    둘 다 봐야 한다. 하드링크만 빠뜨리면 같은 방법으로 빠져나갈 수 있다.
                let links: [(String, UnsafePointer<CChar>?)] = [
                    ("심볼릭 링크", archive_entry_symlink(currentEntry)),
                    ("하드링크", archive_entry_hardlink(currentEntry))
                ]
                for (kind, ptr) in links {
                    guard let ptr = ptr else { continue }
                    let linkTarget = String(cString: ptr)
                    if linkTarget.isEmpty { continue }
                    if linkTarget.hasPrefix("/") {
                        let msg = "압축 해제 차단 — 외부 절대 경로를 가리키는 \(kind)가 있습니다: \(entryName) -> \(linkTarget)"
                        reportBlocked(msg)
                        return .failure(.forbidden(msg))
                    }
                    let linkParent = (standardized as NSString).deletingLastPathComponent
                    let resolvedTarget = ((linkParent as NSString).appendingPathComponent(linkTarget) as NSString).standardizingPath
                    let resolvedTargetSlash = resolvedTarget.hasSuffix("/") ? resolvedTarget : (resolvedTarget + "/")
                    if resolvedTarget != resolvedDest && !resolvedTargetSlash.hasPrefix(destPrefix) {
                        let msg = "압축 해제 차단 — 대상 폴더 밖을 가리키는 \(kind)가 있습니다: \(entryName) -> \(linkTarget)"
                        reportBlocked(msg)
                        return .failure(.forbidden(msg))
                    }
                }
            }
            archive_read_data_skip(archive)
            readStatus = archive_read_next_header(archive, &entry)
        }

        // ARCHIVE_EOF(1)로 끝났을 때만 '전부 검사했다'고 말할 수 있다.
        // ARCHIVE_WARN(-20)/ARCHIVE_FATAL(-30)로 끊기면 뒤쪽 항목은 보지 못한 것이므로 막는다.
        guard readStatus == ARCHIVE_EOF else {
            return refuseUncheckable("압축 파일이 손상되어 검사를 끝내지 못했습니다")
        }

        return .success(())
    }

    /// 검사 자체가 불가능할 때는 통과시키지 않고 막는다.
    /// 검사는 libarchive, 해제는 minizip이라 둘의 해석이 갈리는 파일 하나로 검사를 통째로
    /// 우회할 수 있다. 보안 검사는 실패하면 닫혀야 한다.
    private static func refuseUncheckable(_ reason: String) -> Result<Void, GuardError> {
        let msg = "압축 해제 차단 — \(reason). 검사할 수 없는 압축 파일은 풀지 않습니다."
        reportBlocked(msg)
        return .failure(.forbidden(msg))
    }
}
