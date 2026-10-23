use crate::error::Result;
use crate::parser::record::Record;

#[derive(Debug, Clone)]
pub struct CtrlHeader {
    pub ctrl_id: u32,
    pub properties: u32,
    pub instance_id: u32,
}

impl CtrlHeader {
    pub fn from_record(record: &Record) -> Result<Self> {
        let mut reader = record.data_reader();

        if reader.remaining() < 4 {
            return Err(crate::error::HwpError::ParseError(format!(
                "CtrlHeader record too small: {} bytes",
                reader.remaining()
            )));
        }

        let ctrl_id = reader.read_u32()?;
        let properties = if reader.remaining() >= 4 {
            reader.read_u32()?
        } else {
            0
        };
        let instance_id = if reader.remaining() >= 4 {
            reader.read_u32()?
        } else {
            0
        };

        Ok(Self {
            ctrl_id,
            properties,
            instance_id,
        })
    }

    pub fn get_control_type(&self) -> ControlType {
        ControlType::from_ctrl_id(self.ctrl_id)
    }

    pub fn is_inline(&self) -> bool {
        (self.properties & 0x01) != 0
    }

    pub fn affects_line_pacing(&self) -> bool {
        (self.properties & 0x02) != 0
    }

    pub fn is_word_break_allowed(&self) -> bool {
        (self.properties & 0x04) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    Table,
    Gso, // Drawing object
    TextBox,
    Equation,
    SectionDefinition,
    ColumnDefinition,
    HeaderFooter,
    Footnote,
    Endnote,
    AutoNumber,
    NewNumber,
    PageHide,
    PageNumberPosition,
    IndexMark,
    BookMark,
    OverlappingLetter,
    HiddenComment,
    Field,
    Unknown,
}

impl ControlType {
    pub fn from_ctrl_id(ctrl_id: u32) -> Self {
        // 네 글자 컨트롤 ID. 파일에는 거꾸로 들어 있어서 리틀엔디언으로
        // 읽으면 'tbl ' 이 0x746C6220 이 된다 — 예전 값들은 두 글자짜리
        // 였고, 그래서 표를 한 번도 못 알아봤다.
        match ctrl_id {
            0x74626C20 => Self::Table, // 'tbl '
            0x67736F20 => Self::Gso, // 'gso '
            0x24726563 => Self::TextBox, // '$rec'
            0x65716564 => Self::Equation, // 'eqed'
            0x73656364 => Self::SectionDefinition, // 'secd'
            0x636F6C64 => Self::ColumnDefinition, // 'cold'
            0x68656164 => Self::HeaderFooter, // 'head'
            0x666F6F74 => Self::HeaderFooter, // 'foot'
            0x666E2020 => Self::Footnote, // 'fn  '
            0x656E2020 => Self::Endnote, // 'en  '
            0x61746E6F => Self::AutoNumber, // 'atno'
            0x6E776E6F => Self::NewNumber, // 'nwno'
            0x70676864 => Self::PageHide, // 'pghd'
            0x70676E70 => Self::PageNumberPosition, // 'pgnp'
            0x6964786D => Self::IndexMark, // 'idxm'
            0x626F6B6D => Self::BookMark, // 'bokm'
            0x74637073 => Self::OverlappingLetter, // 'tcps'
            0x74636D74 => Self::HiddenComment, // 'tcmt'
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Table => "Table",
            Self::Gso => "DrawingObject",
            Self::TextBox => "TextBox",
            Self::Equation => "Equation",
            Self::SectionDefinition => "SectionDefinition",
            Self::ColumnDefinition => "ColumnDefinition",
            Self::HeaderFooter => "HeaderFooter",
            Self::Footnote => "Footnote",
            Self::Endnote => "Endnote",
            Self::AutoNumber => "AutoNumber",
            Self::NewNumber => "NewNumber",
            Self::PageHide => "PageHide",
            Self::PageNumberPosition => "PageNumberPosition",
            Self::IndexMark => "IndexMark",
            Self::BookMark => "BookMark",
            Self::OverlappingLetter => "OverlappingLetter",
            Self::HiddenComment => "HiddenComment",
            Self::Field => "Field",
            Self::Unknown => "Unknown",
        }
    }
}
