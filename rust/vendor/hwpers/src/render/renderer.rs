use crate::model::ParaShape;
use crate::render::layout::{
    LayoutEngine, RenderedLine, RenderedPage, RenderedParagraph, TextRun,
};
use crate::HwpDocument;

/// Rendering options
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub dpi: u32,             // Output DPI (default: 96)
    pub scale: f32,           // Scale factor (default: 1.0)
    pub show_margins: bool,   // Show page margins
    pub show_baselines: bool, // Show text baselines
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            dpi: 96,
            scale: 1.0,
            show_margins: false,
            show_baselines: false,
        }
    }
}

/// HWP document renderer
pub struct HwpRenderer<'a> {
    document: &'a HwpDocument,
    options: RenderOptions,
}

impl<'a> HwpRenderer<'a> {
    pub fn new(document: &'a HwpDocument, options: RenderOptions) -> Self {
        Self { document, options }
    }

    /// Render document to visual layout description
    pub fn render(&self) -> RenderResult {
        let layout_engine = LayoutEngine::new(self.document);
        let layout = layout_engine.calculate_layout();

        let mut pages = Vec::new();

        for layout_page in layout.pages {
            pages.push(self.render_page(&layout_page));
        }

        RenderResult { pages }
    }

    /// Render a single page
    fn render_page(&self, page: &RenderedPage) -> RenderedPageOutput {
        let mut elements = Vec::new();

        // Convert page dimensions from HWP units to pixels
        let width_px = self.hwp_to_px(page.width as i32);
        let height_px = self.hwp_to_px(page.height as i32);

        // Render each paragraph
        for para in &page.paragraphs {
            elements.extend(self.render_paragraph(para));
        }

        // 표는 칸 테두리를 먼저 긋고 그 위에 글을 올린다.
        for table in &page.tables {
            for cell in &table.cells {
                elements.push(RenderElement::Rectangle {
                    x: self.hwp_to_px(cell.x),
                    y: self.hwp_to_px(cell.y),
                    width: self.hwp_to_px(cell.width),
                    height: self.hwp_to_px(cell.height),
                    fill_color: None,
                    stroke_color: Some(0x000000),
                    stroke_width: 1.0,
                });
                for para in &cell.paragraphs {
                    elements.extend(self.render_paragraph(para));
                }
            }
        }

        RenderedPageOutput {
            width: width_px,
            height: height_px,
            elements,
            page_number: page.page_number,
        }
    }

    /// Render a paragraph
    fn render_paragraph(&self, para: &RenderedParagraph) -> Vec<RenderElement> {
        let mut elements = Vec::new();

        // Get paragraph shape for styling
        let para_shape = self.document.get_para_shape(para.para_shape_id as usize);

        // Render each line
        for line in &para.lines {
            elements.extend(self.render_line(line, para_shape));
        }

        elements
    }

    /// Render a line of text
    fn render_line(
        &self,
        line: &RenderedLine,
        _para_shape: Option<&ParaShape>,
    ) -> Vec<RenderElement> {
        let mut elements = Vec::new();

        // Show baseline if requested
        if self.options.show_baselines {
            elements.push(RenderElement::Line {
                x1: self.hwp_to_px(line.runs.first().map(|r| r.x).unwrap_or(0)),
                y1: self.hwp_to_px(line.baseline_y),
                x2: self.hwp_to_px(line.runs.last().map(|r| r.x + r.width).unwrap_or(0)),
                y2: self.hwp_to_px(line.baseline_y),
                color: 0xFF0000, // Red
                width: 1.0,
            });
        }

        // Render each text run
        for run in &line.runs {
            if let Some(element) = self.render_text_run(run, line.baseline_y) {
                elements.push(element);
            }
        }

        elements
    }

    /// Render a text run
    fn render_text_run(&self, run: &TextRun, baseline_y: i32) -> Option<RenderElement> {
        // 글자 모양이나 글꼴 이름을 못 찾아도 글은 내보낸다. 없는 번호를
        // 가리키는 문단 하나 때문에 쪽 전체가 비는 일이 있었다.
        let char_shape = self.document.get_char_shape(run.char_shape_id as usize);
        let font_family = char_shape
            .and_then(|shape| self.document.get_face_name(shape.face_name_ids[0] as usize))
            .map(|face| face.font_name.clone())
            .unwrap_or_else(|| "함초롬바탕".to_owned());

        Some(RenderElement::Text {
            x: self.hwp_to_px(run.x),
            y: self.hwp_to_px(baseline_y),
            text: run.text.clone(),
            font_family,
            font_size: self.hwp_to_pt(run.font_size),
            color: char_shape.map(|shape| shape.text_color).unwrap_or(0),
            bold: char_shape.is_some_and(|shape| shape.is_bold()),
            italic: char_shape.is_some_and(|shape| shape.is_italic()),
            underline: char_shape.is_some_and(|shape| shape.is_underline()),
        })
    }

    /// Convert HWP units to pixels
    fn hwp_to_px(&self, hwp_units: i32) -> i32 {
        // HWP unit = 1/7200 inch
        // pixels = (hwp_units / 7200) * dpi * scale
        let inches = hwp_units as f32 / 7200.0;
        (inches * self.options.dpi as f32 * self.options.scale) as i32
    }

    /// Convert HWP units to points
    fn hwp_to_pt(&self, hwp_units: i32) -> f32 {
        // Points = (hwp_units / 7200) * 72
        (hwp_units as f32 / 100.0) * self.options.scale
    }
}

/// Render result containing all pages
#[derive(Debug)]
pub struct RenderResult {
    pub pages: Vec<RenderedPageOutput>,
}

/// Rendered page output
#[derive(Debug)]
pub struct RenderedPageOutput {
    pub width: i32,
    pub height: i32,
    pub elements: Vec<RenderElement>,
    pub page_number: u32,
}

/// Individual render element
#[derive(Debug, Clone)]
pub enum RenderElement {
    Text {
        x: i32,
        y: i32,
        text: String,
        font_family: String,
        font_size: f32,
        color: u32,
        bold: bool,
        italic: bool,
        underline: bool,
    },
    Line {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
        width: f32,
    },
    Rectangle {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        fill_color: Option<u32>,
        stroke_color: Option<u32>,
        stroke_width: f32,
    },
    Image {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        data: Vec<u8>,
    },
}

impl RenderResult {
    /// Export to SVG format
    pub fn to_svg(&self, page_index: usize) -> Option<String> {
        let page = self.pages.get(page_index)?;

        let mut svg = format!(
            r#"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">"#,
            page.width, page.height
        );

        // White background
        svg.push_str(&format!(
            r#"<rect width="{}" height="{}" fill="white"/>"#,
            page.width, page.height
        ));

        // Render elements
        for element in &page.elements {
            match element {
                RenderElement::Text {
                    x,
                    y,
                    text,
                    font_family,
                    font_size,
                    color,
                    bold,
                    italic,
                    underline,
                } => {
                    svg.push_str(&format!(
                        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" fill=\"#{:06X}\"",
                        x, y, font_family, font_size, color & 0xFFFFFF
                    ));

                    if *bold {
                        svg.push_str(r#" font-weight="bold""#);
                    }
                    if *italic {
                        svg.push_str(r#" font-style="italic""#);
                    }
                    if *underline {
                        svg.push_str(r#" text-decoration="underline""#);
                    }

                    svg.push_str(&format!(">{}</text>", escape_xml(text)));
                }
                RenderElement::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    width,
                } => {
                    svg.push_str(&format!(
                        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{:06X}\" stroke-width=\"{}\"/>",
                        x1, y1, x2, y2, color & 0xFFFFFF, width
                    ));
                }
                RenderElement::Rectangle {
                    x,
                    y,
                    width,
                    height,
                    fill_color,
                    stroke_color,
                    stroke_width,
                } => {
                    svg.push_str(&format!(
                        r#"<rect x="{x}" y="{y}" width="{width}" height="{height}""#
                    ));

                    if let Some(fill) = fill_color {
                        svg.push_str(&format!(" fill=\"#{:06X}\"", fill & 0xFFFFFF));
                    } else {
                        svg.push_str(r#" fill="none""#);
                    }

                    if let Some(stroke) = stroke_color {
                        svg.push_str(&format!(
                            " stroke=\"#{:06X}\" stroke-width=\"{}\"",
                            stroke & 0xFFFFFF,
                            stroke_width
                        ));
                    }

                    svg.push_str("/>");
                }
                _ => {} // Skip other elements for now
            }
        }

        svg.push_str("</svg>");
        Some(svg)
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
