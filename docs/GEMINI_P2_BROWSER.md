# 2단계 지시서 — 두 칸 탐색기와 키보드

`HANDOVER.md` 와 `GEMINI_P1_SAFETY.md` 를 먼저. 1단계가 끝난 뒤에 시작합니다.

상세 명세가 따로 있습니다. **그대로 따르십시오.**
- `DUAL_PANE_SPEC.md` — 두 칸 탐색기
- `KEYBOARD_SHORTCUTS.md` — 단축키 60여 개

---

## 선행 수술 — 이것부터입니다

`ContentView.swift` 1,880줄 안에 파일 목록이 통째로 박혀 있고, `directory`·`masterFiles`·`multiSelectFiles` 를 **하나씩만** 들고 있습니다. 칸이 둘이 되려면 떼어내야 합니다.

### 2-1. `FilePaneModel.swift` (새 파일)

칸 하나의 상태 전부를 담습니다.

```swift
@MainActor
final class FilePaneModel: ObservableObject {
    enum Source { case local, webdav(connectionID: String) }

    @Published var source: Source = .local
    @Published var directory: String = "/var/mobile/Media"
    @Published var files: [SpartanFile] = []
    @Published var selection: Set<String> = []
    @Published var isLoading = false
    @Published var sort: SortKey = .name
    @Published var ascending = true
    @Published var showHidden = false
    @Published var filter: String = ""
    @Published private(set) var history: [String] = []   // ⌘[ ⌘]

    func reload()
    func enter(_ name: String)
    func goUp()
    func goBack() / goForward()
    func jumpToPrefix(_ s: String)   // 글자 입력 점프
}
```

### 2-2. `FilePaneView.swift` (새 파일)

목록을 그리는 뷰. **`FilePaneModel` 하나만 받습니다.** 전역 상태를 참조하면 두 번 못 씁니다.

```swift
struct FilePaneView: View {
    @ObservedObject var model: FilePaneModel
    let isActive: Bool
    let onOpen: (SpartanFile) -> Void
}
```

**행당 포커스 버튼은 하나입니다.** 지금 코드에도 그 규칙이 지켜져 있으니 옮길 때 깨뜨리지 마십시오.

### 2-3. `ContentView` 를 두 칸으로

```swift
@StateObject private var leftPane  = FilePaneModel()
@StateObject private var rightPane = FilePaneModel()
@State private var activePane: Pane = .left
```

툴바의 복사·이동·압축은 **활성 칸의 선택**에 대해 동작하고, 붙여넣기는 **반대 칸**으로 갑니다.

---

## 포커스 — 조심할 지점

**칸 사이 이동은 공짜입니다.** tvOS 포커스 엔진이 좌우로 알아서 건너갑니다. 손대지 마십시오.

손댈 곳은 **바깥 끝에서 툴바로 빠지는 것** 하나입니다.

```
툴바  ←  왼쪽 칸  ⇄  오른쪽 칸  →  툴바
```

`UIFocusGuide` 를 양쪽 바깥에 두고 `preferredFocusEnvironments` 를 툴바로 지정하십시오. **포커스 가이드가 정확히 이 일을 하라고 있는 물건이라** 기본 동작과 싸우지 않습니다.

> **이전에 크게 당한 지점입니다.** 문서 뷰어에서 뷰컨트롤러의 `pressesBegan` 으로 방향키를 받으려다 전부 포커스 이동에 먹혀 아무것도 안 됐습니다. **입력은 포커스를 가진 뷰에 먼저 갑니다.**

---

## 키보드

`KEYBOARD_SHORTCUTS.md` 의 표가 명세입니다. 구현 쪽 사실만 적습니다.

- **`UIKeyCommand` 는 tvOS에 있습니다.** 리스폰더 체인으로 갑니다
- **iOS 13의 `UIKeyCommand(title:action:input:modifierFlags:)` 는 없습니다.** tvOS에서 그 자리는 `image:`·`propertyList:` 를 더 요구합니다. **`UIKeyCommand(input:modifierFlags:action:)` 를 쓰십시오** (CI로 확인함)
- 화살표 상수는 `UIKeyCommand.inputUpArrow` 입니다. `UIKeyInputUpArrow` 는 Swift에서 이름이 바뀌었습니다
- `pressesBegan` + `UIKey.charactersIgnoringModifiers` 로 **글자 입력 점프**를 만드십시오. 파일 300개 폴더에서 이게 가장 큰 이득입니다
- `discoverabilityTitle` 이 있습니다. **`⌘/` 도움말 화면이 이것을 읽게 하십시오**

SwiftUI에서 리스폰더 체인에 끼려면 `UIHostingController` 를 상속해 `keyCommands` 를 덮어쓰는 방법이 보통입니다.

### 반드시 지킬 것

- **경로·검색 칸에 포커스가 있을 때는 단축키가 먹으면 안 됩니다.** 글자가 그대로 들어가야 합니다
- **키보드를 빼도 리모컨만으로 모든 기능이 되어야 합니다.** 단축키는 지름길이지 유일한 길이 아닙니다
- `⌘ Space`, `⌘ Tab` 은 시스템이 씁니다. 피하십시오
- tvOS에는 메뉴 막대가 없어 **단축키를 알 길이 없습니다.** `⌘/` 도움말이 반드시 있어야 합니다

### `⌘Z` 는 아직 정해지지 않았습니다

압축으로 하자는 요청이 있었지만 `⌘Z` 는 모든 프로그램에서 되돌리기입니다. 문서에는 **압축 `⌘⇧Z` / 되돌리기 `⌘Z`** 로 적어 두었습니다. **바꾸라는 지시가 따로 오면 그때 바꾸십시오.**

---

## 탐색기 본연의 기능

같이 넣습니다. `ROADMAP.md` 4장의 표가 목록입니다.

우선순위:
1. **반대 칸으로 복사·이동** (두 칸의 존재 이유)
2. **정렬** (이름·크기·날짜·종류)
3. **진행률 + 취소** — 큰 파일 복사 중 화면이 멈추면 안 됩니다
4. 걸러보기, 숨김 토글, 폴더 용량
5. 되돌리기 (이동·이름변경)

---

## 분량이 많습니다 — 나눠서 주십시오

한 번에 다 주지 마십시오. 검수가 밀립니다.

```
2-A  FilePaneModel + FilePaneView 로 떼어내기 (기능 변화 없음, 한 칸 그대로)
2-B  두 칸 배치 + 포커스 가이드 + 반대 칸 복사·붙여넣기
2-C  키보드 단축키 + ⌘/ 도움말
2-D  정렬·걸러보기·진행률·되돌리기
```

**2-A 가 통과한 뒤 2-B 를 주십시오.** 2-A 는 겉보기 동작이 하나도 안 바뀌어야 합니다 — 그게 제대로 떼어냈다는 증거입니다.
