# 제미나이 P4 지시문 (이 파일 전체를 그대로 복사해 붙여 넣는다)

```
너는 Argo(애플TV용 파일 탐색기·문서 뷰어)에 엑셀 셀 단위 뷰어를 붙이는 작업을 한다.
저장소는 sojubada/argo. 아래 규칙은 모든 단계에 적용된다.

[환경]
- Rust 코드는 rust/ 아래. 새 크레이트는 rust/argosheet.
- 빌드 대상은 aarch64-apple-tvos, nightly 툴체인에 -Z build-std. 따라서
  의존성은 순수 Rust 만 쓴다. C 바인딩이 있는 크레이트(libxml2, zlib-sys,
  freetype 등)는 쓰지 않는다. zip·xml·소수 연산도 순수 Rust 로 된 것만.
- 이미 쓰는 것: quick-xml, rust_decimal. 새 의존성을 추가하려면 먼저 이름과
  이유를 적어 물어본다.
- Swift 는 tvOS 16.0 최소, UIKit. SwiftUI 는 쓰지 않는다(포커스가 안 잡힌다).

[재사용 — 새로 짜지 말고 옮겨 쓸 것]
rust/vendor/minipdf/src/xlsx.rs 안의 다음 함수는 이미 실제 문서로 검증됐다.
  - read_part_relationships, resolve_part_target : ZIP 안 부품 관계 읽기
  - read_theme_colors, apply_color_tint          : 테마 색과 밝기 보정(tint)
  - excel_column_width_to_points_with_mdw, excel_max_digit_width : 열 너비 환산
  - rasterize_emf, read_legacy_drawing_images    : EMF·레거시 그림
그대로 argosheet 로 옮기고, 고칠 일이 있으면 무엇을 왜 고쳤는지 적는다.

[쓰지 말 것 — 이미 실패한 길]
- CoreXLSX, XMLCoder : 시트 전체를 Codable 로 올려 큰 시트에서 메모리가 터진다.
- NSExpression       : 문법이 조금만 달라도 ObjC 예외가 나고 Swift 에서 못 잡아
                       앱이 그대로 종료된다. 수식은 직접 파서를 만든다.
- NumberFormatter 로 엑셀 서식 흉내 : 반올림 규칙이 달라 2.5 가 2 로 나온다.
- 셀마다 포커스를 받는 UICollectionView : 포커스 후보가 수백 개가 되고 확대
                       효과가 이웃 테두리를 가린다. 타일 + 셀 커서 하나로 간다.
- 정규식으로 수식 문자열의 셀 참조를 치환 : LOG10 이 셀 참조로 바뀐다.
- 남의 바이너리·스크립트를 내려받아 실행하지 않는다.

[코드 규칙]
- 모든 C ABI 진입점은 catch_unwind 로 감싸고 오류는 코드로 돌려준다. 패닉이
  경계를 넘으면 앱이 죽는다.
- 좌표는 엑셀 pt 하나로만 계산한다(EMU·픽셀·문자 단위를 섞지 않는다).
- 주석은 한국어로, "무엇을" 이 아니라 "왜" 를 적는다.
- 시험 벡터는 코드 안 #[test] 로 넣는다. 시험 없는 계산 코드는 받지 않는다.

[합격 판정]
- 정답 파일 golden_values.json (시트 7개, 수식 1,208칸, LibreOffice 로 재계산)
  과 비교한다. 숫자는 상대오차 1e-9, 글은 완전 일치.
- 보기 전용이다. 셀 값을 바꾸는 길(입력·목록 선택·저장)은 만들지 않는다.

[제출물]
- 바뀐 파일 전체 내용(조각 diff 말고).
- 새 시험 목록과 돌린 결과.
- 못 한 것·의심스러운 것을 끝에 솔직히 적는다. 지어내지 않는다.
```

```
[이번 단계 납품 규칙 — P1~P3 에서 세 번 어겨 새로 정한 것. 어기면 받지 않는다]
1. 바탕은 저장소 sojubada/argo 의 main 최신판이다. 네가 예전에 낸 파일을 다시 쓰지 않는다.
   시작 전에 아래 파일을 저장소에서 읽고, 맨 위에 각 파일의 줄 수를 적어 증명한다.
     rust/argosheet/src/lib.rs, draw.rs, parse.rs, style.rs, calc.rs
     rust/argosheet/include/argosheet.h
     Spartan/FileOpeners/Readers/ArgoSheetViewer.swift
2. 파일을 통째로 내지 않는다. 내는 것은 두 가지뿐이다.
   (가) 새 파일: rust/argosheet/src/cf.rs (조건부 서식 전부를 여기에) — 전체 내용.
   (나) 기존 파일에서 **새로 만들거나 바꾼 함수만**, 함수 하나를 통째로.
        각 함수 위에 "파일 / 들어갈 자리(바로 앞 함수 이름) / 새것인지 바꾼 것인지" 를 적는다.
3. 기존 함수·구조체·필드·시험을 지우거나 이름을 바꾸지 않는다. 시험은 더하기만 한다.
4. C 헤더(argosheet.h)와 ASDrawCmd·ASCellInfo 배치는 **바꾸지 않는다**.
   새 그림이 필요하면 기존 명령(Fill·Line·Text·Marker)과 sub_type 새 번호로 표현한다.
   부득이 바꿔야 하면 코드를 쓰기 전에 먼저 물어본다.
5. Swift 는 바꾸지 않는다. 아이콘(화살표)처럼 Swift 가 새로 그려야 하는 것이 있으면
   Marker 의 sub_type 번호와 모양(좌표 규칙)을 표로 적어만 둔다 — Swift 는 Claude 가 쓴다.
6. 한글이 든 바이트 문자열은 br"…" 가 아니라 r"…".as_bytes().
7. 사용자 파일 값으로 반복 횟수·메모리 크기가 정해지는 곳에는 상한을 둔다
   (예: sqref="A1:XFD1048576" 를 칸마다 펼치지 않는다 — 범위 그대로 들고 검사한다).
8. 보고서 맨 위에 `cargo test -p argosheet` 마지막 20줄을 그대로 붙인다. 돌리지 않았으면
   "돌리지 않음" 이라고 쓴다. 정답은 손으로 적지 않는다.
9. 고정값·자리표시자 금지. 못 한 규칙은 "못 한 것" 에 이름으로 적는다.

[이번 단계에서 손댈 곳]
- parse.rs : 시트 XML 의 <conditionalFormatting sqref=…><cfRule …> 읽기 (지금은 안 읽는다).
             <extLst> 안의 x14:conditionalFormatting(데이터 막대 음수·축)도 읽는다.
- style.rs : dxfs 는 이미 읽는다(DxfStyle). 그대로 쓴다.
- cf.rs    : 규칙 평가(셀 값 → 적용할 dxf / 막대 길이 / 색조 색 / 아이콘 번호).
             수식 규칙(expression)은 calc.rs 의 엔진으로 계산한다(새 파서 금지).
             범위의 min·max·백분위·평균·상위 N 은 범위마다 한 번만 계산해 캐시한다.
- draw.rs  : build_tile_drawlist 안에서 채우기·글자색을 정할 때 cf 결과를 얹는다
             (바꾼 함수 통째로). 데이터 막대는 Fill(그라데이션이면 Gradient),
             아이콘은 Marker 새 sub_type.
```

```
[할 일]
- 데이터 막대: cfvo(min/max/num/percentile), 색, 음수 처리, 축 위치.
  샘플: 00 시트 I10:I15(0~1, #0F766E), 01 시트 N열(min~max, #3B82F6),
        06 시트 J열(0~1, #10B981).
- 색조 3색: 양쪽을 따로 보간한다. 샘플: 01 시트 Q열(#FCA5A5·흰색·#86EFAC),
  04 시트 B6:F10(#F8FAFC·#BAE6FD·#0284C7), 06 시트 G열(min·0·max).
- 아이콘 집합: 3Arrows 부터. 임계값 2개(-15, 15)와 방향 규칙, showValue=1 이면
  숫자도 같이 그린다. 아이콘은 벡터로 그린다(그림 파일 아님).
- cellIs·expression·top10·평균·중복·텍스트·빈칸/오류 규칙.
- 표(table) 스타일, 줄무늬, 머리글·요약 행, 필터 단추는 **표시만** 한다
  (눌러서 거르는 기능은 넣지 않는다 — 보기 전용).

[합격] 01·04·06 시트의 색·막대·아이콘이 LibreOffice 렌더와 눈으로 같다.
```
