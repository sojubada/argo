# 제미나이 P6 지시문 — 엑셀 뷰어 사용성 (이 파일의 코드 블록 두 개를 차례로 그대로 붙여 넣는다)

- 붙여 넣을 때 **첨부 1개**: 저장소 main 의 `Spartan/FileOpeners/Readers/ArgoSheetViewer.swift` (1,405줄, 커밋 bedaa75).
- 블록 ①(공통 규칙)과 블록 ②(이번 일)를 한 메시지에 붙인다. 길어서 오류가 나면 ①을 먼저 보내고 "다음 메시지에 일이 온다"고 한 뒤 ②를 보낸다.
- 받은 것은 Claude 가 첨부본을 바탕으로 3-way 병합·검토·빌드한다.

## ① 공통 규칙

```
너는 Argo(애플TV용 파일 탐색기·문서 뷰어, tvOS 16, TrollStore 설치)에 붙은 엑셀 셀 뷰어를 고친다.
저장소는 sojubada/argo. 너는 저장소를 직접 읽을 수 없으니, 이 메시지에 붙은 첨부와 아래 설명만 바탕으로 한다.
첨부에 없는 코드를 지어내지 않는다. 없는 함수가 필요하면 "필요한 것" 에 이름과 시그니처를 적는다.

[환경]
- Swift 는 tvOS 16.0 최소, UIKit. SwiftUI 는 쓰지 않는다(포커스가 안 잡힌다).
- tvOS 에 없는 iOS API 금지: UIContextMenuInteraction, UIEditMenuInteraction, UISheetPresentationController,
  UIPointerInteraction, UIHoverGestureRecognizer, UIScrollView 확대(zoomScale), SwiftUI .keyboardShortcut/onKeyPress,
  iOS 13 의 UIKeyCommand(title:action:input:...) 생성자. 확실하지 않은 API 는 쓰지 말고 "의심" 에 적는다.
- 키보드는 pressesBegan 의 UIPress.key(UIKey: keyCode, modifierFlags, charactersIgnoringModifiers)로 받는다.
  이 앱에서 이미 검증된 방식이다(코드 편집기). tvOS 는 키를 누르고 있어도 pressesBegan 을 **한 번만** 보낸다
  → 되풀이는 직접 타이머로 하고, 같은 keyCode 의 pressesEnded/pressesCancelled 에서 멈춘다.
- 화면 글자는 L("한국어") 로 감싼다(앱의 번역 함수).
- 설정 저장은 UserDefaults.settings (앱이 쓰는 UserDefaults 확장, 이미 있음).

[코드 규칙]
- 첨부 파일의 기존 함수·속성·기능을 지우거나 이름을 바꾸지 않는다(차트 오버레이, 링크 QR, 링크 이동, 타일 세대 번호 등).
- 커서를 움직일 때 타일을 다시 그리지 않는다. 십자·머리글·커서는 따로 된 가벼운 층으로만 갱신한다.
- 사용자 파일 값으로 반복 횟수가 정해지는 곳에는 상한을 둔다(행 1,048,576 · 열 16,384).
- 주석은 한국어로 "왜" 를 적는다.

[제출물]
1. 맨 위에 첨부 ArgoSheetViewer.swift 의 줄 수를 적어 받은 파일을 바탕으로 했음을 증명한다.
2. Spartan/FileOpeners/Readers/ArgoSheetViewer.swift — 고친 파일 **전체**.
3. Spartan/FileOpeners/Readers/ArgoSheetChrome.swift — 새 파일 **전체**(아래 [나눌 곳] 참고).
4. ArgoFocusCanvas 클래스 — 고친 클래스 **하나만 통째로**(ArgoPDFReader.swift 안에 있다).
5. Rust: 새로 만들거나 바꾼 함수만 함수 하나씩 통째로, 각 위에 "파일 / 들어갈 자리 / 새것·바꾼 것" 표시.
   argosheet.h 에 더할 줄. 새 시험(#[test]).
6. 끝에 "못 한 것", "의심(tvOS 에 있는지 확실하지 않은 API)", "Claude 가 기기에서 볼 것" 을 솔직히 적는다.
   돌리지 않은 시험을 돌렸다고 쓰지 않는다. Swift 는 네가 컴파일할 수 없으니 "컴파일 안 함" 이라고 적는다.
```

## ② 이번 일 (P6)

```
[지금 구조 — 첨부 파일 요약]
- ArgoSheetBook: as_open 결과를 감싸 백그라운드 작업 중에도 살아 있게 한다.
- ArgoSheetTileView: 256pt 타일을 백그라운드에서 그린다. 틀 고정 때문에 4개(corner/top/left/body)가 있고
  각자 sheetOriginPt·zoom 을 가진다. 시트 pt → 뷰 좌표 = (p - sheetOriginPt) * zoom.
  타일 그리기는 static drawTile → static drawCommands(ctx, drawlist) (타일·차트 공용).
  drawTile 은 맨 먼저 흰색으로 칠한다(404줄). 차트·그림 오버레이는 타일 뷰 안 zPosition 400, 커서는 500.
- ArgoSheetViewerController: topBar(주소·값·수식), grid(4개 host + 구분선), bottomBar(sheetLabel·zoomLabel).
  colOffsets/rowOffsets(시트 pt 누적), freezeRows/freezeCols, bodyOffset(본문 스크롤, 화면 pt), zoom,
  cursorRow/cursorCol/cursorInfo, cursorSheetRect(), placeCursor(), scrollToCursor(), layoutQuadrants(),
  loadSheet(_:), setZoom(_:), nextSheet(), jump(to:), showCellSheet()(선택 버튼 → 셀 정보 알림창), showQR(_:).
  입력: canvas(ArgoFocusCanvas).onPress(방향키·재생/일시정지=다음 시트·메뉴=닫기), onSelect, onRelease.
- 이 화면이 떠 있는 동안 앱 전체 단축키(UIKeyCommand)는 꺼진다(presentedViewController 가 있으면 무시하도록 되어 있음).

[ArgoFocusCanvas 지금 코드 — ArgoPDFReader.swift 안, 다른 뷰어도 쓴다. 기존 동작은 그대로 두고 더하기만]
final class ArgoFocusCanvas: UIControl {
    var onPress: ((UIPress.PressType) -> Bool)?
    var onSelect: (() -> Void)?
    var onRelease: ((UIPress.PressType) -> Void)?
    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if let onRelease = onRelease { for press in presses { onRelease(press.type) } }
        super.pressesEnded(presses, with: event)
    }
    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        if let onRelease = onRelease { for press in presses { onRelease(press.type) } }
        super.pressesCancelled(presses, with: event)
    }
    override var canBecomeFocused: Bool { true }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        for press in presses {
            ArgoDiag.log("canvas", "pressesBegan type=\(press.type.rawValue) focused=\(isFocused)")
            if press.type == .select { onSelect?(); return }
            if let handled = onPress?(press.type), handled { return }
        }
        super.pressesBegan(presses, with: event)
    }
}
→ 더할 것: var onKey: ((UIKey) -> Bool)? 와 var onKeyUp: ((UIKey) -> Void)?.
  pressesBegan 에서 press.key 가 있고 onKey 가 true 를 돌려주면 거기서 끝(select·onPress 보다 먼저).
  false 면 지금처럼 흘려보낸다. pressesEnded/Cancelled 에서 press.key 가 있으면 onKeyUp 도 부른다.
  onKey 를 안 넣은 다른 뷰어는 동작이 전과 같아야 한다.

[나눌 곳]
ArgoSheetChrome.swift(새 파일)에는 컨트롤러의 private 속성에 기대지 않는 독립 부품만 둔다:
  ArgoSheetPalette(밝은·어두운 색표와 색 바꾸기), ArgoSheetHeaderView(행·열 머리글),
  ArgoSheetTabStrip(시트 탭 줄), ArgoSheetHelpController(단축키 설명서 화면),
  ArgoSheetKeyMap(단축키 표 — 처리와 설명서가 같은 표를 쓴다. 설명서에만 있고 처리가 없는 키가 없어야 한다).
컨트롤러 쪽 연결은 ArgoSheetViewer.swift 에.

설정 키(UserDefaults.settings): sheetDarkMode(Bool, 기본 false), sheetHeaders(Bool, 기본 true),
  sheetCross(Bool, 기본 true), sheetTextWeight(Int 0/1/2, 기본 1), sheetMinFont(Bool, 기본 false), sheetZoom(Double).

==================================================================
1. 글자 굵기 (3m 거리)
==================================================================
- makeFont: sheetTextWeight 에 따라 굵기를 올린다.
    0 보통: 지금과 같음 / 1 굵게(기본): 보통 글자 → .semibold, 굵은 글자 → .heavy /
    2 아주 굵게: 1 과 같고 획 두께 추가.
  한글은 시스템 대체 글꼴로 그려져 굵기 지정이 안 먹을 수 있다. 그래서 글자에 획을 함께 그린다:
  kCTStrokeWidthAttributeName 음수(채우고+획) — 0: 0, 1: -1.5, 2: -3.0 (글자 크기의 %), 획 색 = 글자 색.
  엑셀의 굵게/보통 차이는 그대로 남아야 한다.
- sheetMinFont 가 켜져 있으면 화면에서 글자 크기(font_size_pt × zoom)가 14pt 보다 작을 때 14pt 로 키워 그린다
  (칸을 넘치면 지금처럼 잘림). 끄면 지금과 같음.
- 정보줄: topBar 높이 76 → 92, 주소 30pt bold, 값 30pt semibold, 수식 26pt medium.
  bottomBar 60 → 72.
- 확대 단계 [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0], 처음 열 때 기본 1.75, 마지막으로 쓴 배율을 sheetZoom 에 저장해 다음에 쓴다.

==================================================================
2. 키보드 단축키 + 퀵 메뉴 + 단축키 설명서
==================================================================
리모컨 조작(방향키·선택·재생/일시정지·메뉴)은 지금 그대로 둔다. 키보드는 canvas.onKey 로 받는다.
애플 키보드에는 PageUp·Home·End 가 없는 것이 많다 → 모든 동작에 글자·화살표 조합 대안을 둔다.

| 동작 | 키 |
|---|---|
| 한 칸 이동(누르고 있으면 반복, 리모컨과 같은 간격) | ←↑→↓ |
| 데이터 끝으로(엑셀 Ctrl+화살표 규칙: 빈 칸/찬 칸 경계까지) | ⌃+화살표, ⌘+화살표 |
| 한 화면 위·아래 | PageUp/PageDown, ⌥↑/⌥↓, ⇧Space/Space |
| 한 화면 왼쪽·오른쪽 | ⌥PageUp/⌥PageDown, ⌥←/⌥→ |
| 행의 처음 | Home, ⌘⌥← |
| A1 / 마지막 칸 | ⌃Home·⌘⌥↑ / ⌃End·⌘⌥↓ |
| 이전·다음 시트 | ⌃PageUp/⌃PageDown, [ / ], ⇧Tab/Tab |
| 셀 주소 입력 | 숫자 1~9 (입력창이 그 숫자가 들어간 채로 열림), ⌃G, ⌘L |
| 확대 / 축소 / 100% | + 또는 = 또는 ⌘+ / - 또는 ⌘- / 0 |
| 찾기 / 다음·이전 결과 | ⌃F·⌘F / F3·⌘G, ⇧F3·⌘⇧G |
| 셀 정보·퀵 메뉴 | Return(=리모컨 선택), M |
| 다크모드 / 머리글 / 십자 강조 켜고 끄기 | D / H / X |
| 단축키 설명서 | ? , / , F1 |
| 닫기(떠 있는 창이 있으면 그것부터) | Esc(=리모컨 메뉴) |

- 데이터 끝 이동: 다음 칸이 비었는지는 as_cell_info 의 kind 로 본다. 찾는 거리는 지금 시트의 rowCount/colCount 까지만.
- Tab 은 포커스 엔진이 쓰는 키라 안 올 수도 있다. 오면 처리하고, 안 와도 다른 키로 되게 한다.
- 셀 주소 입력창(UIAlertController + 글상자): "B12", "12"(지금 열의 12행), "C"(지금 행의 C열),
  "시트이름!B3", "'시트 이름'!B3" 를 받는다. 이미 있는 jump(to:) 를 재사용·확장한다. 틀린 주소는 짧게 알리고 그대로.
- 찾기: 글상자 입력 → as_find(book, text, hits, 500). 결과를 들고 지금 커서 뒤의 첫 결과로 간다.
  다른 시트면 그 시트를 연다. 화면 가운데 위에 "3 / 17" 을 1.5초 띄운다. 없으면 "찾지 못함".
- 퀵 메뉴 = 지금의 showCellSheet 를 넓힌다(리모컨 선택 버튼, Return, M).
  제목 셀 주소, 본문 셀 정보(지금과 같음). 항목 순서:
  [링크로 이동 / 링크 QR 보기(있을 때)] · 셀 주소로 이동 · 찾기 · 시트 목록… · 확대 n% · 축소 n% ·
  다크모드 켜기/끄기 · 머리글 보이기/숨기기 · 십자 강조 켜기/끄기 · 글자 굵기… · 단축키 설명서 · 닫기.
  "시트 목록…" 은 보이는 시트를 나열하고 지금 시트 앞에 ✓. "글자 굵기…" 는 보통/굵게/아주 굵게.
- 단축키 설명서(ArgoSheetHelpController): 반투명 어두운 판 위 큰 글자(26pt 이상) 표.
  왼쪽 칸 "리모컨", 오른쪽 칸 "키보드". 표 내용은 ArgoSheetKeyMap 에서 만든다(손으로 따로 적지 않는다).
  화면을 넘치면 두 단으로 나눈다. 리모컨 메뉴·Esc·아무 키로 닫힌다.
  showQR 이 이미 쓰는 "overFullScreen 으로 띄운 포커스 없는 화면" 방식을 따라 한다.

==================================================================
3. 다크모드 (기본 꺼짐, 퀵 메뉴·D 키로 켬)
==================================================================
- 색 바꾸기는 Swift 의 drawCommands 한 곳에서 한다(타일·차트가 같은 길을 지나므로 색이 어긋나지 않는다).
  drawCommands 에 팔레트 인자를 더한다. 켜고 끄면 타일·오버레이를 다시 그린다(reset 과 같은 길).
- 규칙(ArgoSheetPalette):
  · 바탕(빈 칸, drawTile 404줄의 흰색, host·grid 배경): #1C1E22
  · 채우기(Fill·Gradient·Pattern 명령): HSL 로 바꿔 색상은 두고 밝기 L' = 0.10 + 0.30 × L, 채도 × 0.85.
  · 글자(Text): 바뀐 바탕과 대비 4.5:1 이상이 될 때까지 밝기만 올린다. 검정 글자는 #E6E8EB 쯤이 된다.
    빨강·파랑 같은 색 글자는 색상을 유지한다.
  · 격자선: #3A3E45. 테두리(Line): 밝기 L' = 1 − 0.7 × L (검정 테두리 → 밝은 회색).
  · 막대(Bar)·아이콘(Icon)·메모 표시(Marker)·차트 계열 색: 그대로.
  · 차트 오버레이: 거의 흰색(L > 0.92)은 #23262B, 거의 검정(L < 0.25)인 글자·축은 #D0D4DA. 나머지 그대로.
  · 그림(PNG·JPEG·EMF): 그대로.
  · 위·아래 막대, 머리글, 탭, 십자, 커서도 어두운 색표를 쓴다.

==================================================================
4. 지금 보고 있는 시트 표시
==================================================================
- bottomBar 의 sheetLabel 을 ArgoSheetTabStrip 으로 바꾼다(포커스 받지 않는 그림 뷰).
  보이는 시트만 탭으로. 지금 시트 탭: 흰 바탕(다크: #2E3238) + 굵은 26pt + 위쪽 5pt 띠(녹색 #10B981).
  다른 탭: 막대 색보다 조금 밝은 바탕 + 흐린 글자 22pt.
  엑셀 탭 색이 있으면(as_sheet_tab_color, 아래 Rust) 지금 탭은 위쪽 띠를, 다른 탭은 아래 4pt 띠를 그 색으로.
  탭이 많아 넘치면 지금 탭이 항상 보이도록 줄을 옮긴다. zoomLabel 은 오른쪽에 그대로.
- 시트를 바꿀 때 화면 가운데에 "3 / 7 · 시트이름" 을 크게(40pt, 반투명 판) 1초 띄우고 서서히 사라지게.

==================================================================
5. 행·열 머리글 (A,B,C / 1,2,3)
==================================================================
- grid 안 위쪽에 열 머리글 줄, 왼쪽에 행 머리글 칸. 그만큼 4개 host 를 오른쪽·아래로 민다.
  layoutQuadrants·placeCursor·scrollToCursor 의 좌표가 이 여백을 반영해야 한다.
- ArgoSheetHeaderView 는 draw(_:) 로 **보이는 범위만** 그린다(열 16,384개를 뷰로 만들지 않는다).
  화면 x → 시트 열: x < 고정 너비면 고정 열 구간(시트 x = 화면 x / zoom), 아니면
  시트 x = freezePt.x + (화면 x − 고정 너비 + bodyOffset.x) / zoom. 행도 같은 식. colOffsets/rowOffsets 를 이분 탐색.
  bodyOffset·zoom·커서가 바뀔 때 setNeedsDisplay.
- 글자: 15 × zoom pt, 18~30pt 로 자름. 열 머리글 높이 = 글자 × 1.6. 행 머리글 너비 = 마지막 행 번호 자릿수에 맞춤.
- 색(밝은): 바탕 #F3F4F6, 글자 #374151 semibold, 칸 경계 #D1D5DB.
  지금 커서의 행·열(병합이면 병합 범위 전체): 바탕 #D1FAE5, 글자 #065F46 bold, 안쪽 가장자리 3pt 띠 #10B981.
  (다크): 바탕 #2B2F36, 글자 #C9CED6, 지금 행·열 바탕 #14532D 글자 #BBF7D0.
- 숨긴 행·열(높이·너비 0)은 그 경계에 2pt 이중선을 그려 번호가 건너뛴 것이 보이게.
- 왼쪽 위 모서리 칸에 지금 셀 주소(예: K32).
- H 키·퀵 메뉴로 숨기기/보이기(sheetHeaders). 숨기면 지금과 똑같은 배치.

==================================================================
6. 커서 중심 십자 강조 (한컴 엑셀뷰어처럼)
==================================================================
- 결정 사항: 선택 칸과 같은 행·열의 빈 칸은 회색, 채움색이 있는 칸은 그 색이 약간 더 진해 보이게.
  → 칸별 색을 알 필요 없이 **반투명 검정 한 장**으로 된다: 검정 알파 0.08 을 덮으면
     흰 칸 → 연회색, 색 칸 → 같은 색이 8% 진해짐, 글자는 그대로 보인다.
  다크모드에서는 흰색 알파 0.07(조금 밝아짐).
- 4개 ArgoSheetTileView 각각에 가로 띠 1장·세로 띠 1장(배경색만 있는 CALayer, 그림 없음 → 메모리 없음)을
  zPosition 300 으로 둔다. 순서: 타일 → 십자(300) → 차트·그림(400) → 커서(500).
  가로 띠: 커서 행(병합이면 병합 행 범위) 높이 × 그 타일 뷰 전체 너비. 세로 띠도 같은 식.
  placeCursor 와 같은 때 갱신, 애니메이션 끔(CATransaction.setDisableActions(true)).
  타일을 다시 그리지 않으므로 커서 이동이 느려지지 않아야 한다.
- 커서 칸 자체는 덮지 않는다(가로·세로 띠가 겹치는 칸만 빼거나, 겹친 곳이 두 번 어두워지지 않게 띠를 커서 칸에서 끊는다).
- X 키·퀵 메뉴로 켜고 끔(sheetCross).

==================================================================
7. Rust — 시트 탭 색 (작은 일)
==================================================================
- parse.rs: 시트 XML 의 <sheetPr><tabColor rgb=".." | theme=".." tint=".." | indexed=".."/></sheetPr> 를 읽어
  Sheet 에 tab_color: Option<Rgba> 를 더한다. 색 해석은 style.rs 의 기존 색 해석(테마·tint·indexed)을 그대로 쓴다.
  Sheet 를 직접 만드는 시험 코드에는 tab_color: None 을 더한다.
- lib.rs: #[no_mangle] pub extern "C" fn as_sheet_tab_color(book: *const ASBook, sheet: u32, out: *mut ASColor) -> bool
  (catch_unwind, 널·범위 검사, 색 없으면 false).
- argosheet.h: bool as_sheet_tab_color(const ASBook* book, uint32_t sheet, ASColor* out);
  기존 구조체(ASSheetInfo·ASCellInfo·ASDrawCmd)는 바꾸지 않는다.
- 시험: rgb, theme+tint, 없음, 범위 밖 시트, 널 포인터.

[합격]
- 리모컨만으로 지금 되는 것이 전부 그대로 된다.
- 키보드만으로 표의 모든 동작이 된다. 설명서 표와 실제 동작이 같다.
- 커서를 빠르게 움직여도 타일이 다시 그려지지 않는다(십자·머리글만 갱신).
- 다크모드에서 검정 글자·검정 테두리·흰 칸이 모두 읽힌다. 차트 계열 색은 그대로다.
```
