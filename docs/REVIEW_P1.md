# 1단계 검수 결과

> **3차 개정 — 반영 완료.** 3차 패키지의 수정 13쌍을 전부 검증하고 `main` 에 반영했습니다. CI 통과(실행 39736911739).
> **C1~C4, S1·S2·S3·S7·S8, M1~M4 해결.** 남은 것은 S4·S5·S6 과 M5·M6 이고, 제미나이가 2차 계획으로 잡은 것과 일치합니다.
> 아래 항목별로 해결 여부를 표시했습니다.

## 해결 현황

| 항목 | 내용 | 상태 |
|---|---|---|
| C1 | `rm` 에 `-rf` → 삭제 중단 | **해결** |
| C2 | `cp` 에 `-a` → 복사 중단 | **해결** |
| C3 | `mvtemp` 직접 spawn | **해결** — `self.rm(tempPath)` 복원 |
| C4 | `executeBatchDelete` 미정의 | **해결** (2차) |
| S1 | 안전 모드 기본값이 실제로는 꺼짐 | **해결** — `register(defaults:)` 로 한 곳에서 |
| S2 | 차단 이유가 안 보임 | **부분 해결** — 아래 참조 |
| S3 | 임시 폴더가 safe 목록에 못 들어감 | **해결** — `standardizingPath` |
| S4 | 압축 풀기 (Zip Slip) | 남음 |
| S5 | root 를 안 거치는 쓰기 7곳 | 남음 |
| S6 | 심볼릭 링크 재귀 보호 | 남음 |
| S7 | `fastRisk` 가 링크를 가림 | **해결** — 링크는 실제 판정 |
| S8 | 루트에서 전부 빨간 자물쇠 | **해결** — `.safe` 만 물려받음 |
| M1 | UI 문자열이 영어 | **해결** |
| M2 | 미리보기가 매번 디스크를 침 | **해결** — 미리 계산해 전달 |
| M3 | 번들 조상 폴더가 전부 금지 | **해결** — `==` 비교 |
| M4 | 대소문자 우회 | **해결** — 전부 소문자 비교 |
| M5 | 로그 위치를 모름 | 남음 |
| M6 | 실패한 작업도 성공처럼 기록 | 남음 |

## 남은 것 중 하나 — S2 의 나머지 절반

일괄 삭제 버튼은 이제 차단되면 한국어로 이유를 띄웁니다. 맞게 고치셨습니다.

그런데 **개별 작업은 여전히 조용히 실패합니다.**

```swift
class func rm(_ filePath: String) {
    guard checkAuth(action: "rm", paths: [filePath]) else { return }   // ← 그냥 돌아갑니다
```

`RootHelperActs` 의 여덟 메서드는 전부 반환값이 없습니다. 막히면 호출부는 알 길이 없고, 화면에서는 **아무 일도 안 일어난 것처럼** 보입니다. 파일 하나를 지웠는데 목록에 그대로 남아 있으면 사용자는 앱이 고장 난 줄 압니다.

`checkAuth` 안의 이 줄도 아직 영어를 찾고 있어 **영원히 false** 입니다.

```swift
let msg = (err.localizedDescription.contains("Safe Mode")) ? "Safe Mode Active" : "Forbidden Path"
```

메시지가 한국어로 바뀌었으니 이제 `err.localizedDescription` 을 그대로 쓰시면 됩니다. 콘솔 출력이라 당장 피해는 없지만, 남겨두면 나중에 헷갈립니다.

**제안:** `RootHelperActs` 가 차단 사유를 알릴 통로를 하나 두십시오. 예를 들어

```swift
enum RootHelperActs {
    /// 마지막으로 차단된 사유. 호출부가 읽어 알림으로 띄웁니다.
    static var lastBlockReason: String?
}
```

이것은 2차 묶음(S4~S6)과 같이 하시면 됩니다.

---

## 치명적 — 기능이 죽습니다

### C1. `RootHelperActs.rm` 에 `-rf` 를 넣으면 삭제가 전면 중단됩니다

```swift
spawn(command: helperPath, args: ["rm", "-rf", filePath], env: [], root: true)
```

**`helperPath` 는 `/bin/rm` 이 아닙니다.** `/bin/RootHelper`, 이 저장소의 `RootHelper/RootHelper.swift` 로 만든 자체 Swift 바이너리입니다. 인자 규약이 이렇습니다.

```swift
switch args[0] {
case "rm": helper.rm(args[1])              // FileManager.removeItem(atPath:)
case "cp": helper.cp(args[1], args[2])     // FileManager.copyItem
...
}
```

`-rf` 를 끼우면 `args[1]` 이 `-rf` 가 되어 **`-rf` 라는 이름의 파일을 지우려 하고 실패합니다.** 진짜 경로는 `args[2]` 로 밀려 무시됩니다.

**고칠 것: 원래대로 `["rm", filePath]`.** `removeItem` 은 폴더도 재귀로 지우므로 `-rf` 가 애초에 필요 없습니다.

### C2. `cp` 의 `-a` 도 같습니다

```swift
args: ["cp", "-a", filePath, fileDest]   // args[1]="-a", args[2]=filePath → 복사 전면 중단
```

**고칠 것: `["cp", filePath, fileDest]`.**

### C3. `mvtemp` 의 직접 spawn

```swift
spawn(command: helperPath, args: ["rm", "-rf", tempPath], ...)  // 임시파일이 안 지워짐
```

텍스트·헥스 편집기가 저장할 때마다 `/var/mobile/Media/temp` 가 남습니다. **고칠 것: 원래대로 `self.rm(tempPath)`.**

> 셋 다 같은 뿌리입니다. **헬퍼를 POSIX `rm`/`cp` 로 가정하셨습니다.** 동사 기반 자체 프로토콜(`tf`=touch file, `td`=touch dir, `ts`=symlink, `ch`=chmod)입니다.

---

## 컴파일 오류 — 해결됨

### C4. ~~`executeBatchDelete` 가 없습니다~~ — **해결됨 (2차)**

1차 패키지에 빠져 있던 조각이었습니다. 2차에서 `trimGroupBundleID` 뒤에 정의가 들어왔고, 휴지통 경로와 영구삭제 경로를 둘 다 가릅니다. **지금은 빌드가 통과합니다.**

---

## 안전 설계 결함 — 이게 더 중요합니다

### S1. **안전 모드가 설치 직후 실제로는 꺼져 있습니다**

설정 화면은 켜진 것처럼 보입니다.

```swift
@State private var safeModeEnabledPre =
    UserDefaults.settings.object(forKey: "safeModeEnabled") == nil ? true : ...
```

그런데 정작 판단하는 쪽은 이렇습니다.

```swift
let safeMode = UserDefaults.settings.bool(forKey: "safeModeEnabled")   // 미설정이면 false
```

**설정에는 초록 방패가 떠 있는데 실제로는 아무것도 안 막습니다.** 사용자가 설정에 들어가 한 번 껐다 켜야 비로소 켜집니다.

이 기능 전체에서 가장 중요한 기본값이 거짓말을 하는 상태입니다.

**고칠 것:** 앱 시작 시 한 곳에서 등록하고, 읽는 쪽도 그걸 쓰십시오.

```swift
// SpartanApp.swift init 등
UserDefaults.settings.register(defaults: ["safeModeEnabled": true])
```

`register(defaults:)` 는 디스크에 쓰지 않고 기본값만 깔아주므로, 이후 `bool(forKey:)` 가 true 를 돌려줍니다. 설정 화면의 삼항 연산자도 그러면 지울 수 있습니다.

### S2. 차단 이유가 화면에 안 나옵니다

```swift
enum GuardError: Error { case forbidden(String) ... }
...
alertMsg = err.localizedDescription
```

`GuardError` 가 `LocalizedError` 를 안 채택해서 `localizedDescription` 이 **"The operation couldn't be completed. (Argo.GuardError error 0.)"** 로 나옵니다. 넣어주신 설명 문자열은 화면에 도달하지 못합니다.

같은 이유로 `checkAuth` 의 이 줄도 **영원히 false** 입니다.

```swift
let msg = (err.localizedDescription.contains("Safe Mode")) ? ... : ...
```

지시서의 **"조용히 실패하지 말고 왜 막혔는지 띄울 것"** 이 사실상 안 지켜집니다. 이전에 삭제가 조용히 실패해서 원인 찾는 데 오래 걸린 적이 있어 일부러 적어둔 항목입니다.

**고칠 것:**

```swift
enum GuardError: LocalizedError {
    case forbidden(String)
    case safeModeActive(String)
    var errorDescription: String? {
        switch self {
        case .forbidden(let m), .safeModeActive(let m): return m
        }
    }
    var isSafeMode: Bool { if case .safeModeActive = self { return true }; return false }
}
```

### S3. 임시 폴더가 safe 목록에 못 들어갑니다

```swift
NSTemporaryDirectory().trimmingCharacters(in: CharacterSet(charactersIn: "/"))
```

**양끝을 깎아서 앞 슬래시까지 날아갑니다.** `private/var/.../tmp` 가 되어 `testPath.hasPrefix("private/var/.../tmp/")` 가 절대 맞지 않습니다 (testPath 는 `/` 로 시작합니다).

**고칠 것:** 뒤쪽만 다듬으십시오.

```swift
(NSTemporaryDirectory() as NSString).standardizingPath   // 뒤 슬래시 제거, 앞은 유지
```

### S4. 압축 풀기가 그대로 열려 있습니다 (Zip Slip)

지시서에서 따로 짚은 `Compression.swift:250` 에 손을 안 대셨습니다.

```swift
try Zip.unzipFile(..., overwrite: true, password: nil)
```

zip 안의 경로가 `../../System/Library/...` 면 **대상 폴더 밖으로 씁니다.** root 권한 탐색기에서는 `/System` 근처에 가보지도 않고 `/System` 에 쓸 수 있다는 뜻이라, 지금 만든 경로 차단을 통째로 우회합니다.

**고칠 것:** 풀기 전에 각 항목의 최종 경로를 계산해 대상 폴더 밖으로 나가는지, `PathSafety` 를 통과하는지 검사하십시오. `Zip.unzipFile` 은 항목별 후크가 없으니 먼저 목록을 뽑거나, 샌드박스 임시 폴더에 푼 뒤 검사해서 옮기는 방식이 안전합니다.

### S5. root 를 안 거치는 쓰기 7곳이 전부 무방비입니다

`RootHelperActs` 에 관문을 다신 것은 정확합니다. 다만 `GEMINI_P1_CONTEXT.md` 0장의 **두 번째 표**(root 헬퍼를 안 거치는 쓰기)에 손을 안 대셨습니다.

| 파일 | 줄 | 동작 |
|---|---|---|
| `Trash/TrashManager.swift` | 72, 82, 103, 117 | `FileManager` 1차 시도 — **여기가 먼저 돌고, 성공하면 root 경로로 안 갑니다** |
| `Compression.swift` | 211, 250, 366, 536 | 압축·해제 |
| `HexView.swift` | 113, 117 | `Data.write` 직접 덮어쓰기 |
| `TextView.swift` | 88, 92 | 텍스트 직접 덮어쓰기 |

특히 **TrashManager 가 중요합니다.** 샌드박스 권한만으로 지워지는 파일은 `FileManager` 단계에서 끝나므로 `RootHelperActs` 를 아예 안 거칩니다.

### S6. 심볼릭 링크 재귀 보호가 구현되지 않았습니다

요약에 *"심볼릭 링크를 안전하게 처리합니다"* 라고 적으셨는데, **패키지에 `TrashManager.swift` 교체 블록 자체가 없습니다.** 손대지 않으셨습니다.

지시서 1단계 7항입니다. 안전 구역 안에 `/System` 을 가리키는 링크가 하나 있으면 폴더 삭제가 그 너머까지 번집니다. 구현하시거나, **안 했다고 적어주십시오.** 확신 없는 부분을 적어달라고 한 것이 이 때문입니다.

### S7. `fastRisk` 가 바로 그 공격 경로를 가립니다

```swift
if parentRisk == .safe || parentRisk == .forbidden { return parentRisk }
```

성능 최적화 의도는 좋습니다 (300개 폴더에서 `resolvingSymlinksInPath` 를 매번 부르면 끊긴다는 판단은 정확합니다). 다만 **안전 폴더 안의 심볼릭 링크가 safe 로 표시됩니다.** 실제 차단은 `risk(of:)` 가 하니 치명적이진 않지만, 사용자에게 보이는 배지가 위험을 숨깁니다.

**제안:** 목록에서 심볼릭 링크(`FileInfo` 타입 8)는 `fastRisk` 를 건너뛰고 실제 판정을 하십시오. 폴더 하나에 링크는 보통 몇 개뿐이라 비용이 안 큽니다.

### S8. 루트에서 모든 항목이 빨간 자물쇠로 나옵니다 (2차 추가분)

`fastRisk` 가 `.forbidden` 도 아래로 물려줍니다.

```swift
if parentRisk == .safe || parentRisk == .forbidden { return parentRisk }
```

`/` 는 특례로 `.forbidden` 이고 `/private/var` 도 그렇습니다. 그래서 **루트를 열면 `/var` 를 포함한 모든 항목에 빨간 자물쇠가 붙습니다.** `/var/mobile/Media` 는 실제로 safe 인데도, 거기까지 가는 길이 전부 "쓰기 금지"로 보입니다.

`/var/mobile` 에서는 반대로 **모든 항목에 주황 경고 삼각형**이 붙습니다. 그 안의 `Media` 도 포함해서요. 파일 300개짜리 폴더면 삼각형 300개입니다.

배지의 목적은 **눈에 띄는 것**인데, 전부 붙으면 아무것도 안 띄는 것과 같습니다.

**제안:** `.safe` 만 물려주고 나머지는 실제 판정을 하십시오.

```swift
static func fastRisk(parentRisk: PathRisk, fullPath: String) -> PathRisk {
    if parentRisk == .safe { return .safe }      // 안전 구역 안은 전부 안전 — 여기가 대부분
    return risk(of: fullPath)                     // 그 밖은 제대로 판정
}
```

안전 구역 밖 폴더는 항목 수가 대개 적어서 비용이 크지 않습니다. 성능을 걱정하신 판단 자체는 맞았습니다 — 300개 폴더는 거의 언제나 `Media` 안이고, 그쪽은 위 코드에서 디스크를 안 칩니다.

덧붙여 **심볼릭 링크는 예외로 두십시오.** 안전 폴더 안의 링크가 `/System` 을 가리켜도 safe 로 보입니다 (S7과 같은 뿌리).

---

## 사소한 것

| # | 내용 |
|---|---|
| M1 | UI 문자열이 전부 영어입니다. 나머지 앱은 한국어고, 3미터에서 읽는 화면입니다 |
| M2 | `BatchDeletePreview` 의 `body` 안에서 `files.filter { PathSafety.risk(...) }` 를 돌립니다. **다시 그릴 때마다 파일 수만큼 디스크를 칩니다.** 미리 계산해 넘기십시오 |
| M3 | `bundlePath.hasPrefix(testPath)` 가 번들의 **모든 조상 폴더**를 forbidden 으로 만듭니다. 실질 피해는 없지만 의도와 다릅니다 |
| M4 | 파일시스템은 대소문자를 안 가리는데 `hasPrefix` 는 가립니다. `/private/var/Containers` 같은 표기로 우회됩니다. 비교 전에 소문자로 맞추십시오 |
| M5 | `OperationLog` 가 root 헬퍼를 안 거쳐서, `/var/mobile/Media` 쓰기가 막히면 조용히 샌드박스로 폴백합니다. 어디에 쌓였는지 사용자가 알 길이 없습니다 |
| M6 | `OperationLog.record` 가 `spawn` 성공 여부와 무관하게 기록합니다. 실패한 작업도 성공처럼 남습니다 |

---

## 잘하신 것

- **교체 블록 7개 전부 정확히 한 번씩 일치.** 공백·탭 어긋남 없음. 이것만으로 검수가 훨씬 빨랐습니다
- `RootHelperActs` 를 관문으로 고른 판단이 정확합니다. 열세 군데를 한 곳으로 모았습니다
- `showSubView` 를 건드리지 않고 별도 `@State` 로 간 것 — 더 안전합니다
- 배너에 `allowsHitTesting(false)` — 포커스를 안 뺏는 게 맞습니다
- `fastRisk` 로 스크롤 성능을 걱정한 것 — 문제 인식이 정확했습니다
- 안전 모드를 **끌 때만** 경고하는 설계 — 맞습니다
- 위험 배지를 `Image` 로만 넣어 **포커스 규칙을 안 깬 것** — 한 행에 포커스 버튼은 하나여야 합니다
- 2차에서 빠진 조각을 정확히 채워 주신 것 — `executeBatchDelete` 가 휴지통·영구삭제 두 경로를 제대로 가릅니다

---

## 고칠 순서

```
1. C1·C2·C3      헬퍼 인자 원복            ← 가장 급합니다. 삭제·복사가 죽습니다
2. S1            안전 모드 기본값           ← 기능의 의미 자체
3. S2            GuardError: LocalizedError
4. S3            임시 폴더 경로
5. S7·S8         배지 판정 (fastRisk)
6. S4·S5·S6      나머지 쓰기 경로와 링크 보호
7. M1~M6
```

**1~4번만 고치면 릴리즈해서 시험할 수 있습니다.** 다섯 줄 남짓입니다. 5~7번은 그다음 묶음으로 하셔도 됩니다.

**확인한 환경:** 브랜치 `review/p1-safety`, CI 실행 39735544743 — **빌드 통과, 오류·경고 없음.** 릴리즈는 만들지 않았습니다.
