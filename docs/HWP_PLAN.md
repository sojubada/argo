# 한글 문서(hwp · hwpx) 읽기·쓰기 계획

검토: 2026-10-29. 근거: [claw-hwp](https://github.com/DoHyun468/claw-hwp) (MIT),
그 바닥의 [rhwp](https://github.com/edwardkim/rhwp) (MIT, Rust).

> 문서 뷰어 단계에 속한다. 뷰어는 보류 중이라 **지시가 있을 때 1단계부터** 시작한다.

## 지금

`rust/argoconv` 가 `hwpers` 로 hwp·hwpx 를 쪽마다 SVG 로 그려 PDF 로 바꾸고,
네이티브 PDF 뷰어가 보여 준다. 읽기만 된다.

## 왜 rhwp 인가

- Rust 라이브러리다. `bindings/Native` 가 staticlib 로 짜여 있어 argoconv 처럼
  aarch64-apple-tvos 로 직접 빌드할 수 있다. 웹 엔진·WASM 이 필요 없다.
- 라이브러리 안에 PDF 내보내기가 있다 (`rhwp::renderer::pdf`, SVG 호환 백엔드 —
  Skia 없이). 지금의 "PDF → 뷰어" 길을 그대로 쓴다.
- 편집 API 가 있다: 글 넣기·지우기·찾아 바꾸기, 표 칸 글, 누름틀 값,
  `exportHwp` / `exportHwpx`.
- 테스트 3,400개 이상. hwpers 보다 표·글상자·수식·차트가 정확할 것으로 본다.

## 함정 (claw-hwp 가 찾아 둔 것)

- rhwp 의 저장은 **큰 hwp(50쪽 이상)를 한컴에서 안 깨지게 다시 쓰지 못한다.**
  claw-hwp 는 그래서 원본 바이트를 두고 바뀐 레코드만 고친다
  (hwp: `cell-patch.js`, hwpx: `hwpx-edit.js` 의 XML 직접 편집).
- 한컴이 거부하는 모양들이 `references/hwp-internals.md` 에 정리돼 있다.
  빈 칸은 PARA_TEXT 없는 빈 문단, 0x80000000 표시는 마지막 문단에만,
  그림을 지우면 BinData 번호를 다시 매긴다, 등.
- 이 코드는 Node.js 라 그대로 쓰지 않는다. **지식만** 옮긴다.

## 단계

1. **읽기 교체** — argoconv 의 hwpers 를 rhwp 로 바꾼다. hwpers 는 한동안
   실패 시 대체로 남긴다.
   - 확인할 것: tvOS 시스템 글꼴(한글)을 PDF 그리기에서 찾는지, 앱 크기(+5~10MB 예상),
     빌드 시간(rhwp 는 Rust 1.93 요구 — CI 는 nightly).
2. **쓰기 1차** — 찾아 바꾸기 · 표 칸 글 고치기 · 누름틀 채우기.
   받은 형식 그대로 저장한다 (hwp→hwp, hwpx→hwpx, 바꾸지 않는다).
   - hwpx: ZIP 안 XML 만 고친다 (claw-hwp 방식). 안전하다.
   - hwp: 작은 파일만 rhwp 직렬화로 저장. 큰 파일은 막고 이유를 보인다.
3. **저장 검증** — 저장한 것을 다시 열어 글을 원본·의도와 비교한다. 다르면 원본을 남기고
   새 이름으로 둔다. 안전장치(FileGuard)를 지난다.
4. **쓰기 2차 (나중)** — 큰 hwp 의 바이트 패치. claw-hwp `cell-patch.js` 의 규칙을
   Rust 로 옮긴다. 규모가 커서 따로 판단한다.

## 하지 않는 것

- claw-hwp 의 스크립트를 앱 안에서 돌리지 않는다 (Node·WASM 필요, 외부 코드 실행).
- hwp ↔ hwpx 변환. claw-hwp 도 손실 때문에 뺐다.
