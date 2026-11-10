//! 시험용: cargo run --release --example office2pdf -- 입력 출력 글꼴.ttf [굵은글꼴.ttf]
//! 앱이 하는 것처럼 문서가 원하는 글꼴 이름을 모두 주어진 글꼴로 바꿔 넣는다.
use std::ffi::CString;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = std::fs::read(&args[1]).expect("input");
    unsafe {
        argoconv::fonts::argoconv_clear_fonts();
        // 3번째부터: 보통·굵게·(그 뒤로는 파일 이름을 글꼴 이름으로). 앱이 하는 것과 같게,
        // ArgoHanja(한자 대체)·LiberationSans(Arial) 도 같이 넣어 시험한다.
        for (i, path) in args.iter().skip(3).enumerate() {
            let bytes = std::fs::read(path).expect("font");
            let stem = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let label = match i {
                0 => "Pretendard".to_owned(),
                1 => "Pretendard bold".to_owned(),
                _ => stem.clone(),
            };
            let name = CString::new(label).unwrap();
            argoconv::argoconv_register_font(name.as_ptr(), bytes.as_ptr(), bytes.len());
        }
        let has_liberation = args.iter().skip(3).any(|p| p.to_lowercase().contains("liberationsans"));
        let mut buf = vec![0u8; 1 << 20];
        let n = argoconv::fonts::argoconv_doc_fonts(input.as_ptr(), input.len(), buf.as_mut_ptr(), buf.len());
        let list = String::from_utf8_lossy(&buf[..n]).to_string();
        for name in list.trim_matches(|c| c == '[' || c == ']').split("\",\"") {
            let name = name.trim_matches('"');
            if name.is_empty() {
                continue;
            }
            let req = CString::new(name).unwrap();
            let low = name.to_lowercase().replace([' ', '-', '_'], "");
            let arial = has_liberation && (low.starts_with("arial") || low.starts_with("helvetica"));
            let act = CString::new(if arial { "Liberation Sans" } else { "Pretendard" }).unwrap();
            argoconv::fonts::argoconv_font_alias(req.as_ptr(), act.as_ptr());
        }
        let mut out: *mut u8 = std::ptr::null_mut();
        let mut out_len = 0usize;
        let t = std::time::Instant::now();
        let is_hwp = args[1].to_lowercase().ends_with(".hwp") || args[1].to_lowercase().ends_with(".hwpx");
        let code = if is_hwp {
            argoconv::hwp::argoconv_hwp_to_pdf(input.as_ptr(), input.len(), &mut out, &mut out_len)
        } else {
            argoconv::argoconv_to_pdf(input.as_ptr(), input.len(), &mut out, &mut out_len)
        };
        let ms = t.elapsed().as_millis();
        let mut note = vec![0u8; 4096];
        let nl = argoconv::argoconv_last_note(note.as_mut_ptr(), note.len());
        println!("code {code} {ms}ms {} bytes: {}", out_len, String::from_utf8_lossy(&note[..nl]));
        if code == 0 {
            let pdf = std::slice::from_raw_parts(out, out_len).to_vec();
            std::fs::write(&args[2], pdf).expect("write");
            argoconv::argoconv_free(out, out_len);
        }
    }
}
