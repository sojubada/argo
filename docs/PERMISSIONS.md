# 권한 — 원본 Spartan 과 Argo 비교

> 요청하신 "초기 버전 스파르탄의 읽기·쓰기·삭제 권한" 확인 결과입니다.

---

## 결론 먼저

삭제가 안 된 것은 **권한 설계가 엄격해서가 아니라 경로 비교가 깨져 있어서**입니다. 안전 구역이 **하나도 매칭되지 않아** 모든 폴더가 `.caution` 으로 떨어졌고, 안전 모드가 기본 켜짐이라 **모든 쓰기가 막혔습니다.**

고쳤습니다. 그리고 **설계도 원본에 맞춰 넓혔습니다.**

---

## 1. 원본 Spartan 의 권한 모델

전부 이 한 함수입니다.

```swift
func filePathIsNotMobileWritable(_ fullPath: String) -> Bool {
    return ((fullPath.count < 19) || (fullPath.substring(toIndex: 19) != "/private/var/mobile"))
}
```

**`/private/var/mobile` 아래는 전부 쓰기 가능**으로 봤습니다. 그 아래를 더 쪼개지 않았습니다.

그리고 **쓰인 곳이 세 군데뿐입니다.**

| 파일 | 쓰임 |
| --- | --- |
| `TextView.swift` | 직접 쓸지, 루트 헬퍼로 보낼지 결정 |
| `HexView.swift` | 같음 |
| `PlistView.swift` | 같음 |

**삭제·이동·복사·압축에는 경로 제한이 아예 없었습니다.** 어디서든 됐고, 실제로 막는 것은 POSIX 권한뿐이었습니다.

---

## 2. 제가 만든 모델

세 단계입니다.

| 등급 | 뜻 |
| --- | --- |
| `forbidden` | 시스템 핵심. 안전 모드와 무관하게 **항상** 차단 |
| `caution` | 애매한 구역. **안전 모드가 켜져 있으면 차단** (기본 켜짐) |
| `safe` | 사용자 영역. 항상 허용 |

그리고 **모든 파괴적 쓰기가 `FileGuard.authorize` 를 지납니다** — 삭제·이동·복사·압축·편집기 저장 전부.

원본보다 훨씬 넓게 적용됩니다. 벽돌 방지가 목적이었으니 거기까지는 의도한 것입니다.

---

## 3. 그런데 버그가 있었습니다 ★

안전 목록을 이렇게 적었습니다.

```swift
"/private/var/mobile/Media"
"/private/var/mobile/Documents"
"/private/var/mobile/Downloads"
```

판정은 이렇게 했습니다.

```swift
let normalized = URL(fileURLWithPath: path).resolvingSymlinksInPath().path
```

**`resolvingSymlinksInPath()` 는 `/private` 접두사를 떼어냅니다.** Foundation 의 문서화된 동작입니다 — "원본 URL 이 `/private` 로 시작하면 그 부분을 떼어낸다".

그래서:

```
/private/var/mobile/Media/Radio/   →  /var/mobile/Media/Radio   →  목록과 불일치
/private/var/mobile/Downloads/a.log →  /var/mobile/Downloads/a.log →  목록과 불일치
```

**안전 구역이 하나도 안 맞았습니다.** 전부 `.caution` 으로 떨어졌고, 안전 모드가 기본 켜짐이라 **자기 파일도 못 지웠습니다.**

화면에 노란 삼각형이 온통 떠 있던 것, "1개가 주의 구역에 있습니다" 가 Downloads 에서 뜬 것, 삭제가 조용히 안 된 것 — 전부 이 한 줄입니다.

금지 목록의 `/private/var/db` 같은 것들도 같은 이유로 **안 맞았습니다.** 즉 **막아야 할 곳은 덜 막고, 열어야 할 곳은 더 막는** 상태였습니다. 양쪽으로 틀렸습니다.

### 고친 방법

경로와 목록을 **같은 모양으로** 만든 뒤 비교합니다.

```swift
private static func canonical(_ path: String) -> String {
    var p = path
    if p.hasPrefix("/private/") { p = String(p.dropFirst("/private".count)) }
    while p.count > 1 && p.hasSuffix("/") { p.removeLast() }
    return p.lowercased()
}
```

---

## 4. 안전 범위를 원본에 맞췄습니다

```swift
private static let safePrefixes = [
    "/var/mobile",                 // ← 원본의 /private/var/mobile 과 같은 범위
    NSTemporaryDirectory()
]
```

**셋만 열어 두면 `Library/` 나 직접 만든 폴더가 전부 '주의'가 됩니다.** 안전 모드가 기본 켜짐이니 자기 집 안에서도 못 지웁니다. 그건 파일 탐색기가 아닙니다.

**벽돌을 막는 것은 금지 목록이지 집 안을 좁히는 것이 아닙니다.** 금지 목록은 그대로입니다.

```
/System  /usr  /bin  /sbin  /lib  /Library  /Applications  /dev  /cores
/var/db  /var/containers  /var/installd  /var/MobileSoftwareUpdate
/var/preferences  /var/keybags  (앱 번들 자신)
```

### 지금 상태

| 경로 | 판정 |
| --- | --- |
| `/var/mobile/**` (Media·Documents·Downloads·Library·직접 만든 폴더 전부) | **safe** |
| `/var/` 의 나머지 | caution (안전 모드 켜짐이면 차단) |
| 위 금지 목록 | **forbidden** (항상 차단) |

원본보다 **여전히 보수적입니다** — 원본은 삭제·이동·복사에 아무 제한이 없었습니다. 다만 사용자의 집 안에서는 원본과 똑같이 자유롭습니다.

---

## 5. 삭제가 안 되던 두 번째 버그

권한과 별개로 하나 더 있었습니다.

미리보기는 **길게 누른 파일**을 보여주는데, 실제 삭제는 `multiSelectFiles` (여러 개 고르기 목록)를 돌았습니다. 길게 눌러 한 개를 지우면 그 배열이 **비어 있어** 아무 일도 안 일어났습니다. 첫 사진의 **"지울 것 0개"** 도 같은 뿌리입니다 — 일괄 삭제 버튼이 미리보기를 띄우면서 지울 목록을 채우지 않았습니다.

이제 **지울 목록을 한 곳(`pendingDeletePaths`)에 담고**, 미리보기와 실제 삭제가 같은 것을 봅니다. 그리고 관문에 막히면 **왜 막혔는지 알려 줍니다** — 조용히 아무 일도 안 일어나는 것이 가장 나쁩니다.

---

## 6. 더 풀지 말지는 결정하실 일입니다

지금도 원본보다 조이는 지점이 둘 있습니다.

1. **`/var/` 의 mobile 바깥**(`/var/log`, `/var/tmp` 등)이 `caution` 입니다 → 안전 모드 켜짐이면 차단
2. **삭제·이동·복사에 관문이 걸려 있습니다** → 원본에는 없던 것

원본처럼 완전히 풀려면 **설정에서 안전 모드를 끄시면 됩니다.** 그러면 `caution` 이 전부 통과하고 `forbidden` 만 남습니다 — `/System` 류만 막히는, 원본에 가장 가까운 상태입니다.

**안전 모드를 기본 꺼짐으로 바꿀지**는 말씀해 주시면 한 줄입니다. 다만 켜 두는 쪽을 권합니다 — 실수로 `/var/db` 를 지우는 것을 막아 주고, 집 안에서는 어차피 아무것도 안 막습니다.
