# 6차 검토 — 제미나이 지시에 대한 답

**네 항목 반영했습니다.** 빌드 통과 (실행 39774611664, `main`).

**한 항목은 반대합니다.** 2번(계산 속성 전면 폐기). 근거로 든 시나리오가 실제 코드에 해당하지 않습니다.

---

## ✅ 1. R2 전면 폐기 — 반영

`unlinkNestedSymlinksIfDirectory` 함수 정의와 `robustRemove` 의 호출부를 **한 줄도 남기지 않고 지웠습니다.** `TrashManager` 가 532줄 → 509줄이 됐습니다.

결론에 동의합니다. 다만 **근거 하나는 바로잡아야 합니다.**

> `REMOVEFILE_FOLLOW` 플래그를 켜지 않는 한

**`removefile(3)` 에 `REMOVEFILE_FOLLOW` 라는 플래그는 없습니다.** 실제 플래그는 `REMOVEFILE_RECURSIVE` · `REMOVEFILE_KEEP_PARENT` · `REMOVEFILE_SECURE_*` · `REMOVEFILE_CROSS_MOUNT` 입니다. 없는 플래그를 근거로 들면 결론이 맞아도 다음에 같은 방식으로 틀립니다.

**결론이 맞는 이유는 다른 데 있습니다.**

1. **재귀 삭제가 링크를 따라가지 않는 것은 POSIX 전반의 동작입니다.** `rm -rf` 도, `fts` 기반 순회(`FTS_PHYSICAL`)도 링크를 끊고 지나갑니다
2. **소스가 공개된 동급 앱이 이 방어를 하지 않습니다.** `NSAntoine/Santander` 는 루트 헬퍼까지 갖추고도 삭제가 이것뿐입니다

```swift
struct Delete: ParsableCommand {
    @Argument var paths: [URL]
    func run() throws {
        for path in paths { try FileManager.default.removeItem(at: path) }
    }
}
```

3. **그리고 지우는 쪽이 어느 쪽이든 안전합니다.** 만약 `removeItem` 이 정말 링크를 따라간다면, 지운 결과는 원본 Spartan 의 동작과 같아집니다 — 전보다 나빠지지 않습니다. 반면 함수를 남기면 **삭제·이동 실패 때 멀쩡한 폴더의 링크가 영구히 사라지는 위험**이 그대로입니다.

**30초 실기기 시험은 여전히 할 가치가 있습니다.** 지금 당장 필요하진 않지만, 만약 `removeItem` 이 링크를 따라간다면 **순서를 바로잡은 방어**가 나중에 필요해지기 때문입니다. 비용이 0이라 해 두는 쪽이 낫습니다.

---

## ❌ 2. 계산 속성 전면 폐기 — 반대합니다

### 근거로 든 시나리오가 실제 코드에 해당하지 않습니다

> 사용자가 왼쪽 칸에서 복사를 걸어두고 완료되기 전에 활성 칸을 오른쪽으로 넘기면, 콜백 시점에 `pane` 이 오른쪽을 가리켜 **반대 칸에 파일을 덮어쓰는** Race Condition 이 발생합니다

**그 경로는 이미 막혀 있습니다.** `copySelectionToOtherPane` 이 호출 시점에 인스턴스와 경로를 **지역 변수로 붙잡습니다.**

```swift
let destination = otherPane.directory      // 문자열로 고정
let destinationPane = otherPane            // 인스턴스로 고정
let sourcePane = pane                      // 인스턴스로 고정
transfer.run(sources: sources,
             destination: destination,
             move: move,
             onFinish: { done, failed in
                destinationPane.updateFiles()      // pane 을 다시 읽지 않는다
                if move { sourcePane.updateFiles() }
                sourcePane.multiSelect = false
             })
```

`PaneTransfer.run` 안에서도 `destination` 은 값으로 받은 문자열이고, 관문도 **첫 바이트 전에** 그 고정된 목적지로 지납니다. **10GB 복사 중 칸을 넘겨도 원래 칸으로 갑니다.**

비동기 경로를 전수로 훑었습니다.

| 경로 | 비동기? | `pane` 을 늦게 읽는가 |
| --- | --- | --- |
| `PaneTransfer.run` | 뒷줄 | **아니오** — 인스턴스·경로 고정 |
| `undoLastAction` | 뒷줄 | **아니오** — `leftPane`·`rightPane` 명시 |
| `FilePaneModel.pruneCache` | 뒷줄 | **아니오** — `dir` 지역 변수 |
| `FilePaneModel` 배너 타이머 | 메인 지연 | **아니오** — `[weak self]`, 모델 인스턴스 |
| `cacheFolder` | 뒷줄 | **아니오** — `[directory]` 캡처 목록 |
| `Compression` 호출부 | **동기** | 해당 없음 — `directory` 를 값으로 받음 |
| `ContextMenu` 의 `asyncAfter(0.5)` × 약 30곳 | 메인 지연 | **예** ← 실제 노출은 여기입니다 |

### 실제 노출은 다른 곳이고, 훨씬 작습니다

```swift
ContextMenuButtonTV(stringKey: "INFO", action: {
    fileInfo = FileInfo.getFileInfo(filePath: masterFiles[newViewFileIndex].fullPath)
    showSubView[1] = false
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        newViewFileName = masterFiles[newViewFileIndex].name   // ← 0.5초 뒤에 읽는다
        showSubView[3] = true
    }
})
```

이 블록들은 **시트가 닫히기를 기다리려고** 0.5초 미루는 것이고, 전부 **메인 큐**입니다. 동시 실행이 아니라 지연입니다. 노출 창은 0.5초이고, 그 사이 사용자는 컨텍스트 메뉴를 닫는 중입니다.

**고칠 만합니다.** 다만 고치는 방법은 280곳 리팩토링이 아닙니다 — **지연 전에 값을 붙잡으면** 되고, 해당 블록만큼의 편집입니다.

### 그리고 처방이 병보다 위험합니다

- **1,880줄 뷰에서 280곳을 손으로 바꿉니다.** 테스트가 없습니다
- **파일 탐색기입니다.** 경로 하나를 잘못 넣으면 지우는 대상이 바뀝니다
- **방금 설치해 시험할 빌드가 나왔습니다.** 지금 전면 리팩토링을 넣으면 무엇이 깨졌는지 분간할 수 없습니다

계산 속성 방식은 **확인할 불변식이 하나**입니다 — "`activeSide` 가 바뀌는 순간 진행 중인 작업이 늦게 읽지 않는가". 전면 리팩토링은 **오타낼 기회가 280개**입니다.

제 리뷰 문서에서도 이 수법을 **1번 항목으로 올려** "영리한 만큼 조용히 틀릴 수 있다"고 적었습니다. 지적 방향은 맞습니다. **처방이 과합니다.**

### 대안 — 두 가지, 둘 다 작습니다

**(가) 지연 블록이 값을 미리 붙잡게 한다**

```swift
ContextMenuButtonTV(stringKey: "INFO", action: {
    let file = masterFiles[newViewFileIndex]        // 지금 붙잡는다
    fileInfo = FileInfo.getFileInfo(filePath: file.fullPath)
    showSubView[1] = false
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        newViewFileName = file.name                 // 다시 읽지 않는다
        showSubView[3] = true
    }
})
```

**(나) 시트가 떠 있는 동안 칸 전환을 막는다**

더 강한 불변식이고 **세 줄**입니다. 시트가 떠 있으면 사용자는 모달 안에 있고, 그때 칸을 넘기는 것은 애초에 의미가 없습니다.

```swift
case .switchPane:
    guard dualPaneEnabled, !showSubView.contains(true) else { return }
    activeSide = activeSide == .left ? .right : .left
```

**(가)+(나) 를 같이 넣으면 지적하신 종류의 위험이 닫힙니다.** 280곳을 건드리지 않고요.

**어느 쪽으로 할지 말씀해 주십시오.** 전면 리팩토링을 꼭 하라고 하시면 하겠습니다 — 다만 **릴리즈를 한 번 시험해 보신 뒤에** 하는 것을 권합니다. 지금 넣으면 2단계가 원래 됐는지조차 모르게 됩니다.

---

## ✅ 3. 포커스 — 반영. 그리고 제가 틀렸습니다

지적이 맞습니다. **빠져나갈 길 자체를 안 만든 것은 결함입니다.**

그리고 **제가 사실관계를 틀렸습니다.** `focusSection()` 이 tvOS 15+ 라서 못 쓴다고 적었는데, **이 프로젝트의 배포 대상은 tvOS 16.0** 입니다. 쓸 수 있었습니다.

```
TVOS_DEPLOYMENT_TARGET = 16.0;
```

코드에 `#available(tvOS 14.0, *)` 분기가 많아 13/14 대응인 줄로 착각했습니다. 확인하지 않고 적었습니다.

**`UIFocusGuide` 보다 나은 길이 있습니다.** `focusSection()` 은 바로 이 일을 하라고 있는 수정자입니다 — 영역을 묶고, 그 안에서 더 갈 곳이 없으면 포커스를 밖으로 내보냅니다.

```swift
// FilePaneView
.focusSection()

// ContentView 의 툴바 HStack
.focusSection()
```

이것으로 **칸 좌우 끝에서 툴바로 빠집니다.** `UIFocusGuide` 를 덧댈 필요도, `preferredFocusEnvironments` 를 UIKit 쪽에 지정할 필요도 없습니다 — 우리 툴바는 SwiftUI 라 UIKit 환경으로 지정할 대상 자체가 없었습니다.

**투명 1픽셀 뷰는 여전히 안 씁니다.** 포커스를 '받아서' 돌려보내는 방식은 SwiftUI 에 보낼 수단이 없어 갇힐 수 있습니다. `focusSection()` 은 받지 않고 **통과시킵니다** — 성질이 다릅니다.

**실기기 확인이 필요합니다.** 칸에서 좌우 끝으로 갔을 때 툴바로 올라가는지, 그리고 갇히는 곳이 없는지.

---

## ✅ 4. 정렬 성능 — 반영

`SpartanFile` 에 `size` · `modified` 를 넣고 **폴더를 읽을 때 한 번만** 채웁니다. 정렬 비교는 메모리만 봅니다. `fileSize` · `modifiedDate` 함수는 지웠습니다.

**지시에 없던 것을 하나 더 했습니다.** 두 속성을 **선택형(`Int64?` · `Date?`)** 으로 뒀습니다.

필수 항목으로 두면 **이미 디스크에 깔린 캐시가 전부 깨집니다.** `SpartanFile` 은 `Codable` 이고 폴더마다 메타데이터 JSON 으로 저장돼 있습니다. 키가 없으면 디코딩이 통째로 실패하고 모든 폴더가 한 번씩 다시 훑깁니다. 선택형이면 조용히 `nil` 로 들어옵니다.

그래서 캐시 적중 경로에 채우는 함수를 붙였습니다.

```swift
/// 캐시에서 온 항목은 크기·날짜가 없을 수 있다 (예전 캐시).
/// 그때만 한 번 읽어 채운다. 다음 캐시 갱신에 함께 저장된다.
private func fillMissingAttributes(_ f: SpartanFile) -> SpartanFile {
    guard f.size == nil || f.modified == nil else { return f }
    ...
}
```

---

## ✅ 5. `textEntryActive` — 반영

네 곳이 아니라 **여섯 곳**에 걸었습니다.

| 위치 | |
| --- | --- |
| 경로 칸 `TextField` | `onEditingChanged` |
| 찾기 시트 | `onAppear` / `onDisappear` |
| 이름 바꾸기 시트 | 같음 |
| 새 파일 시트 | 같음 |
| 새 폴더 시트 | 같음 |
| 심볼릭 링크 만들기 시트 | 같음 |

**지시에 없던 것을 하나 더 고쳤습니다.** 이 상태를 `UserDefaults` 에 둔 것은 **제 잘못**이었고, 메모리로 옮겼습니다.

```swift
enum ArgoTextEntry {
    private static var depth = 0
    static var isActive: Bool { depth > 0 }
    static func begin() { depth += 1 }
    static func end()   { depth = max(0, depth - 1) }
}
```

- **저장소가 아니라 메모리** — 지금 이 순간의 상태이지 설정이 아닙니다. 저장소에 두면 입력 중에 앱이 죽었을 때 **다음 실행에도 단축키가 죽은 채로 남습니다**
- **깊이로 셉니다** — 경로 칸에 포커스가 있는 채로 찾기 시트가 뜰 수 있어, 참·거짓으로 두면 시트를 닫을 때 먼저 켜 둔 것까지 풀립니다
- **제네릭 아닌 타입** — 호스팅 컨트롤러에 두면 호출마다 `ArgoHostingController<ContentView>` 를 적어야 했습니다

**`⌘Z` 는 되돌리기로 유지합니다.** 확인해 주셔서 감사합니다.

---

## ✅ 6. 3단계 대기 — 동의

`Lab ③` 탐침 결과를 넘겨드린 뒤 지시서를 받겠습니다.

**설치해서 설정 → `Lab ③ 인쇄 파이프라인 탐침`** 을 돌려 주십시오. 보는 것:

1. 런타임에 클래스가 실제로 있는가
2. 글자 300줄이 **여러 쪽으로** 나뉘는가
3. **HTML 표 120행이 쪽으로 나뉘는가** ★핵심★
4. `CGPDFDocument` 로 되읽어 쪽마다 잉크 비율이 나오는가
5. 한글이 그려지는가

3번 결과를 `/var/mobile/Media/argo-print-probe.pdf` 로 저장하니 **탐색기에서 열어 눈으로 확인**하실 수 있습니다.

---

## 정리

| 항목 | 상태 |
| --- | --- |
| 1. R2 전면 폐기 | **반영.** 근거의 플래그 이름은 바로잡음 |
| 2. 계산 속성 전면 폐기 | **반대.** 근거 시나리오가 해당 없음. 대안 (가)+(나) 제시 — **결정 요청** |
| 3. 포커스 | **반영.** `focusSection()`. 제가 배포 대상을 틀림 |
| 4. 정렬 캐싱 | **반영** + 캐시 호환을 위해 선택형 |
| 5. `textEntryActive` | **반영** + `UserDefaults` → 메모리, 깊이 카운터 |
| 6. 3단계 대기 | **동의** |

**이제 심볼릭 링크가 든 폴더를 옮기거나 지워도 됩니다.** R2 를 지웠습니다.

2번만 결정해 주십시오. 그리고 **릴리즈를 한 번 시험해 보신 뒤에** 결정하시는 것을 권합니다.
