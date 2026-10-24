use crate::error::Result;
use crate::model::bin_data::BinData;
use crate::model::border_fill::BorderFill;
use crate::model::numbering::{Bullet, Numbering};
use crate::model::style::Style;
use crate::model::tab_def::TabDef;
use crate::model::{CharShape, DocumentProperties, FaceName, ParaShape};
use crate::parser::record::{HwpTag, Record};
use crate::reader::StreamReader;
use crate::utils::compression::decompress_stream;

pub struct DocInfoParser;

impl DocInfoParser {
    pub fn parse(data: Vec<u8>, is_compressed: bool) -> Result<DocInfo> {
        let data = if is_compressed {
            decompress_stream(&data)?
        } else {
            data
        };

        let mut reader = StreamReader::new(data);
        let mut doc_info = DocInfo::default();

        while reader.remaining() >= 4 {
            // Need at least 4 bytes for record header
            let record = match Record::parse(&mut reader) {
                Ok(r) => r,
                Err(_) => break, // Stop parsing on error
            };

            match HwpTag::from_u16(record.tag_id()) {
                // 레코드 하나가 깨졌다고 그때까지 읽은 것을 버리지 않는다.
                //
                // 전에는 물음표 하나가 글꼴·글자모양·스타일을 통째로
                // 날렸다. 모양표가 아예 없으면 기본 모양으로 떨어지는데,
                // 절반이라도 살리는 쪽이 글씨체가 덜 어긋난다.
                Some(HwpTag::DocumentProperties) => {
                    doc_info.properties = DocumentProperties::from_record(&record).ok();
                }
                Some(HwpTag::FaceName) => {
                    if let Ok(value) = FaceName::from_record(&record) {
                        doc_info.face_names.push(value);
                    }
                }
                Some(HwpTag::CharShape) => {
                    if let Ok(value) = CharShape::from_record(&record) {
                        doc_info.char_shapes.push(value);
                    }
                }
                Some(HwpTag::ParaShape) => {
                    if let Ok(value) = ParaShape::from_record(&record) {
                        doc_info.para_shapes.push(value);
                    }
                }
                Some(HwpTag::Style) => {
                    if let Ok(value) = Style::from_record(&record) {
                        doc_info.styles.push(value);
                    }
                }
                Some(HwpTag::BorderFill) => {
                    if let Ok(value) = BorderFill::from_record(&record) {
                        doc_info.border_fills.push(value);
                    }
                }
                Some(HwpTag::TabDef) => {
                    if let Ok(value) = TabDef::from_record(&record) {
                        doc_info.tab_defs.push(value);
                    }
                }
                Some(HwpTag::Numbering) => {
                    if let Ok(value) = Numbering::from_record(&record) {
                        doc_info.numberings.push(value);
                    }
                }
                Some(HwpTag::Bullet) => {
                    if let Ok(value) = Bullet::from_record(&record) {
                        doc_info.bullets.push(value);
                    }
                }
                Some(HwpTag::BinData) => {
                    if let Ok(value) = BinData::from_record(&record) {
                        doc_info.bin_data.push(value);
                    }
                }
                _ => {
                    // Skip unknown or unimplemented tags
                }
            }
        }

        Ok(doc_info)
    }
}

#[derive(Debug, Default)]
pub struct DocInfo {
    pub properties: Option<DocumentProperties>,
    pub face_names: Vec<FaceName>,
    pub char_shapes: Vec<CharShape>,
    pub para_shapes: Vec<ParaShape>,
    pub styles: Vec<Style>,
    pub border_fills: Vec<BorderFill>,
    pub tab_defs: Vec<TabDef>,
    pub numberings: Vec<Numbering>,
    pub bullets: Vec<Bullet>,
    pub bin_data: Vec<BinData>,
}
