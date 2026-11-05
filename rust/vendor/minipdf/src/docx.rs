use std::collections::HashMap;
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::pdf::{styled_text_width_with_font, PdfColor, PdfDocument};
use crate::{read_zip_text, ConversionOptions, PageSize, Result};

const PAGE_WIDTH: f32 = 595.28;
const PAGE_HEIGHT: f32 = 841.89;
const MARGIN: f32 = 54.0;
const BODY_FONT_SIZE: f32 = 11.0;
const TABLE_CELL_PADDING_HORIZONTAL: f32 = 5.4;
const TABLE_BORDER_WIDTH: f32 = 0.5;
const VML_INLINE_IMAGE_TOP_LEADING: f32 = 2.0;

#[derive(Debug, PartialEq)]
struct DocxDocument {
    blocks: Vec<DocxBlock>,
    /// 머리말·꼬리말 (보통 쪽, 첫 쪽). 문단만 담는다.
    header: Vec<DocxParagraph>,
    footer: Vec<DocxParagraph>,
    first_header: Option<Vec<DocxParagraph>>,
    first_footer: Option<Vec<DocxParagraph>>,
    header_distance: f32,
    footer_distance: f32,
    page_size: PageSize,
    margins: DocxMargins,
    grid_line_pitch: f32,
    compatibility_mode: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DocxMargins {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

#[derive(Debug, PartialEq)]
enum DocxBlock {
    Paragraph(DocxParagraph),
    Table(DocxTable),
    Image(DocxImage),
    PageBreak,
}

#[derive(Debug, Clone, PartialEq)]
struct DocxParagraph {
    runs: Vec<DocxRun>,
    floating_images: Vec<DocxFloatingImage>,
    style_id: Option<String>,
    contextual_spacing: bool,
    alignment: TextAlignment,
    indent_left: f32,
    indent_right: f32,
    spacing_before: f32,
    spacing_after: f32,
    line_spacing: f32,
    line_spacing_explicit: bool,
    fill: Option<PdfColor>,
    bottom_border: Option<DocxBorder>,
    /// 첫 줄 들여쓰기(+) / 내어쓰기(-)
    first_line: f32,
    justify: bool,
    tabs: Vec<TabStop>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TabStop {
    pos: f32,
    align: TabAlign,
    leader: Option<char>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TabAlign {
    Left,
    Right,
    Center,
}

/// 번호 매기기 정의 (word/numbering.xml)
#[derive(Debug, Clone, Default)]
struct NumberingDefs {
    /// numId → (abstractNumId, 수준별 시작값 덮어쓰기)
    nums: HashMap<String, (String, HashMap<u8, i32>)>,
    abstracts: HashMap<String, Vec<NumLevel>>,
}

#[derive(Debug, Clone, Default)]
struct NumLevel {
    start: i32,
    format: String,
    text: String,
    suffix: String,
    paragraph: ParagraphProperties,
    run: RunProperties,
}

#[derive(Debug, Clone, PartialEq)]
struct DocxFloatingImage {
    image: DocxImage,
    offset_x: f32,
    offset_y: f32,
    inset_left: f32,
    inset_top: f32,
    line_top_leading: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct DocxRun {
    text: String,
    font_size: f32,
    font_name: Option<String>,
    bold: bool,
    italic: bool,
    underline: bool,
    color: PdfColor,
    highlight: Option<PdfColor>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DocxBorder {
    color: PdfColor,
    width: f32,
    space: f32,
    is_double: bool,
    pattern: BorderPattern,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BorderPattern {
    Solid,
    DotDash,
    Dotted,
    Dashed,
}

#[derive(Debug, Clone, Default)]
struct DocxStyles {
    paragraph_defaults: ParagraphProperties,
    run_defaults: RunProperties,
    styles: HashMap<String, DocxStyle>,
    theme_fonts: DocxThemeFonts,
    numbering: NumberingDefs,
    /// 번호 세기 (abstractNumId → 수준별 지금 값)
    counters: std::cell::RefCell<HashMap<String, [Option<i32>; 9]>>,
}

#[derive(Debug, Clone, Default)]
struct DocxStyle {
    based_on: Option<String>,
    paragraph: ParagraphProperties,
    run: RunProperties,
    /// 표 스타일이 주는 테두리. Table Grid 가 선을 긋는 방식이다.
    table_borders: DocxCellBorders,
}

#[derive(Debug, Clone, Default)]
struct ParagraphProperties {
    contextual_spacing: Option<bool>,
    alignment: Option<TextAlignment>,
    indent_left: Option<f32>,
    indent_right: Option<f32>,
    spacing_before: Option<f32>,
    spacing_after: Option<f32>,
    line_spacing: Option<f32>,
    fill: Option<PdfColor>,
    bottom_border: Option<DocxBorder>,
    first_line: Option<f32>,
    justify: Option<bool>,
    /// (numId, 수준)
    numbering: Option<(String, u8)>,
    tabs: Option<Vec<(TabStop, bool)>>,
}

#[derive(Debug, Clone, Default)]
struct RunProperties {
    font_size: Option<f32>,
    font_name: Option<String>,
    font_theme: Option<ThemeFontReference>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    color: Option<PdfColor>,
    highlight: Option<PdfColor>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ThemeFontReference {
    MajorEastAsia,
    MinorEastAsia,
}

#[derive(Debug, Clone, Default)]
struct DocxThemeFonts {
    major_east_asia: Option<String>,
    minor_east_asia: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct DocxImage {
    data: DocxImageData,
    width: f32,
    height: f32,
    alignment: TextAlignment,
}

#[derive(Debug, Clone, PartialEq)]
enum DocxImageData {
    Jpeg {
        data: Vec<u8>,
        width: u16,
        height: u16,
    },
    Rgba {
        data: Vec<u8>,
        width: u16,
        height: u16,
    },
}

#[derive(Debug, PartialEq)]
struct DocxTable {
    column_widths: Vec<f32>,
    rows: Vec<DocxTableRow>,
    alignment: TextAlignment,
    cell_margin_left: f32,
    cell_margin_right: f32,
    cell_margin_top: f32,
    cell_margin_bottom: f32,
    legacy_cjk_metrics: bool,
}

#[derive(Debug, PartialEq)]
struct DocxTableRow {
    cells: Vec<DocxTableCell>,
    height: Option<f32>,
}

#[derive(Debug, PartialEq)]
struct DocxTableCell {
    runs: Vec<DocxRun>,
    images: Vec<DocxImage>,
    width: Option<f32>,
    spacing_before: f32,
    spacing_after: f32,
    line_spacing: f32,
    fill: Option<PdfColor>,
    alignment: TextAlignment,
    vertical_alignment: VerticalAlignment,
    grid_span: usize,
    vertical_merge: VerticalMerge,
    borders: DocxCellBorders,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct DocxCellBorders {
    top: Option<DocxBorder>,
    right: Option<DocxBorder>,
    bottom: Option<DocxBorder>,
    left: Option<DocxBorder>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VerticalAlignment {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum VerticalMerge {
    None,
    Restart,
    Continue,
}

pub(crate) fn convert_docx_bytes(input: &[u8], options: &ConversionOptions) -> Result<Vec<u8>> {
    let document = read_docx_document(input)?;
    crate::stat(format!("docx 블록 {}개", document.blocks.len()));
    let mut doc = PdfDocument::new();
    render_docx(&mut doc, &document, options);
    Ok(doc.to_bytes())
}

fn read_docx_document(input: &[u8]) -> Result<DocxDocument> {
    let cursor = Cursor::new(input);
    let mut archive = ZipArchive::new(cursor)?;
    let Some(document_xml) = read_zip_text(&mut archive, "word/document.xml")? else {
        return Ok(empty_docx_document());
    };
    let styles = read_styles(&mut archive)?;
    let relationships = read_document_relationships(&mut archive)?;
    let compatibility_mode = read_compatibility_mode(&mut archive)?;

    let xml = roxmltree::Document::parse(&document_xml)?;
    let Some(body) = xml.descendants().find(|node| node.has_tag_name("body")) else {
        return Ok(empty_docx_document());
    };
    let (page_size, margins, grid_line_pitch) = read_page_layout(body);
    let mut blocks = Vec::new();

    for node in body.children().filter(|node| node.is_element()) {
        if node.has_tag_name("p") {
            read_paragraph(node, &mut blocks, &styles, &relationships, &mut archive)?;
        } else if node.has_tag_name("tbl") {
            blocks.push(DocxBlock::Table(read_table(
                node,
                &styles,
                &relationships,
                &mut archive,
                compatibility_mode <= 12,
            )?));
        }
    }

    if blocks.is_empty() {
        blocks.push(DocxBlock::Paragraph(plain_paragraph(
            "Empty DOCX document".to_owned(),
        )));
    }

    let (header, footer, first_header, first_footer, header_distance, footer_distance) =
        read_headers_footers(&xml, body, &styles, &relationships, &mut archive)?;
    Ok(DocxDocument {
        blocks,
        header,
        footer,
        first_header,
        first_footer,
        header_distance,
        footer_distance,
        page_size,
        margins,
        grid_line_pitch,
        compatibility_mode,
    })
}

fn read_compatibility_mode(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<u8> {
    let Some(settings_xml) = read_zip_text(archive, "word/settings.xml")? else {
        return Ok(15);
    };
    let xml = roxmltree::Document::parse(&settings_xml)?;
    Ok(xml
        .descendants()
        .find(|node| {
            node.has_tag_name("compatSetting")
                && attribute_by_local_name(*node, "name") == Some("compatibilityMode")
        })
        .and_then(|node| attribute_by_local_name(node, "val"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(15))
}

fn empty_docx_document() -> DocxDocument {
    DocxDocument {
        blocks: vec![DocxBlock::Paragraph(plain_paragraph(
            "Empty DOCX document".to_owned(),
        ))],
        header: Vec::new(),
        footer: Vec::new(),
        first_header: None,
        first_footer: None,
        header_distance: 36.0,
        footer_distance: 36.0,
        page_size: PageSize {
            width: PAGE_WIDTH,
            height: PAGE_HEIGHT,
        },
        margins: DocxMargins {
            top: MARGIN,
            right: MARGIN,
            bottom: MARGIN,
            left: MARGIN,
        },
        grid_line_pitch: 0.0,
        compatibility_mode: 15,
    }
}

fn plain_paragraph(text: String) -> DocxParagraph {
    DocxParagraph {
        runs: vec![DocxRun {
            text,
            font_size: BODY_FONT_SIZE,
            font_name: None,
            bold: false,
            italic: false,
            underline: false,
            color: PdfColor::BLACK,
            highlight: None,
        }],
        floating_images: Vec::new(),
        style_id: None,
        contextual_spacing: false,
        alignment: TextAlignment::Left,
        indent_left: 0.0,
        indent_right: 0.0,
        spacing_before: 0.0,
        spacing_after: 0.0,
        line_spacing: 1.0,
        line_spacing_explicit: false,
        fill: None,
        bottom_border: None,
        first_line: 0.0,
        justify: false,
        tabs: Vec::new(),
    }
}

fn read_page_layout(body: roxmltree::Node<'_, '_>) -> (PageSize, DocxMargins, f32) {
    let section = body.children().find(|node| node.has_tag_name("sectPr"));
    let page_size_node = section.and_then(|node| child(node, "pgSz"));
    let margin_node = section.and_then(|node| child(node, "pgMar"));
    let page_size = PageSize {
        width: twips_attribute(page_size_node, "w").unwrap_or(PAGE_WIDTH),
        height: twips_attribute(page_size_node, "h").unwrap_or(PAGE_HEIGHT),
    };
    let margins = DocxMargins {
        top: twips_attribute(margin_node, "top").unwrap_or(MARGIN),
        right: twips_attribute(margin_node, "right").unwrap_or(MARGIN),
        bottom: twips_attribute(margin_node, "bottom").unwrap_or(MARGIN),
        left: twips_attribute(margin_node, "left").unwrap_or(MARGIN),
    };
    let grid_line_pitch = section
        .and_then(|node| child(node, "docGrid"))
        .filter(|node| {
            matches!(
                attribute_by_local_name(*node, "type"),
                Some("lines" | "linesAndChars" | "snapToChars")
            )
        })
        .and_then(|node| twips_attribute(Some(node), "linePitch"))
        .unwrap_or(0.0);
    (page_size, margins, grid_line_pitch)
}

fn read_styles(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<DocxStyles> {
    let theme_fonts = read_theme_fonts(archive)?;
    let Some(styles_xml) = read_zip_text(archive, "word/styles.xml")? else {
        return Ok(DocxStyles {
            theme_fonts,
            ..DocxStyles::default()
        });
    };
    let xml = roxmltree::Document::parse(&styles_xml)?;
    let defaults = xml
        .descendants()
        .find(|node| node.has_tag_name("docDefaults"));
    let paragraph_defaults = defaults
        .and_then(|node| node.descendants().find(|child| child.has_tag_name("pPr")))
        .map(read_paragraph_properties)
        .unwrap_or_default();
    let run_defaults = defaults
        .and_then(|node| node.descendants().find(|child| child.has_tag_name("rPr")))
        .map(read_run_properties)
        .unwrap_or_default();
    let styles = xml
        .descendants()
        .filter(|node| node.has_tag_name("style"))
        .filter_map(|node| {
            let id = attribute_by_local_name(node, "styleId")?;
            Some((
                id.to_owned(),
                DocxStyle {
                    based_on: child(node, "basedOn")
                        .and_then(|value| attribute_by_local_name(value, "val"))
                        .map(str::to_owned),
                    paragraph: child(node, "pPr")
                        .map(read_paragraph_properties)
                        .unwrap_or_default(),
                    run: child(node, "rPr")
                        .map(read_run_properties)
                        .unwrap_or_default(),
                    table_borders: child(node, "tblPr")
                        .and_then(|node| child(node, "tblBorders"))
                        .map(read_table_borders)
                        .unwrap_or_default(),
                },
            ))
        })
        .collect();
    let numbering = read_numbering(archive)?;
    Ok(DocxStyles {
        paragraph_defaults,
        run_defaults,
        styles,
        theme_fonts,
        numbering,
        counters: Default::default(),
    })
}

/// word/numbering.xml 을 읽는다.
fn read_numbering(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<NumberingDefs> {
    let Some(text) = read_zip_text(archive, "word/numbering.xml")? else {
        return Ok(NumberingDefs::default());
    };
    let xml = roxmltree::Document::parse(&text)?;
    let mut defs = NumberingDefs::default();
    for abs in xml.descendants().filter(|n| n.has_tag_name("abstractNum")) {
        let Some(id) = attribute_by_local_name(abs, "abstractNumId") else { continue };
        let mut levels = vec![NumLevel::default(); 9];
        for (i, level) in levels.iter_mut().enumerate() {
            level.start = 1;
            level.format = "decimal".into();
            level.text = format!("%{}.", i + 1);
            level.suffix = "tab".into();
        }
        for lvl in abs.children().filter(|n| n.has_tag_name("lvl")) {
            let Some(index) = attribute_by_local_name(lvl, "ilvl").and_then(|v| v.parse::<usize>().ok()) else { continue };
            if index > 8 {
                continue;
            }
            let val = |name: &str| child(lvl, name).and_then(|n| attribute_by_local_name(n, "val")).map(str::to_owned);
            let level = &mut levels[index];
            if let Some(start) = val("start").and_then(|v| v.parse().ok()) {
                level.start = start;
            }
            if let Some(format) = val("numFmt") {
                level.format = format;
            }
            level.text = val("lvlText").unwrap_or_default();
            if let Some(suffix) = val("suff") {
                level.suffix = suffix;
            }
            level.paragraph = child(lvl, "pPr").map(read_paragraph_properties).unwrap_or_default();
            level.run = child(lvl, "rPr").map(read_run_properties).unwrap_or_default();
        }
        defs.abstracts.insert(id.to_owned(), levels);
    }
    for num in xml.descendants().filter(|n| n.has_tag_name("num")) {
        let Some(id) = attribute_by_local_name(num, "numId") else { continue };
        let Some(abs) = child(num, "abstractNumId").and_then(|n| attribute_by_local_name(n, "val")) else { continue };
        let mut overrides = HashMap::new();
        for ov in num.children().filter(|n| n.has_tag_name("lvlOverride")) {
            let level = attribute_by_local_name(ov, "ilvl").and_then(|v| v.parse::<u8>().ok());
            let start = child(ov, "startOverride").and_then(|n| attribute_by_local_name(n, "val")).and_then(|v| v.parse::<i32>().ok());
            if let (Some(level), Some(start)) = (level, start) {
                overrides.insert(level, start);
            }
        }
        defs.nums.insert(id.to_owned(), (abs.to_owned(), overrides));
    }
    Ok(defs)
}

fn roman(mut n: i32, upper: bool) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let table = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
    let mut out = String::new();
    for (value, text) in table {
        while n >= value {
            out.push_str(text);
            n -= value;
        }
    }
    if upper { out.to_uppercase() } else { out }
}

fn letters(n: i32, upper: bool) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let base = if upper { b'A' } else { b'a' };
    let ch = (base + ((n - 1) % 26) as u8) as char;
    std::iter::repeat(ch).take(((n - 1) / 26 + 1) as usize).collect()
}

fn format_number(n: i32, format: &str) -> String {
    const CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳";
    const GANADA: &str = "가나다라마바사아자차카타파하";
    const CHOSUNG: &str = "ㄱㄴㄷㄹㅁㅂㅅㅇㅈㅊㅋㅌㅍㅎ";
    let pick = |set: &str| -> String {
        let chars: Vec<char> = set.chars().collect();
        if n >= 1 && (n as usize) <= chars.len() { chars[n as usize - 1].to_string() } else { n.to_string() }
    };
    match format {
        "lowerRoman" => roman(n, false),
        "upperRoman" => roman(n, true),
        "lowerLetter" => letters(n, false),
        "upperLetter" => letters(n, true),
        "decimalZero" => format!("{n:02}"),
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" => pick(CIRCLED),
        "ganada" => pick(GANADA),
        "chosung" => pick(CHOSUNG),
        "none" => String::new(),
        _ => n.to_string(),
    }
}

/// 글머리 기호 글꼴(Symbol·Wingdings)의 사설 영역 글자를 보이는 글자로.
fn bullet_glyph(text: &str) -> String {
    text.chars()
        .map(|c| match c as u32 {
            0xF0B7 | 0xF09F | 0x2022 => '•',
            0xF0A7 | 0xF06E | 0xF0FC => '▪',
            0xF0A8 => '□',
            0xF02D => '-',
            u if (0xF020..=0xF0FF).contains(&u) => '•',
            _ => c,
        })
        .collect()
}

/// 번호 붙은 문단의 번호 글자와 그 수준. 세기를 한 칸 올린다.
fn next_number_label(styles: &DocxStyles, num_id: &str, level: u8) -> Option<(String, NumLevel)> {
    let (abs_id, overrides) = styles.numbering.nums.get(num_id)?;
    let levels = styles.numbering.abstracts.get(abs_id)?;
    let lvl = levels.get(level as usize)?.clone();
    let mut counters = styles.counters.borrow_mut();
    let key = format!("{abs_id}#{num_id}");
    let state = counters.entry(key).or_insert([None; 9]);
    let start_of = |i: usize| overrides.get(&(i as u8)).copied().unwrap_or(levels[i].start);
    let li = level as usize;
    state[li] = Some(state[li].map_or(start_of(li), |v| v + 1));
    for deeper in state.iter_mut().skip(li + 1) {
        *deeper = None;
    }
    if lvl.format == "bullet" {
        return Some((bullet_glyph(&lvl.text), lvl));
    }
    let mut label = lvl.text.clone();
    for i in (0..=li).rev() {
        let value = state[i].unwrap_or_else(|| start_of(i));
        let shown = if lvl.format == "decimal" && i != li && levels[i].format != "decimal" {
            // 상위 수준이 문자여도 "1.1" 처럼 숫자로 이어 적는 문서가 많다
            format_number(value, &levels[i].format)
        } else if i == li {
            format_number(value, &lvl.format)
        } else {
            format_number(value, &levels[i].format)
        };
        label = label.replace(&format!("%{}", i + 1), &shown);
    }
    Some((label, lvl))
}

/// 머리말·꼬리말 문단들을 읽는다. 표 안의 문단도 차례로 꺼낸다.
#[allow(clippy::type_complexity)]
fn read_headers_footers(
    xml: &roxmltree::Document<'_>,
    body: roxmltree::Node<'_, '_>,
    styles: &DocxStyles,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<(Vec<DocxParagraph>, Vec<DocxParagraph>, Option<Vec<DocxParagraph>>, Option<Vec<DocxParagraph>>, f32, f32)> {
    let sections: Vec<_> = xml.descendants().filter(|n| n.has_tag_name("sectPr")).collect();
    let mut refs: HashMap<(bool, String), String> = HashMap::new();
    let mut title_page = false;
    for section in &sections {
        for r in section.children().filter(|n| n.has_tag_name("headerReference") || n.has_tag_name("footerReference")) {
            let is_header = r.has_tag_name("headerReference");
            let kind = attribute_by_local_name(r, "type").unwrap_or("default").to_owned();
            if let Some(id) = attribute_by_local_name(r, "id") {
                refs.entry((is_header, kind)).or_insert_with(|| id.to_owned());
            }
        }
        if child(*section, "titlePg").is_some_and(property_enabled) {
            title_page = true;
        }
    }
    let margins = body
        .children()
        .find(|n| n.has_tag_name("sectPr"))
        .or_else(|| sections.first().copied())
        .and_then(|s| child(s, "pgMar"));
    let header_distance = twips_attribute(margins, "header").unwrap_or(36.0);
    let footer_distance = twips_attribute(margins, "footer").unwrap_or(36.0);

    let mut load = |is_header: bool, kind: &str| -> Result<Option<Vec<DocxParagraph>>> {
        let Some(id) = refs.get(&(is_header, kind.to_owned())) else { return Ok(None) };
        let Some(target) = relationships.get(id) else { return Ok(None) };
        let path = resolve_part_target("word/document.xml", target);
        let Some(text) = read_zip_text(archive, &path)? else { return Ok(None) };
        let part = roxmltree::Document::parse(&text)?;
        let mut blocks = Vec::new();
        for p in part.descendants().filter(|n| n.has_tag_name("p")) {
            if p.ancestors().skip(1).any(|a| a.has_tag_name("p") || a.has_tag_name("txbxContent")) {
                continue;
            }
            read_paragraph(p, &mut blocks, styles, &HashMap::new(), archive)?;
        }
        // 글상자 안의 글(분류 표시 등)은 뒤에 이어 붙인다. 같은 글상자가
        // mc:Fallback(VML)에 한 번 더 있으므로 그쪽은 건너뛴다.
        for p in part.descendants().filter(|n| n.has_tag_name("p")) {
            let in_box = p.ancestors().skip(1).any(|a| a.has_tag_name("txbxContent"));
            let in_fallback = p.ancestors().any(|a| a.has_tag_name("Fallback"));
            let nested = p.ancestors().skip(1).take_while(|a| !a.has_tag_name("txbxContent")).any(|a| a.has_tag_name("p"));
            if in_box && !in_fallback && !nested {
                read_paragraph(p, &mut blocks, styles, &HashMap::new(), archive)?;
            }
        }
        Ok(Some(
            blocks
                .into_iter()
                .filter_map(|b| match b {
                    DocxBlock::Paragraph(p) => Some(p),
                    _ => None,
                })
                .collect(),
        ))
    };
    let header = load(true, "default")?.unwrap_or_default();
    let footer = load(false, "default")?.unwrap_or_default();
    let first_header = if title_page { Some(load(true, "first")?.unwrap_or_default()) } else { None };
    let first_footer = if title_page { Some(load(false, "first")?.unwrap_or_default()) } else { None };
    Ok((header, footer, first_header, first_footer, header_distance, footer_distance))
}

fn read_theme_fonts(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<DocxThemeFonts> {
    let Some(theme_xml) = read_zip_text(archive, "word/theme/theme1.xml")? else {
        return Ok(DocxThemeFonts::default());
    };
    let xml = roxmltree::Document::parse(&theme_xml)?;
    let font_scheme = xml
        .descendants()
        .find(|node| node.has_tag_name("fontScheme"));
    Ok(DocxThemeFonts {
        major_east_asia: font_scheme
            .and_then(|node| child(node, "majorFont"))
            .and_then(theme_east_asia_font),
        minor_east_asia: font_scheme
            .and_then(|node| child(node, "minorFont"))
            .and_then(theme_east_asia_font),
    })
}

fn theme_east_asia_font(group: roxmltree::Node<'_, '_>) -> Option<String> {
    child(group, "ea")
        .and_then(|node| attribute_by_local_name(node, "typeface"))
        .filter(|name| !name.is_empty())
        .or_else(|| {
            group
                .children()
                .find(|node| {
                    node.has_tag_name("font")
                        && attribute_by_local_name(*node, "script") == Some("Hans")
                })
                .and_then(|node| attribute_by_local_name(node, "typeface"))
        })
        .map(str::to_owned)
}

fn read_paragraph_properties(properties: roxmltree::Node<'_, '_>) -> ParagraphProperties {
    let spacing = child(properties, "spacing");
    let indentation = child(properties, "ind");
    ParagraphProperties {
        contextual_spacing: child(properties, "contextualSpacing").map(property_enabled),
        alignment: child(properties, "jc")
            .and_then(|node| attribute_by_local_name(node, "val"))
            .map(parse_alignment),
        indent_left: indentation
            .and_then(|node| numeric_attribute(node, "left"))
            .map(|twips| twips / 20.0),
        indent_right: indentation
            .and_then(|node| numeric_attribute(node, "right"))
            .map(|twips| twips / 20.0),
        spacing_before: spacing
            .and_then(|node| numeric_attribute(node, "before"))
            .map(|twips| twips / 20.0),
        spacing_after: spacing
            .and_then(|node| numeric_attribute(node, "after"))
            .map(|twips| twips / 20.0),
        line_spacing: spacing
            .and_then(|node| numeric_attribute(node, "line"))
            .map(|line| line / 240.0),
        fill: child(properties, "shd")
            .and_then(|node| attribute_by_local_name(node, "fill"))
            .and_then(parse_hex_color),
        bottom_border: child(properties, "pBdr")
            .and_then(|node| child(node, "bottom"))
            .and_then(read_border),
        first_line: indentation.and_then(|node| {
            numeric_attribute(node, "hanging")
                .map(|twips| -twips / 20.0)
                .or_else(|| numeric_attribute(node, "firstLine").map(|twips| twips / 20.0))
        }),
        justify: child(properties, "jc")
            .and_then(|node| attribute_by_local_name(node, "val"))
            .map(|value| matches!(value, "both" | "distribute" | "justify" | "lowKashida" | "mediumKashida" | "highKashida")),
        numbering: child(properties, "numPr").and_then(|node| {
            let id = child(node, "numId").and_then(|n| attribute_by_local_name(n, "val"))?;
            let level = child(node, "ilvl")
                .and_then(|n| attribute_by_local_name(n, "val"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0u8);
            Some((id.to_owned(), level.min(8)))
        }),
        tabs: child(properties, "tabs").map(|tabs| {
            tabs.children()
                .filter(|n| n.has_tag_name("tab"))
                .filter_map(|n| {
                    let pos = numeric_attribute(n, "pos")? / 20.0;
                    let val = attribute_by_local_name(n, "val").unwrap_or("left");
                    let align = match val {
                        "right" | "end" | "decimal" => TabAlign::Right,
                        "center" => TabAlign::Center,
                        _ => TabAlign::Left,
                    };
                    let leader = match attribute_by_local_name(n, "leader") {
                        Some("dot") => Some('.'),
                        Some("middleDot") => Some('·'),
                        Some("hyphen") => Some('-'),
                        Some("underscore" | "heavy") => Some('_'),
                        _ => None,
                    };
                    Some((TabStop { pos, align, leader }, val == "clear"))
                })
                .collect()
        }),
    }
}

fn read_run_properties(properties: roxmltree::Node<'_, '_>) -> RunProperties {
    let fonts = child(properties, "rFonts");
    RunProperties {
        font_size: child(properties, "sz")
            .and_then(|node| numeric_attribute(node, "val"))
            .map(|half_points| half_points / 2.0),
        font_name: fonts
            .and_then(|node| {
                attribute_by_local_name(node, "eastAsia")
                    .or_else(|| attribute_by_local_name(node, "ascii"))
                    .or_else(|| attribute_by_local_name(node, "hAnsi"))
            })
            .map(str::to_owned),
        font_theme: fonts
            .and_then(|node| {
                attribute_by_local_name(node, "eastAsiaTheme")
                    .or_else(|| attribute_by_local_name(node, "asciiTheme"))
                    .or_else(|| attribute_by_local_name(node, "hAnsiTheme"))
            })
            .and_then(|value| match value {
                "majorEastAsia" => Some(ThemeFontReference::MajorEastAsia),
                "minorEastAsia" => Some(ThemeFontReference::MinorEastAsia),
                _ => None,
            }),
        bold: child(properties, "b").map(property_enabled),
        italic: child(properties, "i").map(property_enabled),
        underline: child(properties, "u").map(property_enabled),
        color: child(properties, "color")
            .and_then(|node| attribute_by_local_name(node, "val"))
            .and_then(parse_hex_color),
        highlight: child(properties, "highlight")
            .and_then(|node| attribute_by_local_name(node, "val"))
            .and_then(parse_highlight_color),
    }
}

fn parse_highlight_color(value: &str) -> Option<PdfColor> {
    match value {
        "yellow" => Some(PdfColor::new(1.0, 1.0, 0.0)),
        "green" => Some(PdfColor::new(0.0, 1.0, 0.0)),
        "cyan" => Some(PdfColor::new(0.0, 1.0, 1.0)),
        "magenta" => Some(PdfColor::new(1.0, 0.0, 1.0)),
        "blue" => Some(PdfColor::new(0.0, 0.0, 1.0)),
        "red" => Some(PdfColor::new(1.0, 0.0, 0.0)),
        "darkBlue" => Some(PdfColor::new(0.0, 0.0, 0.5)),
        "darkCyan" => Some(PdfColor::new(0.0, 0.5, 0.5)),
        "darkGreen" => Some(PdfColor::new(0.0, 0.5, 0.0)),
        "darkMagenta" => Some(PdfColor::new(0.5, 0.0, 0.5)),
        "darkRed" => Some(PdfColor::new(0.5, 0.0, 0.0)),
        "darkYellow" => Some(PdfColor::new(0.5, 0.5, 0.0)),
        "darkGray" => Some(PdfColor::new(0.5, 0.5, 0.5)),
        "lightGray" => Some(PdfColor::new(0.75, 0.75, 0.75)),
        "black" => Some(PdfColor::BLACK),
        _ => None,
    }
}

fn read_border(border: roxmltree::Node<'_, '_>) -> Option<DocxBorder> {
    let style = attribute_by_local_name(border, "val")?;
    if matches!(style, "nil" | "none") {
        return None;
    }
    Some(DocxBorder {
        color: attribute_by_local_name(border, "color")
            .and_then(parse_hex_color)
            .unwrap_or(PdfColor::BLACK),
        width: numeric_attribute(border, "sz").unwrap_or(4.0) / 8.0,
        space: numeric_attribute(border, "space").unwrap_or(0.0),
        is_double: style == "double",
        pattern: match style {
            "dotDash" | "dashDotStroked" => BorderPattern::DotDash,
            "dotted" => BorderPattern::Dotted,
            "dashed" | "dashSmallGap" => BorderPattern::Dashed,
            _ => BorderPattern::Solid,
        },
    })
}

fn parse_alignment(value: &str) -> TextAlignment {
    match value {
        "center" => TextAlignment::Center,
        "right" | "end" => TextAlignment::Right,
        _ => TextAlignment::Left,
    }
}

fn property_enabled(node: roxmltree::Node<'_, '_>) -> bool {
    !matches!(
        attribute_by_local_name(node, "val"),
        Some("0" | "false" | "off" | "none")
    )
}

fn merge_paragraph_properties(target: &mut ParagraphProperties, source: &ParagraphProperties) {
    if source.contextual_spacing.is_some() {
        target.contextual_spacing = source.contextual_spacing;
    }
    if source.alignment.is_some() {
        target.alignment = source.alignment;
    }
    if source.indent_left.is_some() {
        target.indent_left = source.indent_left;
    }
    if source.indent_right.is_some() {
        target.indent_right = source.indent_right;
    }
    if source.spacing_before.is_some() {
        target.spacing_before = source.spacing_before;
    }
    if source.spacing_after.is_some() {
        target.spacing_after = source.spacing_after;
    }
    if source.line_spacing.is_some() {
        target.line_spacing = source.line_spacing;
    }
    if source.fill.is_some() {
        target.fill = source.fill;
    }
    if source.bottom_border.is_some() {
        target.bottom_border = source.bottom_border;
    }
    if source.first_line.is_some() {
        target.first_line = source.first_line;
    }
    if source.justify.is_some() {
        target.justify = source.justify;
    }
    if source.numbering.is_some() {
        target.numbering.clone_from(&source.numbering);
    }
    if let Some(new_tabs) = &source.tabs {
        // 탭은 쌓인다. clear 는 같은 자리의 위쪽 탭을 지운다.
        let mut merged: Vec<(TabStop, bool)> = target.tabs.clone().unwrap_or_default();
        for (tab, clear) in new_tabs {
            merged.retain(|(t, _)| (t.pos - tab.pos).abs() > 0.5);
            if !*clear {
                merged.push((*tab, false));
            }
        }
        target.tabs = Some(merged);
    }
}

fn merge_run_properties(target: &mut RunProperties, source: &RunProperties) {
    if source.font_size.is_some() {
        target.font_size = source.font_size;
    }
    if source.font_name.is_some() {
        target.font_name.clone_from(&source.font_name);
        target.font_theme = None;
    }
    if source.font_theme.is_some() {
        target.font_theme = source.font_theme;
        target.font_name = None;
    }
    if source.bold.is_some() {
        target.bold = source.bold;
    }
    if source.italic.is_some() {
        target.italic = source.italic;
    }
    if source.underline.is_some() {
        target.underline = source.underline;
    }
    if source.color.is_some() {
        target.color = source.color;
    }
    if source.highlight.is_some() {
        target.highlight = source.highlight;
    }
}

fn merge_style_chain(
    styles: &DocxStyles,
    style_id: &str,
    paragraph: &mut ParagraphProperties,
    run: &mut RunProperties,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    let Some(style) = styles.styles.get(style_id) else {
        return;
    };
    if let Some(parent) = &style.based_on {
        merge_style_chain(styles, parent, paragraph, run, depth + 1);
    }
    merge_paragraph_properties(paragraph, &style.paragraph);
    merge_run_properties(run, &style.run);
}

fn read_paragraph(
    paragraph: roxmltree::Node<'_, '_>,
    blocks: &mut Vec<DocxBlock>,
    styles: &DocxStyles,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<()> {
    if child(paragraph, "pPr")
        .and_then(|properties| child(properties, "pageBreakBefore"))
        .is_some_and(property_enabled)
    {
        blocks.push(DocxBlock::PageBreak);
    }

    let direct_properties = child(paragraph, "pPr")
        .map(read_paragraph_properties)
        .unwrap_or_default();
    let style_id = child(paragraph, "pPr")
        .and_then(|node| child(node, "pStyle"))
        .and_then(|node| attribute_by_local_name(node, "val"));
    let mut paragraph_properties = styles.paragraph_defaults.clone();
    let mut base_run_properties = styles.run_defaults.clone();
    if let Some(style_id) = style_id {
        merge_style_chain(
            styles,
            style_id,
            &mut paragraph_properties,
            &mut base_run_properties,
            0,
        );
    }
    // 번호: 스타일 < 번호 수준 < 문단 직접 순으로 들여쓰기가 덮인다.
    let numbering = direct_properties
        .numbering
        .clone()
        .or_else(|| paragraph_properties.numbering.clone())
        .filter(|(id, _)| id != "0");
    let label = numbering.and_then(|(id, level)| next_number_label(styles, &id, level));
    if let Some((_, level)) = &label {
        merge_paragraph_properties(&mut paragraph_properties, &level.paragraph);
    }
    merge_paragraph_properties(&mut paragraph_properties, &direct_properties);
    if let Some(properties) = child(paragraph, "pPr").and_then(|node| child(node, "rPr")) {
        merge_run_properties(&mut base_run_properties, &read_run_properties(properties));
    }

    let floating_images = read_vml_floating_images(paragraph, relationships, archive)?;

    let mut runs = Vec::new();
    if let Some((text, level)) = &label {
        let mut props = base_run_properties.clone();
        let mut level_run = level.run.clone();
        if level.format == "bullet" {
            // Symbol·Wingdings 글꼴은 기기에 없다. 기호는 이미 보이는 글자로 바꿨으니
            // 문단 글꼴로 그린다.
            level_run.font_name = None;
            level_run.font_theme = None;
        }
        merge_run_properties(&mut props, &level_run);
        props.underline = Some(false);
        props.highlight = None;
        let suffix = match level.suffix.as_str() {
            "space" => " ",
            "nothing" => "",
            _ => "\t",
        };
        if !text.is_empty() {
            runs.push(create_run(format!("{text}{suffix}"), &props, &styles.theme_fonts));
        }
    }
    let mut emitted_non_text = false;
    // 필드: 0 없음, 1 명령 읽는 중, 2 결과 숨김(쪽 번호로 바꿈), 3 결과 그대로
    let mut field_mode = 0u8;
    let mut field_instr = String::new();
    for run_node in paragraph
        .descendants()
        .filter(|node| node.has_tag_name("r"))
    {
        // fldSimple PAGE / NUMPAGES: 첫 글토막을 자리표시로, 나머지는 버린다.
        if let Some(simple) = run_node.ancestors().find(|n| n.has_tag_name("fldSimple")) {
            let instr = attribute_by_local_name(simple, "instr").unwrap_or("").trim().to_uppercase();
            let placeholder = field_placeholder(&instr);
            if let Some(ph) = placeholder {
                let first = simple.descendants().find(|n| n.has_tag_name("r"));
                if first == Some(run_node) {
                    let mut run_properties = base_run_properties.clone();
                    if let Some(properties) = child(run_node, "rPr") {
                        merge_run_properties(&mut run_properties, &read_run_properties(properties));
                    }
                    runs.push(create_run(ph.to_string(), &run_properties, &styles.theme_fonts));
                }
                continue;
            }
        }
        if run_node
            .ancestors()
            .take_while(|node| *node != paragraph)
            .any(|node| node.has_tag_name("txbxContent"))
            || run_node
                .descendants()
                .any(|node| node.has_tag_name("txbxContent"))
        {
            continue;
        }
        if let Some(drawing) = run_node
            .descendants()
            .find(|node| node.has_tag_name("drawing"))
        {
            push_paragraph_runs(blocks, &mut runs, style_id, &paragraph_properties);
            if let Some(image) = read_image(drawing, paragraph, relationships, archive)? {
                blocks.push(DocxBlock::Image(image));
                emitted_non_text = true;
            }
            continue;
        }

        let mut run_properties = base_run_properties.clone();
        if let Some(properties) = child(run_node, "rPr") {
            merge_run_properties(&mut run_properties, &read_run_properties(properties));
        }
        let mut text = String::new();
        for node in run_node.descendants() {
            if node.has_tag_name("fldChar") {
                match attribute_by_local_name(node, "fldCharType") {
                    Some("begin") => {
                        field_mode = 1;
                        field_instr.clear();
                    }
                    Some("separate") => {
                        if let Some(ph) = field_placeholder(&field_instr.trim().to_uppercase()) {
                            text.push(ph);
                            field_mode = 2;
                        } else {
                            field_mode = 3;
                        }
                    }
                    Some("end") => {
                        if field_mode == 1 {
                            if let Some(ph) = field_placeholder(&field_instr.trim().to_uppercase()) {
                                text.push(ph);
                            }
                        }
                        field_mode = 0;
                    }
                    _ => {}
                }
                continue;
            }
            if node.has_tag_name("instrText") {
                if field_mode == 1 {
                    field_instr.push_str(node.text().unwrap_or(""));
                }
                continue;
            }
            if field_mode == 1 || field_mode == 2 {
                continue;
            }
            if node.has_tag_name("t") {
                if let Some(value) = node.text() {
                    text.push_str(value);
                }
            } else if node.has_tag_name("tab") && !node.ancestors().any(|a| a.has_tag_name("tabs")) {
                text.push('\t');
            } else if node.has_tag_name("br") {
                if attribute_by_local_name(node, "type") == Some("page") {
                    if !text.is_empty() {
                        runs.push(create_run(
                            std::mem::take(&mut text),
                            &run_properties,
                            &styles.theme_fonts,
                        ));
                    }
                    push_paragraph_runs(blocks, &mut runs, style_id, &paragraph_properties);
                    blocks.push(DocxBlock::PageBreak);
                    emitted_non_text = true;
                } else {
                    text.push('\n');
                }
            }
        }
        if !text.is_empty() {
            runs.push(create_run(text, &run_properties, &styles.theme_fonts));
        }
    }
    if !runs.is_empty() || !emitted_non_text {
        let mut output =
            create_paragraph(std::mem::take(&mut runs), style_id, &paragraph_properties);
        output.floating_images = floating_images;
        blocks.push(DocxBlock::Paragraph(output));
    }
    Ok(())
}

/// 쪽 번호 필드 자리표시 글자. 그릴 때 실제 번호로 바뀐다.
const PAGE_FIELD: char = '\u{E000}';
const NUMPAGES_FIELD: char = '\u{E001}';

fn field_placeholder(instr: &str) -> Option<char> {
    let word = instr.split_whitespace().next()?;
    match word {
        "PAGE" => Some(PAGE_FIELD),
        "NUMPAGES" | "SECTIONPAGES" => Some(NUMPAGES_FIELD),
        _ => None,
    }
}

fn read_vml_floating_images(
    paragraph: roxmltree::Node<'_, '_>,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<Vec<DocxFloatingImage>> {
    let mut images = Vec::new();
    for shape in paragraph
        .descendants()
        .filter(|node| node.has_tag_name("shape"))
    {
        let Some(style) = shape.attribute("style") else {
            continue;
        };
        if !vml_style_value(style, "position")
            .is_some_and(|value| value.eq_ignore_ascii_case("absolute"))
        {
            continue;
        }
        let Some(textbox) = shape
            .descendants()
            .find(|node| node.has_tag_name("txbxContent"))
        else {
            continue;
        };
        let offset_x = vml_style_point(style, "margin-left").unwrap_or(0.0);
        let offset_y = vml_style_point(style, "margin-top").unwrap_or(0.0);
        let (inset_left, inset_top) = vml_textbox_insets(shape);
        for drawing in textbox
            .descendants()
            .filter(|node| node.has_tag_name("drawing"))
        {
            let image_paragraph = drawing
                .ancestors()
                .find(|node| node.has_tag_name("p"))
                .unwrap_or(paragraph);
            if let Some(image) = read_image(drawing, image_paragraph, relationships, archive)? {
                images.push(DocxFloatingImage {
                    image,
                    offset_x,
                    offset_y,
                    inset_left,
                    inset_top,
                    line_top_leading: VML_INLINE_IMAGE_TOP_LEADING,
                });
            }
        }
    }
    Ok(images)
}

fn vml_style_value<'a>(style: &'a str, name: &str) -> Option<&'a str> {
    style.split(';').find_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        property
            .trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim())
    })
}

fn vml_style_point(style: &str, name: &str) -> Option<f32> {
    parse_vml_length(vml_style_value(style, name)?)
}

fn parse_vml_length(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(points) = value.strip_suffix("pt") {
        points.trim().parse().ok()
    } else if let Some(inches) = value.strip_suffix("in") {
        inches
            .trim()
            .parse::<f32>()
            .ok()
            .map(|length| length * 72.0)
    } else if let Some(pixels) = value.strip_suffix("px") {
        pixels
            .trim()
            .parse::<f32>()
            .ok()
            .map(|length| length * 0.75)
    } else {
        value.parse().ok()
    }
}

fn vml_textbox_insets(shape: roxmltree::Node<'_, '_>) -> (f32, f32) {
    let inset = shape
        .descendants()
        .find(|node| node.has_tag_name("textbox"))
        .and_then(|node| node.attribute("inset"));
    let Some(inset) = inset else {
        return (7.2, 3.6);
    };
    let values: Vec<_> = inset
        .split(',')
        .filter_map(|value| parse_vml_length(value.trim()))
        .collect();
    (
        values.first().copied().unwrap_or(7.2),
        values.get(1).copied().unwrap_or(3.6),
    )
}

fn create_run(text: String, properties: &RunProperties, theme_fonts: &DocxThemeFonts) -> DocxRun {
    DocxRun {
        text,
        font_size: properties.font_size.unwrap_or(BODY_FONT_SIZE),
        font_name: properties
            .font_name
            .clone()
            .or_else(|| match properties.font_theme {
                Some(ThemeFontReference::MajorEastAsia) => theme_fonts.major_east_asia.clone(),
                Some(ThemeFontReference::MinorEastAsia) => theme_fonts.minor_east_asia.clone(),
                None => None,
            }),
        bold: properties.bold.unwrap_or(false),
        italic: properties.italic.unwrap_or(false),
        underline: properties.underline.unwrap_or(false),
        color: properties.color.unwrap_or(PdfColor::BLACK),
        highlight: properties.highlight,
    }
}

fn create_paragraph(
    runs: Vec<DocxRun>,
    style_id: Option<&str>,
    properties: &ParagraphProperties,
) -> DocxParagraph {
    let spacing_after = if runs.is_empty() {
        0.0
    } else {
        properties.spacing_after.unwrap_or(0.0)
    };
    DocxParagraph {
        runs,
        floating_images: Vec::new(),
        style_id: style_id.map(str::to_owned),
        contextual_spacing: properties.contextual_spacing.unwrap_or(false),
        alignment: properties.alignment.unwrap_or(TextAlignment::Left),
        indent_left: properties.indent_left.unwrap_or(0.0),
        indent_right: properties.indent_right.unwrap_or(0.0),
        spacing_before: properties.spacing_before.unwrap_or(0.0),
        spacing_after,
        // 문서가 줄 간격을 안 정하면 워드 기본은 '한 줄'(1.0)이다.
        line_spacing: properties.line_spacing.unwrap_or(1.0),
        line_spacing_explicit: properties.line_spacing.is_some(),
        fill: properties.fill,
        bottom_border: properties.bottom_border,
        first_line: properties.first_line.unwrap_or(0.0),
        justify: properties.justify.unwrap_or(false),
        tabs: {
            let mut tabs: Vec<TabStop> = properties
                .tabs
                .as_ref()
                .map(|t| t.iter().filter(|(_, clear)| !clear).map(|(tab, _)| *tab).collect())
                .unwrap_or_default();
            tabs.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            tabs
        },
    }
}

fn push_paragraph_runs(
    blocks: &mut Vec<DocxBlock>,
    runs: &mut Vec<DocxRun>,
    style_id: Option<&str>,
    properties: &ParagraphProperties,
) {
    if !runs.is_empty() {
        blocks.push(DocxBlock::Paragraph(create_paragraph(
            std::mem::take(runs),
            style_id,
            properties,
        )));
    }
}

fn read_document_relationships(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<HashMap<String, String>> {
    let Some(relationships_xml) = read_zip_text(archive, "word/_rels/document.xml.rels")? else {
        return Ok(HashMap::new());
    };
    let xml = roxmltree::Document::parse(&relationships_xml)?;
    Ok(xml
        .descendants()
        .filter(|node| node.has_tag_name("Relationship"))
        .filter_map(|node| {
            Some((
                node.attribute("Id")?.to_owned(),
                node.attribute("Target")?.to_owned(),
            ))
        })
        .collect())
}

fn read_image(
    drawing: roxmltree::Node<'_, '_>,
    paragraph: roxmltree::Node<'_, '_>,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<Option<DocxImage>> {
    let Some(container) = drawing
        .descendants()
        .find(|node| node.has_tag_name("inline"))
    else {
        return Ok(None);
    };
    let Some(relationship_id) = container
        .descendants()
        .find(|node| node.has_tag_name("blip"))
        .and_then(|node| attribute_by_local_name(node, "embed"))
    else {
        return Ok(None);
    };
    let Some(target) = relationships.get(relationship_id) else {
        return Ok(None);
    };
    let image_path = resolve_part_target("word/document.xml", target);
    let Ok(mut entry) = archive.by_name(&image_path) else {
        return Ok(None);
    };
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    drop(entry);

    let (data, pixel_width, pixel_height) = if let Some((width, height)) = jpeg_dimensions(&bytes) {
        (
            DocxImageData::Jpeg {
                data: bytes,
                width,
                height,
            },
            width,
            height,
        )
    } else {
        let Ok(decoded) = image::load_from_memory(&bytes) else {
            return Ok(None);
        };
        let width = u16::try_from(decoded.width()).unwrap_or(u16::MAX);
        let height = u16::try_from(decoded.height()).unwrap_or(u16::MAX);
        (
            DocxImageData::Rgba {
                data: decoded.into_rgba8().into_raw(),
                width,
                height,
            },
            width,
            height,
        )
    };
    let extent = child(container, "extent");
    let width = extent
        .and_then(|node| numeric_attribute(node, "cx"))
        .map(|emu| emu / 12_700.0)
        .unwrap_or(pixel_width as f32 * 0.75);
    let height = extent
        .and_then(|node| numeric_attribute(node, "cy"))
        .map(|emu| emu / 12_700.0)
        .unwrap_or(pixel_height as f32 * 0.75);

    Ok(Some(DocxImage {
        data,
        width,
        height,
        alignment: paragraph_alignment(paragraph),
    }))
}

fn resolve_part_target(source_part: &str, target: &str) -> String {
    let combined = if target.starts_with('/') {
        target.trim_start_matches('/').to_owned()
    } else {
        let parent = source_part
            .rsplit_once('/')
            .map(|(path, _)| path)
            .unwrap_or("");
        format!("{parent}/{target}")
    };
    let normalized = combined.replace('\\', "/");
    let mut segments = Vec::new();
    for segment in normalized.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }
    segments.join("/")
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u16, u16)> {
    if !data.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut index = 2;
    while index + 3 < data.len() {
        while index < data.len() && data[index] != 0xff {
            index += 1;
        }
        while index < data.len() && data[index] == 0xff {
            index += 1;
        }
        if index >= data.len() {
            break;
        }
        let marker = data[index];
        index += 1;
        if matches!(marker, 0xd8 | 0xd9) {
            continue;
        }
        if index + 1 >= data.len() {
            break;
        }
        let segment_length = u16::from_be_bytes([data[index], data[index + 1]]) as usize;
        if segment_length < 2 || index + segment_length > data.len() {
            break;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
            && segment_length >= 7
        {
            let height = u16::from_be_bytes([data[index + 3], data[index + 4]]);
            let width = u16::from_be_bytes([data[index + 5], data[index + 6]]);
            return Some((width, height));
        }
        index += segment_length;
    }
    None
}

/// 표 스타일의 테두리를 찾는다. 없으면 상속 사슬을 따라 올라간다.
fn style_table_borders(styles: &DocxStyles, style_id: &str) -> Option<DocxCellBorders> {
    let mut current = Some(style_id.to_owned());
    // 사슬이 자기를 가리키는 문서가 있어 횟수를 묶는다.
    for _ in 0..8 {
        let style = styles.styles.get(current.as_deref()?)?;
        if style.table_borders != DocxCellBorders::default() {
            return Some(style.table_borders.clone());
        }
        current = style.based_on.clone();
    }
    None
}

fn read_table(
    table: roxmltree::Node<'_, '_>,
    styles: &DocxStyles,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    legacy_cjk_metrics: bool,
) -> Result<DocxTable> {
    let properties = child(table, "tblPr");
    let cell_margins = properties.and_then(|node| child(node, "tblCellMar"));
    // 표 전체 칸 여백이 없으면 첫 칸의 위·아래 여백(tcMar)을 쓴다 — 칸마다 같은
    // 여백을 적어 두는 문서가 많고, 그걸 버리면 줄 높이가 반으로 준다.
    // 좌우는 칸마다 크게 다른 문서(주소 라벨 등)가 있어 따르지 않는다.
    let vertical_margins = cell_margins.or_else(|| {
        table
            .descendants()
            .find(|node| node.has_tag_name("tcMar") && node.parent().is_some_and(|p| p.has_tag_name("tcPr")))
    });
    let style_id = properties
        .and_then(|node| child(node, "tblStyle"))
        .and_then(|node| attribute_by_local_name(node, "val"));
    // 표가 직접 적은 테두리가 먼저고, 없으면 표 스타일이 주는 것을 쓴다.
    //
    // 전에는 `style_id == Some("af2")` 였다 — 개발자가 시험한 어느 문서의
    // 스타일 ID 가 박혀 있어서, 보통 문서의 Table Grid 는 걸리지 않았다.
    // tblBorders 도 읽기만 하고 값은 버렸다. 그래서 선이 하나도 안 나왔다.
    let inherited_borders = properties
        .and_then(|node| child(node, "tblBorders"))
        .map(read_table_borders)
        .or_else(|| style_id.and_then(|id| style_table_borders(styles, id)))
        .unwrap_or_default();
    let column_widths = child(table, "tblGrid")
        .map(|grid| {
            grid.children()
                .filter(|node| node.has_tag_name("gridCol"))
                .filter_map(|node| twips_attribute(Some(node), "w"))
                .collect()
        })
        .unwrap_or_default();
    let mut rows = Vec::new();
    for row in table.children().filter(|node| node.has_tag_name("tr")) {
        let height = child(row, "trPr")
            .and_then(|node| child(node, "trHeight"))
            .and_then(|node| twips_attribute(Some(node), "val"));
        let mut cells = Vec::new();
        for cell in row.children().filter(|node| node.has_tag_name("tc")) {
            cells.push(read_table_cell(
                cell,
                styles,
                relationships,
                archive,
                &inherited_borders,
            )?);
        }
        rows.push(DocxTableRow { cells, height });
    }
    Ok(DocxTable {
        column_widths,
        rows,
        alignment: properties
            .and_then(|node| child(node, "jc"))
            .and_then(|node| attribute_by_local_name(node, "val"))
            .map(parse_alignment)
            .unwrap_or(TextAlignment::Left),
        cell_margin_left: table_margin(cell_margins, "left", TABLE_CELL_PADDING_HORIZONTAL),
        cell_margin_right: table_margin(cell_margins, "right", TABLE_CELL_PADDING_HORIZONTAL),
        cell_margin_top: table_margin(vertical_margins, "top", 0.0),
        cell_margin_bottom: table_margin(vertical_margins, "bottom", 0.0),
        legacy_cjk_metrics,
    })
}

fn table_margin(margins: Option<roxmltree::Node<'_, '_>>, side: &str, default: f32) -> f32 {
    margins
        .and_then(|node| child(node, side))
        .and_then(|node| twips_attribute(Some(node), "w"))
        .unwrap_or(default)
}

fn read_table_cell(
    cell: roxmltree::Node<'_, '_>,
    styles: &DocxStyles,
    relationships: &HashMap<String, String>,
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    inherited_borders: &DocxCellBorders,
) -> Result<DocxTableCell> {
    let properties = child(cell, "tcPr");
    let paragraphs: Vec<_> = cell
        .children()
        .filter(|node| node.has_tag_name("p"))
        .collect();
    let mut runs: Vec<DocxRun> = Vec::new();
    for paragraph in &paragraphs {
        let paragraph_runs = read_table_cell_runs(*paragraph, styles);
        if paragraph_runs.is_empty() {
            continue;
        }
        if let Some(previous) = runs.last_mut() {
            previous.text.push('\n');
        }
        runs.extend(paragraph_runs);
    }
    let first_paragraph = paragraphs.first().copied();
    let first_paragraph_properties = first_paragraph
        .and_then(|paragraph| child(paragraph, "pPr"))
        .map(read_paragraph_properties)
        .unwrap_or_default();
    let mut images = Vec::new();
    for paragraph in &paragraphs {
        for drawing in paragraph
            .descendants()
            .filter(|node| node.has_tag_name("drawing"))
        {
            if let Some(image) = read_image(drawing, *paragraph, relationships, archive)? {
                images.push(image);
            }
        }
    }

    Ok(DocxTableCell {
        runs,
        images,
        width: properties
            .and_then(|node| child(node, "tcW"))
            .and_then(|node| twips_attribute(Some(node), "w")),
        spacing_before: first_paragraph_properties.spacing_before.unwrap_or(0.0),
        spacing_after: first_paragraph_properties.spacing_after.unwrap_or(0.0),
        line_spacing: first_paragraph_properties.line_spacing.unwrap_or(1.0),
        fill: properties
            .and_then(|node| child(node, "shd"))
            .and_then(|node| attribute_by_local_name(node, "fill"))
            .and_then(parse_hex_color),
        alignment: first_paragraph
            .and_then(|node| node.descendants().find(|child| child.has_tag_name("jc")))
            .and_then(|node| attribute_by_local_name(node, "val"))
            .map(|value| match value {
                "center" => TextAlignment::Center,
                "right" | "end" => TextAlignment::Right,
                _ => TextAlignment::Left,
            })
            .unwrap_or(TextAlignment::Left),
        vertical_alignment: properties
            .and_then(|node| child(node, "vAlign"))
            .and_then(|node| attribute_by_local_name(node, "val"))
            .map(|value| match value {
                "center" => VerticalAlignment::Center,
                "bottom" => VerticalAlignment::Bottom,
                _ => VerticalAlignment::Top,
            })
            .unwrap_or(VerticalAlignment::Top),
        grid_span: properties
            .and_then(|node| child(node, "gridSpan"))
            .and_then(|node| numeric_attribute(node, "val"))
            .map(|value| value.max(1.0) as usize)
            .unwrap_or(1),
        vertical_merge: properties
            .and_then(|node| child(node, "vMerge"))
            .map(|node| match attribute_by_local_name(node, "val") {
                Some("restart") => VerticalMerge::Restart,
                _ => VerticalMerge::Continue,
            })
            .unwrap_or(VerticalMerge::None),
        borders: DocxCellBorders {
            top: read_cell_border(properties, "top", inherited_borders.top),
            right: read_cell_border(properties, "right", inherited_borders.right),
            bottom: read_cell_border(properties, "bottom", inherited_borders.bottom),
            left: read_cell_border(properties, "left", inherited_borders.left),
        },
    })
}

fn read_table_cell_runs(paragraph: roxmltree::Node<'_, '_>, styles: &DocxStyles) -> Vec<DocxRun> {
    let style_id = child(paragraph, "pPr")
        .and_then(|node| child(node, "pStyle"))
        .and_then(|node| attribute_by_local_name(node, "val"));
    let mut paragraph_properties = styles.paragraph_defaults.clone();
    let mut base_run_properties = styles.run_defaults.clone();
    if let Some(style_id) = style_id {
        merge_style_chain(
            styles,
            style_id,
            &mut paragraph_properties,
            &mut base_run_properties,
            0,
        );
    }
    if let Some(properties) = child(paragraph, "pPr").and_then(|node| child(node, "rPr")) {
        merge_run_properties(&mut base_run_properties, &read_run_properties(properties));
    }

    paragraph
        .descendants()
        .filter(|node| node.has_tag_name("r"))
        .filter_map(|run_node| {
            let mut properties = base_run_properties.clone();
            if let Some(direct) = child(run_node, "rPr") {
                merge_run_properties(&mut properties, &read_run_properties(direct));
            }
            let text = paragraph_text(run_node);
            (!text.is_empty()).then(|| create_run(text, &properties, &styles.theme_fonts))
        })
        .collect()
}

/// 표 수준 테두리를 셀에 적용할 모양으로 읽는다.
///
/// 바깥 네 변과 안쪽 두 선이 따로 적혀 있다. 셀 하나하나의 위치를 여기서는
/// 모르므로, 안쪽 선이 있으면 그것을 네 변에 쓴다 — 격자가 되고, 그것이
/// Table Grid 가 눈에 보이는 모양이다.
fn read_table_borders(node: roxmltree::Node<'_, '_>) -> DocxCellBorders {
    let side = |name: &str| child(node, name).and_then(read_border);
    let inside_h = side("insideH");
    let inside_v = side("insideV");
    DocxCellBorders {
        top: inside_h.or_else(|| side("top")),
        bottom: inside_h.or_else(|| side("bottom")),
        left: inside_v.or_else(|| side("left")),
        right: inside_v.or_else(|| side("right")),
    }
}

fn read_cell_border(
    properties: Option<roxmltree::Node<'_, '_>>,
    side: &str,
    inherited: Option<DocxBorder>,
) -> Option<DocxBorder> {
    properties
        .and_then(|node| child(node, "tcBorders"))
        .and_then(|node| child(node, side))
        .and_then(read_border)
        .or(inherited)
}

fn paragraph_text(paragraph: roxmltree::Node<'_, '_>) -> String {
    let mut text = String::new();
    for node in paragraph.descendants() {
        if node.has_tag_name("t") {
            if let Some(value) = node.text() {
                text.push_str(value);
            }
        } else if node.has_tag_name("tab") {
            text.push('\t');
        } else if node.has_tag_name("br") {
            text.push('\n');
        }
    }
    text
}

fn paragraph_alignment(paragraph: roxmltree::Node<'_, '_>) -> TextAlignment {
    paragraph
        .descendants()
        .find(|node| node.has_tag_name("jc"))
        .and_then(|node| attribute_by_local_name(node, "val"))
        .map(|value| match value {
            "center" => TextAlignment::Center,
            "right" | "end" => TextAlignment::Right,
            _ => TextAlignment::Left,
        })
        .unwrap_or(TextAlignment::Left)
}

fn child<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    local_name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    node.children().find(|child| child.has_tag_name(local_name))
}

fn twips_attribute(node: Option<roxmltree::Node<'_, '_>>, name: &str) -> Option<f32> {
    node.and_then(|node| numeric_attribute(node, name))
        .map(|twips| twips / 20.0)
}

fn numeric_attribute(node: roxmltree::Node<'_, '_>, name: &str) -> Option<f32> {
    attribute_by_local_name(node, name)?.parse().ok()
}

fn parse_hex_color(value: &str) -> Option<PdfColor> {
    if value.len() != 6 || value.eq_ignore_ascii_case("auto") {
        return None;
    }
    let red = u8::from_str_radix(&value[0..2], 16).ok()?;
    let green = u8::from_str_radix(&value[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(PdfColor::new(
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ))
}

fn attribute_by_local_name<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    local_name: &str,
) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == local_name)
        .map(|attribute| attribute.value())
}

fn render_docx(doc: &mut PdfDocument, document: &DocxDocument, options: &ConversionOptions) {
    let first_page = doc.pages().len();
    render_docx_body(doc, document, options);
    let page_size = options.page_size.unwrap_or(document.page_size);
    render_headers_footers(doc, document, page_size, first_page);
}

fn render_docx_body(doc: &mut PdfDocument, document: &DocxDocument, options: &ConversionOptions) {
    let mut page_index = doc.pages().len();
    let page_size = options.page_size.unwrap_or(document.page_size);
    let margins = document.margins;
    doc.add_page(page_size.width, page_size.height);
    let mut y = page_size.height - margins.top - BODY_FONT_SIZE;
    let max_width = page_size.width - margins.left - margins.right;

    for (block_index, block) in document.blocks.iter().enumerate() {
        match block {
            DocxBlock::PageBreak => {
                page_index = doc.pages().len();
                doc.add_page(page_size.width, page_size.height);
                y = page_size.height - margins.top - BODY_FONT_SIZE;
            }
            DocxBlock::Paragraph(paragraph) => {
                if block_index + 1 == document.blocks.len()
                    && paragraph.runs.is_empty()
                    && paragraph.fill.is_none()
                    && paragraph.bottom_border.is_none()
                {
                    continue;
                }
                let previous_paragraph =
                    block_index
                        .checked_sub(1)
                        .and_then(|index| match document.blocks.get(index) {
                            Some(DocxBlock::Paragraph(previous)) => Some(previous),
                            _ => None,
                        });
                let previous_spacing_after = previous_paragraph.map_or(0.0, |previous| {
                    if previous.contextual_spacing && previous.style_id == paragraph.style_id {
                        0.0
                    } else {
                        previous.spacing_after
                    }
                });
                let spacing_before = if document.compatibility_mode <= 12 {
                    paragraph.spacing_before
                } else if previous_paragraph.is_some() {
                    (paragraph.spacing_before - previous_spacing_after).max(0.0)
                } else {
                    paragraph.spacing_before
                };
                let spacing_after = if paragraph.contextual_spacing
                    && matches!(
                        document.blocks.get(block_index + 1),
                        Some(DocxBlock::Paragraph(next)) if next.style_id == paragraph.style_id
                    ) {
                    0.0
                } else {
                    paragraph.spacing_after
                };
                let previous_grid_ascent = previous_paragraph
                    .map(|previous| paragraph_line_metrics(previous, document.grid_line_pitch).2);
                render_paragraph(
                    doc,
                    paragraph,
                    (
                        spacing_before,
                        spacing_after,
                        document.grid_line_pitch,
                        previous_grid_ascent,
                    ),
                    page_size,
                    margins,
                    &mut page_index,
                    &mut y,
                );
            }
            DocxBlock::Table(table) => render_table(
                doc,
                table,
                page_size,
                margins,
                max_width,
                &mut page_index,
                &mut y,
            ),
            DocxBlock::Image(image) => render_image(
                doc,
                image,
                page_size,
                margins,
                max_width,
                &mut page_index,
                &mut y,
            ),
        }
    }
}

fn render_paragraph(
    doc: &mut PdfDocument,
    paragraph: &DocxParagraph,
    layout: (f32, f32, f32, Option<f32>),
    page_size: PageSize,
    margins: DocxMargins,
    page_index: &mut usize,
    y: &mut f32,
) {
    let (spacing_before, spacing_after, grid_line_pitch, previous_grid_ascent) = layout;
    let (_, line_height, baseline_offset) = paragraph_line_metrics(paragraph, grid_line_pitch);
    let text_width = (page_size.width - margins.left - margins.right).max(1.0);
    let lines = flow_lines(paragraph, text_width);
    let line_count = lines.len().max(1);
    let mut top = *y + BODY_FONT_SIZE;
    // 첫 줄도 안 들어가면 다음 쪽으로. 그 뒤로는 줄마다 넘긴다.
    if top - spacing_before - line_height < margins.bottom {
        *page_index = doc.pages().len();
        doc.add_page(page_size.width, page_size.height);
        top = page_size.height - margins.top;
    }
    top -= spacing_before;
    if grid_line_pitch > 0.0 {
        if let Some(previous_ascent) = previous_grid_ascent.filter(|ascent| *ascent > 0.0) {
            top -= baseline_offset - previous_ascent;
        }
    }
    let box_left = margins.left + paragraph.indent_left;
    let box_width = (text_width - paragraph.indent_left - paragraph.indent_right).max(1.0);
    let first_page = *page_index;
    let first_top = top;
    let mut line_top = top;

    for index in 0..line_count {
        if index > 0 && line_top - line_height < margins.bottom {
            *page_index = doc.pages().len();
            doc.add_page(page_size.width, page_size.height);
            line_top = page_size.height - margins.top;
        }
        if let Some(fill) = paragraph.fill {
            doc.page_mut(*page_index)
                .expect("page index is valid")
                .add_rect(box_left, line_top - line_height, box_width, line_height, fill);
        }
        if let Some(line) = lines.get(index) {
            let baseline = line_top - baseline_offset;
            let page_number = *page_index + 1;
            draw_flow_line(doc.page_mut(*page_index).expect("page index is valid"), paragraph, line, margins.left, baseline, page_number, 0);
        }
        line_top -= line_height;
    }

    for floating in &paragraph.floating_images {
        let image_id = register_image(doc, &floating.image);
        doc.page_mut(first_page)
            .expect("page index is valid")
            .add_image(
                image_id,
                margins.left + floating.offset_x + floating.inset_left,
                first_top - floating.offset_y
                    - floating.inset_top
                    - floating.line_top_leading
                    - floating.image.height,
                floating.image.width,
                floating.image.height,
            );
    }

    let bottom = line_top;
    if let Some(border) = paragraph.bottom_border {
        let border_y = bottom - border.space;
        let page = doc.page_mut(*page_index).expect("page index is valid");
        draw_border(
            page,
            (box_left, border_y),
            (box_left + box_width, border_y),
            border,
        );
        if border.is_double {
            draw_border(
                page,
                (box_left, border_y - border.width * 2.0),
                (box_left + box_width, border_y - border.width * 2.0),
                border,
            );
        }
    }
    *y = bottom - spacing_after - BODY_FONT_SIZE;
}

// ---------------------------------------------------------------------------
// 문단 줄 배치: 낱말 단위 줄바꿈, 탭(채움 점선 포함), 양쪽 맞춤, 내어쓰기

#[derive(Debug, Clone, Copy, PartialEq)]
enum PieceKind {
    Word,
    Space,
    Tab,
    Newline,
}

#[derive(Debug, Clone)]
struct Piece {
    run: usize,
    text: String,
    width: f32,
    kind: PieceKind,
}

#[derive(Debug, Clone)]
struct Placed {
    run: usize,
    text: String,
    x: f32,
    width: f32,
    tab: Option<Option<char>>,
    space: bool,
}

#[derive(Debug, Clone, Default)]
struct FlowLine {
    items: Vec<Placed>,
}

const DEFAULT_TAB: f32 = 36.0;

fn is_breakable_cjk(ch: char) -> bool {
    // 한자·가나는 글자마다 끊는다. 한글은 띄어쓰기로 끊는다(워드 기본).
    matches!(ch as u32, 0x2e80..=0x2fff | 0x3000..=0x30ff | 0x3400..=0x9fff | 0xf900..=0xfaff | 0xff00..=0xffef)
}

fn display_text(text: &str, page: usize, total: usize) -> String {
    if !text.contains([PAGE_FIELD, NUMPAGES_FIELD]) {
        return text.to_owned();
    }
    text.replace(PAGE_FIELD, &page.to_string())
        .replace(NUMPAGES_FIELD, &if total > 0 { total.to_string() } else { page.to_string() })
}

fn pieces_of(runs: &[DocxRun]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for (ri, run) in runs.iter().enumerate() {
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut Vec<Piece>| {
            if !word.is_empty() {
                let measure = display_text(word, 88, 88);
                out.push(Piece { run: ri, width: run_width(run, &measure), text: std::mem::take(word), kind: PieceKind::Word });
            }
        };
        for ch in run.text.chars() {
            match ch {
                '\n' => {
                    flush(&mut word, &mut out);
                    out.push(Piece { run: ri, text: String::new(), width: 0.0, kind: PieceKind::Newline });
                }
                '\t' => {
                    flush(&mut word, &mut out);
                    out.push(Piece { run: ri, text: String::new(), width: 0.0, kind: PieceKind::Tab });
                }
                ' ' | '\u{3000}' => {
                    flush(&mut word, &mut out);
                    let text = ch.to_string();
                    out.push(Piece { run: ri, width: run_width(run, &text), text, kind: PieceKind::Space });
                }
                c if is_breakable_cjk(c) && !is_forbidden_line_start(c) && !matches!(c, '、' | '。' | '，' | '．' | '：' | '；' | '！' | '？' | '」' | '』' | '）') => {
                    flush(&mut word, &mut out);
                    word.push(c);
                    flush(&mut word, &mut out);
                }
                c => {
                    // 닫는 문장부호는 앞 낱말에 붙인다 (줄 첫머리 금칙).
                    if word.is_empty() && (is_forbidden_line_start(c) || matches!(c, '、' | '。' | '，' | '．' | '」' | '』' | '）')) {
                        if let Some(last) = out.last_mut().filter(|p| p.kind == PieceKind::Word && p.run == ri) {
                            last.text.push(c);
                            last.width = run_width(run, &display_text(&last.text, 88, 88));
                            continue;
                        }
                    }
                    word.push(c);
                }
            }
        }
        flush(&mut word, &mut out);
    }
    out
}

fn next_tab_stop(paragraph: &DocxParagraph, x: f32) -> TabStop {
    if paragraph.first_line < 0.0 && x < paragraph.indent_left - 0.01 {
        let custom_before = paragraph.tabs.iter().find(|t| t.pos > x + 0.01 && t.pos < paragraph.indent_left);
        if let Some(t) = custom_before {
            return *t;
        }
        return TabStop { pos: paragraph.indent_left, align: TabAlign::Left, leader: None };
    }
    if let Some(t) = paragraph.tabs.iter().find(|t| t.pos > x + 0.01) {
        return *t;
    }
    let next = ((x + 0.01) / DEFAULT_TAB).floor() * DEFAULT_TAB + DEFAULT_TAB;
    TabStop { pos: next, align: TabAlign::Left, leader: None }
}

/// 문단을 줄로 나눈다. x 는 본문 왼쪽 여백 기준.
fn flow_lines(paragraph: &DocxParagraph, text_width: f32) -> Vec<FlowLine> {
    let mut pieces = pieces_of(&paragraph.runs);
    let limit = (text_width - paragraph.indent_right).max(paragraph.indent_left + 1.0);
    let mut lines: Vec<FlowLine> = Vec::new();
    let mut ends: Vec<bool> = Vec::new(); // 줄이 문단 끝/강제 줄바꿈으로 끝났는가
    let mut line = FlowLine::default();
    let mut first = true;
    let mut x = paragraph.indent_left + paragraph.first_line;
    let start_x = |first: bool| if first { paragraph.indent_left + paragraph.first_line } else { paragraph.indent_left };
    let mut i = 0;
    let mut guard = 0usize;
    while i < pieces.len() {
        guard += 1;
        if guard > pieces.len() * 4 + 1000 {
            break;
        }
        let piece = pieces[i].clone();
        match piece.kind {
            PieceKind::Newline => {
                lines.push(std::mem::take(&mut line));
                ends.push(true);
                first = false;
                x = start_x(false);
                i += 1;
            }
            PieceKind::Space => {
                // 줄바꿈으로 넘어온 줄의 첫 공백만 버린다. 문단 첫머리 공백은 들여쓰기다.
                let wrapped_start = line.items.is_empty() && ends.last() == Some(&false);
                if !wrapped_start {
                    line.items.push(Placed { run: piece.run, text: piece.text.clone(), x, width: piece.width, tab: None, space: true });
                    x += piece.width;
                }
                i += 1;
            }
            PieceKind::Tab => {
                let stop = next_tab_stop(paragraph, x);
                // 오른쪽·가운데 탭: 다음 탭 전까지의 글 폭만큼 당긴다.
                let following: f32 = pieces[i + 1..]
                    .iter()
                    .take_while(|p| !matches!(p.kind, PieceKind::Tab | PieceKind::Newline))
                    .map(|p| p.width)
                    .sum();
                let target = match stop.align {
                    TabAlign::Left => stop.pos,
                    TabAlign::Right => stop.pos - following,
                    TabAlign::Center => stop.pos - following / 2.0,
                };
                let width = (target - x).max(0.0);
                line.items.push(Placed { run: piece.run, text: String::new(), x, width, tab: Some(stop.leader), space: false });
                x += width;
                i += 1;
            }
            PieceKind::Word => {
                let has_content = line.items.iter().any(|p| !p.space);
                if x + piece.width > limit + 0.01 && has_content {
                    while line.items.last().is_some_and(|p| p.space) {
                        line.items.pop();
                    }
                    lines.push(std::mem::take(&mut line));
                    ends.push(false);
                    first = false;
                    x = start_x(false);
                    continue;
                }
                if x + piece.width > limit + 0.01 && !has_content {
                    // 한 낱말이 줄보다 길다: 글자 단위로 자른다.
                    let run = &paragraph.runs[piece.run];
                    let mut part = String::new();
                    let mut part_width = 0.0;
                    let mut rest = String::new();
                    let mut overflow = false;
                    for ch in piece.text.chars() {
                        if overflow {
                            rest.push(ch);
                            continue;
                        }
                        let cw = run_width(run, &ch.to_string());
                        if x + part_width + cw > limit + 0.01 && !part.is_empty() {
                            overflow = true;
                            rest.push(ch);
                        } else {
                            part.push(ch);
                            part_width += cw;
                        }
                    }
                    line.items.push(Placed { run: piece.run, text: part, x, width: part_width, tab: None, space: false });
                    if rest.is_empty() {
                        x += part_width;
                        i += 1;
                    } else {
                        lines.push(std::mem::take(&mut line));
                        ends.push(false);
                        first = false;
                        x = start_x(false);
                        let rest_width = run_width(run, &rest);
                        pieces[i] = Piece { run: piece.run, text: rest, width: rest_width, kind: PieceKind::Word };
                    }
                    continue;
                }
                line.items.push(Placed { run: piece.run, text: piece.text.clone(), x, width: piece.width, tab: None, space: false });
                x += piece.width;
                i += 1;
            }
        }
    }
    while line.items.last().is_some_and(|p| p.space) {
        line.items.pop();
    }
    lines.push(line);
    ends.push(true);
    let _ = first;
    align_lines(paragraph, &mut lines, &ends, limit);
    lines
}

fn align_lines(paragraph: &DocxParagraph, lines: &mut [FlowLine], ends: &[bool], limit: f32) {
    for (index, line) in lines.iter_mut().enumerate() {
        let Some(last) = line.items.last() else { continue };
        let right = last.x + last.width;
        let free = limit - right;
        if free <= 0.0 {
            continue;
        }
        let last_tab = line.items.iter().rposition(|p| p.tab.is_some());
        let natural_end = ends.get(index).copied().unwrap_or(true);
        if paragraph.justify && !natural_end {
            let from = last_tab.map_or(0, |t| t + 1);
            let spaces = line.items[from..].iter().filter(|p| p.space).count();
            if spaces > 0 && free < limit * 0.5 {
                let gap = free / spaces as f32;
                let mut shift = 0.0;
                for item in line.items[from..].iter_mut() {
                    item.x += shift;
                    if item.space {
                        item.width += gap;
                        shift += gap;
                    }
                }
            }
            continue;
        }
        if last_tab.is_some() {
            continue;
        }
        let offset = match paragraph.alignment {
            TextAlignment::Center => free / 2.0,
            TextAlignment::Right => free,
            TextAlignment::Left => 0.0,
        };
        if offset > 0.0 {
            for item in &mut line.items {
                item.x += offset;
            }
        }
    }
}

/// 한 줄을 그린다. left 는 본문 왼쪽 여백(쪽 좌표).
fn draw_flow_line(
    page: &mut crate::pdf::PdfPage,
    paragraph: &DocxParagraph,
    line: &FlowLine,
    left: f32,
    baseline: f32,
    page_number: usize,
    total_pages: usize,
) {
    for item in &line.items {
        let run = &paragraph.runs[item.run];
        let x = left + item.x;
        if let Some(leader) = item.tab {
            if let Some(ch) = leader {
                let glyph = ch.to_string();
                let cw = run_width(run, &glyph).max(0.5);
                let count = ((item.width - cw) / cw).floor();
                if count >= 1.0 {
                    let dots: String = std::iter::repeat(ch).take(count as usize).collect();
                    let start = x + item.width - count * cw - cw * 0.5;
                    page.add_styled_text_with_font(&dots, start, baseline, run.font_size, run.color, false, false, run.font_name.as_deref());
                }
            }
            continue;
        }
        if item.text.is_empty() {
            continue;
        }
        let text = display_text(&item.text, page_number, total_pages);
        if let Some(highlight) = run.highlight {
            page.add_rect(x, baseline - run.font_size * 0.24, item.width, run.font_size * 1.18, highlight);
        }
        if !item.space {
            page.add_styled_text_with_font(&text, x, baseline, run.font_size, run.color, run.bold, run.italic, run.font_name.as_deref());
        }
        if run.underline && !text.trim().is_empty() || run.underline && item.space {
            page.add_line(x, baseline - 1.5, x + item.width, baseline - 1.5, run.color, 0.5);
        }
    }
}

/// 머리말·꼬리말을 모든 쪽에 그린다 (쪽 수가 정해진 뒤).
fn render_headers_footers(doc: &mut PdfDocument, document: &DocxDocument, page_size: PageSize, first_page: usize) {
    let total = doc.pages().len() - first_page;
    let text_width = (page_size.width - document.margins.left - document.margins.right).max(1.0);
    for n in 0..total {
        let index = first_page + n;
        let is_first = n == 0;
        let header = if is_first { document.first_header.as_ref().unwrap_or(&document.header) } else { &document.header };
        let footer = if is_first { document.first_footer.as_ref().unwrap_or(&document.footer) } else { &document.footer };
        // 머리말: 위에서 header_distance 아래부터 내려간다.
        let mut top = page_size.height - document.header_distance;
        for paragraph in header {
            let (_, line_height, baseline_offset) = paragraph_line_metrics(paragraph, 0.0);
            top -= paragraph.spacing_before;
            for line in flow_lines(paragraph, text_width) {
                if let Some(page) = doc.page_mut(index) {
                    draw_flow_line(page, paragraph, &line, document.margins.left, top - baseline_offset, n + 1, total);
                }
                top -= line_height;
            }
            top -= paragraph.spacing_after;
        }
        // 꼬리말: 아래에서 footer_distance 위에 마지막 줄이 놓이게 높이를 먼저 잰다.
        let laid: Vec<(Vec<FlowLine>, f32, f32)> = footer
            .iter()
            .map(|p| {
                let (_, lh, bo) = paragraph_line_metrics(p, 0.0);
                (flow_lines(p, text_width), lh, bo)
            })
            .collect();
        let height: f32 = laid
            .iter()
            .zip(footer)
            .map(|((lines, lh, _), p)| p.spacing_before + lines.len().max(1) as f32 * lh + p.spacing_after)
            .sum();
        let mut top = document.footer_distance + height;
        for ((lines, lh, bo), paragraph) in laid.iter().zip(footer) {
            top -= paragraph.spacing_before;
            for line in lines {
                if let Some(page) = doc.page_mut(index) {
                    draw_flow_line(page, paragraph, line, document.margins.left, top - bo, n + 1, total);
                }
                top -= lh;
            }
            top -= paragraph.spacing_after;
        }
    }
}

fn paragraph_line_metrics(paragraph: &DocxParagraph, grid_line_pitch: f32) -> (f32, f32, f32) {
    let font_size = paragraph
        .runs
        .iter()
        .map(|run| run.font_size)
        .fold(BODY_FONT_SIZE, f32::max);
    // 글꼴마다 '한 줄' 높이가 다르다. 맑은 고딕은 워드에서 크기의 1.73배다
    // (워드 PDF 에서 잰 값: 11pt·줄간격 1.333 → 25.4pt).
    let font_factor = if paragraph.runs.iter().filter_map(|run| run.font_name.as_deref()).any(|name| {
        let lower = name.to_ascii_lowercase();
        name.contains("맑은") || lower.contains("malgun")
    }) {
        1.73
    } else {
        1.18
    };
    let mut line_height = font_size * font_factor * paragraph.line_spacing;
    if grid_line_pitch <= 0.0 {
        return (font_size, line_height, font_size);
    }

    if paragraph.line_spacing_explicit {
        line_height = line_height.max(grid_line_pitch * paragraph.line_spacing);
        let text_length: usize = paragraph
            .runs
            .iter()
            .map(|run| run.text.chars().count())
            .sum();
        let is_centered_cjk_heading = paragraph.alignment == TextAlignment::Center
            && font_size >= 14.0
            && text_length <= 12
            && paragraph
                .runs
                .iter()
                .any(|run| run.text.chars().any(is_cjk_character));
        if is_centered_cjk_heading {
            line_height =
                line_height.max((font_size * 1.29 / grid_line_pitch).ceil() * grid_line_pitch);
        }
    } else if font_size <= grid_line_pitch {
        line_height = grid_line_pitch;
    } else {
        line_height = line_height.max(grid_line_pitch);
    }

    let font_name = paragraph
        .runs
        .iter()
        .find_map(|run| run.font_name.as_deref());
    let baseline_offset = if font_name.is_some_and(is_tall_cjk_font_name) {
        (line_height - font_size) / 2.0 + font_size * 0.86
    } else {
        (line_height + font_size) / 2.0
    };
    (font_size, line_height, baseline_offset)
}

fn is_cjk_character(ch: char) -> bool {
    matches!(ch as u32, 0x2e80..=0x9fff | 0xf900..=0xfaff)
}

fn is_tall_cjk_font_name(font_name: &str) -> bool {
    let normalized = font_name.trim().to_ascii_lowercase();
    [
        "kaiti",
        "simsun",
        "simhei",
        "nsimsun",
        "fangsong",
        "dengxian",
        "microsoft yahei",
    ]
    .iter()
    .any(|name| normalized.contains(name))
        || ["宋体", "黑体", "楷体", "仿宋", "等线", "微软雅黑"]
            .iter()
            .any(|name| font_name.contains(name))
}

fn wrap_paragraph_runs(runs: &[DocxRun], max_width: f32) -> Vec<Vec<DocxRun>> {
    let mut lines = vec![Vec::new()];
    let mut line_width = 0.0;
    for run in runs {
        let mut segment = String::new();
        for ch in run.text.chars() {
            if ch == '\n' {
                push_run_segment(lines.last_mut().expect("line exists"), run, &mut segment);
                lines.push(Vec::new());
                line_width = 0.0;
                continue;
            }
            let char_width = run_width(run, &ch.to_string());
            if line_width + char_width > max_width && line_width > 0.0 {
                push_run_segment(lines.last_mut().expect("line exists"), run, &mut segment);
                lines.push(Vec::new());
                line_width = 0.0;
            }
            segment.push(ch);
            line_width += char_width;
        }
        push_run_segment(lines.last_mut().expect("line exists"), run, &mut segment);
    }
    lines
}

fn push_run_segment(line: &mut Vec<DocxRun>, run: &DocxRun, segment: &mut String) {
    if segment.is_empty() {
        return;
    }
    let mut output = run.clone();
    output.text = std::mem::take(segment);
    line.push(output);
}

fn run_width(run: &DocxRun, text: &str) -> f32 {
    styled_text_width_with_font(
        text,
        run.font_size,
        run.bold,
        run.italic,
        run.font_name.as_deref(),
    )
}

fn render_image(
    doc: &mut PdfDocument,
    image: &DocxImage,
    page_size: PageSize,
    margins: DocxMargins,
    max_width: f32,
    page_index: &mut usize,
    y: &mut f32,
) {
    let mut width = image.width;
    let mut height = image.height;
    if width > max_width {
        let scale = max_width / width;
        width *= scale;
        height *= scale;
    }
    let mut top = *y + BODY_FONT_SIZE;
    if top - height < margins.bottom {
        *page_index = doc.pages().len();
        doc.add_page(page_size.width, page_size.height);
        top = page_size.height - margins.top;
    }
    let x = match image.alignment {
        TextAlignment::Left => margins.left,
        TextAlignment::Center => margins.left + (max_width - width) / 2.0,
        TextAlignment::Right => page_size.width - margins.right - width,
    };
    let image_id = match &image.data {
        DocxImageData::Jpeg {
            data,
            width,
            height,
        } => doc.add_jpeg_image(data.clone(), *width, *height),
        DocxImageData::Rgba {
            data,
            width,
            height,
        } => doc.add_rgba_image(data.clone(), *width, *height),
    };
    doc.page_mut(*page_index)
        .expect("page index is valid")
        .add_image(image_id, x, top - height, width, height);
    *y = top - height - BODY_FONT_SIZE;
}

fn render_table(
    doc: &mut PdfDocument,
    table: &DocxTable,
    page_size: PageSize,
    margins: DocxMargins,
    max_width: f32,
    page_index: &mut usize,
    y: &mut f32,
) {
    let max_columns = table
        .rows
        .iter()
        .map(|row| row.cells.iter().map(|cell| cell.grid_span).sum())
        .max()
        .unwrap_or(0);
    if max_columns == 0 {
        return;
    }
    let mut widths = if table.column_widths.len() >= max_columns {
        table.column_widths[..max_columns].to_vec()
    } else {
        let inferred_widths: Vec<_> = (0..max_columns)
            .map(|index| {
                table
                    .rows
                    .iter()
                    .find_map(|row| row.cells.get(index).and_then(|cell| cell.width))
            })
            .collect();
        if inferred_widths.iter().all(Option::is_some) {
            inferred_widths.into_iter().flatten().collect()
        } else {
            vec![max_width / max_columns as f32; max_columns]
        }
    };
    let width_sum: f32 = widths.iter().sum();
    if width_sum > max_width && width_sum > 0.0 {
        let scale = max_width / width_sum;
        for width in &mut widths {
            *width *= scale;
        }
    }

    let table_width: f32 = widths.iter().sum();
    let table_left = match table.alignment {
        TextAlignment::Left => margins.left,
        TextAlignment::Center => margins.left + (max_width - table_width) / 2.0,
        TextAlignment::Right => page_size.width - margins.right - table_width,
    };

    let mut row_heights: Vec<f32> = table
        .rows
        .iter()
        .map(|row| {
            let mut grid_index = 0;
            let content_height = row
                .cells
                .iter()
                .map(|cell| {
                    let width = spanned_width(&widths, grid_index, cell.grid_span);
                    grid_index += cell.grid_span;
                    if cell.vertical_merge != VerticalMerge::None {
                        return 0.0;
                    }
                    table_cell_content_height(table, cell, width)
                })
                .fold(0.0_f32, f32::max)
                .max(BODY_FONT_SIZE);
            row.height
                .map_or(content_height, |height| height.max(content_height))
        })
        .collect();

    for (row_index, row) in table.rows.iter().enumerate() {
        let mut grid_index = 0;
        for cell in &row.cells {
            if cell.vertical_merge == VerticalMerge::Restart {
                let mut end_row = row_index;
                for next_row in (row_index + 1)..table.rows.len() {
                    let Some(next_cell) = cell_at_grid(&table.rows[next_row], grid_index) else {
                        break;
                    };
                    if next_cell.vertical_merge != VerticalMerge::Continue {
                        break;
                    }
                    end_row = next_row;
                }
                let width = spanned_width(&widths, grid_index, cell.grid_span);
                let required_height = table_cell_content_height(table, cell, width);
                let current_height: f32 = row_heights[row_index..=end_row].iter().sum();
                if required_height > current_height {
                    row_heights[end_row] += required_height - current_height;
                }
            }
            grid_index += cell.grid_span;
        }
    }

    let mut row_top = *y + BODY_FONT_SIZE;
    for (row_index, row) in table.rows.iter().enumerate() {
        let row_height = row_heights[row_index];

        if row_top - row_height < margins.bottom {
            *page_index = doc.pages().len();
            doc.add_page(page_size.width, page_size.height);
            row_top = page_size.height - margins.top;
        }

        let mut grid_index = 0;
        for cell in &row.cells {
            let x = table_left + widths.iter().take(grid_index).sum::<f32>();
            let width = spanned_width(&widths, grid_index, cell.grid_span);
            if cell.vertical_merge == VerticalMerge::Continue {
                grid_index += cell.grid_span;
                continue;
            }
            let cell_height = if cell.vertical_merge == VerticalMerge::Restart {
                merged_cell_height(table, &row_heights, row_index, grid_index)
            } else {
                row_height
            };
            let bottom = row_top - cell_height;
            let page = doc.page_mut(*page_index).expect("page index is valid");
            if let Some(fill) = cell.fill {
                page.add_rect(x, bottom, width, cell_height, fill);
            }
            let wrapped_lines = wrap_table_cell_runs(
                &cell.runs,
                (width - table.cell_margin_left - table.cell_margin_right).max(1.0),
            );
            let line_height = table_cell_line_height(table, cell);
            let text_height = wrapped_lines.len() as f32 * line_height;
            let font_size = table_cell_font_size(cell);
            let image_height: f32 = cell
                .images
                .iter()
                .map(|image| {
                    image_render_size(
                        image,
                        (width - table.cell_margin_left - table.cell_margin_right).max(1.0),
                    )
                    .1
                })
                .sum();
            let content_height =
                cell.spacing_before + text_height + image_height + cell.spacing_after;
            let content_top = match cell.vertical_alignment {
                VerticalAlignment::Top => row_top - table.cell_margin_top,
                VerticalAlignment::Center => bottom + (cell_height + content_height) / 2.0,
                VerticalAlignment::Bottom => bottom + table.cell_margin_bottom + content_height,
            };
            let mut text_y = content_top - cell.spacing_before - font_size;
            for line in &wrapped_lines {
                let text_width: f32 = line.iter().map(|run| run_width(run, &run.text)).sum();
                let text_x = match cell.alignment {
                    TextAlignment::Left => x + table.cell_margin_left,
                    TextAlignment::Center => x + (width - text_width) / 2.0,
                    TextAlignment::Right => x + width - table.cell_margin_right - text_width,
                };
                let mut run_x = text_x.max(x + TABLE_BORDER_WIDTH);
                for run in line {
                    let width = run_width(run, &run.text);
                    if let Some(highlight) = run.highlight {
                        page.add_rect(
                            run_x - 0.7,
                            text_y - run.font_size * 0.24,
                            width + 1.4,
                            run.font_size * 1.18,
                            highlight,
                        );
                    }
                    page.add_styled_text_with_font(
                        &run.text,
                        run_x,
                        text_y,
                        run.font_size,
                        run.color,
                        run.bold,
                        run.italic,
                        run.font_name.as_deref(),
                    );
                    run_x += width;
                }
                text_y -= line_height;
            }
            let mut image_top = content_top - cell.spacing_before - text_height;
            for image in &cell.images {
                let (image_width, image_height) = image_render_size(
                    image,
                    (width - table.cell_margin_left - table.cell_margin_right).max(1.0),
                );
                let image_x = match image.alignment {
                    TextAlignment::Left => x + table.cell_margin_left,
                    TextAlignment::Center => x + (width - image_width) / 2.0,
                    TextAlignment::Right => x + width - table.cell_margin_right - image_width,
                };
                let image_id = register_image(doc, image);
                doc.page_mut(*page_index)
                    .expect("page index is valid")
                    .add_image(
                        image_id,
                        image_x,
                        image_top - image_height,
                        image_width,
                        image_height,
                    );
                image_top -= image_height;
            }
            let page = doc.page_mut(*page_index).expect("page index is valid");
            if let Some(border) = cell.borders.top {
                draw_border(page, (x, row_top), (x + width, row_top), border);
            }
            if let Some(border) = cell.borders.bottom {
                draw_border(page, (x, bottom), (x + width, bottom), border);
            }
            if let Some(border) = cell.borders.left {
                draw_border(page, (x, bottom), (x, row_top), border);
            }
            if let Some(border) = cell.borders.right {
                draw_border(page, (x + width, bottom), (x + width, row_top), border);
            }
            grid_index += cell.grid_span;
        }
        row_top -= row_height;
    }
    *y = row_top - BODY_FONT_SIZE;
}

fn draw_border(
    page: &mut crate::pdf::PdfPage,
    start: (f32, f32),
    end: (f32, f32),
    border: DocxBorder,
) {
    let pattern: &[f32] = match border.pattern {
        BorderPattern::Solid => &[],
        BorderPattern::DotDash => &[1.0, 1.0, 3.0, 1.0],
        BorderPattern::Dotted => &[0.8, 1.2],
        BorderPattern::Dashed => &[3.0, 2.0],
    };
    page.add_line_with_dash_pattern(start, end, border.color, border.width, pattern);
}

fn spanned_width(widths: &[f32], grid_index: usize, grid_span: usize) -> f32 {
    widths.iter().skip(grid_index).take(grid_span).sum()
}

fn merged_cell_height(
    table: &DocxTable,
    row_heights: &[f32],
    start_row: usize,
    grid_index: usize,
) -> f32 {
    let mut height = row_heights[start_row];
    for (row_index, row) in table.rows.iter().enumerate().skip(start_row + 1) {
        let Some(cell) = cell_at_grid(row, grid_index) else {
            break;
        };
        if cell.vertical_merge != VerticalMerge::Continue {
            break;
        }
        height += row_heights[row_index];
    }
    height
}

fn cell_at_grid(row: &DocxTableRow, target_grid_index: usize) -> Option<&DocxTableCell> {
    let mut grid_index = 0;
    for cell in &row.cells {
        if grid_index == target_grid_index {
            return Some(cell);
        }
        grid_index += cell.grid_span;
    }
    None
}

fn table_cell_content_height(table: &DocxTable, cell: &DocxTableCell, width: f32) -> f32 {
    let content_width = (width - table.cell_margin_left - table.cell_margin_right).max(1.0);
    let lines = wrap_table_cell_runs(&cell.runs, content_width);
    let text_height = cell.spacing_before
        + lines.len() as f32 * table_cell_line_height(table, cell)
        + cell.spacing_after;
    let image_height: f32 = cell
        .images
        .iter()
        .map(|image| image_render_size(image, content_width).1)
        .sum();
    text_height + image_height + table.cell_margin_top + table.cell_margin_bottom
}

fn table_cell_line_height(table: &DocxTable, cell: &DocxTableCell) -> f32 {
    let fonts = || cell.runs.iter().filter_map(|run| run.font_name.as_deref());
    let metrics_factor = if table.legacy_cjk_metrics && fonts().any(is_tall_cjk_font_name) {
        1.35
    } else if fonts().any(|name| name.contains("맑은") || name.to_ascii_lowercase().contains("malgun")) {
        1.73
    } else {
        1.18
    };
    table_cell_font_size(cell) * metrics_factor * cell.line_spacing
}

fn table_cell_font_size(cell: &DocxTableCell) -> f32 {
    cell.runs
        .iter()
        .map(|run| run.font_size)
        .reduce(f32::max)
        .unwrap_or(BODY_FONT_SIZE)
}

fn image_render_size(image: &DocxImage, max_width: f32) -> (f32, f32) {
    if image.width <= max_width {
        return (image.width, image.height);
    }
    let scale = max_width / image.width;
    (image.width * scale, image.height * scale)
}

fn register_image(doc: &mut PdfDocument, image: &DocxImage) -> usize {
    match &image.data {
        DocxImageData::Jpeg {
            data,
            width,
            height,
        } => doc.add_jpeg_image(data.clone(), *width, *height),
        DocxImageData::Rgba {
            data,
            width,
            height,
        } => doc.add_rgba_image(data.clone(), *width, *height),
    }
}

fn wrap_table_cell_runs(runs: &[DocxRun], max_width: f32) -> Vec<Vec<DocxRun>> {
    let Some(first_run) = runs.first() else {
        return Vec::new();
    };
    let mut measurement_run = first_run.clone();
    measurement_run.font_name = runs.iter().find_map(|run| run.font_name.clone());
    let (text, styled_characters) = normalize_table_cell_runs(runs);
    let wrapped_text = wrap_styled_text_with_font(
        &text,
        max_width,
        measurement_run.font_size,
        measurement_run.bold,
        measurement_run.italic,
        measurement_run.font_name.as_deref(),
    );
    let mut characters = styled_characters.into_iter();
    wrapped_text
        .into_iter()
        .map(|line| {
            let mut output = Vec::new();
            for _ in line.chars() {
                if let Some(character) = characters.next() {
                    push_styled_character(&mut output, character);
                }
            }
            output
        })
        .collect()
}

fn normalize_table_cell_runs(runs: &[DocxRun]) -> (String, Vec<DocxRun>) {
    let mut text = String::new();
    let mut characters = Vec::new();
    let mut pending_space = false;
    let mut line_has_text = false;
    for run in runs {
        for character in run.text.replace('\t', "    ").chars() {
            if character == '\n' {
                text.push('\n');
                pending_space = false;
                line_has_text = false;
            } else if character.is_whitespace() {
                pending_space = line_has_text;
            } else {
                if pending_space {
                    let mut space = run.clone();
                    space.text = " ".to_owned();
                    text.push(' ');
                    characters.push(space);
                }
                let mut styled_character = run.clone();
                styled_character.text = character.to_string();
                text.push(character);
                characters.push(styled_character);
                pending_space = false;
                line_has_text = true;
            }
        }
    }
    (text, characters)
}

fn push_styled_character(line: &mut Vec<DocxRun>, character: DocxRun) {
    if let Some(last) = line.last_mut() {
        if same_run_style(last, &character) {
            last.text.push_str(&character.text);
            return;
        }
    }
    line.push(character);
}

fn same_run_style(left: &DocxRun, right: &DocxRun) -> bool {
    left.font_size == right.font_size
        && left.font_name == right.font_name
        && left.bold == right.bold
        && left.italic == right.italic
        && left.underline == right.underline
        && left.color == right.color
        && left.highlight == right.highlight
}

fn wrap_styled_text_with_font(
    text: &str,
    max_width: f32,
    font_size: f32,
    bold: bool,
    italic: bool,
    font_name: Option<&str>,
) -> Vec<String> {
    let text = text.replace('\t', "    ");
    let mut lines = Vec::new();

    for forced_line in text.lines() {
        let mut current = String::new();
        for word in forced_line.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };

            if styled_text_width_with_font(&candidate, font_size, bold, italic, font_name)
                <= max_width
            {
                current = candidate;
            } else if word.chars().next().is_some_and(is_forbidden_line_start)
                && !current.is_empty()
            {
                let trailing = current.pop().expect("preceding character exists");
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                current.push(trailing);
                current.push_str(word);
            } else {
                if !current.is_empty() {
                    lines.push(current);
                }
                current = append_wrapped_word(
                    &mut lines, word, max_width, font_size, bold, italic, font_name,
                );
            }
        }

        if current.is_empty() {
            lines.push(String::new());
        } else {
            lines.push(current);
        }
    }

    lines
}

fn append_wrapped_word(
    lines: &mut Vec<String>,
    word: &str,
    max_width: f32,
    font_size: f32,
    bold: bool,
    italic: bool,
    font_name: Option<&str>,
) -> String {
    if styled_text_width_with_font(word, font_size, bold, italic, font_name) <= max_width {
        return word.to_owned();
    }

    let mut chunks = split_word_to_width(word, max_width, font_size, bold, italic, font_name);
    let current = chunks.pop().unwrap_or_default();
    for chunk in chunks {
        lines.push(chunk);
    }
    current
}

fn split_word_to_width(
    word: &str,
    max_width: f32,
    font_size: f32,
    bold: bool,
    italic: bool,
    font_name: Option<&str>,
) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in word.chars() {
        let mut candidate = current.clone();
        candidate.push(ch);
        if !current.is_empty()
            && styled_text_width_with_font(&candidate, font_size, bold, italic, font_name)
                > max_width
        {
            if is_forbidden_line_start(ch) {
                let trailing = current.pop().expect("preceding character exists");
                if !current.is_empty() {
                    chunks.push(std::mem::take(&mut current));
                }
                current.push(trailing);
            } else if current.chars().last().is_some_and(is_forbidden_line_end) {
                let opening = current.pop().expect("line-ending character exists");
                if !current.is_empty() {
                    chunks.push(std::mem::take(&mut current));
                }
                current.push(opening);
            } else {
                chunks.push(std::mem::take(&mut current));
            }
        }
        current.push(ch);
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn is_forbidden_line_end(ch: char) -> bool {
    matches!(
        ch,
        '(' | '[' | '{' | '（' | '［' | '｛' | '《' | '〈' | '「' | '『'
    )
}

fn is_forbidden_line_start(ch: char) -> bool {
    matches!(
        ch,
        ')' | ']' | '}' | '）' | '］' | '｝' | '》' | '〉' | '」' | '』'
    )
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    use super::{
        convert_docx_bytes, plain_paragraph, read_docx_document, wrap_styled_text_with_font,
        DocxBlock, DocxImageData,
    };
    use crate::ConversionOptions;

    fn create_docx(document_xml: &str) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            archive
                .start_file("word/document.xml", SimpleFileOptions::default())
                .unwrap();
            archive.write_all(document_xml.as_bytes()).unwrap();
            archive.finish().unwrap();
        }
        output.into_inner()
    }

    fn create_docx_with_settings(document_xml: &str, settings_xml: &str) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            for (path, data) in [
                ("word/document.xml", document_xml.as_bytes()),
                ("word/settings.xml", settings_xml.as_bytes()),
            ] {
                archive
                    .start_file(path, SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(data).unwrap();
            }
            archive.finish().unwrap();
        }
        output.into_inner()
    }

    fn create_docx_with_image(document_xml: &str, relationships_xml: &str) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 255, 255]));
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();

        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            for (path, data) in [
                ("word/document.xml", document_xml.as_bytes()),
                ("word/_rels/document.xml.rels", relationships_xml.as_bytes()),
                ("word/media/image1.png", png.get_ref()),
            ] {
                archive
                    .start_file(path, SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(data).unwrap();
            }
            archive.finish().unwrap();
        }
        output.into_inner()
    }

    #[test]
    fn wraps_unspaced_text_without_truncating() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let lines = wrap_styled_text_with_font(text, 40.0, 12.0, false, false, None);

        assert!(lines.len() > 1);
        assert_eq!(lines.concat(), text);
        assert!(lines.iter().all(|line| {
            crate::pdf::styled_text_width_with_font(line, 12.0, false, false, None) <= 40.0
        }));
    }

    #[test]
    fn keeps_cjk_opening_punctuation_off_line_end() {
        let text = "机房（扶梯上机舱 ）";
        let max_width =
            crate::pdf::styled_text_width_with_font("机房（", 8.0, true, false, Some("宋体"));

        let lines = wrap_styled_text_with_font(text, max_width, 8.0, true, false, Some("宋体"));

        assert_eq!(lines, vec!["机房", "（扶梯", "上机", "舱）"]);
        assert!(lines.iter().all(|line| !line.ends_with('（')));
    }

    #[test]
    fn preserves_explicit_page_breaks() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t><w:br w:type="page"/><w:t>Second</w:t></w:r></w:p><w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(
            document.blocks,
            vec![
                DocxBlock::Paragraph(plain_paragraph("First".to_owned())),
                DocxBlock::PageBreak,
                DocxBlock::Paragraph(plain_paragraph("Second".to_owned())),
                DocxBlock::PageBreak,
                DocxBlock::Paragraph(plain_paragraph("Third".to_owned())),
            ]
        );
        let pdf = convert_docx_bytes(&input, &ConversionOptions::default()).unwrap();
        assert_eq!(
            pdf.windows(b"/Type /Page /Parent".len())
                .filter(|chunk| *chunk == b"/Type /Page /Parent")
                .count(),
            3
        );
    }

    #[test]
    fn ignores_disabled_page_break_before() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:pageBreakBefore w:val="0"/></w:pPr><w:r><w:t>Second</w:t></w:r></w:p><w:p><w:pPr><w:pageBreakBefore w:val="false"/></w:pPr><w:r><w:t>Third</w:t></w:r></w:p><w:p><w:pPr><w:pageBreakBefore w:val="1"/></w:pPr><w:r><w:t>Fourth</w:t></w:r></w:p></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(
            document.blocks,
            vec![
                DocxBlock::Paragraph(plain_paragraph("First".to_owned())),
                DocxBlock::Paragraph(plain_paragraph("Second".to_owned())),
                DocxBlock::Paragraph(plain_paragraph("Third".to_owned())),
                DocxBlock::PageBreak,
                DocxBlock::Paragraph(plain_paragraph("Fourth".to_owned())),
            ]
        );
        let pdf = convert_docx_bytes(&input, &ConversionOptions::default()).unwrap();
        assert_eq!(
            pdf.windows(b"/Type /Page /Parent".len())
                .filter(|chunk| *chunk == b"/Type /Page /Parent")
                .count(),
            2
        );
    }

    #[test]
    fn ignores_page_break_before_in_change_history() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t></w:r></w:p><w:p><w:pPr><w:pPrChange><w:pPr><w:pageBreakBefore w:val="1"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>Second</w:t></w:r></w:p></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(
            document.blocks,
            vec![
                DocxBlock::Paragraph(plain_paragraph("First".to_owned())),
                DocxBlock::Paragraph(plain_paragraph("Second".to_owned())),
            ]
        );
    }

    #[test]
    fn reads_and_renders_basic_table() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/><w:shd w:fill="336699"/></w:tcPr><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:rPr><w:b/><w:sz w:val="16"/><w:color w:val="FFFFFF"/></w:rPr><w:t>Header</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Value</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800"/></w:sectPr></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(document.page_size, crate::PageSize::LETTER);
        assert_eq!(document.margins.left, 90.0);
        let DocxBlock::Table(table) = &document.blocks[0] else {
            panic!("expected table block");
        };
        assert_eq!(table.column_widths, vec![100.0, 150.0]);
        assert_eq!(table.alignment, super::TextAlignment::Left);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[0].cells[0].runs[0].font_size, 8.0);
        assert!(table.rows[0].cells[0].runs[0].bold);
        assert_eq!(table.rows[0].cells[0].runs[0].text, "Header");
        assert!(table.rows[0].cells[0].fill.is_some());

        let pdf = convert_docx_bytes(&input, &ConversionOptions::default()).unwrap();
        assert_eq!(
            pdf.windows(b"/Type /Page /Parent".len())
                .filter(|chunk| *chunk == b"/Type /Page /Parent")
                .count(),
            1
        );
    }

    #[test]
    fn reads_table_spans_merges_and_declared_height() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblPr><w:jc w:val="center"/></w:tblPr><w:tblGrid><w:gridCol w:w="500"/><w:gridCol w:w="1000"/><w:gridCol w:w="1200"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="240"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="500" w:type="dxa"/><w:vMerge w:val="restart"/><w:vAlign w:val="center"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();
        let DocxBlock::Table(table) = &document.blocks[0] else {
            panic!("expected table block");
        };

        assert_eq!(table.alignment, super::TextAlignment::Center);
        assert_eq!(table.rows[0].height, Some(12.0));
        assert_eq!(table.rows[0].cells[1].grid_span, 2);
        assert_eq!(
            table.rows[0].cells[0].vertical_merge,
            super::VerticalMerge::Restart
        );
        assert_eq!(
            table.rows[1].cells[0].vertical_merge,
            super::VerticalMerge::Continue
        );
        assert_eq!(
            table.rows[0].cells[0].vertical_alignment,
            super::VerticalAlignment::Center
        );
    }

    #[test]
    fn reads_table_font_highlight_margins_and_borders() {
        let input = create_docx(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblPr><w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcBorders><w:top w:val="dotDash" w:sz="4"/><w:right w:val="nil"/></w:tcBorders></w:tcPr><w:p><w:pPr><w:spacing w:before="180" w:after="120" w:line="276"/></w:pPr><w:r><w:t>□</w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:eastAsia="宋体"/><w:highlight w:val="yellow"/><w:sz w:val="18"/></w:rPr><w:t>高亮</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        );

        let document = read_docx_document(&input).unwrap();
        let DocxBlock::Table(table) = &document.blocks[0] else {
            panic!("expected table block");
        };
        let cell = &table.rows[0].cells[0];

        assert_eq!(table.cell_margin_left, 0.0);
        assert_eq!(table.cell_margin_right, 0.0);
        assert_eq!(cell.spacing_before, 9.0);
        assert_eq!(cell.spacing_after, 6.0);
        assert_eq!(cell.line_spacing, 1.15);
        assert_eq!(cell.runs.len(), 2);
        assert_eq!(cell.runs[0].font_name, None);
        assert_eq!(cell.runs[1].font_name.as_deref(), Some("宋体"));
        assert_eq!(cell.runs[1].font_size, 9.0);
        assert_eq!(
            cell.runs[1].highlight,
            Some(crate::PdfColor::new(1.0, 1.0, 0.0))
        );
        assert_eq!(
            cell.borders.top.map(|border| border.pattern),
            Some(super::BorderPattern::DotDash)
        );
        assert_eq!(cell.borders.right, None);
    }

    #[test]
    fn reads_word_2007_compatibility_mode() {
        let input = create_docx_with_settings(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Legacy</w:t></w:r></w:p></w:body></w:document>"#,
            r#"<w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="12"/></w:compat></w:settings>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(document.compatibility_mode, 12);
    }

    #[test]
    fn reads_and_renders_inline_png() {
        let input = create_docx_with_image(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/><a:graphic><a:graphicData><a:blip r:embed="rId1"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:body></w:document>"#,
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#,
        );

        let document = read_docx_document(&input).unwrap();

        let DocxBlock::Image(image) = &document.blocks[0] else {
            panic!("expected image block");
        };
        assert_eq!((image.width, image.height), (72.0, 36.0));
        let DocxImageData::Rgba {
            data,
            width,
            height,
        } = &image.data
        else {
            panic!("expected RGBA image");
        };
        assert_eq!((*width, *height, data.len()), (2, 1, 8));

        let pdf = convert_docx_bytes(&input, &ConversionOptions::default()).unwrap();
        assert!(pdf
            .windows(b"/Subtype /Image".len())
            .any(|chunk| chunk == b"/Subtype /Image"));
    }

    #[test]
    fn anchors_vml_textbox_image_once() {
        let input = create_docx_with_image(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Title</w:t></w:r><w:r><w:pict><v:shape style="position:absolute;margin-left:393.95pt;margin-top:-1.9pt;width:116.95pt;height:52.7pt"><v:textbox><w:txbxContent><w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/><a:graphic><a:graphicData><a:blip r:embed="rId1"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p></w:body></w:document>"#,
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#,
        );

        let document = read_docx_document(&input).unwrap();

        assert_eq!(document.blocks.len(), 1);
        let DocxBlock::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph block");
        };
        assert_eq!(paragraph.runs.len(), 1);
        assert_eq!(paragraph.runs[0].text, "Title");
        assert_eq!(paragraph.floating_images.len(), 1);
        assert_eq!(paragraph.floating_images[0].offset_x, 393.95);
        assert_eq!(paragraph.floating_images[0].offset_y, -1.9);
        assert_eq!(paragraph.floating_images[0].line_top_leading, 2.0);
    }
}
