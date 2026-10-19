# 3단계 재작업 지침 — PDFKit → Core Graphics

## 1. 무슨 일이 있었나

제출하신 3단계 코드는 빌드에서 **딱 한 줄**로 실패했습니다.

```
Spartan/FileOpeners/PDFReader/PDFReaderModel.swift:7:8:
error: no such module 'PDFKit'
```

**tvOS SDK에는 PDFKit 프레임워크가 없습니다.** Apple 문서의 가용성 표기에는
`tvOS: 11.0.0 -` 로 적혀 있지만 실제 SDK(appletvos 17.5)에는 모듈이 존재하지
않습니다. 이것은 귀하의 잘못이 아니라 **사양서를 쓴 쪽(저)의 착오**입니다.
문서 표기만 믿고 "PDFKit은 tvOS에서 쓸 수 있다"고 적어 드렸습니다.

PDFKit 기반 4개 파일은 되돌렸고, main은 현재 정상 빌드 상태입니다.

## 2. 대안은 검증을 마쳤습니다

Core Graphics의 PDF API로 대체합니다. **추측이 아닙니다.** 아래 모든 심볼을
담은 탐침 파일을 실제 CI에서 컴파일시켜 통과를 확인했습니다.

| 기능 | 검증된 API |
|---|---|
| 문서 열기 | `CGPDFDocument(url as CFURL)` |
| 페이지 수 | `doc.numberOfPages` (1부터 시작) |
| 페이지 | `doc.page(at: Int)` → `CGPDFPage` |
| 암호 | `doc.isEncrypted`, `doc.isUnlocked`, `doc.unlockWithPassword(UnsafePointer<CChar>)` |
| 권한 | `doc.allowsCopying`, `doc.allowsPrinting` |
| 크기 | `page.getBoxRect(.cropBox)` / `.mediaBox` |
| 회전 | `page.rotationAngle`, `page.pageNumber` |
| 변환 | `page.getDrawingTransform(_:rect:rotate:preserveAspectRatio:)` |
| 렌더 | `CGContext.drawPDFPage(_:)` + `UIGraphicsImageRenderer` |
| 목차 원본 | `doc.catalog`, `CGPDFDictionaryGetDictionary/GetArray/GetString`, `CGPDFStringCopyTextString`, `CGPDFArrayGetDictionary`, `CGPDFArrayGetCount` |

**검증 파일은 저장소의 `Spartan/FileOpeners/PDFReader/CGPDFProbe.swift` 에 있습니다.**
컴파일이 보장된 호출 형태가 그대로 들어 있으니 **반드시 먼저 읽고 그 문법을
그대로 따르십시오.** 특히 `CGPDFDictionaryGet*` 계열의 `inout` 포인터 사용법과
PDF 좌표계 뒤집기(원점이 좌하단)는 그 파일 그대로 쓰시면 됩니다.

재작업이 끝나면 `CGPDFProbe.swift`는 삭제 대상입니다. 결과물 마지막에
"CGPDFProbe.swift 삭제 요청"이라고 적어 주십시오.

## 3. 무엇이 바뀌고 무엇이 그대로인가

### 그대로 쓸 수 있는 것

지난 제출물의 **구조와 UI는 대부분 살릴 수 있습니다.** 재사용하십시오.

- `PDFReaderView`의 화면 구성 — 상단 툴바, 하단 진행바, 단일 `.sheet(item:)`
  분기, `.onExitCommand` 단계적 뒤로가기, `remoteControlCatcher`의
  `.onMoveCommand` / `.onPlayPauseCommand` 처리
- `PDFReaderModel`의 상태 설계 — 보기 모드/맞춤/테마/밝기 열거형,
  읽던 위치·북마크 저장(`UserDefaults.settings`, SHA256 키), 환경설정 저장
- `PDFOutlineView`의 목차·북마크 2탭, 썸네일 격자
- 테마 적용 방식 — SwiftUI의 `.colorInvert()` / `.colorMultiply()` /
  `.brightness()`는 렌더된 `Image`에도 똑같이 걸립니다

### 바뀌어야 하는 것

| 항목 | 기존(PDFKit) | 변경 |
|---|---|---|
| 렌더링 | `PDFView` + `UIViewRepresentable` | 페이지를 `UIImage`로 직접 래스터라이즈해 SwiftUI `Image`로 표시 |
| 페이지 인덱스 | 0부터 | `CGPDFDocument`는 **1부터**. 내부 상태는 0 기반으로 유지하고 `doc.page(at: index + 1)`로 변환할 것 |
| 페이지 전환 알림 | `PDFViewPageChanged` 노티 | 불필요. 상태를 우리가 소유하므로 `PDFKitContainer`와 `Coordinator` 전부 삭제 |
| 확대/패닝 | 내부 `UIScrollView` 탐색 | 더 높은 배율로 렌더한 뒤 `.scaleEffect` + `.offset`로 이동. `findScrollView` 삭제 |
| 맞춤 | `autoScales`, `scaleFactorForSizeToFit` | 렌더 시 목표 크기 계산으로 대체 |
| 목차 | `PDFOutline`, `PDFActionGoTo` | 카탈로그 딕셔너리 직접 순회 (탐침 파일 참조) |
| 썸네일 | `page.thumbnail(of:for:)` | 작은 목표 크기로 동일 렌더 함수 재사용 |
| 암호 | `doc.unlock(withPassword: String)` | `password.withCString { doc.unlockWithPassword($0) }` |

## 4. 렌더링 설계 (중요)

TV는 메모리가 넉넉하지 않고, PDF는 수백 MB일 수 있습니다.

1. **렌더는 백그라운드에서.** `DispatchQueue.global(qos: .userInitiated)`에서
   `UIImage`를 만들고 `DispatchQueue.main`에서 상태에 반영하십시오.
2. **현재 페이지 ±1 정도만 캐시.** `NSCache<NSNumber, UIImage>`에 담되
   `countLimit`을 5 내외로 두십시오. 전 페이지를 미리 렌더하지 마십시오.
3. **렌더 취소.** 페이지를 빠르게 넘길 때 이전 렌더가 뒤늦게 도착해 화면이
   튀지 않도록, 요청마다 증가하는 토큰을 두고 완료 시 현재 토큰과 비교해
   어긋나면 버리십시오.
4. **렌더 해상도**는 화면 크기 기준(1920×1080 안쪽)으로 제한하십시오.
   확대 모드에서만 배율을 곱해 다시 렌더하되, 최대 변 길이를 4096으로
   제한해 거대한 비트맵을 만들지 마십시오.
5. **로딩 중 표시.** 첫 렌더가 끝나기 전에는 `ProgressView`를 보이십시오.

## 5. 보기 모드 구현

| 모드 | 구현 |
|---|---|
| 한 쪽 | 현재 페이지 한 장을 렌더해 표시. 좌우로 페이지 이동 |
| 두 쪽 (책) | 두 페이지를 각각 렌더해 `HStack`에 나란히. 좌우 이동은 2쪽 단위 |
| 연속 스크롤 | `ScrollView` + `LazyVStack`으로 페이지 셀을 쌓되, **각 셀이 화면에 들어올 때 렌더**하고 벗어나면 해제. `ScrollViewReader`로 페이지 점프 |

연속 모드가 메모리상 가장 위험합니다. 셀마다 `.onAppear`에서 렌더하고
`.onDisappear`에서 이미지를 `nil`로 비우십시오.

## 6. 목차(Outline) 구현

`CGPDFProbe.swift`의 `probeOutline` / `walk`가 뼈대입니다. 추가로 필요한 것:

**목적지 → 페이지 번호 변환.** `/Dest` 배열의 첫 원소는 페이지 딕셔너리
참조입니다. 페이지 번호를 얻으려면 문서의 모든 페이지를 한 번 훑어
`page.dictionary`와 포인터 비교로 맵을 만들어 두십시오.

```swift
// 문서를 연 직후 1회만
var pageMap: [UnsafeRawPointer: Int] = [:]
for i in 1...doc.numberOfPages {
    if let p = doc.page(at: i), let d = p.dictionary {
        pageMap[UnsafeRawPointer(d)] = i - 1   // 0 기반으로 저장
    }
}
```

**이름 기반 목적지**(`/Dest`가 배열이 아니라 이름인 경우)는 `/Names` → `/Dests`
트리를 더 타야 합니다. **이번 단계에서는 구현하지 마십시오.** 해석에 실패한
항목은 **목록에서 빼십시오.** 0쪽으로 보내면 사용자가 더 혼란스럽습니다.

목차가 하나도 해석되지 않으면 "This PDF has no table of contents."를 그대로
보여 주시면 됩니다.

## 7. 그 외 유지할 제약

0절~0.5절의 모든 제약이 그대로 적용됩니다. 특히:

- 새 외부 의존성 금지
- **이번에는 "되는 줄 알았다"가 반복되면 안 됩니다.** `CGPDFProbe.swift`에
  없는 API를 쓰려거든, 쓰기 전에 "확신이 없는 부분"에 반드시 적으십시오
- 행당 포커스 버튼 하나, 뷰당 `.sheet` 하나
- `showSubView` 크기는 이미 38, 인덱스 36이 PDF용으로 비어 있음
- 원본 PDF를 수정하는 코드 금지 (이번 단계는 읽기 전용)

## 8. 이미 반영되어 있는 것 (다시 하지 마십시오)

지난 제출물 중 **PDFKit과 무관한 부분은 이미 적용되어 main에 있습니다.**

- `FileInfo.swift`의 `isPDF(filePath:)` 와 타입값 15 판정
  - 단, **위치를 옮겼습니다.** `isHTML` 직후, `isExecutableFile`/`isArchive`
    **앞**에 두었습니다. 이유: `isArchive`가 `fileManager.contents(atPath:)`로
    파일 전체를 메모리에 올리기 때문에, 그 뒤에 두면 선두 5바이트만 읽는
    최적화가 무의미해집니다. 또 실행 비트가 선 PDF가 executable로 오분류됩니다
  - 확장자 배열은 `[".pdf"]` 하나만 둡니다. 기존 `doesFileHaveFileExtension`은
    루프 첫 원소에서 `return` 해 버려 **두 번째 이후 확장자를 검사하지 않습니다**
    (상류 버그). 대문자 `.PDF`는 매직 바이트 쪽에서 잡힙니다
- `ContentView.swift`의 `case 15` 세 지점
  - `defaultAction`의 `switch fileType` → `showSubView[36] = true`
  - 파일 아이콘 `switch` **두 곳** (목록 / tvOS13 메뉴) → `doc.richtext`
    (지난번에 한 곳분만 주셨는데, 동일 블록이 두 곳에 있어 양쪽 모두 적용했습니다)
  - `.sheet(isPresented: $showSubView[36])` → `PDFReaderView(filePath:fileName:isPresented:)`

**따라서 `ContentView.swift`와 `FileInfo.swift`는 더 손댈 필요가 없습니다.**
`PDFReaderView`의 생성자 시그니처만 `(filePath: String, fileName: String,
isPresented: Binding<Bool>)` 로 유지해 주시면 그대로 맞물립니다.

`filePath`에는 **디렉터리 경로**(끝에 `/`)가, `fileName`에는 파일명이 들어옵니다.
지난 제출물의 `init`에서 둘을 합치던 처리를 그대로 두시면 됩니다.

## 9. 제출 범위

```
Spartan/FileOpeners/PDFReader/
├── PDFReaderModel.swift      // CGPDFDocument 기반으로 재작성
├── PDFPageRenderer.swift     // 신규 — 렌더링, 캐시, 취소 토큰
├── PDFReaderView.swift       // 화면 구성은 유지, 렌더 표시부만 교체
├── PDFOutlineView.swift      // 목차/북마크/썸네일 — 썸네일 생성부만 교체
└── (PDFKitContainer.swift 는 삭제 — 더 이상 필요 없음)
```

4.1절 제출 형식과 4.2절 자가 점검 목록을 그대로 지켜 주십시오.
