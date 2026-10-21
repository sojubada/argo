# 1단계 지시서 — 안전장치

`HANDOVER.md` 를 먼저 읽으십시오. 특히 2장(실패 사례)과 2-3(tvOS UI 함정).

---

## 왜 이것부터인가

Argo는 **root 권한으로 돕니다.** TrollStore가 `com.apple.private.security.no-sandbox` 를 넣어 주고, 안 되면 루트 헬퍼로 다시 시도합니다. 즉 `/System` 안의 파일도 지워집니다.

지금 그것을 막는 장치가 **하나도 없습니다.** 리모컨으로 폴더를 훑다가 실수로 선택하고 삭제를 누르면 애플TV가 부팅을 못 하게 될 수 있습니다. 복구는 DFU 복원뿐이고, 그러면 TrollStore도 날아갑니다.

**2단계(두 칸 탐색기)와 3단계는 파일을 더 많이, 더 빨리 다루게 만드는 작업입니다.** 보호 장치 없이 그걸 먼저 하면 위험만 커집니다.

---

## 만들 것

### 1. 경로 등급 — `PathSafety.swift` (새 파일)

```swift
enum PathRisk {
    case safe       // 마음껏
    case caution    // 경고 후 확인
    case forbidden  // 쓰기 금지 (읽기는 허용)
}

enum PathSafety {
    static func risk(of path: String) -> PathRisk
    static func reason(for path: String) -> String   // 왜 그 등급인지, 사람이 읽을 말로
    static func isWriteAllowed(_ path: String) -> Bool
}
```

**금지 (쓰기 전면 차단)** — 이 아래 어디든:
```
/System        /usr        /bin        /sbin       /lib
/Library       /Applications           /private/var/db
/private/var/containers                /private/var/installd
/private/var/MobileSoftwareUpdate      /dev        /cores
/private/var/preferences                /private/var/keybags
/  (루트 자체)
```
앱 자신의 번들과 컨테이너도 금지입니다. 자기 자신을 지우면 복구가 안 됩니다.

**주의 (경고 + 확인)** — 안전 구역 밖의 나머지 전부. 예: `/private/var/mobile/Library`, `/private/var/root`, `/private/var/log`

**안전** — 다음 아래:
```
/private/var/mobile/Media
/private/var/mobile/Documents
/private/var/mobile/Downloads
NSTemporaryDirectory()
앱 샌드박스의 Documents
```

**주의: `/var` 는 `/private/var` 의 심볼릭 링크입니다.** 판정 전에 `URL(fileURLWithPath:).resolvingSymlinksInPath()` 로 정규화하고, 경로 비교는 반드시 **구분자까지 포함**해 하십시오 (`/Systemic` 이 `/System` 에 걸리면 안 됩니다).

### 2. 폴더에 들어갈 때 1초 배너

사용자 요청: *"시스템에 영향을 미치는 폴더로 이동할 경우 짧게(약 1초) 시스템 폴더 경고 메시지"*

- **금지** 폴더 진입: 빨간 배너 **"시스템 폴더 — 쓰기가 차단됩니다"** + 폴더 이유 한 줄
- **주의** 폴더 진입: 노란 배너 **"주의 — 시스템에 영향을 줄 수 있습니다"**
- **안전**: 배너 없음
- 1.2초 뒤 사라짐. 화면 위쪽 가운데. **포커스를 가져가면 안 됩니다** — 읽던 흐름을 끊지 않게
- 같은 폴더에 다시 들어가도 매번 띄웁니다 (익숙해져서 무시하는 것보다 낫습니다)

### 3. 쓰기 작업 차단

`TrashManager`, 복사, 이동, 이름변경, 압축풀기, 권한변경 — **모든 쓰기 경로**가 실행 전에 `PathSafety.isWriteAllowed` 를 통과해야 합니다.

막혔을 때는 **조용히 실패하지 말고** 왜 막혔는지 화면에 띄우십시오. (이전에 삭제가 조용히 실패해서 원인 찾는 데 오래 걸렸습니다.)

차단 지점을 한 곳으로 모으십시오. 호출부마다 검사를 흩뿌리면 빠뜨립니다.

```swift
enum FileGuard {
    /// 모든 파괴적 작업은 여기를 지나갑니다.
    static func authorize(_ op: Operation, paths: [String]) -> Result<Void, GuardError>
}
```

### 4. 안전 모드 (설정, 기본 켜짐)

설정에 **"안전 모드"** 스위치. 켜져 있으면:
- 안전 구역 **밖의 모든 쓰기**가 막힙니다 (주의 구역 포함)
- 읽기·탐색·문서 열기는 그대로

끄려면 **확인 한 번** 더 받으십시오: "시스템 파일을 바꿀 수 있게 됩니다. 애플TV가 부팅하지 못할 수 있습니다."

### 5. 작업 기록 — `OperationLog.swift` (새 파일)

지운·옮긴·이름 바꾼 것을 전부 남깁니다. 뭔가 잘못됐을 때 **무슨 일이 있었는지 알 수 있어야 합니다.**

```
2026-10-22 09:14:03  DELETE  /var/mobile/Media/a.pdf  (→ 휴지통)
2026-10-22 09:14:55  MOVE    /var/mobile/Media/b.zip  →  /var/mobile/Documents/
```

`/var/mobile/Media/argo-operations.log` 에 덧붙입니다. 실패하면 샌드박스 Documents로. 설정에서 볼 수 있게.

### 6. 일괄 작업 미리보기

여러 개를 지우거나 옮기기 전에 **무엇이 어떻게 되는지** 먼저 보여주십시오.

```
지울 것 12개 (3.4 GB)
  보고서.pdf
  사진/  (파일 847개)
  …외 10개
  ⚠ 2개가 주의 구역에 있습니다
                        [ 취소 ]  [ 휴지통으로 ]
```

### 7. 재귀 삭제의 심볼릭 링크

폴더를 지울 때 **심볼릭 링크를 따라가면 안 됩니다.** `/var/mobile/Media` 안에 `/System` 을 가리키는 링크가 있으면 그 너머까지 지워집니다. 링크는 링크만 지우십시오.

`FileManager.enumerator` 에 `.skipsPackageDescendants` 와 함께 링크를 건너뛰도록 하고, 루트 헬퍼로 `rm -rf` 를 쓸 때도 마찬가지입니다.

---

## 점검 항목 (이걸로 검수합니다)

| # | 항목 | 통과 기준 |
|---|---|---|
| S1 | `/System` 에 들어가면 빨간 배너가 1초 뜬다 | 포커스를 뺏지 않음 |
| S2 | `/System` 안에서 삭제가 막히고 **이유가 뜬다** | 조용히 실패하지 않음 |
| S3 | `/var/mobile/Library` 에서 노란 배너 | |
| S4 | `/var/mobile/Media` 에서는 배너 없음 | |
| S5 | `/var` 와 `/private/var` 가 같게 판정된다 | 심볼릭 링크 정규화 |
| S6 | `/Systemic` 같은 이름이 `/System` 으로 오판되지 않는다 | 구분자 포함 비교 |
| S7 | 안전 모드가 기본으로 켜져 있다 | |
| S8 | 안전 모드를 끌 때 경고가 뜬다 | |
| S9 | 앱 자신의 번들을 지울 수 없다 | |
| S10 | 일괄 삭제 전에 목록과 개수가 보인다 | |
| S11 | 심볼릭 링크를 지워도 대상은 멀쩡하다 | |
| S12 | 작업 기록이 남는다 | 시각·동작·경로 |
| S13 | 휴지통이 금지 구역 파일을 받지 않는다 | |
| S14 | 안전 모드에서도 **읽기와 문서 열기는 된다** | |

---

## 하지 말 것

- 기존에 도는 기능을 건드리지 마십시오 — 휴지통·즐겨찾기·WebDAV·문서 뷰어
- `showSubView` 배열을 건드렸다면 **반드시 알려 주십시오** (현재 39)
- 읽기까지 막지 마십시오. 보는 것은 위험하지 않습니다
- 확인 대화상자를 두 번 세 번 겹치지 마십시오. 리모컨으로는 고역입니다
