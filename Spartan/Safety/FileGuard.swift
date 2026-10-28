//
//  FileGuard.swift
//  Argo
//

import Foundation
import libarchiveBridge

extension Notification.Name {
    static let fileGuardBlocked = Notification.Name("ArgoFileGuardBlockedNotification")
}

/// 안전판 · 모험가판.
///
/// 같은 코드에서 두 IPA 를 낸다. 빌드할 때 ARGO_SAFE 를 주면 안전판이다.
/// - 안전판: 안전 모드가 늘 켜져 있고 끌 수 없다.
/// - 모험가판: 안전 모드를 설정에서 끌 수 있다(지금까지의 동작).
/// 시스템 폴더 쓰기 금지는 두 판 모두 같다 — 그것은 벽돌을 막는 선이다.
enum ArgoEdition {
    #if ARGO_SAFE
    static let isSafe = true
    #else
    static let isSafe = false
    #endif

    static var name: String { isSafe ? L("안전판") : L("모험가판") }

    /// 지금 안전 모드인가. 안전판은 설정값과 상관없이 늘 켜짐이다.
    static var safeMode: Bool {
        isSafe || UserDefaults.settings.bool(forKey: "safeModeEnabled")
    }
}

enum GuardError: LocalizedError {
    case forbidden(String)
    case safeModeActive(String)
    case notEnoughSpace(String)

    var errorDescription: String? {
        switch self {
        case .forbidden(let msg), .safeModeActive(let msg), .notEnoughSpace(let msg):
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
        let safeMode = ArgoEdition.safeMode

        for path in paths {
            // 심볼릭 링크 자체를 지우거나 옮길 때는 링크가 가리키는 대상이 아니라 '링크가 놓인 부모 폴더' 기준으로 판정
            let targetForCheck = effectivePathForWriteCheck(path)
            let risk = PathSafety.risk(of: targetForCheck)
            let itemName = (path as NSString).lastPathComponent

            if risk == .forbidden {
                let err = GuardError.forbidden(LF("시스템 폴더 — 쓰기가 차단됩니다: '{0}'", itemName))
                if notifyOnBlock {
                    reportBlocked(err.localizedDescription)
                }
                return .failure(err)
            }
            if safeMode && risk == .caution {
                let err = GuardError.safeModeActive(LF("안전 모드 — 안전 구역 밖의 쓰기가 차단됩니다: '{0}'", itemName))
                if notifyOnBlock {
                    reportBlocked(err.localizedDescription)
                }
                return .failure(err)
            }
            // 안전 모드에서 폴더를 지울 때는 안까지 본다. 잠긴 것이
            // 하나라도 있으면 통째로 막는다 — 폴더 하나 지우려다 그 안의
            // 시스템 파일이 같이 사라지면 안 된다.
            if safeMode && Self.destructive.contains(action),
               let locked = lockedDescendant(in: targetForCheck) {
                let err = GuardError.safeModeActive(LF("안전 모드 — 안에 잠긴 항목이 있어 막았습니다: '{0}' ({1})", itemName, locked))
                if notifyOnBlock {
                    reportBlocked(err.localizedDescription)
                }
                return .failure(err)
            }
        }
        return .success(())
    }

    /// 안까지 살펴볼 작업. 지우기뿐이다 — 이름 바꾸기·옮기기도 mv 를 타지만
    /// 그것까지 막으면 root 파일이 하나 든 폴더는 이름도 못 바꾼다.
    private static let destructive: Set<String> = ["rm", "delete", "trash", "batch_delete"]

    /// 폴더 안의 잠긴 항목을 찾는다. 없으면 nil, 있으면 무엇이 왜 잠겼는지.
    ///
    /// 잠금으로 치는 것:
    /// - 시스템 폴더를 가리키는 링크 (목록에 자물쇠가 붙는 것)
    /// - 바꿀 수 없게 표시된 파일 (immutable · append-only 플래그)
    /// - root 가 가진 파일 — 사용자 것이 아니다
    /// 너무 커서 정해진 시간 안에 다 보지 못하면 그것도 막는다. 다 보지
    /// 못한 것을 괜찮다고 할 수는 없다.
    static func lockedDescendant(in path: String) -> String? {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: path, isDirectory: &isDir), isDir.boolValue else {
            return nil
        }
        // 링크 자체를 지우는 것이면 안을 볼 일이 없다.
        if let t = (try? FileManager.default.attributesOfItem(atPath: path))?[.type] as? FileAttributeType,
           t == .typeSymbolicLink {
            return nil
        }
        let keys: [URLResourceKey] = [.isSymbolicLinkKey]
        guard let walker = FileManager.default.enumerator(at: URL(fileURLWithPath: path),
                                                          includingPropertiesForKeys: keys,
                                                          options: [], errorHandler: { _, _ in true }) else {
            return nil
        }
        let started = Date()
        var seen = 0
        while let url = walker.nextObject() as? URL {
            seen += 1
            if seen > 50_000 || Date().timeIntervalSince(started) > 3 {
                return L("너무 커서 다 살펴보지 못했습니다")
            }
            let p = url.path
            let name = url.lastPathComponent
            if let v = try? url.resourceValues(forKeys: Set(keys)), v.isSymbolicLink == true {
                if PathSafety.risk(of: p) == .forbidden {
                    return LF("{0}: 시스템 폴더를 가리킵니다", name)
                }
                continue
            }
            guard let a = try? FileManager.default.attributesOfItem(atPath: p) else { continue }
            if (a[.immutable] as? Bool) == true || (a[.appendOnly] as? Bool) == true {
                return LF("{0}: 바꿀 수 없게 잠겨 있습니다", name)
            }
            // 앱이 root 로 돈다면 앱이 만든 것도 root 것이라 이 기준은 쓸 수 없다.
            if getuid() != 0, let owner = a[.ownerAccountID] as? NSNumber, owner.intValue == 0 {
                return LF("{0}: 시스템(root) 소유입니다", name)
            }
        }
        return nil
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
                finalMessage = LF("{0}개 작업이 차단되었습니다.\n({1})", count, first)
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
            return refuseUncheckable(L("경로를 읽을 수 없습니다"))
        }

        guard let archive = archive_read_new() else {
            return refuseUncheckable(L("검사기를 준비할 수 없습니다"))
        }
        defer {
            archive_read_close(archive)
            archive_read_free(archive)
        }

        archive_read_support_format_all(archive)
        archive_read_support_filter_all(archive)

        guard archive_read_open_filename(archive, cPath, 10240) == ARCHIVE_OK else {
            return refuseUncheckable(L("압축 파일을 열 수 없습니다"))
        }

        // 헤더를 어차피 훑으므로 풀었을 때의 크기를 같이 센다 — 공짜다.
        var totalUncompressed: Int64 = 0

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
                    let msg = LF("압축 해제 차단 — 절대 경로 항목이 포함되어 있습니다: {0}", entryName)
                    reportBlocked(msg)
                    return .failure(.forbidden(msg))
                }

                // 2. 상대 경로(..) 정규화 후 목적지 폴더 이탈 여부 검사
                let combined = (destPrefix as NSString).appendingPathComponent(entryName)
                let standardized = (combined as NSString).standardizingPath
                let standardizedWithSlash = standardized.hasSuffix("/") ? standardized : (standardized + "/")

                if standardized != resolvedDest && !standardizedWithSlash.hasPrefix(destPrefix) {
                    let msg = LF("압축 해제 차단 (Zip Slip) — 대상 폴더 밖으로 벗어나는 경로입니다: {0}", entryName)
                    reportBlocked(msg)
                    return .failure(.forbidden(msg))
                }

                // 3. 링크 항목이 목적지 밖을 가리키는지 검사.
                //    심볼릭 링크(AE_IFLNK)와 하드링크는 libarchive에서 서로 다른 필드로 들어오므로
                //    둘 다 봐야 한다. 하드링크만 빠뜨리면 같은 방법으로 빠져나갈 수 있다.
                let links: [(String, UnsafePointer<CChar>?)] = [
                    (L("심볼릭 링크"), archive_entry_symlink(currentEntry)),
                    (L("하드링크"), archive_entry_hardlink(currentEntry))
                ]
                for (kind, ptr) in links {
                    guard let ptr = ptr else { continue }
                    let linkTarget = String(cString: ptr)
                    if linkTarget.isEmpty { continue }
                    if linkTarget.hasPrefix("/") {
                        let msg = LF("압축 해제 차단 — 외부 절대 경로를 가리키는 {0}가 있습니다: {1} -> {2}", kind, entryName, linkTarget)
                        reportBlocked(msg)
                        return .failure(.forbidden(msg))
                    }
                    let linkParent = (standardized as NSString).deletingLastPathComponent
                    let resolvedTarget = ((linkParent as NSString).appendingPathComponent(linkTarget) as NSString).standardizingPath
                    let resolvedTargetSlash = resolvedTarget.hasSuffix("/") ? resolvedTarget : (resolvedTarget + "/")
                    if resolvedTarget != resolvedDest && !resolvedTargetSlash.hasPrefix(destPrefix) {
                        let msg = LF("압축 해제 차단 — 대상 폴더 밖을 가리키는 {0}가 있습니다: {1} -> {2}", kind, entryName, linkTarget)
                        reportBlocked(msg)
                        return .failure(.forbidden(msg))
                    }
                }
            }
            if let currentEntry = entry {
                let size = archive_entry_size(currentEntry)
                if size > 0 { totalUncompressed += Int64(size) }
            }
            archive_read_data_skip(archive)
            readStatus = archive_read_next_header(archive, &entry)
        }

        // ARCHIVE_EOF(1)로 끝났을 때만 '전부 검사했다'고 말할 수 있다.
        // ARCHIVE_WARN(-20)/ARCHIVE_FATAL(-30)로 끊기면 뒤쪽 항목은 보지 못한 것이므로 막는다.
        guard readStatus == ARCHIVE_EOF else {
            return refuseUncheckable(L("압축 파일이 손상되어 검사를 끝내지 못했습니다"))
        }

        // 공간이 모자라면 시작하지 않는다. 반쯤 풀린 상태가 가장 수습하기 어렵다.
        if totalUncompressed > 0 {
            if case .failure(let err) = checkFreeSpace(needed: totalUncompressed,
                                                       at: resolvedDest,
                                                       what: L("압축을 풀기")) {
                return .failure(err)
            }
        }

        return .success(())
    }

    // MARK: - 빈 공간

    /// 애플TV는 저장공간이 넉넉하지 않다. 10GB 파일을 다루면 원본과 결과가
    /// 동시에 올라가므로, 시작 전에 들어갈 자리가 있는지 본다.
    /// 여유 10%를 둔다 — 파일시스템 메타데이터와 임시 파일 몫이다.
    static func checkFreeSpace(needed: Int64,
                               at path: String,
                               what: String) -> Result<Void, GuardError> {
        guard let attrs = try? FileManager.default.attributesOfFileSystem(forPath: path),
              let free = (attrs[.systemFreeSize] as? NSNumber)?.int64Value else {
            // 알 수 없으면 막지 않는다. 이것은 안전 검사가 아니라 편의 검사다 —
            // 막아야 할 쓰기를 놓치는 것이 아니라 실패를 미리 알려 주는 것이다.
            return .success(())
        }

        let withMargin = needed + needed / 10
        guard free < withMargin else { return .success(()) }

        let f = ByteCountFormatter()
        f.countStyle = .file
        let msg = LF("{0}에 약 {1} 가 필요하지만 ", what, f.string(fromByteCount: withMargin))
                + LF("남은 공간은 {0} 입니다.", f.string(fromByteCount: free))
        reportBlocked(msg)
        return .failure(.notEnoughSpace(msg))
    }

    /// 압축하기 전. 원본 합계만큼은 있어야 한다 (압축이 전혀 안 되는 경우 기준).
    static func authorizeCompression(sources: [String],
                                     destination: String) -> Result<Void, GuardError> {
        if case .failure(let err) = authorize(action: "compress", paths: [destination]) {
            return .failure(err)
        }

        var total: Int64 = 0
        for path in sources {
            total += directorySize(of: path)
        }
        guard total > 0 else { return .success(()) }

        let parent = (destination as NSString).deletingLastPathComponent
        return checkFreeSpace(needed: total, at: parent, what: L("압축하기"))
    }

    /// 파일이면 크기, 폴더면 아래 전부의 합.
    private static func directorySize(of path: String) -> Int64 {
        let fm = FileManager.default
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: path, isDirectory: &isDir) else { return 0 }

        if !isDir.boolValue {
            let attrs = try? fm.attributesOfItem(atPath: path)
            return (attrs?[.size] as? NSNumber)?.int64Value ?? 0
        }

        var total: Int64 = 0
        guard let e = fm.enumerator(atPath: path) else { return 0 }
        for case let name as String in e {
            let full = (path as NSString).appendingPathComponent(name)
            if let attrs = try? fm.attributesOfItem(atPath: full),
               let size = (attrs[.size] as? NSNumber)?.int64Value {
                total += size
            }
        }
        return total
    }

    /// 검사 자체가 불가능할 때는 통과시키지 않고 막는다.
    /// 검사는 libarchive, 해제는 minizip이라 둘의 해석이 갈리는 파일 하나로 검사를 통째로
    /// 우회할 수 있다. 보안 검사는 실패하면 닫혀야 한다.
    private static func refuseUncheckable(_ reason: String) -> Result<Void, GuardError> {
        let msg = LF("압축 해제 차단 — {0}. 검사할 수 없는 압축 파일은 풀지 않습니다.", reason)
        reportBlocked(msg)
        return .failure(.forbidden(msg))
    }
}
