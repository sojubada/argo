//! 시트의 차트·그림 오버레이를 나열하고, 그리기 목록을 JSON 으로 낸다(확인용).
//! argosheet_overlays <file.xlsx> [출력 폴더]
use argosheet::*;
use std::ffi::{CStr, CString};
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = CString::new(a[1].clone()).unwrap();
    let outdir = a.get(2).cloned();
    let t0 = std::time::Instant::now();
    let book = as_open(c.as_ptr());
    assert!(!book.is_null());
    println!("열기 {:?}", t0.elapsed());
    for s in 0..as_sheet_count(book) {
        let n = as_sheet_overlay_count(book, s);
        for i in 0..n {
            let mut oi: ASOverlayInfo = unsafe { std::mem::zeroed() };
            as_sheet_overlay_info(book, s, i, &mut oi);
            let title = unsafe { CStr::from_ptr(oi.title.as_ptr()) }.to_string_lossy();
            let dl = as_sheet_overlay_drawlist(book, s, i);
            let cnt = as_drawlist_count(dl);
            println!("시트{} #{} 종류{} 위치({:.0},{:.0}) 크기({:.0}x{:.0}) 시작({},{}) 명령{} 제목 {:?}",
                s, i, oi.kind, oi.frame_pt.x, oi.frame_pt.y, oi.frame_pt.w, oi.frame_pt.h, oi.start_row, oi.start_col, cnt, title);
            if let (Some(dir), false) = (&outdir, dl.is_null()) {
                let cmds = unsafe { std::slice::from_raw_parts(as_drawlist_commands(dl), cnt as usize) };
                let mut sl = 0u32;
                let sp = as_drawlist_strings(dl, &mut sl) as *const u8;
                let strs = if sp.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(sp, sl as usize) } };
                let mut f = std::fs::File::create(format!("{}/ov_{}_{}.json", dir, s, i)).unwrap();
                writeln!(f, "{{\"w\":{},\"h\":{},\"cmds\":[", oi.frame_pt.w, oi.frame_pt.h).unwrap();
                for (j, k) in cmds.iter().enumerate() {
                    let o = k.text_offset as usize;
                    let tx = if k.cmd_type == 4 && o + k.text_len as usize <= strs.len() { String::from_utf8_lossy(&strs[o..o + k.text_len as usize]).into_owned() } else { String::new() };
                    writeln!(f, "{}{{\"t\":{},\"s\":{},\"ah\":{},\"av\":{},\"r\":[{},{},{},{}],\"c\":[{},{},{},{}],\"k1\":[{},{},{},{}],\"k2\":[{},{},{},{}],\"w\":{},\"fs\":{},\"rot\":{},\"ind\":{},\"fl\":{},\"tx\":{:?}}}",
                        if j > 0 { "," } else { "" }, k.cmd_type, k.sub_type, k.align_horiz, k.align_vert,
                        k.rect.x, k.rect.y, k.rect.w, k.rect.h, k.clip_rect.x, k.clip_rect.y, k.clip_rect.w, k.clip_rect.h,
                        k.color1.r, k.color1.g, k.color1.b, k.color1.a, k.color2.r, k.color2.g, k.color2.b, k.color2.a,
                        k.stroke_width, k.font_size_pt, k.rotation, k.indent, k.text_flags, tx).unwrap();
                }
                writeln!(f, "]}}").unwrap();
            }
            as_drawlist_free(dl);
        }
    }
    as_close(book);
}
