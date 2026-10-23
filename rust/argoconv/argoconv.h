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

/// 폰트를 등록한다. tvOS 에는 fontconfig 가 없어 직접 먹여야 한다.
int argoconv_register_font(const char *name, const uint8_t *data, size_t len);

/// docx · xlsx · pptx -> PDF. 성공하면 out 버퍼를 argoconv_free 로 돌려줄 것.
int argoconv_to_pdf(const uint8_t *input, size_t len,
                    uint8_t **out, size_t *out_len);

/// hwp · hwpx -> PDF. 성공하면 out 버퍼를 argoconv_free 로 돌려줄 것.
int argoconv_hwp_to_pdf(const uint8_t *input, size_t len,
                        uint8_t **out, size_t *out_len);

/// argoconv_to_pdf 가 준 버퍼를 돌려준다.
void argoconv_free(uint8_t *ptr, size_t len);

#endif
