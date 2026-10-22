# 4차 검토 — 제미나이 반박에 대한 답

먼저 판정입니다.

| 반박 | 판정 |
|---|---|
| ① `static var` 로는 SwiftUI 갱신이 안 된다 | **맞습니다.** 제 제안이 부실했습니다. 다만 두 가지가 더 필요합니다 |
| ② 임시 폴더 해제는 용량이 두 배 든다 | **절반만 맞습니다.** 스테이징 자체가 필요 없습니다 |
| ③ `spawn` 직후 검사는 경합이 난다 | **틀렸습니다.** 근거를 아래 답니다 |

그런데 셋보다 중요한 것을 찾았습니다. **지금 만든 안전장치가 거의 작동하지 않는 자리에 걸려 있습니다.** 마지막 장입니다.

---

## ① `static var` — 지적이 맞습니다

평범한 `static var` 는 SwiftUI 갱신을 일으키지 않습니다. 제가 `lastBlockReason` 을 스케치로 적으면서 **어떻게 관찰할지를 안 적었습니다.** 그 지적이 정확합니다.

`NotificationCenter` 로 하셔도 됩니다. 다만 **두 가지가 빠지면 새 버그가 생깁니다.**

### 메인 스레드

`RootHelperActs` 는 백그라운드에서 불립니다. 휴지통 일괄 작업이 그렇습니다. 거기서 바로 알림을 쏘고 `.onReceive` 가 `@State` 를 건드리면 **"Publishing changes from background threads is not allowed"** 가 뜨고 운이 나쁘면 죽습니다.

```swift
DispatchQueue.main.async {
    NotificationCenter.default.post(name: .fileGuardBlocked, object: reason)
}
```

### 묶음 처리

파일 50개를 지우는데 전부 막히면 **알림이 50번 날아갑니다.** 경고창 50개가 줄 서거나, 마지막 하나만 보이고 나머지는 사라집니다.

차단당한 건수를 모았다가 한 번만 띄우십시오.

```
"12개가 차단되었습니다 — 시스템 폴더"
```

`FileGuard.authorize` 는 경로 묶음을 받아 **첫 실패에서 돌아옵니다.** 일괄 삭제 버튼은 그래서 알림이 하나지만, `RootHelperActs` 는 파일마다 따로 불리므로 거기서 묶어야 합니다.

### 참고

`ObservableObject` + `@Published` 도 같은 일을 합니다. `NotificationCenter` 보다 이름 오타가 없고 타입이 잡히지만, **어느 쪽이든 위 두 가지는 똑같이 필요합니다.** 선택은 맡기겠습니다.

---

## ② Zip Slip — 스테이징이 필요 없습니다

용량 걱정은 타당합니다. 다만 **전제가 틀렸습니다. 압축을 풀기 전에 항목 이름만 읽을 수 있습니다.**

`Compression.swift` 가 이미 이렇게 시작합니다.

```swift
import libarchiveBridge
import Zip
```

**libarchive 가 이미 링크돼 있습니다.** 319행부터 `archive_write_*` 로 압축을 만들고 있고요. 읽기 쪽도 그대로 쓸 수 있습니다.

```c
archive_read_new()
archive_read_support_format_all(a)
archive_read_open_filename(a, path, 10240)
while (archive_read_next_header(a, &entry) == ARCHIVE_OK) {
    archive_entry_pathname(entry)      // ← 한 바이트도 안 풀고 이름만
    archive_read_data_skip(a)
}
```

중앙 디렉터리 한 번 훑는 것이라 **디스크를 안 쓰고, I/O도 한 번**입니다. 전부 통과하면 지금처럼 바로 목적지에 풀고, 하나라도 걸리면 **아예 시작하지 않습니다.** 스테이징도 복사도 없습니다.

### 검사할 것이 셋입니다

`../` 만으로는 부족합니다.

| # | 공격 | 검사 |
|---|---|---|
| 1 | `../../System/...` | 정규화한 최종 경로가 목적지 밖으로 나가는가 |
| 2 | `/System/...` (절대 경로) | 항목 이름이 `/` 로 시작하는가 |
| 3 | **심볼릭 링크 항목** | `archive_entry_filetype` 이 `AE_IFLNK` 인가 |

3번이 교묘합니다. 압축 안에 `evil -> /System/Library` 라는 **링크를 먼저 풀고**, 다음 항목이 `evil/foo` 면 `/System/Library/foo` 에 씁니다. 이름만 보면 `evil/foo` 라 1·2번 검사를 전부 통과합니다.

**목적지 밖을 가리키는 링크 항목은 거절하십시오.** `archive_entry_symlink` 로 대상을 읽을 수 있습니다.

> `defer` 로 임시 공간을 비우자는 제안은 (스테이징을 안 하면 쓸 일이 없지만) 원칙으로는 옳습니다. 다른 곳의 임시 폴더에도 적용해 두시면 좋겠습니다.

---

## ③ `spawn` 경합 — 그런 경합은 없습니다

`spawn` 은 **두 경로 모두 동기입니다.** 저장소에서 확인했습니다.

**탈옥 경로** — `External/CommandRunner.swift`

```swift
mutex.wait()
mutex.wait()
var status: Int32 = 0
waitpid(pid, &status, 0)      // ← 자식이 끝날 때까지 막습니다
```

**TrollStore 경로** — `External/TSUtil.m`

```objc
if (waitpid(task_pid, &status, 0) != -1) { ... }
...
return WEXITSTATUS(status);   // ← 종료 코드까지 돌려줍니다
```

`spawn` 이 돌아왔다는 것은 헬퍼 프로세스가 **이미 끝났다**는 뜻이고, 헬퍼 안의 `FileManager.removeItem` 도 이미 반환했다는 뜻입니다. POSIX 는 완료된 `unlink`·`rename` 이 **다른 프로세스의 다음 `stat` 에 보이는 것을 보장합니다.**

말씀하신 "디스크 동기화 지연"은 **내구성(fsync)** 의 문제입니다 — 전원이 꺼져도 남아 있느냐. **가시성** 과는 다른 층위입니다. 같은 커널의 다른 프로세스가 보는 데는 지연이 없습니다.

`asyncAfter` 로 딜레이를 주면 오히려 **새 문제가 생깁니다.** 기록이 비동기가 되어, 사용자가 이미 다른 폴더로 옮긴 뒤에 로그가 찍히고, 연달아 작업하면 순서가 섞입니다.

### 더 나은 방법이 이미 있습니다

`spawn` 은 **stdout+stderr 를 문자열로 돌려줍니다.** 그리고 헬퍼는 실패하면 이렇게 찍습니다.

```swift
print("Failed to delete file: \(error.localizedDescription)")
```

반환 문자열만 보면 됩니다. 파일시스템을 다시 칠 필요가 없습니다.

```swift
let out = spawn(command: helperPath, args: ["rm", filePath], env: [], root: true)
let ok = !out.contains("Failed to")
if ok { OperationLog.record(...) }
```

`fileExists` 교차 검증을 더하셔도 좋습니다. **동기이므로 정확합니다.** 다만 둘 중 하나면 충분합니다.

---

## 그보다 큰 것 — 지금 안전장치가 죽은 길목을 지키고 있습니다

`TROLLSTORE` 라는 컴파일 플래그가 **저장소 어디에도 정의돼 있지 않습니다.** `project.pbxproj` 에도, 빌드 워크플로에도 없습니다.

```swift
#if TROLLSTORE
    isJB = false
    helperPath = Bundle.main.path(forResource: "RootHelper", ofType: nil)
#else
    print("We are jailbroken!")     // ← 실제로 여기로 갑니다
#endif
```

그래서 지금 빌드는 이렇습니다.

- `isJB` 가 `true` 로 남습니다
- `helperPath` 가 **`/bin/RootHelper`** 로 남습니다
- 그런데 **`RootHelper` 바이너리는 IPA 안에 없습니다.** 지난 릴리즈를 풀어 확인했습니다
- `/bin` 은 읽기 전용 시스템 볼륨입니다. 탈옥 없이는 아무것도 설치할 수 없습니다

**즉, `RootHelperActs` 호출은 전부 조용히 실패해 왔을 가능성이 큽니다.**

그런데도 휴지통이 동작했습니다. **`TrashManager` 가 `FileManager` 를 먼저 시도하기 때문입니다.** 앱에 `no-sandbox` 와 `absolute-path.read-write: /` 권한이 있어서, mobile 사용자가 쓸 수 있는 곳이면 `FileManager` 로 그냥 써집니다. `/System` 은 root 소유라 EPERM 으로 막히고요.

**애플TV를 지켜온 것은 앱의 안전장치가 아니라 POSIX 권한이었습니다.**

### 이것이 뜻하는 바

1. **`-rf` 버그가 시험에서 안 잡힌 이유**가 설명됩니다. 헬퍼가 애초에 안 돌았습니다
2. **`RootHelperActs` 에 건 `FileGuard` 는 거의 안 타는 길을 지킵니다.** 관문을 고른 판단 자체는 옳았지만, 지금 빌드에서는 효력이 적습니다
3. **S5 가 "다음 묶음"이 아니라 핵심입니다.** 실제 쓰기는 `TrashManager` 의 `FileManager` 1차 시도, `Compression`, `HexView`, `TextView` 로 갑니다. 그 일곱 곳이 안전장치의 전부입니다

### 확인은 30초면 됩니다

Argo 로 **`/bin`** 을 열어 `RootHelper` 가 있는지 보십시오. 없으면 위 분석이 맞습니다.

### 그래서 어떻게

**S5 를 먼저 하십시오.** 순서를 바꾸는 것이 맞습니다.

그리고 별도로 정해야 할 것이 있습니다. **루트 헬퍼를 살릴 것인가.**

- **살린다면**: `TROLLSTORE` 플래그를 빌드에 정의하고, `RootHelper` 타깃을 앱 번들에 넣고, 서명 단계에 포함해야 합니다. 그러면 root 쓰기가 실제로 가능해지고 — **그 순간 `/System` 을 지울 수 있게 됩니다.** 안전장치가 비로소 진짜로 필요해집니다
- **안 살린다면**: 지금처럼 mobile 권한으로만 돌고, `/System` 은 POSIX 가 막습니다. 안전장치는 "mobile 이 쓸 수 있는 곳 중 위험한 곳"(예: `/var/mobile/Library`)을 지키는 역할이 됩니다

**어느 쪽이든 S5 는 필요합니다.** 다만 전자를 택한다면 S4·S6 의 무게가 훨씬 커집니다.

---

## 정리

| | |
|---|---|
| **반영** | ① 맞습니다. 메인 스레드 전환과 묶음 처리를 더하십시오 |
| **대안** | ② libarchive 로 이름만 먼저 읽으십시오. 스테이징 불필요. 심볼릭 링크 항목도 검사 |
| **철회 요청** | ③ `spawn` 은 동기입니다. 딜레이는 넣지 마십시오. 반환 문자열이나 `fileExists` 를 바로 쓰십시오 |
| **순서 변경** | S5 를 먼저. 지금 실제 쓰기가 전부 그리로 갑니다 |
| **확인 요청** | `/bin/RootHelper` 가 기기에 있는지 |
