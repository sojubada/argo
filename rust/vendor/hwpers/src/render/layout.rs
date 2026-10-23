use crate::model::{CharShape, PageDef, ParaLineSeg, ParaShape, Paragraph, Section};
use crate::HwpDocument;

/// Represents a single rendered line of text
#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub y: i32,          // Vertical position in HWP units
    pub height: i32,     // Line height in HWP units
    pub baseline_y: i32, // Baseline position
    pub runs: Vec<TextRun>,
}

/// Represents a run of text with consistent formatting
#[derive(Debug, Clone)]
pub struct TextRun {
    pub x: i32,             // Horizontal position in HWP units
    pub width: i32,         // Width of the run
    pub text: String,       // Text content
    pub char_shape_id: u16, // Character shape ID
    pub font_size: i32,     // Actual font size in HWP units
}

/// Represents a rendered paragraph
#[derive(Debug, Clone)]
pub struct RenderedParagraph {
    pub x: i32,                   // Left position
    pub y: i32,                   // Top position
    pub width: i32,               // Paragraph width
    pub height: i32,              // Total height
    pub lines: Vec<RenderedLine>, // Rendered lines
    pub para_shape_id: u16,       // Paragraph shape ID
}

/// 자리와 크기가 정해진 표 한 칸.
#[derive(Debug, Clone)]
pub struct RenderedCell {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// 칸 안의 글. 바깥 문단과 똑같이 배치된다.
    pub paragraphs: Vec<RenderedParagraph>,
}

/// 자리가 정해진 표 하나.
#[derive(Debug, Clone)]
pub struct RenderedTable {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub cells: Vec<RenderedCell>,
}

/// Represents a rendered page
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width: u32,                         // Page width
    pub height: u32,                        // Page height
    pub paragraphs: Vec<RenderedParagraph>, // Rendered paragraphs
    /// 이 쪽에 놓인 표들.
    pub tables: Vec<RenderedTable>,
    pub page_number: u32,                   // Page number
}

/// Layout calculation result
#[derive(Debug)]
pub struct LayoutResult {
    pub pages: Vec<RenderedPage>,
    pub total_height: i32,
}

/// Layout engine for HWP documents
pub struct LayoutEngine<'a> {
    document: &'a HwpDocument,
}

impl<'a> LayoutEngine<'a> {
    pub fn new(document: &'a HwpDocument) -> Self {
        Self { document }
    }

    /// Perform full layout calculation for the document
    pub fn calculate_layout(&self) -> LayoutResult {
        let mut pages = Vec::new();
        let mut current_page_num = 1;

        for section in self.document.sections() {
            let section_pages = self.layout_section(section, &mut current_page_num);
            pages.extend(section_pages);
        }

        let total_height = pages.iter().map(|p| p.height as i32).sum();

        LayoutResult {
            pages,
            total_height,
        }
    }

    /// Layout a single section
    fn layout_section(&self, section: &Section, page_num: &mut u32) -> Vec<RenderedPage> {
        let mut pages = Vec::new();

        // 쪽 정의가 없는 구역이 실제로 있다.
        //
        // 전에는 여기서 expect 로 패닉했고, 변환기가 패닉하면 앱이 통째로
        // 죽었다 — 한글 문서 하나가 파일 탐색기를 끌고 갔다. 문서가 말하지
        // 않으면 A4 로 가정한다. 추측이 틀려도 쪽 크기가 어긋날 뿐이고,
        // 아무것도 못 보는 것보다 낫다.
        let fallback = PageDef::default();
        let page_def = section.page_def.as_ref().unwrap_or(&fallback);

        let mut current_page = RenderedPage {
            width: page_def.width,
            height: page_def.height,
            paragraphs: Vec::new(),
            tables: Vec::new(),
            page_number: *page_num,
        };

        // Calculate content area
        let content_x = page_def.left_margin as i32;
        let content_y = page_def.top_margin as i32;
        let content_width = page_def.effective_width() as i32;
        let content_height = page_def.effective_height() as i32;

        let mut current_y = content_y;

        // Layout each paragraph
        for paragraph in &section.paragraphs {
            if let Some(table) = paragraph.table_data.as_ref() {
                let rendered = self.layout_table(table, content_x, current_y, content_width);
                if current_y + rendered.height > content_y + content_height
                    && current_y > content_y
                {
                    pages.push(current_page);
                    *page_num += 1;
                    current_page = RenderedPage {
                        width: page_def.width,
                        height: page_def.height,
                        paragraphs: Vec::new(),
                        tables: Vec::new(),
                        page_number: *page_num,
                    };
                    current_y = content_y;
                }
                let shift = current_y - rendered.y;
                let mut rendered = rendered;
                shift_table(&mut rendered, shift);
                current_y += rendered.height;
                current_page.tables.push(rendered);
                continue;
            }
            if let Some(rendered_para) =
                self.layout_paragraph(paragraph, content_x, current_y, content_width)
            {
                // Check if paragraph fits on current page
                if current_y + rendered_para.height > content_y + content_height {
                    // Start new page
                    pages.push(current_page);
                    *page_num += 1;

                    current_page = RenderedPage {
                        width: page_def.width,
                        height: page_def.height,
                        paragraphs: Vec::new(),
                        tables: Vec::new(),
                        page_number: *page_num,
                    };

                    current_y = content_y;
                }

                current_y += rendered_para.height;
                current_page.paragraphs.push(rendered_para);
            }
        }

        if !current_page.paragraphs.is_empty() || !current_page.tables.is_empty() {
            pages.push(current_page);
            *page_num += 1;
        }

        pages
    }

    /// 표 하나를 격자로 놓는다.
    ///
    /// 칸 크기는 문서가 이미 계산해 둔 값을 쓴다 — 한글이 정한 폭과 높이를
    /// 그대로 따르는 쪽이 우리가 다시 재는 것보다 원본에 가깝다. 표가
    /// 본문 폭보다 넓으면 전체를 같은 비율로 줄인다.
    fn layout_table(
        &self,
        table: &crate::model::control::Table,
        x: i32,
        y: i32,
        available_width: i32,
    ) -> RenderedTable {
        let cols = table.cols.max(1) as usize;
        let rows = table.rows.max(1) as usize;

        let mut col_widths = vec![0i32; cols];
        let mut row_heights = vec![0i32; rows];
        for cell in &table.cells {
            let (row, col) = (cell.cell_address.0 as usize, cell.cell_address.1 as usize);
            if cell.col_span <= 1 && col < cols {
                col_widths[col] = col_widths[col].max(cell.width as i32);
            }
            if cell.row_span <= 1 && row < rows {
                row_heights[row] = row_heights[row].max(cell.height as i32);
            }
        }
        // 병합 때문에 한 번도 안 잡힌 열과 행을 메운다.
        let known: Vec<i32> = col_widths.iter().copied().filter(|w| *w > 0).collect();
        let spare = if known.is_empty() {
            available_width / cols as i32
        } else {
            known.iter().sum::<i32>() / known.len() as i32
        };
        for width in col_widths.iter_mut().filter(|w| **w <= 0) {
            *width = spare;
        }
        let default_row = row_heights.iter().copied().max().unwrap_or(2000).max(600);
        for height in row_heights.iter_mut().filter(|h| **h <= 0) {
            *height = default_row;
        }

        let total_width: i32 = col_widths.iter().sum();
        if total_width > available_width && total_width > 0 {
            for width in col_widths.iter_mut() {
                *width = *width * available_width / total_width;
            }
        }

        let col_x: Vec<i32> = col_widths
            .iter()
            .scan(x, |acc, w| {
                let here = *acc;
                *acc += w;
                Some(here)
            })
            .collect();
        let row_y: Vec<i32> = row_heights
            .iter()
            .scan(y, |acc, h| {
                let here = *acc;
                *acc += h;
                Some(here)
            })
            .collect();

        let mut cells = Vec::with_capacity(table.cells.len());
        for cell in &table.cells {
            let (row, col) = (cell.cell_address.0 as usize, cell.cell_address.1 as usize);
            if row >= rows || col >= cols {
                continue;
            }
            let span_cols = (cell.col_span.max(1) as usize).min(cols - col);
            let span_rows = (cell.row_span.max(1) as usize).min(rows - row);
            let width: i32 = col_widths[col..col + span_cols].iter().sum();
            let height: i32 = row_heights[row..row + span_rows].iter().sum();

            let inner_x = col_x[col] + cell.left_margin as i32;
            let inner_width = (width - cell.left_margin as i32 - cell.right_margin as i32).max(1);
            let mut inner_y = row_y[row] + cell.top_margin as i32;

            let mut paragraphs = Vec::new();
            for para in &cell.paragraphs {
                if let Some(laid) = self.layout_paragraph(para, inner_x, inner_y, inner_width) {
                    inner_y += laid.height;
                    paragraphs.push(laid);
                }
            }

            cells.push(RenderedCell {
                x: col_x[col],
                y: row_y[row],
                width,
                height,
                paragraphs,
            });
        }

        RenderedTable {
            x,
            y,
            width: col_widths.iter().sum(),
            height: row_heights.iter().sum(),
            cells,
        }
    }

    /// Layout a single paragraph
    fn layout_paragraph(
        &self,
        paragraph: &Paragraph,
        x: i32,
        y: i32,
        width: i32,
    ) -> Option<RenderedParagraph> {
        let text = paragraph.text.as_ref()?.content.as_str();
        if text.is_empty() {
            return None;
        }

        // Get paragraph shape
        let fallback_shape = ParaShape::fallback();
        let para_shape = self
            .document
            .get_para_shape(paragraph.para_shape_id as usize)
            .unwrap_or(&fallback_shape);

        // Calculate paragraph margins
        let para_x = x + para_shape.left_margin;
        let para_width = width - para_shape.left_margin - para_shape.right_margin;

        // Check if paragraph has pre-calculated line segments
        if let Some(line_segs) = &paragraph.line_segments {
            // Use pre-calculated line layout
            let lines = self.render_with_line_segments(text, paragraph, line_segs, para_x, y);

            let total_height = line_segs.total_height();

            return Some(RenderedParagraph {
                x: para_x,
                y,
                width: para_width,
                height: total_height,
                lines,
                para_shape_id: paragraph.para_shape_id,
            });
        }

        // Otherwise, calculate line breaks dynamically
        let lines = self.calculate_line_breaks(text, paragraph, para_shape, para_x, y, para_width);

        let total_height = lines.iter().map(|l| l.height).sum();

        Some(RenderedParagraph {
            x: para_x,
            y,
            width: para_width,
            height: total_height,
            lines,
            para_shape_id: paragraph.para_shape_id,
        })
    }

    /// Render using pre-calculated line segments
    fn render_with_line_segments(
        &self,
        text: &str,
        paragraph: &Paragraph,
        line_segs: &ParaLineSeg,
        x: i32,
        y: i32,
    ) -> Vec<RenderedLine> {
        // 글자 위치는 UTF-16 단위다. 한글은 바이트로 자르면 글자 가운데가
        // 잘려 나가므로 미리 UTF-16 으로 펼쳐 놓고 센다.
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut lines = Vec::new();

        for (idx, segment) in line_segs.line_segments.iter().enumerate() {
            // 이 줄은 자기 시작 위치에서 다음 줄의 시작 위치까지다.
            //
            // 예전에는 앞 줄의 시작부터 이 줄의 시작까지로 잡았다. 그래서
            // 한 줄짜리 문단은 늘 0..0, 곧 빈 줄이 되어 글이 한 자도
            // 나오지 않았다.
            let start = (segment.text_start_position as usize).min(units.len());
            let end = line_segs
                .line_segments
                .get(idx + 1)
                .map(|next| next.text_start_position as usize)
                .unwrap_or(units.len())
                .clamp(start, units.len());

            let line_text = String::from_utf16_lossy(&units[start..end]);
            let line_text = line_text.trim_end_matches(['\r', '\n', '\u{0}']);
            let runs = self.create_text_runs_for_line(line_text, paragraph, x, start as u32);

            lines.push(RenderedLine {
                y: y + segment.line_vertical_position,
                height: segment.line_height,
                baseline_y: y
                    + segment.line_vertical_position
                    + segment.distance_baseline_to_line_vertical_position,
                runs,
            });
        }

        lines
    }

    /// Calculate line breaks dynamically
    fn calculate_line_breaks(
        &self,
        text: &str,
        paragraph: &Paragraph,
        para_shape: &ParaShape,
        x: i32,
        y: i32,
        _width: i32,
    ) -> Vec<RenderedLine> {
        let mut lines = Vec::new();
        let mut current_y = y + para_shape.top_para_space;

        // Get default character shape
        let default_char_shape_id = if let Some(char_shapes) = &paragraph.char_shapes {
            char_shapes
                .char_positions
                .first()
                .map(|cs| cs.char_shape_id)
                .unwrap_or(0)
        } else {
            0
        };

        let char_shape = self.document.get_char_shape(default_char_shape_id as usize);
        let line_height = self.calculate_line_height(para_shape, char_shape);

        // Simple line breaking - split by newlines for now
        for line_text in text.lines() {
            if line_text.is_empty() {
                current_y += line_height;
                continue;
            }

            let runs = self.create_text_runs_for_line(line_text, paragraph, x, 0);

            lines.push(RenderedLine {
                y: current_y,
                height: line_height,
                baseline_y: current_y + (line_height * 4 / 5), // Approximate baseline
                runs,
            });

            current_y += line_height;
        }

        lines
    }

    /// Create text runs for a line of text
    fn create_text_runs_for_line(
        &self,
        line_text: &str,
        paragraph: &Paragraph,
        x: i32,
        text_offset: u32,
    ) -> Vec<TextRun> {
        let mut runs = Vec::new();

        if let Some(char_shapes) = &paragraph.char_shapes {
            // Create runs based on character shape changes
            let mut current_x = x;

            for (idx, pos_shape) in char_shapes.char_positions.iter().enumerate() {
                let start = pos_shape.position as usize;
                let end = if idx + 1 < char_shapes.char_positions.len() {
                    char_shapes.char_positions[idx + 1].position as usize
                } else {
                    line_text.len() + text_offset as usize
                };

                if start >= text_offset as usize && (start - text_offset as usize) < line_text.len()
                {
                    let run_start = (start - text_offset as usize).min(line_text.len());
                    let run_end = (end - text_offset as usize).min(line_text.len());

                    if run_start < run_end {
                        let run_text = &line_text[run_start..run_end];
                        let char_shape = self
                            .document
                            .get_char_shape(pos_shape.char_shape_id as usize);

                        let width = self.calculate_text_width(run_text, char_shape);

                        runs.push(TextRun {
                            x: current_x,
                            width,
                            text: run_text.to_string(),
                            char_shape_id: pos_shape.char_shape_id,
                            font_size: char_shape.map(|cs| cs.base_size).unwrap_or(1000),
                        });

                        current_x += width;
                    }
                }
            }
        } else {
            // Single run for entire line
            let width = self.calculate_text_width(line_text, None);

            runs.push(TextRun {
                x,
                width,
                text: line_text.to_string(),
                char_shape_id: 0,
                font_size: 1000, // Default 10pt
            });
        }

        runs
    }

    /// Calculate line height based on paragraph and character shapes
    fn calculate_line_height(&self, para_shape: &ParaShape, char_shape: Option<&CharShape>) -> i32 {
        let base_size = char_shape.map(|cs| cs.base_size).unwrap_or(1000);

        match para_shape.line_space_type {
            0 => {
                // Percentage
                base_size * para_shape.line_space / 100
            }
            1 => {
                // Fixed value
                para_shape.line_space
            }
            2 => {
                // At least
                base_size.max(para_shape.line_space)
            }
            _ => base_size,
        }
    }

    /// Calculate text width (simplified)
    fn calculate_text_width(&self, text: &str, char_shape: Option<&CharShape>) -> i32 {
        let font_size = char_shape.map(|cs| cs.base_size).unwrap_or(1000);

        // Approximate width calculation
        // In reality, this would use font metrics
        let char_count = text.chars().count() as i32;
        let avg_char_width = font_size / 2; // Rough approximation

        char_count * avg_char_width
    }
}

/// 표와 그 안의 글을 세로로 통째로 옮긴다.
fn shift_table(table: &mut RenderedTable, dy: i32) {
    if dy == 0 {
        return;
    }
    table.y += dy;
    for cell in &mut table.cells {
        cell.y += dy;
        for para in &mut cell.paragraphs {
            para.y += dy;
            for line in &mut para.lines {
                line.y += dy;
                line.baseline_y += dy;
            }
        }
    }
}
