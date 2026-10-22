# 5차 검토 — 반영 후 잔여 위험

4차 패키지를 **전부 반영하고 `main` 에 병합했습니다.** CI 통과(실행 39746308508 계열).

제미나이가 4차 검토를 전부 수용한 것, 그리고 **루트 헬퍼를 살리지 않기로 한 판단**이 맞습니다. 벽돌 위험을 스스로 만들 이유가 없습니다.

제가 직접 연결한 것이 넷입니다 (제미나이가 "검수자 적용"으로 남긴 것).

| 위치 | 한 일 |
|---|---|
| `Compression.swift` × 3 | `compressFile` · `compressFiles` · `uncompressFile` 맨 앞에 관문. 해제는 `validateArchiveBeforeExtract` |
| `CompressionError` | `blockedBySafety(String)` 추가 + `explanation()` 에 연결 (`switch must be exhaustive` 오류 해결) |
| `HexView.swift` | **제미나이 지시보다 위로 옮겼습니다.** 원안은 `filePathIsNotMobileWritable` 의 `if` 안쪽이라 `else` 분기(직접 쓰기)가 안 막혔습니다 |

`TextView` 는 제미나이 블록이 두 분기를 다 덮고 있어 그대로 썼습니다. 차단 사유를 화면에 바로 띄우는 것도 좋습니다.

**IPA 설치는 영향 없습니다.** `IPAInstaller` 는 `trollstorehelper` 와 `apple-magnifier://` 로 가고 `RootHelperActs` 를 안 거칩니다.

---

## 상 (Critical) — 둘

### R1. 압축 검사가 실패하면 **통과시킵니다** (fail-open)

```swift
guard let cPath = archivePath.cString(using: .utf8) else { return .success(()) }
guard let archive = archive_read_new()                else { return .success(()) }
guard archive_read_open_filename(...) == ARCHIVE_OK   else { return .success(()) }
```

**libarchive 가 열지 못하면 "안전하다"고 답하고, 그대로 `Zip.unzipFile(overwrite: true)` 가 돕니다.**

이게 왜 치명적이냐면, **검사하는 파서와 푸는 파서가 다릅니다.** 검사는 libarchive, 해제는 `Zip`(minizip)입니다. libarchive 가 거부하고 minizip 이 받아들이는 압축 파일 하나면 검사가 통째로 우회됩니다. 파서 차이를 이용하는 것은 Zip Slip 공격의 교과서적 수법입니다.

**보안 검사는 실패하면 막아야 합니다.**

```swift
guard archive_read_open_filename(archive, cPath, 10240) == ARCHIVE_OK else {
    let msg = "압축 파일을 검사할 수 없어 해제를 중단합니다."
    reportBlocked(msg)
    return .failure(.forbidden(msg))
}
```
네 군데 전부 같습니다.

### R1-b. 순회가 중간에 끊겨도 통과시킵니다

```swift
while archive_read_next_header(archive, &entry) == ARCHIVE_OK {
```

`ARCHIVE_OK` 는 0, **`ARCHIVE_EOF` 는 1, `ARCHIVE_WARN` 은 -20** 입니다. 정상 종료(EOF)와 **손상·경고(WARN/FATAL)** 가 똑같이 루프를 끝내고 `.success` 로 떨어집니다. 50개 중 3번째에서 경고가 나면 **나머지 47개는 검사도 안 하고 풀립니다.**

```swift
var r = archive_read_next_header(archive, &entry)
while r == ARCHIVE_OK {
    ... 검사 ...
    archive_read_data_skip(archive)
    r = archive_read_next_header(archive, &entry)
}
guard r == ARCHIVE_EOF else {
    let msg = "압축 파일이 손상되어 검사를 마치지 못했습니다."
    reportBlocked(msg)
    return .failure(.forbidden(msg))
}
```

### R2. 실패할 작업에서 심볼릭 링크를 **먼저 지웁니다** — 자료 손실

```swift
private func robustRemove(_ path: String) {
    ...
    unlinkNestedSymlinksIfDirectory(norm)   // ← 여기서 링크를 지우고
    do { try fileManager.removeItem(atPath: norm) }
    catch { RootHelperActs.rm(norm) }       // ← 여기서 실패할 수 있습니다
}
```

삭제가 실패하면 **폴더는 그대로 남고 안의 심볼릭 링크만 영구히 사라집니다.** 복구 수단이 없습니다.

더 날카로운 경우가 있습니다. `robustMove` 가 목적지를 비우려고 `robustRemove(normDst)` 를 부릅니다. **옮기기가 실패하면, 건드릴 생각도 없던 목적지 폴더의 링크만 날아갑니다.**

그리고 이 방어가 **필요한지 자체가 미확인입니다.** `FileManager.removeItem` 과 `FileManager.enumerator(at:)` 가 심볼릭 링크를 따라가는지, [애플의 `removefile(3)` 문서](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/removefile.3.html)는 **아무 말도 하지 않습니다.** 최상위 경로가 링크일 때 `EMLINK` 로 실패한다는 것만 적혀 있습니다.

**30초짜리 실기기 시험으로 끝납니다.**

```
1. /var/mobile/Media/target.txt 를 만든다
2. /var/mobile/Media/box/ 폴더를 만들고
   그 안에 target.txt 를 가리키는 심볼릭 링크를 만든다 (툴바의 링크 만들기)
3. box 폴더를 휴지통으로 보낸다
4. target.txt 가 살아 있는가?
```

- **살아 있으면** → `removeItem` 이 링크를 안 따라갑니다. **`unlinkNestedSymlinksIfDirectory` 를 지우십시오.** 보호 효과는 없고 손실 위험만 남습니다
- **사라졌으면** → 방어가 필요합니다. 다만 **성공이 확실해진 뒤에** 지우도록 바꿔야 합니다. 아래로 훑으며 지우되 링크는 `unlink` 만 하는 식으로요

어느 쪽이든 **`robustMove` 의 목적지 비우기에서는 빼십시오.**

---

## 중 (Major) — 넷

### R3. 하드링크 항목이 검사에서 빠졌습니다

심볼릭 링크는 `archive_entry_symlink` 로 보시는데, **하드링크는 `archive_entry_hardlink`** 로 따로 들어옵니다. 압축 안에 목적지 밖을 가리키는 하드링크가 있으면 같은 방식으로 빠져나갑니다. 검사를 한 줄 더 붙이십시오.

### R4. 차단 메시지가 쌓이기만 합니다

```swift
pendingMessages.append(message)
```

큰 폴더가 전부 막히면 문자열이 수천 개 쌓이고, 정작 요약은 **첫 번째 하나**만 보여줍니다.

건수만 세고 메시지는 **처음 한 개와 마지막 한 개**만 들고 계시면 충분합니다. `pendingMessages` 에 상한(예: 20개)을 두십시오.

### R5. 긴 작업에서 알림이 끝까지 안 뜹니다

디바운스가 **호출마다 타이머를 다시 깝니다.** 파일 2,000개를 30초에 걸쳐 지우는데 전부 막히면, **30초 내내 화면에 아무것도 안 뜨다가** 끝나고 0.25초 뒤에 한 번 뜹니다.

최대 대기 시간을 두십시오 — 첫 차단으로부터 **2초가 지나면 무조건 한 번 쏘고** 다시 모으는 식입니다.

### R6. 작업 기록이 무한정 커집니다

`OperationLog` 에 상한이 없습니다. 파일을 많이 다루면 `/var/mobile/Media/argo-operations.log` 가 계속 자랍니다. 애플TV 저장공간은 넉넉하지 않습니다.

1MB를 넘으면 뒤쪽 절반만 남기고 자르는 정도면 충분합니다.

---

## 하 (Minor)

| # | 내용 |
|---|---|
| m1 | `OperationLog.record` 가 기본 경로에 못 쓸 때마다 `createFile` 을 다시 시도합니다. 한 번 실패하면 세션 내내 폴백만 쓰도록 기억해 두십시오 |
| m2 | `effectivePathForWriteCheck` 가 호출마다 `attributesOfItem` 을 칩니다. 일괄 작업에서 파일 수만큼 디스크를 칩니다 |
| m3 | ~~`checkAuth` 가 영어 `"Safe Mode"` 문자열을 찾고 있다~~ — **제 오독입니다.** 그 코드는 없습니다. `GuardError.isSafeMode` 가 아무 데서도 안 쓰이는 죽은 코드인 것만 사실이고, 해로울 건 없습니다 |

---

## 참고 — 공개 소스 파일 관리자와 비교

Filza 는 **소스가 없습니다.** TIGI Software 의 상용 비공개 앱이고, GitHub 의 `FilzaJailedDS` · `Filza26Maker` 같은 저장소는 **배포 바이너리를 IPA 로 재포장하는 도구**지 소스가 아닙니다.

소스가 공개된 동급 앱은 `NSAntoine/Santander`(Swift, 샌드박스 밖 파일 관리자) 하나입니다. 받아서 봤습니다. **삭제가 이게 전부입니다.**

```swift
struct Delete: ParsableCommand {
    @Argument var paths: [URL]
    func run() throws {
        for path in paths {
            try FileManager.default.removeItem(at: path)
        }
    }
}
```

경로 검사도, 심볼릭 링크 선처리도, Zip Slip 검사도 없습니다. `/System` · `..` · `safeMode` 전부 검색했지만 한 건도 안 나옵니다. 압축 해제도 `Compression.shared.extract(path:to:)` 한 줄입니다.

**R2 에 대한 함의:** 루트 헬퍼까지 갖춘 앱이 `removeItem` 을 그냥 부릅니다. 폴더 안의 심볼릭 링크를 미리 끊는 코드를 **아무도 쓰지 않습니다.** 정황 증거일 뿐이지만 "지우는" 쪽에 무게가 실립니다. 확정은 여전히 위의 30초 실기기 시험입니다 — `removeItem` 이 tvOS 16 에서 하위 링크를 따라가는지는 남의 코드로 알 수 없습니다.

**R1 에 대한 함의:** 없습니다. Santander 가 Zip Slip 을 안 막는다고 우리 검사가 괜찮아지지 않습니다. **검사가 없는 것보다 열려 있는 검사가 더 나쁩니다** — 막고 있다고 착각하게 만들기 때문입니다.

---

## 정리

**4차 패키지 반영 완료**, 빌드 통과, `main` 병합 완료.

**R1 · R1-b · R3 · R4 · R5 · R6 은 제미나이에게 넘기지 않고 제가 직접 고쳤습니다** (`review/p1-round5`). `return .success(())` 를 `.failure` 로 바꾸는 수준이라 왕복 비용이 수정 비용보다 큽니다. 내용은 위 각 항목 그대로입니다.

| 항목 | 처리 |
|---|---|
| R1 (fail-open) | `refuseUncheckable` 하나로 모아 **막는** 쪽으로 전환 |
| R1-b (순회 종료) | `readStatus` 보존, `ARCHIVE_EOF` 일 때만 `.success` |
| R3 (하드링크) | `archive_entry_hardlink` 를 심볼릭 링크와 같은 검사에 포함 |
| R4 (메시지 적재) | 건수는 따로 세고 메시지는 20개 상한 |
| R5 (디바운스) | 첫 차단으로부터 2초 상한 추가 |
| R6 (로그 용량) | 1MB 초과 시 뒤쪽 절반만 유지, 폴백 전환 1회 기억 |

**남은 것은 R2 하나입니다.** 이것만 결정이 안 됩니다.

**R2 는 이번에 새로 생긴 자료 손실 위험입니다.** 고치기 전에는 **심볼릭 링크가 든 폴더를 옮기거나 지우지 마십시오.** 위의 30초 실기기 시험으로 "지울 것인가 고칠 것인가"가 바로 갈립니다.

제미나이는 **R2 를 기다리지 않고 2단계로 가면 됩니다.** R2 는 `TrashManager` 안의 함수 하나라 2단계(`ContentView` 분해, 이중 창)와 겹치지 않습니다.
