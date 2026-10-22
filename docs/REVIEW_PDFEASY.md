# PDFEasy-iOS 검토 — 그리고 3단계의 방향이 바뀝니다

`balzo-tech/PDFEasy-iOS` (`72cac2c`) 를 받아 읽었습니다.

**결론부터: 이 저장소가 3단계 설계를 바꿉니다.** 코드를 가져오는 게 아니라 **방법**을 가져옵니다.

---

## 1. PDFEasy 가 문서를 PDF 로 바꾸는 방법

`pdfexpert/InternalUtils/DocumentRenderUtility.swift` 한 파일이 전부입니다. 다리가 셋입니다.

```
문서 → WKWebView 가 그린다
     → webView.viewPrintFormatter() + UIPrintPageRenderer 가 A4 로 쪽을 나눈다
     → UIGraphicsPDFRenderer 가 PDF 로 찍는다
     → PDFDocument 가 빈 쪽인지 검사한다
```

**주석에 적힌 두 가지가 특히 값집니다.**

첫째, `WKWebView.createPDF(configuration:)` 를 **쓰지 않은 이유**입니다.

> createPDF 는 내용 크기에 맞춰 출력을 잡으므로, 긴 문서가 쪽 구분 없는 거대한 한 장으로 나온다. `viewPrintFormatter()` 만이 제대로 쪽을 나눈다.

우리가 지금 겪는 문제가 바로 이것입니다. 지금 뷰어는 HTML 을 길게 흘려보내 **'쪽'이라는 개념이 없습니다.** 그래서 요청하신 13번(가로 폭 맞춤·세로 폭 맞춤·2장 보기·책장 넘기기)이 **애초에 성립하지 않았습니다.** 쪽이 없는데 쪽을 넘길 수가 없습니다.

둘째, **실패가 오류로 안 온다**는 사실입니다.

> WebKit 이 렌더링할 수 없는 문서는 보통 오류가 아니라 *빈 쪽*을 내놓는다. 호출자가 그것을 알아야 한다.

**우리가 이미 당한 곳입니다.** ppt 가 흰 글자를 흰 슬라이드에 그려 빈 화면으로 보였고, hwp 는 오류도 출력도 없었습니다. PDFEasy 는 이것을 `validateRenderedPdf` 로 막습니다 — 쪽을 작게 흑백으로 그려 **어두운 점의 비율**을 세고, 임계값 아래면 "빈 출력"으로 거부합니다.

```swift
let inkedCount = pixels.reduce(0) { $1 < inkLevel ? $0 + 1 : $0 }
return CGFloat(inkedCount) / CGFloat(width * height)
```

우리 탐침의 색 수 세기와 같은 생각인데 더 단단합니다. 글자가 있으면 바로 통과시키고(`page.string`), 없을 때만 픽셀을 셉니다.

---

## 2. tvOS 에서 되는지 — SDK 에 물었습니다

PDFKit 도 WebKit 도 없는 tvOS 에서 이 세 다리가 설 수 있나. **인쇄 API 는 tvOS 기능이 아니므로 없을 것이라 예상했는데, 틀렸습니다.**

SDK 조회 (실행 39766805425, tvOS 17.5 SDK):

| API | 결과 |
|---|---|
| `UIPrintPageRenderer` | **PRESENT** |
| `UIPrintFormatter` | **PRESENT** |
| `UISimpleTextPrintFormatter` | **PRESENT** |
| `UIMarkupTextPrintFormatter` | **PRESENT** |
| `viewPrintFormatter` | **PRESENT** |
| `UIGraphicsPDFRenderer` | **PRESENT** |
| `CGPDFDocument` | **PRESENT** |
| `CGContextDrawPDFPage` | **PRESENT** |
| `PDFKit` · `WebKit` · `QuickLook` | ABSENT (기존과 동일) |

**인쇄 서식기가 전부 들어 있습니다.** tvOS 에 인쇄 기능이 없는데도 클래스는 ship 됩니다.

없는 것은 `PDFDocument` 하나이고, 그 역할(되읽기 + 빈 쪽 판정)은 **`CGPDFDocument` + `CGContextDrawPDFPage` 로 그대로 대체됩니다.** 잉크 세기는 `PDFPage.thumbnail` 대신 `CGContext` 에 직접 그려서 하면 됩니다.

그리고 **`UIMarkupTextPrintFormatter` 가 있다는 것이 결정적입니다.** 이것은 **HTML 을 받아 쪽으로 나눕니다 — WebKit 이 전혀 필요 없습니다.**

---

## 3. 그래서 3단계가 이렇게 바뀝니다

### 지금

```
문서 → JS 렌더러(docx-preview/SheetJS/pptx-preview/hwp.js) → HTML → WKWebView 가 길게 흘려보냄
```

쪽이 없습니다. 맞춤·2장 보기·책장 넘기기가 불가능합니다.

### 바뀐 뒤

```
문서 → JS 렌더러 → HTML → UIMarkupTextPrintFormatter
                         → UIPrintPageRenderer (A4 로 쪽 나누기)
                         → UIGraphicsPDFRenderer (PDF)
                         → CGPDFDocument 로 쪽 단위 그리기
                         → 잉크 비율로 빈 쪽 검사
```

**JS 렌더러는 그대로 둡니다.** 이미 잘 돌고 있습니다. 바뀌는 것은 **그 뒤**입니다.

얻는 것:

- **진짜 쪽** — 가로 폭 맞춤, 세로 폭 맞춤, 2장 같이 보기, 책장 넘기기가 전부 성립합니다
- **일관된 UI** — pdf·docx·xlsx·pptx·hwp 가 전부 "쪽이 있는 PDF" 가 되므로 뷰어가 **하나**로 통일됩니다. 요청하신 12번입니다
- **빈 출력 판정** — ppt 가 빈 화면으로 나오던 종류의 실패를 사용자에게 내보내지 않고 잡아냅니다
- **기존 pdf 경로와 합쳐짐** — `CGPDFDocument` 가 이미 있으니 pdf 파일은 변환 없이 같은 뷰어로 들어갑니다

잃는 것 / 조심할 것:

- **쪽 나누기가 레이아웃을 다시 잡습니다.** 원본 생김새가 조금 달라질 수 있습니다 — 특히 넓은 엑셀 표가 A4 폭에서 잘릴 수 있습니다. 용지 크기를 가로로 두거나 축소 배율을 주는 선택지가 필요합니다
- **PDFEasy 도 한계를 인정합니다.** 주석에 "Word/RTF 는 좋고 복잡한 표와 docx 머리말·로고·물결무늬는 약하다"고 적혀 있습니다. 다만 우리는 WebKit 의 Office 렌더링이 아니라 **우리 JS 렌더러의 HTML** 을 넣으므로 이 한계는 그대로 적용되지 않습니다
- **한글 글꼴** — 서식기가 한글을 그리는지 확인이 필요합니다

---

## 4. 가져오지 않는 것

**코드를 복사하지 않습니다.** PDFEasy 는 라이선스 파일이 보이지 않고, 상용 앱 저장소입니다. 방법과 API 사용법은 사실이지 저작물이 아니므로 그것만 씁니다.

**서버 변환(`StirlingApiManager`)은 안 씁니다.** PDFEasy 는 기기에서 안 되면 Stirling 서버로 보냅니다. 우리는 서버가 없고, 파일을 밖으로 보내는 것은 파일 탐색기가 할 일이 아닙니다.

> 앞서 말씀하신 **프린터의 PDF 변환 기능**이 사실 이 자리입니다. 기기에서 안 되는 문서를 프린터에 보내 PDF 로 받아오는 것 — 다만 네트워크·규격 확인이 필요해 지금 범위 밖에 둡니다.

`PdfKitView`·`PdfRedactUtility` 등 PDFKit 에 기댄 나머지는 tvOS 에서 쓸 수 없습니다.

---

## 5. 다음 한 걸음 — 탐침부터

**"declare 되어 있다"는 "작동한다"가 아닙니다.** PDFKit 때 그 차이로 한 번 다시 썼습니다.

`Lab ③ 인쇄 파이프라인 탐침` 을 만들었습니다 (설정 안). 묻는 것은 '클래스가 있나'가 아니라 **'잉크가 나오나'** 입니다.

1. 런타임에 클래스가 실제로 있는지 (`NSClassFromString`, `instancesRespond(to:)`)
2. 글자 300줄이 **여러 쪽으로 나뉘는지** — 한 쪽으로 나오면 쪽 나누기가 안 되는 것입니다
3. **HTML 표 120행이 쪽으로 나뉘는지** ★핵심★
4. `CGPDFDocument` 로 되읽어 **쪽마다 잉크 비율**을 재는지
5. **한글이 그려지는지**

3번 PDF 를 `/var/mobile/Media/argo-print-probe.pdf` 로 저장하므로 **탐색기에서 열어 눈으로 확인**하실 수 있습니다.

이 다섯 개가 통과하면 위 설계대로 갑니다. 2번이나 3번이 한 쪽으로 나오면 서식기가 헤더에만 있는 것이고, 그때는 **`CoreText` 로 직접 쪽을 나누는 길**로 갑니다 (PRESENT 이고, 더 손이 많이 가지만 확실합니다).

**탐침 결과를 보고 3단계 지시서를 쓰겠습니다.** 지금 쓰면 가정 위에 쓰는 것입니다.
