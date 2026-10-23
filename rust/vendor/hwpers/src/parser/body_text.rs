use crate::error::Result;
use crate::model::control::{Table, TableCell};
use crate::model::ctrl_header::ControlType;
use crate::model::{
    CtrlHeader, ListHeader, PageDef, ParaCharShape, ParaLineSeg, ParaText, Paragraph, Section,
};
use crate::parser::record::{HwpTag, Record};
use crate::reader::StreamReader;
use crate::utils::compression::decompress_stream;

pub struct BodyTextParser;

/// 여는 중인 표 하나.
///
/// HWP 는 표를 문단 안쪽에 접어 넣는다. CTRL_HEADER('tbl ') 가 열고, 그
/// 안에 TABLE 레코드가 격자 크기를, 이어지는 LIST_HEADER 가 셀 하나씩을
/// 적는다. 셀의 글은 그 LIST_HEADER 뒤에 오는 평범한 문단들이다. 레코드
/// 머리의 level 이 이 중첩을 말해 주므로, 그 깊이로 닫을 때를 안다.
struct OpenTable {
    /// CTRL_HEADER 의 깊이. 이보다 얕은 레코드가 오면 표가 끝난 것이다.
    ctrl_level: u8,
    /// 표를 품은 문단. 다 읽으면 여기에 표를 붙여서 돌려보낸다.
    owner: Option<Paragraph>,
    table: Table,
    cells: Vec<TableCell>,
    /// 지금 글을 받고 있는 셀.
    current_cell: Option<usize>,
}

impl BodyTextParser {
    pub fn parse(data: Vec<u8>, is_compressed: bool) -> Result<BodyText> {
        let data = if is_compressed {
            decompress_stream(&data)?
        } else {
            data
        };

        let mut reader = StreamReader::new(data);
        let mut section = Section::default();
        let mut current: Option<Paragraph> = None;
        let mut open_tables: Vec<OpenTable> = Vec::new();

        while reader.remaining() >= 4 {
            let record = match Record::parse(&mut reader) {
                Ok(r) => r,
                Err(_) => break,
            };
            let level = record.header.level;
            let tag = HwpTag::from_u16(record.tag_id());

            // 깊이가 얕아졌으면 그만큼 표를 닫는다.
            while open_tables
                .last()
                .is_some_and(|open| level <= open.ctrl_level)
            {
                current = close_table(&mut open_tables, &mut section, current);
            }

            match tag {
                Some(HwpTag::ParaHeader) => {
                    stash(&mut current, &mut open_tables, &mut section);
                    current = Some(Paragraph::from_header_record(&record).unwrap_or_default());
                }
                Some(HwpTag::ParaText) => {
                    if let Some(para) = current.as_mut() {
                        para.text = ParaText::from_record(&record).ok();
                    }
                }
                Some(HwpTag::ParaCharShape) => {
                    if let Some(para) = current.as_mut() {
                        para.char_shapes = ParaCharShape::from_record(&record).ok();
                    }
                }
                Some(HwpTag::ParaLineSeg) => {
                    if let Some(para) = current.as_mut() {
                        para.line_segments = ParaLineSeg::from_record(&record).ok();
                    }
                }
                Some(HwpTag::ParaRangeTag) => {
                    if let Some(para) = current.as_mut() {
                        if let Ok(link) = crate::model::hyperlink::Hyperlink::from_record(&record) {
                            para.hyperlinks.push(link);
                        }
                    }
                }
                Some(HwpTag::PageDef) => {
                    if section.page_def.is_none() {
                        section.page_def = PageDef::from_record(&record).ok();
                    }
                }
                Some(HwpTag::CtrlHeader) => {
                    let header = CtrlHeader::from_record(&record).ok();
                    if let (Some(header), Some(para)) = (header.as_ref(), current.as_mut()) {
                        para.ctrl_header = Some(header.clone());
                    }
                    if header
                        .as_ref()
                        .map(|h| h.get_control_type() == ControlType::Table)
                        .unwrap_or(false)
                    {
                        open_tables.push(OpenTable {
                            ctrl_level: level,
                            owner: current.take(),
                            table: Table::new_default(0, 0),
                            cells: Vec::new(),
                            current_cell: None,
                        });
                    }
                }
                Some(HwpTag::Table) => {
                    if let (Some(open), Ok(table)) =
                        (open_tables.last_mut(), Table::from_record(&record))
                    {
                        open.table = table;
                    }
                }
                Some(HwpTag::ListHeader) => {
                    if open_tables.last().is_some() {
                        // 표 안의 LIST_HEADER 는 셀 하나를 연다.
                        stash(&mut current, &mut open_tables, &mut section);
                        if let Ok(cell) = TableCell::from_list_header_record(&record) {
                            let open = open_tables.last_mut().expect("방금 확인했다");
                            open.cells.push(cell);
                            open.current_cell = Some(open.cells.len() - 1);
                        }
                    } else if let Some(para) = current.as_mut() {
                        para.list_header = ListHeader::from_record(&record).ok();
                    }
                }
                _ => {}
            }
        }

        stash(&mut current, &mut open_tables, &mut section);
        while !open_tables.is_empty() {
            current = close_table(&mut open_tables, &mut section, current);
            stash(&mut current, &mut open_tables, &mut section);
        }

        Ok(BodyText {
            sections: vec![section],
        })
    }
}

/// 손에 든 문단을 제 자리에 놓는다 — 열린 셀이 있으면 그 셀, 없으면 구역.
fn stash(current: &mut Option<Paragraph>, open: &mut [OpenTable], section: &mut Section) {
    let Some(para) = current.take() else { return };
    if let Some(table) = open.last_mut() {
        if let Some(index) = table.current_cell {
            table.cells[index].paragraphs.push(para);
            return;
        }
    }
    section.paragraphs.push(para);
}

/// 가장 안쪽 표를 닫고, 그 표를 품고 있던 문단을 돌려준다.
fn close_table(
    open: &mut Vec<OpenTable>,
    section: &mut Section,
    current: Option<Paragraph>,
) -> Option<Paragraph> {
    let mut current = current;
    stash(&mut current, open, section);
    let Some(mut finished) = open.pop() else {
        return current;
    };
    finished.table.cells = finished.cells;
    let mut owner = finished.owner?;
    owner.table_data = Some(finished.table);
    Some(owner)
}

#[derive(Debug, Default)]
pub struct BodyText {
    pub sections: Vec<Section>,
}

impl BodyText {
    pub fn extract_text(&self) -> String {
        let mut result = String::new();
        for section in &self.sections {
            for para in &section.paragraphs {
                append_paragraph_text(para, &mut result);
            }
        }
        result
    }
}

fn append_paragraph_text(para: &Paragraph, out: &mut String) {
    if let Some(text) = para.text.as_ref() {
        out.push_str(&text.content);
        out.push('\n');
    }
    if let Some(table) = para.table_data.as_ref() {
        for cell in &table.cells {
            for inner in &cell.paragraphs {
                append_paragraph_text(inner, out);
            }
        }
    }
}
