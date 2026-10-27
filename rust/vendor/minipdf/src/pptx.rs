use std::collections::HashMap;
use std::io::{Cursor, Read};

use roxmltree::{Document, Node};
use zip::ZipArchive;

use crate::pdf::styled_text_width_with_font;
use crate::pdf::PdfPathCommand;
use crate::{ConversionOptions, MiniPdfError, PdfColor, PdfDocument, Result};

const EMUS_PER_POINT: f32 = 12_700.0;
const DEFAULT_SLIDE_WIDTH_EMU: i64 = 9_144_000;
const DEFAULT_SLIDE_HEIGHT_EMU: i64 = 6_858_000;
const OFFICE_RELATIONSHIPS_NS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

#[derive(Debug)]
struct PptxDocument {
    width: f32,
    height: f32,
    slides: Vec<PptxSlide>,
}

#[derive(Debug)]
struct PptxSlide {
    width: f32,
    height: f32,
    background: Option<PdfColor>,
    elements: Vec<PptxElement>,
}

#[derive(Debug)]
enum PptxElement {
    Shape(PptxShape),
    Picture(PptxPicture),
    Line(PptxLine),
}

#[derive(Debug)]
struct PptxShape {
    shape_type: String,
    bounds: PptxRect,
    fill: Option<PdfColor>,
    outline: Option<PptxOutline>,
    paragraphs: Vec<PptxParagraph>,
    text_body: PptxTextBody,
    /// 모양을 직접 적은 점 목록 (슬라이드 좌표, y 아래쪽이 큼).
    ///
    /// 차트의 부채꼴처럼 기본 도형 이름으로는 못 적는 모양에 쓴다.
    polygon: Option<Vec<(f32, f32)>>,
}

#[derive(Debug)]
struct PptxPicture {
    bounds: PptxRect,
    data: PptxImageData,
}

#[derive(Debug)]
enum PptxImageData {
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
    Svg {
        data: Vec<u8>,
        crop: PptxCrop,
    },
}

#[derive(Debug, Clone, Copy, Default)]
struct PptxCrop {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[derive(Debug)]
struct PptxLine {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    outline: PptxOutline,
    /// 중간에 꺾이는 점들. 비어 있으면 곧은 선이다.
    ///
    /// 도식의 연결선은 대개 ㄱ자로 꺾인다(bentConnector2·3·4). 꺾임을 버리고
    /// 상자 모서리에서 모서리로 곧게 그으면 대각선이 되어, 상자 사이를
    /// 가로지르는 엉뚱한 그림이 된다.
    bends: Vec<(f32, f32)>,
    /// 끝에 붙는 화살촉. (시작, 끝) 순서다.
    heads: (Option<PptxArrowHead>, Option<PptxArrowHead>),
}

/// 선 끝의 화살촉.
#[derive(Debug, Clone, Copy)]
struct PptxArrowHead {
    /// 선 굵기의 몇 배로 그릴지. w·len 속성에서 온다.
    length: f32,
    width: f32,
    /// 삼각형을 속까지 채울지. open 은 선 두 줄로만 그린다.
    filled: bool,
}

#[derive(Debug, Clone)]
struct PptxOutline {
    color: PdfColor,
    width: f32,
    dash_pattern: Vec<f32>,
}

#[derive(Debug)]
struct PptxParagraph {
    runs: Vec<PptxRun>,
    alignment: TextAlignment,
    margin_left: f32,
    indent: f32,
    space_before: f32,
    line_spacing: f32,
}

#[derive(Debug, Clone)]
struct PptxRun {
    text: String,
    font_size: f32,
    color: PdfColor,
    bold: bool,
    italic: bool,
    underline: bool,
    font_name: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum TextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy)]
enum VerticalAnchor {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy)]
struct PptxTextBody {
    left_inset: f32,
    top_inset: f32,
    right_inset: f32,
    bottom_inset: f32,
    anchor: VerticalAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlaceholderKey {
    placeholder_type: String,
    index: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct PlaceholderStyle {
    bounds: Option<PptxRect>,
    shape_type: Option<String>,
    fill: Option<PdfColor>,
    outline: Option<PptxOutline>,
    text_body: PptxTextBody,
    paragraphs: HashMap<usize, ParagraphDefaults>,
}

#[derive(Debug, Clone, Default)]
struct ParagraphDefaults {
    alignment: Option<TextAlignment>,
    margin_left: Option<f32>,
    indent: Option<f32>,
    space_before: Option<f32>,
    line_spacing: Option<f32>,
    bullet: Option<bool>,
    run: RunDefaults,
}

#[derive(Debug, Clone, Default)]
struct RunDefaults {
    font_size: Option<f32>,
    color: Option<PdfColor>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    latin_font: Option<String>,
    east_asian_font: Option<String>,
    complex_font: Option<String>,
}

impl Default for PptxTextBody {
    fn default() -> Self {
        Self {
            left_inset: 0.0,
            top_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
            anchor: VerticalAnchor::Top,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PptxRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Copy)]
struct CoordinateMap {
    target_x: f64,
    target_y: f64,
    target_width: f64,
    target_height: f64,
    source_x: f64,
    source_y: f64,
    source_width: f64,
    source_height: f64,
}

impl CoordinateMap {
    fn root(width: i64, height: i64) -> Self {
        Self {
            target_x: 0.0,
            target_y: 0.0,
            target_width: width as f64,
            target_height: height as f64,
            source_x: 0.0,
            source_y: 0.0,
            source_width: width as f64,
            source_height: height as f64,
        }
    }

    fn map_x(self, value: i64) -> f64 {
        self.target_x
            + (value as f64 - self.source_x) * self.target_width / self.source_width.max(1.0)
    }

    fn map_y(self, value: i64) -> f64 {
        self.target_y
            + (value as f64 - self.source_y) * self.target_height / self.source_height.max(1.0)
    }

    fn map_rect(self, x: i64, y: i64, width: i64, height: i64) -> PptxRect {
        let left = self.map_x(x);
        let top = self.map_y(y);
        let right = self.map_x(x + width);
        let bottom = self.map_y(y + height);
        PptxRect {
            x: (left / EMUS_PER_POINT as f64) as f32,
            y: (top / EMUS_PER_POINT as f64) as f32,
            width: ((right - left) / EMUS_PER_POINT as f64) as f32,
            height: ((bottom - top) / EMUS_PER_POINT as f64) as f32,
        }
    }
}

#[derive(Debug, Clone)]
struct Relationship {
    target: String,
    rel_type: String,
    external: bool,
    relationship_path: String,
}

pub(crate) fn convert_pptx_bytes(input: &[u8], options: &ConversionOptions) -> Result<Vec<u8>> {
    let presentation = read_pptx(input)?;

    // 무엇을 읽었는지 세어 둔다. 슬라이드가 비어 보일 때 읽기에서 빠진
    // 것인지 그리기에서 빠진 것인지 이 숫자로 갈린다.
    let mut shapes = 0usize;
    let mut pictures = 0usize;
    let mut lines = 0usize;
    let mut filled = 0usize;
    let mut with_text = 0usize;
    for slide in &presentation.slides {
        for element in &slide.elements {
            match element {
                PptxElement::Shape(shape) => {
                    shapes += 1;
                    if shape.fill.is_some() {
                        filled += 1;
                    }
                    if !shape.paragraphs.is_empty() {
                        with_text += 1;
                    }
                }
                PptxElement::Picture(_) => pictures += 1,
                PptxElement::Line(_) => lines += 1,
            }
        }
    }
    crate::stat(format!(
        "pptx 슬라이드 {}장, 도형 {shapes}개(칠한 것 {filled}, 글 있는 것 {with_text}), \
         그림 {pictures}개, 선 {lines}개{}",
        presentation.slides.len(),
        chart_summary()
    ));

    Ok(render_pptx(&presentation, options).to_bytes())
}

/// 차트를 몇 개 보고 몇 개를 그렸는지.
static CHARTS_DRAWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CHARTS_SKIPPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn chart_summary() -> String {
    use std::sync::atomic::Ordering;
    let drawn = CHARTS_DRAWN.swap(0, Ordering::Relaxed);
    let skipped = CHARTS_SKIPPED.swap(0, Ordering::Relaxed);
    if drawn == 0 && skipped == 0 {
        String::new()
    } else {
        format!(", 차트 {drawn}개 그림/{skipped}개 못 그림")
    }
}

fn read_pptx(input: &[u8]) -> Result<PptxDocument> {
    let mut archive = ZipArchive::new(Cursor::new(input))?;
    let presentation_xml =
        read_zip_bytes(&mut archive, "ppt/presentation.xml")?.ok_or_else(|| {
            MiniPdfError::InvalidInput("PPTX package is missing ppt/presentation.xml".to_owned())
        })?;
    let presentation_text = String::from_utf8(presentation_xml)
        .map_err(|_| MiniPdfError::InvalidInput("presentation.xml is not UTF-8".to_owned()))?;
    let presentation = Document::parse(&presentation_text)?;
    let relationships = read_relationships(&mut archive, "ppt/presentation.xml")?;
    let theme = read_theme_colors(&mut archive, &relationships)?;

    let slide_size = presentation
        .descendants()
        .find(|node| is_element(*node, "sldSz"));
    let width_emu = integer_attribute(slide_size, "cx").unwrap_or(DEFAULT_SLIDE_WIDTH_EMU);
    let height_emu = integer_attribute(slide_size, "cy").unwrap_or(DEFAULT_SLIDE_HEIGHT_EMU);
    let width = width_emu as f32 / EMUS_PER_POINT;
    let height = height_emu as f32 / EMUS_PER_POINT;

    let mut slide_paths = Vec::new();
    for slide_id in presentation
        .descendants()
        .filter(|node| is_element(*node, "sldId"))
    {
        let Some(id) = slide_id
            .attributes()
            .find(|attribute| {
                attribute.name() == "id" && attribute.namespace() == Some(OFFICE_RELATIONSHIPS_NS)
            })
            .map(|attribute| attribute.value())
        else {
            continue;
        };
        let Some(relationship) = relationships.get(id) else {
            continue;
        };
        slide_paths.push(resolve_part_target(
            "ppt/presentation.xml",
            &relationship.target,
        ));
    }
    if slide_paths.is_empty() {
        slide_paths = archive
            .file_names()
            .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .map(str::to_owned)
            .collect();
        slide_paths.sort_by_key(|path| natural_slide_number(path));
    }

    let mut slides = Vec::new();
    for slide_path in slide_paths {
        if let Some(slide) = read_slide(
            &mut archive,
            &slide_path,
            width,
            height,
            width_emu,
            height_emu,
            &theme,
        )? {
            slides.push(slide);
        }
    }
    if slides.is_empty() {
        let mut fallback_paths: Vec<_> = archive
            .file_names()
            .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .map(str::to_owned)
            .collect();
        fallback_paths.sort_by_key(|path| natural_slide_number(path));
        for slide_path in fallback_paths {
            if let Some(slide) = read_slide(
                &mut archive,
                &slide_path,
                width,
                height,
                width_emu,
                height_emu,
                &theme,
            )? {
                slides.push(slide);
            }
        }
    }

    Ok(PptxDocument {
        width,
        height,
        slides,
    })
}

#[allow(clippy::too_many_arguments)]
fn read_slide(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    slide_path: &str,
    width: f32,
    height: f32,
    width_emu: i64,
    height_emu: i64,
    theme: &HashMap<String, PdfColor>,
) -> Result<Option<PptxSlide>> {
    let Some(slide_bytes) = read_zip_bytes(archive, slide_path)? else {
        return Ok(None);
    };
    let slide_text = String::from_utf8(slide_bytes)
        .map_err(|_| MiniPdfError::InvalidInput(format!("{slide_path} is not UTF-8")))?;
    let slide = Document::parse(&slide_text)?;
    let relationships = read_relationships(archive, slide_path)?;
    let root_map = CoordinateMap::root(width_emu, height_emu);
    let mut elements = Vec::new();

    let layout = read_related_xml(archive, slide_path, &relationships, "/slideLayout")?;
    let master = if let Some((layout_path, _, layout_relationships)) = &layout {
        read_related_xml(archive, layout_path, layout_relationships, "/slideMaster")?
    } else {
        None
    };

    let master_document = master
        .as_ref()
        .map(|(_, xml, _)| Document::parse(xml))
        .transpose()?;
    let layout_document = layout
        .as_ref()
        .map(|(_, xml, _)| Document::parse(xml))
        .transpose()?;
    let slide_theme = apply_color_map(
        theme,
        &slide,
        layout_document.as_ref(),
        master_document.as_ref(),
    );
    let mut placeholder_defaults = HashMap::new();
    if let Some(document) = &master_document {
        add_placeholder_defaults(document, &slide_theme, root_map, &mut placeholder_defaults);
    }
    if let Some(document) = &layout_document {
        add_placeholder_defaults(document, &slide_theme, root_map, &mut placeholder_defaults);
    }

    if let (Some(master_document), Some((_, _, master_relationships))) = (&master_document, &master)
    {
        read_shape_tree(
            archive,
            master_document,
            master_relationships,
            &slide_theme,
            root_map,
            &mut elements,
            false,
            None,
        )?;
    }
    if let (Some(layout_document), Some((_, _, layout_relationships))) = (&layout_document, &layout)
    {
        read_shape_tree(
            archive,
            layout_document,
            layout_relationships,
            &slide_theme,
            root_map,
            &mut elements,
            false,
            None,
        )?;
    }
    read_shape_tree(
        archive,
        &slide,
        &relationships,
        &slide_theme,
        root_map,
        &mut elements,
        true,
        Some(&placeholder_defaults),
    )?;

    let background = read_background(&slide, &slide_theme)
        .or_else(|| {
            layout_document
                .as_ref()
                .and_then(|doc| read_background(doc, &slide_theme))
        })
        .or_else(|| {
            master_document
                .as_ref()
                .and_then(|doc| read_background(doc, &slide_theme))
        });

    Ok(Some(PptxSlide {
        width,
        height,
        background,
        elements,
    }))
}

#[allow(clippy::too_many_arguments)]
fn read_shape_tree(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    document: &Document<'_>,
    relationships: &HashMap<String, Relationship>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
    include_placeholders: bool,
    placeholder_defaults: Option<&HashMap<PlaceholderKey, PlaceholderStyle>>,
) -> Result<()> {
    let Some(shape_tree) = document
        .descendants()
        .find(|node| is_element(*node, "spTree"))
    else {
        return Ok(());
    };
    read_shape_children(
        archive,
        shape_tree,
        relationships,
        theme,
        coordinate_map,
        elements,
        include_placeholders,
        placeholder_defaults,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn read_shape_children(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    container: Node<'_, '_>,
    relationships: &HashMap<String, Relationship>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
    include_placeholders: bool,
    placeholder_defaults: Option<&HashMap<PlaceholderKey, PlaceholderStyle>>,
    group_fill: Option<PdfColor>,
) -> Result<()> {
    for child_node in container.children().filter(Node::is_element) {
        let placeholder = child_node.descendants().any(|node| is_element(node, "ph"));
        if placeholder && !include_placeholders {
            continue;
        }
        match child_node.tag_name().name() {
            "sp" => read_shape(
                child_node,
                theme,
                coordinate_map,
                elements,
                placeholder_defaults,
                group_fill,
            ),
            "cxnSp" => read_connector(child_node, theme, coordinate_map, elements),
            "pic" => read_picture(archive, child_node, relationships, coordinate_map, elements)?,
            "graphicFrame" => read_graphic_frame(
                archive,
                child_node,
                relationships,
                theme,
                coordinate_map,
                elements,
            )?,
            "grpSp" => {
                let properties = child(child_node, "grpSpPr");
                let group_map = properties
                    .and_then(|node| child(node, "xfrm"))
                    .map(|transform| create_group_map(transform, coordinate_map))
                    .unwrap_or(coordinate_map);
                // grpFill 은 "그룹에서 물려받는다"는 뜻이다. 그 그룹의 색을
                // 들고 내려가야 물려받을 것이 있다 — 이 덱만 해도 147개
                // 도형이 grpFill 이고, 그 중 다수의 그룹이 실제 색을 갖는다.
                let inherited_fill = properties
                    .filter(|node| has_any_fill(*node))
                    .and_then(|node| {
                        read_fill(node, theme).or_else(|| read_complex_fill(node, theme))
                    })
                    .or(group_fill);
                read_shape_children(
                    archive,
                    child_node,
                    relationships,
                    theme,
                    group_map,
                    elements,
                    include_placeholders,
                    placeholder_defaults,
                    inherited_fill,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_shape(
    shape: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
    placeholder_defaults: Option<&HashMap<PlaceholderKey, PlaceholderStyle>>,
    group_fill: Option<PdfColor>,
) {
    let inherited = read_placeholder_key(shape).and_then(|key| {
        placeholder_defaults.and_then(|defaults| {
            defaults.get(&key).or_else(|| {
                defaults.get(&PlaceholderKey {
                    placeholder_type: key.placeholder_type,
                    index: None,
                })
            })
        })
    });
    let shape_properties = child(shape, "spPr");
    let Some(bounds) = shape_properties
        .and_then(|node| child(node, "xfrm"))
        .map(|transform| read_bounds(transform, coordinate_map))
        .or_else(|| inherited.and_then(|style| style.bounds))
    else {
        return;
    };
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let shape_type = shape_properties
        .and_then(|node| child(node, "prstGeom"))
        .and_then(|node| node.attribute("prst"))
        .map(str::to_owned)
        .or_else(|| inherited.and_then(|style| style.shape_type.clone()))
        .unwrap_or_else(|| "rect".to_owned());
    // noFill 은 "안 칠한다"는 말이지 "안 적었다"는 말이 아니다.
    //
    // 둘을 같게 보면 도형 모양표(style/fillRef)의 색으로 되돌아가 버린다.
    // 실제 덱에서 바로 이 일이 났다 — 테두리 없는 투명 바탕에 주황 글씨로
    // 쓴 제목이, 주황으로 꽉 칠한 막대가 되어 글자가 그 안에 묻혔다.
    let fill_stated = shape_properties.is_some_and(has_any_fill);
    let outline_stated = shape_properties
        .and_then(|node| child(node, "ln"))
        .is_some_and(has_any_fill);

    let outline = if outline_stated {
        shape_properties
            .and_then(|node| child(node, "ln"))
            .and_then(|node| read_outline(node, theme))
    } else {
        shape_properties
            .and_then(|node| child(node, "ln"))
            .and_then(|node| read_outline(node, theme))
            .or_else(|| read_style_outline(shape, theme))
            .or_else(|| inherited.and_then(|style| style.outline.clone()))
    };
    if shape_type.eq_ignore_ascii_case("line") {
        // 도형으로 그린 직선도 끝에 화살촉이 붙는다. 연결선만 보면
        // '선 도형' 으로 그린 화살표가 그대로 맨 선으로 남는다.
        let line_node = shape_properties.and_then(|node| child(node, "ln"));
        add_line_from_bounds(
            shape_properties.and_then(|node| child(node, "xfrm")),
            bounds,
            outline.unwrap_or_else(default_outline),
            Vec::new(),
            (
                line_node.and_then(|node| read_arrow_head(node, "headEnd")),
                line_node.and_then(|node| read_arrow_head(node, "tailEnd")),
            ),
            elements,
        );
        return;
    }
    let fill = if fill_stated {
        // 적혀 있으니 그대로 따른다. 단색이면 그 색, 그라데이션·무늬면
        // 대표색, 그룹상속이면 그 그룹의 색, noFill·그림이면 비운다.
        let inherits_group = shape_properties
            .is_some_and(|node| child(node, "grpFill").is_some());
        shape_properties
            .and_then(|node| read_fill(node, theme).or_else(|| read_complex_fill(node, theme)))
            .or(if inherits_group { group_fill } else { None })
    } else {
        read_style_fill(shape, theme).or_else(|| inherited.and_then(|style| style.fill))
    };
    // 글상자도 자리표시자도 아닌 도형은 파워포인트가 글을 한가운데에
    // 놓는다. 이걸 빼면 네모 왼쪽 위에 글이 붙어 도형 밖으로 삐져나온
    // 것처럼 보인다.
    let is_text_box = child(shape, "nvSpPr")
        .and_then(|node| child(node, "cNvSpPr"))
        .and_then(|node| node.attribute("txBox"))
        .is_some_and(read_bool);
    let autoshape = inherited.is_none() && !is_text_box;
    let default_alignment = if autoshape {
        TextAlignment::Center
    } else {
        TextAlignment::Left
    };

    let paragraphs = child(shape, "txBody")
        .map(|body| {
            read_paragraphs(
                body,
                theme,
                inherited,
                read_style_font_color(shape, theme),
                default_alignment,
            )
        })
        .unwrap_or_default();
    let mut text_body = child(shape, "txBody")
        .and_then(|body| child(body, "bodyPr"))
        .map(|body| read_text_body(body, inherited.map(|style| style.text_body)))
        .or_else(|| inherited.map(|style| style.text_body))
        .unwrap_or_default();
    let anchor_given = child(shape, "txBody")
        .and_then(|body| child(body, "bodyPr"))
        .is_some_and(|node| node.attribute("anchor").is_some());
    if autoshape && !anchor_given {
        text_body.anchor = VerticalAnchor::Middle;
    }
    // 그림으로 채운 도형은 색 하나로 줄일 수 없다. 그렇다고 아무것도 안
    // 그리면 자리까지 사라지므로, 테두리만 옅게 그어 거기 무엇이 있었다고
    // 알린다. 이 덱에서는 두 개뿐이지만 사진 한 장이 슬라이드의 전부인
    // 자료도 있다.
    let picture_fill = shape_properties
        .is_some_and(|node| child(node, "blipFill").is_some())
        && fill.is_none();
    if picture_fill && paragraphs.is_empty() {
        let frame = PptxOutline {
            color: PdfColor {
                r: 0.72,
                g: 0.72,
                b: 0.72,
            },
            width: 1.0,
            dash_pattern: vec![3.0, 3.0],
        };
        elements.push(PptxElement::Shape(PptxShape {
            shape_type: shape_type.clone(),
            bounds,
            fill: None,
            outline: Some(frame),
            paragraphs: vec![PptxParagraph {
                runs: vec![PptxRun {
                    text: "그림".to_owned(),
                    font_size: 10.0,
                    color: PdfColor {
                        r: 0.55,
                        g: 0.55,
                        b: 0.55,
                    },
                    bold: false,
                    italic: false,
                    underline: false,
                    font_name: None,
                }],
                alignment: TextAlignment::Center,
                margin_left: 0.0,
                indent: 0.0,
                space_before: 0.0,
                line_spacing: 1.0,
            }],
            text_body: PptxTextBody {
                left_inset: 0.0,
                top_inset: 0.0,
                right_inset: 0.0,
                bottom_inset: 0.0,
                anchor: VerticalAnchor::Middle,
            },
            polygon: None,
        }));
        return;
    }

    if fill.is_some() || outline.is_some() || !paragraphs.is_empty() {
        elements.push(PptxElement::Shape(PptxShape {
            shape_type,
            bounds,
            fill,
            outline,
            paragraphs,
            text_body,
            polygon: None,
        }));
    }
}

fn read_connector(
    connector: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
) {
    let Some(properties) = child(connector, "spPr") else {
        return;
    };
    let Some(transform) = child(properties, "xfrm") else {
        return;
    };
    let bounds = read_bounds(transform, coordinate_map);
    let line_node = child(properties, "ln");
    let outline = line_node
        .and_then(|node| read_outline(node, theme))
        .unwrap_or_else(default_outline);

    // 어떤 모양의 연결선인지. 이것을 안 보면 ㄱ자가 대각선이 된다.
    let preset = child(properties, "prstGeom")
        .and_then(|node| node.attribute("prst"))
        .unwrap_or("line");

    let flip_h = transform.attribute("flipH").is_some_and(read_bool);
    let flip_v = transform.attribute("flipV").is_some_and(read_bool);
    let bends = connector_bends(preset, bounds, flip_h, flip_v);

    let heads = (
        line_node.and_then(|node| read_arrow_head(node, "headEnd")),
        line_node.and_then(|node| read_arrow_head(node, "tailEnd")),
    );

    add_line_from_bounds(Some(transform), bounds, outline, bends, heads, elements);
}

/// 꺾인 연결선의 중간 점들. 곧은 연결선이면 빈 배열.
///
/// bentConnector3 는 가운데에서 한 번 꺾여 두 번 도는 ㄱ자 두 개짜리
/// 모양이고, 2 는 한 번, 4·5 는 더 여러 번 꺾인다. 꺾이는 자리는 원래
/// adj 값으로 조절되지만, 조절값이 없으면 PowerPoint 도 가운데에서 꺾는다 —
/// 가운데로 두는 것만으로 대각선보다 훨씬 원본에 가깝다.
fn connector_bends(
    preset: &str,
    bounds: PptxRect,
    flip_h: bool,
    flip_v: bool,
) -> Vec<(f32, f32)> {
    let left = bounds.x;
    let right = bounds.x + bounds.width;
    let top = bounds.y;
    let bottom = bounds.y + bounds.height;
    let (sx, ex) = if flip_h { (right, left) } else { (left, right) };
    let (sy, ey) = if flip_v { (bottom, top) } else { (top, bottom) };
    let mid_x = (sx + ex) / 2.0;
    let mid_y = (sy + ey) / 2.0;

    match preset {
        // 한 번 꺾는다. 가로로 나갔다가 세로로 붙는다.
        "bentConnector2" => vec![(ex, sy)],
        // 두 번 꺾는다. 가장 흔한 모양이다.
        "bentConnector3" => vec![(mid_x, sy), (mid_x, ey)],
        // 세 번. 가로 → 세로 → 가로.
        "bentConnector4" => vec![(mid_x, sy), (mid_x, mid_y), (ex, mid_y)],
        "bentConnector5" => vec![
            (mid_x, sy),
            (mid_x, mid_y),
            (ex, mid_y),
        ],
        // 굽은 연결선은 꺾은선으로 근사한다. 곧은 대각선보다 가깝다.
        "curvedConnector2" => vec![(ex, sy)],
        "curvedConnector3" | "curvedConnector4" | "curvedConnector5" => {
            vec![(mid_x, sy), (mid_x, ey)]
        }
        _ => Vec::new(),
    }
}

/// 선 끝의 화살촉을 읽는다. 없으면 None.
fn read_arrow_head(line: Node<'_, '_>, name: &str) -> Option<PptxArrowHead> {
    let node = child(line, name)?;
    let kind = node.attribute("type").unwrap_or("none");
    if kind == "none" {
        return None;
    }
    // sm·med·lg 세 단계다. 없으면 med.
    fn step(value: Option<&str>) -> f32 {
        match value.unwrap_or("med") {
            "sm" => 2.0,
            "lg" => 5.0,
            _ => 3.5,
        }
    }
    Some(PptxArrowHead {
        length: step(node.attribute("len")),
        width: step(node.attribute("w")),
        // stealth·triangle·diamond·oval 은 채우고, open 만 비운다.
        filled: kind != "open",
    })
}

fn read_picture(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    picture: Node<'_, '_>,
    relationships: &HashMap<String, Relationship>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
) -> Result<()> {
    if picture
        .descendants()
        .find(|node| is_element(*node, "cNvPr"))
        .and_then(|node| node.attribute("hidden"))
        .is_some_and(read_bool)
    {
        return Ok(());
    }
    let Some(transform) = picture
        .children()
        .find(|node| is_element(*node, "spPr"))
        .and_then(|node| child(node, "xfrm"))
    else {
        return Ok(());
    };
    let bounds = read_bounds(transform, coordinate_map);
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return Ok(());
    }
    let Some(embed_id) = picture
        .descendants()
        .filter(|node| is_element(*node, "blip") || is_element(*node, "svgBlip"))
        .find_map(|node| attribute_by_local_name(node, "embed"))
    else {
        return Ok(());
    };
    let Some(relationship) = relationships.get(embed_id) else {
        return Ok(());
    };
    if relationship.external {
        return Ok(());
    }
    let source_path = relationship_source_path(relationship);
    let media_path = resolve_part_target(&source_path, &relationship.target);
    let Some(bytes) = read_zip_bytes(archive, &media_path)? else {
        return Ok(());
    };
    let crop = picture
        .descendants()
        .find(|node| is_element(*node, "srcRect"))
        .map(|node| PptxCrop {
            left: crop_attribute(node, "l"),
            top: crop_attribute(node, "t"),
            right: crop_attribute(node, "r"),
            bottom: crop_attribute(node, "b"),
        })
        .unwrap_or_default();
    let data = if media_path.to_ascii_lowercase().ends_with(".svg") || bytes.starts_with(b"<svg") {
        PptxImageData::Svg { data: bytes, crop }
    } else if let Some((width, height)) = jpeg_dimensions(&bytes) {
        PptxImageData::Jpeg {
            data: bytes,
            width,
            height,
        }
    } else if let Ok(image) = image::load_from_memory(&bytes) {
        let width = u16::try_from(image.width()).unwrap_or(u16::MAX);
        let height = u16::try_from(image.height()).unwrap_or(u16::MAX);
        PptxImageData::Rgba {
            data: image.into_rgba8().into_raw(),
            width,
            height,
        }
    } else {
        return Ok(());
    };
    elements.push(PptxElement::Picture(PptxPicture { bounds, data }));
    Ok(())
}

fn read_graphic_frame(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    frame: Node<'_, '_>,
    relationships: &HashMap<String, Relationship>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    elements: &mut Vec<PptxElement>,
) -> Result<()> {
    let Some(transform) = child(frame, "xfrm") else {
        return Ok(());
    };
    let bounds = read_bounds(transform, coordinate_map);
    let graphic_data = frame
        .descendants()
        .find(|node| is_element(*node, "graphicData"));
    if graphic_data
        .and_then(|node| node.attribute("uri"))
        .is_some_and(|uri| uri.ends_with("/diagram"))
    {
        read_diagram(archive, relationships, theme, transform, bounds, elements)?;
        return Ok(());
    }
    if let Some(graphic_data) = graphic_data {
        if read_chart(archive, graphic_data, relationships, theme, bounds, elements)? {
            return Ok(());
        }
    }
    let Some(table) = frame.descendants().find(|node| is_element(*node, "tbl")) else {
        return Ok(());
    };
    let widths: Vec<i64> = table
        .descendants()
        .filter(|node| is_element(*node, "gridCol"))
        .filter_map(|node| node.attribute("w").and_then(|value| value.parse().ok()))
        .collect();
    let rows: Vec<_> = table
        .children()
        .filter(|node| is_element(*node, "tr"))
        .collect();
    let total_width: i64 = widths.iter().sum();
    let total_height: i64 = rows
        .iter()
        .filter_map(|row| {
            row.attribute("h")
                .and_then(|value| value.parse::<i64>().ok())
        })
        .sum();
    if widths.is_empty() || total_width <= 0 || total_height <= 0 {
        return Ok(());
    }
    let mut top = bounds.y;
    for row in rows {
        let row_height = bounds.height
            * row
                .attribute("h")
                .and_then(|value| value.parse::<f32>().ok())
                .unwrap_or(0.0)
            / total_height as f32;
        let mut left = bounds.x;
        let mut column = 0;
        for cell_node in row.children().filter(|node| is_element(*node, "tc")) {
            if column >= widths.len() {
                break;
            }
            let span = cell_node
                .attribute("gridSpan")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .max(1);
            let spanned_width: i64 = widths.iter().skip(column).take(span).sum();
            let cell_width = bounds.width * spanned_width as f32 / total_width as f32;
            let cell_bounds = PptxRect {
                x: left,
                y: top,
                width: cell_width,
                height: row_height,
            };
            let properties = child(cell_node, "tcPr");
            let fill = properties.and_then(|node| read_fill(node, theme));
            let paragraphs = child(cell_node, "txBody")
                .map(|body| read_paragraphs(body, theme, None, None, TextAlignment::Left))
                .unwrap_or_default();
            elements.push(PptxElement::Shape(PptxShape {
                shape_type: "rect".to_owned(),
                bounds: cell_bounds,
                fill,
                outline: None,
                paragraphs,
                text_body: PptxTextBody::default(),
                polygon: None,
            }));
            add_table_borders(properties, cell_bounds, theme, elements);
            left += cell_width;
            column += span;
        }
        top += row_height;
    }
    Ok(())
}

fn add_table_borders(
    properties: Option<Node<'_, '_>>,
    bounds: PptxRect,
    theme: &HashMap<String, PdfColor>,
    elements: &mut Vec<PptxElement>,
) {
    let Some(properties) = properties else {
        return;
    };
    for (name, x1, y1, x2, y2) in [
        (
            "lnL",
            bounds.x,
            bounds.y,
            bounds.x,
            bounds.y + bounds.height,
        ),
        (
            "lnR",
            bounds.x + bounds.width,
            bounds.y,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
        ),
        ("lnT", bounds.x, bounds.y, bounds.x + bounds.width, bounds.y),
        (
            "lnB",
            bounds.x,
            bounds.y + bounds.height,
            bounds.x + bounds.width,
            bounds.y + bounds.height,
        ),
    ] {
        if let Some(outline) = child(properties, name).and_then(|node| read_outline(node, theme)) {
            elements.push(PptxElement::Line(PptxLine {
                x1,
                y1,
                x2,
                y2,
                outline,
                bends: Vec::new(),
                heads: (None, None),
            }));
        }
    }
}

fn read_diagram(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    relationships: &HashMap<String, Relationship>,
    theme: &HashMap<String, PdfColor>,
    frame_transform: Node<'_, '_>,
    bounds: PptxRect,
    elements: &mut Vec<PptxElement>,
) -> Result<()> {
    let Some(relationship) = relationships.values().find(|relationship| {
        relationship.rel_type.ends_with("/diagramDrawing") && !relationship.external
    }) else {
        return Ok(());
    };
    let drawing_path = resolve_part_target(
        &relationship_source_path(relationship),
        &relationship.target,
    );
    let Some(bytes) = read_zip_bytes(archive, &drawing_path)? else {
        return Ok(());
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| MiniPdfError::InvalidInput(format!("{drawing_path} is not UTF-8")))?;
    let drawing = Document::parse(&text)?;
    let extent = child(frame_transform, "ext");
    let frame_width = integer_attribute(extent, "cx").unwrap_or(0);
    let frame_height = integer_attribute(extent, "cy").unwrap_or(0);
    if frame_width <= 0 || frame_height <= 0 {
        return Ok(());
    }
    let map = CoordinateMap {
        target_x: bounds.x as f64 * EMUS_PER_POINT as f64,
        target_y: bounds.y as f64 * EMUS_PER_POINT as f64,
        target_width: bounds.width as f64 * EMUS_PER_POINT as f64,
        target_height: bounds.height as f64 * EMUS_PER_POINT as f64,
        source_x: 0.0,
        source_y: 0.0,
        source_width: frame_width as f64,
        source_height: frame_height as f64,
    };
    let Some(shape_tree) = drawing
        .descendants()
        .find(|node| is_element(*node, "spTree"))
    else {
        return Ok(());
    };
    for shape in shape_tree.children().filter(|node| is_element(*node, "sp")) {
        read_shape(shape, theme, map, elements, None, None);
    }
    Ok(())
}

fn add_placeholder_defaults(
    document: &Document<'_>,
    theme: &HashMap<String, PdfColor>,
    coordinate_map: CoordinateMap,
    defaults: &mut HashMap<PlaceholderKey, PlaceholderStyle>,
) {
    let Some(shape_tree) = document
        .descendants()
        .find(|node| is_element(*node, "spTree"))
    else {
        return;
    };
    for shape in shape_tree
        .descendants()
        .filter(|node| is_element(*node, "sp"))
    {
        let Some(key) = read_placeholder_key(shape) else {
            continue;
        };
        let properties = child(shape, "spPr");
        let text_body = child(shape, "txBody");
        let body_properties = text_body.and_then(|node| child(node, "bodyPr"));
        let style = PlaceholderStyle {
            bounds: properties
                .and_then(|node| child(node, "xfrm"))
                .map(|node| read_bounds(node, coordinate_map)),
            shape_type: properties
                .and_then(|node| child(node, "prstGeom"))
                .and_then(|node| node.attribute("prst"))
                .map(str::to_owned),
            fill: properties
                .and_then(|node| read_fill(node, theme))
                .or_else(|| read_style_fill(shape, theme)),
            outline: properties
                .and_then(|node| child(node, "ln"))
                .and_then(|node| read_outline(node, theme))
                .or_else(|| read_style_outline(shape, theme)),
            text_body: body_properties
                .map(|node| read_text_body(node, None))
                .unwrap_or_default(),
            paragraphs: text_body
                .map(|node| read_paragraph_defaults(node, theme))
                .unwrap_or_default(),
        };
        // 덮어쓰지 않고 겹친다.
        //
        // 마스터를 먼저 읽고 레이아웃을 나중에 읽는데, 레이아웃의 자리표시자는
        // 위치를 안 적는 경우가 많다 — 마스터 것을 쓰라는 뜻이다. 그대로
        // insert 하면 위치가 비어 있는 항목이 마스터의 위치를 지워 버리고,
        // 슬라이드의 제목이 그릴 자리를 잃어 통째로 사라진다.
        // type="ctrTitle" 만 멀쩡했던 것은 마스터에 그 항목이 없어서였다.
        merge_placeholder(defaults.entry(key.clone()).or_default(), style.clone());
        merge_placeholder(
            defaults
                .entry(PlaceholderKey {
                    placeholder_type: key.placeholder_type,
                    index: None,
                })
                .or_default(),
            style,
        );
    }
}

/// 레이아웃이 말한 것만 덮고, 말하지 않은 것은 마스터 것을 남긴다.
fn merge_placeholder(base: &mut PlaceholderStyle, over: PlaceholderStyle) {
    if over.bounds.is_some() {
        base.bounds = over.bounds;
    }
    if over.shape_type.is_some() {
        base.shape_type = over.shape_type;
    }
    if over.fill.is_some() {
        base.fill = over.fill;
    }
    if over.outline.is_some() {
        base.outline = over.outline;
    }
    base.text_body = over.text_body;
    base.paragraphs.extend(over.paragraphs);
}

fn read_placeholder_key(shape: Node<'_, '_>) -> Option<PlaceholderKey> {
    let placeholder = shape.descendants().find(|node| is_element(*node, "ph"))?;
    Some(PlaceholderKey {
        placeholder_type: placeholder.attribute("type").unwrap_or("body").to_owned(),
        index: placeholder.attribute("idx").map(str::to_owned),
    })
}

fn read_paragraph_defaults(
    text_body: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
) -> HashMap<usize, ParagraphDefaults> {
    let Some(list_style) = child(text_body, "lstStyle") else {
        return HashMap::new();
    };
    list_style
        .children()
        .filter(Node::is_element)
        .filter_map(|properties| {
            let name = properties.tag_name().name();
            let level = name
                .strip_prefix("lvl")?
                .strip_suffix("pPr")?
                .parse::<usize>()
                .ok()?
                .saturating_sub(1);
            Some((
                level,
                ParagraphDefaults {
                    alignment: read_alignment(Some(properties)),
                    margin_left: emu_attribute(Some(properties), "marL"),
                    indent: emu_attribute(Some(properties), "indent"),
                    space_before: read_space_before(Some(properties)),
                    line_spacing: read_line_spacing(Some(properties)),
                    bullet: read_bullet_setting(properties).or_else(|| (level > 0).then_some(true)),
                    run: child(properties, "defRPr")
                        .map(|node| read_run_defaults(node, theme))
                        .unwrap_or_default(),
                },
            ))
        })
        .collect()
}

fn read_paragraphs(
    text_body: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
    inherited_style: Option<&PlaceholderStyle>,
    style_color: Option<PdfColor>,
    default_alignment: TextAlignment,
) -> Vec<PptxParagraph> {
    // 자동 번호는 한 글상자 안에서 이어 센다. 단계마다 따로 세고, 윗단계가
    // 하나 올라가면 그 아래 단계는 다시 1 부터다.
    let mut counters = [0u32; 9];
    text_body
        .children()
        .filter(|node| is_element(*node, "p"))
        .map(|paragraph| {
            let properties = child(paragraph, "pPr");
            let level = properties
                .and_then(|node| node.attribute("lvl"))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0)
                .min(8);
            let inherited = inherited_style.and_then(|style| {
                style
                    .paragraphs
                    .get(&level)
                    .or_else(|| style.paragraphs.get(&0))
            });
            let default_run = properties.and_then(|node| child(node, "defRPr"));
            let is_bullet = properties
                .and_then(read_bullet_setting)
                .or_else(|| inherited.and_then(|style| style.bullet))
                .unwrap_or(false);

            // 무엇을 앞에 붙일지. 번호면 세어서 번호를, 글자를 적어 뒀으면
            // 그 글자를, 아무 말이 없으면 여태처럼 가운뎃점을 붙인다.
            //
            // 전에는 buAutoNum 도 가운뎃점으로 그렸다. 번호를 매겨 둔 목록이
            // 전부 같은 점으로 나와, 차례가 있는 글이 차례 없는 글이 됐다.
            let marker = if !is_bullet {
                String::new()
            } else {
                match properties.and_then(read_bullet_marker) {
                    Some(BulletMarker::Number { scheme, start }) => {
                        let slot = &mut counters[level];
                        *slot = if *slot == 0 { start } else { *slot + 1 };
                        let n = *slot;
                        for deeper in counters.iter_mut().skip(level + 1) {
                            *deeper = 0;
                        }
                        format!("{} ", format_auto_number(&scheme, n))
                    }
                    Some(BulletMarker::Char(ch)) => {
                        counters[level] = 0;
                        format!("{ch} ")
                    }
                    None => {
                        counters[level] = 0;
                        "\u{2022} ".to_owned()
                    }
                }
            };

            let mut runs = Vec::new();
            let mut has_text = false;
            for run_node in paragraph.children().filter(Node::is_element) {
                match run_node.tag_name().name() {
                    "r" | "fld" => {
                        let Some(text_node) =
                            run_node.descendants().find(|node| is_element(*node, "t"))
                        else {
                            continue;
                        };
                        let mut text = text_node.text().unwrap_or_default().to_owned();
                        if is_bullet && !has_text {
                            text.insert_str(0, &marker);
                        }
                        if !text.is_empty() {
                            runs.push(read_run(
                                text,
                                child(run_node, "rPr"),
                                default_run,
                                inherited.map(|style| &style.run),
                                theme,
                                style_color,
                            ));
                            has_text = true;
                        }
                    }
                    "br" => runs.push(read_run(
                        "\n".to_owned(),
                        child(run_node, "rPr"),
                        default_run,
                        inherited.map(|style| &style.run),
                        theme,
                        style_color,
                    )),
                    "tab" => runs.push(read_run(
                        "\t".to_owned(),
                        child(run_node, "rPr"),
                        default_run,
                        inherited.map(|style| &style.run),
                        theme,
                        style_color,
                    )),
                    _ => {}
                }
            }
            PptxParagraph {
                runs,
                alignment: read_alignment(properties)
                    .or_else(|| inherited.and_then(|style| style.alignment))
                    .unwrap_or(default_alignment),
                margin_left: emu_attribute(properties, "marL")
                    .or_else(|| inherited.and_then(|style| style.margin_left))
                    .unwrap_or(0.0),
                indent: emu_attribute(properties, "indent")
                    .or_else(|| inherited.and_then(|style| style.indent))
                    .unwrap_or(0.0),
                space_before: read_space_before(properties)
                    .or_else(|| inherited.and_then(|style| style.space_before))
                    .unwrap_or(0.0),
                line_spacing: read_line_spacing(properties)
                    .or_else(|| inherited.and_then(|style| style.line_spacing))
                    .unwrap_or(1.15),
            }
        })
        .collect()
}

fn read_run(
    text: String,
    properties: Option<Node<'_, '_>>,
    defaults: Option<Node<'_, '_>>,
    inherited: Option<&RunDefaults>,
    theme: &HashMap<String, PdfColor>,
    style_color: Option<PdfColor>,
) -> PptxRun {
    let font_size = properties
        .and_then(|node| node.attribute("sz"))
        .or_else(|| defaults.and_then(|node| node.attribute("sz")))
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| (value / 100.0).max(1.0))
        .or_else(|| inherited.and_then(|style| style.font_size))
        .unwrap_or(18.0);
    let color = properties
        .and_then(|node| read_fill(node, theme))
        .or_else(|| defaults.and_then(|node| read_fill(node, theme)))
        .or_else(|| inherited.and_then(|style| style.color))
        // 도형의 style/fontRef 가 정한 글자색. 파워포인트는 짙은 강조색
        // 도형 위에 흰 글씨를 얹는다. 이걸 빼면 남색 위에 검은 글씨가
        // 되어 아무것도 안 읽힌다.
        .or(style_color)
        .or_else(|| theme.get("tx1").copied())
        .unwrap_or(PdfColor::BLACK);
    let bold = optional_bool_attribute(properties, "b")
        .or_else(|| optional_bool_attribute(defaults, "b"))
        .or_else(|| inherited.and_then(|style| style.bold))
        .unwrap_or(false);
    let italic = optional_bool_attribute(properties, "i")
        .or_else(|| optional_bool_attribute(defaults, "i"))
        .or_else(|| inherited.and_then(|style| style.italic))
        .unwrap_or(false);
    let underline = properties
        .and_then(|node| node.attribute("u"))
        .or_else(|| defaults.and_then(|node| node.attribute("u")))
        .map(|value| !value.eq_ignore_ascii_case("none"))
        .or_else(|| inherited.and_then(|style| style.underline))
        .unwrap_or(false)
        || properties.is_some_and(|node| {
            node.children()
                .any(|child_node| is_element(child_node, "hlinkClick"))
        });
    let font_name = select_font_name(&text, properties)
        .or_else(|| select_font_name(&text, defaults))
        .or_else(|| inherited.and_then(|style| inherited_font_name(&text, style)));
    PptxRun {
        text,
        font_size,
        color,
        bold,
        italic,
        underline,
        font_name,
    }
}

fn read_text_body(properties: Node<'_, '_>, inherited: Option<PptxTextBody>) -> PptxTextBody {
    let fallback = inherited.unwrap_or_default();
    PptxTextBody {
        left_inset: emu_attribute(Some(properties), "lIns").unwrap_or(fallback.left_inset),
        top_inset: emu_attribute(Some(properties), "tIns").unwrap_or(fallback.top_inset),
        right_inset: emu_attribute(Some(properties), "rIns").unwrap_or(fallback.right_inset),
        bottom_inset: emu_attribute(Some(properties), "bIns").unwrap_or(fallback.bottom_inset),
        anchor: match properties.attribute("anchor") {
            Some("ctr") => VerticalAnchor::Middle,
            Some("b") => VerticalAnchor::Bottom,
            Some(_) => VerticalAnchor::Top,
            None => fallback.anchor,
        },
    }
}

fn read_alignment(properties: Option<Node<'_, '_>>) -> Option<TextAlignment> {
    match properties?.attribute("algn")? {
        "ctr" => Some(TextAlignment::Center),
        "r" => Some(TextAlignment::Right),
        _ => Some(TextAlignment::Left),
    }
}

fn read_space_before(properties: Option<Node<'_, '_>>) -> Option<f32> {
    child(properties?, "spcBef")
        .and_then(|node| child(node, "spcPts"))
        .and_then(|node| node.attribute("val"))
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| (value / 100.0).max(0.0))
}

fn read_line_spacing(properties: Option<Node<'_, '_>>) -> Option<f32> {
    child(properties?, "lnSpc")
        .and_then(|node| child(node, "spcPct"))
        .and_then(|node| node.attribute("val"))
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| (value / 100_000.0).max(0.1))
}

fn read_bullet_setting(properties: Node<'_, '_>) -> Option<bool> {
    if child(properties, "buNone").is_some() {
        Some(false)
    } else if child(properties, "buChar").is_some() || child(properties, "buAutoNum").is_some() {
        Some(true)
    } else {
        None
    }
}

/// 문단 앞에 붙는 표시.
enum BulletMarker {
    /// 적어 둔 글자. buChar 의 char 속성.
    Char(String),
    /// 번호. 몇 번부터 셀지와 어떤 꼴로 적을지.
    Number { scheme: String, start: u32 },
}

/// 이 문단이 제 몫으로 적어 둔 표시. 물려받는 것은 보지 않는다.
fn read_bullet_marker(properties: Node<'_, '_>) -> Option<BulletMarker> {
    if child(properties, "buNone").is_some() {
        return None;
    }
    if let Some(node) = child(properties, "buAutoNum") {
        return Some(BulletMarker::Number {
            scheme: node.attribute("type").unwrap_or("arabicPeriod").to_owned(),
            start: node
                .attribute("startAt")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1),
        });
    }
    if let Some(node) = child(properties, "buChar") {
        if let Some(ch) = node.attribute("char") {
            let font = child(properties, "buFont")
                .and_then(|n| n.attribute("typeface"))
                .unwrap_or_default();
            return Some(BulletMarker::Char(bullet_glyph(ch, font)));
        }
    }
    None
}

/// 글머리 기호로 적어 둔 글자를, 그 글자가 뜻하는 모양으로 바꾼다.
///
/// 적어 둔 글자를 그대로 내보내면 안 된다. 기호 글꼴을 쓰는 목록은 글자
/// 자체가 아니라 그 글꼴에서의 **그림**을 뜻한다 — Wingdings 의 '§' 는
/// 채운 네모이고, 그대로 내보내면 화면에 '§' 가 찍힌다.
///
/// 그 글꼴이 기기에 없으므로 뜻이 같은 유니코드 글자로 옮긴다. 모르는
/// 글자는 가운뎃점으로 돌아간다 — 틀린 기호보다 낫다.
fn bullet_glyph(ch: &str, font: &str) -> String {
    let symbol_font = {
        let lower = font.to_ascii_lowercase();
        lower.starts_with("wingdings")
            || lower.starts_with("webdings")
            || lower.starts_with("symbol")
            || lower.starts_with("monotype sorts")
    };
    if !symbol_font {
        return ch.to_owned();
    }
    match ch {
        "\u{a7}" => "\u{25aa}".to_owned(), // § → ▪
        "n" | "\u{6e}" => "\u{25a0}".to_owned(), // n → ■
        "l" | "\u{6c}" => "\u{25cf}".to_owned(), // l → ●
        "u" | "\u{75}" => "\u{25c6}".to_owned(), // u → ◆
        "p" => "\u{2751}".to_owned(),
        "\u{fc}" => "\u{2713}".to_owned(), // ü → ✓
        "\u{fe}" => "\u{2794}".to_owned(), // þ → ➔
        "\u{d8}" | "\u{e0}" => "\u{2192}".to_owned(),
        "\u{b7}" => "\u{2022}".to_owned(), // Symbol 의 · → •
        _ => "\u{2022}".to_owned(),
    }
}

/// 번호를 적는 꼴.
///
/// 파워포인트는 셈 방식과 뒤에 붙는 문장부호를 한 이름에 담는다 —
/// arabicPeriod 는 "1.", arabicParenR 은 "1)" 이다.
fn format_auto_number(scheme: &str, n: u32) -> String {
    let body = if scheme.starts_with("roman") {
        let upper = scheme.starts_with("romanUc");
        let r = roman_numeral(n);
        if upper { r } else { r.to_lowercase() }
    } else if scheme.starts_with("alpha") {
        let upper = scheme.starts_with("alphaUc");
        let letter = alpha_numeral(n);
        if upper { letter } else { letter.to_lowercase() }
    } else {
        n.to_string()
    };
    if scheme.ends_with("ParenBoth") {
        format!("({body})")
    } else if scheme.ends_with("ParenR") {
        format!("{body})")
    } else if scheme.ends_with("Period") {
        format!("{body}.")
    } else {
        body
    }
}

fn roman_numeral(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"),
        (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
    ];
    let mut out = String::new();
    for (value, sign) in TABLE {
        while n >= value {
            out.push_str(sign);
            n -= value;
        }
    }
    out
}

/// 1→A, 26→Z, 27→AA. 0 은 오지 않는다.
fn alpha_numeral(n: u32) -> String {
    let mut n = n.max(1);
    let mut out = Vec::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        out.push((b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

fn read_run_defaults(node: Node<'_, '_>, theme: &HashMap<String, PdfColor>) -> RunDefaults {
    RunDefaults {
        font_size: node
            .attribute("sz")
            .and_then(|value| value.parse::<f32>().ok())
            .map(|value| (value / 100.0).max(1.0)),
        color: read_fill(node, theme),
        bold: node.attribute("b").map(read_bool),
        italic: node.attribute("i").map(read_bool),
        underline: node
            .attribute("u")
            .map(|value| !value.eq_ignore_ascii_case("none")),
        latin_font: child(node, "latin")
            .and_then(|font| font.attribute("typeface"))
            .map(str::to_owned),
        east_asian_font: child(node, "ea")
            .and_then(|font| font.attribute("typeface"))
            .map(str::to_owned),
        complex_font: child(node, "cs")
            .and_then(|font| font.attribute("typeface"))
            .map(str::to_owned),
    }
}

fn inherited_font_name(text: &str, style: &RunDefaults) -> Option<String> {
    if contains_east_asian(text) {
        style.east_asian_font.clone()
    } else if contains_complex_script(text) {
        style.complex_font.clone()
    } else {
        style.latin_font.clone()
    }
    .or_else(|| style.latin_font.clone())
    .or_else(|| style.east_asian_font.clone())
    .or_else(|| style.complex_font.clone())
    .filter(|name| !name.starts_with('+'))
}

fn read_bounds(transform: Node<'_, '_>, coordinate_map: CoordinateMap) -> PptxRect {
    let offset = child(transform, "off");
    let extent = child(transform, "ext");
    coordinate_map.map_rect(
        integer_attribute(offset, "x").unwrap_or(0),
        integer_attribute(offset, "y").unwrap_or(0),
        integer_attribute(extent, "cx").unwrap_or(0),
        integer_attribute(extent, "cy").unwrap_or(0),
    )
}

fn create_group_map(transform: Node<'_, '_>, parent: CoordinateMap) -> CoordinateMap {
    let offset = child(transform, "off");
    let extent = child(transform, "ext");
    let child_offset = child(transform, "chOff");
    let child_extent = child(transform, "chExt");
    let group_x = integer_attribute(offset, "x").unwrap_or(0);
    let group_y = integer_attribute(offset, "y").unwrap_or(0);
    let group_width = integer_attribute(extent, "cx").unwrap_or(0);
    let group_height = integer_attribute(extent, "cy").unwrap_or(0);
    if group_width <= 0 || group_height <= 0 {
        return parent;
    }
    CoordinateMap {
        target_x: parent.map_x(group_x),
        target_y: parent.map_y(group_y),
        target_width: parent.map_x(group_x + group_width) - parent.map_x(group_x),
        target_height: parent.map_y(group_y + group_height) - parent.map_y(group_y),
        source_x: integer_attribute(child_offset, "x").unwrap_or(group_x) as f64,
        source_y: integer_attribute(child_offset, "y").unwrap_or(group_y) as f64,
        // chExt 가 0 인 그룹이 있다. 없는 것과 같이 다뤄 그룹 제 크기로
        // 물러선다.
        //
        // 전에는 0 이 그대로 들어가, 나누는 쪽의 `.max(1.0)` 이 0 으로
        // 나누기는 막았지만 대신 1 로 나눴다. 그러면 자식의 좌표가 그대로
        // 배율이 되어 도형이 슬라이드를 꿰뚫는 크기로 커진다 — 쪽 32 의
        // 주황 세로 막대 넷이 그것이었다. 자르지 않고 키워 놓으니 화살표를
        // 고친 뒤에는 그 막대에 촉까지 달렸다.
        source_width: integer_attribute(child_extent, "cx")
            .filter(|value| *value > 0)
            .unwrap_or(group_width) as f64,
        source_height: integer_attribute(child_extent, "cy")
            .filter(|value| *value > 0)
            .unwrap_or(group_height) as f64,
    }
}

fn add_line_from_bounds(
    transform: Option<Node<'_, '_>>,
    bounds: PptxRect,
    outline: PptxOutline,
    bends: Vec<(f32, f32)>,
    heads: (Option<PptxArrowHead>, Option<PptxArrowHead>),
    elements: &mut Vec<PptxElement>,
) {
    let flip_horizontal = transform
        .and_then(|node| node.attribute("flipH"))
        .is_some_and(read_bool);
    let flip_vertical = transform
        .and_then(|node| node.attribute("flipV"))
        .is_some_and(read_bool);
    elements.push(PptxElement::Line(PptxLine {
        x1: if flip_horizontal {
            bounds.x + bounds.width
        } else {
            bounds.x
        },
        y1: if flip_vertical {
            bounds.y + bounds.height
        } else {
            bounds.y
        },
        x2: if flip_horizontal {
            bounds.x
        } else {
            bounds.x + bounds.width
        },
        y2: if flip_vertical {
            bounds.y
        } else {
            bounds.y + bounds.height
        },
        outline,
        bends,
        heads,
    }));
}

fn read_outline(node: Node<'_, '_>, theme: &HashMap<String, PdfColor>) -> Option<PptxOutline> {
    if child(node, "noFill").is_some() {
        return None;
    }
    let width = node
        .attribute("w")
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| value / EMUS_PER_POINT)
        .filter(|value| *value > 0.0)
        .unwrap_or(1.0);
    let dash_pattern = child(node, "prstDash")
        .and_then(|dash| dash.attribute("val"))
        .map(|value| match value {
            "dot" | "sysDot" => vec![1.0, 2.0],
            "dash" | "sysDash" => vec![3.0, 3.0],
            "lgDash" => vec![6.0, 3.0],
            "dashDot" | "sysDashDot" => vec![3.0, 2.0, 1.0, 2.0],
            _ => Vec::new(),
        })
        .unwrap_or_default();
    Some(PptxOutline {
        color: read_fill(node, theme).unwrap_or(PdfColor::BLACK),
        width,
        dash_pattern,
    })
}

fn default_outline() -> PptxOutline {
    PptxOutline {
        color: PdfColor::BLACK,
        width: 1.0,
        dash_pattern: Vec::new(),
    }
}

/// 채우기가 적혀 있기는 한가 — 종류를 가리지 않는다.
///
/// 적혀 있으면 도형 모양표(style/fillRef)로 되돌아가면 안 된다. noFill 에서
/// 겪은 것과 같은 일이 gradFill·pattFill·blipFill·grpFill 에서도 그대로
/// 난다: 그라데이션 자리에 엉뚱한 테마 단색이 들어차 글자를 덮는다.
fn has_any_fill(node: Node<'_, '_>) -> bool {
    node.children().any(|child_node| {
        matches!(
            child_node.tag_name().name(),
            "noFill" | "solidFill" | "gradFill" | "pattFill" | "blipFill" | "grpFill"
        )
    })
}

/// 단색이 아닌 채우기를 색 하나로 줄인다.
///
/// 그라데이션은 첫 정지점, 무늬는 앞 색을 쓴다 — 실제보다 단조롭지만 자리와
/// 색감은 남는다. 그림 채우기와 그룹 상속은 색을 지어낼 수 없으므로 비운다.
/// 비우는 쪽이 글자를 덮는 것보다 낫다.
fn read_complex_fill(
    node: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
) -> Option<PdfColor> {
    if let Some(gradient) = child(node, "gradFill") {
        return child(gradient, "gsLst")
            .and_then(|list| list.children().find(|n| is_element(*n, "gs")))
            .and_then(|stop| stop.children().find(Node::is_element))
            .and_then(|color| read_color_node(color, theme));
    }
    if let Some(pattern) = child(node, "pattFill") {
        return child(pattern, "fgClr")
            .and_then(|node| node.children().find(Node::is_element))
            .and_then(|color| read_color_node(color, theme));
    }
    None
}

fn read_fill(node: Node<'_, '_>, theme: &HashMap<String, PdfColor>) -> Option<PdfColor> {
    if child(node, "noFill").is_some() {
        return None;
    }
    let fill = if is_element(node, "solidFill") {
        Some(node)
    } else {
        child(node, "solidFill")
    }?;
    let color_node = fill.children().find(Node::is_element)?;
    let color = match color_node.tag_name().name() {
        "srgbClr" => color_node.attribute("val").and_then(parse_hex_color),
        "schemeClr" => color_node
            .attribute("val")
            .and_then(|name| theme.get(name).copied()),
        "sysClr" => color_node
            .attribute("lastClr")
            .or_else(|| color_node.attribute("val"))
            .and_then(parse_hex_color),
        "prstClr" => color_node.attribute("val").and_then(preset_color),
        _ => None,
    }?;
    Some(apply_luminance(color, color_node))
}

fn read_style_fill(shape: Node<'_, '_>, theme: &HashMap<String, PdfColor>) -> Option<PdfColor> {
    child(shape, "style")
        .and_then(|style| child(style, "fillRef"))
        .and_then(|reference| reference.children().find(Node::is_element))
        .and_then(|color| read_color_node(color, theme))
}

/// 도형 모양표가 정한 글자색 (style/fontRef).
fn read_style_font_color(
    shape: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
) -> Option<PdfColor> {
    child(shape, "style")
        .and_then(|style| child(style, "fontRef"))
        .and_then(|reference| reference.children().find(Node::is_element))
        .and_then(|color| read_color_node(color, theme))
}

fn read_style_outline(
    shape: Node<'_, '_>,
    theme: &HashMap<String, PdfColor>,
) -> Option<PptxOutline> {
    let color = child(shape, "style")
        .and_then(|style| child(style, "lnRef"))
        .and_then(|reference| reference.children().find(Node::is_element))
        .and_then(|color| read_color_node(color, theme))?;
    Some(PptxOutline {
        color,
        width: 1.0,
        dash_pattern: Vec::new(),
    })
}

fn read_background(document: &Document<'_>, theme: &HashMap<String, PdfColor>) -> Option<PdfColor> {
    let background = document
        .descendants()
        .find(|node| is_element(*node, "bg"))?;
    child(background, "bgPr")
        .and_then(|node| read_fill(node, theme))
        .or_else(|| {
            child(background, "bgRef")
                .and_then(|node| node.children().find(Node::is_element))
                .and_then(|node| read_color_node(node, theme))
        })
}

fn apply_color_map(
    theme: &HashMap<String, PdfColor>,
    slide: &Document<'_>,
    layout: Option<&Document<'_>>,
    master: Option<&Document<'_>>,
) -> HashMap<String, PdfColor> {
    let mut mapped = theme.clone();
    let color_map = override_color_map(slide)
        .or_else(|| layout.and_then(override_color_map))
        .or_else(|| {
            master.and_then(|document| {
                document
                    .root_element()
                    .children()
                    .find(|node| is_element(*node, "clrMap"))
            })
        });
    let Some(color_map) = color_map else {
        return mapped;
    };
    for alias in [
        "bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ] {
        if let Some(source) = color_map.attribute(alias) {
            if let Some(color) = theme.get(source).copied() {
                mapped.insert(alias.to_owned(), color);
            }
        }
    }
    mapped
}

fn override_color_map<'a, 'input>(document: &'a Document<'input>) -> Option<Node<'a, 'input>> {
    document
        .root_element()
        .children()
        .find(|node| is_element(*node, "clrMapOvr"))
        .and_then(|node| child(node, "overrideClrMapping"))
}

fn read_theme_colors(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    relationships: &HashMap<String, Relationship>,
) -> Result<HashMap<String, PdfColor>> {
    let mut colors = default_theme_colors();
    let theme_path = relationships
        .values()
        .find(|relationship| relationship.rel_type.ends_with("/theme"))
        .map(|relationship| resolve_part_target("ppt/presentation.xml", &relationship.target))
        .unwrap_or_else(|| "ppt/theme/theme1.xml".to_owned());
    let Some(bytes) = read_zip_bytes(archive, &theme_path)? else {
        return Ok(colors);
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(colors);
    };
    let Ok(document) = Document::parse(&text) else {
        return Ok(colors);
    };
    if let Some(scheme) = document
        .descendants()
        .find(|node| is_element(*node, "clrScheme"))
    {
        for slot in scheme.children().filter(Node::is_element) {
            if let Some(color_node) = slot.children().find(Node::is_element) {
                if let Some(color) = read_color_node(color_node, &colors) {
                    colors.insert(slot.tag_name().name().to_owned(), color);
                }
            }
        }
    }
    add_theme_aliases(&mut colors);
    Ok(colors)
}

fn read_color_node(node: Node<'_, '_>, theme: &HashMap<String, PdfColor>) -> Option<PdfColor> {
    let color = match node.tag_name().name() {
        "srgbClr" => node.attribute("val").and_then(parse_hex_color),
        "schemeClr" => node
            .attribute("val")
            .and_then(|name| theme.get(name).copied()),
        "sysClr" => node
            .attribute("lastClr")
            .or_else(|| node.attribute("val"))
            .and_then(parse_hex_color),
        "prstClr" => node.attribute("val").and_then(preset_color),
        _ => None,
    }?;
    Some(apply_luminance(color, node))
}

fn default_theme_colors() -> HashMap<String, PdfColor> {
    let mut colors = HashMap::from([
        ("lt1".to_owned(), PdfColor::WHITE),
        ("dk1".to_owned(), PdfColor::BLACK),
        ("lt2".to_owned(), parse_hex_color("EEECE1").unwrap()),
        ("dk2".to_owned(), parse_hex_color("1F497D").unwrap()),
        ("accent1".to_owned(), parse_hex_color("4F81BD").unwrap()),
        ("accent2".to_owned(), parse_hex_color("C0504D").unwrap()),
        ("accent3".to_owned(), parse_hex_color("9BBB59").unwrap()),
        ("accent4".to_owned(), parse_hex_color("8064A2").unwrap()),
        ("accent5".to_owned(), parse_hex_color("4BACC6").unwrap()),
        ("accent6".to_owned(), parse_hex_color("F79646").unwrap()),
        ("hlink".to_owned(), parse_hex_color("0000FF").unwrap()),
        ("folHlink".to_owned(), parse_hex_color("800080").unwrap()),
    ]);
    add_theme_aliases(&mut colors);
    colors
}

fn add_theme_aliases(colors: &mut HashMap<String, PdfColor>) {
    for (alias, source) in [
        ("bg1", "lt1"),
        ("tx1", "dk1"),
        ("bg2", "lt2"),
        ("tx2", "dk2"),
    ] {
        if let Some(color) = colors.get(source).copied() {
            colors.insert(alias.to_owned(), color);
        }
    }
}

fn render_pptx(presentation: &PptxDocument, options: &ConversionOptions) -> PdfDocument {
    let mut document = PdfDocument::new();
    if presentation.slides.is_empty() {
        let size = options.page_size.unwrap_or(crate::PageSize {
            width: presentation.width,
            height: presentation.height,
        });
        document.add_page(size.width, size.height);
        return document;
    }
    for slide in &presentation.slides {
        render_slide(&mut document, slide, options);
    }
    document
}

fn render_slide(document: &mut PdfDocument, slide: &PptxSlide, options: &ConversionOptions) {
    let page_size = options.page_size.unwrap_or(crate::PageSize {
        width: slide.width,
        height: slide.height,
    });
    let scale_x = page_size.width / slide.width;
    let scale_y = page_size.height / slide.height;
    let image_ids: HashMap<usize, usize> = slide
        .elements
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            let PptxElement::Picture(picture) = element else {
                return None;
            };
            let image_id = match &picture.data {
                PptxImageData::Jpeg {
                    data,
                    width,
                    height,
                } => document.add_jpeg_image(data.clone(), *width, *height),
                PptxImageData::Rgba {
                    data,
                    width,
                    height,
                } => document.add_rgba_image(data.clone(), *width, *height),
                PptxImageData::Svg { .. } => return None,
            };
            Some((index, image_id))
        })
        .collect();
    let page = document.add_page(page_size.width, page_size.height);
    if let Some(background) = slide.background {
        page.add_rect(0.0, 0.0, page_size.width, page_size.height, background);
    }
    for (index, element) in slide.elements.iter().enumerate() {
        match element {
            PptxElement::Picture(picture) => {
                let bounds = scale_rect(picture.bounds, scale_x, scale_y);
                if let PptxImageData::Svg { data, crop } = &picture.data {
                    render_svg(
                        page,
                        data,
                        *crop,
                        bounds,
                        page_size.height,
                        slide.background,
                    );
                } else if let Some(image_id) = image_ids.get(&index) {
                    page.add_image(
                        *image_id,
                        bounds.x,
                        page_size.height - bounds.y - bounds.height,
                        bounds.width,
                        bounds.height,
                    );
                }
            }
            PptxElement::Line(line) => {
                render_line(page, line, page_size.height, scale_x, scale_y);
            }
            PptxElement::Shape(shape) => {
                render_shape(page, shape, page_size.height, scale_x, scale_y);
            }
        }
    }
}

/// 연결선 하나를 그린다. 꺾임과 화살촉을 함께 본다.
fn render_line(
    page: &mut crate::pdf::PdfPage,
    line: &PptxLine,
    page_height: f32,
    scale_x: f32,
    scale_y: f32,
) {
    let to_pdf = |x: f32, y: f32| (x * scale_x, page_height - y * scale_y);
    let width = line.outline.width * scale_x.min(scale_y);

    // 시작점 → 꺾임들 → 끝점. 꺾임이 없으면 두 점짜리 곧은 선이 된다.
    let mut points = Vec::with_capacity(line.bends.len() + 2);
    points.push(to_pdf(line.x1, line.y1));
    for bend in &line.bends {
        points.push(to_pdf(bend.0, bend.1));
    }
    points.push(to_pdf(line.x2, line.y2));

    for pair in points.windows(2) {
        page.add_line_with_dash_pattern(
            pair[0],
            pair[1],
            line.outline.color,
            width,
            &line.outline.dash_pattern,
        );
    }

    // 화살촉은 그 끝에 닿는 마디의 방향을 따른다. 꺾인 선에서 전체
    // 시작·끝을 이은 방향으로 그리면 화살촉만 비뚤게 붙는다.
    if let (Some(head), Some(pair)) = (line.heads.0, points.get(0..2)) {
        add_arrow_head(page, pair[0], pair[1], head, line.outline.color, width);
    }
    if let Some(head) = line.heads.1 {
        let last = points.len() - 1;
        if last > 0 {
            add_arrow_head(
                page,
                points[last],
                points[last - 1],
                head,
                line.outline.color,
                width,
            );
        }
    }
}

/// `tip` 자리에 `from` 쪽을 등지고 선 화살촉을 그린다.
fn add_arrow_head(
    page: &mut crate::pdf::PdfPage,
    tip: (f32, f32),
    from: (f32, f32),
    head: PptxArrowHead,
    color: PdfColor,
    line_width: f32,
) {
    let (dx, dy) = (from.0 - tip.0, from.1 - tip.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.01 {
        return;
    }
    // 뒤로 가는 단위 벡터와, 그것에 직각인 단위 벡터.
    let (ux, uy) = (dx / length, dy / length);
    let (px, py) = (-uy, ux);

    // 선이 굵을수록 촉도 커진다. 너무 작으면 보이지 않으므로 바닥을 둔다.
    let back = (line_width.max(0.5) * head.length).min(length);
    let half = line_width.max(0.5) * head.width / 2.0;

    let base = (tip.0 + ux * back, tip.1 + uy * back);
    let left = (base.0 + px * half, base.1 + py * half);
    let right = (base.0 - px * half, base.1 - py * half);

    if head.filled {
        page.add_path(
            vec![
                crate::pdf::PdfPathCommand::MoveTo(tip.0, tip.1),
                crate::pdf::PdfPathCommand::LineTo(left.0, left.1),
                crate::pdf::PdfPathCommand::LineTo(right.0, right.1),
                crate::pdf::PdfPathCommand::Close,
            ],
            color,
        );
    } else {
        // 열린 촉은 선 두 줄이다.
        page.add_line(tip.0, tip.1, left.0, left.1, color, line_width);
        page.add_line(tip.0, tip.1, right.0, right.1, color, line_width);
    }
}

fn render_svg(
    page: &mut crate::pdf::PdfPage,
    data: &[u8],
    crop: PptxCrop,
    bounds: PptxRect,
    page_height: f32,
    background: Option<PdfColor>,
) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(document) = Document::parse(text) else {
        return;
    };
    let root = document.root_element();
    let Some(mut view_box) = read_svg_view_box(root) else {
        return;
    };
    view_box.x += view_box.width * crop.left;
    view_box.y += view_box.height * crop.top;
    view_box.width *= (1.0 - crop.left - crop.right).max(0.01);
    view_box.height *= (1.0 - crop.top - crop.bottom).max(0.01);
    if view_box.width <= 0.0 || view_box.height <= 0.0 {
        return;
    }
    let scale_x = bounds.width / view_box.width;
    let scale_y = bounds.height / view_box.height;
    let pdf_bottom = page_height - bounds.y - bounds.height;
    for path in root.descendants().filter(|node| is_element(*node, "path")) {
        let Some(path_data) = path.attribute("d") else {
            continue;
        };
        let Some(fill) = svg_fill(path, background.unwrap_or(PdfColor::WHITE)) else {
            continue;
        };
        let commands = parse_svg_path(path_data, |x, y| {
            (
                bounds.x + (x - view_box.x) * scale_x,
                pdf_bottom + (view_box.height - (y - view_box.y)) * scale_y,
            )
        });
        page.add_path_with_rule(commands, fill, svg_even_odd(path));
    }
}

/// 이 경로를 짝홀 규칙으로 칠해야 하는가.
///
/// fill-rule 은 물려받는 속성이라 <g> 에 걸어 두고 그 안의 경로들이 따르는
/// 식으로 쓰는 일이 많다. 그래서 경로에 없으면 위로 거슬러 올라가며 찾는다.
/// 속성과 style="fill-rule:evenodd" 두 가지 적는 법을 다 본다.
fn svg_even_odd(path: Node<'_, '_>) -> bool {
    let mut node = Some(path);
    while let Some(current) = node {
        if let Some(value) = current.attribute("fill-rule") {
            return value.trim().eq_ignore_ascii_case("evenodd");
        }
        if let Some(style) = current.attribute("style") {
            for declaration in style.split(';') {
                let mut parts = declaration.splitn(2, ':');
                let (Some(name), Some(value)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if name.trim().eq_ignore_ascii_case("fill-rule") {
                    return value.trim().eq_ignore_ascii_case("evenodd");
                }
            }
        }
        node = current.parent_element();
    }
    false
}

fn read_svg_view_box(root: Node<'_, '_>) -> Option<PptxRect> {
    if let Some(value) = root.attribute("viewBox") {
        let values = parse_number_list(value);
        if values.len() >= 4 {
            return Some(PptxRect {
                x: values[0],
                y: values[1],
                width: values[2],
                height: values[3],
            });
        }
    }
    Some(PptxRect {
        x: 0.0,
        y: 0.0,
        width: parse_svg_length(root.attribute("width")?)?,
        height: parse_svg_length(root.attribute("height")?)?,
    })
}

fn parse_svg_length(value: &str) -> Option<f32> {
    let end = value
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit() && !matches!(character, '.' | '-' | '+'))
        .map(|(index, _)| index)
        .unwrap_or(value.len());
    value[..end].parse().ok()
}

fn svg_fill(path: Node<'_, '_>, background: PdfColor) -> Option<PdfColor> {
    let value = path.attribute("fill")?.trim();
    if value.eq_ignore_ascii_case("none") {
        return None;
    }
    let color = if value.starts_with('#') {
        let mut hex = value.trim_start_matches('#').to_owned();
        if hex.len() == 3 {
            hex = hex
                .chars()
                .flat_map(|character| [character, character])
                .collect();
        }
        parse_hex_color(&hex)
    } else {
        preset_color(value)
    }?;
    let opacity = path
        .attribute("fill-opacity")
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    Some(PdfColor::new(
        color.r * opacity + background.r * (1.0 - opacity),
        color.g * opacity + background.g * (1.0 - opacity),
        color.b * opacity + background.b * (1.0 - opacity),
    ))
}

fn parse_svg_path(path_data: &str, map: impl Fn(f32, f32) -> (f32, f32)) -> Vec<PdfPathCommand> {
    let tokens = tokenize_svg_path(path_data);
    let mut commands = Vec::new();
    let mut index = 0;
    let mut command = ' ';
    let mut current = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    let mut last_control = None;
    while index < tokens.len() {
        let iteration_start = index;
        if let Some(value) = command_token(&tokens[index]) {
            command = value;
            index += 1;
        }
        if command == ' ' {
            break;
        }
        let relative = command.is_ascii_lowercase();
        match command.to_ascii_uppercase() {
            'M' => {
                let mut first = true;
                while let Some((x, y)) = read_pair(&tokens, &mut index) {
                    current = if relative {
                        (current.0 + x, current.1 + y)
                    } else {
                        (x, y)
                    };
                    let point = map(current.0, current.1);
                    if first {
                        commands.push(PdfPathCommand::MoveTo(point.0, point.1));
                        start = current;
                        first = false;
                    } else {
                        commands.push(PdfPathCommand::LineTo(point.0, point.1));
                    }
                    last_control = None;
                }
            }
            'L' => {
                while let Some((x, y)) = read_pair(&tokens, &mut index) {
                    current = if relative {
                        (current.0 + x, current.1 + y)
                    } else {
                        (x, y)
                    };
                    let point = map(current.0, current.1);
                    commands.push(PdfPathCommand::LineTo(point.0, point.1));
                    last_control = None;
                }
            }
            'H' | 'V' => {
                while let Some(value) = read_number(&tokens, &mut index) {
                    if command.eq_ignore_ascii_case(&'H') {
                        current.0 = if relative { current.0 + value } else { value };
                    } else {
                        current.1 = if relative { current.1 + value } else { value };
                    }
                    let point = map(current.0, current.1);
                    commands.push(PdfPathCommand::LineTo(point.0, point.1));
                    last_control = None;
                }
            }
            'C' => {
                while let (Some((mut x1, mut y1)), Some((mut x2, mut y2)), Some((mut x, mut y))) = (
                    read_pair(&tokens, &mut index),
                    read_pair(&tokens, &mut index),
                    read_pair(&tokens, &mut index),
                ) {
                    if relative {
                        x1 += current.0;
                        y1 += current.1;
                        x2 += current.0;
                        y2 += current.1;
                        x += current.0;
                        y += current.1;
                    }
                    let first = map(x1, y1);
                    let second = map(x2, y2);
                    let end = map(x, y);
                    commands.push(PdfPathCommand::CurveTo(
                        first.0, first.1, second.0, second.1, end.0, end.1,
                    ));
                    current = (x, y);
                    last_control = Some((x2, y2));
                }
            }
            'S' => {
                while let (Some((mut x2, mut y2)), Some((mut x, mut y))) = (
                    read_pair(&tokens, &mut index),
                    read_pair(&tokens, &mut index),
                ) {
                    let first_control = last_control
                        .map(|point| (current.0 * 2.0 - point.0, current.1 * 2.0 - point.1))
                        .unwrap_or(current);
                    if relative {
                        x2 += current.0;
                        y2 += current.1;
                        x += current.0;
                        y += current.1;
                    }
                    let first = map(first_control.0, first_control.1);
                    let second = map(x2, y2);
                    let end = map(x, y);
                    commands.push(PdfPathCommand::CurveTo(
                        first.0, first.1, second.0, second.1, end.0, end.1,
                    ));
                    current = (x, y);
                    last_control = Some((x2, y2));
                }
            }
            'Z' => {
                commands.push(PdfPathCommand::Close);
                current = start;
                last_control = None;
            }
            _ => break,
        }
        if index == iteration_start {
            break;
        }
    }
    commands
}

fn tokenize_svg_path(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut number = String::new();
    for character in value.chars() {
        if matches!(character, 'e' | 'E') && !number.is_empty() && !number.ends_with(['e', 'E']) {
            number.push(character);
        } else if character.is_ascii_alphabetic() {
            if !number.is_empty() {
                tokens.push(std::mem::take(&mut number));
            }
            tokens.push(character.to_string());
        } else if character == ',' || character.is_whitespace() {
            if !number.is_empty() {
                tokens.push(std::mem::take(&mut number));
            }
        } else if matches!(character, '-' | '+')
            && !number.is_empty()
            && !number.ends_with('e')
            && !number.ends_with('E')
        {
            tokens.push(std::mem::take(&mut number));
            number.push(character);
        } else {
            number.push(character);
        }
    }
    if !number.is_empty() {
        tokens.push(number);
    }
    tokens
}

fn command_token(value: &str) -> Option<char> {
    let mut characters = value.chars();
    let character = characters.next()?;
    (characters.next().is_none() && character.is_ascii_alphabetic()).then_some(character)
}

fn read_number(tokens: &[String], index: &mut usize) -> Option<f32> {
    if *index >= tokens.len() || command_token(&tokens[*index]).is_some() {
        return None;
    }
    let value = tokens[*index].parse().ok()?;
    *index += 1;
    Some(value)
}

fn read_pair(tokens: &[String], index: &mut usize) -> Option<(f32, f32)> {
    let original = *index;
    let first = read_number(tokens, index)?;
    let Some(second) = read_number(tokens, index) else {
        *index = original;
        return None;
    };
    Some((first, second))
}

fn parse_number_list(value: &str) -> Vec<f32> {
    value
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn render_shape(
    page: &mut crate::pdf::PdfPage,
    shape: &PptxShape,
    page_height: f32,
    scale_x: f32,
    scale_y: f32,
) {
    let bounds = scale_rect(shape.bounds, scale_x, scale_y);
    let bottom = page_height - bounds.y - bounds.height;
    let ellipse = matches!(
        shape.shape_type.as_str(),
        "ellipse" | "arc" | "oval" | "flowChartConnector"
    );

    // 기본 도형의 모양을 점으로 받는다. 못 받으면 네모로 돌아간다 —
    // 모르는 도형을 안 그리는 것보다는 네모라도 자리를 잡는 편이 낫다.
    let outline_points = if let Some(points) = shape.polygon.as_ref() {
        Some(
            points
                .iter()
                .map(|(x, y)| (x * scale_x, page_height - y * scale_y))
                .collect::<Vec<_>>(),
        )
    } else if ellipse {
        None
    } else {
        crate::pptx_presets::preset_points(&shape.shape_type, bounds.width, bounds.height).map(
            |points| {
                points
                    .into_iter()
                    .map(|(fx, fy)| (bounds.x + fx * bounds.width, bottom + fy * bounds.height))
                    .collect::<Vec<_>>()
            },
        )
    };

    if let Some(fill) = shape.fill {
        match (&outline_points, ellipse) {
            // 점이 하나도 없는 모양이 올 수 있다. 빈 목록으로 칠하려
            // 들지 말고 네모로 돌아간다.
            (Some(points), _) if !points.is_empty() => {
                let mut commands =
                    vec![crate::pdf::PdfPathCommand::MoveTo(points[0].0, points[0].1)];
                commands.extend(
                    points[1..]
                        .iter()
                        .map(|(x, y)| crate::pdf::PdfPathCommand::LineTo(*x, *y)),
                );
                commands.push(crate::pdf::PdfPathCommand::Close);
                page.add_path(commands, fill);
            }
            (_, true) => page.add_ellipse(bounds.x, bottom, bounds.width, bounds.height, fill),
            (_, false) => page.add_rect(bounds.x, bottom, bounds.width, bounds.height, fill),
        }
    }

    if let Some(outline) = &shape.outline {
        if !ellipse {
            let left = bounds.x;
            let right = bounds.x + bounds.width;
            let top = page_height - bounds.y;
            let floor = top - bounds.height;
            let corners = outline_points.clone().unwrap_or_else(|| {
                vec![(left, top), (right, top), (right, floor), (left, floor)]
            });
            let width = outline.width * scale_x.min(scale_y);
            for index in 0..corners.len() {
                let start = corners[index];
                let end = corners[(index + 1) % corners.len()];
                page.add_line_with_dash_pattern(
                    start,
                    end,
                    outline.color,
                    width,
                    &outline.dash_pattern,
                );
            }
        }
    }
    render_text(page, shape, bounds, page_height, scale_x.min(scale_y));
}

fn render_text(
    page: &mut crate::pdf::PdfPage,
    shape: &PptxShape,
    bounds: PptxRect,
    page_height: f32,
    font_scale: f32,
) {
    if shape.paragraphs.is_empty() {
        return;
    }
    let text_body = shape.text_body;
    let left = bounds.x + text_body.left_inset * font_scale;
    let top = bounds.y + text_body.top_inset * font_scale;
    let width =
        (bounds.width - (text_body.left_inset + text_body.right_inset) * font_scale).max(1.0);
    let available_height =
        (bounds.height - (text_body.top_inset + text_body.bottom_inset) * font_scale).max(1.0);
    let total_height: f32 = shape
        .paragraphs
        .iter()
        .map(|paragraph| paragraph_height(paragraph, width, font_scale))
        .sum();
    let mut current_top = match text_body.anchor {
        VerticalAnchor::Top => top,
        VerticalAnchor::Middle => top + ((available_height - total_height) / 2.0).max(0.0),
        VerticalAnchor::Bottom => top + (available_height - total_height).max(0.0),
    };
    page.push_clip(
        bounds.x,
        page_height - bounds.y - bounds.height,
        bounds.width,
        bounds.height,
    );
    for paragraph in &shape.paragraphs {
        current_top += paragraph.space_before * font_scale;
        let paragraph_left =
            left + (paragraph.margin_left + paragraph.indent.min(0.0)).max(0.0) * font_scale;
        let paragraph_width = (width
            - (paragraph.margin_left + paragraph.indent.min(0.0)).max(0.0) * font_scale)
            .max(1.0);
        let lines = wrap_paragraph(paragraph, paragraph_width, font_scale);
        for line in lines {
            let line_width: f32 = line.iter().map(|segment| segment.width).sum();
            let line_height = line
                .iter()
                .map(|segment| segment.run.font_size * font_scale)
                .fold(12.0, f32::max)
                * paragraph.line_spacing;
            let mut x = match paragraph.alignment {
                TextAlignment::Left => paragraph_left,
                TextAlignment::Center => {
                    paragraph_left + ((paragraph_width - line_width) / 2.0).max(0.0)
                }
                TextAlignment::Right => paragraph_left + (paragraph_width - line_width).max(0.0),
            };
            for segment in line {
                let font_size = segment.run.font_size * font_scale;
                let baseline = page_height - current_top - font_size;
                page.add_styled_text_with_font(
                    &segment.text,
                    x,
                    baseline,
                    font_size,
                    segment.run.color,
                    segment.run.bold,
                    segment.run.italic,
                    segment.run.font_name.as_deref(),
                );
                if segment.run.underline && !segment.text.trim().is_empty() {
                    page.add_line(
                        x,
                        baseline - font_size * 0.08,
                        x + segment.width,
                        baseline - font_size * 0.08,
                        segment.run.color,
                        (font_size * 0.05).max(0.5),
                    );
                }
                x += segment.width;
            }
            current_top += line_height;
        }
        if current_top > bounds.y + bounds.height {
            break;
        }
    }
    page.pop_clip();
}

#[derive(Debug)]
struct TextSegment<'a> {
    text: String,
    run: &'a PptxRun,
    width: f32,
}

fn wrap_paragraph<'a>(
    paragraph: &'a PptxParagraph,
    max_width: f32,
    font_scale: f32,
) -> Vec<Vec<TextSegment<'a>>> {
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut line_width = 0.0;
    for run in &paragraph.runs {
        for token in split_text_tokens(&run.text) {
            if token == "\n" {
                lines.push(std::mem::take(&mut line));
                line_width = 0.0;
                continue;
            }
            let font_size = run.font_size * font_scale;
            let width = styled_text_width_with_font(
                &token,
                font_size,
                run.bold,
                run.italic,
                run.font_name.as_deref(),
            );
            if !line.is_empty() && line_width + width > max_width {
                lines.push(std::mem::take(&mut line));
                line_width = 0.0;
            }
            line.push(TextSegment {
                text: token,
                run,
                width,
            });
            line_width += width;
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

fn paragraph_height(paragraph: &PptxParagraph, width: f32, font_scale: f32) -> f32 {
    let lines = wrap_paragraph(paragraph, width, font_scale);
    paragraph.space_before * font_scale
        + lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|segment| segment.run.font_size * font_scale)
                    .fold(12.0, f32::max)
                    * paragraph.line_spacing
            })
            .sum::<f32>()
}

fn split_text_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut whitespace = false;
    for character in text.replace('\t', "    ").chars() {
        if character == '\r' {
            continue;
        }
        if character == '\n' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push("\n".to_owned());
            continue;
        }
        let is_whitespace = character.is_whitespace();
        if !current.is_empty() && is_whitespace != whitespace {
            tokens.push(std::mem::take(&mut current));
        }
        current.push(character);
        whitespace = is_whitespace;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn scale_rect(rect: PptxRect, scale_x: f32, scale_y: f32) -> PptxRect {
    PptxRect {
        x: rect.x * scale_x,
        y: rect.y * scale_y,
        width: rect.width * scale_x,
        height: rect.height * scale_y,
    }
}

fn read_relationships(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    part_path: &str,
) -> Result<HashMap<String, Relationship>> {
    let relationship_path = relationship_part_path(part_path);
    let Some(bytes) = read_zip_bytes(archive, &relationship_path)? else {
        return Ok(HashMap::new());
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| MiniPdfError::InvalidInput(format!("{relationship_path} is not UTF-8")))?;
    let document = Document::parse(&text)?;
    Ok(document
        .descendants()
        .filter(|node| is_element(*node, "Relationship"))
        .filter_map(|node| {
            Some((
                node.attribute("Id")?.to_owned(),
                Relationship {
                    target: node.attribute("Target")?.to_owned(),
                    rel_type: node.attribute("Type").unwrap_or_default().to_owned(),
                    external: node
                        .attribute("TargetMode")
                        .is_some_and(|value| value.eq_ignore_ascii_case("External")),
                    relationship_path: relationship_path.clone(),
                },
            ))
        })
        .collect())
}

type RelatedXml = (String, String, HashMap<String, Relationship>);

fn read_related_xml(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    source_path: &str,
    relationships: &HashMap<String, Relationship>,
    type_suffix: &str,
) -> Result<Option<RelatedXml>> {
    let Some(relationship) = relationships.values().find(|relationship| {
        relationship.rel_type.ends_with(type_suffix) && !relationship.external
    }) else {
        return Ok(None);
    };
    let path = resolve_part_target(source_path, &relationship.target);
    let Some(bytes) = read_zip_bytes(archive, &path)? else {
        return Ok(None);
    };
    let xml = String::from_utf8(bytes)
        .map_err(|_| MiniPdfError::InvalidInput(format!("{path} is not UTF-8")))?;
    let part_relationships = read_relationships(archive, &path)?;
    Ok(Some((path, xml, part_relationships)))
}

fn read_zip_bytes(archive: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Result<Option<Vec<u8>>> {
    let Ok(mut entry) = archive.by_name(path) else {
        return Ok(None);
    };
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

fn relationship_part_path(part_path: &str) -> String {
    match part_path.rsplit_once('/') {
        Some((directory, file_name)) => format!("{directory}/_rels/{file_name}.rels"),
        None => format!("_rels/{part_path}.rels"),
    }
}

fn relationship_source_path(relationship: &Relationship) -> String {
    let Some((directory, file_name)) = relationship.relationship_path.split_once("/_rels/") else {
        return relationship.relationship_path.clone();
    };
    format!("{directory}/{}", file_name.trim_end_matches(".rels"))
}

fn resolve_part_target(source_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.trim_start_matches('/').to_owned();
    }
    let directory = source_path
        .rsplit_once('/')
        .map(|(path, _)| path)
        .unwrap_or_default();
    let mut normalized = Vec::new();
    let combined = format!("{directory}/{target}");
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                normalized.pop();
            }
            value => normalized.push(value),
        }
    }
    normalized.join("/")
}

fn child<'a, 'input>(node: Node<'a, 'input>, local_name: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child_node| is_element(*child_node, local_name))
}

fn is_element(node: Node<'_, '_>, local_name: &str) -> bool {
    node.is_element() && node.tag_name().name() == local_name
}

fn attribute_by_local_name<'a, 'input>(node: Node<'a, 'input>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn integer_attribute(node: Option<Node<'_, '_>>, name: &str) -> Option<i64> {
    node?.attribute(name)?.parse().ok()
}

fn emu_attribute(node: Option<Node<'_, '_>>, name: &str) -> Option<f32> {
    integer_attribute(node, name).map(|value| value as f32 / EMUS_PER_POINT)
}

fn crop_attribute(node: Node<'_, '_>, name: &str) -> f32 {
    node.attribute(name)
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| (value / 100_000.0).clamp(0.0, 0.95))
        .unwrap_or(0.0)
}

fn optional_bool_attribute(node: Option<Node<'_, '_>>, name: &str) -> Option<bool> {
    node?.attribute(name).map(read_bool)
}

fn read_bool(value: &str) -> bool {
    value == "1" || value.eq_ignore_ascii_case("true")
}

fn parse_hex_color(value: &str) -> Option<PdfColor> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    Some(PdfColor::new(
        u8::from_str_radix(&value[0..2], 16).ok()? as f32 / 255.0,
        u8::from_str_radix(&value[2..4], 16).ok()? as f32 / 255.0,
        u8::from_str_radix(&value[4..6], 16).ok()? as f32 / 255.0,
    ))
}

fn preset_color(value: &str) -> Option<PdfColor> {
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(PdfColor::BLACK),
        "white" => Some(PdfColor::WHITE),
        "red" => Some(PdfColor::new(1.0, 0.0, 0.0)),
        "green" => Some(PdfColor::new(0.0, 0.5, 0.0)),
        "blue" => Some(PdfColor::new(0.0, 0.0, 1.0)),
        "yellow" => Some(PdfColor::new(1.0, 1.0, 0.0)),
        _ => None,
    }
}

fn apply_luminance(color: PdfColor, node: Node<'_, '_>) -> PdfColor {
    let modifier = child(node, "lumMod")
        .and_then(|node| node.attribute("val"))
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| value / 100_000.0)
        .unwrap_or(1.0);
    let offset = child(node, "lumOff")
        .and_then(|node| node.attribute("val"))
        .and_then(|value| value.parse::<f32>().ok())
        .map(|value| value / 100_000.0)
        .unwrap_or(0.0);
    if (modifier - 1.0).abs() < f32::EPSILON && offset.abs() < f32::EPSILON {
        return color;
    }
    let (hue, saturation, luminance) = rgb_to_hsl(color);
    hsl_to_rgb(
        hue,
        saturation,
        (luminance * modifier + offset).clamp(0.0, 1.0),
    )
}

fn rgb_to_hsl(color: PdfColor) -> (f32, f32, f32) {
    let max = color.r.max(color.g).max(color.b);
    let min = color.r.min(color.g).min(color.b);
    let luminance = (max + min) / 2.0;
    if (max - min).abs() < 0.000_01 {
        return (0.0, 0.0, luminance);
    }
    let delta = max - min;
    let saturation = if luminance > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if (max - color.r).abs() < 0.000_01 {
        (color.g - color.b) / delta + if color.g < color.b { 6.0 } else { 0.0 }
    } else if (max - color.g).abs() < 0.000_01 {
        (color.b - color.r) / delta + 2.0
    } else {
        (color.r - color.g) / delta + 4.0
    };
    (hue / 6.0, saturation, luminance)
}

fn hsl_to_rgb(hue: f32, saturation: f32, luminance: f32) -> PdfColor {
    if saturation <= 0.000_01 {
        return PdfColor::new(luminance, luminance, luminance);
    }
    let upper = if luminance < 0.5 {
        luminance * (1.0 + saturation)
    } else {
        luminance + saturation - luminance * saturation
    };
    let lower = 2.0 * luminance - upper;
    PdfColor::new(
        hue_to_rgb(lower, upper, hue + 1.0 / 3.0),
        hue_to_rgb(lower, upper, hue),
        hue_to_rgb(lower, upper, hue - 1.0 / 3.0),
    )
}

fn hue_to_rgb(lower: f32, upper: f32, mut hue: f32) -> f32 {
    if hue < 0.0 {
        hue += 1.0;
    }
    if hue > 1.0 {
        hue -= 1.0;
    }
    if hue < 1.0 / 6.0 {
        lower + (upper - lower) * 6.0 * hue
    } else if hue < 0.5 {
        upper
    } else if hue < 2.0 / 3.0 {
        lower + (upper - lower) * (2.0 / 3.0 - hue) * 6.0
    } else {
        lower
    }
}

fn select_font_name(text: &str, properties: Option<Node<'_, '_>>) -> Option<String> {
    let properties = properties?;
    let preferred = if contains_east_asian(text) {
        child(properties, "ea")
    } else if contains_complex_script(text) {
        child(properties, "cs")
    } else {
        child(properties, "latin")
    };
    preferred
        .or_else(|| child(properties, "latin"))
        .or_else(|| child(properties, "ea"))
        .or_else(|| child(properties, "cs"))
        .and_then(|node| node.attribute("typeface"))
        .filter(|name| !name.starts_with('+'))
        .map(str::to_owned)
}

fn contains_east_asian(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(character as u32, 0x3000..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xff00..=0xffef)
    })
}

fn contains_complex_script(text: &str) -> bool {
    text.chars().any(
        |character| matches!(character as u32, 0x0590..=0x08ff | 0x0900..=0x0d7f | 0x0e00..=0x0eff),
    )
}

fn natural_slide_number(path: &str) -> u32 {
    path.rsplit_once("slide")
        .and_then(|(_, suffix)| suffix.trim_end_matches(".xml").parse().ok())
        .unwrap_or(u32::MAX)
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u16, u16)> {
    if data.len() < 4 || data[..2] != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2;
    while offset + 4 <= data.len() {
        if data[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = data[offset + 1];
        offset += 2;
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        if offset + 2 > data.len() {
            break;
        }
        let length = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        if length < 2 || offset + length > data.len() {
            break;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) && length >= 7 {
            return Some((
                u16::from_be_bytes([data[offset + 5], data[offset + 6]]),
                u16::from_be_bytes([data[offset + 3], data[offset + 4]]),
            ));
        }
        offset += length;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    use super::{convert_pptx_bytes, read_pptx};
    use crate::{convert_bytes_to_pdf, detect_office_format, ConversionOptions, OfficeFormat};

    #[test]
    fn converts_text_shapes_and_custom_slide_size() {
        let pptx = create_pptx();
        assert_eq!(detect_office_format(&pptx).unwrap(), OfficeFormat::Pptx);
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        assert_eq!(presentation.slides.len(), 1);
        assert!((presentation.width - 720.0).abs() < 0.01);
        assert!((presentation.height - 540.0).abs() < 0.01);
        assert_eq!(presentation.slides[0].elements.len(), 2);
        let super::PptxElement::Shape(shape) = &presentation.slides[0].elements[0] else {
            panic!("first element is a shape");
        };
        assert_eq!(shape.paragraphs[0].runs[0].text, "Hello Rust PPTX");

        let pdf = convert_pptx_bytes(&pptx, &ConversionOptions::default()).expect("PPTX converts");
        let text = String::from_utf8_lossy(&pdf);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert!(pdf.len() > 500);
        assert!(text.contains("/MediaBox [0 0 720.00 540.00]"));

        let detected_pdf = convert_bytes_to_pdf(&pptx).expect("detected PPTX converts");
        assert!(detected_pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn renders_office_svg_and_smartart_fallback() {
        let pptx = create_svg_and_smartart_pptx();
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        assert!(presentation.slides[0].elements.iter().any(|element| {
            matches!(
                element,
                super::PptxElement::Picture(super::PptxPicture {
                    data: super::PptxImageData::Svg { .. },
                    ..
                })
            )
        }));
        assert!(presentation.slides[0].elements.iter().any(|element| {
            let super::PptxElement::Shape(shape) = element else {
                return false;
            };
            shape.paragraphs.iter().any(|paragraph| {
                paragraph
                    .runs
                    .iter()
                    .any(|run| run.text.contains("SmartArt Text"))
            })
        }));

        let pdf = convert_pptx_bytes(&pptx, &ConversionOptions::default()).expect("PPTX converts");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.len() > 700);
    }

    #[test]
    fn inherits_placeholder_bounds_and_text_layout() {
        let pptx = create_placeholder_pptx();
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        let shapes: Vec<_> = presentation.slides[0]
            .elements
            .iter()
            .filter_map(|element| {
                let super::PptxElement::Shape(shape) = element else {
                    return None;
                };
                Some(shape)
            })
            .collect();
        assert_eq!(shapes.len(), 2);
        assert!((shapes[0].bounds.x - 72.0).abs() < 0.01);
        assert!((shapes[0].bounds.width - 288.0).abs() < 0.01);
        assert!(matches!(
            shapes[0].paragraphs[0].alignment,
            super::TextAlignment::Center
        ));
        assert!((shapes[0].paragraphs[0].runs[0].font_size - 24.0).abs() < 0.01);
        assert!(shapes[1].paragraphs[1].runs[0].text.starts_with('\u{2022}'));

        let pdf = convert_pptx_bytes(&pptx, &ConversionOptions::default()).expect("PPTX converts");
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn applies_master_color_map_aliases() {
        let theme = super::default_theme_colors();
        let slide = roxmltree::Document::parse(
            r#"<p:sld xmlns:p="urn:p"><p:clrMapOvr><p:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        )
        .unwrap();
        let master = roxmltree::Document::parse(
            r#"<p:sldMaster xmlns:p="urn:p"><p:clrMap bg1="dk1" tx1="lt1"/></p:sldMaster>"#,
        )
        .unwrap();
        let mapped = super::apply_color_map(&theme, &slide, None, Some(&master));
        assert_eq!(mapped["bg1"], super::PdfColor::BLACK);
        assert_eq!(mapped["tx1"], super::PdfColor::WHITE);
    }

    #[test]
    fn preserves_presentation_slide_order() {
        let pptx = create_reordered_pptx();
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        let slide_text = |index: usize| {
            presentation.slides[index]
                .elements
                .iter()
                .find_map(|element| {
                    let super::PptxElement::Shape(shape) = element else {
                        return None;
                    };
                    shape
                        .paragraphs
                        .first()?
                        .runs
                        .first()
                        .map(|run| run.text.as_str())
                })
                .unwrap()
        };
        assert_eq!(slide_text(0), "Second");
        assert_eq!(slide_text(1), "First");
    }

    #[test]
    fn falls_back_when_slide_relationship_targets_are_missing() {
        let pptx = create_broken_relationship_pptx();
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        assert_eq!(presentation.slides.len(), 1);
        let super::PptxElement::Shape(shape) = &presentation.slides[0].elements[0] else {
            panic!("fallback slide element is a shape");
        };
        assert_eq!(shape.paragraphs[0].runs[0].text, "Recovered");
    }

    #[test]
    fn skips_pictures_with_non_positive_bounds() {
        let pptx = create_invalid_picture_bounds_pptx();
        let presentation = read_pptx(&pptx).expect("PPTX parses");
        assert!(presentation.slides[0]
            .elements
            .iter()
            .all(|element| !matches!(element, super::PptxElement::Picture(_))));
    }

    /// 번호를 매겨 둔 목록은 번호로 나와야 한다.
    ///
    /// buAutoNum 을 가운뎃점으로 그리던 동안, 차례가 있는 글이 차례 없는
    /// 글이 됐다 — 쪽 4 의 "1. 2." 가 "• •" 로 나온 것이 그것이다.
    #[test]
    fn numbers_the_lists_that_asked_to_be_numbered() {
        assert_eq!(super::format_auto_number("arabicPeriod", 1), "1.");
        assert_eq!(super::format_auto_number("arabicPeriod", 12), "12.");
        assert_eq!(super::format_auto_number("arabicParenR", 3), "3)");
        assert_eq!(super::format_auto_number("arabicParenBoth", 3), "(3)");
        assert_eq!(super::format_auto_number("alphaUcPeriod", 1), "A.");
        assert_eq!(super::format_auto_number("alphaLcParenR", 2), "b)");
        assert_eq!(super::format_auto_number("romanUcPeriod", 4), "IV.");
        assert_eq!(super::format_auto_number("romanLcPeriod", 9), "ix.");
        // 모르는 꼴이면 숫자만.
        assert_eq!(super::format_auto_number("someScheme", 7), "7");

        // 스물여섯을 넘어가도 이어진다.
        assert_eq!(super::alpha_numeral(26), "Z");
        assert_eq!(super::alpha_numeral(27), "AA");

        // 글상자 하나를 통째로 거쳐 번호가 이어지는지 본다.
        let xml = r#"<p:txBody xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:bodyPr/><a:p><a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:t>첫째</a:t></a:r></a:p><a:p><a:pPr><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:t>둘째</a:t></a:r></a:p><a:p><a:pPr><a:buChar char="▪"/></a:pPr><a:r><a:t>점</a:t></a:r></a:p></p:txBody>"#;
        let doc = roxmltree::Document::parse(xml).expect("parses");
        let body = doc.root_element();
        let paragraphs = super::read_paragraphs(
            body, &std::collections::HashMap::new(), None, None, super::TextAlignment::Left);
        let texts: Vec<String> = paragraphs.iter()
            .map(|p| p.runs.iter().map(|r| r.text.clone()).collect::<String>())
            .collect();
        assert_eq!(texts[0], "1. 첫째", "첫 번호가 1 이어야 한다");
        assert_eq!(texts[1], "2. 둘째", "번호가 이어져야 한다");
        assert_eq!(texts[2], "▪ 점", "적어 둔 글자를 써야 한다");
    }

    /// chExt 가 0 인 그룹의 자식이 제자리에 있어야 한다.
    ///
    /// 0 이 그대로 나누는 쪽으로 가면 `.max(1.0)` 때문에 1 로 나뉘고,
    /// 자식 좌표가 그대로 배율이 되어 도형이 슬라이드를 꿰뚫는다.
    #[test]
    fn a_group_with_no_child_extent_does_not_blow_up_its_children() {
        let xml = r#"<a:xfrm xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:off x="1000000" y="500000"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm>"#;
        let doc = roxmltree::Document::parse(xml).expect("parses");
        let parent = super::CoordinateMap {
            target_x: 0.0, target_y: 0.0,
            target_width: 9906000.0, target_height: 6858000.0,
            source_x: 0.0, source_y: 0.0,
            source_width: 9906000.0, source_height: 6858000.0,
        };
        let map = super::create_group_map(doc.root_element(), parent);
        // chExt 가 0 이면 그룹 제 크기로 물러서야 한다.
        assert_eq!(map.source_width, 2000000.0);
        assert_eq!(map.source_height, 1000000.0);

        // 그 그룹 안의 도형이 그룹 밖으로 터져 나가지 않아야 한다.
        let r = map.map_rect(1000000, 500000, 500000, 250000);
        assert!(r.width <= 2000000.0 && r.height <= 1000000.0,
                "자식이 그룹보다 커졌다: {r:?}");
    }

    /// 기호 글꼴의 글머리 기호는 그 글꼴에서의 모양으로 나와야 한다.
    ///
    /// 적어 둔 글자를 그대로 내보내면 Wingdings 의 채운 네모가 화면에
    /// '§' 로 찍힌다.
    #[test]
    fn maps_symbol_font_bullets_to_what_they_mean() {
        assert_eq!(super::bullet_glyph("\u{a7}", "Wingdings"), "\u{25aa}");
        assert_eq!(super::bullet_glyph("n", "Wingdings"), "\u{25a0}");
        assert_eq!(super::bullet_glyph("l", "Wingdings"), "\u{25cf}");
        // 모르는 기호는 가운뎃점으로.
        assert_eq!(super::bullet_glyph("\u{2b}", "Wingdings"), "\u{2022}");
        // 보통 글꼴이면 적어 둔 그대로.
        assert_eq!(super::bullet_glyph("\u{2013}", "Arial"), "\u{2013}");
        assert_eq!(super::bullet_glyph("\u{25aa}", ""), "\u{25aa}");
    }

    /// 꺾인 연결선은 꺾인 채로, 화살표는 촉을 달고 나와야 한다.
    ///
    /// 전에는 연결선의 prstGeom 을 보지 않아 bentConnector3 가 상자 모서리를
    /// 잇는 대각선 하나로 나왔고, headEnd·tailEnd 를 읽는 코드가 아예 없어
    /// 모든 화살표가 맨 선이었다.
    #[test]
    fn draws_connectors_bent_and_with_their_arrow_heads() {
        // 가로 200 세로 100 짜리 자리에 놓인 ㄱ자 연결선.
        let bends = super::connector_bends(
            "bentConnector3",
            super::PptxRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
            false,
            false,
        );
        assert_eq!(bends, vec![(100.0, 0.0), (100.0, 100.0)],
                   "가운데에서 두 번 꺾여야 한다");

        // 곧은 연결선은 꺾이지 않는다.
        assert!(super::connector_bends(
            "straightConnector1",
            super::PptxRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
            false, false).is_empty());

        // 뒤집힌 연결선은 꺾이는 쪽도 따라 뒤집힌다.
        let flipped = super::connector_bends(
            "bentConnector2",
            super::PptxRect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
            true, false,
        );
        assert_eq!(flipped, vec![(0.0, 0.0)], "flipH 면 끝 x 가 왼쪽이다");

        // 그려 보고 촉이 실제로 들어갔는지 본다.
        fn content(line: super::PptxLine) -> String {
            let mut document = crate::pdf::PdfDocument::new();
            {
                let page = document.add_page(300.0, 200.0);
                super::render_line(page, &line, 200.0, 1.0, 1.0);
            }
            String::from_utf8_lossy(&document.to_bytes()).into_owned()
        }
        let outline = super::PptxOutline {
            color: crate::pdf::PdfColor::new(0.0, 0.0, 0.0),
            width: 2.0,
            dash_pattern: Vec::new(),
        };
        let head = super::PptxArrowHead { length: 3.5, width: 3.5, filled: true };

        // 촉이 없으면 채운 경로가 없어야 한다.
        let bare = content(super::PptxLine {
            x1: 0.0, y1: 0.0, x2: 100.0, y2: 0.0,
            outline: outline.clone(), bends: Vec::new(), heads: (None, None),
        });
        assert!(!bare.contains(" f\n"), "촉이 없는데 삼각형을 채웠다");

        // 촉이 있으면 채운 삼각형이 생긴다.
        let tipped = content(super::PptxLine {
            x1: 0.0, y1: 0.0, x2: 100.0, y2: 0.0,
            outline: outline.clone(), bends: Vec::new(), heads: (None, Some(head)),
        });
        assert!(tipped.contains(" f\n"), "tailEnd 촉을 안 그렸다");

        // 꺾인 선은 마디 수만큼 선이 늘어난다.
        let straight_ops = bare.matches(" S\n").count();
        let bent = content(super::PptxLine {
            x1: 0.0, y1: 0.0, x2: 200.0, y2: 100.0,
            outline, bends: vec![(100.0, 0.0), (100.0, 100.0)], heads: (None, None),
        });
        assert!(bent.matches(" S\n").count() > straight_ops,
                "꺾인 선인데 마디가 하나뿐이다");
    }

    /// 짝홀 규칙으로 그린 아이콘은 구멍이 뚫린 채 나와야 한다.
    ///
    /// 바깥 네모와 안쪽 네모를 같은 방향으로 그려 둔 모양이다. nonzero 로
    /// 칠하면 두 네모가 겹쳐 속이 꽉 찬 사각형이 되고, 그것이 슬라이드에서
    /// 아이콘 자리마다 검은 덩어리가 나오던 모습이다.
    #[test]
    fn fills_svg_icons_with_the_rule_the_file_asks_for() {
        fn content_of(svg: &str) -> String {
            let mut document = crate::pdf::PdfDocument::new();
            {
                let page = document.add_page(100.0, 100.0);
                super::render_svg(
                    page,
                    svg.as_bytes(),
                    super::PptxCrop::default(),
                    super::PptxRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
                    100.0,
                    None,
                );
            }
            String::from_utf8_lossy(&document.to_bytes()).into_owned()
        }

        const HEAD: &str =
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">"##;
        // 바깥 네모 하나, 안쪽 네모 하나. 짝홀이면 가운데가 뚫린다.
        const RING: &str = "M0 0 H10 V10 H0 Z M2 2 H8 V8 H2 Z";

        // 경로에 바로 적은 경우.
        let direct = format!(
            r##"{HEAD}<path fill-rule="evenodd" fill="#000000" d="{RING}"/></svg>"##
        );
        assert!(content_of(&direct).contains("f*"),
                "fill-rule=evenodd 인데 짝홀로 칠하지 않았다");

        // <g> 에 걸어 두고 물려받는 경우. 아이콘은 보통 이렇게 생겼다.
        let inherited = format!(
            r##"{HEAD}<g fill-rule="evenodd"><path fill="#000000" d="{RING}"/></g></svg>"##
        );
        assert!(content_of(&inherited).contains("f*"),
                "<g> 에서 물려받은 fill-rule 을 못 봤다");

        // style 로 적는 경우.
        let styled = format!(
            r##"{HEAD}<path style="fill-rule:evenodd" fill="#000000" d="{RING}"/></svg>"##
        );
        assert!(content_of(&styled).contains("f*"), "style 의 fill-rule 을 못 봤다");

        // 아무 말이 없으면 SVG 기본값인 nonzero 그대로여야 한다.
        let plain = format!(r##"{HEAD}<path fill="#000000" d="{RING}"/></svg>"##);
        let out = content_of(&plain);
        assert!(!out.contains("f*"), "말이 없는데 짝홀로 칠했다");
        assert!(out.contains("f\n"), "칠하기는 해야 한다");
    }

    #[test]
    fn parses_svg_path_scientific_notation_without_stalling() {
        let commands = super::parse_svg_path("M1e-3 2E+2 L3.5e1 4e0 Z", |x, y| (x, y));
        assert_eq!(commands.len(), 3);
        assert!(
            matches!(commands[0], super::PdfPathCommand::MoveTo(x, y) if (x - 0.001).abs() < f32::EPSILON && (y - 200.0).abs() < f32::EPSILON)
        );
        assert!(
            matches!(commands[1], super::PdfPathCommand::LineTo(x, y) if (x - 35.0).abs() < f32::EPSILON && (y - 4.0).abs() < f32::EPSILON)
        );
        assert!(matches!(commands[2], super::PdfPathCommand::Close));
    }

    fn create_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:spPr><a:xfrm><a:off x="127000" y="254000"/><a:ext cx="2540000" cy="1270000"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:srgbClr val="336699"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="1800" b="1"/><a:t>Hello Rust PPTX</a:t></a:r></a:p></p:txBody></p:sp><p:cxnSp><p:spPr><a:xfrm><a:off x="127000" y="1778000"/><a:ext cx="2540000" cy="127000"/></a:xfrm><a:ln w="25400"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:prstDash val="dash"/></a:ln></p:spPr></p:cxnSp></p:spTree></p:cSld></p:sld>"#,
            );
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn create_svg_and_smartart_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram"><p:cSld><p:spTree><p:pic><p:nvPicPr><p:cNvPr id="2" name="SVG"/></p:nvPicPr><p:blipFill><a:blip><a:extLst><a:ext><asvg:svgBlip r:embed="rIdImage"/></a:ext></a:extLst></a:blip><a:srcRect l="25000" t="25000" r="25000" b="25000"/></p:blipFill><p:spPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="1828800" cy="1828800"/></a:xfrm></p:spPr></p:pic><p:graphicFrame><p:xfrm><a:off x="3657600" y="914400"/><a:ext cx="3657600" cy="1828800"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds/></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.svg"/><Relationship Id="rIdDiagram" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="../diagrams/drawing1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/media/image1.svg",
                r##"<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg"><path d="M0 0 L100 0 L100 100 Z" fill="#E56925"/></svg>"##,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/diagrams/drawing1.xml",
                r#"<dsp:drawing xmlns:dsp="http://schemas.microsoft.com/office/drawing/2008/diagram" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><dsp:spTree><dsp:sp><dsp:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="3657600" cy="1828800"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:srgbClr val="2F5597"/></a:solidFill></dsp:spPr><dsp:txBody><a:bodyPr/><a:p><a:r><a:rPr sz="1800"><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill></a:rPr><a:t>SmartArt Text</a:t></a:r></a:p></dsp:txBody></dsp:sp></dsp:spTree></dsp:drawing>"#,
            );
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn create_placeholder_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr anchor="ctr"/><a:p><a:r><a:t>Centered Title</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p><a:r><a:t>Heading</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:t>Indented bullet</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slideLayouts/slideLayout1.xml",
                r#"<p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="914400" y="914400"/><a:ext cx="3657600" cy="914400"/></a:xfrm></p:spPr><p:txBody><a:bodyPr anchor="ctr"/><a:lstStyle><a:lvl1pPr algn="ctr"><a:defRPr sz="2400"/></a:lvl1pPr></a:lstStyle></p:txBody></p:sp><p:sp><p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="914400" y="2286000"/><a:ext cx="3657600" cy="2743200"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr marL="0" indent="0"><a:buNone/><a:defRPr sz="1800"/></a:lvl1pPr><a:lvl2pPr marL="283464" indent="-283464"><a:defRPr sz="1800"/></a:lvl2pPr></a:lstStyle></p:txBody></p:sp></p:spTree></p:cSld></p:sldLayout>"#,
            );
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn create_reordered_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId1"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide2.xml"/></Relationships>"#,
            );
            for (index, text) in [(1, "First"), (2, "Second")] {
                write_entry(
                    &mut archive,
                    options,
                    &format!("ppt/slides/slide{index}.xml"),
                    &format!(
                        r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="1270000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
                    ),
                );
            }
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn create_broken_relationship_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rIdMissing"/></p:sldIdLst></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdMissing" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/missing.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree><p:sp><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="1270000"/></a:xfrm></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:t>Recovered</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#,
            );
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn create_invalid_picture_bounds_pptx() -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            write_entry(
                &mut archive,
                options,
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst></p:presentation>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:pic><p:blipFill><a:blip r:embed="rIdImage"/></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="1270000"/></a:xfrm></p:spPr></p:pic><p:pic><p:blipFill><a:blip r:embed="rIdImage"/></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="-1"/></a:xfrm></p:spPr></p:pic></p:spTree></p:cSld></p:sld>"#,
            );
            write_entry(
                &mut archive,
                options,
                "ppt/slides/_rels/slide1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdImage" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.jpg"/></Relationships>"#,
            );
            archive
                .start_file("ppt/media/image1.jpg", options)
                .expect("start image entry");
            archive
                .write_all(&[
                    0xff, 0xd8, 0xff, 0xc0, 0x00, 0x07, 0x08, 0x00, 0x01, 0x00, 0x01,
                ])
                .expect("write image entry");
            archive.finish().expect("finish PPTX ZIP");
        }
        output.into_inner()
    }

    fn write_entry(
        archive: &mut zip::ZipWriter<&mut Cursor<Vec<u8>>>,
        options: SimpleFileOptions,
        path: &str,
        content: &str,
    ) {
        archive.start_file(path, options).expect("start ZIP entry");
        archive
            .write_all(content.as_bytes())
            .expect("write ZIP entry");
    }
}

// ---------------------------------------------------------------------------
// 차트
//
// 차트는 슬라이드가 아니라 ppt/charts/chartN.xml 에 따로 들어 있고, 엑셀이
// 그리는 그림이라 모양 정보가 거의 없다. 숫자와 계열 이름만 있고 막대
// 하나하나는 없다. 그래서 여기서 직접 그린다 — 안 그리면 슬라이드 한가운데가
// 통째로 빈다.
// ---------------------------------------------------------------------------

/// 차트 한 계열.
struct ChartSeries {
    name: String,
    color: Option<PdfColor>,
    values: Vec<f32>,
}

/// 그릴 수 있는 차트 종류.
#[derive(Clone, Copy, PartialEq)]
enum ChartKind {
    Column,
    Bar,
    Line,
    /// 면을 칠하는 꺾은선. 누적이면 계열마다 띠가 된다.
    Area,
    Pie,
    /// 그릴 줄 모르는 종류. 자리만 잡고 왜 비었는지 적는다.
    Unsupported,
}

/// 막대를 어떻게 쌓는가.
///
/// 누적을 묶은 막대로 그리면 숫자의 뜻이 바뀐다 — 합계를 보여 주려고 만든
/// 그림이 비교 그림이 된다. 이 덱만 해도 막대 57개 중 35개가 누적이다.
#[derive(Clone, Copy, PartialEq)]
enum ChartGrouping {
    Clustered,
    Stacked,
    PercentStacked,
}

const CHART_AXIS: PdfColor = PdfColor {
    r: 0.52,
    g: 0.52,
    b: 0.52,
};
const CHART_GRID: PdfColor = PdfColor {
    r: 0.85,
    g: 0.85,
    b: 0.85,
};

fn read_chart(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    graphic_data: Node<'_, '_>,
    relationships: &HashMap<String, Relationship>,
    theme: &HashMap<String, PdfColor>,
    bounds: PptxRect,
    elements: &mut Vec<PptxElement>,
) -> Result<bool> {
    let Some(reference) = graphic_data
        .descendants()
        .find(|node| is_element(*node, "chart"))
    else {
        return Ok(false);
    };
    let Some(id) = reference
        .attributes()
        .find(|attribute| attribute.name() == "id")
        .map(|attribute| attribute.value())
    else {
        return Ok(false);
    };
    let Some(relationship) = relationships.get(id).filter(|rel| !rel.external) else {
        return Ok(false);
    };
    let chart_path = resolve_part_target(
        &relationship_source_path(relationship),
        &relationship.target,
    );
    let Some(bytes) = read_zip_bytes(archive, &chart_path)? else {
        return Ok(false);
    };
    let text = String::from_utf8(bytes)
        .map_err(|_| MiniPdfError::InvalidInput(format!("{chart_path} is not UTF-8")))?;
    let document = Document::parse(&text)?;

    let Some(plot_area) = document
        .descendants()
        .find(|node| is_element(*node, "plotArea"))
    else {
        return Ok(false);
    };
    let Some(plot) = plot_area.children().filter(Node::is_element).find_map(|node| {
        let kind = match node.tag_name().name() {
            "barChart" | "bar3DChart" => {
                match child(node, "barDir").and_then(|dir| dir.attribute("val")) {
                    Some("bar") => ChartKind::Bar,
                    _ => ChartKind::Column,
                }
            }
            "lineChart" | "line3DChart" => ChartKind::Line,
            "areaChart" | "area3DChart" => ChartKind::Area,
            "pieChart" | "pie3DChart" | "doughnutChart" => ChartKind::Pie,
            // 분산형은 x 도 숫자라 범주 축이 없다. 선으로 그리면 점의 가로
            // 자리가 거짓이 되므로, 그리지 않고 그렇다고 적는다.
            "scatterChart" | "bubbleChart" | "radarChart" | "stockChart"
            | "surfaceChart" | "ofPieChart" => ChartKind::Unsupported,
            _ => return None,
        };
        Some((kind, node))
    }) else {
        return Ok(false);
    };
    let (kind, plot_node) = plot;
    let grouping = match child(plot_node, "grouping").and_then(|node| node.attribute("val")) {
        Some("stacked") => ChartGrouping::Stacked,
        Some("percentStacked") => ChartGrouping::PercentStacked,
        _ => ChartGrouping::Clustered,
    };
    let kind_label = plot_node.tag_name().name().to_owned();

    let title = if document
        .descendants()
        .any(|node| is_element(node, "autoTitleDeleted") && node.attribute("val") == Some("1"))
    {
        None
    } else {
        document
            .descendants()
            .find(|node| is_element(*node, "title"))
            .map(collect_text)
            .filter(|text| !text.is_empty())
    };
    let has_legend = document
        .descendants()
        .any(|node| is_element(node, "legend"));

    // 미지원 종류는 계열을 읽기 전에 끝낸다. 분산형은 c:val 이 아니라
    // c:xVal/c:yVal 이라 계열이 비어 보이고, 그대로 두면 아무것도 안 그린 채
    // 조용히 돌아가 슬라이드 한가운데가 빈다.
    if kind == ChartKind::Unsupported {
        CHARTS_SKIPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        draw_unsupported_chart(bounds, title.as_deref(), &kind_label, elements);
        return Ok(true);
    }

    let mut categories: Vec<String> = Vec::new();
    let mut series = Vec::new();
    for node in plot_node.children().filter(|node| is_element(*node, "ser")) {
        if categories.is_empty() {
            categories = child(node, "cat")
                .map(|cat| read_cache_strings(cat))
                .unwrap_or_default();
        }
        let values = child(node, "val")
            .map(|val| read_cache_numbers(val))
            .unwrap_or_default();
        if values.is_empty() {
            continue;
        }
        let name = child(node, "tx")
            .map(|tx| read_cache_strings(tx))
            .and_then(|names| names.into_iter().next())
            .unwrap_or_default();
        series.push(ChartSeries {
            name,
            color: child(node, "spPr").and_then(|node| read_fill(node, theme)),
            values,
        });
    }
    if series.is_empty() {
        return Ok(false);
    }
    if categories.is_empty() {
        categories = (1..=series[0].values.len())
            .map(|index| index.to_string())
            .collect();
    }


    CHARTS_DRAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    draw_chart(
        kind,
        grouping,
        bounds,
        title.as_deref(),
        has_legend,
        &categories,
        &series,
        theme,
        elements,
    );
    Ok(true)
}

/// c:strCache 나 c:numCache 안의 값들을 idx 순서대로.
fn read_cache_strings(node: Node<'_, '_>) -> Vec<String> {
    let mut points: Vec<(usize, String)> = node
        .descendants()
        .filter(|node| is_element(*node, "pt"))
        .map(|point| {
            let index = point
                .attribute("idx")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            (index, collect_text(point))
        })
        .collect();
    if points.is_empty() {
        let direct = collect_text(node);
        return if direct.is_empty() {
            Vec::new()
        } else {
            vec![direct]
        };
    }
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).collect()
}

fn read_cache_numbers(node: Node<'_, '_>) -> Vec<f32> {
    let mut points: Vec<(usize, f32)> = node
        .descendants()
        .filter(|node| is_element(*node, "pt"))
        .filter_map(|point| {
            let index = point
                .attribute("idx")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            collect_text(point).trim().parse().ok().map(|v| (index, v))
        })
        .collect();
    points.sort_by_key(|(index, _)| *index);
    points.into_iter().map(|(_, value)| value).collect()
}

fn collect_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|node| node.is_text())
        .filter(|node| {
            node.parent()
                .is_some_and(|parent| matches!(parent.tag_name().name(), "v" | "t"))
        })
        .filter_map(|node| node.text())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_owned()
}

/// 계열 색. 문서가 안 정했으면 테마의 강조색을 차례로 쓴다.
fn series_color(
    series: &ChartSeries,
    index: usize,
    theme: &HashMap<String, PdfColor>,
) -> PdfColor {
    if let Some(color) = series.color {
        return color;
    }
    let name = format!("accent{}", index % 6 + 1);
    theme.get(&name).copied().unwrap_or(PdfColor {
        r: 0.25,
        g: 0.45,
        b: 0.70,
    })
}

#[allow(clippy::too_many_arguments)]
/// 그릴 줄 모르는 차트는 자리만 잡고 왜 비었는지 적는다.
///
/// 엉뚱한 모양으로 그리는 것보다 낫다 — 숫자를 잘못 보여 주는 것이 뷰어가
/// 할 수 있는 가장 나쁜 일이다.
fn draw_unsupported_chart(
    bounds: PptxRect,
    title: Option<&str>,
    kind: &str,
    elements: &mut Vec<PptxElement>,
) {
    let frame = PdfColor {
        r: 0.78,
        g: 0.78,
        b: 0.78,
    };
    for (start, end) in [
        ((bounds.x, bounds.y), (bounds.x + bounds.width, bounds.y)),
        (
            (bounds.x + bounds.width, bounds.y),
            (bounds.x + bounds.width, bounds.y + bounds.height),
        ),
        (
            (bounds.x + bounds.width, bounds.y + bounds.height),
            (bounds.x, bounds.y + bounds.height),
        ),
        ((bounds.x, bounds.y + bounds.height), (bounds.x, bounds.y)),
    ] {
        push_line(start.0, start.1, end.0, end.1, frame, elements);
    }
    let label = match title {
        Some(title) if !title.is_empty() => format!("{title} — 그리지 못하는 차트 ({kind})"),
        _ => format!("그리지 못하는 차트 ({kind})"),
    };
    push_label(
        PptxRect {
            x: bounds.x,
            y: bounds.y + bounds.height / 2.0 - 8.0,
            width: bounds.width,
            height: 16.0,
        },
        &label,
        9.0,
        false,
        TextAlignment::Center,
        CHART_AXIS,
        elements,
    );
}

fn draw_chart(
    kind: ChartKind,
    grouping: ChartGrouping,
    bounds: PptxRect,
    title: Option<&str>,
    has_legend: bool,
    categories: &[String],
    series: &[ChartSeries],
    theme: &HashMap<String, PdfColor>,
    elements: &mut Vec<PptxElement>,
) {
    let mut top = bounds.y;
    let mut bottom = bounds.y + bounds.height;

    if let Some(title) = title {
        push_label(
            PptxRect {
                x: bounds.x,
                y: top,
                width: bounds.width,
                height: 22.0,
            },
            title,
            13.0,
            true,
            TextAlignment::Center,
            PdfColor::BLACK,
            elements,
        );
        top += 26.0;
    }
    if has_legend && !series.is_empty() {
        bottom -= 18.0;
        // 원 그래프는 계열이 하나뿐이라 계열 이름으로는 범례가 되지
        // 않는다. 파워포인트처럼 조각 이름을 늘어놓는다.
        let entries: Vec<(String, PdfColor)> = if kind == ChartKind::Pie {
            categories
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let color = theme
                        .get(&format!("accent{}", index % 6 + 1))
                        .copied()
                        .unwrap_or(PdfColor::BLACK);
                    (name.clone(), color)
                })
                .collect()
        } else {
            series
                .iter()
                .enumerate()
                .map(|(index, one)| (one.name.clone(), series_color(one, index, theme)))
                .collect()
        };
        draw_legend(
            PptxRect {
                x: bounds.x,
                y: bottom,
                width: bounds.width,
                height: 16.0,
            },
            &entries,
            elements,
        );
    }

    if kind == ChartKind::Pie {
        draw_pie(
            PptxRect {
                x: bounds.x,
                y: top,
                width: bounds.width,
                height: (bottom - top).max(1.0),
            },
            categories,
            &series[0],
            theme,
            elements,
        );
        return;
    }

    // 값 축에 자리를 내준다. 범주 이름은 가로 막대일 때 왼쪽으로 간다.
    let gutter = 44.0_f32.min(bounds.width * 0.3);
    let category_band = 16.0_f32.min((bottom - top) * 0.25);
    let plot = PptxRect {
        x: bounds.x + gutter,
        y: top,
        width: (bounds.width - gutter - 8.0).max(1.0),
        height: (bottom - top - category_band).max(1.0),
    };

    let (low, high) = value_range(series, grouping);
    let ticks = 4;
    let tick_step = (high - low) / ticks as f32;

    // 눈금선과 값.
    for step in 0..=ticks {
        let value = low + (high - low) * step as f32 / ticks as f32;
        if kind == ChartKind::Bar {
            let x = plot.x + plot.width * step as f32 / ticks as f32;
            push_line(x, plot.y, x, plot.y + plot.height, CHART_GRID, elements);
            push_label(
                PptxRect {
                    x: x - 24.0,
                    y: plot.y + plot.height + 2.0,
                    width: 48.0,
                    height: category_band,
                },
                &format_tick(value, tick_step),
                8.0,
                false,
                TextAlignment::Center,
                CHART_AXIS,
                elements,
            );
        } else {
            let y = plot.y + plot.height * (1.0 - step as f32 / ticks as f32);
            push_line(plot.x, y, plot.x + plot.width, y, CHART_GRID, elements);
            push_label(
                PptxRect {
                    x: bounds.x,
                    y: y - 6.0,
                    width: gutter - 4.0,
                    height: 12.0,
                },
                &format_tick(value, tick_step),
                8.0,
                false,
                TextAlignment::Right,
                CHART_AXIS,
                elements,
            );
        }
    }

    let span = (high - low).max(f32::EPSILON);
    let zero_offset = ((0.0 - low) / span).clamp(0.0, 1.0);
    let count = categories
        .len()
        .max(series.iter().map(|s| s.values.len()).max().unwrap_or(1))
        .max(1);
    let slot = if kind == ChartKind::Bar {
        plot.height / count as f32
    } else {
        plot.width / count as f32
    };
    let stacked = grouping != ChartGrouping::Clustered;
    // 누적이면 계열마다 자리를 나눠 갖지 않고 한 줄에 겹쳐 쌓는다.
    let lanes = if stacked { 1 } else { series.len().max(1) };
    let thickness = (slot * 0.72 / lanes as f32).max(1.0);

    // 퍼센트 누적은 범주마다 합이 100이 되도록 고쳐 읽는다.
    let totals: Vec<f32> = (0..count)
        .map(|index| {
            series
                .iter()
                .filter_map(|one| one.values.get(index))
                .map(|value| value.abs())
                .sum::<f32>()
        })
        .collect();

    // 범주마다 지금까지 쌓은 높이 (위로, 아래로 따로).
    let mut stack_up = vec![0.0_f32; count];
    let mut stack_down = vec![0.0_f32; count];

    for (series_index, one) in series.iter().enumerate() {
        let color = series_color(one, series_index, theme);
        let mut previous: Option<(f32, f32)> = None;
        // 영역 차트는 윗변과 밑변을 모아 두었다가 띠 하나로 칠한다.
        // 선만 그으면 누적의 둘째 계열부터 선의 높이를 그 계열의 값으로
        // 잘못 읽게 된다 — 실제로는 아래 계열까지 더한 값이다.
        let mut tops: Vec<(f32, f32)> = Vec::new();
        let mut bases: Vec<(f32, f32)> = Vec::new();
        for (category_index, raw) in one.values.iter().enumerate() {
            let value = if grouping == ChartGrouping::PercentStacked {
                match totals.get(category_index) {
                    Some(total) if *total > 0.0 => raw / total * 100.0,
                    _ => 0.0,
                }
            } else {
                *raw
            };
            let value = &value;
            // 누적이면 이 토막은 앞까지 쌓인 데서 시작해 제 값만큼 더 간다.
            let (base_value, tip_value) = if stacked {
                let stack = if *value >= 0.0 {
                    &mut stack_up[category_index.min(count - 1)]
                } else {
                    &mut stack_down[category_index.min(count - 1)]
                };
                let base = *stack;
                *stack += *value;
                (base, *stack)
            } else {
                (0.0_f32.max(low), *value)
            };
            let fraction = ((tip_value - low) / span).clamp(0.0, 1.0);
            let base_fraction = ((base_value - low) / span).clamp(0.0, 1.0);
            match kind {
                ChartKind::Line => {
                    let x = plot.x + slot * (category_index as f32 + 0.5);
                    let y = plot.y + plot.height * (1.0 - fraction);
                    if let Some((px, py)) = previous {
                        push_line_wide(px, py, x, y, color, 1.8, elements);
                    }
                    previous = Some((x, y));
                }
                ChartKind::Area => {
                    let x = plot.x + slot * (category_index as f32 + 0.5);
                    tops.push((x, plot.y + plot.height * (1.0 - fraction)));
                    bases.push((x, plot.y + plot.height * (1.0 - base_fraction)));
                }
                ChartKind::Bar => {
                    let start = plot.x
                        + plot.width * if stacked { base_fraction } else { zero_offset };
                    let end = plot.x + plot.width * fraction;
                    let lane = if stacked { 0 } else { series_index };
                    let y = plot.y
                        + slot * category_index as f32
                        + slot * 0.14
                        + thickness * lane as f32;
                    push_block(
                        start.min(end),
                        y,
                        (end - start).abs().max(0.5),
                        thickness,
                        color,
                        elements,
                    );
                }
                _ => {
                    let base = plot.y
                        + plot.height
                            * (1.0 - if stacked { base_fraction } else { zero_offset });
                    let tip = plot.y + plot.height * (1.0 - fraction);
                    let lane = if stacked { 0 } else { series_index };
                    let x = plot.x
                        + slot * category_index as f32
                        + slot * 0.14
                        + thickness * lane as f32;
                    push_block(
                        x,
                        base.min(tip),
                        thickness,
                        (base - tip).abs().max(0.5),
                        color,
                        elements,
                    );
                }
            }
        }

        if kind == ChartKind::Area && tops.len() >= 2 {
            let mut polygon = tops.clone();
            polygon.extend(bases.iter().rev().copied());
            elements.push(PptxElement::Shape(PptxShape {
                shape_type: "rect".to_owned(),
                bounds: plot,
                fill: Some(color),
                outline: None,
                paragraphs: Vec::new(),
                text_body: PptxTextBody::default(),
                polygon: Some(polygon),
            }));
            // 윗변을 한 번 더 그어 띠끼리 맞닿는 자리를 분명히 한다.
            for pair in tops.windows(2) {
                push_line_wide(pair[0].0, pair[0].1, pair[1].0, pair[1].1, color, 1.2, elements);
            }
        }
    }

    // 축선.
    push_line(
        plot.x,
        plot.y + plot.height,
        plot.x + plot.width,
        plot.y + plot.height,
        CHART_AXIS,
        elements,
    );
    push_line(plot.x, plot.y, plot.x, plot.y + plot.height, CHART_AXIS, elements);

    // 범주 이름.
    for (index, name) in categories.iter().enumerate() {
        if kind == ChartKind::Bar {
            push_label(
                PptxRect {
                    x: bounds.x,
                    y: plot.y + slot * (index as f32 + 0.5) - 6.0,
                    width: gutter - 4.0,
                    height: 12.0,
                },
                name,
                8.0,
                false,
                TextAlignment::Right,
                CHART_AXIS,
                elements,
            );
        } else {
            push_label(
                PptxRect {
                    x: plot.x + slot * index as f32,
                    y: plot.y + plot.height + 2.0,
                    width: slot,
                    height: category_band,
                },
                name,
                8.0,
                false,
                TextAlignment::Center,
                CHART_AXIS,
                elements,
            );
        }
    }
}

/// 0 을 포함하는 보기 좋은 값 범위.
///
/// 누적 막대는 범주마다 쌓인 합계가 축의 끝이다. 계열 하나의 최대값으로
/// 잡으면 막대가 그림 밖으로 나간다.
fn value_range(series: &[ChartSeries], grouping: ChartGrouping) -> (f32, f32) {
    let mut low = 0.0_f32;
    let mut high = 0.0_f32;
    match grouping {
        ChartGrouping::PercentStacked => return (0.0, 100.0),
        ChartGrouping::Stacked => {
            let count = series.iter().map(|one| one.values.len()).max().unwrap_or(0);
            for index in 0..count {
                let (mut up, mut down) = (0.0_f32, 0.0_f32);
                for one in series {
                    match one.values.get(index) {
                        Some(value) if *value >= 0.0 => up += value,
                        Some(value) => down += value,
                        None => {}
                    }
                }
                high = high.max(up);
                low = low.min(down);
            }
        }
        ChartGrouping::Clustered => {
            for one in series {
                for value in &one.values {
                    low = low.min(*value);
                    high = high.max(*value);
                }
            }
        }
    }
    if (high - low).abs() < f32::EPSILON {
        high = low + 1.0;
    }
    // 위쪽을 한 자리 올려 막대가 천장에 닿지 않게 한다.
    let step = nice_step((high - low) / 4.0);
    let high = (high / step).ceil() * step;
    let low = if low < 0.0 {
        (low / step).floor() * step
    } else {
        0.0
    };
    (low, high.max(low + step))
}

fn nice_step(rough: f32) -> f32 {
    if rough <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f32.powf(rough.log10().floor());
    let normalized = rough / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

/// 눈금 숫자를 그 간격에 맞는 자릿수로 적는다.
///
/// 한 자리로 고정하면 0.25 가 0.2 로 적혀 눈금이 고르지 않아 보인다.
fn format_tick(value: f32, step: f32) -> String {
    if (value - value.round()).abs() < step.abs() * 0.001 {
        return format!("{}", value.round() as i64);
    }
    let digits = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else if step >= 0.01 {
        2
    } else {
        3
    };
    format!("{value:.digits$}")
}

fn draw_legend(
    bounds: PptxRect,
    entries: &[(String, PdfColor)],
    elements: &mut Vec<PptxElement>,
) {
    if entries.is_empty() {
        return;
    }
    let slot = bounds.width / entries.len() as f32;
    for (index, (name, color)) in entries.iter().enumerate() {
        let left = bounds.x + slot * index as f32;
        push_block(left + 4.0, bounds.y + 4.0, 8.0, 8.0, *color, elements);
        push_label(
            PptxRect {
                x: left + 16.0,
                y: bounds.y,
                width: (slot - 18.0).max(1.0),
                height: bounds.height,
            },
            name,
            8.0,
            false,
            TextAlignment::Left,
            PdfColor::BLACK,
            elements,
        );
    }
}

fn draw_pie(
    bounds: PptxRect,
    categories: &[String],
    series: &ChartSeries,
    theme: &HashMap<String, PdfColor>,
    elements: &mut Vec<PptxElement>,
) {
    let total: f32 = series.values.iter().map(|value| value.abs()).sum();
    if total <= 0.0 {
        return;
    }
    let radius = (bounds.width.min(bounds.height) / 2.0 - 6.0).max(1.0);
    let center = (bounds.x + bounds.width / 2.0, bounds.y + bounds.height / 2.0);
    let mut start = -std::f32::consts::FRAC_PI_2;

    for (index, value) in series.values.iter().enumerate() {
        let sweep = std::f32::consts::TAU * value.abs() / total;
        let steps = ((sweep / 0.2).ceil() as usize).max(2);
        let mut points = vec![center];
        for step in 0..=steps {
            let angle = start + sweep * step as f32 / steps as f32;
            points.push((
                center.0 + radius * angle.cos(),
                center.1 + radius * angle.sin(),
            ));
        }
        let color = theme
            .get(&format!("accent{}", index % 6 + 1))
            .copied()
            .unwrap_or(PdfColor::BLACK);
        elements.push(PptxElement::Shape(PptxShape {
            shape_type: "rect".to_owned(),
            bounds: PptxRect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
            },
            fill: Some(color),
            outline: None,
            paragraphs: Vec::new(),
            text_body: PptxTextBody::default(),
            polygon: Some(points),
        }));

        // 조각 이름은 바깥쪽에 적는다.
        if let Some(name) = categories.get(index) {
            let middle = start + sweep / 2.0;
            let label_radius = radius * 0.72;
            push_label(
                PptxRect {
                    x: center.0 + label_radius * middle.cos() - 30.0,
                    y: center.1 + label_radius * middle.sin() - 6.0,
                    width: 60.0,
                    height: 12.0,
                },
                name,
                8.0,
                false,
                TextAlignment::Center,
                PdfColor::WHITE,
                elements,
            );
        }
        start += sweep;
    }
}

fn push_block(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: PdfColor,
    elements: &mut Vec<PptxElement>,
) {
    elements.push(PptxElement::Shape(PptxShape {
        shape_type: "rect".to_owned(),
        bounds: PptxRect {
            x,
            y,
            width,
            height,
        },
        fill: Some(color),
        outline: None,
        paragraphs: Vec::new(),
        text_body: PptxTextBody::default(),
        polygon: None,
    }));
}

fn push_line(x1: f32, y1: f32, x2: f32, y2: f32, color: PdfColor, elements: &mut Vec<PptxElement>) {
    push_line_wide(x1, y1, x2, y2, color, 0.75, elements);
}

fn push_line_wide(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: PdfColor,
    width: f32,
    elements: &mut Vec<PptxElement>,
) {
    elements.push(PptxElement::Line(PptxLine {
        x1,
        y1,
        x2,
        y2,
        outline: PptxOutline {
            color,
            width,
            dash_pattern: Vec::new(),
        },
        bends: Vec::new(),
        heads: (None, None),
    }));
}

#[allow(clippy::too_many_arguments)]
fn push_label(
    bounds: PptxRect,
    text: &str,
    size: f32,
    bold: bool,
    alignment: TextAlignment,
    color: PdfColor,
    elements: &mut Vec<PptxElement>,
) {
    if text.is_empty() {
        return;
    }
    elements.push(PptxElement::Shape(PptxShape {
        shape_type: "rect".to_owned(),
        bounds,
        fill: None,
        outline: None,
        paragraphs: vec![PptxParagraph {
            runs: vec![PptxRun {
                text: text.to_owned(),
                font_size: size,
                color,
                bold,
                italic: false,
                underline: false,
                font_name: None,
            }],
            alignment,
            margin_left: 0.0,
            indent: 0.0,
            space_before: 0.0,
            line_spacing: 1.0,
        }],
        text_body: PptxTextBody {
            left_inset: 0.0,
            top_inset: 0.0,
            right_inset: 0.0,
            bottom_inset: 0.0,
            anchor: VerticalAnchor::Middle,
        },
        polygon: None,
    }));
}
