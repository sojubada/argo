//! 실제 파일로 C ABI 를 끝까지 돌려 본다: 열기 → 시트마다 모든 타일 → 셀 정보.
//! 쓰는 법: cargo run --release --example argosheet_tiles -- <file.xlsx>
use argosheet::*;
use std::ffi::CString;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("xlsx 경로");
    let c = CString::new(path).unwrap();
    let t0 = Instant::now();
    let book = as_open(c.as_ptr());
    assert!(!book.is_null(), "열기 실패");
    println!("열기(계산 포함) {:?}", t0.elapsed());
    let n = as_sheet_count(book);
    for s in 0..n {
        let mut info: ASSheetInfo = unsafe { std::mem::zeroed() };
        as_sheet_info(book, s, &mut info);
        let (w, h) = (info.total_width_pt, info.total_height_pt);
        let tile = 256.0 / 1.5; // 화면 256pt, 확대 150%
        let t1 = Instant::now();
        let (mut tiles, mut cmds, mut first) = (0, 0usize, None);
        let mut y = 0.0;
        while y < h {
            let mut x = 0.0;
            while x < w {
                let dl = as_tile(book, s, x, y, tile, tile, 1.5);
                assert!(!dl.is_null());
                cmds += as_drawlist_count(dl) as usize;
                as_drawlist_free(dl);
                tiles += 1;
                if tiles == 12 { first = Some(t1.elapsed()); }
                x += tile;
            }
            y += tile;
        }
        let mut ci: ASCellInfo = unsafe { std::mem::zeroed() };
        for r in 1..=info.row_count.min(200) {
            for col in 1..=info.col_count.min(60) {
                as_cell_info(book, s, r, col, &mut ci);
            }
        }
        println!("시트 {} {}x{} 타일 {}장 명령 {} 전체 {:?} 첫 화면(12장) {:?}",
            s, info.row_count, info.col_count, tiles, cmds, t1.elapsed(), first.unwrap_or(t1.elapsed()));
    }
    as_close(book);
}
