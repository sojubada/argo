use crate::calc::{CalcErr, CellAddr, Value, WorkbookCalcResult};
use crate::numfmt::{builtin_format_code, CompiledFormat, FormatCompiler, FormatEngine, FormatValue};
use crate::style::{BorderEdge, CellXf, Rgba};
use crate::{Cell, CellKind, CellRange, Sheet, Workbook, CELL_FLAG_HAS_COMMENT, CELL_FLAG_HAS_FORMULA};
use std::collections::{HashMap, HashSet};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawCmdType {
    Fill     = 0,
    Gradient = 1,
    Pattern  = 2,
    Line     = 3,
    Text     = 4,
    Marker   = 5,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ASRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ASColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<Rgba> for ASColor {
    fn from(c: Rgba) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
            a: c.a,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ASDrawCmd {
    pub cmd_type: u8,
    pub sub_type: u8,
    pub align_horiz: u8, // 0=left, 1=center, 2=right
    pub align_vert: u8,  // 0=top, 1=center, 2=bottom
    pub rect: ASRect,
    pub clip_rect: ASRect,
    pub color1: ASColor,
    pub color2: ASColor,
    pub stroke_width: f32,
    pub text_offset: u32,
    pub text_len: u32,
    pub font_size_pt: f32,
    pub font_bold: bool,
    pub font_italic: bool,
    pub wrap_text: bool,
    pub _pad: u8,
}

pub struct DrawList {
    pub commands: Vec<ASDrawCmd>,
    pub string_buf: String,
}

impl DrawList {
    pub fn with_capacity(cmd_cap: usize, str_cap: usize) -> Self {
        Self {
            commands: Vec::with_capacity(cmd_cap),
            string_buf: String::with_capacity(str_cap),
        }
    }

    pub fn intern_str(&mut self, s: &str) -> (u32, u32) {
        if s.is_empty() {
            return (0, 0);
        }
        let off = self.string_buf.len() as u32;
        let len = s.len() as u32;
        self.string_buf.push_str(s);
        (off, len)
    }

    pub fn push_fill(&mut self, rect: ASRect, color: ASColor) {
        self.commands.push(ASDrawCmd {
            cmd_type: DrawCmdType::Fill as u8,
            sub_type: 0,
            align_horiz: 0,
            align_vert: 0,
            rect,
            clip_rect: rect,
            color1: color,
            color2: ASColor::default(),
            stroke_width: 0.0,
            text_offset: 0,
            text_len: 0,
            font_size_pt: 0.0,
            font_bold: false,
            font_italic: false,
            wrap_text: false,
            _pad: 0,
        });
    }

    pub fn push_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: ASColor, sub_type: u8) {
        self.commands.push(ASDrawCmd {
            cmd_type: DrawCmdType::Line as u8,
            sub_type,
            align_horiz: 0,
            align_vert: 0,
            rect: ASRect {
                x: x1,
                y: y1,
                w: x2,
                h: y2,
            },
            clip_rect: ASRect::default(),
            color1: color,
            color2: ASColor::default(),
            stroke_width: width,
            text_offset: 0,
            text_len: 0,
            font_size_pt: 0.0,
            font_bold: false,
            font_italic: false,
            wrap_text: false,
            _pad: 0,
        });
    }
}

/// 시트별 사전 계산된 누적 오프셋 및 병합 조회 캐시
/// 왜: 백그라운드 타일 스레드가 매번 누적 합산이나 선형 탐색을 하지 않도록 as_open 시점에 만든다.
pub struct SheetRenderCache {
    pub col_offsets_pt: Vec<f32>, // 길이: max_col + 1 (0번째는 0.0)
    pub row_offsets_pt: Vec<f32>, // 길이: max_row + 1 (0번째는 0.0)
    pub merge_map: HashMap<u64, CellRange>, // 작은 병합의 모든 칸(마스터+자식) -> 병합 범위
    /// 칸 수가 많은 병합(예: 시트 전체 A1:XFD1048576)은 칸마다 넣으면 수십억 개가 되어
    /// 메모리가 터진다. 이런 것은 따로 두고 범위 검사로 찾는다(개수가 적다).
    pub big_merges: Vec<CellRange>,
}

impl SheetRenderCache {
    pub fn build(sheet: &Sheet) -> Self {
        let mut col_offsets_pt = Vec::with_capacity(sheet.col_widths_pt.len() + 1);
        let mut acc_x = 0.0f32;
        col_offsets_pt.push(0.0);
        for &w in &sheet.col_widths_pt {
            acc_x += w;
            col_offsets_pt.push(acc_x);
        }

        let mut row_offsets_pt = Vec::with_capacity(sheet.row_heights_pt.len() + 1);
        let mut acc_y = 0.0f32;
        row_offsets_pt.push(0.0);
        for &h in &sheet.row_heights_pt {
            acc_y += h;
            row_offsets_pt.push(acc_y);
        }

        const SMALL_MERGE_CELLS: u64 = 4096;
        const MAP_BUDGET: u64 = 2_000_000;
        let mut merge_map = HashMap::new();
        let mut big_merges = Vec::new();
        let mut budget = 0u64;
        for &mr in &sheet.merges {
            if mr.start_row == 0 || mr.start_col == 0 || mr.start_row > mr.end_row || mr.start_col > mr.end_col {
                continue; // 망가진 병합은 무시
            }
            let cells = (mr.end_row - mr.start_row + 1) as u64 * (mr.end_col - mr.start_col + 1) as u64;
            if cells <= SMALL_MERGE_CELLS && budget + cells <= MAP_BUDGET {
                budget += cells;
                for r in mr.start_row..=mr.end_row {
                    for c in mr.start_col..=mr.end_col {
                        merge_map.insert(crate::cell_key(r, c), mr);
                    }
                }
            } else {
                big_merges.push(mr);
            }
        }

        Self {
            col_offsets_pt,
            row_offsets_pt,
            merge_map,
            big_merges,
        }
    }

    /// (r, c) 가 속한 병합 범위.
    #[inline]
    pub fn merge_at(&self, r: u32, c: u32) -> Option<CellRange> {
        if let Some(&m) = self.merge_map.get(&crate::cell_key(r, c)) {
            return Some(m);
        }
        self.big_merges
            .iter()
            .find(|m| r >= m.start_row && r <= m.end_row && c >= m.start_col && c <= m.end_col)
            .copied()
    }

    #[inline]
    pub fn cell_rect_pt(&self, start_row: u32, start_col: u32, end_row: u32, end_col: u32) -> ASRect {
        let c1 = (start_col.saturating_sub(1) as usize).min(self.col_offsets_pt.len().saturating_sub(1));
        let c2 = (end_col as usize).min(self.col_offsets_pt.len().saturating_sub(1));
        let r1 = (start_row.saturating_sub(1) as usize).min(self.row_offsets_pt.len().saturating_sub(1));
        let r2 = (end_row as usize).min(self.row_offsets_pt.len().saturating_sub(1));

        let x = self.col_offsets_pt[c1];
        let y = self.row_offsets_pt[r1];
        let w = (self.col_offsets_pt[c2] - x).max(0.0);
        let h = (self.row_offsets_pt[r2] - y).max(0.0);
        ASRect { x, y, w, h }
    }
}

/// 워크북 내 모든 숫자 서식을 사전 컴파일해 둔 테이블
pub struct FormatCache {
    pub map: HashMap<u32, CompiledFormat>,
}

impl FormatCache {
    pub fn build(wb: &Workbook) -> Self {
        let mut map = HashMap::new();
        for xf in &wb.styles.cell_xfs {
            let id = xf.num_fmt_id;
            map.entry(id).or_insert_with(|| {
                let code = wb
                    .styles
                    .num_fmts
                    .get(&id)
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| builtin_format_code(id));
                FormatCompiler::compile(code)
            });
        }
        Self { map }
    }

    pub fn get(&self, num_fmt_id: u32) -> CompiledFormat {
        self.map
            .get(&num_fmt_id)
            .cloned()
            .unwrap_or_else(|| FormatCompiler::compile(builtin_format_code(num_fmt_id)))
    }
}

/// 지정한 시트 pt 영역(`tile_rect`)에 해당하는 그리기 목록을 생성한다.
/// 왜: 여러 백그라운드 스레드에서 동시에 호출되므로 일체의 가변 전역 상태를 쓰지 않는다.
pub fn build_tile_drawlist(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    fmt_cache: &FormatCache,
    render_cache: &SheetRenderCache,
    sheet_idx: usize,
    tile_rect: ASRect,
    _zoom: f32,
) -> DrawList {
    let mut dl = DrawList::with_capacity(512, 2048);
    let Some(sheet) = wb.sheets.get(sheet_idx) else {
        return dl;
    };

    // 1. 바탕 흰색 칠하기
    dl.push_fill(
        tile_rect,
        ASColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );

    if sheet.max_row == 0 || sheet.max_col == 0 {
        return dl;
    }

    // 2. 타일 경계에 걸치는 1-based 행·열 범위를 이진 탐색으로 산출 (이웃 테두리 여유분 4pt 포함)
    let min_x = (tile_rect.x - 4.0).max(0.0);
    let max_x = tile_rect.x + tile_rect.w + 4.0;
    let min_y = (tile_rect.y - 4.0).max(0.0);
    let max_y = tile_rect.y + tile_rect.h + 4.0;

    let start_col = render_cache
        .col_offsets_pt
        .partition_point(|&ox| ox < min_x)
        .max(1) as u32;
    let end_col = render_cache
        .col_offsets_pt
        .partition_point(|&ox| ox <= max_x)
        .min(sheet.max_col as usize)
        .max(start_col as usize) as u32;

    let start_row = render_cache
        .row_offsets_pt
        .partition_point(|&oy| oy < min_y)
        .max(1) as u32;
    let end_row = render_cache
        .row_offsets_pt
        .partition_point(|&oy| oy <= max_y)
        .min(sheet.max_row as usize)
        .max(start_row as usize) as u32;

    if start_col > end_col || start_row > end_row {
        return dl;
    }

    let grid_color = ASColor {
        r: 217,
        g: 217,
        b: 217,
        a: 255,
    }; // #D9D9D9

    let mut drawn_merges: HashSet<(u32, u32)> = HashSet::new();

    // 3. 1패스: 배경 채우기 + 기본 격자선
    for r in start_row..=end_row {
        for c in start_col..=end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = if let Some(mr) = render_cache.merge_at(r, c) {
                if !drawn_merges.insert((mr.start_row, mr.start_col)) {
                    continue;
                }
                (mr.start_row, mr.start_col, mr.end_row, mr.end_col)
            } else {
                (r, c, r, c)
            };

            let rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }

            let cell = sheet.data.get_cell(mr_sr, mr_sc);
            let style_idx = cell.map(|cell| cell.style_idx as usize).unwrap_or(0);
            let xf = wb.styles.cell_xfs.get(style_idx).cloned().unwrap_or_default();

            let mut has_fill = false;
            if let Some(fill) = wb.styles.fills.get(xf.fill_id as usize) {
                if fill.pattern_type == "solid" {
                    if let Some(fg) = fill.fg_color {
                        dl.push_fill(rect, fg.into());
                        has_fill = true;
                    }
                } else if !fill.pattern_type.is_empty() && fill.pattern_type != "none" {
                    if let Some(fg) = fill.fg_color {
                        dl.commands.push(ASDrawCmd {
                            cmd_type: DrawCmdType::Pattern as u8,
                            sub_type: 1,
                            align_horiz: 0,
                            align_vert: 0,
                            rect,
                            clip_rect: rect,
                            color1: fg.into(),
                            color2: fill.bg_color.unwrap_or(Rgba::WHITE).into(),
                            stroke_width: 0.0,
                            text_offset: 0,
                            text_len: 0,
                            font_size_pt: 0.0,
                            font_bold: false,
                            font_italic: false,
                            wrap_text: false,
                            _pad: 0,
                        });
                        has_fill = true;
                    }
                }
            }

            // 격자선: showGridLines가 켜져 있고 채우기가 없는 셀의 우측·하단 변
            if sheet.show_grid_lines && !has_fill {
                dl.push_line(
                    rect.x + rect.w,
                    rect.y,
                    rect.x + rect.w,
                    rect.y + rect.h,
                    0.5,
                    grid_color,
                    0,
                );
                dl.push_line(
                    rect.x,
                    rect.y + rect.h,
                    rect.x + rect.w,
                    rect.y + rect.h,
                    0.5,
                    grid_color,
                    0,
                );
            }
        }
    }

    // 4. 2패스: 텍스트 + 셀 테두리 + 메모 마커
    drawn_merges.clear();
    for r in start_row..=end_row {
        for c in start_col..=end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = if let Some(mr) = render_cache.merge_at(r, c) {
                if !drawn_merges.insert((mr.start_row, mr.start_col)) {
                    continue;
                }
                (mr.start_row, mr.start_col, mr.end_row, mr.end_col)
            } else {
                (r, c, r, c)
            };

            let rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }

            let cell = sheet.data.get_cell(mr_sr, mr_sc);
            let style_idx = cell.map(|cell| cell.style_idx as usize).unwrap_or(0);
            let xf = wb.styles.cell_xfs.get(style_idx).cloned().unwrap_or_default();

            // 텍스트 출력
            if let Some(c_data) = cell {
                let (disp_str, val_kind) = format_cell_display(
                    wb,
                    calc_res,
                    fmt_cache,
                    sheet,
                    sheet_idx,
                    mr_sr,
                    mr_sc,
                    c_data,
                    &xf,
                );
                if !disp_str.is_empty() {
                    let font = wb
                        .styles
                        .fonts
                        .get(xf.font_id as usize)
                        .cloned()
                        .unwrap_or_default();
                    let text_color = font.color.unwrap_or(Rgba::BLACK);
                    let (t_off, t_len) = dl.intern_str(&disp_str);

                    let align_h = match xf.alignment.horizontal.as_deref() {
                        Some("center") | Some("centerContinuous") => 1,
                        Some("right") => 2,
                        Some("left") => 0,
                        _ => match val_kind {
                            CellKind::Num => 2,
                            CellKind::Bool | CellKind::Err => 1,
                            _ => 0,
                        },
                    };
                    let align_v = match xf.alignment.vertical.as_deref() {
                        Some("top") => 0,
                        Some("center") => 1,
                        _ => 2, // 엑셀 기본 세로 정렬은 bottom
                    };

                    let pad_x = 1.5f32; // 좌우 1.5pt(2px) 여백
                    let text_rect = ASRect {
                        x: rect.x + pad_x,
                        y: rect.y + 1.0,
                        w: (rect.w - pad_x * 2.0).max(0.0),
                        h: (rect.h - 2.0).max(0.0),
                    };

                    dl.commands.push(ASDrawCmd {
                        cmd_type: DrawCmdType::Text as u8,
                        sub_type: 0,
                        align_horiz: align_h,
                        align_vert: align_v,
                        rect: text_rect,
                        clip_rect: rect,
                        color1: text_color.into(),
                        color2: ASColor::default(),
                        stroke_width: 0.0,
                        text_offset: t_off,
                        text_len: t_len,
                        font_size_pt: font.size_pt,
                        font_bold: font.bold,
                        font_italic: font.italic,
                        wrap_text: xf.alignment.wrap_text,
                        _pad: 0,
                    });
                }

                // 메모 마커 (오른쪽 위 빨간 삼각형)
                if (c_data.flags & CELL_FLAG_HAS_COMMENT) != 0 {
                    dl.commands.push(ASDrawCmd {
                        cmd_type: DrawCmdType::Marker as u8,
                        sub_type: 0,
                        align_horiz: 0,
                        align_vert: 0,
                        rect: ASRect {
                            x: rect.x + rect.w - 5.0,
                            y: rect.y,
                            w: 5.0,
                            h: 5.0,
                        },
                        clip_rect: rect,
                        color1: ASColor {
                            r: 220,
                            g: 38,
                            b: 38,
                            a: 255,
                        },
                        color2: ASColor::default(),
                        stroke_width: 0.0,
                        text_offset: 0,
                        text_len: 0,
                        font_size_pt: 0.0,
                        font_bold: false,
                        font_italic: false,
                        wrap_text: false,
                        _pad: 0,
                    });
                }
            }

            // 셀 테두리 출력
            if let Some(border) = wb.styles.borders.get(xf.border_id as usize) {
                emit_border_edge(&mut dl, &border.top, rect.x, rect.y, rect.x + rect.w, rect.y);
                emit_border_edge(
                    &mut dl,
                    &border.bottom,
                    rect.x,
                    rect.y + rect.h,
                    rect.x + rect.w,
                    rect.y + rect.h,
                );
                emit_border_edge(&mut dl, &border.left, rect.x, rect.y, rect.x, rect.y + rect.h);
                emit_border_edge(
                    &mut dl,
                    &border.right,
                    rect.x + rect.w,
                    rect.y,
                    rect.x + rect.w,
                    rect.y + rect.h,
                );
            }
        }
    }

    dl
}

fn emit_border_edge(dl: &mut DrawList, edge: &BorderEdge, x1: f32, y1: f32, x2: f32, y2: f32) {
    if edge.style.is_empty() || edge.style == "none" {
        return;
    }
    let (width, sub_type) = match edge.style.as_str() {
        "hair" => (0.5, 1),
        "dotted" => (0.75, 2),
        "dashed" | " dashDot" | "dashDotDot" => (0.75, 3),
        "thin" => (0.75, 0),
        "medium" | "mediumDashed" | "mediumDashDot" => (1.5, 0),
        "thick" => (2.25, 0),
        "double" => (2.0, 4),
        _ => (0.75, 0),
    };
    let color = edge.color.unwrap_or(Rgba::BLACK).into();
    dl.push_line(x1, y1, x2, y2, width, color, sub_type);
}

#[allow(clippy::too_many_arguments)]
pub fn format_cell_display(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    fmt_cache: &FormatCache,
    _sheet: &Sheet,
    sheet_idx: usize,
    row: u32,
    col: u32,
    cell: &Cell,
    xf: &CellXf,
) -> (String, CellKind) {
    // 1. 값 결정: 캐시 값(<v>)이 있으면 우선 사용하고, <v>가 비어 있는 수식 셀이면 수식 계산값을 쓴다.
    let val: Value = unsafe {
        match cell.kind_enum() {
            CellKind::Num => Value::Num(cell.payload.num),
            CellKind::Str => Value::Str(wb.pool.get(cell.payload.str_span).to_string()),
            CellKind::Bool => Value::Bool(cell.payload.boolean != 0),
            CellKind::Err => {
                let s = wb.pool.get(cell.payload.str_span);
                Value::Err(CalcErr::from_str(s).unwrap_or(CalcErr::Value))
            }
            CellKind::Blank => {
                if (cell.flags & CELL_FLAG_HAS_FORMULA) != 0 {
                    calc_res
                        .values
                        .get(&CellAddr {
                            sheet: sheet_idx,
                            row,
                            col,
                        })
                        .cloned()
                        .unwrap_or(Value::Blank)
                } else {
                    Value::Blank
                }
            }
        }
    };

    let compiled = fmt_cache.get(xf.num_fmt_id);
    match val {
        Value::Blank => (String::new(), CellKind::Blank),
        Value::Num(n) => (
            FormatEngine::format_value(&FormatValue::Number(n), &compiled),
            CellKind::Num,
        ),
        Value::Str(s) => (
            FormatEngine::format_value(&FormatValue::Text(s), &compiled),
            CellKind::Str,
        ),
        Value::Bool(b) => (
            FormatEngine::format_value(&FormatValue::Bool(b), &compiled),
            CellKind::Bool,
        ),
        Value::Err(e) => (e.as_str().to_string(), CellKind::Err),
        _ => (String::new(), CellKind::Blank),
    }
}

