pub mod calc;
pub mod draw;
pub mod numfmt;
pub mod package;
pub mod parse;
pub mod style;

use crate::calc::{evaluate_workbook, CellAddr, WorkbookCalcResult};
use crate::draw::{
    build_tile_drawlist, format_cell_display, ASDrawCmd, ASRect, DrawList, FormatCache,
    SheetRenderCache,
};
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
        let end = start.saturating_add(span.len as usize);
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

// ============================================================================
// C ABI 구조체 및 진입점 (모두 catch_unwind 보호, 읽기 전용 불변 캐시 내장)
// ============================================================================

pub struct ASBook {
    pub wb: Workbook,
    pub calc_res: WorkbookCalcResult,
    pub fmt_cache: FormatCache,
    pub render_caches: Vec<SheetRenderCache>,
}

// 왜: ASBook은 open 이후 절대 변경되지 않는 불변 구조체이므로 다중 백그라운드 타일 큐에서 안전하게 공유된다.
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
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ASHit {
    pub sheet: u32,
    pub row: u32,
    pub col: u32,
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
                let render_caches = wb.sheets.iter().map(SheetRenderCache::build).collect();
                Box::into_raw(Box::new(ASBook {
                    wb,
                    calc_res,
                    fmt_cache,
                    render_caches,
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
        let Some(s) = b.wb.sheets.get(index as usize) else {
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
        let tile_rect = ASRect { x, y, w, h };
        let inner = build_tile_drawlist(&b.wb, &b.calc_res, &b.fmt_cache, rc, s_idx, tile_rect, zoom);
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

        let comment = s
            .comments
            .iter()
            .find(|cm| cm.row == target_r && cm.col == target_c)
            .map(|cm| b.wb.pool.get(cm.text))
            .unwrap_or("");
        write_c_string_buf(&mut info.comment_text, comment);

        if let Some(c_data) = cell {
            let (disp, resolved_kind) = format_cell_display(
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
            info.num_value = if resolved_kind == CellKind::Num {
                if (c_data.flags & CELL_FLAG_HAS_FORMULA) != 0 {
                    b.calc_res
                        .values
                        .get(&CellAddr {
                            sheet: s_idx,
                            row: target_r,
                            col: target_c,
                        })
                        .and_then(|v| v.coerce_num().ok())
                        .unwrap_or(0.0)
                } else {
                    unsafe { c_data.payload.num }
                }
            } else {
                0.0
            };
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
                    let (disp, _) = format_cell_display(
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
        assert_eq!(size_of::<ASSheetInfo>(), 324);
        assert_eq!(size_of::<ASCellInfo>(), 1736);
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
        assert_eq!(offset_of!(ASDrawCmd, rect), 4);
        assert_eq!(offset_of!(ASDrawCmd, clip_rect), 20);
        assert_eq!(offset_of!(ASDrawCmd, color1), 36);
        assert_eq!(offset_of!(ASDrawCmd, stroke_width), 44);
        assert_eq!(offset_of!(ASDrawCmd, text_offset), 48);
        assert_eq!(offset_of!(ASDrawCmd, font_size_pt), 56);
        assert_eq!(offset_of!(ASDrawCmd, font_bold), 60);
        assert_eq!(offset_of!(ASDrawCmd, _pad), 63);
    }
}
