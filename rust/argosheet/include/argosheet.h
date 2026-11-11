#ifndef ARGOSHEET_H
#define ARGOSHEET_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ASBook ASBook;

typedef enum ASSheetState {
    AS_SHEET_VISIBLE     = 0,
    AS_SHEET_HIDDEN      = 1,
    AS_SHEET_VERY_HIDDEN = 2,
} ASSheetState;

typedef struct ASSheetInfo {
    char     name[256];            // 시트 이름 (UTF-8 null-terminated)
    uint32_t sheet_id;             // workbook.xml sheetId
    uint8_t  state;                // ASSheetState
    uint32_t row_count;            // 최대 행 번호 (1-based 기준)
    uint32_t col_count;            // 최대 열 번호 (1-based 기준)
    uint32_t cell_count;           // 기록된 셀 수
    uint32_t formula_count;        // 수식이 있는 셀 수
    uint32_t merge_count;          // 병합 영역 수
    uint32_t validation_count;     // 데이터 유효성 규칙 수
    uint32_t comment_count;        // 메모 수
    bool     is_frozen;            // 틀 고정 여부
    uint32_t freeze_rows;          // ySplit
    uint32_t freeze_cols;          // xSplit
    uint32_t top_left_row;         // 1-based 행
    uint32_t top_left_col;         // 1-based 열
    bool     show_grid_lines;      // 격자선 표시 여부
    float    total_width_pt;       // 전체 열 너비 합 (엑셀 pt)
    float    total_height_pt;      // 전체 행 높이 합 (엑셀 pt)
} ASSheetInfo;

const char* as_last_note(void);
ASBook*     as_open(const char* path);
void        as_close(ASBook* book);
uint32_t    as_sheet_count(const ASBook* book);
bool        as_sheet_info(const ASBook* book, uint32_t index, ASSheetInfo* out);

#ifdef __cplusplus
}
#endif

#endif /* ARGOSHEET_H */

