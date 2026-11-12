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
    uint32_t merge_start_row;
    uint32_t merge_start_col;
    uint32_t merge_end_row;
    uint32_t merge_end_col;
    uint8_t  kind;
    uint8_t  flags;
    double   num_value;
    char     address[32];
    char     display_text[512];
    char     formula[512];
    char     format_code[128];
    char     comment_text[512];
    char     comment_author[128];
    char     link_target[512];
    bool     link_is_internal;
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

// 선 스타일 (13종 + 격자선 0)
typedef enum ASLineStyle {
    AS_LINE_GRID             = 0,
    AS_LINE_HAIR             = 1,
    AS_LINE_DOTTED           = 2,
    AS_LINE_DASH_DOT_DOT     = 3,
    AS_LINE_DASH_DOT         = 4,
    AS_LINE_DASHED           = 5,
    AS_LINE_THIN             = 6,
    AS_LINE_MEDIUM_DASH_DOT_DOT = 7,
    AS_LINE_SLANT_DASH_DOT   = 8,
    AS_LINE_MEDIUM_DASH_DOT  = 9,
    AS_LINE_MEDIUM_DASHED    = 10,
    AS_LINE_MEDIUM           = 11,
    AS_LINE_DOUBLE           = 12,
    AS_LINE_THICK            = 13,
} ASLineStyle;

// 가로 정렬 (P3 전체 규격)
typedef enum ASAlignHoriz {
    AS_ALIGN_H_LEFT              = 0,
    AS_ALIGN_H_CENTER            = 1,
    AS_ALIGN_H_RIGHT             = 2,
    AS_ALIGN_H_FILL              = 3,
    AS_ALIGN_H_JUSTIFY           = 4,
    AS_ALIGN_H_DISTRIBUTED       = 5,
    AS_ALIGN_H_CENTER_CONTINUOUS = 6,
} ASAlignHoriz;

// 세로 정렬 (기본값은 BOTTOM = 2)
typedef enum ASAlignVert {
    AS_ALIGN_V_TOP         = 0,
    AS_ALIGN_V_CENTER      = 1,
    AS_ALIGN_V_BOTTOM      = 2,
    AS_ALIGN_V_JUSTIFY     = 3,
    AS_ALIGN_V_DISTRIBUTED = 4,
} ASAlignVert;

// 텍스트 속성 비트 플래그
#define AS_TEXT_FLAG_BOLD          (1u << 0)
#define AS_TEXT_FLAG_ITALIC        (1u << 1)
#define AS_TEXT_FLAG_UNDERLINE     (1u << 2)
#define AS_TEXT_FLAG_STRIKE        (1u << 3)
#define AS_TEXT_FLAG_WRAP          (1u << 4)
#define AS_TEXT_FLAG_SHRINK_TO_FIT (1u << 5)

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
    uint8_t  sub_type;     // ASLineStyle / Pattern ID / Marker ID(0=comment, 1=filter)
    uint8_t  align_horiz;  // ASAlignHoriz
    uint8_t  align_vert;   // ASAlignVert
    ASRect   rect;         // Line일 때: (x=x1, y=y1, w=x2, h=y2), Text일 때: 여백/넘침 적용된 텍스트 박스
    ASRect   clip_rect;    // Text 클리핑 사각형 (넘친 빈 칸까지 확장된 영역)
    ASColor  color1;
    ASColor  color2;
    float    stroke_width;
    uint32_t text_offset;
    uint32_t text_len;
    float    font_size_pt;
    int16_t  rotation;     // 0, 1..=180, 255(세로쓰기)
    uint8_t  indent;       // 들여쓰기 단계
    uint8_t  text_flags;   // AS_TEXT_FLAG_*
} ASDrawCmd;

const char*      as_last_note(void);
void             as_set_timezone_offset(int32_t seconds_from_gmt); // TODAY()/NOW() 용, as_open 전에
ASBook*          as_open(const char* path);
void             as_close(ASBook* book);
uint32_t         as_sheet_count(const ASBook* book);
bool             as_sheet_info(const ASBook* book, uint32_t index, ASSheetInfo* out);
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

