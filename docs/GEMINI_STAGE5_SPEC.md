# 5단계 작성 지침 — 진단 화면과 워드 문서 읽기

## 0. 이번에 만들 것

두 가지입니다. **순서대로** 하십시오.

1. **진단 화면(Lab)** — 기능마다 점검 항목을 버튼으로 돌려 보고 결과를 로그로 남기는 화면
2. **워드 문서를 PDF로 바꿔 읽기** — 그 첫 적용 대상

지금 문서 뷰어는 글자만 나열합니다. 쪽도 없고 여백도 없어 문서로 읽히지 않습니다.
원본 서식 그대로 변환하는 것은 tvOS에 레이아웃 엔진이 없어 불가능합니다.
대신 **추출한 내용을 CoreText로 조판해 진짜 PDF로 만들고, 이미 동작하는 PDF 뷰어에 넘깁니다.**
쪽 넘김·두 쪽 보기·야간 모드가 공짜로 따라옵니다. 닫을 때 임시 PDF는 지웁니다.

---

## 1. 반드시 지킬 환경 제약

이 제약을 어긴 코드는 빌드가 깨지거나 앱이 켜지자마자 죽습니다. 전부 실제로 겪은 것들입니다.

### 빌드
- 배포 타깃 **tvOS 16.0**, Xcode 15.4 / tvOS 17.5 SDK, GitHub Actions `macos-14`
- 로컬 Xcode 없음. 빌드는 CI에서만 돕니다
- **`@retroactive` 쓰지 마십시오.** Xcode 15가 모릅니다
- 새 파일을 만들면 `project.pbxproj` 등록이 필요합니다. **파일 목록을 따로 알려 주십시오**

### 없는 API (문서에 있다고 쓰지 마십시오)
- **PDFKit 없음.** 애플 문서에는 tvOS 11+ 로 나오지만 SDK에 실물이 없습니다. 한 번 당했습니다
- **WebKit / WKWebView 없음.** HTML로 렌더링하는 길은 막혀 있습니다
- **QuickLook 없음**

### 있는 것 (CI에서 컴파일 확인 완료)
`CGPDFDocument` `CGPDFPage` `CGContext.drawPDFPage` `UIGraphicsImageRenderer`
`UIGraphicsPDFRenderer` `CGDataConsumer`
`CTFramesetterCreateWithAttributedString` `CTFramesetterCreateFrame`
`CTFramesetterSuggestFrameSizeWithConstraints` `CTFrameGetVisibleStringRange`
`CTFrameDraw` `CTLineCreateWithAttributedString` `CTLineDraw`
`CTLineGetTypographicBounds` `os.Logger`

→ `Spartan/Diagnostics/DocPipelineProbe.swift` 에 실제 호출 예가 있습니다. **그 파일의 호출 형태를 그대로 따르십시오.**

### tvOS UI 규칙
- **한 행에 포커스 받는 버튼은 하나만.** 행 전체가 하나의 포커스 단위라, 버튼이 둘이면 뒤쪽 것만 눌립니다
- **한 화면에 `.sheet` 는 하나만.** 둘 이상 달면 안 열립니다
- 뒤로가기(메뉴) 버튼은 `.onExitCommand`
- 드래그앤드롭 없음
- 가장자리 5% 는 TV가 잘라먹습니다. `safeAreaLayoutGuide` 안쪽으로
- **시청 거리 3미터, 55인치입니다.** 본문 28pt 이상, 로그는 Menlo 22pt 이상

### 앱이 죽는 지점
- `ContentView.swift` 의 `showSubView` 배열은 현재 **39개**입니다. 새 시트를 쓰려면 배열 크기부터 키우십시오. 안 키우면 **켜자마자 죽습니다**
- `ContentView.swift` 는 **공백과 탭이 섞여** 있습니다. 교체 블록은 글자 그대로 복사하십시오

---

## 2. 진단 화면 (Lab)

### 2-1. 생김새

화면을 셋으로 나눕니다.

```
┌──────────────┬───────────────────────────────┐
│ A. 워드 읽기 │  A1 컨테이너가 풀리는가   ✓  │
│ B. 표 읽기   │  A2 글자가 나오는가       ✓  │
│ C. 발표자료  │  A3 문단이 쪼개지지 않았나 ✗ │
│ D. 한글      │  A4 표가 섞이지 않았나    –  │
│ E. PDF       │           [ 모두 실행 ]       │
│ F. 삭제 권한 ├───────────────────────────────┤
│ G. 미디어    │ 15:02:11 A1 PASS  36 entries  │
│ H. WebDAV    │ 15:02:11 A3 FAIL  avg 1.2 ch  │
│              │ 15:02:12 A4 SKIP  no table    │
└──────────────┴───────────────────────────────┘
```

- **왼쪽** — 최상위 기능 목록. 행 하나당 버튼 하나
- **오른쪽 위** — 고른 기능의 점검 항목. 항목마다 상태 기호. 맨 아래 `모두 실행` 버튼 하나
- **오른쪽 아래** — 로그. 검은 바탕 초록 글자, Menlo 22pt, 자동 스크롤

상태 기호: `✓` 통과 / `✗` 실패 / `–` 건너뜀 / `○` 아직 안 돌림

### 2-2. 자료형

이 네 개로 고정합니다. 이름 바꾸지 마십시오.

```swift
enum DiagResult {
    case notRun
    case pass(String)   // 통과 + 근거 (예: "36 paragraphs, avg 47 chars")
    case fail(String)   // 실패 + 이유
    case skip(String)   // 해당 없음 + 이유
}

struct DiagCheck: Identifiable {
    let id: String              // "A3"
    let title: String           // "문단이 쪼개지지 않았는가"
    let run: (DiagLog) -> DiagResult
}

struct DiagCapability: Identifiable {
    let id: String              // "A"
    let title: String           // "워드 문서 읽기"
    let checks: [DiagCheck]
}

final class DiagLog: ObservableObject {
    @Published private(set) var lines: [String] = []
    func write(_ s: String)              // 화면 + os_log 동시 기록
    func save(to path: String) -> Bool   // 파일로 저장
}
```

### 2-3. 지킬 것

- 점검은 **반드시 백그라운드 큐**에서 돌리고 결과만 메인으로 보냅니다. 메인에서 돌리면 TV가 멈춥니다
- 점검 하나가 실패해도 **다음 항목은 계속 돕니다.** `모두 실행`이 중간에 끊기면 안 됩니다
- 로그 한 줄 형식: `HH:mm:ss  <id>  <PASS|FAIL|SKIP>  <상세>`
- `로그 저장` 버튼을 두고 `/var/mobile/Media/argo-diag-<yyyyMMdd-HHmmss>.log` 에 씁니다. **실패하면 샌드박스 안 `Documents/` 로 떨어뜨리고 어디에 썼는지 로그에 남기십시오**
- 시험 파일은 사용자가 고릅니다. 진단 화면에 **`시험 파일 고르기` 버튼**을 두고, 고른 경로를 `@AppStorage` 로 기억하십시오. 고른 게 없으면 모든 항목이 `skip("시험 파일을 먼저 고르십시오")`
- 진단 화면 진입은 **설정 화면 맨 아래 `진단 (Lab)` 항목** 하나로 합니다. 파일 목록 툴바는 건드리지 마십시오

### 2-4. 이번에 넣을 점검 항목

`A1`~`A12`만 구현하십시오. B~H는 **목록에만 올리고 전부 `skip("아직 구현 안 함")`** 으로 두십시오.
각 항목의 통과 기준은 `docs/CAPABILITY_MATRIX.md` 에 있습니다. 그대로 따르십시오.

`A3`이 이번의 핵심입니다. 판정식은 이렇게 하십시오.

```
평균 문단 글자 수 = 전체 글자 수 / 문단 수
  ≥ 12  → pass
  < 12  → fail("avg N chars/paragraph — 문단 분할 의심")
```

---

## 3. 고칠 것 — 문단이 단어 단위로 쪼개지는 버그

### 3-1. 증상

독일어 워드 문서를 열었더니 한 문단이 이렇게 나왔습니다.

```
1  Bayerisches
2  Landesamt
3  für Umwelt
```

### 3-2. 원인 (확정)

`OfficeDocReaderView.swift` 의 `extractParagraphs` 가 태그를 **접두어로만** 비교합니다.

```swift
let openP = "<\(paragraphTag)"           // "<w:p"
let pBlocks = xml.components(separatedBy: openP)
```

`"<w:p"` 는 `<w:p>` 말고도 **`<w:pPr>` `<w:pStyle>` `<w:proofErr>` `<w:pict>`** 에 걸립니다.
워드는 사전에 없는 단어마다 `<w:proofErr w:type="spellStart"/>` 를 끼워 넣습니다. 독일어 문서라 거의 모든 단어에 붙었고, 그래서 단어마다 끊긴 것입니다.

같은 문제가 텍스트 태그에도 있습니다.
- `"<w:t"` → `<w:tbl>` `<w:tc>` `<w:tr>` 에 걸림
- `"<a:t"` → `<a:tbl>` `<a:tr>` 에 걸림
- `"<a:p"` → `<a:pPr>` `<a:prstGeom>` 에 걸림

### 3-3. 고치는 법

태그 이름 **다음 글자가 `>` `/` 공백 탭 줄바꿈 중 하나**일 때만 진짜 태그로 봅니다.

```swift
private static func isTagBoundary(_ c: Character) -> Bool {
    return c == ">" || c == "/" || c == " " || c == "\t" || c == "\n" || c == "\r"
}
```

`components(separatedBy:)` 로는 이 판정을 못 합니다. **직접 훑는 방식으로 바꾸십시오.**
`w:p` `a:p` `hp:p` 세 포맷이 같은 함수를 쓰므로 여기만 고치면 셋 다 해결됩니다.

덤으로 같이 처리할 것:
- `<w:br/>` `<w:cr/>` → 줄바꿈, `<w:tab/>` → 탭
- `<a:fld ... type="slidenum">` 안의 글자는 **버립니다** (지금 슬라이드에 `1` 이 본문으로 나옵니다)
- `<w:tbl>` / `<a:tbl>` 안은 본문과 **따로** 모읍니다

---

## 4. 만들 것 — 문서를 PDF로 조판하기

### 4-1. 새 파일

`Spartan/FileOpeners/OfficeReader/DocumentPDFBuilder.swift`

```swift
struct DocumentBlock {
    enum Kind { case heading, body, tableRow, pageBreak }
    let kind: Kind
    let text: String
    let cells: [String]     // tableRow 일 때만
}

enum DocumentPDFBuilder {
    /// 블록을 조판해 PDF 파일로 씁니다. 성공하면 쪽수를 돌려줍니다.
    static func build(blocks: [DocumentBlock],
                      title: String,
                      to path: String) -> Int?
}
```

### 4-2. 조판 규칙

- 쪽 크기 **1920 × 1080** (TV 화면비). 세로 A4로 만들면 TV에서 양옆이 비어 글자가 작아집니다
- 여백 상하 80, 좌우 120 (오버스캔 고려)
- 본문 **34pt**, 제목 **46pt 굵게**, 줄간격 1.5배. 3미터에서 읽어야 합니다
- 쪽 아래 가운데에 `— n —` 쪽번호
- 첫 쪽 맨 위에 문서 이름
- 글꼴: `UIFont.systemFont(ofSize:)` 로 두십시오. 한글이 시스템 글꼴로 나옵니다.
  `DocPipelineProbe.probeKoreanFontNames()` 로 실제 쓸 수 있는 이름을 로그에 남겨 주십시오
- 표는 `CTLineCreateWithAttributedString` + `CTLineDraw` 로 열 위치를 직접 잡습니다.
  `CTFramesetter` 로는 열을 못 맞춥니다

### 4-3. 쪽 나누기

`DocPipelineProbe.probeWritePaginatedPDF` 의 짜임을 그대로 쓰십시오. 요점만 적으면

```
location = 0
while location < 전체길이 {
    beginPage()
    cgContext 를 뒤집는다 (CoreText 는 아래가 원점)
    CTFramesetterCreateFrame(framesetter, CFRangeMake(location, 0), path, nil)
    CTFrameDraw
    visible = CTFrameGetVisibleStringRange(frame)
    if visible.length <= 0 { break }     // 무한루프 방지. 반드시 넣으십시오
    location += visible.length
}
```

`visible.length <= 0` 검사를 빠뜨리면 **영원히 돕니다.** 꼭 넣으십시오.

### 4-4. 임시 파일 수명

- 변환본은 `NSTemporaryDirectory()/argo_docpdf_<UUID>/<원래이름>.pdf`
- **뷰어를 닫을 때 그 폴더째 지웁니다**
- 원본은 절대 건드리지 마십시오
- 앱이 켜질 때 `argo_docpdf_` 로 시작하는 남은 폴더를 싹 지우십시오 (강제 종료 대비)

### 4-5. 화면 연결

`OfficeDocReaderView` 를 이렇게 바꾸십시오.

1. 열면 바로 파싱 → `DocumentPDFBuilder.build` → 성공하면 **그 PDF 경로로 `PDFReaderView` 를 띄웁니다**
2. 변환이 실패하면 **지금의 글자 목록 화면으로 떨어집니다** (안전망이니 지우지 마십시오)
3. 변환 중에는 `문서를 여는 중… (n/m 쪽)` 진행 표시

**한 화면에 `.sheet` 는 하나입니다.** `OfficeDocReaderView` 안에서 `PDFReaderView` 를 또 `.sheet` 로 띄우지 마십시오. 같은 화면 안에서 뷰를 **갈아 끼우는** 방식으로 하십시오.

---

## 5. 곁다리로 같이 고칠 것

변환기와 무관하게 지금 틀린 것들입니다. 같이 고치십시오.

| 위치 | 문제 | 고칠 내용 |
|---|---|---|
| `OfficeDocReaderView.swift:43` | 주간 모드가 `Color(white: 0.12)` 라 야간과 구분이 안 됨 | 주간 = 밝은 바탕 + 검은 글자 |
| `:138` | 줄마다 버튼이라 긴 문서를 못 내려감 | 변환 성공 시엔 PDF 뷰어가 처리. 안전망 화면은 쪽 단위 이동으로 |
| `:141` | 왼쪽 줄번호가 잡음 | 표(xlsx)에서만 표시 |
| `:279` | 엑셀 시트가 사전순 정렬 (sheet10 → sheet2) | 파일명 끝 숫자로 정렬. pptx 방식 그대로 |
| `:287` | 탭이 `Sheet 1` | `xl/workbook.xml` 의 진짜 이름 |
| `:343` | 글자 없는 슬라이드가 사라지고 번호가 밀림 | 파일명 번호 사용, 빈 슬라이드도 자리 유지 |
| `:354` | 한글 구역도 사전순 정렬 | 숫자 정렬 |
| `PDFReaderView.swift:159` | `.colorInvert()` 가 컬러 슬라이드를 허옇게 만듦 | 쪽 평균 밝기를 재서 **이미 어두운 쪽은 반전하지 않음** |

마지막 항목 보충: `UIGraphicsImageRenderer` 로 쪽을 작게(예: 40×22) 뽑아 픽셀 평균 밝기를 구하십시오.
밝기 0.5 이상(흰 바탕 문서)만 반전하고, 미만이면 그대로 두거나 살짝 어둡게만 하십시오.
판정값을 로그에 남겨야 진단 화면에서 확인할 수 있습니다.

---

## 6. 제출 형식

파일마다 이렇게 주십시오.

1. **전체 파일** — 새로 만든 파일은 처음부터 끝까지
2. **교체 블록** — 기존 파일을 고칠 때는 `바꾸기 전` / `바꾼 후` 를 짝으로.
   `바꾸기 전` 은 **현재 파일에 딱 한 번만 나오도록** 앞뒤를 충분히 붙이십시오.
   `ContentView.swift` 는 같은 블록이 **두 군데** 나오는 경우가 있으니 몇 번째인지 적어 주십시오
3. **새 파일 목록** — `project.pbxproj` 에 등록해야 하니 경로를 따로 모아 주십시오
4. **자가 점검** — 아래에 스스로 답하십시오
   - `showSubView` 배열 크기를 건드렸는가? 그렇다면 몇으로?
   - 한 행에 포커스 버튼이 둘 이상인 곳이 있는가?
   - 한 화면에 `.sheet` 가 둘 이상인 곳이 있는가?
   - tvOS 16에 없는 API를 쓴 곳이 있는가?
   - 쪽 나누기 루프에 종료 조건을 넣었는가?
5. **확신이 안 서는 부분** — 짐작으로 메우지 말고 그대로 적어 주십시오. 제가 실제 SDK로 확인해 드립니다

---

## 7. 하지 말 것

- 기존에 동작하는 기능을 건드리지 마십시오. 휴지통·즐겨찾기·WebDAV·PDF 뷰어의 읽기 부분은 그대로 둡니다
- PDF 뷰어에 목차·북마크·썸네일·형광펜·검색을 **되살리지 마십시오.** 일부러 걷어낸 것입니다
- 원본 파일을 고치거나 지우지 마십시오
- 한 번에 B~H까지 다 하려 하지 마십시오. **A만** 합니다
