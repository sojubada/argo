pub mod calc;
pub mod cf;
pub mod draw;
pub mod drawing;
pub mod numfmt;
pub mod package;
pub mod parse;
pub mod style;

use crate::calc::{evaluate_workbook, WorkbookCalcResult};
use crate::cf::{build_sheet_cf_cache, CfBlock, SheetCfCache, TableDef};
use crate::draw::{
    build_tile_drawlist, format_cell_display, ASDrawCmd, ASRect, DrawList, FormatCache,
    SheetRenderCache,
};
use crate::drawing::{build_sheet_overlays, RawSheetDrawing, ResolvedOverlay};
use crate::numfmt::builtin_format_code;
use crate::parse::format_cell_address;
use crate::style::StyleSheet;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Blank = 0,
    Num   = 1,
    Str   = 2,
    Bool  = 3,
    Err   = 4,
}

impl CellKind {
    pub fn as_tag_str(self, has_formula: bool) -> &'static str {
        match self {
            CellKind::Blank if has_formula => "f",
            CellKind::Blank => "blank",
            CellKind::Num => "n",
            CellKind::Str => "s",
            CellKind::Bool => "b",
            CellKind::Err => "e",
        }
    }
}

pub const CELL_FLAG_HAS_FORMULA: u8    = 1 << 0;
pub const CELL_FLAG_FORMULA_SHARED: u8 = 1 << 1;
pub const CELL_FLAG_FORMULA_ARRAY: u8  = 1 << 2;
pub const CELL_FLAG_HAS_COMMENT: u8    = 1 << 3;
pub const CELL_FLAG_HAS_LINK: u8       = 1 << 4;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StrSpan {
    pub offset: u32,
    pub len: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union CellPayload {
    pub num: f64,
    pub str_span: StrSpan,
    pub boolean: u64,
    pub raw: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cell {
    pub col: u32,
    pub kind: u8,
    pub flags: u8,
    pub style_idx: u16,
    pub payload: CellPayload,
}

impl Cell {
    pub fn kind_enum(&self) -> CellKind {
        match self.kind {
            1 => CellKind::Num,
            2 => CellKind::Str,
            3 => CellKind::Bool,
            4 => CellKind::Err,
            _ => CellKind::Blank,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct StringPool {
    buf: String,
}

impl StringPool {
    pub fn intern(&mut self, s: &str) -> StrSpan {
        if s.is_empty() {
            return StrSpan { offset: 0, len: 0 };
        }
        let offset = self.buf.len() as u32;
        let len = s.len() as u32;
        self.buf.push_str(s);
        StrSpan { offset, len }
    }

    pub fn get(&self, span: StrSpan) -> &str {
        if span.len == 0 {
            return "";
        }
        let start = span.offset as usize;
        let Some(end) = start.checked_add(span.len as usize) else {
            return "";
        };
        self.buf.get(start..end).unwrap_or("")
    }
}

#[derive(Default, Clone)]
pub struct Rows {
    pub row_ids: Vec<u32>,
    pub row_start: Vec<u32>,
    pub cells: Vec<Cell>,
}

impl Rows {
    pub fn push_row(&mut self, row_1based: u32, row_cells: &[Cell]) {
        if self.row_start.is_empty() {
            self.row_start.push(0);
        }
        self.row_ids.push(row_1based);
        self.cells.extend_from_slice(row_cells);
        self.row_start.push(self.cells.len() as u32);
    }

    pub fn get_cell(&self, row_1based: u32, col_1based: u32) -> Option<&Cell> {
        let r_pos = self.row_ids.binary_search(&row_1based).ok()?;
        let start = self.row_start[r_pos] as usize;
        let end = self.row_start[r_pos + 1] as usize;
        let slice = &self.cells[start..end];
        let c_pos = slice.binary_search_by_key(&col_1based, |c| c.col).ok()?;
        Some(&slice[c_pos])
    }

    pub fn get_cell_mut(&mut self, row_1based: u32, col_1based: u32) -> Option<&mut Cell> {
        let r_pos = self.row_ids.binary_search(&row_1based).ok()?;
        let start = self.row_start[r_pos] as usize;
        let end = self.row_start[r_pos + 1] as usize;
        let slice = &mut self.cells[start..end];
        let c_pos = slice.binary_search_by_key(&col_1based, |c| c.col).ok()?;
        Some(&mut slice[c_pos])
    }
}

#[inline]
pub fn cell_key(row_1based: u32, col_1based: u32) -> u64 {
    ((row_1based as u64) << 32) | (col_1based as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaKind {
    Normal,
    Shared,
    Array,
}

#[derive(Debug, Clone)]
pub struct FormulaEntry {
    pub kind: FormulaKind,
    pub text: StrSpan,
    pub ref_range: Option<CellRange>,
    pub shared_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezePane {
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub top_left_row: u32,
    pub top_left_col: u32,
}

#[derive(Debug, Clone, Default)]
pub struct DataValidationEntry {
    pub validation_type: String,
    pub allow_blank: bool,
    pub ranges: Vec<CellRange>,
    pub formula1: Option<String>,
    pub formula2: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommentEntry {
    pub row: u32,
    pub col: u32,
    pub author: Option<StrSpan>,
    pub text: StrSpan,
}

#[derive(Debug, Clone, Copy)]
pub struct HyperlinkEntry {
    pub target: StrSpan,
    pub is_internal: bool,
}

#[derive(Debug, Clone)]
pub struct DefinedName {
    pub name: String,
    pub local_sheet_id: Option<u32>,
    pub formula: String,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetState {
    Visible    = 0,
    Hidden     = 1,
    VeryHidden = 2,
}

pub struct Sheet {
    pub name: String,
    pub sheet_id: u32,
    pub state: SheetState,
    pub max_row: u32,
    pub max_col: u32,
    pub cell_count: u32,
    pub col_widths_pt: Vec<f32>,
    pub row_heights_pt: Vec<f32>,
    pub default_col_width_pt: f32,
    pub default_row_height_pt: f32,
    pub data: Rows,
    pub formulas: HashMap<u64, FormulaEntry>,
    pub merges: Vec<CellRange>,
    pub freeze: Option<FreezePane>,
    pub show_grid_lines: bool,
    pub auto_filter: Option<CellRange>,
    pub validations: Vec<DataValidationEntry>,
    pub comments: Vec<CommentEntry>,
    pub links: HashMap<u64, HyperlinkEntry>,
    pub cf_blocks: Vec<CfBlock>,
    pub tables: Vec<TableDef>,
    pub raw_drawings: Vec<RawSheetDrawing>,
    /// 시트 탭 색(`<sheetPr><tabColor>`). 없으면 None.
    pub tab_color: Option<crate::style::Rgba>,
}

impl Sheet {
    pub fn total_width_pt(&self) -> f32 {
        self.col_widths_pt.iter().sum()
    }

    pub fn total_height_pt(&self) -> f32 {
        self.row_heights_pt.iter().sum()
    }

    pub fn raw_value_str(&self, pool: &StringPool, cell: &Cell) -> String {
        unsafe {
            match cell.kind_enum() {
                CellKind::Blank => String::new(),
                CellKind::Num => {
                    let v = cell.payload.num;
                    if v.fract() == 0.0 && v.abs() < 1e15 {
                        format!("{:.0}", v)
                    } else {
                        v.to_string()
                    }
                }
                CellKind::Str | CellKind::Err => pool.get(cell.payload.str_span).to_string(),
                CellKind::Bool => {
                    if cell.payload.boolean != 0 {
                        "TRUE".to_string()
                    } else {
                        "FALSE".to_string()
                    }
                }
            }
        }
    }

    pub fn formula_str<'a>(&self, pool: &'a StringPool, row_1based: u32, col_1based: u32) -> &'a str {
        let key = cell_key(row_1based, col_1based);
        match self.formulas.get(&key) {
            Some(entry) => pool.get(entry.text),
            None => "",
        }
    }
}

pub struct Workbook {
    pub path: String,
    pub date1904: bool,
    pub full_calc_on_load: bool,
    pub defined_names: Vec<DefinedName>,
    pub styles: StyleSheet,
    pub pool: StringPool,
    pub sheets: Vec<Sheet>,
}

pub struct ASBook {
    pub wb: Workbook,
    pub calc_res: WorkbookCalcResult,
    pub fmt_cache: FormatCache,
    pub render_caches: Vec<SheetRenderCache>,
    pub cf_caches: Vec<SheetCfCache>,
    pub overlays: Vec<Vec<ResolvedOverlay>>,
}

unsafe impl Sync for ASBook {}
unsafe impl Send for ASBook {}

pub struct ASDrawList {
    pub inner: DrawList,
}

#[repr(C)]
pub struct ASSheetInfo {
    pub name: [c_char; 256],
    pub sheet_id: u32,
    pub state: u8,
    pub row_count: u32,
    pub col_count: u32,
    pub cell_count: u32,
    pub formula_count: u32,
    pub merge_count: u32,
    pub validation_count: u32,
    pub comment_count: u32,
    pub is_frozen: bool,
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub top_left_row: u32,
    pub top_left_col: u32,
    pub show_grid_lines: bool,
    pub total_width_pt: f32,
    pub total_height_pt: f32,
    /// P5: 시트의 차트·그림·도형 수. 기존 필드 배치를 바꾸지 않으려고 맨 끝에 둔다.
    pub overlay_count: u32,
}

#[repr(C)]
pub struct ASCellInfo {
    pub row: u32,
    pub col: u32,
    pub merge_start_row: u32,
    pub merge_start_col: u32,
    pub merge_end_row: u32,
    pub merge_end_col: u32,
    pub kind: u8,
    pub flags: u8,
    pub num_value: f64,
    pub address: [c_char; 32],
    pub display_text: [c_char; 512],
    pub formula: [c_char; 512],
    pub format_code: [c_char; 128],
    pub comment_text: [c_char; 512],
    pub comment_author: [c_char; 128],
    pub link_target: [c_char; 512],
    pub link_is_internal: bool,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ASHit {
    pub sheet: u32,
    pub row: u32,
    pub col: u32,
}

#[repr(C)]
pub struct ASOverlayInfo {
    pub kind: u8,
    pub frame_pt: ASRect,
    pub start_row: u32,
    pub start_col: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub crop_left: f32,
    pub crop_top: f32,
    pub crop_right: f32,
    pub crop_bottom: f32,
    pub rotation_deg: f32,
    pub title: [c_char; 256],
}

thread_local! {
    static LAST_NOTE: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

pub fn set_last_note(msg: &str) {
    let clean = msg.replace('\0', " ").replace('\n', " ");
    if let Ok(cstr) = CString::new(clean) {
        LAST_NOTE.with(|cell| {
            *cell.borrow_mut() = cstr;
        });
    }
}

fn write_c_string_buf(dst: &mut [c_char], src: &str) {
    dst.fill(0);
    let bytes = src.as_bytes();
    let mut copy_len = bytes.len().min(dst.len().saturating_sub(1));
    // 한글(3바이트) 한가운데서 자르면 Swift 쪽에서 깨진 글자(�)가 된다. 글자 경계까지 물린다.
    while copy_len > 0 && !src.is_char_boundary(copy_len) {
        copy_len -= 1;
    }
    for (i, &b) in bytes[..copy_len].iter().enumerate() {
        dst[i] = b as c_char;
    }
}

#[no_mangle]
pub extern "C" fn as_last_note() -> *const c_char {
    LAST_NOTE.with(|cell| cell.borrow().as_ptr())
}

/// 기기 시간대(초, GMT 기준). TODAY()/NOW() 가 쓴다. as_open 전에 부른다.
#[no_mangle]
pub extern "C" fn as_set_timezone_offset(seconds_from_gmt: i32) {
    calc::set_timezone_offset_seconds(seconds_from_gmt as i64);
}

#[no_mangle]
pub extern "C" fn as_open(path: *const c_char) -> *mut ASBook {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if path.is_null() {
            set_last_note("as_open: path 포인터가 NULL입니다");
            return ptr::null_mut();
        }
        let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_note("as_open: 유효하지 않은 UTF-8 경로입니다");
                return ptr::null_mut();
            }
        };

        match parse::open_xlsx_workbook(path_str) {
            Ok(wb) => {
                let calc_res = evaluate_workbook(&wb);
                let fmt_cache = FormatCache::build(&wb);
                let render_caches: Vec<SheetRenderCache> =
                    wb.sheets.iter().map(SheetRenderCache::build).collect();
                // 조건부 서식의 수식 규칙과 차트(시트 값 읽기)는 계산 엔진·재귀를 쓰므로
                // 큰 스택 스레드에서 만든다. 실패하면(스레드 생성 실패·패닉) 그 부분 없이 연다.
                let (cf_caches, overlays): (Vec<SheetCfCache>, Vec<Vec<ResolvedOverlay>>) =
                    calc::run_on_big_stack(|| {
                        let cf: Vec<SheetCfCache> = (0..wb.sheets.len())
                            .map(|idx| build_sheet_cf_cache(&wb, &calc_res, idx))
                            .collect();
                        let ov: Vec<Vec<ResolvedOverlay>> = (0..wb.sheets.len())
                            .map(|idx| {
                                std::panic::catch_unwind(AssertUnwindSafe(|| {
                                    build_sheet_overlays(
                                        &wb,
                                        &calc_res,
                                        &render_caches[idx],
                                        idx,
                                        &wb.sheets[idx].raw_drawings,
                                    )
                                }))
                                .unwrap_or_default()
                            })
                            .collect();
                        (cf, ov)
                    })
                    .unwrap_or_else(|| {
                        (
                            (0..wb.sheets.len()).map(|_| SheetCfCache::default()).collect(),
                            (0..wb.sheets.len()).map(|_| Vec::new()).collect(),
                        )
                    });
                Box::into_raw(Box::new(ASBook {
                    wb,
                    calc_res,
                    fmt_cache,
                    render_caches,
                    cf_caches,
                    overlays,
                }))
            }
            Err(e) => {
                set_last_note(&e);
                ptr::null_mut()
            }
        }
    }));

    res.unwrap_or_else(|_| {
        set_last_note("as_open: 내부 패닉 포착");
        ptr::null_mut()
    })
}

#[no_mangle]
pub extern "C" fn as_close(book: *mut ASBook) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !book.is_null() {
            unsafe {
                drop(Box::from_raw(book));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn as_sheet_count(book: *const ASBook) -> u32 {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() {
            return 0;
        }
        let b = unsafe { &*book };
        b.wb.sheets.len() as u32
    }));
    res.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn as_sheet_info(book: *const ASBook, index: u32, out: *mut ASSheetInfo) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || out.is_null() {
            return false;
        }
        let b = unsafe { &*book };
        let s_idx = index as usize;
        let Some(s) = b.wb.sheets.get(s_idx) else {
            return false;
        };

        let info = unsafe { &mut *out };
        write_c_string_buf(&mut info.name, &s.name);
        info.sheet_id = s.sheet_id;
        info.state = s.state as u8;
        info.row_count = s.max_row;
        info.col_count = s.max_col;
        info.cell_count = s.cell_count;
        info.formula_count = s.formulas.len() as u32;
        info.merge_count = s.merges.len() as u32;
        info.validation_count = s.validations.len() as u32;
        info.comment_count = s.comments.len() as u32;
        info.overlay_count = b
            .overlays
            .get(s_idx)
            .map(|v| v.len() as u32)
            .unwrap_or(0);

        if let Some(f) = s.freeze {
            info.is_frozen = true;
            info.freeze_rows = f.freeze_rows;
            info.freeze_cols = f.freeze_cols;
            info.top_left_row = f.top_left_row;
            info.top_left_col = f.top_left_col;
        } else {
            info.is_frozen = false;
            info.freeze_rows = 0;
            info.freeze_cols = 0;
            info.top_left_row = 1;
            info.top_left_col = 1;
        }

        info.show_grid_lines = s.show_grid_lines;
        info.total_width_pt = s.total_width_pt();
        info.total_height_pt = s.total_height_pt();
        true
    }));
    res.unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn as_sheet_axes(
    book: *const ASBook,
    sheet: u32,
    col_widths: *mut *const f32,
    num_cols: *mut u32,
    row_heights: *mut *const f32,
    num_rows: *mut u32,
) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null()
            || col_widths.is_null()
            || num_cols.is_null()
            || row_heights.is_null()
            || num_rows.is_null()
        {
            return false;
        }
        let b = unsafe { &*book };
        let Some(s) = b.wb.sheets.get(sheet as usize) else {
            return false;
        };
        unsafe {
            *col_widths = s.col_widths_pt.as_ptr();
            *num_cols = s.col_widths_pt.len() as u32;
            *row_heights = s.row_heights_pt.as_ptr();
            *num_rows = s.row_heights_pt.len() as u32;
        }
        true
    }));
    res.unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn as_tile(
    book: *const ASBook,
    sheet: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    zoom: f32,
) -> *mut ASDrawList {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() {
            return ptr::null_mut();
        }
        let b = unsafe { &*book };
        let s_idx = sheet as usize;
        let Some(rc) = b.render_caches.get(s_idx) else {
            return ptr::null_mut();
        };
        let Some(cfc) = b.cf_caches.get(s_idx) else {
            return ptr::null_mut();
        };
        let tile_rect = ASRect { x, y, w, h };
        let inner = build_tile_drawlist(
            &b.wb,
            &b.calc_res,
            &b.fmt_cache,
            rc,
            cfc,
            s_idx,
            tile_rect,
            zoom,
        );
        Box::into_raw(Box::new(ASDrawList { inner }))
    }));
    res.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn as_drawlist_free(list: *mut ASDrawList) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !list.is_null() {
            unsafe {
                drop(Box::from_raw(list));
            }
        }
    }));
}

#[no_mangle]
pub extern "C" fn as_drawlist_count(list: *const ASDrawList) -> u32 {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if list.is_null() {
            return 0;
        }
        let l = unsafe { &*list };
        l.inner.commands.len() as u32
    }));
    res.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn as_drawlist_commands(list: *const ASDrawList) -> *const ASDrawCmd {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if list.is_null() {
            return ptr::null();
        }
        let l = unsafe { &*list };
        l.inner.commands.as_ptr()
    }));
    res.unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn as_drawlist_strings(list: *const ASDrawList, out_len: *mut u32) -> *const c_char {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if list.is_null() {
            if !out_len.is_null() {
                unsafe { *out_len = 0 };
            }
            return ptr::null();
        }
        let l = unsafe { &*list };
        if !out_len.is_null() {
            unsafe { *out_len = l.inner.string_buf.len() as u32 };
        }
        l.inner.string_buf.as_ptr() as *const c_char
    }));
    res.unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn as_cell_info(
    book: *const ASBook,
    sheet: u32,
    row: u32,
    col: u32,
    out: *mut ASCellInfo,
) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || out.is_null() {
            return false;
        }
        let b = unsafe { &*book };
        let s_idx = sheet as usize;
        let Some(s) = b.wb.sheets.get(s_idx) else {
            return false;
        };
        let Some(rc) = b.render_caches.get(s_idx) else {
            return false;
        };

        let info = unsafe { &mut *out };

        // 병합 영역에 속한 경우 마스터 좌표와 병합 범위를 함께 반환
        let (target_r, target_c, mr) = if let Some(m_range) = rc.merge_at(row, col) {
            (m_range.start_row, m_range.start_col, m_range)
        } else {
            (
                row,
                col,
                CellRange {
                    start_row: row,
                    start_col: col,
                    end_row: row,
                    end_col: col,
                },
            )
        };

        info.row = row;
        info.col = col;
        info.merge_start_row = mr.start_row;
        info.merge_start_col = mr.start_col;
        info.merge_end_row = mr.end_row;
        info.merge_end_col = mr.end_col;

        let addr_str = if mr.start_row == mr.end_row && mr.start_col == mr.end_col {
            format_cell_address(row, col)
        } else {
            format!(
                "{}:{}",
                format_cell_address(mr.start_row, mr.start_col),
                format_cell_address(mr.end_row, mr.end_col)
            )
        };
        write_c_string_buf(&mut info.address, &addr_str);

        let cell = s.data.get_cell(target_r, target_c);
        let style_idx = cell.map(|c| c.style_idx as usize).unwrap_or(0);
        let xf = b.wb.styles.cell_xfs.get(style_idx).cloned().unwrap_or_default();
        let fmt_code = b
            .wb
            .styles
            .num_fmts
            .get(&xf.num_fmt_id)
            .cloned()
            .unwrap_or_else(|| builtin_format_code(xf.num_fmt_id).to_string());
        write_c_string_buf(&mut info.format_code, &fmt_code);

        let formula = s.formula_str(&b.wb.pool, target_r, target_c);
        write_c_string_buf(&mut info.formula, formula);

        if let Some(cm) = s
            .comments
            .iter()
            .find(|cm| cm.row == target_r && cm.col == target_c)
        {
            write_c_string_buf(&mut info.comment_text, b.wb.pool.get(cm.text));
            let author = cm.author.map(|a| b.wb.pool.get(a)).unwrap_or("");
            write_c_string_buf(&mut info.comment_author, author);
        } else {
            write_c_string_buf(&mut info.comment_text, "");
            write_c_string_buf(&mut info.comment_author, "");
        }

        let target_key = cell_key(target_r, target_c);
        if let Some(lnk) = s.links.get(&target_key) {
            write_c_string_buf(&mut info.link_target, b.wb.pool.get(lnk.target));
            info.link_is_internal = lnk.is_internal;
        } else {
            write_c_string_buf(&mut info.link_target, "");
            info.link_is_internal = false;
        }

        if let Some(c_data) = cell {
            let (disp, resolved_kind, raw_num) = format_cell_display(
                &b.wb,
                &b.calc_res,
                &b.fmt_cache,
                s,
                s_idx,
                target_r,
                target_c,
                c_data,
                &xf,
            );
            info.kind = resolved_kind as u8;
            info.flags = c_data.flags;
            info.num_value = raw_num.unwrap_or(0.0);
            write_c_string_buf(&mut info.display_text, &disp);
        } else {
            info.kind = CellKind::Blank as u8;
            info.flags = 0;
            info.num_value = 0.0;
            write_c_string_buf(&mut info.display_text, "");
        }
        true
    }));
    res.unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn as_find(
    book: *const ASBook,
    text: *const c_char,
    hits: *mut ASHit,
    max_hits: u32,
) -> u32 {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || text.is_null() || hits.is_null() || max_hits == 0 {
            return 0;
        }
        let query = match unsafe { CStr::from_ptr(text) }.to_str() {
            Ok(s) if !s.trim().is_empty() => s.to_lowercase(),
            _ => return 0,
        };
        let b = unsafe { &*book };
        let mut count = 0u32;
        let out_slice = unsafe { std::slice::from_raw_parts_mut(hits, max_hits as usize) };

        for (s_idx, s) in b.wb.sheets.iter().enumerate() {
            for (r_pos, &r) in s.data.row_ids.iter().enumerate() {
                let start = s.data.row_start[r_pos] as usize;
                let end = s.data.row_start[r_pos + 1] as usize;
                for cell in &s.data.cells[start..end] {
                    let xf = b
                        .wb
                        .styles
                        .cell_xfs
                        .get(cell.style_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    let (disp, _, _) = format_cell_display(
                        &b.wb,
                        &b.calc_res,
                        &b.fmt_cache,
                        s,
                        s_idx,
                        r,
                        cell.col,
                        cell,
                        &xf,
                    );
                    if disp.to_lowercase().contains(&query) {
                        out_slice[count as usize] = ASHit {
                            sheet: s_idx as u32,
                            row: r,
                            col: cell.col,
                        };
                        count += 1;
                        if count >= max_hits {
                            return count;
                        }
                    }
                }
            }
        }
        count
    }));
    res.unwrap_or(0)
}

// ============================================================================
// P5: 차트·그림·도형 오버레이 C ABI 4종 (모두 catch_unwind 보호)
// ============================================================================

#[no_mangle]
pub extern "C" fn as_sheet_overlay_count(book: *const ASBook, sheet: u32) -> u32 {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() {
            return 0;
        }
        let b = unsafe { &*book };
        b.overlays
            .get(sheet as usize)
            .map(|v| v.len() as u32)
            .unwrap_or(0)
    }));
    res.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn as_sheet_overlay_info(
    book: *const ASBook,
    sheet: u32,
    overlay_idx: u32,
    out: *mut ASOverlayInfo,
) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || out.is_null() {
            return false;
        }
        let b = unsafe { &*book };
        let Some(ov) = b
            .overlays
            .get(sheet as usize)
            .and_then(|list| list.get(overlay_idx as usize))
        else {
            return false;
        };

        let info = unsafe { &mut *out };
        info.kind = ov.kind;
        info.frame_pt = ov.frame_pt;
        info.start_row = ov.start_row;
        info.start_col = ov.start_col;
        info.pixel_width = ov.pixel_width;
        info.pixel_height = ov.pixel_height;
        info.crop_left = ov.crop.0;
        info.crop_top = ov.crop.1;
        info.crop_right = ov.crop.2;
        info.crop_bottom = ov.crop.3;
        info.rotation_deg = ov.rotation_deg;
        write_c_string_buf(&mut info.title, &ov.title);
        true
    }));
    res.unwrap_or(false)
}

#[no_mangle]
pub extern "C" fn as_sheet_overlay_drawlist(
    book: *const ASBook,
    sheet: u32,
    overlay_idx: u32,
) -> *mut ASDrawList {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() {
            return ptr::null_mut();
        }
        let b = unsafe { &*book };
        let Some(ov) = b
            .overlays
            .get(sheet as usize)
            .and_then(|list| list.get(overlay_idx as usize))
        else {
            return ptr::null_mut();
        };
        let Some(dl) = &ov.draw_list else {
            return ptr::null_mut();
        };
        let cloned = DrawList {
            commands: dl.commands.clone(),
            string_buf: dl.string_buf.clone(),
        };
        Box::into_raw(Box::new(ASDrawList { inner: cloned }))
    }));
    res.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn as_sheet_overlay_bytes(
    book: *const ASBook,
    sheet: u32,
    overlay_idx: u32,
    out_len: *mut u32,
) -> *const u8 {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() {
            if !out_len.is_null() {
                unsafe { *out_len = 0 };
            }
            return ptr::null();
        }
        let b = unsafe { &*book };
        let Some(ov) = b
            .overlays
            .get(sheet as usize)
            .and_then(|list| list.get(overlay_idx as usize))
        else {
            if !out_len.is_null() {
                unsafe { *out_len = 0 };
            }
            return ptr::null();
        };
        if !out_len.is_null() {
            unsafe { *out_len = ov.raw_bytes.len() as u32 };
        }
        if ov.raw_bytes.is_empty() {
            ptr::null()
        } else {
            ov.raw_bytes.as_ptr()
        }
    }));
    res.unwrap_or(ptr::null())
}

/// 시트 탭 색. ASSheetInfo 레이아웃을 바꾸지 않으려고 따로 둔다. 색이 없으면 false.
#[no_mangle]
pub extern "C" fn as_sheet_tab_color(book: *const ASBook, sheet: u32, out: *mut crate::draw::ASColor) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || out.is_null() {
            return false;
        }
        let b = unsafe { &*book };
        let Some(s) = b.wb.sheets.get(sheet as usize) else {
            return false;
        };
        let Some(rgba) = s.tab_color else {
            return false;
        };
        unsafe { *out = rgba.into() };
        true
    }));
    res.unwrap_or(false)
}

// ============================================================================
// P5 통합 시험: 차트 시트 계산값 연동, 10만 셀 성능, 망가진 파일 75개 방어
// ============================================================================

#[cfg(test)]
mod p2_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_p2_c_abi_axes_tile_cell_info_and_find() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let opts = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="01_시트" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#.as_bytes()).unwrap();

            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <cols><col min="1" max="19" width="15" customWidth="1"/></cols>
  <sheetData>
    <row r="1" ht="28"><c r="A1" t="inlineStr"><is><t>표준품셈 제목</t></is></c></row>
    <row r="60" ht="18"><c r="S60"><f>100+250</f><v/></c></row>
  </sheetData>
  <mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>
</worksheet>"#.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());

        // 1. as_sheet_axes 검증 (60행 x 19열)
        let mut c_ptr: *const f32 = ptr::null();
        let mut r_ptr: *const f32 = ptr::null();
        let mut n_cols = 0u32;
        let mut n_rows = 0u32;
        assert!(as_sheet_axes(book, 0, &mut c_ptr, &mut n_cols, &mut r_ptr, &mut n_rows));
        assert_eq!(n_cols, 19);
        assert_eq!(n_rows, 60);

        // 2. as_tile 그리기 목록 검증
        let dl = as_tile(book, 0, 0.0, 0.0, 256.0, 256.0, 2.0);
        assert!(!dl.is_null());
        assert!(as_drawlist_count(dl) > 2);
        as_drawlist_free(dl);

        // 3. as_cell_info 병합 자식(B1) 조회 시 마스터(A1:B1) 값 반환 검증
        let mut c_info: ASCellInfo = unsafe { std::mem::zeroed() };
        assert!(as_cell_info(book, 0, 1, 2, &mut c_info));
        assert_eq!(c_info.merge_start_col, 1);
        assert_eq!(c_info.merge_end_col, 2);
        let disp = unsafe { CStr::from_ptr(c_info.display_text.as_ptr()) }.to_str().unwrap();
        assert_eq!(disp, "표준품셈 제목");

        // 4. S60 수식 계산 결과(350) 및 as_find 검증
        assert!(as_cell_info(book, 0, 60, 19, &mut c_info));
        let s60_disp = unsafe { CStr::from_ptr(c_info.display_text.as_ptr()) }.to_str().unwrap();
        assert_eq!(s60_disp, "350");

        let query = CString::new("350").unwrap();
        let mut hits = [ASHit { sheet: 0, row: 0, col: 0 }; 4];
        let hit_cnt = as_find(book, query.as_ptr(), hits.as_mut_ptr(), 4);
        assert_eq!(hit_cnt, 1);
        assert_eq!((hits[0].row, hits[0].col), (60, 19));

        as_close(book);
    }
}

/// 검수 추가: 앱을 죽일 수 있는 입력으로 C ABI 를 두드린다.
#[cfg(test)]
mod p2_review_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn xlsx_with_sheet(sheet_xml: &str) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let o = SimpleFileOptions::default();
            zip.start_file("_rels/.rels", o).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();
            zip.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();
            zip.start_file("xl/workbook.xml", o).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#).unwrap();
            zip.start_file("xl/worksheets/sheet1.xml", o).unwrap();
            zip.write_all(sheet_xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        tmp
    }

    #[test]
    fn huge_merge_and_far_cell_do_not_explode() {
        let tmp = xlsx_with_sheet(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다라마바사아자차카타파하</t></is></c></row>
<row r="1048576"><c r="XFD1048576"><v>1</v></c></row></sheetData>
<mergeCells count="2"><mergeCell ref="A1:XFD1048576"/><mergeCell ref="C3:B2"/></mergeCells></worksheet>"#);
        let t0 = std::time::Instant::now();
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null(), "열기 실패: {:?}", unsafe { CStr::from_ptr(as_last_note()) });
        for (x, y) in [(0.0f32, 0.0f32), (1e9, 1e9), (-50.0, -50.0), (f32::NAN, f32::INFINITY)] {
            let dl = as_tile(book, 0, x, y, 256.0, 256.0, 2.0);
            as_drawlist_free(dl);
        }
        let mut info: ASCellInfo = unsafe { std::mem::zeroed() };
        for (r, c) in [(0u32, 0u32), (1, 1), (5000, 9000), (u32::MAX, u32::MAX)] {
            let _ = as_cell_info(book, 0, r, c, &mut info);
        }
        // 병합 안쪽 아무 칸이나 마스터(A1)로 간다
        assert!(as_cell_info(book, 0, 777, 777, &mut info));
        assert_eq!((info.merge_start_row, info.merge_start_col), (1, 1));
        // 긴 한글은 512 바이트에서 글자 경계로 잘린다(깨진 UTF-8 없음)
        let disp = unsafe { CStr::from_ptr(info.display_text.as_ptr()) };
        assert!(disp.to_str().is_ok(), "잘린 UTF-8");
        let mut hits = [ASHit::default(); 3];
        let q = CString::new("가나").unwrap();
        assert!(as_find(book, q.as_ptr(), hits.as_mut_ptr(), 3) >= 1);
        assert_eq!(as_sheet_info(book, 99, ptr::null_mut()), false);
        assert!(as_tile(book, 99, 0.0, 0.0, 1.0, 1.0, 1.0).is_null());
        as_close(book);
        assert!(t0.elapsed().as_secs() < 10, "너무 느림: {:?}", t0.elapsed());
    }

    #[test]
    fn hyperlink_whole_sheet_range_is_capped() {
        let tmp = xlsx_with_sheet(r##"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>a</t></is></c><c r="B1" t="str"><f>HYPERLINK("#'S'!C9","go")</f><v>go</v></c></row></sheetData>
<hyperlinks><hyperlink ref="A1:XFD1048576" location="S!B2"/><hyperlink ref="A:A" location="S!B3"/></hyperlinks></worksheet>"##);
        let t0 = std::time::Instant::now();
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());
        assert!(t0.elapsed().as_secs() < 5, "링크 범위 펼치기가 너무 느림 {:?}", t0.elapsed());
        let mut info: ASCellInfo = unsafe { std::mem::zeroed() };
        assert!(as_cell_info(book, 0, 1, 2, &mut info));
        let link = unsafe { CStr::from_ptr(info.link_target.as_ptr()) }.to_str().unwrap().to_string();
        assert!(info.link_is_internal);
        assert!(!link.is_empty());
        as_close(book);
    }

    #[test]
    fn conditional_format_whole_sheet_ranges_are_bounded() {
        // 시트 전체에 걸린 조건부 서식 4종 + 맨 끝 칸. 칸마다 돌면 170억 칸이다.
        let tmp = xlsx_with_sheet(r##"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>-5</v></c></row><row r="2"><c r="A2"><v>9</v></c></row>
<row r="1048576"><c r="XFD1048576"><v>1</v></c></row></sheetData>
<autoFilter ref="A1:XFD1048576"/>
<conditionalFormatting sqref="A1:XFD1048576"><cfRule type="expression" priority="1"><formula>SUM($A:$A)&gt;A1</formula></cfRule>
<cfRule type="dataBar" priority="2"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF3B82F6"/></dataBar></cfRule>
<cfRule type="colorScale" priority="3"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFFF0000"/><color rgb="FFFFFFFF"/><color rgb="FF00FF00"/></colorScale></cfRule>
<cfRule type="iconSet" priority="4"><iconSet iconSet="5Arrows"><cfvo type="percent" val="0"/><cfvo type="percent" val="20"/><cfvo type="percent" val="40"/><cfvo type="percent" val="60"/><cfvo type="percent" val="80"/></iconSet></cfRule>
<cfRule type="top10" priority="5" rank="4000000000"/><cfRule type="duplicateValues" priority="6"/><cfRule type="cellIs" operator="between" priority="7"><formula>1/0</formula><formula>((((</formula></cfRule>
</conditionalFormatting>
<conditionalFormatting sqref="XFD1048576:A1 ZZZ9 A0"><cfRule type="iconSet" priority="9"><iconSet iconSet="없는집합"/></cfRule><cfRule type="dataBar" priority="8"/></conditionalFormatting>
</worksheet>"##);
        let t0 = std::time::Instant::now();
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null(), "열기 실패: {:?}", unsafe { CStr::from_ptr(as_last_note()) });
        let dl = as_tile(book, 0, 0.0, 0.0, 256.0, 256.0, 1.0);
        assert!(!dl.is_null());
        as_drawlist_free(dl);
        as_close(book);
        assert!(t0.elapsed().as_secs() < 20, "조건부 서식 범위가 너무 오래 걸림: {:?}", t0.elapsed());
    }

    #[test]
    fn icon_set_with_thresholds_only_as_in_sample_06() {
        // 샘플 06 시트 F열 그대로: 3Arrows 인데 cfvo 가 하한 두 개뿐이다.
        let tmp = xlsx_with_sheet(r##"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1"><v>-20</v></c></row><row r="2"><c r="A2"><v>0</v></c></row><row r="3"><c r="A3"><v>15</v></c></row></sheetData>
<conditionalFormatting sqref="A1:A3"><cfRule type="iconSet" priority="3"><iconSet iconSet="3Arrows" showValue="1" reverse="0"><cfvo type="num" val="-15"/><cfvo type="num" val="15"/></iconSet></cfRule></conditionalFormatting>
</worksheet>"##);
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());
        let dl = as_tile(book, 0, 0.0, 0.0, 256.0, 256.0, 1.0);
        let n = as_drawlist_count(dl) as usize;
        let cmds = unsafe { std::slice::from_raw_parts(as_drawlist_commands(dl), n) };
        let icons: Vec<u8> = cmds.iter().filter(|c| c.cmd_type == 7).map(|c| c.align_horiz).collect();
        assert_eq!(icons, vec![0, 1, 2], "아래·옆·위 순서");
        as_drawlist_free(dl);
        as_close(book);
    }

    #[test]
    fn overlays_survive_hostile_charts_images_and_zip_bombs() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let o = SimpleFileOptions::default();
            let put = |zip: &mut ZipWriter<&mut std::fs::File>, name: &str, body: &[u8]| {
                zip.start_file(name, o).unwrap();
                zip.write_all(body).unwrap();
            };
            put(&mut zip, "_rels/.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
            put(&mut zip, "xl/_rels/workbook.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#);
            put(&mut zip, "xl/workbook.xml", br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#);
            put(&mut zip, "xl/worksheets/_rels/sheet1.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="d1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#);
            put(&mut zip, "xl/worksheets/sheet1.xml", br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1"><v>1</v></c></row><row r="1048576"><c r="XFD1048576"><v>2</v></c></row></sheetData><drawing r:id="d1"/></worksheet>"#);
            put(&mut zip, "xl/drawings/_rels/drawing1.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="c1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/><Relationship Id="i1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/bomb.png"/><Relationship Id="i2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/bad.emf"/></Relationships>"#);
            let anchor = |inner: &str| format!(r#"<xdr:twoCellAnchor><xdr:from><xdr:col>4294967295</xdr:col><xdr:colOff>-9223372036854775808</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>99999999</xdr:col><xdr:colOff>9223372036854775807</xdr:colOff><xdr:row>4000000000</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{}</xdr:twoCellAnchor>"#, inner);
            let drawing = format!(r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{}{}{}</xdr:wsDr>"#,
                anchor(r#"<xdr:graphicFrame><a:graphic><a:graphicData><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="c1"/></a:graphicData></a:graphic></xdr:graphicFrame>"#),
                anchor(r#"<xdr:pic><xdr:blipFill><a:blip r:embed="i1"/><a:srcRect l="-99999999" r="99999999"/></xdr:blipFill></xdr:pic>"#),
                anchor(r#"<xdr:pic><xdr:blipFill><a:blip r:embed="i2"/></xdr:blipFill><xdr:spPr><a:xfrm rot="2147483647"/></xdr:spPr></xdr:pic>"#));
            put(&mut zip, "xl/drawings/drawing1.xml", drawing.as_bytes());
            put(&mut zip, "xl/charts/chart1.xml", r#"<?xml version="1.0"?><c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:lineChart><c:ser><c:cat><c:numRef><c:f>S!$A$1:$XFD$1048576</c:f></c:numRef></c:cat><c:val><c:numRef><c:f>S!$A$1:$XFD$1048576</c:f></c:numRef></c:val></c:ser></c:lineChart><c:barChart><c:ser><c:val><c:numRef><c:f>'없는 시트'!A1:B2,S!ZZZZ1</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#.as_bytes());
            // 압축하면 작지만 풀면 70MB 인 그림(압축 폭탄)
            zip.start_file("xl/media/bomb.png", SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
            let zeros = vec![0u8; 1 << 20];
            for _ in 0..70 { zip.write_all(&zeros).unwrap(); }
            // 경계값이 i32 끝인 EMF
            let mut emf = vec![0u8; 108];
            emf[0..4].copy_from_slice(&1u32.to_le_bytes());
            emf[4..8].copy_from_slice(&108u32.to_le_bytes());
            emf[8..12].copy_from_slice(&i32::MIN.to_le_bytes());
            emf[16..20].copy_from_slice(&i32::MAX.to_le_bytes());
            emf[40..44].copy_from_slice(&0x464D4520u32.to_le_bytes());
            let mut rect = vec![0u8; 24];
            rect[0..4].copy_from_slice(&43u32.to_le_bytes());
            rect[4..8].copy_from_slice(&24u32.to_le_bytes());
            rect[8..12].copy_from_slice(&i32::MIN.to_le_bytes());
            rect[16..20].copy_from_slice(&i32::MAX.to_le_bytes());
            emf.extend_from_slice(&rect);
            put(&mut zip, "xl/media/bad.emf", &emf);
            zip.finish().unwrap();
        }
        let t0 = std::time::Instant::now();
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null(), "열기 실패: {:?}", unsafe { CStr::from_ptr(as_last_note()) });
        let n = as_sheet_overlay_count(book, 0);
        for i in 0..n + 2 {
            let mut oi: ASOverlayInfo = unsafe { std::mem::zeroed() };
            let _ = as_sheet_overlay_info(book, 0, i, &mut oi);
            assert!(oi.frame_pt.x.is_finite() && oi.frame_pt.w.is_finite());
            let dl = as_sheet_overlay_drawlist(book, 0, i);
            as_drawlist_free(dl);
            let mut len = 0u32;
            let _ = as_sheet_overlay_bytes(book, 0, i, &mut len);
            assert!(len < 64 * 1024 * 1024, "압축 폭탄이 그대로 들어옴: {}", len);
        }
        assert_eq!(as_sheet_overlay_count(book, 9), 0);
        assert!(as_sheet_overlay_drawlist(ptr::null(), 0, 0).is_null());
        as_close(book);
        assert!(t0.elapsed().as_secs() < 20, "오버레이 처리가 너무 오래 걸림: {:?}", t0.elapsed());
    }

    #[test]
    fn many_anchors_share_one_image_and_are_capped() {
        // 같은 그림을 가리키는 앵커 5,000개: 그림은 한 번만 읽어 함께 쓰고, 시트당 1,000개에서 자른다.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let o = SimpleFileOptions::default();
            let put = |zip: &mut ZipWriter<&mut std::fs::File>, name: &str, body: &[u8]| {
                zip.start_file(name, o).unwrap();
                zip.write_all(body).unwrap();
            };
            put(&mut zip, "_rels/.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
            put(&mut zip, "xl/_rels/workbook.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#);
            put(&mut zip, "xl/workbook.xml", br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#);
            put(&mut zip, "xl/worksheets/_rels/sheet1.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="d1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#);
            put(&mut zip, "xl/worksheets/sheet1.xml", br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="d1"/></worksheet>"#);
            put(&mut zip, "xl/drawings/_rels/drawing1.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="i1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/logo.png"/></Relationships>"#);
            let mut d = String::from(r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#);
            for i in 0..5000 {
                d.push_str(&format!(r#"<xdr:twoCellAnchor><xdr:from><xdr:col>{c}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{c2}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r2}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:blipFill><a:blip r:embed="i1"/></xdr:blipFill></xdr:pic></xdr:twoCellAnchor>"#, c = i % 50, r = i / 50, c2 = i % 50 + 1, r2 = i / 50 + 1));
            }
            d.push_str("</xdr:wsDr>");
            put(&mut zip, "xl/drawings/drawing1.xml", d.as_bytes());
            put(&mut zip, "xl/media/logo.png", &vec![7u8; 1 << 20]);
            zip.finish().unwrap();
        }
        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());
        let n = as_sheet_overlay_count(book, 0);
        assert_eq!(n as usize, crate::drawing::MAX_DRAWINGS_PER_SHEET);
        let mut len0 = 0u32;
        let p0 = as_sheet_overlay_bytes(book, 0, 0, &mut len0);
        assert_eq!(len0, 1 << 20);
        let mut len1 = 0u32;
        let p1 = as_sheet_overlay_bytes(book, 0, n - 1, &mut len1);
        assert_eq!(p0, p1, "같은 그림을 앵커마다 따로 복사함");
        as_close(book);
    }

    #[test]
    fn null_and_garbage_pointers_are_refused() {
        assert!(as_open(ptr::null()).is_null());
        let bad = CString::new("/없는/파일.xlsx").unwrap();
        assert!(as_open(bad.as_ptr()).is_null());
        assert_eq!(as_sheet_count(ptr::null()), 0);
        assert!(as_tile(ptr::null(), 0, 0.0, 0.0, 1.0, 1.0, 1.0).is_null());
        as_drawlist_free(ptr::null_mut());
        as_close(ptr::null_mut());
        assert_eq!(as_drawlist_count(ptr::null()), 0);
        let mut n = 7u32;
        assert!(as_drawlist_strings(ptr::null(), &mut n).is_null());
        assert_eq!(n, 0);
    }
}

// ---- P0·P1-a 시험 (P2 납품에서 지워져 복원) ----
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::shift_formula_references;
    use crate::style::{apply_color_tint, excel_column_width_to_points_with_mdw, Rgba};
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_cell_size_is_16_bytes() {
        assert_eq!(std::mem::size_of::<CellPayload>(), 8);
        assert_eq!(std::mem::size_of::<Cell>(), 16);
    }

    #[test]
    fn test_shared_formula_shifting_without_corrupting_log10() {
        // LOG10 함수 이름과 문자열 리터럴 안의 "A1", 작은따옴표 시트 이름은 바뀌지 않아야 한다.
        let master = r#"LOG10(A2)+$B$2+C$2+$D2+'01_A1'!E2+"A2""#;
        let shifted = shift_formula_references(master, 3, 2);
        assert_eq!(
            shifted,
            r#"LOG10(C5)+$B$2+E$2+$D5+'01_A1'!G5+"A2""#
        );
    }

    #[test]
    fn test_openpyxl_empty_v_inline_str_and_p1a_features() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let valid_bytes = build_sample_xlsx_bytes();
        tmp.write_all(&valid_bytes).unwrap();
        tmp.flush().unwrap();

        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());
        assert_eq!(as_sheet_count(book), 1);

        let mut info: ASSheetInfo = unsafe { std::mem::zeroed() };
        assert!(as_sheet_info(book, 0, &mut info));
        assert_eq!(info.row_count, 5);
        assert_eq!(info.col_count, 5);
        assert_eq!(info.formula_count, 3); // 마스터 1 + 공유 자식 1 + 빈 <v/> 수식 1
        assert_eq!(info.merge_count, 1);
        assert_eq!(info.validation_count, 1);
        assert_eq!(info.comment_count, 1);
        assert!(info.is_frozen);
        assert_eq!(info.freeze_rows, 4);
        assert_eq!(info.freeze_cols, 4);
        assert_eq!(info.top_left_row, 5);
        assert_eq!(info.top_left_col, 5);

        let wb = unsafe { &(*book).wb };
        assert!(wb.full_calc_on_load);
        assert_eq!(wb.defined_names.len(), 1);

        let sheet = &wb.sheets[0];
        // 03 시트 함정: 앞뒤 공백 보존 확인
        let cell_a1 = sheet.data.get_cell(1, 1).unwrap();
        assert_eq!(sheet.raw_value_str(&wb.pool, cell_a1), "  앞뒤공백보존  ");

        // 공유 수식 자식 셀(E3)의 수식 문자열 복원 확인
        assert_eq!(sheet.formula_str(&wb.pool, 2, 5), "C2+D2");
        assert_eq!(sheet.formula_str(&wb.pool, 3, 5), "C3+D3");

        // 02 시트 함정: 데이터 유효성 목록 수식 앞의 '=' 제거 확인
        assert_eq!(
            sheet.validations[0].formula1.as_deref(),
            Some("'01_시트'!$A$5:$A$64")
        );

        as_close(book);
    }

    #[test]
    fn test_corrupted_files_10_cases_zero_panic() {
        let valid = build_sample_xlsx_bytes();

        let mut cases: Vec<Vec<u8>> = Vec::with_capacity(10);
        // 1. 앞 100바이트 삭제
        cases.push(if valid.len() > 100 { valid[100..].to_vec() } else { vec![] });
        // 2. 중간 1KB 비트 뒤집기
        let mut mid_flipped = valid.clone();
        let mid_start = mid_flipped.len() / 3;
        let mid_end = (mid_start + 1024).min(mid_flipped.len());
        for b in &mut mid_flipped[mid_start..mid_end] {
            *b ^= 0xFF;
        }
        cases.push(mid_flipped);
        // 3. 끝 128바이트 잘라내기 (Central Directory 파손)
        cases.push(valid[..valid.len().saturating_sub(128)].to_vec());
        // 4. 0바이트 빈 파일
        cases.push(Vec::new());
        // 5. 무작위 비-ZIP 바이트
        cases.push(vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22, 0x33]);
        // 6. PK 로컬 헤더만 있고 잘린 파일
        cases.push(b"PK\x03\x04\x14\x00\x00\x00\x08\x00".to_vec());
        // 7. workbook.xml 태그가 중간에 끊긴 ZIP
        cases.push(build_broken_zip("xl/workbook.xml", b"<workbook><sheets><sheet name=\"A\""));
        // 8. sheet1.xml에 잘못된 UTF-8 바이트가 들어간 ZIP
        cases.push(build_broken_zip(
            "xl/worksheets/sheet1.xml",
            b"<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is><t>\xFF\xFE</t></is></c></row></sheetData>",
        ));
        // 9. sharedStrings.xml이 깨진 ZIP
        cases.push(build_broken_zip("xl/sharedStrings.xml", b"<sst><si><t>unclosed"));
        // 10. 마스터 없는 공유 수식 및 범위 초과 셀 참조가 있는 ZIP
        cases.push(build_broken_zip(
            "xl/worksheets/sheet1.xml",
            b"<worksheet><sheetData><row r=\"1\"><c r=\"ZZZ9999999\"><f t=\"shared\" si=\"99\"/></c></row></sheetData></worksheet>",
        ));

        assert_eq!(cases.len(), 10);

        for (idx, bytes) in cases.iter().enumerate() {
            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            tmp.write_all(bytes).unwrap();
            tmp.flush().unwrap();

            let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
            let book = as_open(c_path.as_ptr());
            if !book.is_null() {
                let count = as_sheet_count(book);
                for s in 0..count {
                    let mut info: ASSheetInfo = unsafe { std::mem::zeroed() };
                    let _ = as_sheet_info(book, s, &mut info);
                }
                as_close(book);
            } else {
                let note = unsafe { CStr::from_ptr(as_last_note()) }.to_string_lossy();
                assert!(!note.is_empty(), "case {} failed without setting note", idx);
            }
        }
    }

    #[test]
    fn test_theme_tint_and_column_width_pt() {
        let base = Rgba::from_hex("4472C4").unwrap();
        let lighter = apply_color_tint(base, 0.4);
        assert!(lighter.r > base.r && lighter.g > base.g && lighter.b > base.b);

        let pt_13 = excel_column_width_to_points_with_mdw(13.0, 7.0);
        let pt_44 = excel_column_width_to_points_with_mdw(44.0, 7.0);
        assert!(pt_13 > 60.0 && pt_13 < 80.0);
        assert!(pt_44 > 220.0 && pt_44 < 250.0);
    }

    fn build_sample_xlsx_bytes() -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="01_시트" sheetId="1" r:id="rId1"/></sheets>
  <definedNames><definedName name="TaxRate">0.1</definedName></definedNames>
  <calcPr fullCalcOnLoad="1"/>
</workbook>"#.as_bytes()).unwrap();

            zip.start_file("xl/worksheets/_rels/sheet1.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/comments1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <authors><author>Rob</author></authors>
  <commentList><comment ref="A1" authorId="0"><text><t>검수 메모</t></text></comment></commentList>
</comments>"#.as_bytes()).unwrap();

            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetViews><sheetView workbookViewId="0"><pane xSplit="4" ySplit="4" topLeftCell="E5" state="frozen"/></sheetView></sheetViews>
  <cols><col min="1" max="5" width="18" customWidth="1"/></cols>
  <sheetData>
    <row r="1" ht="28" customHeight="1">
      <c r="A1" t="inlineStr"><is><t xml:space="preserve">  앞뒤공백보존  </t></is></c>
    </row>
    <row r="2" ht="18" customHeight="1">
      <c r="C2"><v>10</v></c>
      <c r="D2"><v>20</v></c>
      <c r="E2"><f t="shared" ref="E2:E3" si="0">C2+D2</f></c>
    </row>
    <row r="3">
      <c r="E3"><f t="shared" si="0"/></c>
    </row>
    <row r="5">
      <c r="E5"><f>SUM(E2:E3)</f><v/></c>
    </row>
  </sheetData>
  <mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>
  <dataValidations count="1">
    <dataValidation type="list" sqref="B2:B10"><formula1>='01_시트'!$A$5:$A$64</formula1></dataValidation>
  </dataValidations>
</worksheet>"#.as_bytes()).unwrap();

            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn build_broken_zip(entry_name: &str, content: &[u8]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            zip.start_file(entry_name, opts).unwrap();
            zip.write_all(content).unwrap();
            let _ = zip.finish();
        }
        cursor.into_inner()
    }
}


/// C 헤더(argosheet.h)와 Rust 구조체 배치가 같은지 — 값은 gcc 로 헤더를 컴파일해 잰 것.
/// 헤더나 구조체를 바꾸면 이 시험이 깨진다. 둘을 같이 고칠 것.
#[cfg(test)]
mod abi_layout {
    #[test]
    fn c_header_layout_matches() {
        use crate::draw::*;
        use crate::*;
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<ASSheetInfo>(), 328);
        assert_eq!(size_of::<ASCellInfo>(), 2384);
        assert_eq!(size_of::<ASDrawCmd>(), 64);
        assert_eq!(size_of::<ASHit>(), 12);
        assert_eq!(offset_of!(ASSheetInfo, state), 260);
        assert_eq!(offset_of!(ASSheetInfo, row_count), 264);
        assert_eq!(offset_of!(ASSheetInfo, is_frozen), 292);
        assert_eq!(offset_of!(ASSheetInfo, show_grid_lines), 312);
        assert_eq!(offset_of!(ASSheetInfo, total_height_pt), 320);
        assert_eq!(offset_of!(ASCellInfo, kind), 24);
        assert_eq!(offset_of!(ASCellInfo, num_value), 32);
        assert_eq!(offset_of!(ASCellInfo, address), 40);
        assert_eq!(offset_of!(ASCellInfo, display_text), 72);
        assert_eq!(offset_of!(ASCellInfo, comment_text), 1224);
        assert_eq!(offset_of!(ASCellInfo, comment_author), 1736);
        assert_eq!(offset_of!(ASCellInfo, link_target), 1864);
        assert_eq!(offset_of!(ASCellInfo, link_is_internal), 2376);
        assert_eq!(offset_of!(ASDrawCmd, rect), 4);
        assert_eq!(offset_of!(ASDrawCmd, clip_rect), 20);
        assert_eq!(offset_of!(ASDrawCmd, color1), 36);
        assert_eq!(offset_of!(ASDrawCmd, color2), 40);
        assert_eq!(offset_of!(ASDrawCmd, stroke_width), 44);
        assert_eq!(offset_of!(ASDrawCmd, text_offset), 48);
        assert_eq!(offset_of!(ASDrawCmd, text_len), 52);
        assert_eq!(offset_of!(ASDrawCmd, font_size_pt), 56);
        assert_eq!(offset_of!(ASDrawCmd, rotation), 60);
        assert_eq!(offset_of!(ASDrawCmd, indent), 62);
        assert_eq!(offset_of!(ASDrawCmd, text_flags), 63);
        assert_eq!(size_of::<ASOverlayInfo>(), 312);
        assert_eq!(offset_of!(ASSheetInfo, overlay_count), 324);
        assert_eq!(offset_of!(ASOverlayInfo, frame_pt), 4);
        assert_eq!(offset_of!(ASOverlayInfo, start_row), 20);
        assert_eq!(offset_of!(ASOverlayInfo, pixel_width), 28);
        assert_eq!(offset_of!(ASOverlayInfo, crop_left), 36);
        assert_eq!(offset_of!(ASOverlayInfo, rotation_deg), 52);
        assert_eq!(offset_of!(ASOverlayInfo, title), 56);
    }
}

// ---- 제미나이 P4 통합 시험 ----
#[cfg(test)]
mod p4_integration_tests {
    use super::*;
    use crate::draw::DrawCmdType;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_p4_end_to_end_databar_colorscale_iconset_and_table_filter() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let opts = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="01_P4" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#).unwrap();

            zip.start_file("xl/worksheets/_rels/sheet1.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdTbl1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/tables/table1.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Tbl1" displayName="Tbl1" ref="A1:C3" headerRowCount="1">
  <autoFilter ref="A1:C3"/>
  <tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/>
</table>"#).unwrap();

            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>항목</t></is></c>
      <c r="B1" t="inlineStr"><is><t>달성률</t></is></c>
      <c r="C1" t="inlineStr"><is><t>증감</t></is></c>
    </row>
    <row r="2">
      <c r="A2"><v>-10</v></c>
      <c r="B2"><v>0.75</v></c>
      <c r="C2"><v>-20</v></c>
    </row>
    <row r="3">
      <c r="A3"><v>20</v></c>
      <c r="B3"><v>0.25</v></c>
      <c r="C3"><v>30</v></c>
    </row>
  </sheetData>
  <conditionalFormatting sqref="A2:A3">
    <cfRule type="colorScale" priority="1">
      <colorScale>
        <cfvo type="min"/><cfvo type="num" val="0"/><cfvo type="max"/>
        <color rgb="FFFCA5A5"/><color rgb="FFFFFFFF"/><color rgb="FF86EFAC"/>
      </colorScale>
    </cfRule>
  </conditionalFormatting>
  <conditionalFormatting sqref="B2:B3">
    <cfRule type="dataBar" priority="2">
      <dataBar>
        <cfvo type="num" val="0"/><cfvo type="num" val="1"/>
        <color rgb="FF0F766E"/>
      </dataBar>
    </cfRule>
  </conditionalFormatting>
  <conditionalFormatting sqref="C2:C3">
    <cfRule type="iconSet" priority="3">
      <iconSet iconSet="3Arrows" showValue="1">
        <cfvo type="percent" val="0"/>
        <cfvo type="num" val="-15"/>
        <cfvo type="num" val="15"/>
      </iconSet>
    </cfRule>
  </conditionalFormatting>
</worksheet>"#.as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());

        let dl = as_tile(book, 0, 0.0, 0.0, 256.0, 256.0, 2.0);
        assert!(!dl.is_null());

        let cnt = as_drawlist_count(dl) as usize;
        let cmds = unsafe { std::slice::from_raw_parts(as_drawlist_commands(dl), cnt) };

        // 1. 데이터 막대(AS_CMD_BAR) 명령 생성 확인
        assert!(cmds.iter().any(|c| c.cmd_type == DrawCmdType::Bar as u8));
        // 2. 3Arrows 벡터 아이콘(AS_CMD_ICON) 명령 생성 확인 (하락 인덱스 0, 상승 인덱스 2)
        assert!(cmds.iter().any(|c| c.cmd_type == DrawCmdType::Icon as u8 && c.align_horiz == 0));
        assert!(cmds.iter().any(|c| c.cmd_type == DrawCmdType::Icon as u8 && c.align_horiz == 2));
        // 3. 표 머리글 필터 단추 마커(AS_CMD_MARKER, sub_type == 1) 생성 확인
        assert!(cmds.iter().any(|c| c.cmd_type == DrawCmdType::Marker as u8 && c.sub_type == 1));

        as_drawlist_free(dl);
        as_close(book);
    }
}

// ---- 제미나이 P5 시험 ----
#[cfg(test)]
mod p5_tests {
    use super::*;
    use crate::draw::DrawCmdType;
    use std::io::Write;
    use std::time::Instant;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_p5_chart_pulls_calculated_sheet_values_when_cache_is_empty() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let opts = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets><sheet name="00_Overview" sheetId="1" r:id="rId1"/></sheets>
</workbook>"#).unwrap();

            zip.start_file("xl/worksheets/_rels/sheet1.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdDrw1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/drawings/_rels/drawing1.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rIdChart1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart1.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/drawings/drawing1.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <xdr:twoCellAnchor>
    <xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
    <xdr:to><xdr:col>8</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>18</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
    <xdr:graphicFrame>
      <a:graphic><a:graphicData><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rIdChart1"/></a:graphicData></a:graphic>
    </xdr:graphicFrame>
  </xdr:twoCellAnchor>
</xdr:wsDr>"#).unwrap();

            // numCache가 전혀 없는 openpyxl 스타일 차트 XML
            zip.start_file("xl/charts/chart1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">
  <c:chart>
    <c:title><c:tx><c:rich><a:p xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:r><a:t>사업부별 실적</a:t></a:r></a:p></c:rich></c:tx></c:title>
    <c:plotArea>
      <c:barChart>
        <c:barDir val="col"/>
        <c:grouping val="clustered"/>
        <c:ser>
          <c:idx val="0"/>
          <c:cat><c:strRef><c:f>'00_Overview'!$A$2:$A$4</c:f></c:strRef></c:cat>
          <c:val><c:numRef><c:f>'00_Overview'!$B$2:$B$4</c:f></c:numRef></c:val>
        </c:ser>
        <c:axId val="10"/><c:axId val="20"/>
      </c:barChart>
    </c:plotArea>
  </c:chart>
</c:chartSpace>"#.as_bytes()).unwrap();

            // B2:B4는 <v/>가 비어 있는 수식 셀 (수식 엔진이 100, 200, 300으로 계산)
            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheetData>
    <row r="2"><c r="A2" t="inlineStr"><is><t>1분기</t></is></c><c r="B2"><f>50*2</f><v/></c></row>
    <row r="3"><c r="A3" t="inlineStr"><is><t>2분기</t></is></c><c r="B3"><f>B2*2</f><v/></c></row>
    <row r="4"><c r="A4" t="inlineStr"><is><t>3분기</t></is></c><c r="B4"><f>B2+B3</f><v/></c></row>
  </sheetData>
  <drawing r:id="rIdDrw1"/>
</worksheet>"#.as_bytes()).unwrap();

            zip.finish().unwrap();
        }

        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());

        assert_eq!(as_sheet_overlay_count(book, 0), 1);
        let mut ov_info: ASOverlayInfo = unsafe { std::mem::zeroed() };
        assert!(as_sheet_overlay_info(book, 0, 0, &mut ov_info));
        assert_eq!(ov_info.kind, 0); // AS_OVERLAY_CHART
        assert!(ov_info.frame_pt.w > 100.0 && ov_info.frame_pt.h > 80.0);

        let dl = as_sheet_overlay_drawlist(book, 0, 0);
        assert!(!dl.is_null());
        let cmd_cnt = as_drawlist_count(dl) as usize;
        let cmds = unsafe { std::slice::from_raw_parts(as_drawlist_commands(dl), cmd_cnt) };
        // 배경 1개 + 막대 3개 이상 Fill 명령이 생성되어야 한다
        let fill_count = cmds.iter().filter(|c| c.cmd_type == DrawCmdType::Fill as u8).count();
        assert!(fill_count >= 4);

        as_drawlist_free(dl);
        as_close(book);
    }

    #[test]
    fn test_p5_100k_cells_first_screen_under_1500ms() {
        let start_time = Instant::now();
        let mut pool = StringPool::default();
        let mut rows = Rows::default();

        // 10,000행 x 10열 = 100,000셀 시트 구성
        for r in 1..=10_000u32 {
            let mut row_cells = Vec::with_capacity(10);
            for c in 1..=10u32 {
                row_cells.push(Cell {
                    col: c,
                    kind: CellKind::Num as u8,
                    flags: 0,
                    style_idx: 0,
                    payload: CellPayload {
                        num: (r * 10 + c) as f64,
                    },
                });
            }
            rows.push_row(r, &row_cells);
        }

        let sheet = Sheet {
            name: "100k_Sheet".to_string(),
            sheet_id: 1,
            state: SheetState::Visible,
            max_row: 10_000,
            max_col: 10,
            cell_count: 100_000,
            col_widths_pt: vec![64.0; 10],
            row_heights_pt: vec![16.0; 10_000],
            default_col_width_pt: 64.0,
            default_row_height_pt: 16.0,
            data: rows,
            formulas: HashMap::new(),
            merges: Vec::new(),
            freeze: Some(FreezePane {
                freeze_rows: 1,
                freeze_cols: 1,
                top_left_row: 2,
                top_left_col: 2,
            }),
            show_grid_lines: true,
            auto_filter: None,
            validations: Vec::new(),
            comments: Vec::new(),
            links: Vec::new().into_iter().collect(),
            cf_blocks: Vec::new(),
            tables: Vec::new(),
            raw_drawings: Vec::new(),
            tab_color: None,
        };

        let wb = Workbook {
            path: String::new(),
            date1904: false,
            full_calc_on_load: false,
            defined_names: Vec::new(),
            styles: StyleSheet::default_with_theme(Vec::new()),
            pool,
            sheets: vec![sheet],
        };

        let calc_res = evaluate_workbook(&wb);
        let fmt_cache = FormatCache::build(&wb);
        let rc = SheetRenderCache::build(&wb.sheets[0]);
        let cfc = build_sheet_cf_cache(&wb, &calc_res, 0);

        // 첫 화면 4K 타일 6장 렌더링
        for ty in 0..2 {
            for tx in 0..3 {
                let dl = build_tile_drawlist(
                    &wb,
                    &calc_res,
                    &fmt_cache,
                    &rc,
                    &cfc,
                    0,
                    ASRect {
                        x: (tx as f32) * 256.0,
                        y: (ty as f32) * 256.0,
                        w: 256.0,
                        h: 256.0,
                    },
                    2.0,
                );
                assert!(!dl.commands.is_empty());
            }
        }

        let elapsed = start_time.elapsed();
        assert!(
            elapsed.as_millis() < 1500,
            "10만 셀 첫 화면 생성 시간이 1.5초를 초과했습니다: {:?}",
            elapsed
        );
    }

    #[test]
    fn test_p5_75_corrupted_files_zero_crash_zero_hang() {
        let base_valid = build_valid_seed_xlsx();
        let mut corpus: Vec<Vec<u8>> = Vec::with_capacity(75);

        // 1~20: 앞부분 삭제 변형 (5바이트 ~ 100바이트 간격 20종)
        for i in 1..=20 {
            let cut = (i * 12).min(base_valid.len());
            corpus.push(base_valid[cut..].to_vec());
        }

        // 21~40: 끝부분 잘라내기 변형 (Central Directory 및 Deflate 스트림 절단 20종)
        for i in 1..=20 {
            let keep = base_valid.len().saturating_sub(i * 18);
            corpus.push(base_valid[..keep].to_vec());
        }

        // 41~60: 중간 바이트 뒤집기 변형 (서로 다른 오프셋 구간 20종)
        for i in 0..20 {
            let mut mutated = base_valid.clone();
            let start = (i * 37) % mutated.len().max(1);
            let end = (start + 64).min(mutated.len());
            for b in &mut mutated[start..end] {
                *b ^= 0xAA;
            }
            corpus.push(mutated);
        }

        // 61~75: 구조적 XML 파손·폭탄·비정상 참조·잘못된 드로잉/차트/EMF ZIP 15종
        let broken_xml_entries: [(&str, &[u8]); 15] = [
            ("xl/workbook.xml", b""),
            ("xl/workbook.xml", b"<workbook><sheets>"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><sheetData><row r=\"999999999\">"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><cols><col min=\"1\" max=\"99999999\" width=\"-50\"/></cols></worksheet>"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><f>((((((1+2</f></c></row></sheetData></worksheet>"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><f>A1</f></c></row></sheetData></worksheet>"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><conditionalFormatting sqref=\"INVALID\"><cfRule type=\"dataBar\"/></conditionalFormatting></worksheet>"),
            ("xl/worksheets/sheet1.xml", b"<worksheet><mergeCells><mergeCell ref=\"Z99:A1\"/></mergeCells></worksheet>"),
            ("xl/styles.xml", b"<styleSheet><cellXfs><xf numFmtId=\"99999\" fontId=\"9999\"/></cellXfs></styleSheet>"),
            ("xl/sharedStrings.xml", b"<sst><si><t>\xFF\xFE\x00\x00</t></si></sst>"),
            ("xl/drawings/drawing1.xml", b"<wsDr><twoCellAnchor><from><col>-99</col></from></twoCellAnchor>"),
            ("xl/charts/chart1.xml", b"<chartSpace><chart><plotArea><barChart><ser><val><numRef><f>INVALID!REF</f></numRef></val></ser></barChart></plotArea></chart></chartSpace>"),
            ("xl/tables/table1.xml", b"<table ref=\"BAD_RANGE\"><autoFilter/></table>"),
            ("xl/comments1.xml", b"<comments><commentList><comment ref=\"$$$\"/></commentList>"),
            ("_rels/.rels", b"<Relationships><Relationship Id=\"r1\" Target=\"../../../etc/passwd\"/></Relationships>"),
        ];

        for (entry, payload) in broken_xml_entries {
            corpus.push(build_zip_with_single_override(entry, payload));
        }

        assert_eq!(corpus.len(), 75);

        for (idx, bytes) in corpus.iter().enumerate() {
            let mut tmp = tempfile::NamedTempFile::new().unwrap();
            tmp.write_all(bytes).unwrap();
            tmp.flush().unwrap();

            let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
            let t0 = Instant::now();
            let book = as_open(c_path.as_ptr());
            if !book.is_null() {
                let s_cnt = as_sheet_count(book);
                for s in 0..s_cnt {
                    let mut info: ASSheetInfo = unsafe { std::mem::zeroed() };
                    let _ = as_sheet_info(book, s, &mut info);
                    let dl = as_tile(book, s, 0.0, 0.0, 256.0, 256.0, 2.0);
                    if !dl.is_null() {
                        as_drawlist_free(dl);
                    }
                }
                as_close(book);
            }
            assert!(
                t0.elapsed().as_secs() < 5,
                "망가진 파일 #{} 처리 시간이 5초를 초과했습니다",
                idx
            );
        }
    }

    fn build_valid_seed_xlsx() -> Vec<u8> {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cur);
            let opts = SimpleFileOptions::default();
            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#).unwrap();
            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#).unwrap();
            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#).unwrap();
            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>42</v></c></row></sheetData></worksheet>"#).unwrap();
            zip.finish().unwrap();
        }
        cur.into_inner()
    }

    fn build_zip_with_single_override(override_name: &str, override_bytes: &[u8]) -> Vec<u8> {
        let mut cur = std::io::Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cur);
            let opts = SimpleFileOptions::default();
            let defaults: [(&str, &[u8]); 4] = [
                ("_rels/.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
                ("xl/_rels/workbook.xml.rels", br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rIdDrw" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="drawings/drawing1.xml"/></Relationships>"#),
                ("xl/workbook.xml", br#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S1" sheetId="1" r:id="rId1"/></sheets></workbook>"#),
                ("xl/worksheets/sheet1.xml", br#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#),
            ];
            for (name, content) in defaults {
                if name != override_name {
                    zip.start_file(name, opts).unwrap();
                    zip.write_all(content).unwrap();
                }
            }
            zip.start_file(override_name, opts).unwrap();
            zip.write_all(override_bytes).unwrap();
            let _ = zip.finish();
        }
        cur.into_inner()
    }
}

#[cfg(test)]
mod p6_tab_color_tests {
    use super::*;
    use crate::draw::ASColor;
    use crate::style::{apply_color_tint, Rgba};
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_as_sheet_tab_color_rgb_theme_tint_none_bounds_and_null() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = ZipWriter::new(tmp.as_file_mut());
            let opts = SimpleFileOptions::default();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/_rels/workbook.xml.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet3.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="RGB_Sheet" sheetId="1" r:id="rId1"/>
    <sheet name="Theme_Sheet" sheetId="2" r:id="rId2"/>
    <sheet name="NoColor_Sheet" sheetId="3" r:id="rId3"/>
  </sheets>
</workbook>"#).unwrap();

            // 시트 0: rgb="FF10B981" 직접 지정
            zip.start_file("xl/worksheets/sheet1.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><tabColor rgb="FF10B981"/></sheetPr>
  <sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>
</worksheet>"#).unwrap();

            // 시트 1: theme="4" (accent1 = #4472C4) + tint="0.4"
            zip.start_file("xl/worksheets/sheet2.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><tabColor theme="4" tint="0.4"/></sheetPr>
  <sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData>
</worksheet>"#).unwrap();

            // 시트 2: tabColor 없음
            zip.start_file("xl/worksheets/sheet3.xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData><row r="1"><c r="A1"><v>3</v></c></row></sheetData>
</worksheet>"#).unwrap();

            zip.finish().unwrap();
        }

        let c_path = CString::new(tmp.path().to_str().unwrap()).unwrap();
        let book = as_open(c_path.as_ptr());
        assert!(!book.is_null());

        let mut color = ASColor::default();

        // 1. RGB 직접 지정 확인 (#10B981)
        assert!(as_sheet_tab_color(book, 0, &mut color));
        assert_eq!((color.r, color.g, color.b, color.a), (0x10, 0xB9, 0x81, 0xFF));

        // 2. theme="4" + tint="0.4" 확인
        assert!(as_sheet_tab_color(book, 1, &mut color));
        let expected_tinted = apply_color_tint(Rgba::from_hex("4472C4").unwrap(), 0.4);
        assert_eq!(
            (color.r, color.g, color.b),
            (expected_tinted.r, expected_tinted.g, expected_tinted.b)
        );

        // 3. 탭 색상 없음 -> false
        assert!(!as_sheet_tab_color(book, 2, &mut color));

        // 4. 범위 밖 시트 인덱스 -> false
        assert!(!as_sheet_tab_color(book, 99, &mut color));

        // 5. 널 포인터 방어 -> 패닉 없이 false
        assert!(!as_sheet_tab_color(ptr::null(), 0, &mut color));
        assert!(!as_sheet_tab_color(book, 0, ptr::null_mut()));

        as_close(book);
    }
}
