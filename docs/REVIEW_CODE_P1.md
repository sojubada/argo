# 코드 리뷰 — 1단계 안전장치

> 탐침·외관 코드는 제외했습니다. 실제로 쓰기를 막는 코드만 있습니다.
> 커밋 `99f2434` (`main`) 기준. CI 통과.

---

## 리뷰할 때 봐주실 것

이 묶음의 존재 이유는 하나입니다. **파일 탐색기가 벽돌을 만들지 않게 하는 것.**

그래서 판단 기준도 하나입니다 — **막아야 할 것을 막는가, 막으면 안 될 것을 막지는 않는가.**

### 1. 관문을 지나지 않는 쓰기 경로가 있는가  ★가장 중요★

모든 파괴적 쓰기는 `FileGuard.authorize` 를 지나야 합니다. 새 경로가 생기면 그것도 지나야 합니다.

지금 관문을 지나는 곳:

| 위치 | 무엇 |
| --- | --- |
| `RootHelperActs.checkAuth` | rm · mv · cp · touch · mkdir · ln · chmod · mvtemp (8곳) |
| `TrashManager` | 휴지통 보내기 · 비우기 · 되살리기 · 영구 삭제 |
| `Compression.swift` | 압축 · 압축 풀기 (3곳) |
| `TextView` · `HexView` | 직접 저장 |

**찾아야 할 것: 이 표에 없는데 파일을 쓰는 코드.** `FileManager` 의 `removeItem` · `moveItem` · `copyItem` · `createFile` · `write` 와 `Data.write(to:)` 를 전부 훑어 주십시오.

알려진 예외 하나 — **IPA 설치**는 `trollstorehelper` 와 `apple-magnifier://` 로 가고 관문을 안 거칩니다. 애플 자신의 설치 경로이고 우리가 파일을 쓰는 게 아니라 의도한 것입니다.

### 2. 과차단

막으면 안 될 것을 막으면 앱이 쓸모없어집니다. 특히:

- `/var/mobile/Media/` 아래가 전부 통과하는가
- **심볼릭 링크 자체를 지우는 것**이 막히지 않는가 (`effectivePathForWriteCheck` 가 링크의 **부모**를 보는 이유)
- 안전 모드가 꺼져 있을 때 `.caution` 이 통과하는가

### 3. 미해결 — R2

**`TrashManager.unlinkNestedSymlinksIfDirectory` 가 이 묶음의 유일한 자료 손실 위험입니다.**

삭제가 **실패할 수 있는데 링크를 먼저 지웁니다**. 실패하면 폴더는 남고 링크만 사라집니다. 복구 수단이 없습니다. `robustMove` 도 목적지를 비우려 같은 함수를 부르므로, **옮기기가 실패하면 건드릴 생각도 없던 폴더의 링크만 날아갑니다.**

그리고 **이 방어가 필요한지 자체가 미확인입니다.** 애플 `removefile(3)` 문서는 하위 심볼릭 링크를 따라가는지 아무 말도 하지 않습니다.

실기기 30초 시험이 결론을 냅니다.

```
1. /var/mobile/Media/target.txt 를 만든다
2. /var/mobile/Media/box/ 안에 target.txt 를 가리키는 링크를 만든다 (툴바의 링크 만들기)
3. box 를 휴지통으로 보낸다
4. target.txt 가 살아 있는가?
```

- 살아 있으면 → `removeItem` 이 링크를 안 따라갑니다. **함수를 지우십시오.** 보호 효과 없이 손실 위험만 남습니다
- 사라졌으면 → 필요합니다. 다만 **성공이 확실해진 뒤에** 지우도록 순서를 바꿔야 합니다

어느 쪽이든 `robustMove` 의 목적지 비우기에서는 빼야 합니다.

**그 전까지 심볼릭 링크가 든 폴더를 옮기거나 지우지 마십시오.**

### 4. 루트 헬퍼는 돌지 않습니다 — 알고 계셔야 합니다

`RootHelperActs` 가 `spawn("/bin/RootHelper", ...)` 를 부르지만 **그 바이너리는 실제로 돈 적이 없습니다.**

- `TROLLSTORE` 컴파일 플래그가 **어디에도 정의되어 있지 않습니다** → `isJB` 는 `true`, `helperPath` 는 `/bin/RootHelper`
- 그 바이너리는 **IPA 에 들어 있지 않습니다**
- `/bin` 은 읽기 전용입니다

**즉 모든 쓰기는 `FileManager` 로 갑니다.** `/System` 을 지켜 온 것은 앱이 아니라 POSIX 권한이었습니다.

그래서 안전장치의 핵심은 헬퍼가 아니라 **`FileManager` 경로**입니다. 리뷰도 거기에 집중해 주십시오.

---

## 1. 경로 위험도 — 세 단계로 나눕니다

### `Spartan/Safety/PathSafety.swift`  (114줄)

```swift
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
```

**볼 곳**

- `fastRisk` 가 `.safe` 만 아래로 상속합니다. 한때 `.forbidden` 도 상속해 `/` 의 모든 항목이 빨간 자물쇠가 됐습니다
- 접두사 목록이 `lowercased()` 되어 있습니다. iOS 파일시스템은 대소문자를 구분하지 않으므로 `/SYSTEM/` 도 막혀야 합니다
- 심볼릭 링크는 상속을 건너뛰고 항상 자기 경로로 판정합니다 — 안전한 폴더 안의 링크가 시스템을 가리킬 수 있습니다

---

## 2. 관문 — 모든 쓰기가 지나는 한 곳

### `Spartan/Safety/FileGuard.swift`  (240줄)

```swift
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
```

**볼 곳**

- **`refuseUncheckable`** — 검사할 수 없으면 **막습니다**. 전에는 `.success(())` 를 돌려줘 그대로 풀렸습니다. 검사는 libarchive, 해제는 minizip 이라 **한쪽이 거부하고 다른 쪽이 받아들이는 파일 하나로 검사가 통째로 우회됐습니다**
- **`readStatus == ARCHIVE_EOF`** — `ARCHIVE_OK` 는 0, `ARCHIVE_EOF` 는 1, `ARCHIVE_WARN` 은 -20 입니다. 전에는 정상 종료와 손상을 구분하지 않아, 50개 중 3번째에서 경고가 나면 나머지 47개가 검사 없이 풀렸습니다
- **하드링크** — `archive_entry_symlink` 와 `archive_entry_hardlink` 는 다른 필드입니다. 심볼릭 링크만 보면 같은 우회가 이름만 바꿔 남습니다
- **`effectivePathForWriteCheck`** — 링크 **자체**를 지울 때 링크가 가리키는 곳이 아니라 링크가 **놓인 부모**로 판정합니다. 이게 없으면 시스템을 가리키는 링크를 지우는 것조차 막혀 치울 수 없습니다
- **`pendingMessages` 상한 20 / 건수는 따로** — 큰 폴더가 전부 막혀도 문자열이 쌓이지 않습니다
- **`maxDeferral` 2초** — 디바운스가 호출마다 리셋되므로, 이게 없으면 30초짜리 일괄 작업 내내 알림이 안 뜨다가 끝나고 한 번 뜹니다

**미사용** — `GuardError.isSafeMode` 는 아무 데서도 안 씁니다. 죽은 코드이고 해롭지 않습니다.

---

## 3. 작업 기록

### `Spartan/Safety/OperationLog.swift`  (99줄)

```swift
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
```

**볼 곳**

- 상한 1MB, 넘으면 뒤쪽 절반만 남기고 **줄 경계에서** 자릅니다 (반쪽 줄이 남지 않게)
- `primaryUnusable` — 기본 경로에 한 번 실패하면 세션 내내 폴백만 씁니다. 전에는 작업마다 `createFile` 을 다시 시도했습니다
- `.atomic` 으로 씁니다 — 자르는 중에 죽어도 로그가 깨지지 않습니다

---

## 4. 휴지통 — 실제 삭제가 전부 지나는 곳

이 파일이 가장 길고 **R2 가 여기 있습니다.**

### `Spartan/Trash/TrashManager.swift`  (533줄)

```swift
//
//  TrashManager.swift
//  Argo
//

import Foundation
import Combine

struct TrashEntry: Codable, Identifiable, Hashable {
    let id: String
    let originalPath: String
    let originalName: String
    let deletedAt: Date
    let isDirectory: Bool
    let size: Int64

    var storedPath: String {
        if originalPath.isEmpty {
            return "\(TrashManager.trashRootPath)/\(originalName)"
        }
        return "\(TrashManager.itemsRootPath)/\(id)/\(originalName)"
    }

    var isLegacy: Bool {
        return originalPath.isEmpty
    }

    var deletedAtDescription: String {
        let formatter = DateFormatter()
        let format = UserDefaults.settings.string(forKey: "dateFormat") ?? "MM-dd-yyyy HH:mm"
        formatter.dateFormat = format.isEmpty ? "MM-dd-yyyy HH:mm" : format
        return formatter.string(from: deletedAt)
    }
}

enum TrashRestoreResult {
    case restored(String)
    case renamed(String)
    case failed(String)
}

private struct TrashIndexFile: Codable {
    var version: Int = 1
    var entries: [TrashEntry]
}

final class TrashManager: ObservableObject {
    static let shared = TrashManager()

    static let trashRootPath = "/private/var/mobile/Media/.Trash"
    static let indexFilePath = "/private/var/mobile/Media/.Trash/argo-trash-index.json"
    static let itemsRootPath = "/private/var/mobile/Media/.Trash/items"

    @Published private(set) var entries: [TrashEntry] = []

    var totalSize: Int64 {
        return entries.reduce(0) { $0 + $1.size }
    }

    private init() {
        ensureDirectories()
        reload()
    }

    // MARK: - Symlink Inspection Helper (S6)

    private func isSymbolicLink(atPath path: String) -> Bool {
        guard let attrs = try? fileManager.attributesOfItem(atPath: path),
              let type = attrs[.type] as? FileAttributeType else {
            return false
        }
        return type == .typeSymbolicLink
    }

    /// 디렉터리 삭제 전 내부의 심볼릭 링크만 먼저 끊어내어 재귀 삭제가 링크 대상을 따라가는 것을 원천 차단
    private func unlinkNestedSymlinksIfDirectory(_ dirPath: String) {
        guard !isSymbolicLink(atPath: dirPath) else { return }
        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: dirPath, isDirectory: &isDir), isDir.boolValue else { return }

        let dirURL = URL(fileURLWithPath: dirPath)
        guard let enumerator = fileManager.enumerator(
            at: dirURL,
            includingPropertiesForKeys: [.isSymbolicLinkKey],
            options: [.skipsPackageDescendants]
        ) else { return }

        var symlinkPaths: [String] = []
        for case let itemURL as URL in enumerator {
            if let values = try? itemURL.resourceValues(forKeys: [.isSymbolicLinkKey]),
               values.isSymbolicLink == true {
                symlinkPaths.append(itemURL.path)
                enumerator.skipDescendants()
            }
        }

        for sym in symlinkPaths {
            try? fileManager.removeItem(atPath: sym)
        }
    }

    // MARK: - Dual File Operations (FileGuard + FileManager First + RootHelperActs Fallback)

    private func robustMkdir(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return }
        if fileManager.fileExists(atPath: norm) { return }
        guard case .success = FileGuard.authorize(action: "mkdir", paths: [norm]) else { return }

        do {
            try fileManager.createDirectory(atPath: norm, withIntermediateDirectories: true, attributes: nil)
        } catch {
            RootHelperActs.mkdir(norm)
        }
    }

    private func robustRemove(_ path: String) {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return }
        guard fileManager.fileExists(atPath: norm) || isSymbolicLink(atPath: norm) else { return }
        guard case .success = FileGuard.authorize(action: "rm", paths: [norm]) else { return }

        // S6: 폴더 내부 심볼릭 링크 선제거 (단, 대상 자체가 심볼릭 링크면 내부를 건드리지 않고 링크만 제거)
        unlinkNestedSymlinksIfDirectory(norm)

        do {
            try fileManager.removeItem(atPath: norm)
        } catch {
            RootHelperActs.rm(norm)
        }
        if fileManager.fileExists(atPath: norm) {
            RootHelperActs.rm(norm)
        }
    }

    private func robustMove(from src: String, to dst: String) -> Bool {
        let normSrc = normalizePath(src)
        let normDst = normalizePath(dst)
        guard !normSrc.isEmpty, !normDst.isEmpty else { return false }
        guard fileManager.fileExists(atPath: normSrc) || isSymbolicLink(atPath: normSrc) else { return false }
        guard case .success = FileGuard.authorize(action: "mv", paths: [normSrc, normDst]) else {
            return false
        }

        if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
            robustRemove(normDst)
        }

        // 1차 시도: FileManager 직접 이동
        do {
            try fileManager.moveItem(atPath: normSrc, toPath: normDst)
            if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
                return true
            }
        } catch {
            // 2차 시도: RootHelperActs.mv
            RootHelperActs.mv(normSrc, normDst)
            if fileManager.fileExists(atPath: normDst) || isSymbolicLink(atPath: normDst) {
                return true
            }
        }

        // 3차 시도: 복사 후 원본 삭제 (단, 심볼릭 링크가 아닐 때만)
        if !isSymbolicLink(atPath: normSrc) {
            do {
                try fileManager.copyItem(atPath: normSrc, toPath: normDst)
                if fileManager.fileExists(atPath: normDst) {
                    robustRemove(normSrc)
                    return !fileManager.fileExists(atPath: normSrc)
                }
            } catch {
                RootHelperActs.cp(normSrc, normDst)
                if fileManager.fileExists(atPath: normDst) {
                    robustRemove(normSrc)
                    return !fileManager.fileExists(atPath: normSrc)
                }
            }
        }

        return fileManager.fileExists(atPath: normDst)
    }

    private func ensureDirectories() {
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
    }

    func reload() {
        ensureDirectories()
        let indexed = loadIndexedEntries()
        let legacy = scanLegacyEntries()
        let combined = indexed + legacy
        entries = combined.sorted { $0.deletedAt > $1.deletedAt }
    }

    private func loadIndexedEntries() -> [TrashEntry] {
        guard fileManager.fileExists(atPath: Self.indexFilePath),
              let data = try? Data(contentsOf: URL(fileURLWithPath: Self.indexFilePath)) else {
            return []
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        guard let indexFile = try? decoder.decode(TrashIndexFile.self, from: data) else {
            return []
        }
        return indexFile.entries.filter { fileManager.fileExists(atPath: $0.storedPath) || isSymbolicLink(atPath: $0.storedPath) }
    }

    private func saveIndexedEntries(_ list: [TrashEntry]) {
        let nonLegacy = list.filter { !$0.isLegacy }
        let indexFile = TrashIndexFile(version: 1, entries: nonLegacy)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted]
        encoder.dateEncodingStrategy = .secondsSince1970
        guard let data = try? encoder.encode(indexFile) else { return }

        do {
            try data.write(to: URL(fileURLWithPath: Self.indexFilePath))
            return
        } catch {
            let sandboxTemp = (NSTemporaryDirectory() as NSString).appendingPathComponent("argo-trash-index.tmp")
            defer { try? fileManager.removeItem(atPath: sandboxTemp) }
            if let _ = try? data.write(to: URL(fileURLWithPath: sandboxTemp)) {
                robustRemove(Self.indexFilePath)
                _ = robustMove(from: sandboxTemp, to: Self.indexFilePath)
                if fileManager.fileExists(atPath: Self.indexFilePath) {
                    return
                }
            }
            if let _ = try? data.write(to: URL(fileURLWithPath: tempPath)) {
                robustRemove(Self.indexFilePath)
                RootHelperActs.mvtemp(Self.indexFilePath)
            }
        }
    }

    private func scanLegacyEntries() -> [TrashEntry] {
        guard let names = try? fileManager.contentsOfDirectory(atPath: Self.trashRootPath) else {
            return []
        }
        var result: [TrashEntry] = []
        for name in names {
            if name == "argo-trash-index.json" || name == "items" || name == "." || name == ".." {
                continue
            }
            let fullPath = "\(Self.trashRootPath)/\(name)"
            var isDir: ObjCBool = false
            guard fileManager.fileExists(atPath: fullPath, isDirectory: &isDir) else { continue }
            let isSym = isSymbolicLink(atPath: fullPath)
            let effectiveIsDir = isSym ? false : isDir.boolValue
            let attrs = try? fileManager.attributesOfItem(atPath: fullPath)
            let modDate = (attrs?[.modificationDate] as? Date) ?? Date(timeIntervalSince1970: 0)
            let itemSize = computeSize(atPath: fullPath, isDirectory: effectiveIsDir)
            let entry = TrashEntry(
                id: "legacy-\(name)",
                originalPath: "",
                originalName: name,
                deletedAt: modDate,
                isDirectory: effectiveIsDir,
                size: itemSize
            )
            result.append(entry)
        }
        return result
    }

    @discardableResult
    func moveToTrash(path: String) -> Bool {
        let normalized = normalizePath(path)
        guard !normalized.isEmpty else { return false }

        if normalized == Self.trashRootPath || normalized.hasPrefix(Self.trashRootPath + "/") {
            return false
        }

        // S5: 휴지통 이동 전 FileGuard 승인 검사 (금지 구역 및 안전 모드 차단 + UI 알림)
        guard case .success = FileGuard.authorize(action: "trash", paths: [normalized]) else {
            return false
        }

        let isSym = isSymbolicLink(atPath: normalized)
        var isDir: ObjCBool = false
        guard isSym || fileManager.fileExists(atPath: normalized, isDirectory: &isDir) else {
            return false
        }
        let effectiveIsDir = isSym ? false : isDir.boolValue

        ensureDirectories()

        let uuid = UUID().uuidString
        let originalName = (normalized as NSString).lastPathComponent
        guard !originalName.isEmpty, originalName != "/" else { return false }

        let itemSize = computeSize(atPath: normalized, isDirectory: effectiveIsDir)
        let uuidDir = "\(Self.itemsRootPath)/\(uuid)"
        let destPath = "\(uuidDir)/\(originalName)"

        robustMkdir(uuidDir)

        guard robustMove(from: normalized, to: destPath) else {
            robustRemove(uuidDir)
            return false
        }

        var currentIndexed = loadIndexedEntries()
        let newEntry = TrashEntry(
            id: uuid,
            originalPath: normalized,
            originalName: originalName,
            deletedAt: Date(),
            isDirectory: effectiveIsDir,
            size: itemSize
        )
        currentIndexed.append(newEntry)
        saveIndexedEntries(currentIndexed)
        OperationLog.record(action: "TRASH", path: normalized, destination: destPath)
        reload()
        return true
    }

    func moveToTrash(paths: [String]) -> (moved: Int, failed: Int) {
        var movedCount = 0
        var failedCount = 0
        for p in paths {
            if moveToTrash(path: p) {
                movedCount += 1
            } else {
                failedCount += 1
            }
        }
        return (movedCount, failedCount)
    }

    func restore(_ entry: TrashEntry) -> TrashRestoreResult {
        if entry.isLegacy || entry.originalPath.isEmpty {
            return .failed("원래 위치를 알 수 없습니다.")
        }

        let sourcePath = entry.storedPath
        guard fileManager.fileExists(atPath: sourcePath) || isSymbolicLink(atPath: sourcePath) else {
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            reload()
            return .failed("휴지통에 해당 항목이 없습니다.")
        }

        let targetOriginal = normalizePath(entry.originalPath)

        switch FileGuard.authorize(action: "restore", paths: [targetOriginal]) {
        case .failure(let err):
            return .failed(err.localizedDescription)
        case .success:
            break
        }

        let parentDir = (targetOriginal as NSString).deletingLastPathComponent
        ensureDirectoryRecursive(parentDir)

        var finalPath = targetOriginal
        var didRename = false

        if fileManager.fileExists(atPath: finalPath) || isSymbolicLink(atPath: finalPath) {
            finalPath = makeNonConflictingPath(for: targetOriginal, isDirectory: entry.isDirectory)
            didRename = true
        }

        guard robustMove(from: sourcePath, to: finalPath) else {
            return .failed("복원 이동 실패: \(finalPath)")
        }

        let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
        robustRemove(uuidDir)

        var currentIndexed = loadIndexedEntries()
        currentIndexed.removeAll { $0.id == entry.id }
        saveIndexedEntries(currentIndexed)
        OperationLog.record(action: "RESTORE", path: sourcePath, destination: finalPath)
        reload()

        if didRename {
            return .renamed(finalPath)
        } else {
            return .restored(finalPath)
        }
    }

    func deleteForever(_ entry: TrashEntry) -> Bool {
        if entry.isLegacy {
            let target = entry.storedPath
            robustRemove(target)
            let ok = !fileManager.fileExists(atPath: target) && !isSymbolicLink(atPath: target)
            if ok {
                OperationLog.record(action: "DELETE_FOREVER", path: target)
            }
            reload()
            return ok
        } else {
            let uuidDir = "\(Self.itemsRootPath)/\(entry.id)"
            robustRemove(uuidDir)
            let ok = !fileManager.fileExists(atPath: uuidDir)
            var currentIndexed = loadIndexedEntries()
            currentIndexed.removeAll { $0.id == entry.id }
            saveIndexedEntries(currentIndexed)
            if ok {
                OperationLog.record(action: "DELETE_FOREVER", path: entry.originalPath)
            }
            reload()
            return ok
        }
    }

    @discardableResult
    func deletePathDirectly(_ path: String) -> Bool {
        let norm = normalizePath(path)
        guard !norm.isEmpty else { return false }
        guard case .success = FileGuard.authorize(action: "delete", paths: [norm]) else {
            return false
        }
        robustRemove(norm)
        let ok = !fileManager.fileExists(atPath: norm) && !isSymbolicLink(atPath: norm)
        if ok {
            OperationLog.record(action: "DELETE", path: norm)
        }
        return ok
    }

    func emptyAll() -> Bool {
        robustRemove(Self.trashRootPath)
        robustMkdir(Self.trashRootPath)
        robustMkdir(Self.itemsRootPath)
        saveIndexedEntries([])
        OperationLog.record(action: "EMPTY_TRASH", path: Self.trashRootPath)
        reload()
        return entries.isEmpty
    }

    // MARK: - Helpers

    private func normalizePath(_ path: String) -> String {
        var result = path.trimmingCharacters(in: .whitespacesAndNewlines)
        while result.count > 1 && result.hasSuffix("/") {
            result.removeLast()
        }
        return result
    }

    private func ensureDirectoryRecursive(_ dirPath: String) {
        let normalized = normalizePath(dirPath)
        guard !normalized.isEmpty, normalized != "/" else { return }
        if fileManager.fileExists(atPath: normalized) { return }

        let components = normalized.split(separator: "/").map(String.init)
        var current = ""
        for comp in components {
            current += "/" + comp
            if !fileManager.fileExists(atPath: current) {
                robustMkdir(current)
            }
        }
    }

    private func makeNonConflictingPath(for originalPath: String, isDirectory: Bool) -> String {
        let nsPath = originalPath as NSString
        let parent = nsPath.deletingLastPathComponent
        let fileName = nsPath.lastPathComponent

        let baseName: String
        let ext: String

        if isDirectory {
            baseName = fileName
            ext = ""
        } else {
            let nsFile = fileName as NSString
            ext = nsFile.pathExtension
            baseName = ext.isEmpty ? fileName : nsFile.deletingPathExtension
        }

        var counter = 1
        while true {
            let candidateName: String
            if ext.isEmpty {
                candidateName = "\(baseName) (\(counter))"
            } else {
                candidateName = "\(baseName) (\(counter)).\(ext)"
            }
            let candidatePath = (parent as NSString).appendingPathComponent(candidateName)
            if !fileManager.fileExists(atPath: candidatePath) && !isSymbolicLink(atPath: candidatePath) {
                return candidatePath
            }
            counter += 1
        }
    }

    private func computeSize(atPath path: String, isDirectory: Bool) -> Int64 {
        if isSymbolicLink(atPath: path) {
            return 0
        }
        if !isDirectory {
            let attrs = try? fileManager.attributesOfItem(atPath: path)
            if let num = attrs?[.size] as? NSNumber {
                return num.int64Value
            }
            if let s = attrs?[.size] as? Int64 {
                return s
            }
            return 0
        }

        var total: Int64 = 0
        let rootURL = URL(fileURLWithPath: path)
        // S6: 용량 계산 시에도 심볼릭 링크를 따라가지 않도록 처리
        if let enumerator = fileManager.enumerator(
            at: rootURL,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
            options: [.skipsPackageDescendants]
        ) {
            for case let fileURL as URL in enumerator {
                if let vals = try? fileURL.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey, .fileSizeKey]) {
                    if vals.isSymbolicLink == true {
                        enumerator.skipDescendants()
                        continue
                    }
                    if vals.isRegularFile == true, let s = vals.fileSize {
                        total += Int64(s)
                    }
                }
            }
        }
        return total
    }
}
```

**볼 곳**

- 모든 공개 진입점이 `FileGuard` 를 먼저 지나는가
- **`robustRemove` 의 `unlinkNestedSymlinksIfDirectory` 호출 위치** ← R2. `removeItem` 보다 **먼저** 부릅니다
- `robustMove` 가 목적지를 비우려 `robustRemove` 를 부르는 경로 ← R2 의 더 날카로운 쪽
- 되살리기가 원래 경로를 복원할 때도 관문을 지나는가
- 휴지통 자체(`.Trash`)가 `.caution` 으로 분류되어 비우기가 막히지는 않는가

---

## 5. 루트 헬퍼 관문

### `Spartan/SpareViews.swift:334-403 (RootHelperActs)`  (70줄)

```swift
class RootHelperActs {
    private static func checkAuth(action: String, paths: [String]) -> Bool {
        switch FileGuard.authorize(action: action, paths: paths) {
        case .success:
            return true
        case .failure(let err):
            print("[FileGuard Blocked] \(action) on \(paths) - \(err.localizedDescription)")
            return false
        }
    }

	class func rm(_ filePath: String) {
        guard checkAuth(action: "rm", paths: [filePath]) else { return }
		spawn(command: helperPath, args: ["rm", filePath], env: [], root: true)
        if !fileManager.fileExists(atPath: filePath) {
            OperationLog.record(action: "DELETE (root)", path: filePath)
        }
	}
	
	class func mv(_ filePath: String, _ fileDest: String) {
        guard checkAuth(action: "mv", paths: [filePath, fileDest]) else { return }
		spawn(command: helperPath, args: ["mv", filePath, fileDest], env: [], root: true)
        if fileManager.fileExists(atPath: fileDest) {
            OperationLog.record(action: "MOVE (root)", path: filePath, destination: fileDest)
        }
	}
	
	class func cp(_ filePath: String, _ fileDest: String) {
        guard checkAuth(action: "cp", paths: [filePath, fileDest]) else { return }
		spawn(command: helperPath, args: ["cp", filePath, fileDest], env: [], root: true)
        if fileManager.fileExists(atPath: fileDest) {
            OperationLog.record(action: "COPY (root)", path: filePath, destination: fileDest)
        }
	}
	
	class func touch(_ filePath: String) {
        guard checkAuth(action: "touch", paths: [filePath]) else { return }
		spawn(command: helperPath, args: ["tf", filePath], env: [], root: true)
        if fileManager.fileExists(atPath: filePath) {
            OperationLog.record(action: "TOUCH (root)", path: filePath)
        }
	}
	
	class func mkdir(_ filePath: String) {
        guard checkAuth(action: "mkdir", paths: [filePath]) else { return }
		spawn(command: helperPath, args: ["td", filePath], env: [], root: true)
        if fileManager.fileExists(atPath: filePath) {
            OperationLog.record(action: "MKDIR (root)", path: filePath)
        }
	}
	
	class func ln(_ filePath: String, _ fileDest: String) {
        guard checkAuth(action: "ln", paths: [filePath, fileDest]) else { return }
		spawn(command: helperPath, args: ["ts", filePath, fileDest], env: [], root: true)
        if fileManager.fileExists(atPath: fileDest) {
            OperationLog.record(action: "SYMLINK (root)", path: filePath, destination: fileDest)
        }
	}
	
	class func chmod(_ filePath: String, _ perms: Int) {
        guard checkAuth(action: "chmod", paths: [filePath]) else { return }
		spawn(command: helperPath, args: ["ch", filePath, String(perms)], env: [], root: true)
        OperationLog.record(action: "CHMOD (root)", path: "\(filePath) to \(perms)")
	}
    
    class func mvtemp(_ filePath: String) {
        guard checkAuth(action: "mvtemp", paths: [filePath]) else { return }
        self.mv(tempPath, filePath)
        self.rm(tempPath)
    }
```

**볼 곳**

- `checkAuth` 가 **단일 관문**입니다. 8개 함수가 전부 이것을 지납니다 — 13곳을 고치는 대신 한 곳을 고친 결과입니다
- `spawn` 은 **양쪽 경로에서 동기**입니다 (탈옥 경로는 `waitpid`, TrollStore 경로는 `waitpid` + `WEXITSTATUS`). 그래서 `spawn` 직후의 `fileExists` 교차 검증이 유효합니다 — 딜레이가 필요 없습니다
- 위에 적은 대로 **이 코드는 실제로 돌지 않습니다.** `TROLLSTORE` 가 정의되지 않고 바이너리가 IPA 에 없습니다

---

## 6. 압축 관문

### `Spartan/FileOpeners/Compression/Compression.swift:102-140 (오류형)`  (39줄)

```swift
enum CompressionError: Error {
    /// FileGuard refused the destination, or the archive tried to escape it.
    case blockedBySafety(String)
    case incorrectCompressionType
    case archiveCreationFailed
    
    case inputFileOpenFailed
    case outputFileOpenFailed
    
    case archiveReadHeaderFailed
    case archiveReadDataFailed
    
    case archiveWriteHeaderFailed
    case archiveWriteDataFailed
    
    case archiveFinishEntryFailed
    
    case archiveUnsupportedCompressionType
    
    func explanation() -> String {
        switch self {
        case .incorrectCompressionType:
            return LocalizedString("COMPERROR_INCORRECTCOMPRESSIONTYPE")
        case .archiveCreationFailed:
            return LocalizedString("COMPERROR_ARCHIVECREATIONFAILED")
        case .inputFileOpenFailed:
            return LocalizedString("COMPERROR_INPUTFILEOPENFAILED")
        case .outputFileOpenFailed:
            return LocalizedString("COMPERROR_OUTPUTFILEOPENFAILED")
        case .archiveReadHeaderFailed:
            return LocalizedString("COMPERROR_ARCHIVEREADHEADERFAILED")
        case .archiveReadDataFailed:
            return LocalizedString("COMPERROR_ARCHIVEREADDATAFAILED")
        case .archiveWriteHeaderFailed:
            return LocalizedString("COMPERROR_ARCHIVEWRITEHEADERFAILED")
        case .archiveWriteDataFailed:
            return LocalizedString("COMPERROR_ARCHIVEWRITEDATAFAILED")
        case .archiveFinishEntryFailed:
            return LocalizedString("COMPERROR_ARCHIVEFINISHENTRYFAILED")
```
### `Spartan/FileOpeners/Compression/Compression.swift:176-290 (세 진입점)`  (115줄)

```swift
    class func compressFile(filePath: String, directory: String, destination: String, compType: CompressionType, tarCompType: TarCompressionType) throws {
        if case .failure(let err) = FileGuard.authorize(action: "compress", paths: [destination]) {
            throw CompressionError.blockedBySafety(err.localizedDescription)
        }

        switch compType {
        case .tar:
            do {
                try createTarC(filePaths: [filePath], destination: tempPath)
                if tarCompType != .none {
                    try compressFileC(filePath: tempPath, destination: destination, type: CompressionType(type: tarCompType))
                    RootHelperActs.rm(tempPath)
                } else {
                    RootHelperActs.mvtemp(destination)
                }
            }
        case .unknown:
            throw CompressionError.incorrectCompressionType
        default:
            do {
                try compressFileC(filePath: directory + filePath, destination: destination, type: compType)
            } catch {
                throw error
            }
        }
    }
    
    class func compressFiles(listOfFiles: [String], directory: String, destination: String, compType: CompressionType, tarCompType: TarCompressionType = .none) throws {
        if case .failure(let err) = FileGuard.authorize(action: "compress", paths: [destination]) {
            throw CompressionError.blockedBySafety(err.localizedDescription)
        }

        var stringList = ""
        for file in listOfFiles {
            stringList += "\(file) "
        }
        
        print(listOfFiles)
        print(stringList)
        
        switch compType {
        case .zip:
            do {
                var urls: [URL] = []
                for file in listOfFiles {
                    urls.append(URL(fileURLWithPath: directory + file))
                }
                print(urls)
                try Zip.zipFiles(paths: urls, zipFilePath: URL(fileURLWithPath: destination), password: nil, progress: nil)
            } catch {
                throw CompressionError.archiveWriteHeaderFailed
            }
        case .zstd:
            var fullPaths: [String] = []
            for file in listOfFiles {
                fullPaths.append(directory + file)
            }
            do {
                try compressZstdC(filePaths: fullPaths, destination: destination)
            } catch {
                throw error
            }
        case .tar:
            var fullPaths: [String] = []
            for file in listOfFiles {
                fullPaths.append(directory + file)
            }
            do {
                try createTarC(filePaths: fullPaths, destination: tempPath)
                if tarCompType != .none {
                    try compressFileC(filePath: tempPath, destination: destination, type: CompressionType(type: tarCompType))
                    RootHelperActs.rm(tempPath)
                } else {
                    RootHelperActs.mvtemp(destination)
                }
            } catch {
                throw error
            }
        default:
            throw CompressionError.incorrectCompressionType
        }
    }
    
    class func uncompressFile(filePath: String, destination: String, compType: CompressionType) throws {
        // Entry names are read before a byte is written: an archive that
        // reaches outside the destination is refused whole.
        if case .failure(let err) = FileGuard.validateArchiveBeforeExtract(archivePath: filePath,
                                                                          destinationDir: destination) {
            throw CompressionError.blockedBySafety(err.localizedDescription)
        }

        switch compType {
        case .zip:
            do {
                try Zip.unzipFile(URL(fileURLWithPath: filePath), destination: URL(fileURLWithPath:destination), overwrite: true, password: nil)
            } catch {
                throw CompressionError.archiveReadDataFailed
            }
        case .zstd:
            do {
                try decompressZstdC(filePath: filePath, destination: destination)
            } catch {
                throw error
            }
        case .unknown:
            throw CompressionError.incorrectCompressionType
        default:
            do {
                try decompressFileC(filePath: filePath, destination: destination)
            } catch {
                throw error
            }
        }
    }
    
```

**볼 곳**

- 세 공개 진입점(`compressFile` · `compressFiles` · `uncompressFile`) 맨 앞에 관문이 있는가
- 해제는 `validateArchiveBeforeExtract` 를 지나는가 — 그리고 그 `.failure` 를 **삼키지 않는가**
- `Zip.unzipFile(overwrite: true)` 로 가므로, 검사가 통과한 뒤에는 덮어씁니다. 검사가 전부인 구조입니다

---

## 7. 배너 · 직접 저장 · 일괄 삭제

### `Spartan/Safety/SafetyBannerAndPreview.swift`  (79줄)

```swift
//
//  SafetyBannerAndPreview.swift
//  Argo
//

import SwiftUI

struct PathSafetyBanner: View {
    let risk: PathRisk
    let message: String
    
    var body: some View {
        VStack {
            HStack {
                Image(systemName: risk == .forbidden ? "lock.fill" : "exclamationmark.triangle.fill")
                Text(message)
                    .font(.headline)
            }
            .padding()
            .background(risk == .forbidden ? Color.red.opacity(0.9) : Color.orange.opacity(0.9))
            .foregroundColor(.white)
            .cornerRadius(12)
            .shadow(radius: 10)
            .padding(.top, 40)
            
            Spacer()
        }
        .allowsHitTesting(false) // 포커스 뺏지 않음
    }
}

struct BatchDeletePreview: View {
    let files: [String]
    let cautionCount: Int
    let onConfirm: () -> Void
    let onCancel: () -> Void
    
    var body: some View {
        ZStack {
            Color.black.opacity(0.85).edgesIgnoringSafeArea(.all)
            
            VStack(spacing: 20) {
                Text("지울 것 \(files.count)개")
                    .font(.largeTitle)
                    .foregroundColor(.red)
                
                ScrollView {
                    VStack(alignment: .leading, spacing: 5) {
                        ForEach(files.prefix(10), id: \.self) { file in
                            Text("• \(file)")
                                .foregroundColor(.white)
                        }
                        if files.count > 10 {
                            Text("...외 \(files.count - 10)개")
                                .foregroundColor(.gray)
                        }
                    }
                }
                .frame(maxHeight: 300)
                
                if cautionCount > 0 {
                    Text("⚠️ \(cautionCount)개가 주의 구역에 있습니다")
                        .foregroundColor(.orange)
                }
                
                HStack(spacing: 40) {
                    Button("취소", action: onCancel)
                    Button(action: onConfirm) {
                        Text("휴지통으로 / 삭제")
                            .foregroundColor(.red)
                    }
                }
            }
            .padding(40)
            .background(Color(white: 0.15))
            .cornerRadius(20)
        }
    }
}
```
### `Spartan/FileOpeners/TextView.swift:82-100 (직접 저장 관문)`  (19줄)

```swift
				}
				Spacer()
				Button(action: {
					do {
						let fullPath = filePath + fileName
						if case .failure(let guardErr) = FileGuard.authorize(action: "write_text", paths: [fullPath], notifyOnBlock: false) {
							errorString = guardErr.localizedDescription
							errorShow = true
							return
						}
						if filePathIsNotMobileWritable(fullPath) {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: tempPath), atomically: true, encoding: encoding)
							RootHelperActs.rm(fullPath)
							RootHelperActs.mvtemp(fullPath)
						} else {
							try stringArrayToString(inputArray: fileContents).write(to: URL(fileURLWithPath: fullPath), atomically: true, encoding: encoding)
							OperationLog.record(action: "EDIT_TEXT", path: fullPath)
						}
						isPresented = false
```
### `Spartan/FileOpeners/HexView.swift:108-124 (직접 저장 관문)`  (17줄)

```swift
				Button(action: {
					let newData = Data(fromHexEncodedString: stringArrayToString(inputArray: hexArray)) // convert hex string to data object
					do {
						let fullPath = filePath + fileName
						// Both branches write; the guard has to come before the fork.
						if case .failure = FileGuard.authorize(action: "write_hex", paths: [fullPath]) {
							return
						}
						if filePathIsNotMobileWritable(fullPath) {
							try newData?.write(to: URL(fileURLWithPath: tempPath)) // write data to file
							RootHelperActs.rm(fullPath)
							RootHelperActs.mvtemp(fullPath)
						} else {
							try newData?.write(to: URL(fileURLWithPath: fullPath)) // write data to file
						}
						isPresented = false
					} catch {
```
### `Spartan/ContentView.swift:508-524 (일괄 삭제 관문)`  (17줄)

```swift
                            Button(action: {
                                if multiSelectFiles.isEmpty { return }
                                // 보안 검사 먼저 수행 (다중 선택)
                                let fullPaths = multiSelectFiles.map { directory + $0 }
                                switch FileGuard.authorize(action: "batch_delete", paths: fullPaths) {
                                case .success:
                                    cautionCountForBatch = fullPaths.filter { PathSafety.risk(of: $0) == .caution }.count
                                    showBatchDeletePreview = true // 승인 후 팝업 띄움
                                case .failure(let err):
                                    alertTitle = "작업 차단됨"
                                    alertMsg = err.localizedDescription
                                    showAlert = true
                                }
                            }) {
                                ZStack {
                                    if(directory == "/private/var/mobile/Media/.Trash/"){
                                        Image(systemName: "trash")
```

**볼 곳**

- `HexView` 의 관문은 **제미나이가 지시한 위치보다 위로 옮겼습니다.** 원안은 `filePathIsNotMobileWritable` 의 `if` 안쪽이라 `else` 분기(직접 쓰기)가 안 막혔습니다
- `TextView` 는 `notifyOnBlock: false` 로 부르고 차단 사유를 화면에 직접 띄웁니다 — 알림과 중복되지 않게
- 일괄 삭제는 **미리보기를 거친 뒤** 관문을 지나고, 미리보기가 `.caution` 건수를 셉니다
- 배너가 1.2초 뒤 사라지는가 (요청하신 "약 1초")

---

## 8. 기본값

### `Spartan/SpartanApp.swift:70-82 (안전 모드 기본값)`  (13줄)

```swift
            helperPath = Bundle.main.path(forResource: "RootHelper", ofType: nil)
            print("We are running on Trollstore...")
        #else
            print("We are jailbroken!")
        #endif
        
        UserDefaults.settings.register(defaults: ["safeModeEnabled": true])
        
        if(!(FileManager.default.fileExists(atPath: "/private/var/mobile/Media/.Trash"))){
            RootHelperActs.mkdir("/private/var/mobile/Media/.Trash")
        } else {
            print("Trash already exists")
        }
```

**볼 곳**

- `register(defaults:)` 로 등록합니다. 전에는 관문이 `bool(forKey:)` 로 읽고 설정 UI 는 `object(forKey:) == nil ? true` 로 읽어 **설정에는 초록 방패가 보이는데 관문은 전부 통과시켰습니다**

---

## 정리 — 리뷰 체크리스트

- [ ] 표에 없는 쓰기 경로가 있는가 (`FileManager` 쓰기 함수 전수 조사)
- [ ] `/var/mobile/Media/` 아래가 전부 통과하는가
- [ ] 심볼릭 링크 자체를 지우는 것이 막히지 않는가
- [ ] 안전 모드 끈 상태에서 `.caution` 이 통과하는가
- [ ] `validateArchiveBeforeExtract` 의 `.failure` 를 삼키는 호출부가 없는가
- [ ] **R2 실기기 시험** ← 이것부터
- [ ] 되살리기가 관문을 지나는가
- [ ] 휴지통 비우기가 자기 자신 때문에 막히지 않는가
