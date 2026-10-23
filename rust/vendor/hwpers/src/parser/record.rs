use crate::error::Result;
use crate::reader::StreamReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordHeader {
    pub tag_id: u16,
    pub level: u8,
    pub size: u32,
}

impl RecordHeader {
    pub fn parse(reader: &mut StreamReader) -> Result<Self> {
        // Read single 32-bit header
        let header = reader.read_u32()?;

        // Extract fields from packed header
        // Bits 0-9: tag_id (10 bits)
        // Bits 10-19: level (10 bits)
        // Bits 20-31: size (12 bits)
        let tag_id = (header & 0x3FF) as u16;
        let level = ((header >> 10) & 0x3FF) as u8;
        let mut size = header >> 20;

        // If size is 0xFFF (4095), read extended size
        if size == 0xFFF {
            size = reader.read_u32()?;
        }

        Ok(Self {
            tag_id,
            level,
            size,
        })
    }
}

#[derive(Debug)]
pub struct Record {
    pub header: RecordHeader,
    pub data: Vec<u8>,
}

impl Record {
    pub fn parse(reader: &mut StreamReader) -> Result<Self> {
        if reader.remaining() < 4 {
            return Err(crate::error::HwpError::ParseError(
                "Not enough data for record header".to_string(),
            ));
        }

        let header = RecordHeader::parse(reader)?;

        // Size field contains the data size (without header)
        let data_size = header.size as usize;

        if data_size > reader.remaining() {
            return Err(crate::error::HwpError::ParseError(format!(
                "Record size {} exceeds remaining data {}",
                data_size,
                reader.remaining()
            )));
        }

        // Read data bytes
        let data = reader.read_bytes(data_size)?;

        Ok(Self { header, data })
    }

    pub fn tag_id(&self) -> u16 {
        self.header.tag_id
    }

    pub fn data_reader(&self) -> StreamReader {
        StreamReader::new(self.data.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum HwpTag {
    // Document Info
    DocumentProperties = 0x10,
    IdMappings = 0x11,
    BinData = 0x12,
    FaceName = 0x13,
    BorderFill = 0x14,
    CharShape = 0x15,
    TabDef = 0x16,
    Numbering = 0x17,
    Bullet = 0x18,
    ParaShape = 0x19,
    Style = 0x1A,
    DocData = 0x1B,
    DistributeDocData = 0x1C,

    // BodyText — 모두 HWPTAG_BEGIN(0x10) + N 이다.
    //
    // 예전 표는 여기서부터 14 만큼 밀려 있었다. 그래서 PageDef 를 한 번도
    // 못 찾아 "Section must have page definition" 으로 죽었고, 표와 셀도
    // 전부 못 보고 지나쳤다.
    ParaHeader = 0x42,
    ParaText = 0x43,
    ParaCharShape = 0x44,
    ParaLineSeg = 0x45,
    ParaRangeTag = 0x46,
    CtrlHeader = 0x47,
    ListHeader = 0x48,
    PageDef = 0x49,
    FootnoteShape = 0x4A,
    PageBorderFill = 0x4B,
    ShapeComponent = 0x4C,
    Table = 0x4D,
    ShapeComponentLine = 0x4E,
    ShapeComponentRectangle = 0x4F,
    ShapeComponentEllipse = 0x50,
    ShapeComponentArc = 0x51,
    ShapeComponentPolygon = 0x52,
    ShapeComponentCurve = 0x53,
    ShapeComponentOle = 0x54,
    ShapeComponentPicture = 0x55,
    ShapeComponentContainer = 0x56,
    CtrlData = 0x57,
    EqEdit = 0x58,
    ShapeComponentTextArt = 0x5A,
    FormObject = 0x5B,
    MemoShape = 0x5C,
    MemoList = 0x5D,
    ChartData = 0x5F,
    VideoData = 0x62,
    ShapeComponentUnknown = 0x73,
}

impl HwpTag {
    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            0x10 => Some(Self::DocumentProperties),
            0x11 => Some(Self::IdMappings),
            0x12 => Some(Self::BinData),
            0x13 => Some(Self::FaceName),
            0x14 => Some(Self::BorderFill),
            0x15 => Some(Self::CharShape),
            0x16 => Some(Self::TabDef),
            0x17 => Some(Self::Numbering),
            0x18 => Some(Self::Bullet),
            0x19 => Some(Self::ParaShape),
            0x1A => Some(Self::Style),
            0x1B => Some(Self::DocData),
            0x1C => Some(Self::DistributeDocData),
            0x42 => Some(Self::ParaHeader),
            0x43 => Some(Self::ParaText),
            0x44 => Some(Self::ParaCharShape),
            0x45 => Some(Self::ParaLineSeg),
            0x46 => Some(Self::ParaRangeTag),
            0x47 => Some(Self::CtrlHeader),
            0x48 => Some(Self::ListHeader),
            0x49 => Some(Self::PageDef),
            0x4A => Some(Self::FootnoteShape),
            0x4B => Some(Self::PageBorderFill),
            0x4C => Some(Self::ShapeComponent),
            0x4D => Some(Self::Table),
            0x4E => Some(Self::ShapeComponentLine),
            0x4F => Some(Self::ShapeComponentRectangle),
            0x50 => Some(Self::ShapeComponentEllipse),
            0x51 => Some(Self::ShapeComponentArc),
            0x52 => Some(Self::ShapeComponentPolygon),
            0x53 => Some(Self::ShapeComponentCurve),
            0x54 => Some(Self::ShapeComponentOle),
            0x55 => Some(Self::ShapeComponentPicture),
            0x56 => Some(Self::ShapeComponentContainer),
            0x57 => Some(Self::CtrlData),
            0x58 => Some(Self::EqEdit),
            0x5A => Some(Self::ShapeComponentTextArt),
            0x5B => Some(Self::FormObject),
            0x5C => Some(Self::MemoShape),
            0x5D => Some(Self::MemoList),
            0x5F => Some(Self::ChartData),
            0x62 => Some(Self::VideoData),
            0x73 => Some(Self::ShapeComponentUnknown),
            _ => None,
        }
    }
}
