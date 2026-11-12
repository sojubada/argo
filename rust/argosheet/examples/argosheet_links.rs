//! 시트의 링크 칸을 나열한다(확인용). argosheet_links <file.xlsx>
use argosheet::*;
use std::ffi::{CStr, CString};
fn main() {
    let c = CString::new(std::env::args().nth(1).unwrap()).unwrap();
    let book = as_open(c.as_ptr());
    for s in 0..as_sheet_count(book) {
        let mut si: ASSheetInfo = unsafe { std::mem::zeroed() };
        as_sheet_info(book, s, &mut si);
        let mut ci: ASCellInfo = unsafe { std::mem::zeroed() };
        for r in 1..=si.row_count { for col in 1..=si.col_count {
            if as_cell_info(book, s, r, col, &mut ci) && ci.link_target[0] != 0 && ci.merge_start_row == r && ci.merge_start_col == col {
                println!("{} {} -> {} (내부 {})", s, unsafe { CStr::from_ptr(ci.address.as_ptr()) }.to_string_lossy(),
                    unsafe { CStr::from_ptr(ci.link_target.as_ptr()) }.to_string_lossy(), ci.link_is_internal);
            }
        }}
    }
    as_close(book);
}
