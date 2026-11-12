//! 시트 하나의 그리기 목록을 JSON 으로 내보낸다(그림 확인용).
//! 쓰는 법: argosheet_drawjson <file.xlsx> <시트 번호(0부터)> > out.json
use argosheet::draw::*;
use argosheet::*;
use std::ffi::{CStr, CString};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let c = CString::new(a[1].clone()).unwrap();
    let s: u32 = a.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
    let book = as_open(c.as_ptr());
    assert!(!book.is_null());
    let mut info: ASSheetInfo = unsafe { std::mem::zeroed() };
    as_sheet_info(book, s, &mut info);
    let dl = as_tile(book, s, 0.0, 0.0, info.total_width_pt, info.total_height_pt, 1.0);
    let n = as_drawlist_count(dl) as usize;
    let cmds = unsafe { std::slice::from_raw_parts(as_drawlist_commands(dl), n) };
    let mut sl = 0u32;
    let sp = as_drawlist_strings(dl, &mut sl) as *const u8;
    let strs = unsafe { std::slice::from_raw_parts(sp, sl as usize) };
    let name = unsafe { CStr::from_ptr(info.name.as_ptr()) }.to_string_lossy();
    println!("{{\"name\":{:?},\"w\":{},\"h\":{},\"cmds\":[", name, info.total_width_pt, info.total_height_pt);
    for (i, k) in cmds.iter().enumerate() {
        let txt = if k.cmd_type == 4 {
            let o = k.text_offset as usize;
            String::from_utf8_lossy(&strs[o..o + k.text_len as usize]).into_owned()
        } else {
            String::new()
        };
        println!("{}{{\"t\":{},\"s\":{},\"ah\":{},\"av\":{},\"r\":[{},{},{},{}],\"c\":[{},{},{},{}],\"k1\":[{},{},{},{}],\"k2\":[{},{},{},{}],\"w\":{},\"fs\":{},\"rot\":{},\"ind\":{},\"fl\":{},\"tx\":{:?}}}",
            if i > 0 { "," } else { "" }, k.cmd_type, k.sub_type, k.align_horiz, k.align_vert,
            k.rect.x, k.rect.y, k.rect.w, k.rect.h, k.clip_rect.x, k.clip_rect.y, k.clip_rect.w, k.clip_rect.h,
            k.color1.r, k.color1.g, k.color1.b, k.color1.a, k.color2.r, k.color2.g, k.color2.b, k.color2.a,
            k.stroke_width, k.font_size_pt, k.rotation, k.indent, k.text_flags, txt);
    }
    println!("]}}");
    as_drawlist_free(dl);
    as_close(book);
}
