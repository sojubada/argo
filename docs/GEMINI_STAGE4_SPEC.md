# 4단계 작업 지침 — 형광펜 · 하이라이트 목록 · 검색

저장소: `sojubada/argo` (브랜치 `main`)
선행 문서: `GEMINI_TASK_SPEC.md` (0절 환경 제약은 그대로 유효)

---

## 0. 먼저 — 이번엔 검증을 마치고 적었습니다

3단계에서 PDFKit이 tvOS에 없다는 걸 빌드에서야 알았습니다. 같은 일을 막으려고,
이번 지침을 쓰기 **전에** 4단계에 필요한 API를 전부 담은 탐침 파일을 CI에서
컴파일시켰습니다. **경고 하나 없이 통과했습니다.**

탐침 파일: `Spartan/FileOpeners/PDFReader/CGPDFProbe4.swift`
(저장소에 있고, 이번 작업에 첨부되어 있습니다)

확인된 것:

| 기능 | 검증된 API | 판정 |
|---|---|---|
| PDF 새로 쓰기 | `UIGraphicsPDFRenderer(bounds:)`, `writePDF(to:)`, `ctx.beginPage(withBounds:pageInfo:)` | **가능** |
| PDF 새로 쓰기 (CG) | `CGDataConsumer(url:)`, `CGContext(consumer:mediaBox:_:)`, `beginPage/endPage/closePDF` | **가능** |
| 블렌드 합성 | `cg.setBlendMode(.multiply)` | **가능** |
| 콘텐츠 스트림 스캔 | `CGPDFOperatorTableCreate/SetCallback/Release`, `CGPDFContentStreamCreateWithPage/Release`, `CGPDFScannerCreate/Scan/Release` | **가능** |
| 연산자 피연산자 | `CGPDFScannerPopString/PopArray/PopNumber/PopName` | **가능** |
| 폰트 자원 접근 | `CGPDFDictionaryApplyFunction`, `CGPDFObjectGetValue`, `CGPDFDictionaryGetStream`, `CGPDFStreamCopyData` | **가능** |

**탐침에 없는 API를 쓰셔야 하면, 쓰기 전에 "확신이 없는 부분"에 반드시 적으십시오.**

---

## 1. 설계 판단 — 왜 이렇게 하는가

PDFKit이 없으므로 `PDFSelection`, `PDFAnnotation`, `page.selection(for:)`,
`document.findString` 이 **전부 없습니다.** 따라서 원래 사양서 3.4·3.5절의
"텍스트에 스냅되는 형광펜"과 "텍스트 검색"을 그대로 만들 수 없습니다.

### 1.1 형광펜 — 텍스트 스냅을 **버립니다**

텍스트 위치를 알려면 콘텐츠 스트림을 직접 파싱해 텍스트 행렬(`Tm`/`Td`),
글자 간격, 글리프 폭을 전부 추적해야 합니다. 분량도 크고, 서브셋 폰트에서는
자주 실패합니다. 검증할 수단도 없습니다.

**대신 자유 사각형 방식으로 만듭니다.** 리모컨으로 반투명 상자를 움직여
칠하고 싶은 영역을 직접 지정합니다.

이게 열등한 타협만은 아닙니다. 스캔본 PDF(이미지만 있고 텍스트가 없는 문서)
에서는 텍스트 스냅이 **아예 동작하지 않지만**, 사각형 방식은 그대로 됩니다.
TV 리모컨으로는 어차피 단어 단위 정밀 선택이 불편합니다.

### 1.2 검색 — **쪽 단위 최선노력(best-effort)** 으로 한정합니다

콘텐츠 스트림 스캔은 가능하지만, 바이트를 읽을 수 있는 글자로 바꾸려면
폰트의 `/ToUnicode` CMap이 필요합니다. 그것이 없는 문서(서브셋 폰트, 스캔본)
에서는 **올바른 텍스트를 얻을 수 없습니다.**

그래서 이렇게 한정합니다.

- `/ToUnicode` 가 **있는** 폰트의 텍스트만 수집합니다
- 없는 폰트는 **건너뜁니다.** 억지로 추측해 깨진 글자를 만들지 마십시오
- 결과는 **"이 단어가 몇 쪽에 있다"** 까지만. 쪽 안에서 위치를 표시하지 않습니다
- 한 쪽도 텍스트를 못 얻으면 **"이 PDF에서는 텍스트를 찾을 수 없습니다
  (스캔본이거나 글꼴에 유니코드 정보가 없습니다)"** 라고 분명히 알립니다

**깨진 글자를 보여주느니 못 찾았다고 말하는 쪽이 낫습니다.**

---

## 2. 현재 코드 상태

`Spartan/FileOpeners/PDFReader/` 에 3단계 결과물이 들어 있습니다.

```
PDFReaderModel.swift     // CGPDFDocument, 페이지/보기모드/테마/북마크/목차
PDFPageRenderer.swift    // 래스터라이즈, NSCache, 슬롯별 취소 토큰
PDFReaderView.swift      // 화면, 툴바, 단일 .sheet, 리모컨 처리
PDFOutlineView.swift     // 목차/북마크 2탭 + 썸네일 격자
CGPDFProbe4.swift        // 탐침 — 4단계 완료 시 삭제 요청할 것
```

**제가 3단계 검수에서 한 군데 바꿨습니다.** `PDFPageRenderer`의 렌더 큐를
`attributes: .concurrent` → **직렬**로 바꿨습니다. `renderDetached`에 취소
토큰이 없어서, 긴 문서를 빠르게 스크롤하면 동시 래스터라이즈가 무제한으로
쌓이기 때문입니다. 이 구조를 유지하십시오.

---

## 3. 작업 1 — 형광펜

### 3.1 좌표계 (가장 중요)

하이라이트는 **PDF 페이지 좌표계**로 저장해야 합니다. 화면 픽셀로 저장하면
확대 배율이나 TV 해상도가 달라지는 순간 위치가 어긋납니다.

- 저장 단위: 해당 페이지의 `cropBox`(없으면 `mediaBox`) 기준, **원점 좌하단**
- 화면 → PDF 변환, PDF → 화면 변환 함수를 **한 쌍으로** 만들고 양쪽에서
  그 함수만 쓰십시오. 변환 로직을 두 군데 적지 마십시오
- `PDFPageRenderer.rasterizePage`가 쓰는 것과 **같은 스케일 계산**을 쓰십시오
  (`scale = min(target.w / effectiveW, target.h / effectiveH)`)

### 3.2 조작 (리모컨)

상단 툴바에 형광펜 버튼(`highlighter`)을 추가하고, 누르면 형광펜 모드 진입.

**모드 안에 두 단계를 둡니다.** 재생/일시정지 버튼으로 전환합니다.

| 단계 | 상하좌우 | 선택(클릭) |
|---|---|---|
| 이동 | 상자를 움직임 | 확정 → 하이라이트 추가 |
| 크기조절 | 상자 크기를 바꿈 | 이동 단계로 복귀 |

- 상자는 반투명 채움 + 뚜렷한 테두리. 현재 단계를 화면 하단에 한 줄로 표시
  ("Move · Play/Pause to resize · Select to highlight · Menu to exit")
- 한 번 누를 때 이동량은 페이지 높이의 2% 내외. **길게 눌러 가속 같은 건
  넣지 마십시오** (구현이 불안정합니다)
- 메뉴 버튼으로 형광펜 모드 종료
- 형광펜 모드에서는 페이지 이동 금지 (방향키가 상자 전용이므로).
  페이지를 넘기려면 모드를 나가게 하십시오

### 3.3 색상

노랑 / 초록 / 파랑 / 분홍 4색. 형광펜 모드 하단에 색 선택 줄을 두고,
마지막 선택을 `UserDefaults.settings` 에 기억하십시오
(`pdfPref.highlightColor`).

### 3.4 기존 하이라이트 지우기

형광펜 모드에서 상자가 기존 하이라이트와 **겹치면**, 하단에
"Select to highlight · Play/Pause ... · ⏭ to delete overlapping" 식으로
안내하고 전용 버튼(예: 건너뛰기 버튼이 없으면 툴바의 지우개 버튼)으로
겹친 것들을 지우게 하십시오.

tvOS 리모컨 버튼이 넉넉하지 않으므로, **지우기는 툴바 버튼으로 빼는 쪽을
권합니다** — 형광펜 모드 툴바에 `eraser` 버튼을 두고, 누르면 현재 상자와
겹치는 하이라이트를 지웁니다.

### 3.5 저장 — 원본은 절대 건드리지 마십시오

```
/private/var/mobile/Documents/Argo/.annotations/<fileHash>.json
```

`fileHash`는 `PDFReaderModel`이 이미 갖고 있는 SHA256을 그대로 씁니다.

```json
{
  "version": 1,
  "sourcePath": "/private/var/mobile/Documents/보고서.pdf",
  "highlights": [
    {
      "id": "UUID 문자열",
      "page": 11,
      "x": 72.0, "y": 540.0, "w": 180.5, "h": 14.0,
      "color": "yellow",
      "createdAt": 1760000000.0
    }
  ]
}
```

- `page`는 **0 기반**
- 좌표는 PDF 포인트 단위, 원점 좌하단
- 문서를 열 때 읽어 오고, 추가·삭제 즉시 저장
- 파일이 깨져 있으면 **무시하고 빈 상태로 시작.** 절대 크래시 금지
- `.annotations` 디렉터리는 앱 Documents 하위이므로 `FileManager`로 직접
  만들 수 있습니다. root 영역이 아니므로 `RootHelperActs` 불필요

신규 파일: `Spartan/FileOpeners/PDFReader/PDFAnnotationStore.swift`

### 3.6 화면에 그리기

렌더된 페이지 이미지 **위에** SwiftUI로 겹쳐 그리십시오. 이미지에 구워넣지
마십시오 (지우기와 색 변경이 불가능해집니다).

```swift
// 개념
ZStack {
    Image(uiImage: pageImage)
    ForEach(highlightsOnThisPage) { h in
        Rectangle()
            .fill(color(h).opacity(0.35))
            .frame(width: screenW(h), height: screenH(h))
            .position(screenCenter(h))
    }
}
```

`.blendMode(.multiply)` 를 쓰면 글자가 비쳐 보여 더 자연스럽습니다.

**주의:** 야간 테마는 전체에 `.colorInvert()`가 걸립니다. 하이라이트도 같이
반전되어 색이 뒤집힙니다. 하이라이트 레이어를 `.colorInvert()` **안쪽**에
두어 함께 반전시키든, 바깥에 두어 원색을 유지하든 **의도적으로 택하고
주석으로 이유를 남기십시오.** 저는 안쪽(함께 반전)을 권합니다 — 야간 모드에서
형광색이 혼자 튀면 눈이 아픕니다.

### 3.7 주석 포함 사본 저장

툴바 또는 설정 패널에 "Save annotated copy" 버튼.

- 원본과 **같은 폴더**에 `원래이름_annotated.pdf` 로 저장
- 같은 이름이 있으면 `_annotated (1).pdf` 식으로 비켜가기
- `CGPDFProbe4.probeWriteWithUIKit` 의 방식을 그대로 쓰십시오:
  페이지마다 `beginPage` → `drawPDFPage` → 하이라이트를 `.multiply`로 덧칠
- **원본 파일을 덮어쓰는 코드가 하나라도 있으면 안 됩니다**
- 저장 중에는 진행 표시, 완료 후 저장 경로를 한 줄로 알림
- 큰 문서는 오래 걸립니다. 백그라운드 큐에서 수행하고 완료 시 메인에서 갱신

원본이 들어 있는 폴더가 쓰기 불가일 수 있습니다. 쓰기에 실패하면
`/private/var/mobile/Documents/Argo/` 로 대체 저장하고 그 경로를 알리십시오.

---

## 4. 작업 2 — 하이라이트 목록

`PDFOutlineView`의 2탭을 **3탭**으로 늘립니다: Outline / Bookmarks / **Highlights**

- 하이라이트를 **쪽 번호 순**으로 나열
- 각 행: 색 점 + "Page N" + 만든 시각
- 행을 고르면 그 쪽으로 점프 (행당 버튼 하나 규칙 준수)
- 목록이 비면 "No highlights yet."

텍스트 스냅이 없으므로 **하이라이트한 글자 내용은 표시할 수 없습니다.**
쪽 번호와 색으로만 구분됩니다. 이게 이번 설계의 한계이고, 그대로 두십시오.

---

## 5. 작업 3 — 검색 (최선노력)

신규 파일: `Spartan/FileOpeners/PDFReader/PDFTextExtractor.swift`

### 5.1 범위를 지키십시오

**하지 말 것:**
- 텍스트 행렬(`Tm`/`Td`) 추적 — 위치가 필요 없으므로 하지 마십시오
- `/ToUnicode` 없는 폰트의 바이트를 추측해 디코딩
- 쪽 안에서 일치 위치 표시
- 복합 폰트(Type0) CMap의 완전한 구현

**할 것:**
- 쪽마다 `Tj` / `TJ` 피연산자 바이트를 모읍니다
- `Tf` 로 현재 폰트 이름을 추적하고, 그 폰트의 `/ToUnicode` CMap으로 변환
- CMap은 `beginbfchar`/`endbfchar`, `beginbfrange`/`endbfrange` 만 해석
- 변환 불가한 바이트는 **버립니다**
- 쪽별 문자열을 합쳐 `[Int: String]` 로 보관

### 5.2 동작

- 검색은 **요청 시점에** 수행. 문서를 열 때 미리 전체를 훑지 마십시오
- 백그라운드 큐에서 쪽을 순서대로 처리하며 진행률 표시
  ("Searching… 42 / 348")
- 중간에 취소 가능 (메뉴 버튼)
- 추출 결과는 문서별로 메모리에 캐시 (같은 문서 재검색 시 재사용)
- 비교는 대소문자 무시 (`localizedCaseInsensitiveContains`)

### 5.3 화면

단일 `.sheet` 분기에 `.search` 케이스를 추가합니다.

- 상단에 `TextField` 하나 (tvOS 키보드 입력이 불편하므로 **최근 검색어 5개**를
  버튼으로 두어 재사용 가능하게)
- 결과: "Page N" 목록. 선택하면 그 쪽으로 점프하고 시트 닫기
- 결과 없음: "No matches found."
- **텍스트를 한 글자도 못 뽑았을 때**는 다르게 안내:
  "No readable text in this PDF. It may be a scan, or its fonts carry no
  Unicode mapping."

### 5.4 최근 검색어

`UserDefaults.settings` 의 `pdfSearchHistory` 에 `[String]` 로 최대 5개.
문서 공통(문서별 아님)으로 둡니다.

---

## 6. 툴바 정리

상단 툴바 버튼이 이미 9개입니다. 여기에 형광펜·검색이 더해지면 11개로,
TV 화면에서 좌우 포커스 이동이 번거로워집니다.

**툴바를 두 줄로 나누십시오.**

| 줄 | 버튼 |
|---|---|
| 1 (탐색) | 이전쪽 / 다음쪽 / 목차·북마크·하이라이트 / 썸네일 / 검색 |
| 2 (보기·편집) | 보기모드 / 테마 / 확대 / 형광펜 / 북마크토글 / 닫기 |

형광펜 모드에 들어가면 **전용 툴바**로 바꾸십시오:
색 4개 / 지우개 / 주석사본저장 / 모드종료

---

## 7. 유지할 제약

`GEMINI_TASK_SPEC.md` 0절 전부. 특히:

- 새 외부 의존성 금지
- tvOS 16.0 API만. `CGPDFProbe4.swift`에 없는 API는 "확신이 없는 부분"에 명기
- 행당 포커스 버튼 하나, 뷰당 `.sheet` 하나
- **원본 PDF 수정 금지** (사본 저장만)
- `PDFPageRenderer`의 직렬 큐 구조 유지
- 큰 문서에서 메인 스레드를 막지 말 것

---

## 8. 제출 범위

```
Spartan/FileOpeners/PDFReader/
├── PDFAnnotationStore.swift   // 신규 — 사이드카 JSON 입출력
├── PDFHighlightOverlay.swift  // 신규 — 선택 상자 UI + 좌표 변환
├── PDFTextExtractor.swift     // 신규 — 콘텐츠 스트림 스캔 + ToUnicode
├── PDFAnnotatedExporter.swift // 신규 — 주석 포함 사본 쓰기
├── PDFReaderModel.swift       // 수정 — 하이라이트 상태, 검색 상태
├── PDFReaderView.swift        // 수정 — 두 줄 툴바, 형광펜 모드, 검색 시트
└── PDFOutlineView.swift       // 수정 — 3탭
```

`ContentView.swift`와 `FileInfo.swift`는 **손대지 마십시오.**
`showSubView` 크기도 그대로 38입니다.

결과물 마지막에 **"CGPDFProbe4.swift 삭제 요청"** 을 적어 주십시오.

`GEMINI_TASK_SPEC.md` 4.1절 제출 형식과 4.2절 자가 점검 목록을 지키십시오.
`PDFReaderModel` / `PDFReaderView` / `PDFOutlineView` 는 수정본 **전체**를
주십시오 (치환 블록 아님).
