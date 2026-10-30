//! 글꼴 찾기에 쓰는 창구들.
//!
//! 문서마다 쓰인 글꼴을 읽어(어떤 이름을 원하는가) 앱이 폰트 폴더에서 맞는
//! 것을 찾고, 없으면 닮은 것을 골라 그 이름으로 넣어 준다. 고르는 일은
//! Swift 가 하고, 여기서는 글꼴 파일과 문서를 읽어 이름만 건넨다.
//!
//! 글꼴을 통째로 넣어 두지 않는다. 사용자가 폰트를 수십 개 넣으면 한 개에
//! 10~20MB 라 메모리가 모자란다. 문서마다 필요한 것만 넣고, 다음 문서 전에 비운다.

use std::collections::{BTreeSet, HashMap};
use std::io::{Cursor, Read};
use std::os::raw::{c_char, c_int};
use std::slice;
use std::sync::Mutex;

use crate::{ARGOCONV_BAD_ARGS, ARGOCONV_OK};

/// SVG(한글 문서) 쪽의 이름 바꿈: 문서가 원한 이름 → 실제로 넣은 글꼴의 이름.
pub(crate) static SVG_ALIASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

fn write_out(text: &str, buffer: *mut u8, cap: usize) -> usize {
    if buffer.is_null() || cap == 0 {
        return 0;
    }
    let bytes = text.as_bytes();
    if bytes.len() > cap {
        // 잘린 JSON 은 쓸모가 없다. 필요한 크기를 알려 다시 부르게 한다.
        return bytes.len();
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
    bytes.len()
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 글꼴 파일 하나의 얼굴들. JSON 배열로 돌려준다.
///
/// `[{"i":0,"names":["함초롬바탕","HCR Batang"],"weight":400,"italic":false,
///    "mono":false,"hangul":true,"ttf":true}]`
///
/// `ttf` 는 PDF 에 심을 수 있는 TrueType 윤곽인지다. OTF(CFF)는 한글 문서 쪽은
/// 되지만 오피스 문서 쪽 변환기가 심지 못한다.
///
/// 돌려준 값이 `cap` 보다 크면 아무것도 쓰지 않았다는 뜻이다 — 그만큼 다시 부른다.
///
/// # Safety
/// `data`/`len` 은 읽을 수 있는 버퍼, `out`/`cap` 은 쓸 수 있는 버퍼여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_font_faces(
    data: *const u8,
    len: usize,
    out: *mut u8,
    cap: usize,
) -> usize {
    if data.is_null() || len == 0 {
        return 0;
    }
    let bytes = slice::from_raw_parts(data, len);
    let count = ttf_parser::fonts_in_collection(bytes).unwrap_or(1).min(64);
    let mut faces = Vec::new();
    for index in 0..count {
        let Ok(face) = ttf_parser::Face::parse(bytes, index) else { continue };
        let mut names = BTreeSet::new();
        for name in face.names() {
            if !matches!(
                name.name_id,
                ttf_parser::name_id::FAMILY
                    | ttf_parser::name_id::TYPOGRAPHIC_FAMILY
                    | ttf_parser::name_id::FULL_NAME
            ) {
                continue;
            }
            if let Some(text) = name.to_string() {
                let text = text.trim().to_owned();
                if !text.is_empty() && text.len() < 120 {
                    names.insert(text);
                }
            }
        }
        if names.is_empty() {
            continue;
        }
        let raw = face.raw_face();
        let ttf = raw.table(ttf_parser::Tag::from_bytes(b"glyf")).is_some();
        let hangul = face.glyph_index('가').is_some() && face.glyph_index('힣').is_some();
        faces.push(format!(
            "{{\"i\":{index},\"names\":[{}],\"weight\":{},\"italic\":{},\"mono\":{},\"hangul\":{hangul},\"ttf\":{ttf}}}",
            names.iter().map(|n| json_escape(n)).collect::<Vec<_>>().join(","),
            face.weight().to_number(),
            face.is_italic(),
            face.is_monospaced(),
        ));
    }
    write_out(&format!("[{}]", faces.join(",")), out, cap)
}

/// 문서가 쓰는 글꼴 이름들. JSON 문자열 배열로 돌려준다.
///
/// - docx · xlsx · pptx · hwpx (ZIP): 안의 XML 에서 글꼴 이름 자리를 훑는다.
/// - hwp (CFB): hwpers 가 읽은 글꼴 목록.
///
/// # Safety
/// `data`/`len` 은 읽을 수 있는 버퍼, `out`/`cap` 은 쓸 수 있는 버퍼여야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_doc_fonts(
    data: *const u8,
    len: usize,
    out: *mut u8,
    cap: usize,
) -> usize {
    if data.is_null() || len == 0 {
        return 0;
    }
    let bytes = slice::from_raw_parts(data, len);
    let names = std::panic::catch_unwind(|| document_fonts(bytes)).unwrap_or_default();
    let list = names.iter().map(|n| json_escape(n)).collect::<Vec<_>>().join(",");
    write_out(&format!("[{list}]"), out, cap)
}

fn document_fonts(bytes: &[u8]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut push = |name: &str| {
        let name = name.trim();
        // 테마 자리표시(+mn-lt 따위)와 빈 이름은 뺀다.
        if name.is_empty() || name.starts_with('+') || name.len() > 100 {
            return;
        }
        if !found.iter().any(|f| f.eq_ignore_ascii_case(name)) && found.len() < 60 {
            found.push(name.to_owned());
        }
    };

    if bytes.starts_with(b"PK") {
        let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else { return found };
        for index in 0..archive.len() {
            let Ok(mut file) = archive.by_index(index) else { continue };
            let name = file.name().to_owned();
            if !name.ends_with(".xml") || file.size() > 32 * 1024 * 1024 {
                continue;
            }
            // 글꼴 이름이 있을 만한 부분만 본다. 본문 전체를 다 훑을 필요는 없다.
            let interesting = name.contains("styles") || name.contains("theme")
                || name.contains("fontTable") || name.contains("document")
                || name.contains("slide") || name.contains("header")
                || name.starts_with("Contents/");
            if !interesting {
                continue;
            }
            let mut xml = String::new();
            if file.read_to_string(&mut xml).is_err() {
                continue;
            }
            for key in ["typeface=\"", "w:ascii=\"", "w:eastAsia=\"", "w:hAnsi=\"", "face=\"", "w:name=\""] {
                let mut rest = xml.as_str();
                while let Some(at) = rest.find(key) {
                    rest = &rest[at + key.len()..];
                    if let Some(end) = rest.find('"') {
                        push(&xml_unescape(&rest[..end]));
                        rest = &rest[end..];
                    } else {
                        break;
                    }
                }
            }
            // xlsx: <font> ... <name val="맑은 고딕"/>
            if name.contains("styles") {
                let mut rest = xml.as_str();
                while let Some(at) = rest.find("<name val=\"") {
                    rest = &rest[at + 11..];
                    if let Some(end) = rest.find('"') {
                        push(&xml_unescape(&rest[..end]));
                        rest = &rest[end..];
                    } else {
                        break;
                    }
                }
            }
        }
    } else if crate::slides::ppt::is_ppt(bytes) {
        // 옛 파워포인트(.ppt): 글꼴 목록
        for name in crate::slides::ppt::font_names(bytes) {
            push(&name);
        }
    } else if let names @ [_, ..] = minipdf::doc_font_names(bytes).as_slice() {
        // 옛 워드(.doc): 글꼴 이름표
        for name in names {
            push(name);
        }
    } else if let Ok(document) = hwpers::HwpReader::from_bytes(bytes) {
        for face in &document.doc_info.face_names {
            push(&face.font_name);
        }
    }
    found
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&quot;", "\"").replace("&apos;", "'")
        .replace("&lt;", "<").replace("&gt;", ">")
}

/// 등록한 글꼴과 이름 바꿈을 모두 비운다. 문서를 열 때마다 처음에 부른다.
#[no_mangle]
pub extern "C" fn argoconv_clear_fonts() -> c_int {
    minipdf::clear_fonts();
    if let Ok(mut fonts) = crate::hwp::SVG_FONTS.lock() {
        fonts.clear();
    }
    if let Ok(mut aliases) = SVG_ALIASES.lock() {
        aliases.clear();
    }
    ARGOCONV_OK
}

/// 한글 문서가 `requested` 를 원하면 `actual` 로 그리게 한다.
///
/// # Safety
/// 둘 다 NUL 로 끝나는 UTF-8 문자열이어야 한다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_font_alias(requested: *const c_char, actual: *const c_char) -> c_int {
    if requested.is_null() || actual.is_null() {
        return ARGOCONV_BAD_ARGS;
    }
    let (Ok(r), Ok(a)) = (
        std::ffi::CStr::from_ptr(requested).to_str(),
        std::ffi::CStr::from_ptr(actual).to_str(),
    ) else {
        return ARGOCONV_BAD_ARGS;
    };
    if let Ok(mut aliases) = SVG_ALIASES.lock() {
        aliases.push((r.trim().to_owned(), a.trim().to_owned()));
    }
    ARGOCONV_OK
}

/// SVG 의 font-family 값에서 원한 이름을 실제 이름으로 바꾼다. 원래 이름은
/// 뒤에 남겨 둔다 — 실제 이름을 못 찾으면 원래 이름으로라도 찾게.
pub(crate) fn apply_aliases(svg: &str) -> String {
    let Ok(aliases) = SVG_ALIASES.lock() else { return svg.to_owned() };
    if aliases.is_empty() {
        return svg.to_owned();
    }
    let map: HashMap<String, &str> = aliases
        .iter()
        .map(|(r, a)| (r.to_lowercase(), a.as_str()))
        .collect();
    let key = "font-family=\"";
    let mut out = String::with_capacity(svg.len() + 256);
    let mut rest = svg;
    while let Some(at) = rest.find(key) {
        out.push_str(&rest[..at + key.len()]);
        rest = &rest[at + key.len()..];
        let Some(end) = rest.find('"') else { break };
        let value = &rest[..end];
        let mut parts: Vec<String> = Vec::new();
        for token in value.split(',') {
            let bare = token.trim().trim_matches('\'').trim_matches('"').trim();
            if let Some(actual) = map.get(&bare.to_lowercase()) {
                if !parts.iter().any(|p| p == actual) {
                    parts.push((*actual).to_owned());
                }
            }
            if !bare.is_empty() && !parts.iter().any(|p| p == bare) {
                parts.push(bare.to_owned());
            }
        }
        out.push_str(&parts.join(", "));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_face_names_and_weight() {
        let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Spartan/Fonts/Pretendard-Bold.ttf")).unwrap();
        let mut buf = vec![0u8; 65536];
        let n = unsafe { argoconv_font_faces(data.as_ptr(), data.len(), buf.as_mut_ptr(), buf.len()) };
        let json = String::from_utf8(buf[..n].to_vec()).unwrap();
        assert!(json.contains("Pretendard"), "{json}");
        assert!(json.contains("\"weight\":700"), "{json}");
        assert!(json.contains("\"hangul\":true"), "{json}");
    }

    #[test]
    fn finds_fonts_in_office_xml() {
        use std::io::Write;
        let mut zipped = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zipped));
            let opt = zip::write::SimpleFileOptions::default();
            w.start_file("word/document.xml", opt).unwrap();
            w.write_all(r#"<w:r><w:rPr><w:rFonts w:ascii="Arial" w:eastAsia="맑은 고딕"/></w:rPr></w:r><a:latin typeface="+mn-lt"/>"#.as_bytes()).unwrap();
            w.start_file("xl/styles.xml", opt).unwrap();
            w.write_all(r#"<font><name val="함초롬바탕"/></font>"#.as_bytes()).unwrap();
            w.finish().unwrap();
        }
        let names = document_fonts(&zipped);
        assert_eq!(names, vec!["Arial", "맑은 고딕", "함초롬바탕"]);
    }

    #[test]
    fn rewrites_svg_font_family() {
        SVG_ALIASES.lock().unwrap().push(("함초롬바탕".into(), "나눔명조".into()));
        let out = apply_aliases(r#"<text font-family="함초롬바탕, serif">가</text>"#);
        assert_eq!(out, r#"<text font-family="나눔명조, 함초롬바탕, serif">가</text>"#);
        SVG_ALIASES.lock().unwrap().clear();
    }
}
