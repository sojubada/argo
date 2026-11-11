pub mod package;
pub mod parse;
pub mod style;

use crate::style::StyleSheet;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

// ============================================================================
// 1. 16바이트 고정 CSR 셀 모델 및 단일 연속 문자열 풀 (P0 + P1-a)
// ============================================================================

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

/// 문자열 버퍼 내 위치 (오프셋 u32, 바이트 길이 u32 -> 총 8바이트)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StrSpan {
    pub offset: u32,
    pub len: u32,
}

/// 셀 값 8바이트 공용체 (union)
/// 왜: 셀마다 String이나 Box를 두면 힙 단편화와 수백 바이트 오버헤드가 발생하므로
/// 숫자는 f64 즉시 저장, 문자열/오류는 단일 버퍼의 (offset, len) 8바이트로 고정한다.
#[repr(C)]
#[derive(Clone, Copy)]
pub union CellPayload {
    pub num: f64,
    pub str_span: StrSpan,
    pub boolean: u64,
    pub raw: u64,
}

/// 정확히 16바이트 고정 크기의 CSR 셀 구조체
/// col(4) + kind(1) + flags(1) + style_idx(2) + payload(8) = 16 bytes
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Cell {
    pub col: u32,          // 1-based 열 번호
    pub kind: u8,          // CellKind
    pub flags: u8,         // CELL_FLAG_*
    pub style_idx: u16,    // cellXfs 서식 번호
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

/// 모든 문자열(공유 문자열, 인라인 문자열, 수식 원문, 메모)을 하나의 연속 버퍼에 저장하는 풀
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

/// 행 단위 압축 희소 행렬 (CSR)
#[derive(Default, Clone)]
pub struct Rows {
    pub row_ids: Vec<u32>,   // 1-based 행 번호 (오름차순)
    pub row_start: Vec<u32>, // row_ids[i]의 시작 인덱스 -> cells[row_start[i]..row_start[i+1]]
    pub cells: Vec<Cell>,    // 각 행 내부에서 col(1-based) 오름차순
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
    pub start_row: u32, // 1-based
    pub start_col: u32, // 1-based
    pub end_row: u32,   // 1-based
    pub end_col: u32,   // 1-based
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezePane {
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub top_left_row: u32, // 1-based
    pub top_left_col: u32, // 1-based
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
    pub row: u32, // 1-based
    pub col: u32, // 1-based
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
    pub max_row: u32,               // 1-based 최대 행
    pub max_col: u32,               // 1-based 최대 열
    pub cell_count: u32,
    pub col_widths_pt: Vec<f32>,    // 1..=max_col 열 너비 (엑셀 pt)
    pub row_heights_pt: Vec<f32>,   // 1..=max_row 행 높이 (엑셀 pt)
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
// 2. C ABI 진입점 5종 (모두 catch_unwind 보호)
// ============================================================================

pub struct ASBook {
    pub wb: Workbook,
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
    let copy_len = bytes.len().min(dst.len().saturating_sub(1));
    for (i, &b) in bytes[..copy_len].iter().enumerate() {
        dst[i] = b as c_char;
    }
}

#[no_mangle]
pub extern "C" fn as_last_note() -> *const c_char {
    LAST_NOTE.with(|cell| cell.borrow().as_ptr())
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
            Ok(wb) => Box::into_raw(Box::new(ASBook { wb })),
            Err(e) => {
                set_last_note(&e);
                ptr::null_mut()
            }
        }
    }));

    match res {
        Ok(ptr) => ptr,
        Err(_) => {
            set_last_note("as_open: 내부 패닉이 포착되었습니다");
            ptr::null_mut()
        }
    }
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
            set_last_note("as_sheet_count: book 포인터가 NULL입니다");
            return 0;
        }
        let b = unsafe { &*book };
        b.wb.sheets.len() as u32
    }));
    res.unwrap_or_else(|_| {
        set_last_note("as_sheet_count: 내부 패닉이 포착되었습니다");
        0
    })
}

#[no_mangle]
pub extern "C" fn as_sheet_info(
    book: *const ASBook,
    index: u32,
    out: *mut ASSheetInfo,
) -> bool {
    let res = catch_unwind(AssertUnwindSafe(|| {
        if book.is_null() || out.is_null() {
            set_last_note("as_sheet_info: NULL 포인터 인자입니다");
            return false;
        }
        let b = unsafe { &*book };
        let Some(s) = b.wb.sheets.get(index as usize) else {
            set_last_note("as_sheet_info: 시트 인덱스가 범위를 벗어났습니다");
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

    res.unwrap_or_else(|_| {
        set_last_note("as_sheet_info: 내부 패닉이 포착되었습니다");
        false
    })
}

// ============================================================================
// 3. 단위 시험 (#[test] 필수 검증)
// ============================================================================

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

