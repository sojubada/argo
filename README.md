# Argo

애플TV(tvOS 16, TrollStore)용 파일 탐색기 겸 문서·미디어 뷰어.
[Spartan](https://github.com/WhitetailAni/Spartan)을 바탕으로, 리모컨과 키보드만으로 쓰는 "TV 속 작은 PC"를 목표로 만들었습니다.

*A file browser and document/media viewer for Apple TV (tvOS 16, TrollStore), forked from Spartan.*

## 주요 기능

| 분야 | 내용 |
|---|---|
| 파일 탐색 | 두 칸 탐색기, 정렬·걸러 보기, 복사·이동·휴지통, 시스템 폴더 보호, 앱 컨테이너 폴더에 앱 이름 표시 |
| 키보드 | 블루투스 키보드 단축키(⌫ 상위 폴더, ⌘⌫ 최상위, Delete 휴지통 등), 한글 입력 |
| 원격 | WebDAV·FTP 접속, 내려받아 열기, NAS의 IPA를 TrollStore로 바로 설치 |
| 문서 | PDF(두 쪽 보기·야간 모드), 한글 hwp·hwpx, 워드 doc·docx, 엑셀 xls·xlsx, 파워포인트 ppt·pptx, 텍스트·코드 편집 |
| 글꼴 | 폰트 폴더의 글꼴로 문서를 그리고, 없으면 가장 비슷한 글꼴로 대신함 |
| 음악 | 애플 뮤직풍 화면, 가사(LRC) 따라가기, 앨범 표지 찾기, 스펙트럼 막대, 사진 슬라이드쇼 |
| 동영상 | 애플TV 기본 재생 화면, 리모컨·키보드 조작 |
| 꾸미기 | 테마 6종과 앱 아이콘 |

문서는 앱 안의 Rust 변환기(`rust/argoconv`)가 쪽마다 SVG를 만든 뒤 PDF로 묶고, 이 PDF를 CoreGraphics로 그립니다.
tvOS에는 PDFKit·WebKit·QuickLook이 없어서 이렇게 만들었습니다.

## 설치

1. [Releases](../../releases)에서 IPA를 받습니다(서명 없음).
   - `Argo-Safe.ipa`: 안전판. 안전 모드가 늘 켜져 있고, 잠긴 항목이 든 폴더는 지울 수 없습니다.
   - `Argo-Adventurer.ipa`: 모험가판. 안전 모드를 끌 수 있습니다.
2. TrollStore로 설치합니다.

빌드는 GitHub Actions(macOS 15, Xcode 16)에서 합니다. 바뀐 내용과 되돌리는 방법은 [`docs/RELEASE_2026-10-30.md`](docs/RELEASE_2026-10-30.md)에 있습니다.

## 원본 소스

| 소스 | 쓰인 곳 | 라이선스 |
|---|---|---|
| [WhitetailAni/Spartan](https://github.com/WhitetailAni/Spartan) | 앱의 바탕(파일 관리자) | MIT |
| [WhitetailAni/PrivateKits-tvOS](https://github.com/WhitetailAni/PrivateKits-tvOS) | 설치된 앱 목록(ApplicationsWrapper) | 저장소 참조 |
| [marmelroy/Zip](https://github.com/marmelroy/Zip) | 압축 풀기 | MIT |
| [opa334/TrollStore](https://github.com/opa334/TrollStore) | 루트 실행 도우미(TSUtil) | MIT |
| Pogo (Amy While) | 명령 실행(CommandRunner) | 원저작자 표기 |
| SwiftTryCatch (William Falcon) | Objective-C 예외 처리 | MIT |
| [edwardkim/rhwp](https://github.com/edwardkim/rhwp) | 한글 hwp·hwpx 렌더링, EMF·WMF 변환 | MIT |
| [Indosaram/hwpers](https://github.com/Indosaram/hwpers) | 한글 예비 엔진 | MIT / Apache-2.0 |
| [mini-software/MiniPdf](https://github.com/mini-software/MiniPdf) | 워드·엑셀 → PDF | Apache-2.0 |
| [resvg 계열](https://github.com/linebender/resvg)(usvg, svg2pdf, pdf-writer, ttf-parser, rustybuzz) | SVG·PDF·글꼴 처리 | MIT / Apache-2.0 |
| ECMA-376 presetShapeDefinitions | 파워포인트 도형 정의 | ECMA 표준 |
| [iTunes Search API](https://performance-partners.apple.com/search-api), [LRCLIB](https://lrclib.net) | 앨범 표지·가사 찾기(끌 수 있음) | 각 서비스 약관 |

파워포인트 렌더러(`rust/argoconv/src/slides`)와 tvOS 쪽 화면은 이 저장소에서 새로 작성했습니다.
벤더링한 소스의 라이선스 표기는 각 폴더(`rust/vendor/*`)의 LICENSE와 Cargo.toml에 그대로 두었습니다.

## 라이선스

MIT. 원본 Spartan © 2023 WhitetailAni. 벤더링한 구성 요소는 각자의 라이선스를 따릅니다.
