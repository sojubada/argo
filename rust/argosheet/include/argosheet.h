#ifndef ARGOSHEET_H
#define ARGOSHEET_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ASBook ASBook;
typedef struct ASDrawList ASDrawList;

typedef enum ASSheetState {
    AS_SHEET_VISIBLE     = 0,
    AS_SHEET_HIDDEN      = 1,
    AS_SHEET_VERY_HIDDEN = 2,
} ASSheetState;

typedef struct ASSheetInfo {
    char     name[256];
    uint32_t sheet_id;
    uint8_t  state;
    uint32_t row_count;
    uint32_t col_count;
    uint32_t cell_count;
    uint32_t formula_count;
    uint32_t merge_count;
    uint32_t validation_count;
    uint32_t comment_count;
    bool     is_frozen;
    uint32_t freeze_rows;
    uint32_t freeze_cols;
    uint32_t top_left_row;
    uint32_t top_left_col;
    bool     show_grid_lines;
    float    total_width_pt;
    float    total_height_pt;
} ASSheetInfo;

typedef struct ASCellInfo {
    uint32_t row;
    uint32_t col;
    uint32_t merge_start_row; // 병합 안 된 셀은 row와 동일
    uint32_t merge_start_col; // 병합 안 된 셀은 col과 동일
    uint32_t merge_end_row;   // 병합 안 된 셀은 row와 동일
    uint32_t merge_end_col;   // 병합 안 된 셀은 col과 동일
    uint8_t  kind;
    uint8_t  flags;
    double   num_value;
    char     address[32];
    char     display_text[512];
    char     formula[512];
    char     format_code[128];
    char     comment_text[512];
} ASCellInfo;

typedef struct ASHit {
    uint32_t sheet;
    uint32_t row;
    uint32_t col;
} ASHit;

typedef enum ASDrawCmdType {
    AS_CMD_FILL     = 0,
    AS_CMD_GRADIENT = 1,
    AS_CMD_PATTERN  = 2,
    AS_CMD_LINE     = 3,
    AS_CMD_TEXT     = 4,
    AS_CMD_MARKER   = 5,
} ASDrawCmdType;

typedef struct ASRect {
    float x;
    float y;
    float w;
    float h;
} ASRect;

typedef struct ASColor {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} ASColor;

typedef struct ASDrawCmd {
    uint8_t  cmd_type;     // ASDrawCmdType
    uint8_t  sub_type;     // Pattern 번호, Marker 종류(0=메모, 1=필터), Line 스타일 등
    uint8_t  align_horiz;  // 0=left, 1=center, 2=right
    uint8_t  align_vert;   // 0=top, 1=center, 2=bottom
    ASRect   rect;         // Line일 때는 (x=x1, y=y1, w=x2, h=y2) 시트 pt 좌표
    ASRect   clip_rect;    // Text 클리핑 사각형 (시트 pt)
    ASColor  color1;
    ASColor  color2;
    float    stroke_width; // pt 단위 두께
    uint32_t text_offset;  // UTF-8 바이트 오프셋
    uint32_t text_len;     // UTF-8 바이트 길이
    float    font_size_pt; // 엑셀 pt 크기
    bool     font_bold;
    bool     font_italic;
    bool     wrap_text;
    uint8_t  _pad;
} ASDrawCmd;

// P0 ~ P1 진입점
const char* as_last_note(void);
void        as_set_timezone_offset(int32_t seconds_from_gmt); // TODAY()/NOW() 용, as_open 전에
ASBook*     as_open(const char* path);
void        as_close(ASBook* book);
uint32_t    as_sheet_count(const ASBook* book);
bool        as_sheet_info(const ASBook* book, uint32_t index, ASSheetInfo* out);

// P2 확장 진입점
bool             as_sheet_axes(const ASBook* book, uint32_t sheet, const float** col_widths, uint32_t* num_cols, const float** row_heights, uint32_t* num_rows);
ASDrawList*      as_tile(const ASBook* book, uint32_t sheet, float x, float y, float w, float h, float zoom);
void             as_drawlist_free(ASDrawList* list);
uint32_t         as_drawlist_count(const ASDrawList* list);
const ASDrawCmd* as_drawlist_commands(const ASDrawList* list);
const char*      as_drawlist_strings(const ASDrawList* list, uint32_t* out_len);
bool             as_cell_info(const ASBook* book, uint32_t sheet, uint32_t row, uint32_t col, ASCellInfo* out);
uint32_t         as_find(const ASBook* book, const char* text, ASHit* hits, uint32_t max_hits);

#ifdef __cplusplus
}
#endif

#endif /* ARGOSHEET_H */

