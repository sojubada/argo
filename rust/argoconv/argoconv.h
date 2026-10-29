//  argoconv.h
//  Argo — 오피스 문서를 PDF 로 바꾸는 Rust 라이브러리의 창구.
//
//  파일이 아니라 바이트를 주고받는다. 읽고 쓰는 일은 Swift 가 한다.

#ifndef ARGOCONV_H
#define ARGOCONV_H

#include <stddef.h>
#include <stdint.h>

#define ARGOCONV_OK              0
#define ARGOCONV_BAD_ARGS       -1
#define ARGOCONV_CONVERT_FAILED -2
#define ARGOCONV_PANICKED       -3

/// 폰트를 등록한다. tvOS 에는 fontconfig 가 없어 직접 먹여야 한다.
int argoconv_register_font(const char *name, const uint8_t *data, size_t len);

/// docx · xlsx · pptx -> PDF. 성공하면 out 버퍼를 argoconv_free 로 돌려줄 것.
int argoconv_to_pdf(const uint8_t *input, size_t len,
                    uint8_t **out, size_t *out_len);

/// hwp · hwpx -> PDF. 성공하면 out 버퍼를 argoconv_free 로 돌려줄 것.
int argoconv_hwp_to_pdf(const uint8_t *input, size_t len,
                        uint8_t **out, size_t *out_len);

/// 마지막 패닉 메시지를 담는다. 담은 바이트 수를 돌려준다.
size_t argoconv_last_error(uint8_t *buffer, size_t cap);

/// 마지막 변환이 남긴 한 줄(무엇을 봤는지)을 담는다.
size_t argoconv_last_note(uint8_t *buffer, size_t cap);

/// 사용자에게 보여 줄 알림을 담는다. 없으면 0.
size_t argoconv_last_user_notice(uint8_t *buffer, size_t cap);

/// 글꼴 파일의 얼굴들(이름·굵기·한글 여부)을 JSON 으로 담는다.
/// 돌려준 값이 cap 보다 크면 아무것도 안 쓴 것 — 그 크기로 다시 부른다.
size_t argoconv_font_faces(const uint8_t *data, size_t len, uint8_t *out, size_t cap);

/// 문서가 쓰는 글꼴 이름들을 JSON 배열로 담는다. 크기 규칙은 위와 같다.
size_t argoconv_doc_fonts(const uint8_t *data, size_t len, uint8_t *out, size_t cap);

/// 등록한 글꼴과 이름 바꿈을 모두 비운다.
int argoconv_clear_fonts(void);

/// 한글 문서가 requested 글꼴을 원하면 actual 로 그린다.
int argoconv_font_alias(const char *requested, const char *actual);

/// argoconv_to_pdf 가 준 버퍼를 돌려준다.
void argoconv_free(uint8_t *ptr, size_t len);

#endif
