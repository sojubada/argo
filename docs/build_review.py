#!/usr/bin/env python3
"""Assemble the code-review documents from the real sources.

Code is pulled from disk rather than retyped, so what the reviewer reads is
what compiles.
"""
import io
import os

ROOT = "/home/claude/argo"


def read(path):
    return io.open(os.path.join(ROOT, path), encoding="utf-8").read().rstrip("\n")


def lines_of(path, start, end):
    """1-indexed inclusive range."""
    all_lines = io.open(os.path.join(ROOT, path), encoding="utf-8").read().split("\n")
    return "\n".join(all_lines[start - 1:end]).rstrip("\n")


def block(path, lang="swift", start=None, end=None, label=None):
    body = lines_of(path, start, end) if start else read(path)
    where = label or (path if not start else "%s:%d-%d" % (path, start, end))
    count = len(body.split("\n"))
    return "### `%s`  (%d줄)\n\n```%s\n%s\n```\n" % (where, count, lang, body)


def write(path, text):
    out = os.path.join(ROOT, "docs", path)
    io.open(out, "w", encoding="utf-8").write(text)
    print("wrote docs/%s  (%d줄)" % (path, len(text.split("\n"))))


P1 = """# 코드 리뷰 — 1단계 안전장치

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

"""


P1 += block("Spartan/Safety/PathSafety.swift")

P1 += """
**볼 곳**

- `fastRisk` 가 `.safe` 만 아래로 상속합니다. 한때 `.forbidden` 도 상속해 `/` 의 모든 항목이 빨간 자물쇠가 됐습니다
- 접두사 목록이 `lowercased()` 되어 있습니다. iOS 파일시스템은 대소문자를 구분하지 않으므로 `/SYSTEM/` 도 막혀야 합니다
- 심볼릭 링크는 상속을 건너뛰고 항상 자기 경로로 판정합니다 — 안전한 폴더 안의 링크가 시스템을 가리킬 수 있습니다

---

## 2. 관문 — 모든 쓰기가 지나는 한 곳

"""

P1 += block("Spartan/Safety/FileGuard.swift")

P1 += """
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

"""

P1 += block("Spartan/Safety/OperationLog.swift")

P1 += """
**볼 곳**

- 상한 1MB, 넘으면 뒤쪽 절반만 남기고 **줄 경계에서** 자릅니다 (반쪽 줄이 남지 않게)
- `primaryUnusable` — 기본 경로에 한 번 실패하면 세션 내내 폴백만 씁니다. 전에는 작업마다 `createFile` 을 다시 시도했습니다
- `.atomic` 으로 씁니다 — 자르는 중에 죽어도 로그가 깨지지 않습니다

---

## 4. 휴지통 — 실제 삭제가 전부 지나는 곳

이 파일이 가장 길고 **R2 가 여기 있습니다.**

"""

P1 += block("Spartan/Trash/TrashManager.swift")

P1 += """
**볼 곳**

- 모든 공개 진입점이 `FileGuard` 를 먼저 지나는가
- **`robustRemove` 의 `unlinkNestedSymlinksIfDirectory` 호출 위치** ← R2. `removeItem` 보다 **먼저** 부릅니다
- `robustMove` 가 목적지를 비우려 `robustRemove` 를 부르는 경로 ← R2 의 더 날카로운 쪽
- 되살리기가 원래 경로를 복원할 때도 관문을 지나는가
- 휴지통 자체(`.Trash`)가 `.caution` 으로 분류되어 비우기가 막히지는 않는가

---

## 5. 루트 헬퍼 관문

"""

P1 += block("Spartan/SpareViews.swift", start=334, end=403,
            label="Spartan/SpareViews.swift:334-403 (RootHelperActs)")

P1 += """
**볼 곳**

- `checkAuth` 가 **단일 관문**입니다. 8개 함수가 전부 이것을 지납니다 — 13곳을 고치는 대신 한 곳을 고친 결과입니다
- `spawn` 은 **양쪽 경로에서 동기**입니다 (탈옥 경로는 `waitpid`, TrollStore 경로는 `waitpid` + `WEXITSTATUS`). 그래서 `spawn` 직후의 `fileExists` 교차 검증이 유효합니다 — 딜레이가 필요 없습니다
- 위에 적은 대로 **이 코드는 실제로 돌지 않습니다.** `TROLLSTORE` 가 정의되지 않고 바이너리가 IPA 에 없습니다

---

## 6. 압축 관문

"""

P1 += block("Spartan/FileOpeners/Compression/Compression.swift", start=102, end=140,
            label="Spartan/FileOpeners/Compression/Compression.swift:102-140 (오류형)")
P1 += block("Spartan/FileOpeners/Compression/Compression.swift", start=176, end=290,
            label="Spartan/FileOpeners/Compression/Compression.swift:176-290 (세 진입점)")

P1 += """
**볼 곳**

- 세 공개 진입점(`compressFile` · `compressFiles` · `uncompressFile`) 맨 앞에 관문이 있는가
- 해제는 `validateArchiveBeforeExtract` 를 지나는가 — 그리고 그 `.failure` 를 **삼키지 않는가**
- `Zip.unzipFile(overwrite: true)` 로 가므로, 검사가 통과한 뒤에는 덮어씁니다. 검사가 전부인 구조입니다

---

## 7. 배너 · 직접 저장 · 일괄 삭제

"""

P1 += block("Spartan/Safety/SafetyBannerAndPreview.swift")

P1 += block("Spartan/FileOpeners/TextView.swift", start=82, end=100,
            label="Spartan/FileOpeners/TextView.swift:82-100 (직접 저장 관문)")
P1 += block("Spartan/FileOpeners/HexView.swift", start=108, end=124,
            label="Spartan/FileOpeners/HexView.swift:108-124 (직접 저장 관문)")
P1 += block("Spartan/ContentView.swift", start=508, end=524,
            label="Spartan/ContentView.swift:508-524 (일괄 삭제 관문)")

P1 += """
**볼 곳**

- `HexView` 의 관문은 **제미나이가 지시한 위치보다 위로 옮겼습니다.** 원안은 `filePathIsNotMobileWritable` 의 `if` 안쪽이라 `else` 분기(직접 쓰기)가 안 막혔습니다
- `TextView` 는 `notifyOnBlock: false` 로 부르고 차단 사유를 화면에 직접 띄웁니다 — 알림과 중복되지 않게
- 일괄 삭제는 **미리보기를 거친 뒤** 관문을 지나고, 미리보기가 `.caution` 건수를 셉니다
- 배너가 1.2초 뒤 사라지는가 (요청하신 "약 1초")

---

## 8. 기본값

"""

P1 += block("Spartan/SpartanApp.swift", start=70, end=82,
            label="Spartan/SpartanApp.swift:70-82 (안전 모드 기본값)")

P1 += """
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
"""

write("REVIEW_CODE_P1.md", P1)


P2 = """# 코드 리뷰 — 2단계 두 칸 탐색기와 키보드

> 탐침·외관 코드는 제외했습니다. 테마(`Spartan/Theme/`)와 도움말 화면도 외관이라 뺐습니다.
> 커밋 `99f2434` (`main`) 기준. CI 통과.

제미나이 없이 작성했습니다.

---

## 리뷰할 때 봐주실 것

### 1. 이름을 유지한 계산 속성 — 이 묶음의 핵심 수법입니다

`ContentView` 는 1,880줄이고 `directory` 를 134곳, `masterFiles` 를 146곳에서 씁니다. 칸이 둘이 되려면 그 280곳이 **어느 칸** 이야기인지 알아야 합니다.

전부 고치는 대신 **이름을 그대로 두고 계산 속성으로 바꿨습니다.**

```swift
var directory: String {
    get { pane.directory }
    nonmutating set { pane.directory = newValue }
}
```

`pane` 이 클래스(`ObservableObject`) 참조라 구조체를 변형하지 않으므로 `nonmutating set` 이 성립합니다. **280곳을 하나도 건드리지 않았습니다.**

**이것이 리뷰의 제1 항목입니다.** 영리한 만큼 조용히 틀릴 수 있습니다. 봐야 할 것:

- `activeSide` 가 바뀌는 순간, 진행 중인 작업이 **엉뚱한 칸**을 보게 되지 않는가
- `openInPane` 이 `activeSide` 를 **먼저** 바꾸고 `defaultAction` 을 부르는가 (순서가 뒤바뀌면 왼쪽 칸을 눌렀는데 오른쪽 칸 파일이 열립니다)
- `copySelectionToOtherPane` 이 `pane` · `otherPane` 을 **지역 변수로 붙잡아** 비동기 완료 시점에 활성 칸이 바뀌어도 원래 칸을 쓰는가
- `$directory` 를 쓰던 10곳이 `directoryBinding` 으로 바뀌었는가 — 하나라도 남으면 컴파일이 안 되므로 확인됐습니다
- SwiftUI 가 두 `@StateObject` 의 변화를 제대로 받는가 (활성 칸만 다시 그려지고 반대 칸이 멈추는 일이 없는가)

### 2. 포커스

**칸 사이 좌우 이동은 손대지 않았습니다.** tvOS 포커스 엔진이 이미 합니다. 코드로 개입하면 싸웁니다.

**요청하신 5·6번(바깥 끝에서 툴바로)은 일부러 넣지 않았습니다.** `UIFocusGuide` 로 포커스가 **도착한 것은** 감지되지만 SwiftUI 에서 포커스를 **보낼** 수단이 없습니다 (`prefersDefaultFocus(_:in:)` 는 tvOS 14+, `focusSection()` 은 15+, 둘 다 "유도"지 "이동"이 아닙니다). 1픽셀 투명 뷰가 포커스를 잡고 못 빠져나오는 **함정**이 될 수 있고, 파일 탐색기에서 포커스가 갇히면 앱을 끄는 수밖에 없습니다.

**리뷰에서 봐주실 것: 포커스가 갇히는 곳이 정말 없는지.** 특히 두 칸에서 바깥 끝으로 갔을 때.

> 이전에 크게 당한 지점입니다. 문서 뷰어에서 뷰컨트롤러의 `pressesBegan` 으로 방향키를 받으려다 전부 포커스 이동에 먹혀 아무것도 안 됐습니다. **입력은 포커스를 가진 뷰에 먼저 갑니다.**

### 3. 쓰기가 관문을 지나는가

2단계가 새 쓰기 경로를 **둘** 만들었습니다.

| 새 경로 | 관문 |
| --- | --- |
| `copySelectionToOtherPane` → `PaneTransfer.run` | `FileGuard.authorize` (첫 바이트 전에) |
| `undoLastAction` → `PaneTransfer.undoLast` | `FileGuard.authorize` |

**둘 다 1단계 관문을 지납니다.** 우회가 없는지 확인해 주십시오.

### 4. 알려진 결정 — 확인이 필요합니다

- **`⌘Z` 를 되돌리기로 뒀습니다.** 압축은 `⌘⇧Z` 입니다. 압축으로 하자는 말씀이 있었지만 `⌘Z` 는 모든 프로그램에서 되돌리기이고, 이번에 되돌리기가 실제로 들어갔습니다. 바꾸라고 하시면 한 줄입니다
- **`Return` 은 비워 뒀습니다.** `⌘Return` 이 이름 바꾸기(파인더 방식)입니다. 맨 `Return` 을 열기로 할지는 리모컨 선택 버튼과 겹쳐 보여 정하지 않았습니다
- **복사는 되돌릴 수 없습니다.** 되돌리려면 지워야 하고, 앱이 알아서 지우는 것은 파일 탐색기가 할 일이 아닙니다

---

## 1. 칸 모델 — 칸 하나의 상태 전부

"""

P2 += block("Spartan/Panes/FilePaneModel.swift")

P2 += """
**볼 곳**

- **`navigate(to:)` 가 기록을 남기는 유일한 통로입니다.** `directory` 를 직접 쓰는 곳이 있으면 `⌘[` 기록이 어긋납니다
- **`applySortAndFilter` 가 디스크를 다시 읽지 않습니다.** `rawFiles` 를 들고 있어 정렬만 바꿀 때 빠릅니다 — 대신 정렬 후 **파일시스템 변화를 못 봅니다.** 새로 읽기(`⌘R`)가 필요한 이유입니다
- **`pruneCache`** — 원본은 순회 중에 `remove(at:)` 를 해서 인덱스가 밀렸습니다 (`decoded[i]` 를 지우면 뒤 항목이 앞으로 당겨짐). `filter` 로 바꿨습니다
- **`resetMultiSelectArrays` 가 `applySortAndFilter` 끝에서 불립니다.** 목록이 바뀌면 선택 배열의 길이가 안 맞기 때문입니다 — 원본 코드도 같은 이유로 비웠습니다. **정렬을 바꾸면 선택이 풀립니다.** 의도한 것이지만 쓰다 보면 거슬릴 수 있습니다
- `fileSize` · `modifiedDate` 가 정렬 비교마다 `attributesOfItem` 을 칩니다. **파일 300개면 O(n log n)번 디스크를 칩니다** — 느리면 여기입니다. 미리 한 번 읽어 캐시하는 것이 맞습니다
- 폴더가 언제나 먼저 옵니다 (`aDir != bDir` 이면 거기서 결정). 정렬 기준과 무관합니다

---

## 2. 칸 뷰 — 모델 하나만 받습니다

전역 상태를 참조하면 두 번 못 씁니다. 그게 두 칸이 가능한 이유입니다.

"""

P2 += block("Spartan/Panes/FilePaneView.swift")

P2 += """
**볼 곳**

- **행당 포커스 버튼은 하나입니다.** tvOS 규칙이고 깨뜨리면 목록을 못 씁니다
- `#available(tvOS 14.0, *)` 로 행이 두 벌입니다. `.if` 수정자로는 분기할 수 없습니다 — `#available` 은 컴파일 시점 검사입니다
- **`FileGlyph.symbol(for:name:)`** — 원래 `ContentView` 의 639줄 블록에 같은 `switch fileType` 이 tvOS 13/14 분기로 **두 벌** 들어 있던 것을 한 곳으로 모았습니다. 종류 번호가 빠진 것이 없는지 확인해 주십시오 (0·1·2·3·4·6·7·8·9·10·11·12·13·14·15·16 — **5가 없는 것이 원본과 같습니다**)
- `fileType` 이 `Double` 입니다 (`SpartanFile` 정의). `Int(fileType)` 로 변환해 `switch` 합니다
- **파일을 여는 일을 하지 않습니다.** `onOpen` 으로 `ContentView` 에 돌려주고 거기서 기존 `defaultAction` 이 39개 시트를 그대로 띄웁니다. **뷰어 쪽은 하나도 안 건드렸습니다**
- 위험도 배지가 `fastRisk(parentRisk:)` 로 부모 상속을 씁니다 — 안전한 폴더에서는 항목마다 디스크를 치지 않습니다

---

## 3. 두 칸 라우팅 — `ContentView`

"""

P2 += block("Spartan/ContentView.swift", start=23, end=79,
            label="Spartan/ContentView.swift:23-79 (칸 보유와 계산 속성)")

P2 += """
"""

P2 += block("Spartan/ContentView.swift", start=1493, end=1583,
            label="Spartan/ContentView.swift:1493-1583 (라우팅)")

P2 += """
**볼 곳**

- `openInPane` 이 **`activeSide` 를 먼저** 바꿉니다. `directory`·`masterFiles` 가 활성 칸을 가리키므로 순서가 전부입니다
- 여러 개 고르는 중이면 열지 않고 선택만 토글합니다
- `copySelectionToOtherPane` 이 선택이 없으면 `newViewFilePath`(마지막으로 만진 파일) 하나로 떨어집니다. 의도한 것이지만 **선택한 줄 알았는데 다른 파일이 복사되는** 경우가 없는지 봐주십시오
- `otherPane` · `pane` 을 지역 변수(`destinationPane` · `sourcePane`)로 붙잡습니다. 비동기 완료 시점에 활성 칸이 바뀌어도 원래 칸을 씁니다

---

## 4. 전송 — 뒷줄에서 돌고 진행률만 올립니다

"""

P2 += block("Spartan/Panes/PaneTransfer.swift")

P2 += """
**볼 곳**

- **관문이 첫 바이트 전에 전부 지납니다.** 한 건이라도 막히면 아무것도 시작하지 않습니다 — **절반만 옮겨진 상태가 가장 수습하기 어렵습니다**
- **덮어쓰지 않습니다.** 목적지에 같은 이름이 있으면 `name 2.txt` 로 비켜 씁니다. 조용한 덮어쓰기는 아무도 못 보는 자료 손실입니다
- 진행률이 **항목 수** 기준입니다. `FileManager.copyItem` 이 진행 중 아무것도 알려주지 않아 바이트 기준이 불가능합니다. **큰 파일 하나를 복사하면 0%에서 100%로 점프합니다** — 한계입니다
- `cancelRequested` 를 **항목 사이**에서만 봅니다. 한 항목 복사 중에는 취소가 안 듣습니다. 10GB 파일이면 취소가 사실상 안 됩니다
- `undoStack` 에 상한이 없습니다. 많이 옮기면 계속 쌓입니다
- 되돌리기도 관문을 지나고, 실패하면 기록을 되살립니다
- `perform` 이 실패하면 `RootHelperActs` 로 한 번 더 시도합니다 — **1단계 리뷰에 적은 대로 헬퍼는 실제로 돌지 않으므로** 이 경로는 사실상 죽은 코드입니다. 해롭지는 않습니다

---

## 5. 키보드

"""

P2 += block("Spartan/Panes/ArgoKeyboard.swift")

P2 += """
**볼 곳**

- **키 조합의 정의가 `ArgoCommand.binding(for:)` 한 곳입니다.** 도움말 화면이 같은 함수를 읽으므로 **표시와 실제가 어긋날 수 없습니다**
- `UIKeyCommand(input:modifierFlags:action:)` 를 씁니다. **iOS 13의 `(title:action:input:modifierFlags:)` 는 tvOS 에 없습니다** — tvOS 에서 그 자리는 `image:`·`propertyList:` 를 더 요구합니다 (CI 로 확인)
- 화살표 상수는 `UIKeyCommand.inputUpArrow` 입니다. `UIKeyInputUpArrow` 는 Swift 에서 이름이 바뀌었습니다
- **`textEntryActive` 가 `UserDefaults.standard` 에 들어 있습니다.** 이것은 깔끔하지 않습니다 — 상태를 저장소에 두는 것이고, 앱을 다시 켜도 남습니다. **봐주실 곳입니다.** 그리고 **지금 이 값을 켜 주는 코드가 없습니다** → 경로·찾기 칸에 글자를 넣을 때 단축키가 먹을 수 있습니다. 미완입니다
- `handleKeyCommand(_:)` 가 input·flags 로 어느 명령인지 되찾습니다. 같은 조합이 둘이면 앞의 것이 이깁니다 — 중복이 없는지 확인해 주십시오
- 글자 입력 점프가 `pressesBegan` 에서 **수정키가 없을 때만** 동작합니다. 수정키가 걸린 입력은 단축키 쪽이 처리합니다
- `⌘Space`·`⌘Tab` 은 시스템이 쓰므로 피했습니다. 다만 **`switchPane` 을 맨 `Tab`** 으로 뒀는데, tvOS 에서 `Tab` 이 포커스 이동에 쓰이는지 확인이 필요합니다

---

## 6. 명령 처리 — `ContentView`

"""

P2 += block("Spartan/ContentView.swift", start=1395, end=1490,
            label="Spartan/ContentView.swift:1395-1490 (단축키 처리)")

P2 += """
**볼 곳**

- 시트 인덱스가 맞는가. **한 번 틀렸습니다** — 처음에 삭제=17(즐겨찾기), 압축=10(오디오 재생), 해제=11(비디오 재생)로 짚었습니다. 지금은 압축=14(`compFileShow`), 해제=34(`uncompFileShow`), 이름=7, 정보=3, 새 폴더=6, 새 파일=5, 찾기=19, 도움말=39 입니다
- **`showSubView` 배열 크기를 39 → 40 으로 늘렸습니다.** 이것을 안 늘리면 **실행 즉시 죽습니다**
- `focusPathField`(`⌘L`)가 경로 칸이 아니라 **찾기 시트**를 엽니다. `TextField` 에 포커스를 코드로 줄 수 없기 때문입니다 — 이름과 동작이 어긋나므로 차라리 이름을 바꾸는 것이 맞습니다
- `deleteSelection`(`⌫`)이 일괄 삭제 미리보기를 거칩니다. 되돌릴 수 없는 일이라 확인을 뺄 수 없습니다

---

## 정리 — 리뷰 체크리스트

- [ ] `activeSide` 가 바뀌는 시점에 엉뚱한 칸을 보는 곳이 있는가 ★
- [ ] 포커스가 갇히는 곳이 정말 없는가 (두 칸 바깥 끝) ★
- [ ] `copySelectionToOtherPane` · `undoLast` 가 관문을 우회하지 않는가
- [ ] `FileGlyph` 에 빠진 파일 종류가 없는가
- [ ] 단축키 조합에 중복이 없는가
- [ ] `textEntryActive` 를 켜는 코드가 없는 문제 (미완)
- [ ] 정렬 비교마다 디스크를 치는 성능 문제
- [ ] 한 항목 복사 중 취소가 안 듣는 문제
- [ ] `⌘Z` 를 압축으로 바꿀지 결정
- [ ] `Return` 키를 열기로 할지 결정
"""

write("REVIEW_CODE_P2.md", P2)


P3 = """# 코드 리뷰 — 3단계 문서 뷰어

> 커밋 `99f2434` (`main`) 기준.

---

## 리뷰할 기능 코드가 없습니다

솔직하게 적겠습니다. **3단계는 기능 코드를 아직 한 줄도 쓰지 않았습니다.**

지금까지 한 것은 셋입니다.

1. **PDFEasy-iOS 검토** → `REVIEW_PDFEASY.md`
2. **SDK 조회** → 쓸 수 있는 것과 막힌 것을 가름
3. **탐침 작성** (`Lab ③`) → 기기에 물을 준비

탐침은 **기능이 아니라 질문**이므로 이 리뷰에서 제외했습니다. 외관(테마·뷰어 겉모습) 개편도 제외했습니다.

**왜 안 썼는지가 이 문서의 내용입니다.**

---

## 막힌 지점 — 쪽이 없습니다

지금 뷰어의 구조입니다.

```
문서 → JS 렌더러(docx-preview · SheetJS · pptx-preview · hwp.js · pdf.js)
     → HTML
     → WKWebView 가 길게 흘려보냄
```

**'쪽'이라는 개념이 없습니다.** 그래서 요청하신 13번이 **애초에 성립하지 않았습니다.**

| 요청 | 왜 안 되는가 |
| --- | --- |
| 가로 폭 맞춤 | 맞출 '쪽'이 없음 |
| 세로 폭 맞춤 | 같음 |
| 2장 같이 보기 | 같음 |
| 책장 넘기기 | 넘길 쪽이 없음 |

지금 `◀ 이전` · `다음 ▶` 은 쪽 이동이 아니라 **스크롤 420픽셀**입니다 (엑셀만 예외로 시트를 넘깁니다).

**쪽을 만들지 않고는 이 네 가지를 흉내만 낼 수 있습니다.** 그래서 쪽부터 풀어야 합니다.

---

## 확정된 사실

### 쓸 수 있는 것

| API | 용도 |
| --- | --- |
| `CGPDFDocument` · `CGPDFPage` | PDF 를 읽고 쪽 수를 셈 |
| `CGContextDrawPDFPage` | 쪽을 화면이나 비트맵에 그림 |
| `CGPDFContextCreate` | PDF 를 만듦 |
| `CoreText` | 글을 틀에 흘려 **직접 쪽을 나눔** |
| `UIGraphicsPDFRenderer` | 확인됨 (이전 탐침) |

**PDF 를 읽고 쪽 단위로 그리는 쪽은 이미 전부 됩니다.**

### 없는 것

`PDFKit` · `WebKit` · `QuickLook` — SDK 에 없습니다. (WebKit 은 **기기에는 있어** `dlopen` 으로 쓰고 있습니다.)

### 막힌 것 — 기기 확인 대기

`UIPrintPageRenderer` · `UIPrintFormatter` · `UISimpleTextPrintFormatter` · `UIMarkupTextPrintFormatter` · `viewPrintFormatter`

헤더에 **이름은 있지만 `API_UNAVAILABLE(tvos)`** 입니다. 컴파일러가 거부합니다.

> **제가 한 번 틀렸습니다.** SDK 조회로 "전부 PRESENT" 라고 드렸는데, 조회 도구가 이름만 grep 하고 같은 줄의 가용성 속성을 안 읽었습니다. 빌드가 즉시 거부했습니다. **도구를 고쳤습니다** — 이제 `USABLE` / `BLOCKED` / `ABSENT` 세 가지로 답하고, `BLOCKED` 는 판정이 아니라 기기에 물어야 할 질문이라고 명시합니다.

**막힌 것과 없는 것은 다릅니다.** WebKit 이 정확히 이랬습니다 — SDK 에 없는데 기기에는 있었습니다. `UIKit` 은 애플이 플랫폼마다 따로 빌드하지 않고 **하나의 바이너리를 공유**하므로, tvOS 에서 가려 놓은 클래스가 실제로 들어 있을 가능성이 실재합니다.

**이것은 기기만 답할 수 있습니다.** 그래서 코드를 쓰지 않고 탐침을 먼저 만들었습니다.

---

## 갈림길 — 탐침 결과가 결정합니다

### 길 A — 서식기가 기기에 있으면

```
문서 → JS 렌더러 → HTML → UIMarkupTextPrintFormatter
                         → UIPrintPageRenderer (A4 로 쪽 나누기)
                         → CGPDFContextCreate (PDF)
                         → CGPDFDocument 로 쪽 단위 그리기
                         → 잉크 비율로 빈 쪽 검사
```

**JS 렌더러는 그대로 둡니다.** 이미 잘 돌고 있습니다. 바뀌는 것은 **그 뒤**입니다.

얻는 것:

- **진짜 쪽** → 가로/세로 맞춤, 2장 보기, 책장 넘기기가 전부 성립
- **뷰어가 하나로 통일** → pdf·docx·xlsx·pptx·hwp 가 전부 "쪽이 있는 PDF" 가 됨 (요청하신 12번)
- **빈 출력 판정** → ppt 가 빈 화면으로 나오던 종류의 실패를 사용자에게 내보내지 않고 잡음
- **기존 pdf 경로와 합쳐짐** → pdf 는 변환 없이 같은 뷰어로

조심할 것:

- **쪽 나누기가 레이아웃을 다시 잡습니다.** 원본 생김새가 조금 달라집니다 — 특히 넓은 엑셀 표가 A4 폭에서 잘릴 수 있어 용지를 가로로 두거나 축소 배율이 필요합니다
- 한글 글꼴이 그려지는지

### 길 B — 없으면

`CoreText` 로 직접 쪽을 나눕니다. `CTFramesetterCreateWithAttributedString` 으로 글을 쪽 크기 틀에 차례로 흘려 넣습니다.

확실하지만 **손이 훨씬 많이 가고 HTML 의 표·이미지를 잃습니다.** 그래서 현실적인 모양은 혼합입니다.

| 형식 | 방식 |
| --- | --- |
| docx · hwp · txt · md (글 위주) | `CoreText` 로 쪽 나누기 |
| xlsx · pptx (배치가 중요) | 지금처럼 HTML 스크롤 |
| pdf | `CGPDFDocument` 로 쪽 단위 (변환 없음) |

이 경우 **뷰어 UI 통일은 겉모습만** 가능하고, 맞춤·2장 보기는 pdf 와 글 위주 문서에만 들어갑니다.

---

## PDFEasy 에서 취한 것

코드는 복사하지 않습니다 — 라이선스 파일이 없고 상용 앱 저장소입니다. **방법만** 가져옵니다.

1. **`createPDF` 대신 `viewPrintFormatter()`** — 전자는 내용 크기에 맞춰 거대한 한 장을 냅니다. 우리가 겪는 문제의 정체입니다
2. **실패가 오류로 오지 않는다** — 렌더링 못 한 문서는 오류가 아니라 **빈 쪽**을 냅니다. 우리가 이미 당했습니다 (ppt 흰 글자, hwp 무응답)
3. **잉크 비율로 빈 쪽 판정** — 쪽을 작게 흑백으로 그려 어두운 점 비율을 셉니다. `PDFKit` 이 없으니 `PDFPage.thumbnail` 대신 `CGContextDrawPDFPage` 로 같은 일을 합니다

가져오지 않는 것: 서버 변환(`StirlingApiManager`) — 서버가 없고, 파일을 밖으로 보내는 것은 파일 탐색기가 할 일이 아닙니다. `PdfKitView` 등 `PDFKit` 의존 코드도 못 씁니다.

---

## ChatGPT 명세에서 취하지 않은 것

**`DocumentViewerAdapter` 프로토콜** 을 만들지 않았습니다.

```
protocol DocumentViewerAdapter {
    var pageCount: Int { get }
    var currentPage: Int { get }
    func goToPage(_ page: Int)
    ...
}
```

`pageCount` 와 `goToPage` 를 전제하는데 **우리는 아직 쪽이 없습니다.** 탐침이 답할 질문 위에 인터페이스를 세우는 셈입니다.

길 A 로 가면 이 모양이 맞고, 길 B 로 가면 형식마다 다른 인터페이스가 됩니다. **둘 중 어느 쪽인지 모르는 상태에서 고르면 한쪽을 버려야 합니다.**

명세에서 취한 것(겉모습·로딩·오류·설정 통합·정보 패널)은 쪽 여부와 무관해 **이미 반영했습니다.**

---

## 다음 한 걸음

**설정 → `Lab ③ 인쇄 파이프라인 탐침`** 을 돌려 주십시오.

보는 것:

1. 런타임에 클래스가 실제로 있는가
2. **글자 300줄이 여러 쪽으로 나뉘는가** — 한 쪽이면 쪽 나누기가 안 되는 것입니다
3. **HTML 표 120행이 쪽으로 나뉘는가** ★핵심★
4. `CGPDFDocument` 로 되읽어 쪽마다 잉크 비율이 나오는가
5. 한글이 그려지는가

3번 결과를 `/var/mobile/Media/argo-print-probe.pdf` 로 저장하니 **탐색기에서 열어 눈으로 확인**하실 수 있습니다.

**결과를 보고 3단계 지시서를 쓰겠습니다.** 지금 쓰면 가정 위에 쓰는 것입니다.
"""

write("REVIEW_CODE_P3.md", P3)


INDEX = """# 코드 리뷰 묶음 — 1~3단계

커밋 `99f2434` (`main`). CI 통과. 설치용 IPA 는 릴리즈 `build-20261022112853`.

**탐침·외관 코드는 제외했습니다.** `Spartan/Diagnostics/` 전부, `Spartan/Theme/` 전부, 뷰어 겉모습, 도움말 화면이 빠졌습니다. 기능 코드만 있습니다.

| 문서 | 내용 | 분량 |
| --- | --- | --- |
| `REVIEW_CODE_P1.md` | 안전장치 — 경로 판정, 관문, 휴지통, 압축 검사, 작업 기록 | 약 1,500줄 |
| `REVIEW_CODE_P2.md` | 두 칸 탐색기, 키보드, 전송·되돌리기, 정렬 | 약 1,450줄 |
| `REVIEW_CODE_P3.md` | 문서 뷰어 — **기능 코드 없음**. 왜 없는지와 갈림길 | 짧음 |

각 문서는 **맨 앞에 "리뷰할 때 봐주실 것"** 이 있고, 코드 뒤마다 **"볼 곳"** 이 붙어 있습니다. 코드는 실제 파일에서 그대로 떴습니다 — 옮겨 적으면 어긋납니다.

---

## 먼저 봐주실 세 가지

### 1. R2 — 유일한 자료 손실 위험 (P1)

`TrashManager.unlinkNestedSymlinksIfDirectory` 가 **삭제가 실패할 수 있는데 링크를 먼저 지웁니다.** 실패하면 폴더는 남고 링크만 사라집니다.

**실기기 30초 시험이 결론을 냅니다.**

```
1. /var/mobile/Media/target.txt 를 만든다
2. /var/mobile/Media/box/ 안에 target.txt 를 가리키는 링크를 만든다
3. box 를 휴지통으로 보낸다
4. target.txt 가 살아 있는가?
```

살아 있으면 함수를 지우고, 사라졌으면 순서를 바꿉니다.

**그 전까지 심볼릭 링크가 든 폴더를 옮기거나 지우지 마십시오.**

### 2. 계산 속성 수법 (P2)

`ContentView` 의 `directory`·`masterFiles` 를 **이름을 그대로 두고** 활성 칸을 가리키는 계산 속성으로 바꿨습니다. 280곳을 건드리지 않은 대가로, **`activeSide` 가 바뀌는 시점이 전부**입니다. 조용히 틀릴 수 있는 종류의 코드입니다.

### 3. 포커스 함정 (P2)

요청하신 "바깥 끝에서 툴바로"(5·6번)를 **일부러 넣지 않았습니다.** SwiftUI 에서 포커스를 보낼 수단이 없어 1픽셀 뷰가 포커스를 잡고 못 빠져나올 위험이 실재합니다. **파일 탐색기에서 포커스가 갇히면 앱을 끄는 수밖에 없습니다.**

실기기에서 **포커스가 갇히는 곳이 정말 없는지** 확인해 주십시오.

---

## 알려진 미완·결정 대기

| 항목 | 상태 |
| --- | --- |
| R2 심볼릭 링크 선삭제 | 실기기 시험 대기 |
| `textEntryActive` 를 켜는 코드 | **없습니다.** 경로·찾기 칸에서 단축키가 먹을 수 있습니다 |
| `⌘Z` = 되돌리기 / 압축 = `⌘⇧Z` | 결정 대기 |
| `Return` 키 | 비워 둠. 열기로 할지 결정 대기 |
| `focusPathField`(`⌘L`) | 경로 칸이 아니라 찾기 시트를 엽니다. 이름이 어긋남 |
| 정렬 비교마다 `attributesOfItem` | 파일 300개면 느릴 수 있음 |
| 한 항목 복사 중 취소 | 안 듣습니다. 항목 사이에서만 확인 |
| 3단계 인쇄 서식기 | `Lab ③` 탐침 결과 대기 |

---

## 제가 틀렸던 것

리뷰하실 때 제 판단을 무조건 믿지 마시라는 뜻에서 적습니다.

- **SDK 조회 "인쇄 API 전부 PRESENT"** — 틀렸습니다. 헤더에 이름만 있고 `API_UNAVAILABLE(tvos)` 입니다. 조회 도구가 가용성 속성을 안 읽었습니다. 도구를 고쳤습니다
- **`m3` "안전 모드 검사가 영어 문자열을 비교한다"** — 틀렸습니다. 그 코드는 없습니다
- **단축키 시트 인덱스** — 삭제=17(즐겨찾기), 압축=10(오디오), 해제=11(비디오)로 짚었습니다. 전부 틀렸습니다
- **`.m` 파일을 Swift 로 등록** — 빌드가 거부했습니다. 등록 도구를 고쳤습니다
- **`Menu` 를 tvOS 14 로 가정** — tvOS 17 부터입니다
"""

write("REVIEW_CODE_INDEX.md", INDEX)
