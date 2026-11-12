use crate::calc::{CalcErr, CellAddr, Value, WorkbookCalcResult};
use crate::cf::{IconSetKind, ResolvedIcon, SheetCfCache};
use crate::numfmt::{builtin_format_code, CompiledFormat, FormatCompiler, FormatEngine, FormatValue};
use crate::style::{BorderEdge, BorderStyle, CellXf, Rgba};
use crate::{
    Cell, CellKind, CellRange, Sheet, Workbook, CELL_FLAG_HAS_COMMENT, CELL_FLAG_HAS_FORMULA,
};
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
    Bar      = 6,
    Icon     = 7,
}

pub const LINE_STYLE_GRID: u8                = 0;
pub const LINE_STYLE_HAIR: u8                = 1;
pub const LINE_STYLE_DOTTED: u8              = 2;
pub const LINE_STYLE_DASH_DOT_DOT: u8        = 3;
pub const LINE_STYLE_DASH_DOT: u8            = 4;
pub const LINE_STYLE_DASHED: u8              = 5;
pub const LINE_STYLE_THIN: u8                = 6;
pub const LINE_STYLE_MEDIUM_DASH_DOT_DOT: u8 = 7;
pub const LINE_STYLE_SLANT_DASH_DOT: u8      = 8;
pub const LINE_STYLE_MEDIUM_DASH_DOT: u8     = 9;
pub const LINE_STYLE_MEDIUM_DASHED: u8       = 10;
pub const LINE_STYLE_MEDIUM: u8              = 11;
pub const LINE_STYLE_DOUBLE: u8              = 12;
pub const LINE_STYLE_THICK: u8               = 13;

pub const ALIGN_H_LEFT: u8              = 0;
pub const ALIGN_H_CENTER: u8            = 1;
pub const ALIGN_H_RIGHT: u8             = 2;
pub const ALIGN_H_FILL: u8              = 3;
pub const ALIGN_H_JUSTIFY: u8           = 4;
pub const ALIGN_H_DISTRIBUTED: u8       = 5;
pub const ALIGN_H_CENTER_CONTINUOUS: u8 = 6;

pub const ALIGN_V_TOP: u8         = 0;
pub const ALIGN_V_CENTER: u8      = 1;
pub const ALIGN_V_BOTTOM: u8      = 2;
pub const ALIGN_V_JUSTIFY: u8     = 3;
pub const ALIGN_V_DISTRIBUTED: u8 = 4;

pub const TEXT_FLAG_BOLD: u8          = 1 << 0;
pub const TEXT_FLAG_ITALIC: u8        = 1 << 1;
pub const TEXT_FLAG_UNDERLINE: u8     = 1 << 2;
pub const TEXT_FLAG_STRIKE: u8        = 1 << 3;
pub const TEXT_FLAG_WRAP: u8          = 1 << 4;
pub const TEXT_FLAG_SHRINK_TO_FIT: u8 = 1 << 5;

pub const CELL_PAD_X_PT: f32 = 1.5;

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
    pub align_horiz: u8,
    pub align_vert: u8,
    pub rect: ASRect,
    pub clip_rect: ASRect,
    pub color1: ASColor,
    pub color2: ASColor,
    pub stroke_width: f32,
    pub text_offset: u32,
    pub text_len: u32,
    pub font_size_pt: f32,
    pub rotation: i16,
    pub indent: u8,
    pub text_flags: u8,
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
            rotation: 0,
            indent: 0,
            text_flags: 0,
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push_line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: ASColor,
        style_code: u8,
    ) {
        self.commands.push(ASDrawCmd {
            cmd_type: DrawCmdType::Line as u8,
            sub_type: style_code,
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
            rotation: 0,
            indent: 0,
            text_flags: 0,
        });
    }
}

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
        let max_c = self.col_offsets_pt.len().saturating_sub(1);
        let max_r = self.row_offsets_pt.len().saturating_sub(1);
        let c1 = (start_col.saturating_sub(1) as usize).min(max_c);
        let c2 = (end_col as usize).min(max_c);
        let r1 = (start_row.saturating_sub(1) as usize).min(max_r);
        let r2 = (end_row as usize).min(max_r);

        let x = self.col_offsets_pt[c1];
        let y = self.row_offsets_pt[r1];
        let w = (self.col_offsets_pt[c2] - x).max(0.0);
        let h = (self.row_offsets_pt[r2] - y).max(0.0);
        ASRect { x, y, w, h }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedEdge {
    pub rank: u8,
    pub style_code: u8,
    pub width_pt: f32,
    pub color: ASColor,
}

pub fn parse_border_edge_spec(edge: &BorderEdge) -> Option<ResolvedEdge> {
    let s = edge.style.trim();
    if s.is_empty() || s == "none" {
        return None;
    }
    let (rank, style_code, width_pt) = match s {
        "hair" => (1, LINE_STYLE_HAIR, 0.5),
        "dotted" => (2, LINE_STYLE_DOTTED, 0.75),
        "dashDotDot" => (2, LINE_STYLE_DASH_DOT_DOT, 0.75),
        "dashDot" => (2, LINE_STYLE_DASH_DOT, 0.75),
        "dashed" => (2, LINE_STYLE_DASHED, 0.75),
        "thin" => (2, LINE_STYLE_THIN, 0.75),
        "mediumDashDotDot" => (3, LINE_STYLE_MEDIUM_DASH_DOT_DOT, 1.5),
        "slantDashDot" => (3, LINE_STYLE_SLANT_DASH_DOT, 1.5),
        "mediumDashDot" => (3, LINE_STYLE_MEDIUM_DASH_DOT, 1.5),
        "mediumDashed" => (3, LINE_STYLE_MEDIUM_DASHED, 1.5),
        "medium" => (3, LINE_STYLE_MEDIUM, 1.5),
        "double" => (4, LINE_STYLE_DOUBLE, 2.25),
        "thick" => (5, LINE_STYLE_THICK, 2.5),
        _ => (2, LINE_STYLE_THIN, 0.75),
    };
    let color = edge.color.unwrap_or(Rgba::BLACK).into();
    Some(ResolvedEdge {
        rank,
        style_code,
        width_pt,
        color,
    })
}

pub fn resolve_shared_border(
    first: Option<ResolvedEdge>,
    second: Option<ResolvedEdge>,
) -> Option<ResolvedEdge> {
    match (first, second) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => {
            if a.rank > b.rank {
                Some(a)
            } else {
                Some(b)
            }
        }
    }
}

fn pattern_name_to_id(pat: &str) -> u8 {
    match pat {
        "solid" => 1,
        "mediumGray" => 2,
        "darkGray" => 3,
        "lightGray" => 4,
        "darkHorizontal" => 5,
        "darkVertical" => 6,
        "darkDown" => 7,
        "darkUp" => 8,
        "darkGrid" => 9,
        "darkTrellis" => 10,
        "lightHorizontal" => 11,
        "lightVertical" => 12,
        "lightDown" => 13,
        "lightUp" => 14,
        "lightGrid" => 15,
        "lightTrellis" => 16,
        "gray125" => 17,
        "gray0625" => 18,
        _ => 0,
    }
}

pub fn estimate_text_width_pt(text: &str, font_size_pt: f32) -> f32 {
    let mut units = 0.0f32;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        if ('\u{1100}'..='\u{D7AF}').contains(&ch)
            || ('\u{3000}'..='\u{9FFF}').contains(&ch)
            || ('\u{F900}'..='\u{FAFF}').contains(&ch)
            || ('\u{FF01}'..='\u{FF60}').contains(&ch)
        {
            units += 1.0;
        } else if ch == 'W' || ch == 'M' || ch == '@' || ch == '%' {
            units += 0.82;
        } else if ch == 'i' || ch == 'l' || ch == 'I' || ch == '.' || ch == ',' || ch == ':' || ch == ' ' {
            units += 0.32;
        } else {
            units += 0.58;
        }
    }
    units * font_size_pt
}

pub fn fit_number_to_cell_width(
    formatted: String,
    raw_num: Option<f64>,
    is_general: bool,
    font_size_pt: f32,
    avail_w_pt: f32,
) -> String {
    if avail_w_pt <= 2.0 {
        return "#".to_string();
    }
    if estimate_text_width_pt(&formatted, font_size_pt) <= avail_w_pt + 1.5 {
        return formatted;
    }

    if is_general {
        if let Some(n) = raw_num {
            for prec in (1..=8).rev() {
                let candidate = if n.abs() >= 1e11 || (n != 0.0 && n.abs() < 1e-6) {
                    format!("{:.*E}", prec, n)
                } else {
                    let s = format!("{:.*}", prec, n);
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                };
                if estimate_text_width_pt(&candidate, font_size_pt) <= avail_w_pt + 1.0 {
                    return candidate;
                }
            }
            let exp_short = format!("{:.1E}", n);
            if estimate_text_width_pt(&exp_short, font_size_pt) <= avail_w_pt + 1.0 {
                return exp_short;
            }
        }
    }

    let hash_w = (font_size_pt * 0.58).max(4.0);
    let count = ((avail_w_pt / hash_w).floor() as usize).max(1).min(64);
    "#".repeat(count)
}

/// 벡터 아이콘 색상 결정: 3Arrows 기준 0=빨강(아래), 1=노랑(오른쪽), 2=초록(위)
fn icon_color_for(icon: ResolvedIcon) -> ASColor {
    if icon.set_kind == IconSetKind::Arrows3Gray {
        return ASColor {
            r: 107,
            g: 114,
            b: 128,
            a: 255,
        };
    }
    let count = icon.set_kind.icon_count();
    match (count, icon.index) {
        (3, 0) | (4, 0) | (5, 0) => ASColor {
            r: 220,
            g: 38,
            b: 38,
            a: 255,
        }, // 빨강
        (3, 1) | (4, 1) | (4, 2) | (5, 1) | (5, 2) | (5, 3) => ASColor {
            r: 234,
            g: 179,
            b: 8,
            a: 255,
        }, // 노랑/주황
        _ => ASColor {
            r: 22,
            g: 163,
            b: 74,
            a: 255,
        }, // 초록
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_tile_drawlist(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    fmt_cache: &FormatCache,
    render_cache: &SheetRenderCache,
    cf_cache: &SheetCfCache,
    sheet_idx: usize,
    tile_rect: ASRect,
    _zoom: f32,
) -> DrawList {
    let mut dl = DrawList::with_capacity(896, 4096);
    let Some(sheet) = wb.sheets.get(sheet_idx) else {
        return dl;
    };

    // 1. 바탕 흰색
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

    // 2. 셀 채우기 (셀 자체 채우기 + 조건부 서식 배경/3색 색조 + 표 줄무늬)
    //    조건부 서식(색조/dxf)이 있으면 셀 채우기 위에 우선 적용된다.
    let mut drawn_merges: HashSet<(u32, u32)> = HashSet::new();
    for r in start_row..=end_row {
        for c in start_col..=end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = resolve_merge_bounds(render_cache, r, c);
            if !drawn_merges.insert((mr_sr, mr_sc)) {
                continue;
            }
            let rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }

            let key = crate::cell_key(mr_sr, mr_sc);
            let cf_ov = cf_cache.cells.get(&key);

            if let Some(cf_bg) = cf_ov.and_then(|o| o.bg_color) {
                dl.push_fill(rect, cf_bg.into());
                continue;
            }

            let xf = get_cell_xf(wb, sheet, mr_sr, mr_sc);
            if let Some(fill) = wb.styles.fills.get(xf.fill_id as usize) {
                let pat_id = pattern_name_to_id(&fill.pattern_type);
                if pat_id == 1 {
                    if let Some(fg) = fill.fg_color {
                        dl.push_fill(rect, fg.into());
                    }
                } else if pat_id > 1 {
                    if let Some(fg) = fill.fg_color {
                        dl.commands.push(ASDrawCmd {
                            cmd_type: DrawCmdType::Pattern as u8,
                            sub_type: pat_id,
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
                            rotation: 0,
                            indent: 0,
                            text_flags: 0,
                        });
                    }
                }
            }
        }
    }

    // 3. 격자선 및 공유 변 테두리(셀 테두리 + CF dxf 테두리) 수집
    let mut border_lines: Vec<ASDrawCmd> = Vec::new();
    emit_gridlines_and_collect_borders(
        &mut dl,
        &mut border_lines,
        wb,
        sheet,
        render_cache,
        cf_cache,
        start_row,
        end_row,
        start_col,
        end_col,
    );

    // 4. 조건부 서식 데이터 막대 (AS_CMD_BAR: 격자선 위, 텍스트/아이콘 아래에 배치)
    drawn_merges.clear();
    for r in start_row..=end_row {
        for c in start_col..=end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = resolve_merge_bounds(render_cache, r, c);
            if !drawn_merges.insert((mr_sr, mr_sc)) {
                continue;
            }
            let key = crate::cell_key(mr_sr, mr_sc);
            let Some(bar) = cf_cache.cells.get(&key).and_then(|o| o.data_bar) else {
                continue;
            };
            let cell_rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if cell_rect.w <= 3.0 || cell_rect.h <= 3.0 {
                continue;
            }

            let inset_x = 1.5f32;
            let inset_y = 2.0f32;
            let avail_w = (cell_rect.w - inset_x * 2.0).max(0.0);
            let bar_x = cell_rect.x + inset_x + bar.start_ratio * avail_w;
            let bar_w = ((bar.end_ratio - bar.start_ratio) * avail_w).max(0.0);
            let bar_rect = ASRect {
                x: bar_x,
                y: cell_rect.y + inset_y,
                w: bar_w,
                h: (cell_rect.h - inset_y * 2.0).max(1.0),
            };
            let (has_axis, axis_x) = match bar.axis_ratio {
                Some(ar) => (1u8, cell_rect.x + inset_x + ar * avail_w),
                None => (0u8, 0.0),
            };

            dl.commands.push(ASDrawCmd {
                cmd_type: DrawCmdType::Bar as u8,
                sub_type: 0,
                align_horiz: 0,
                align_vert: has_axis,
                rect: bar_rect,
                clip_rect: cell_rect,
                color1: bar.color.into(),
                color2: bar.axis_color.into(),
                stroke_width: axis_x,
                text_offset: 0,
                text_len: 0,
                font_size_pt: 0.0,
                rotation: 0,
                indent: 0,
                text_flags: 0,
            });
        }
    }

    // 5. 조건부 서식 벡터 아이콘(AS_CMD_ICON) + 셀 텍스트(옆 칸 넘침 및 숫자 ### 포함)
    let text_scan_start_col = start_col.saturating_sub(8).max(1);
    let text_scan_end_col = (end_col + 8).min(sheet.max_col);

    for r in start_row..=end_row {
        let row_h = render_cache.cell_rect_pt(r, 1, r, 1).h;
        if row_h <= 0.0 {
            continue;
        }

        let mut c = text_scan_start_col;
        while c <= text_scan_end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = resolve_merge_bounds(render_cache, r, c);
            if r != mr_sr || c != mr_sc {
                c += 1;
                continue;
            }

            let base_rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if base_rect.w <= 0.0 || base_rect.h <= 0.0 {
                c = mr_ec + 1;
                continue;
            }

            let key = crate::cell_key(mr_sr, mr_sc);
            let cf_ov = cf_cache.cells.get(&key);

            // 5-a. 벡터 아이콘 출력 (셀 왼쪽에 배치하고 텍스트 영역을 오른쪽으로 민다)
            let mut icon_left_advance = 0.0f32;
            if let Some(icon) = cf_ov.and_then(|o| o.icon) {
                let icon_size = (base_rect.h - 4.0).clamp(7.5, 12.5);
                if base_rect.x + base_rect.w >= tile_rect.x
                    && base_rect.x <= tile_rect.x + tile_rect.w
                {
                    let icon_rect = ASRect {
                        x: base_rect.x + 2.0,
                        y: base_rect.y + (base_rect.h - icon_size) * 0.5,
                        w: icon_size,
                        h: icon_size,
                    };
                    dl.commands.push(ASDrawCmd {
                        cmd_type: DrawCmdType::Icon as u8,
                        sub_type: icon.set_kind as u8,
                        align_horiz: icon.index,
                        align_vert: 0,
                        rect: icon_rect,
                        clip_rect: base_rect,
                        color1: icon_color_for(icon),
                        color2: ASColor::default(),
                        stroke_width: 0.0,
                        text_offset: 0,
                        text_len: 0,
                        font_size_pt: 0.0,
                        rotation: 0,
                        indent: 0,
                        text_flags: 0,
                    });
                }
                icon_left_advance = icon_size + 3.0;
            }

            // showValue="0" 이면 숫자를 숨긴다
            if cf_ov.map(|o| o.hide_value).unwrap_or(false) {
                c = mr_ec + 1;
                continue;
            }

            let Some(c_data) = sheet.data.get_cell(mr_sr, mr_sc) else {
                c = mr_ec + 1;
                continue;
            };

            let xf = get_cell_xf(wb, sheet, mr_sr, mr_sc);
            let (mut disp_str, val_kind, raw_num) = format_cell_display_with_cf(
                wb,
                calc_res,
                fmt_cache,
                sheet,
                sheet_idx,
                mr_sr,
                mr_sc,
                c_data,
                &xf,
                cf_ov.and_then(|o| o.num_fmt_code.as_deref()),
            );

            if disp_str.is_empty() {
                c = mr_ec + 1;
                continue;
            }

            let font = wb
                .styles
                .fonts
                .get(xf.font_id as usize)
                .cloned()
                .unwrap_or_default();

            let align_h = resolve_align_horiz(&xf, val_kind);
            let align_v = resolve_align_vert(&xf);
            let indent_u8 = (xf.alignment.indent.min(32)) as u8;
            let indent_pt = (indent_u8 as f32) * 7.0;
            let rotation = xf.alignment.text_rotation as i16;

            let mut draw_rect = base_rect;
            let mut clip_rect = base_rect;

            let is_merged = mr_sr != mr_er || mr_sc != mr_ec;
            let can_overflow = val_kind == CellKind::Str
                && !is_merged
                && !xf.alignment.wrap_text
                && !xf.alignment.shrink_to_fit
                && rotation == 0
                && icon_left_advance == 0.0
                && matches!(align_h, ALIGN_H_LEFT | ALIGN_H_RIGHT | ALIGN_H_CENTER);

            if align_h == ALIGN_H_CENTER_CONTINUOUS && !is_merged {
                let mut end_cc = mr_ec;
                while end_cc < sheet.max_col {
                    let next_c = end_cc + 1;
                    if !is_cell_empty_for_overflow(sheet, render_cache, r, next_c) {
                        break;
                    }
                    let next_xf = get_cell_xf(wb, sheet, r, next_c);
                    if next_xf.alignment.horizontal.as_deref() != Some("centerContinuous") {
                        break;
                    }
                    end_cc = next_c;
                }
                let span_rect = render_cache.cell_rect_pt(r, mr_sc, r, end_cc);
                draw_rect = span_rect;
                clip_rect = span_rect;
            } else if can_overflow {
                let needed_w = estimate_text_width_pt(&disp_str, font.size_pt)
                    + CELL_PAD_X_PT * 2.0
                    + indent_pt;
                if needed_w > base_rect.w {
                    let (ov_start_c, ov_end_c) = compute_overflow_col_span(
                        sheet,
                        render_cache,
                        r,
                        mr_sc,
                        needed_w,
                        align_h,
                    );
                    clip_rect = render_cache.cell_rect_pt(r, ov_start_c, r, ov_end_c);
                    draw_rect = match align_h {
                        ALIGN_H_LEFT => ASRect {
                            x: base_rect.x,
                            y: base_rect.y,
                            w: (clip_rect.x + clip_rect.w) - base_rect.x,
                            h: base_rect.h,
                        },
                        ALIGN_H_RIGHT => ASRect {
                            x: clip_rect.x,
                            y: base_rect.y,
                            w: (base_rect.x + base_rect.w) - clip_rect.x,
                            h: base_rect.h,
                        },
                        _ => clip_rect,
                    };
                }
            } else if val_kind == CellKind::Num && !xf.alignment.shrink_to_fit && rotation == 0 {
                let avail_w =
                    (base_rect.w - CELL_PAD_X_PT * 2.0 - indent_pt - icon_left_advance).max(1.0);
                let is_general = xf.num_fmt_id == 0
                    || wb
                        .styles
                        .num_fmts
                        .get(&xf.num_fmt_id)
                        .map(|s| s.eq_ignore_ascii_case("general"))
                        .unwrap_or(false);
                disp_str = fit_number_to_cell_width(
                    disp_str,
                    raw_num,
                    is_general,
                    font.size_pt,
                    avail_w,
                );
            }

            if clip_rect.x + clip_rect.w >= tile_rect.x
                && clip_rect.x <= tile_rect.x + tile_rect.w
            {
                let (t_off, t_len) = dl.intern_str(&disp_str);
                let is_bold = cf_ov.and_then(|o| o.font_bold).unwrap_or(font.bold);
                let is_italic = cf_ov.and_then(|o| o.font_italic).unwrap_or(font.italic);
                let is_underline = cf_ov
                    .and_then(|o| o.font_underline)
                    .unwrap_or(font.underline);
                let is_strike = cf_ov.and_then(|o| o.font_strike).unwrap_or(font.strike);
                let final_color = cf_ov
                    .and_then(|o| o.text_color)
                    .or(font.color)
                    .unwrap_or(Rgba::BLACK);

                let mut text_flags = 0u8;
                if is_bold {
                    text_flags |= TEXT_FLAG_BOLD;
                }
                if is_italic {
                    text_flags |= TEXT_FLAG_ITALIC;
                }
                if is_underline {
                    text_flags |= TEXT_FLAG_UNDERLINE;
                }
                if is_strike {
                    text_flags |= TEXT_FLAG_STRIKE;
                }
                if xf.alignment.wrap_text {
                    text_flags |= TEXT_FLAG_WRAP;
                }
                if xf.alignment.shrink_to_fit && !xf.alignment.wrap_text {
                    text_flags |= TEXT_FLAG_SHRINK_TO_FIT;
                }

                // 필터 단추가 있는 머리글 셀은 우측 여백을 11pt 비워 글자가 단추와 겹치지 않게 한다
                let filter_right_pad = if cf_cache.filter_buttons.contains(&key) {
                    11.0
                } else {
                    0.0
                };

                let inner_rect = ASRect {
                    x: draw_rect.x + CELL_PAD_X_PT + icon_left_advance,
                    y: draw_rect.y + 1.0,
                    w: (draw_rect.w
                        - CELL_PAD_X_PT * 2.0
                        - icon_left_advance
                        - filter_right_pad)
                        .max(0.0),
                    h: (draw_rect.h - 2.0).max(0.0),
                };

                dl.commands.push(ASDrawCmd {
                    cmd_type: DrawCmdType::Text as u8,
                    sub_type: 0,
                    align_horiz: align_h,
                    align_vert: align_v,
                    rect: inner_rect,
                    clip_rect,
                    color1: final_color.into(),
                    color2: ASColor::default(),
                    stroke_width: 0.0,
                    text_offset: t_off,
                    text_len: t_len,
                    font_size_pt: font.size_pt,
                    rotation,
                    indent: indent_u8,
                    text_flags,
                });
            }

            c = mr_ec + 1;
        }
    }

    // 6. 테두리 및 대각선
    dl.commands.extend(border_lines);

    // 7. 메모 삼각형(sub_type=0) 및 표/자동필터 드롭다운 단추(sub_type=1)
    drawn_merges.clear();
    for r in start_row..=end_row {
        for c in start_col..=end_col {
            let (mr_sr, mr_sc, mr_er, mr_ec) = resolve_merge_bounds(render_cache, r, c);
            if !drawn_merges.insert((mr_sr, mr_sc)) {
                continue;
            }
            let rect = render_cache.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
            if rect.w <= 0.0 || rect.h <= 0.0 {
                continue;
            }
            let key = crate::cell_key(mr_sr, mr_sc);

            if let Some(c_data) = sheet.data.get_cell(mr_sr, mr_sc) {
                if (c_data.flags & CELL_FLAG_HAS_COMMENT) != 0 {
                    dl.commands.push(ASDrawCmd {
                        cmd_type: DrawCmdType::Marker as u8,
                        sub_type: 0,
                        align_horiz: 0,
                        align_vert: 0,
                        rect: ASRect {
                            x: rect.x + rect.w - 5.5,
                            y: rect.y,
                            w: 5.5,
                            h: 5.5,
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
                        rotation: 0,
                        indent: 0,
                        text_flags: 0,
                    });
                }
            }

            // 표 머리글 또는 autoFilter 필터 단추 표시 (보기 전용)
            if cf_cache.filter_buttons.contains(&key) && rect.w >= 14.0 && rect.h >= 10.0 {
                let btn_size = 9.5f32.min(rect.h - 2.0);
                dl.commands.push(ASDrawCmd {
                    cmd_type: DrawCmdType::Marker as u8,
                    sub_type: 1, // AS_MARKER_FILTER_BUTTON
                    align_horiz: 0,
                    align_vert: 0,
                    rect: ASRect {
                        x: rect.x + rect.w - btn_size - 1.5,
                        y: rect.y + (rect.h - btn_size) * 0.5,
                        w: btn_size,
                        h: btn_size,
                    },
                    clip_rect: rect,
                    color1: ASColor {
                        r: 100,
                        g: 116,
                        b: 139,
                        a: 255,
                    },
                    color2: ASColor::default(),
                    stroke_width: 0.0,
                    text_offset: 0,
                    text_len: 0,
                    font_size_pt: 0.0,
                    rotation: 0,
                    indent: 0,
                    text_flags: 0,
                });
            }
        }
    }

    dl
}

#[inline]
fn resolve_merge_bounds(rc: &SheetRenderCache, r: u32, c: u32) -> (u32, u32, u32, u32) {
    if let Some(mr) = rc.merge_at(r, c) {
        (mr.start_row, mr.start_col, mr.end_row, mr.end_col)
    } else {
        (r, c, r, c)
    }
}

#[inline]
fn get_cell_xf(wb: &Workbook, sheet: &Sheet, r: u32, c: u32) -> CellXf {
    let style_idx = sheet
        .data
        .get_cell(r, c)
        .map(|cell| cell.style_idx as usize)
        .unwrap_or(0);
    wb.styles.cell_xfs.get(style_idx).cloned().unwrap_or_default()
}

fn cell_has_fill(
    wb: &Workbook,
    sheet: &Sheet,
    rc: &SheetRenderCache,
    cf_cache: &SheetCfCache,
    r: u32,
    c: u32,
) -> bool {
    if r == 0 || c == 0 || r > sheet.max_row || c > sheet.max_col {
        return false;
    }
    let (mr_sr, mr_sc, _, _) = resolve_merge_bounds(rc, r, c);
    let key = crate::cell_key(mr_sr, mr_sc);
    if cf_cache
        .cells
        .get(&key)
        .and_then(|o| o.bg_color)
        .is_some()
    {
        return true;
    }
    let xf = get_cell_xf(wb, sheet, mr_sr, mr_sc);
    if let Some(fill) = wb.styles.fills.get(xf.fill_id as usize) {
        let pat_id = pattern_name_to_id(&fill.pattern_type);
        if pat_id >= 1 && fill.fg_color.is_some() {
            return true;
        }
    }
    false
}

fn get_effective_cell_border<'a>(
    wb: &'a Workbook,
    sheet: &Sheet,
    cf_cache: &'a SheetCfCache,
    r: u32,
    c: u32,
) -> Option<&'a BorderStyle> {
    if r == 0 || c == 0 || r > sheet.max_row || c > sheet.max_col {
        return None;
    }
    let key = crate::cell_key(r, c);
    if let Some(cf_b) = cf_cache.cells.get(&key).and_then(|o| o.border.as_ref()) {
        return Some(cf_b);
    }
    let style_idx = sheet.data.get_cell(r, c)?.style_idx as usize;
    let xf = wb.styles.cell_xfs.get(style_idx)?;
    wb.styles.borders.get(xf.border_id as usize)
}

#[allow(clippy::too_many_arguments)]
fn emit_gridlines_and_collect_borders(
    dl: &mut DrawList,
    border_lines: &mut Vec<ASDrawCmd>,
    wb: &Workbook,
    sheet: &Sheet,
    rc: &SheetRenderCache,
    cf_cache: &SheetCfCache,
    start_row: u32,
    end_row: u32,
    start_col: u32,
    end_col: u32,
) {
    let grid_color = ASColor {
        r: 217,
        g: 217,
        b: 217,
        a: 255,
    };

    let r_begin = start_row.saturating_sub(1);
    for r in r_begin..=end_row {
        let y = rc.row_offsets_pt.get(r as usize).copied().unwrap_or(0.0);
        for c in start_col..=end_col {
            let rect_c = rc.cell_rect_pt(1, c, 1, c);
            if rect_c.w <= 0.0 {
                continue;
            }
            let x1 = rect_c.x;
            let x2 = rect_c.x + rect_c.w;

            if r >= 1 && r < sheet.max_row {
                if let Some(mr) = rc.merge_at(r, c) {
                    if mr.end_row > r {
                        continue;
                    }
                }
            }

            let top_edge = if r >= 1 {
                get_effective_cell_border(wb, sheet, cf_cache, r, c)
                    .and_then(|b| parse_border_edge_spec(&b.bottom))
            } else {
                None
            };
            let bottom_edge = if r < sheet.max_row {
                get_effective_cell_border(wb, sheet, cf_cache, r + 1, c)
                    .and_then(|b| parse_border_edge_spec(&b.top))
            } else {
                None
            };

            if let Some(winner) = resolve_shared_border(top_edge, bottom_edge) {
                border_lines.push(ASDrawCmd {
                    cmd_type: DrawCmdType::Line as u8,
                    sub_type: winner.style_code,
                    align_horiz: 0,
                    align_vert: 0,
                    rect: ASRect {
                        x: x1,
                        y,
                        w: x2,
                        h: y,
                    },
                    clip_rect: ASRect::default(),
                    color1: winner.color,
                    color2: ASColor::default(),
                    stroke_width: winner.width_pt,
                    text_offset: 0,
                    text_len: 0,
                    font_size_pt: 0.0,
                    rotation: 0,
                    indent: 0,
                    text_flags: 0,
                });
            } else if sheet.show_grid_lines && r >= 1 && r <= sheet.max_row {
                let fill_above = cell_has_fill(wb, sheet, rc, cf_cache, r, c);
                let fill_below = cell_has_fill(wb, sheet, rc, cf_cache, r + 1, c);
                if !fill_above && !fill_below {
                    dl.push_line(x1, y, x2, y, 0.5, grid_color, LINE_STYLE_GRID);
                }
            }
        }
    }

    let c_begin = start_col.saturating_sub(1);
    for c in c_begin..=end_col {
        let x = rc.col_offsets_pt.get(c as usize).copied().unwrap_or(0.0);
        for r in start_row..=end_row {
            let rect_r = rc.cell_rect_pt(r, 1, r, 1);
            if rect_r.h <= 0.0 {
                continue;
            }
            let y1 = rect_r.y;
            let y2 = rect_r.y + rect_r.h;

            if c >= 1 && c < sheet.max_col {
                if let Some(mr) = rc.merge_at(r, c) {
                    if mr.end_col > c {
                        continue;
                    }
                }
            }

            let left_edge = if c >= 1 {
                get_effective_cell_border(wb, sheet, cf_cache, r, c)
                    .and_then(|b| parse_border_edge_spec(&b.right))
            } else {
                None
            };
            let right_edge = if c < sheet.max_col {
                get_effective_cell_border(wb, sheet, cf_cache, r, c + 1)
                    .and_then(|b| parse_border_edge_spec(&b.left))
            } else {
                None
            };

            if let Some(winner) = resolve_shared_border(left_edge, right_edge) {
                border_lines.push(ASDrawCmd {
                    cmd_type: DrawCmdType::Line as u8,
                    sub_type: winner.style_code,
                    align_horiz: 0,
                    align_vert: 0,
                    rect: ASRect {
                        x,
                        y: y1,
                        w: x,
                        h: y2,
                    },
                    clip_rect: ASRect::default(),
                    color1: winner.color,
                    color2: ASColor::default(),
                    stroke_width: winner.width_pt,
                    text_offset: 0,
                    text_len: 0,
                    font_size_pt: 0.0,
                    rotation: 0,
                    indent: 0,
                    text_flags: 0,
                });
            } else if sheet.show_grid_lines && c >= 1 && c <= sheet.max_col {
                let fill_left = cell_has_fill(wb, sheet, rc, cf_cache, r, c);
                let fill_right = cell_has_fill(wb, sheet, rc, cf_cache, r, c + 1);
                if !fill_left && !fill_right {
                    dl.push_line(x, y1, x, y2, 0.5, grid_color, LINE_STYLE_GRID);
                }
            }
        }
    }

    for r in start_row..=end_row {
        for c in start_col..=end_col {
            if let Some(b) = get_effective_cell_border(wb, sheet, cf_cache, r, c) {
                if !b.diagonal_up && !b.diagonal_down {
                    continue;
                }
                if let Some(diag) = parse_border_edge_spec(&b.diagonal) {
                    let (mr_sr, mr_sc, mr_er, mr_ec) = resolve_merge_bounds(rc, r, c);
                    if r != mr_sr || c != mr_sc {
                        continue;
                    }
                    let rect = rc.cell_rect_pt(mr_sr, mr_sc, mr_er, mr_ec);
                    if b.diagonal_down {
                        border_lines.push(ASDrawCmd {
                            cmd_type: DrawCmdType::Line as u8,
                            sub_type: diag.style_code,
                            align_horiz: 0,
                            align_vert: 0,
                            rect: ASRect {
                                x: rect.x,
                                y: rect.y,
                                w: rect.x + rect.w,
                                h: rect.y + rect.h,
                            },
                            clip_rect: ASRect::default(),
                            color1: diag.color,
                            color2: ASColor::default(),
                            stroke_width: diag.width_pt,
                            text_offset: 0,
                            text_len: 0,
                            font_size_pt: 0.0,
                            rotation: 0,
                            indent: 0,
                            text_flags: 0,
                        });
                    }
                    if b.diagonal_up {
                        border_lines.push(ASDrawCmd {
                            cmd_type: DrawCmdType::Line as u8,
                            sub_type: diag.style_code,
                            align_horiz: 0,
                            align_vert: 0,
                            rect: ASRect {
                                x: rect.x,
                                y: rect.y + rect.h,
                                w: rect.x + rect.w,
                                h: rect.y,
                            },
                            clip_rect: ASRect::default(),
                            color1: diag.color,
                            color2: ASColor::default(),
                            stroke_width: diag.width_pt,
                            text_offset: 0,
                            text_len: 0,
                            font_size_pt: 0.0,
                            rotation: 0,
                            indent: 0,
                            text_flags: 0,
                        });
                    }
                }
            }
        }
    }
}

fn is_cell_empty_for_overflow(sheet: &Sheet, rc: &SheetRenderCache, r: u32, c: u32) -> bool {
    if c < 1 || c > sheet.max_col {
        return false;
    }
    if rc.merge_at(r, c).is_some() {
        return false;
    }
    match sheet.data.get_cell(r, c) {
        None => true,
        Some(cell) => {
            cell.kind_enum() == CellKind::Blank && (cell.flags & CELL_FLAG_HAS_FORMULA) == 0
        }
    }
}

fn compute_overflow_col_span(
    sheet: &Sheet,
    rc: &SheetRenderCache,
    row: u32,
    col: u32,
    needed_w_pt: f32,
    align_h: u8,
) -> (u32, u32) {
    let base_w = rc.cell_rect_pt(row, col, row, col).w;
    if needed_w_pt <= base_w {
        return (col, col);
    }

    match align_h {
        ALIGN_H_LEFT => {
            let mut acc_w = base_w;
            let mut end_c = col;
            while acc_w < needed_w_pt && end_c < sheet.max_col {
                let next_c = end_c + 1;
                if !is_cell_empty_for_overflow(sheet, rc, row, next_c) {
                    break;
                }
                acc_w += rc.cell_rect_pt(row, next_c, row, next_c).w;
                end_c = next_c;
            }
            (col, end_c)
        }
        ALIGN_H_RIGHT => {
            let mut acc_w = base_w;
            let mut start_c = col;
            while acc_w < needed_w_pt && start_c > 1 {
                let prev_c = start_c - 1;
                if !is_cell_empty_for_overflow(sheet, rc, row, prev_c) {
                    break;
                }
                acc_w += rc.cell_rect_pt(row, prev_c, row, prev_c).w;
                start_c = prev_c;
            }
            (start_c, col)
        }
        ALIGN_H_CENTER => {
            let half_extra = (needed_w_pt - base_w) * 0.5;
            let mut left_w = 0.0f32;
            let mut start_c = col;
            while left_w < half_extra && start_c > 1 {
                let prev_c = start_c - 1;
                if !is_cell_empty_for_overflow(sheet, rc, row, prev_c) {
                    break;
                }
                left_w += rc.cell_rect_pt(row, prev_c, row, prev_c).w;
                start_c = prev_c;
            }
            let mut right_w = 0.0f32;
            let mut end_c = col;
            while right_w < half_extra && end_c < sheet.max_col {
                let next_c = end_c + 1;
                if !is_cell_empty_for_overflow(sheet, rc, row, next_c) {
                    break;
                }
                right_w += rc.cell_rect_pt(row, next_c, row, next_c).w;
                end_c = next_c;
            }
            (start_c, end_c)
        }
        _ => (col, col),
    }
}

fn resolve_align_horiz(xf: &CellXf, val_kind: CellKind) -> u8 {
    match xf.alignment.horizontal.as_deref() {
        Some("left") => ALIGN_H_LEFT,
        Some("center") => ALIGN_H_CENTER,
        Some("right") => ALIGN_H_RIGHT,
        Some("fill") => ALIGN_H_FILL,
        Some("justify") => ALIGN_H_JUSTIFY,
        Some("distributed") => ALIGN_H_DISTRIBUTED,
        Some("centerContinuous") => ALIGN_H_CENTER_CONTINUOUS,
        _ => match val_kind {
            CellKind::Num => ALIGN_H_RIGHT,
            CellKind::Bool | CellKind::Err => ALIGN_H_CENTER,
            _ => ALIGN_H_LEFT,
        },
    }
}

fn resolve_align_vert(xf: &CellXf) -> u8 {
    match xf.alignment.vertical.as_deref() {
        Some("top") => ALIGN_V_TOP,
        Some("center") => ALIGN_V_CENTER,
        Some("justify") => ALIGN_V_JUSTIFY,
        Some("distributed") => ALIGN_V_DISTRIBUTED,
        _ => ALIGN_V_BOTTOM,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn format_cell_display(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    fmt_cache: &FormatCache,
    sheet: &Sheet,
    sheet_idx: usize,
    row: u32,
    col: u32,
    cell: &Cell,
    xf: &CellXf,
) -> (String, CellKind, Option<f64>) {
    format_cell_display_with_cf(
        wb, calc_res, fmt_cache, sheet, sheet_idx, row, col, cell, xf, None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn format_cell_display_with_cf(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    fmt_cache: &FormatCache,
    _sheet: &Sheet,
    sheet_idx: usize,
    row: u32,
    col: u32,
    cell: &Cell,
    xf: &CellXf,
    cf_fmt_override: Option<&str>,
) -> (String, CellKind, Option<f64>) {
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

    let compiled = match cf_fmt_override {
        Some(code) => FormatCompiler::compile(code),
        None => fmt_cache.get(xf.num_fmt_id),
    };

    match val {
        Value::Blank => (String::new(), CellKind::Blank, None),
        Value::Num(n) => (
            FormatEngine::format_value(&FormatValue::Number(n), &compiled),
            CellKind::Num,
            Some(n),
        ),
        Value::Str(s) => (
            FormatEngine::format_value(&FormatValue::Text(s), &compiled),
            CellKind::Str,
            None,
        ),
        Value::Bool(b) => (
            FormatEngine::format_value(&FormatValue::Bool(b), &compiled),
            CellKind::Bool,
            None,
        ),
        Value::Err(e) => (e.as_str().to_string(), CellKind::Err, None),
        _ => (String::new(), CellKind::Blank, None),
    }
}