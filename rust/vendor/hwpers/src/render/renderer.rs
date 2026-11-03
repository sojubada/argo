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
                self.render_cell_frame(cell, &mut elements);
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
            // 한글 파일의 색은 COLORREF(0x00BBGGRR) 다. 그대로 쓰면 빨강이 파랑이 된다.
            color: char_shape.map(|shape| colorref_to_rgb(shape.text_color)).unwrap_or(0),
            bold: char_shape.is_some_and(|shape| shape.is_bold()),
            italic: char_shape.is_some_and(|shape| shape.is_italic()),
            underline: char_shape.is_some_and(|shape| shape.is_underline()),
        })
    }

    /// 칸 배경과 네 변의 테두리. 문서가 정한 BorderFill 을 따른다.
    /// 번호가 없거나 못 찾으면 예전처럼 검은 1px 네모를 긋는다.
    fn render_cell_frame(
        &self,
        cell: &crate::render::layout::RenderedCell,
        elements: &mut Vec<RenderElement>,
    ) {
        let x = self.hwp_to_px(cell.x);
        let y = self.hwp_to_px(cell.y);
        let w = self.hwp_to_px(cell.width);
        let h = self.hwp_to_px(cell.height);
        let bf = (cell.border_fill_id as usize)
            .checked_sub(1)
            .and_then(|i| self.document.get_border_fill(i));
        let Some(bf) = bf else {
            elements.push(RenderElement::Rectangle {
                x,
                y,
                width: w,
                height: h,
                fill_color: None,
                stroke_color: Some(0x000000),
                stroke_width: 1.0,
            });
            return;
        };

        // 단색 채우기 (bit0). 흰색·투명은 그리지 않는다.
        let fill = &bf.fill_info;
        if fill.fill_type & 1 != 0 && fill.back_color != 0xFFFFFFFF {
            let rgb = colorref_to_rgb(fill.back_color);
            if rgb != 0xFFFFFF {
                elements.push(RenderElement::Rectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                    fill_color: Some(rgb),
                    stroke_color: None,
                    stroke_width: 0.0,
                });
            }
        } else if fill.fill_type & 4 != 0 {
            // 그러데이션은 두 색의 가운데로 칠한다.
            if let Some(g) = &fill.gradient_info {
                let (a, b) = (colorref_to_rgb(g.start_color), colorref_to_rgb(g.end_color));
                let mid = (((a >> 16 & 0xFF) + (b >> 16 & 0xFF)) / 2) << 16
                    | (((a >> 8 & 0xFF) + (b >> 8 & 0xFF)) / 2) << 8
                    | ((a & 0xFF) + (b & 0xFF)) / 2;
                elements.push(RenderElement::Rectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                    fill_color: Some(mid),
                    stroke_color: None,
                    stroke_width: 0.0,
                });
            }
        }

        let sides = [
            (&bf.left, x, y, x, y + h),
            (&bf.right, x + w, y, x + w, y + h),
            (&bf.top, x, y, x + w, y),
            (&bf.bottom, x, y + h, x + w, y + h),
        ];
        for (line, x1, y1, x2, y2) in sides {
            if line.line_type == 0 {
                continue;
            }
            let width = self.border_width_px(line.thickness);
            let color = colorref_to_rgb(line.color);
            let dash = match line.line_type {
                2 => "6 3",
                3 => "1.5 2",
                4 => "6 2 1.5 2",
                5 => "6 2 1.5 2 1.5 2",
                6 => "10 4",
                7 => "0.5 2.5",
                _ => "",
            };
            if matches!(line.line_type, 8..=11) {
                // 이중·삼중선: 가는 선 두 줄로 흉내 낸다.
                let off = (width * 1.5).max(1.5) as i32;
                let (dx, dy) = if x1 == x2 { (off, 0) } else { (0, off) };
                for k in [-1, 1] {
                    elements.push(RenderElement::StyledLine {
                        x1: x1 + dx * k / 2,
                        y1: y1 + dy * k / 2,
                        x2: x2 + dx * k / 2,
                        y2: y2 + dy * k / 2,
                        color,
                        width: (width / 2.0).max(0.6),
                        dash: "",
                    });
                }
                continue;
            }
            elements.push(RenderElement::StyledLine {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
                dash,
            });
        }
    }

    /// 테두리 굵기 번호 → 화면 굵기. 번호마다 정해진 mm 값이 있다.
    fn border_width_px(&self, code: u8) -> f32 {
        const MM: [f32; 16] = [
            0.1, 0.12, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0,
        ];
        let mm = MM[(code as usize).min(15)];
        let px = mm / 25.4 * self.options.dpi as f32 * self.options.scale;
        px.max(0.5)
    }

    /// Convert HWP units to pixels
    fn hwp_to_px(&self, hwp_units: i32) -> i32 {
        // HWP unit = 1/7200 inch
        // pixels = (hwp_units / 7200) * dpi * scale
        let inches = hwp_units as f32 / 7200.0;
        (inches * self.options.dpi as f32 * self.options.scale) as i32
    }

    /// 글자 크기를 좌표와 같은 자로 잰다.
    ///
    /// 자리는 hwp_to_px 로 96dpi 픽셀에 찍는데 크기만 포인트로 주면
    /// 글자가 자기 자리보다 3분의 1쯤 작게 그려진다. 글자 사이가 벌어져
    /// 보이던 것이 이것이다 — 같은 dpi 로 환산한다.
    fn hwp_to_pt(&self, hwp_units: i32) -> f32 {
        let inches = hwp_units as f32 / 7200.0;
        inches * self.options.dpi as f32 * self.options.scale
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
    /// 점선·파선이 되는 선 (표 테두리). dash 는 SVG stroke-dasharray, 빈 글이면 실선.
    StyledLine {
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        color: u32,
        width: f32,
        dash: &'static str,
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
                        x, y, escape_xml(font_family), font_size, color & 0xFFFFFF
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
                RenderElement::StyledLine {
                    x1,
                    y1,
                    x2,
                    y2,
                    color,
                    width,
                    dash,
                } => {
                    svg.push_str(&format!(
                        "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#{:06X}\" stroke-width=\"{}\"",
                        x1, y1, x2, y2, color & 0xFFFFFF, width
                    ));
                    if !dash.is_empty() {
                        let scaled: Vec<String> = dash
                            .split(' ')
                            .filter_map(|v| v.parse::<f32>().ok())
                            .map(|v| format!("{}", v * width.max(1.0)))
                            .collect();
                        svg.push_str(&format!(" stroke-dasharray=\"{}\"", scaled.join(" ")));
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

/// XML 에 넣을 글. 한글 문서의 글에는 조판 부호(0x01~0x1F)가 섞여 나올 수 있는데,
/// 그 글자 하나가 SVG 쪽 전체를 못 읽게 만든다. XML 이 허락하지 않는 글자는 뺀다.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// 한글 COLORREF(0x00BBGGRR) → 0xRRGGBB.
fn colorref_to_rgb(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}
