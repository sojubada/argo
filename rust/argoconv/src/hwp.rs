//! 한글 문서를 PDF 로.
//!
//! hwpers 가 hwp(CFB) 와 hwpx(OWPML) 를 읽고 자체 레이아웃 엔진으로 쪽을
//! 나눠 SVG 로 그립니다. 우리는 그 SVG 를 쪽마다 PDF 로 옮겨 한 파일로
//! 묶습니다 — 그러면 오피스 문서와 똑같이 PDF 뷰어가 받습니다.

use std::os::raw::c_int;
use std::slice;
use std::sync::Mutex;

use hwpers::render::{HwpRenderer, RenderOptions};
use hwpers::{HwpReader, HwpxReader};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use svg2pdf::usvg;

use crate::{guard, ARGOCONV_BAD_ARGS, ARGOCONV_CONVERT_FAILED, ARGOCONV_OK};

/// SVG 안의 글자를 그리려면 usvg 쪽에도 폰트가 있어야 합니다.
/// minipdf 에 넘긴 것과 같은 바이트를 여기에도 쌓아 둡니다.
pub(crate) static SVG_FONTS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

fn font_options() -> usvg::Options<'static> {
    let mut options = usvg::Options::default();
    {
        let db = options.fontdb_mut();
        if let Ok(fonts) = SVG_FONTS.lock() {
            for bytes in fonts.iter() {
                db.load_font_data(bytes.clone());
            }
        }
        // 한글 문서가 지정한 글꼴이 기기에 없을 때 돌아갈 자리입니다.
        // 이름을 먼저 복사해 둡니다 — db 를 읽으면서 쓸 수는 없습니다.
        let fallback = db
            .faces()
            .next()
            .and_then(|face| face.families.first().map(|(name, _)| name.clone()));
        if let Some(family) = fallback {
            db.set_sans_serif_family(family.clone());
            db.set_serif_family(family);
        }
    }
    options
}

/// hwp · hwpx -> PDF.
///
/// # Safety
/// `input`/`len` 은 유효한 버퍼, `out`/`out_len` 은 쓸 수 있는 포인터여야 합니다.
#[no_mangle]
pub unsafe extern "C" fn argoconv_hwp_to_pdf(
    input: *const u8,
    len: usize,
    out: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    if input.is_null() || len == 0 || out.is_null() || out_len.is_null() {
        return ARGOCONV_BAD_ARGS;
    }
    *out = std::ptr::null_mut();
    *out_len = 0;

    crate::clear_note();
    let bytes = slice::from_raw_parts(input, len);
    let (out_ptr, out_size) = (out, out_len);
    guard(move || convert(bytes, out_ptr, out_size))
}

/// 패닉을 잡는 바깥쪽과 실제 일을 나눕니다.
unsafe fn convert(bytes: &[u8], out: *mut *mut u8, out_len: *mut usize) -> c_int {
    // hwpx 는 ZIP, hwp 는 CFB 입니다. 확장자를 믿지 않고 앞 네 바이트로 가립니다.
    let document = if bytes.starts_with(b"PK") {
        HwpxReader::from_bytes(bytes)
    } else {
        HwpReader::from_bytes(bytes)
    };
    let document = match document {
        Ok(document) => document,
        Err(error) => {
            crate::note(format!("hwp 파일을 못 읽었다: {error}"));
            return ARGOCONV_CONVERT_FAILED;
        }
    };

    // 표를 몇 개나 봤는지 세어 로그로 넘긴다. 기기에서 표가 안 보일 때
    // 못 읽은 것인지 못 그린 것인지 이 숫자 하나로 갈린다.
    let mut tables = 0usize;
    let mut cells = 0usize;
    let mut paragraphs = 0usize;
    for section in document.sections() {
        paragraphs += section.paragraphs.len();
        for para in &section.paragraphs {
            if let Some(table) = para.table_data.as_ref() {
                tables += 1;
                cells += table.cells.len();
            }
        }
    }

    let rendered = HwpRenderer::new(&document, RenderOptions::default()).render();

    // 쪽은 나왔는데 비어 있는 일이 있었다. 그릴 것을 몇 개나 만들었는지,
    // 첫 글자가 어떤 크기와 색으로 나갔는지까지 적어 둔다 — 레이아웃이
    // 빈 것인지 그리다 사라진 것인지 이 줄로 갈린다.
    let elements: usize = rendered.pages.iter().map(|page| page.elements.len()).sum();
    let first = rendered
        .pages
        .iter()
        .flat_map(|page| page.elements.iter())
        .find_map(|element| match element {
            hwpers::render::renderer::RenderElement::Text {
                text,
                font_size,
                color,
                font_family,
                ..
            } => Some(format!(
                ", 첫 글자 \"{}\" {font_size:.1}크기 #{:06X} {font_family}",
                text.chars().take(6).collect::<String>(),
                color & 0xFFFFFF
            )),
            _ => None,
        })
        .unwrap_or_else(|| ", 글자 요소 없음".to_owned());

    // 그냥 지나친 레코드도 적는다. 표가 안 보일 때 우리가 무엇을 못 보고
    // 지나쳤는지 태그 번호로 바로 짚을 수 있어야 한다.
    let mut skipped: std::collections::BTreeMap<u16, usize> = Default::default();
    for body in &document.body_texts {
        for (tag, count) in &body.skipped {
            *skipped.entry(*tag).or_insert(0) += count;
        }
    }
    let skipped_text = if skipped.is_empty() {
        String::new()
    } else {
        let mut worst: Vec<_> = skipped.iter().collect();
        worst.sort_by(|a, b| b.1.cmp(a.1));
        let list = worst
            .iter()
            .take(5)
            .map(|(tag, count)| format!("0x{tag:02X}×{count}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!(", 지나친 레코드 {list}")
    };

    crate::note(format!(
        "hwp 문단 {paragraphs}개, 표 {tables}개, 칸 {cells}개, 쪽 {}장, \
         그릴 것 {elements}개{first}{skipped_text}",
        rendered.pages.len()
    ));
    if rendered.pages.is_empty() {
        return ARGOCONV_CONVERT_FAILED;
    }

    let Ok(pdf) = build_pdf(&rendered) else {
        crate::note(format!(
            "hwp 쪽 {}장을 PDF 로 못 옮겼다",
            rendered.pages.len()
        ));
        return ARGOCONV_CONVERT_FAILED;
    };

    let mut boxed = pdf.into_boxed_slice();
    *out_len = boxed.len();
    *out = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    ARGOCONV_OK
}

/// 쪽마다 SVG 를 XObject 로 넣고 한 PDF 로 묶습니다.
fn build_pdf(rendered: &hwpers::render::renderer::RenderResult) -> Result<Vec<u8>, ()> {
    let options = font_options();
    let conversion = svg2pdf::ConversionOptions::default();

    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog_id = alloc.bump();
    let tree_id = alloc.bump();
    let mut page_ids: Vec<Ref> = Vec::new();

    for index in 0..rendered.pages.len() {
        let Some(svg) = rendered.to_svg(index) else {
            continue;
        };
        let Ok(tree) = usvg::Tree::from_str(&svg, &options) else {
            continue;
        };
        let Ok((chunk, xobject)) = svg2pdf::to_chunk(&tree, conversion) else {
            continue;
        };

        // 쪽마다 번호를 새로 매깁니다. 안 그러면 두 번째 쪽부터 겹칩니다.
        let mut mapping = std::collections::HashMap::new();
        chunk.renumber_into(&mut pdf, |old| {
            *mapping.entry(old).or_insert_with(|| alloc.bump())
        });
        let Some(&xobject_id) = mapping.get(&xobject) else {
            continue;
        };

        let page_id = alloc.bump();
        let content_id = alloc.bump();
        let width = tree.size().width();
        let height = tree.size().height();
        let name = Name(b"S");

        let mut content = Content::new();
        content.transform([width, 0.0, 0.0, height, 0.0, 0.0]);
        content.x_object(name);
        pdf.stream(content_id, &content.finish());

        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, width, height));
        page.parent(tree_id);
        page.contents(content_id);
        page.resources().x_objects().pair(name, xobject_id);
        page.finish();

        page_ids.push(page_id);
    }

    if page_ids.is_empty() {
        return Err(());
    }

    pdf.pages(tree_id).kids(page_ids.iter().copied()).count(page_ids.len() as i32);
    pdf.catalog(catalog_id).pages(tree_id);
    Ok(pdf.finish())
}
