# Argo 인수인계

Apple TV(tvOS 16, TrollStore)용 파일 탐색기 **Argo**. WhitetailAni의 Spartan에서 갈라져 나왔습니다 (MIT).

문서 넷으로 나뉩니다.

| 문서 | 내용 |
|---|---|
| **HANDOVER.md** (이 문서) | 지금까지 만든 것, 겪은 실패, 반드시 지킬 제약 |
| `ROADMAP.md` | 앞으로 만들 것, 참조 라이브러리 판정 |
| `GEMINI_P1_SAFETY.md` | 1단계 지시서 — 안전장치 |
| `GEMINI_P2_BROWSER.md` | 2단계 지시서 — 두 칸 탐색기·키보드 |
| `GEMINI_P3_VIEWER.md` | 3단계 지시서 — 문서 뷰어 통일 |

---

## 1. 지금 있는 것

소스 약 14,500줄. 저장소 `sojubada/argo`, 브랜치 `main`.

### 1-1. 핵심

| 파일 | 줄 | 하는 일 |
|---|---|---|
| `Spartan/ContentView.swift` | 1,880 | 파일 목록, 툴바, 모든 뷰어 디스패치 |
| `Spartan/FileInfo.swift` | ~300 | 파일 종류 판정 (내용 기반) |
| `Spartan/SpareViews.swift` | 473 | 공용 뷰 |
| `Spartan/SpartanApp.swift` | 116 | 진입점 |

### 1-2. 문서 뷰어 (WebKit 기반, 새로 만든 것)

| 파일 | 줄 | 하는 일 |
|---|---|---|
| `FileOpeners/OfficeReader/WebDocumentView.m` | 440 | 뷰컨트롤러 — 툴바, 리모컨, 웹뷰 수명 |
| `FileOpeners/OfficeReader/WebDocumentShell.m` | 232 | 형식별 HTML·JS 생성, `argoNav` 정의 |
| `FileOpeners/OfficeReader/WebDocumentSwiftView.swift` | 32 | SwiftUI 연결 |
| `Spartan/webdoc/` | 3.9MB | 번들된 JS 렌더러 6종 |

`webdoc/` 내용:

| 파일 | 패키지 | 라이선스 | 전역 이름 |
|---|---|---|---|
| `jszip.min.js` | jszip 3.10.2 | MIT | `JSZip` |
| `docx-preview.min.js` | docx-preview 0.4.1 | Apache-2.0 | `docx` |
| `xlsx.min.js` | SheetJS 0.18.5 | Apache-2.0 | `XLSX` |
| `pptx.min.js` | pptx-preview 1.0.7 | ISC (**소스 비공개**) | `pptxPreview` |
| `hwp.min.js` | hwp.js 0.0.3 | Apache-2.0 | `hwpjs` |
| `pdf.min.js` + `pdf.worker.min.js` | pdfjs-dist 3.11.174 (legacy) | Apache-2.0 | `pdfjsLib` |

### 1-3. 기존 기능

| 폴더 | 내용 |
|---|---|
| `Trash/` | 휴지통 — 이동·복원·영구삭제 (877줄) |
| `WebDAV/` | NAS 접속, IPA 설치 (1,456줄) |
| `Favorites/` | 즐겨찾기 |
| `PDFReader/` | **구버전 Core Graphics PDF 뷰어 — 지금은 안 쓰임. 삭제 대상** (857줄) |
| `PlistEditor/` | plist 편집기 (1,400줄) |
| `FileOpeners/` | 텍스트·이미지·영상·폰트·헥스·deb 뷰어 |
| `Diagnostics/` | **탐침들 — 역할 끝남. 삭제 대상** |

### 1-4. 삭제해도 되는 것

역할이 끝났습니다. 지우면 앱이 가벼워집니다.

```
Spartan/FileOpeners/PDFReader/          (857줄, PDF.js로 대체됨)
Spartan/FileOpeners/OfficeReader/OfficeDocReaderView.swift  (512줄, 옛 글자 목록)
Spartan/Diagnostics/DocPipelineProbe.swift
Spartan/Diagnostics/KeyboardProbe.swift
Spartan/Diagnostics/WebKitProbe.{h,m}   + WebKitProbeView.swift
Spartan/Diagnostics/WebDocProbe.{h,m}   + WebDocProbeView.swift
SettingsView.swift 의 Lab ①② 항목
```

지울 때 `project.pbxproj` 등록도 같이 빼야 합니다.

---

## 2. 성공과 실패 — 반드시 읽을 것

여기 적힌 것은 **전부 실제로 겪은 일**입니다. 같은 실수를 되풀이하지 않는 것이 이 문서의 목적입니다.

### 2-1. 가장 비쌌던 실패

**PDFKit은 tvOS에 없습니다.** 애플 공식 문서에는 "tvOS 11.0+"로 나옵니다. SDK에 실물이 없습니다. 이걸 믿고 3단계 전체를 설계했다가 전부 버렸습니다.

**교훈: 문서를 믿지 말고 SDK에 물어보십시오.** 그러라고 만들어 둔 것이 있습니다.

> Actions 탭 → **tvOS SDK inventory** 워크플로 → `symbols` 란에 이름을 쉼표로 넣고 실행
> → PRESENT / ABSENT 가 헤더를 근거로 나옵니다

헤더에 이름이 있다고 끝이 아닙니다. `UIKeyCommand` 는 있지만 **iOS 13의 `title:` 생성자는 없습니다**. 그래서 쓸 코드를 그대로 호출하는 **컴파일 전용 탐침**을 CI에 올려 초록불을 본 뒤에 설계하십시오.

### 2-2. 지금까지 확인된 사실

**없는 것** — PDFKit, WebKit(SDK), QuickLook, SafariServices, Compression
**있는 것(SDK)** — CoreText, CoreGraphics, UIKit, AVFoundation, libarchive, libxml2, libz, UIKeyCommand, UIKey, UIKeyboardHIDUsage, pressesBegan, UIFocusGuide, UIGraphicsPDFRenderer, CGPDFDocument

**결정적 발견 — 기기에는 WebKit이 있습니다.**
SDK에는 없지만 `/System/Library/Frameworks/WebKit.framework` 가 실기기에 있고, `dlopen` 으로 열면 `WKWebView` 가 **완전히 동작합니다.** 페이지 로드, 자바스크립트, 실제 픽셀 렌더링, `WebContent`·`GPU` XPC 프로세스 전부 정상. 한글도 제대로 나옵니다.

AirDrop 작업에서 `Sharing.framework` 를 dlopen 으로 쓴 것과 같은 방식입니다.

**헤더가 없으므로 모든 WebKit 호출은 런타임 경유입니다.** `NSClassFromString` + `objc_msgSend` / `performSelector`. 컴파일 시점에 WebKit 심볼을 참조하면 링크가 깨집니다. `WebDocumentView.m` 이 그 본보기입니다.

### 2-3. tvOS UI 함정

| 함정 | 증상 | 해법 |
|---|---|---|
| 한 행에 포커스 버튼 둘 | 뒤쪽 것만 눌림 | 행당 버튼 하나 |
| 한 화면에 `.sheet` 둘 | 안 열림 | 서로 다른 뷰에 붙이기 |
| 뷰컨트롤러에서 `pressesBegan` | **방향키가 안 먹음** | 포커스 가진 뷰에서 받을 것 |
| 포커스 경쟁자 존재 | 방향키가 포커스 이동으로 소모 | 안 쓸 버튼은 `enabled=NO` (비활성은 포커스 못 받음) |
| 툴바 맨 앞에 긴 파일명 | **뒤 버튼들이 화면 밖으로** | 조작 버튼 먼저, 제목 맨 뒤 |
| `@ViewBuilder` 가 홀로 한 줄 | 사이에 함수를 넣으면 속성을 가로챔 | 바로 붙여 쓸 것 |
| 가장자리 5% | TV가 잘라먹음 | `safeAreaLayoutGuide` |

**시청 거리 3미터, 55인치입니다.** 본문 28pt 이상, 로그 Menlo 22pt 이상.

### 2-4. 앱이 켜지자마자 죽는 지점

`ContentView.swift` 의 `showSubView` 배열은 현재 **39개**입니다. 새 시트를 쓰면 **배열 크기부터 키우십시오.** 안 키우면 실행 즉시 인덱스 초과로 죽습니다.

`ContentView.swift` 는 **공백과 탭이 섞여** 있습니다. 교체 블록은 글자 그대로 복사하십시오.

### 2-5. 자바스크립트 쪽 함정

| 함정 | 증상 | 해법 |
|---|---|---|
| Safari의 `error.stack` | **메시지가 없음** (크롬과 다름) | `error.message` 를 따로 읽을 것 |
| 스크립트 최상위 `return` | **구문 오류 → 블록 전체가 안 돌아감, 조용히** | 함수로 감쌀 것 |
| `file://` 페이지의 `fetch` | 형제 파일을 못 읽음 | `allowFileAccessFromFileURLs` 설정, 또는 base64 로 싣기 |
| 디버그 CSS의 `color` | **흰 바탕에 흰 글씨 = 백지로 보임** | 문서 컨테이너에 색이 새지 않게 |
| ObjC 리터럴의 `\u0001` | 컴파일 오류 | 제어문자는 UCN 으로 못 씀 |

### 2-6. 제미나이가 전에 틀렸던 것

- **"확장자 검사 버그 때문에 그림·음성이 안 열린다"** — 틀렸습니다. `isImage`는 `UIImage(contentsOfFile:)`, `isVideo`/`isAudio`는 `AVAsset` 트랙으로 **내용을 봅니다.** 확장자와 무관합니다
- **오피스 판정을 `isText` 앞에 두라** — 그 자리면 안 됩니다. OOXML은 ZIP이라 앞에 있는 `isArchive`(504B)가 먼저 가로챕니다. **`isArchive` 보다 앞**이어야 합니다
- **`defer { isLoading = false }` + 함수 안 `save()`** — defer는 함수 끝에 돌아서 `guard !isLoading` 에 막힙니다. 저장이 조용히 안 됐습니다
- **렌더 큐를 concurrent 로** — 취소 토큰이 없어 빠르게 넘기면 무한정 쌓입니다. serial 로 고쳤습니다

### 2-7. 성공한 것 (그대로 두십시오)

- **진단 화면 방식** — 기능마다 점검 항목을 버튼으로 돌리고 로그로 확인. AirDrop 때도 이 방식이 통했고, WebKit·문서 렌더링도 이것으로 뚫었습니다
- **탐침 먼저, 설계 나중** — 컴파일 전용 파일을 CI에 올려 확인한 뒤 짜기
- **숫자로 판정** — "백지로 보인다"로는 원인을 못 가립니다. 색 가짓수, 요소 개수, 평균 밝기, 계산된 스타일을 찍으면 갈립니다
- **docx 0.85초 / xlsx 0.17초 / hwp 0.24초 / pptx 2.1초** — 형광펜까지 원본 그대로

---

## 3. 빌드

- tvOS 16.0 타깃, Xcode 15.4 / tvOS 17.5 SDK, GitHub Actions `macos-14`
- **로컬 Xcode 없음.** 빌드는 CI에서만
- `@retroactive` 금지 (Xcode 15가 모름)
- 새 파일은 `project.pbxproj` 등록 필요 — **파일 목록을 따로 알려 주십시오**
- 릴리즈는 `workflow_dispatch` 의 `make_release` 로만. 푸시는 컴파일만 확인

---

## 4. 제출 형식

1. **새 파일은 전체**, 기존 파일은 **`바꾸기 전` / `바꾼 후` 짝**
2. `바꾸기 전` 은 현재 파일에 **딱 한 번만** 나오도록 앞뒤를 충분히. `ContentView.swift` 는 같은 블록이 두 군데 나오는 경우가 있으니 몇 번째인지 적을 것
3. **새 파일 경로 목록**을 따로
4. **자가 점검** — `showSubView` 크기를 건드렸는가 / 한 행에 포커스 버튼이 둘인 곳 / 한 화면에 `.sheet` 둘 / tvOS에 없는 API / 루프 종료 조건
5. **확신이 안 서는 부분은 짐작으로 메우지 말고 그대로 적을 것.** 실제 SDK로 확인해 드립니다
