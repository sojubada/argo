# 검증 C — 경로 권한 · 관문 · 삭제 · 휴지통

커밋 `6869fc9`.

이 묶음의 목적은 하나입니다. **파일 탐색기가 애플TV 를 벽돌로 만들지 않게
하는 것.** 그래서 판단 기준도 하나입니다 — **막아야 할 것을 막는가,
막으면 안 될 것을 막지는 않는가.**

여기서 한 번 크게 틀렸습니다. 아래 2장을 먼저 봐 주십시오.

---

## 답해 주셔야 할 질문

### Q1. 경로 경계표에서 틀린 줄을 찾아 주십시오  ★

### Q2. 관문을 지나지 않는 쓰기 경로가 있습니까?  ★

모든 파괴적 쓰기는 `FileGuard.authorize` 를 지나야 합니다.
`FileManager` 의 `removeItem` · `moveItem` · `copyItem` · `createFile` ·
`write` 와 `Data.write(to:)` 를 전수로 훑어 주십시오.

지금 관문을 지나는 곳:

| 위치 | 무엇 |
| --- | --- |
| `RootHelperActs.checkAuth` | rm · mv · cp · touch · mkdir · ln · chmod · mvtemp |
| `TrashManager` | 휴지통 보내기 · 비우기 · 되살리기 · 영구 삭제 |
| `Compression` | 압축 · 압축 풀기 |
| `PaneTransfer` | 칸 사이 복사·이동, 되돌리기 |
| `TextView` · `HexView` | 직접 저장 |
| `ContentView.requestDeleteSelection` | 일괄 삭제 |

**알려진 예외 하나**: IPA 설치는 `apple-magnifier://` 로 가고 관문을 안 거칩니다.
애플 자신의 설치 경로이고 우리가 파일을 쓰는 것이 아니라 의도한 것입니다.

**새로 생긴 것 하나**: `FilePaneModel.writeCacheIfWritable` 은 관문 대신
`PathSafety.isWriteAllowed` 를 직접 봅니다. 캐시는 편의이지 사용자 작업이
아니라서 차단 알림을 띄우지 않으려고 그렇게 했습니다. **이 판단이 맞습니까?**

### Q3. 과차단은 없습니까?

막으면 안 될 것을 막으면 앱이 쓸모없어집니다. 특히:

- `/var/mobile/` 아래가 전부 통과합니까?
- **심볼릭 링크 자체를 지우는 것**이 막히지 않습니까?
  (`effectivePathForWriteCheck` 가 링크의 **부모**를 보는 이유입니다 —
  이게 없으면 시스템을 가리키는 링크를 치울 수조차 없습니다)
- 안전 모드를 끄면 `.caution` 이 통과합니까?

### Q4. 안전 범위를 `/var/mobile` 전체로 넓힌 것이 맞습니까?

원본 Spartan 의 권한 모델은 이것뿐이었습니다.

```swift
func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
    return ((fullPath.count < 19) || (fullPath.substring(toIndex: 19) != "/private/var/mobile"))
}
```

`/private/var/mobile` 아래 전부 쓰기 가능이고, **쓰인 곳은 텍스트·헥스·plist
편집기 세 곳뿐**이었습니다. **삭제·이동·복사에는 경로 제한이 아예 없었습니다.**

저는 처음에 `Media`·`Documents`·`Downloads` 셋만 열었다가, 그러면
`Library/` 나 사용자가 만든 폴더가 전부 '주의'가 되어 자기 파일도 못 지우는
것을 보고 **원본과 같은 `/var/mobile` 전체**로 넓혔습니다.

- 이 범위가 적절합니까? `/var/mobile/Library/Preferences` 를 지우는 것이
  위험합니까? (벽돌은 아니지만 설정이 날아갑니다)
- 반대로 `/var/mobile/Containers` 처럼 더 막아야 할 하위 경로가 있습니까?

### Q5. 삭제가 한 길로만 갑니까?

두 번 망가졌던 곳입니다.

1. 미리보기는 **길게 누른 파일**을 보여 주는데 실제 삭제는
   `multiSelectFiles`(여러 개 고르기 목록)를 돌았습니다. 한 개를 길게 눌러
   지우면 그 배열이 비어 있어 **아무 일도 일어나지 않았습니다**
2. 툴바의 일괄 삭제 버튼은 미리보기를 띄우면서 지울 목록을 **채우지
   않아** `지울 것 0개` 가 떴습니다

지금은 `pendingDeletePaths` 한 곳에 담고 미리보기와 삭제가 같은 것을 봅니다.
**다른 경로가 남아 있지 않은지** 봐 주십시오.

### Q6. R2 는 함수를 지우는 것으로 끝났습니다 — 맞습니까?

`TrashManager.unlinkNestedSymlinksIfDirectory` 는 폴더를 지우기 전에 안의
심볼릭 링크를 미리 끊던 함수입니다. **지웠습니다.**

근거:
- 재귀 삭제가 링크를 따라가지 않는 것은 POSIX 전반의 동작입니다
  (`rm -rf`, `fts`(`FTS_PHYSICAL`) 모두 링크를 끊고 지나갑니다)
- 공개 소스 동급 앱(`NSAntoine/Santander`)은 루트 헬퍼까지 갖추고도
  `try FileManager.default.removeItem(at:)` 한 줄이 전부입니다
- 어느 쪽이든 지우는 것이 안전합니다 — 만약 정말 따라간다 해도 결과는
  원본 Spartan 과 같아져 전보다 나빠지지 않습니다. 반면 남겨 두면
  **삭제·이동 실패 시 멀쩡한 폴더의 링크만 영구히 사라집니다**

**이 논증에 구멍이 있습니까?** 특히 `FileManager.removeItem` 이 내부적으로
`removefile(3)` 을 쓸 때의 기본 동작을 확인해 주십시오.

---

## 1. 경로 위험도

### `Spartan/Safety/PathSafety.swift`  (134줄)

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
        "/var/mobile",
        (NSTemporaryDirectory() as NSString).standardizingPath
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
```

---

## 2. 지금 규칙으로 뽑은 경계표  ★

위 코드를 그대로 흉내내 돌린 결과입니다. **틀린 줄을 찾아 주십시오.**

| 경로 | 판정 |
| --- | --- |
| `/private/var/mobile/Media/Radio/` | safe |
| `/var/mobile/Media/x.pdf` | safe |
| `/private/var/mobile/Library/` | safe |
| `/var/mobile/Downloads/a.log` | safe |
| `/var/mobile/Documents/` | safe |
| `/private/var/mobile/` | safe |
| `/var/mobile` | safe |
| `/var/tmp/`, `/private/var/tmp/z` | safe |
| `/VAR/MOBILE/Media/x` | safe (대소문자 무시) |
| `/var/mobileXYZ/evil` | **caution** (접두사 충돌 아님) |
| `/var/log/` | caution |
| `/System/Library/` | forbidden |
| `/private/var/db/x` | forbidden |
| `/var/containers/Bundle/` | forbidden |
| `/Applications/` | forbidden |
| `/`, `/var`, `/private/var` | forbidden |

**고치기 전에는 이 표의 safe 가 전부 caution 이었습니다.**
`resolvingSymlinksInPath()` 가 `/private` 접두사를 떼어내는데 목록은
`/private/var/...` 로 적혀 있어 **하나도 안 맞았기** 때문입니다.
안전 모드가 기본 켜짐이라 결과적으로 **모든 쓰기가 막혔습니다.**

---

## 3. 관문

### `Spartan/Safety/FileGuard.swift`  (323줄)

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
            return refuseUncheckable("압축 파일이 손상되어 검사를 끝내지 못했습니다")
        }

        // 공간이 모자라면 시작하지 않는다. 반쯤 풀린 상태가 가장 수습하기 어렵다.
        if totalUncompressed > 0 {
            if case .failure(let err) = checkFreeSpace(needed: totalUncompressed,
                                                       at: resolvedDest,
                                                       what: "압축을 풀기") {
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
        let msg = "\(what)에 약 \(f.string(fromByteCount: withMargin)) 가 필요하지만 "
                + "남은 공간은 \(f.string(fromByteCount: free)) 입니다."
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
        return checkFreeSpace(needed: total, at: parent, what: "압축하기")
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
        let msg = "압축 해제 차단 — \(reason). 검사할 수 없는 압축 파일은 풀지 않습니다."
        reportBlocked(msg)
        return .failure(.forbidden(msg))
    }
}
```

**볼 곳**

- **`refuseUncheckable`** — 검사할 수 없으면 **막습니다.** 전에는 성공을
  돌려줘 그대로 풀렸습니다. 검사는 libarchive, 해제는 minizip 이라
  **한쪽이 거부하고 다른 쪽이 받아들이는 파일 하나로 검사가 통째로
  우회됐습니다**
- **`ARCHIVE_EOF`** — `ARCHIVE_OK` 는 0, `EOF` 는 1, `WARN` 은 -20 입니다.
  전에는 정상 종료와 손상을 구분하지 않아 50개 중 3번째에서 경고가 나면
  나머지 47개가 검사 없이 풀렸습니다
- **하드링크** — `archive_entry_symlink` 와 `archive_entry_hardlink` 는
  다른 필드입니다. 심볼릭 링크만 보면 같은 우회가 이름만 바꿔 남습니다
- **`checkFreeSpace`** — 읽지 못하면 **막지 않습니다.** 이것은 안전 검사가
  아니라 편의 검사이기 때문입니다. 압축 검사와 실패 방향이 **반대**인데
  이 구분이 맞습니까?

---

## 4. 휴지통

### `Spartan/Trash/TrashManager.swift`  (510줄)

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

        // 폴더 내부 심볼릭 링크를 미리 끊는 방어가 여기 있었다. 지웠다.
        // removeItem 은 아래로 훑으며 링크를 '끊고' 따라가지 않는다. 막을 위험이
        // 없었고, 대신 아래 removeItem 이 실패하면 폴더는 남은 채 안의 링크만
        // 영구히 사라졌다 — robustMove 가 목적지를 비우려 이 함수를 부르므로,
        // 옮기기가 실패하면 건드릴 생각도 없던 폴더의 링크만 날아갔다.
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

- 모든 공개 진입점이 관문을 먼저 지나는가
- 되살리기가 원래 경로를 복원할 때도 관문을 지나는가
- 휴지통 자체(`.Trash`)가 `.caution` 으로 분류되어 비우기가 막히지는 않는가

---

## 5. 삭제 경로

### `ContentView.swift (삭제 요청)`  (24줄)

```swift

    /// ⌫ 로 삭제. 기존 일괄 삭제 미리보기를 거친다 — 되돌릴 수 없는 일이라 확인을 뺄 수 없다.
    func requestDeleteSelection() {
        let targets = pane.selectedPaths.isEmpty
            ? (newViewFilePath.isEmpty ? [] : [newViewFilePath])
            : pane.selectedPaths
        guard !targets.isEmpty else {
            alertTitle = "삭제할 항목이 없습니다"
            alertMsg = "먼저 항목을 고르십시오."
            showAlert = true
            return
        }
        // 관문을 먼저 지난다. 막히면 왜 막혔는지 말해 준다 —
        // 조용히 아무 일도 안 일어나는 것이 가장 나쁘다.
        if case .failure(let err) = FileGuard.authorize(action: "batch_delete", paths: targets) {
            alertTitle = "삭제가 차단되었습니다"
            alertMsg = err.localizedDescription
            showAlert = true
            return
        }
        pendingDeletePaths = targets
        cautionCountForBatch = targets.filter { PathSafety.risk(of: $0) != .safe }.count
        showBatchDeletePreview = true
    }
```

### `ContentView.swift (실제 삭제)`  (26줄)

```swift
    func executeBatchDelete() {
        // 미리보기가 보여준 그 목록을 그대로 지운다.
        let targets = pendingDeletePaths
        guard !targets.isEmpty else { return }

        let inTrash = PathSafety.risk(of: directory) != .forbidden
            && directory.contains("/.Trash")

        for path in targets {
            if inTrash {
                TrashManager.shared.deletePathDirectly(path)
            } else {
                TrashManager.shared.moveToTrash(path: path)
            }
        }

        pendingDeletePaths = []
        multiSelect = false
        resetMultiSelectArrays()
        updateFiles()
    }
}

struct SpartanFile: Hashable, Encodable, Decodable {
    var name: String
    var fullPath: String
```

---

## 6. 작업 기록

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

