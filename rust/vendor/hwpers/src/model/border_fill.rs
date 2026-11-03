use crate::error::Result;
use crate::parser::record::Record;
use crate::reader::StreamReader;

#[derive(Debug, Clone)]
pub struct BorderFill {
    pub properties: u16,
    pub left: BorderLine,
    pub right: BorderLine,
    pub top: BorderLine,
    pub bottom: BorderLine,
    pub diagonal: BorderLine,
    pub fill_info: FillInfo,
}

#[derive(Debug, Clone)]
pub struct BorderLine {
    pub line_type: u8,
    pub thickness: u8,
    pub color: u32,
}

#[derive(Debug, Clone)]
pub struct FillInfo {
    pub fill_type: u32,
    pub back_color: u32,
    pub pattern_color: u32,
    pub pattern_type: u32,
    pub image_info: Option<ImageInfo>,
    pub gradient_info: Option<GradientInfo>,
}

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub brightness: u8,
    pub contrast: u8,
    pub effect: u8,
    pub bin_data_id: u16,
}

#[derive(Debug, Clone)]
pub struct GradientInfo {
    pub gradient_type: u8,
    pub start_color: u32,
    pub end_color: u32,
    pub angle: u16,
    pub center_x: u16,
    pub center_y: u16,
    pub blur_degree: u16,
}

impl BorderFill {
    pub fn from_record(record: &Record) -> Result<Self> {
        let mut reader = record.data_reader();

        if reader.remaining() < 2 {
            return Err(crate::error::HwpError::ParseError(format!(
                "BorderFill record too small: {} bytes",
                reader.remaining()
            )));
        }

        let properties = reader.read_u16()?;

        // Initialize with defaults
        let mut left = BorderLine {
            line_type: 0,
            thickness: 0,
            color: 0,
        };
        let mut right = BorderLine {
            line_type: 0,
            thickness: 0,
            color: 0,
        };
        let mut top = BorderLine {
            line_type: 0,
            thickness: 0,
            color: 0,
        };
        let mut bottom = BorderLine {
            line_type: 0,
            thickness: 0,
            color: 0,
        };
        let mut diagonal = BorderLine {
            line_type: 0,
            thickness: 0,
            color: 0,
        };

        // Read border lines if available
        if reader.remaining() >= 6 {
            left = BorderLine::read(&mut reader)?;
        }
        if reader.remaining() >= 6 {
            right = BorderLine::read(&mut reader)?;
        }
        if reader.remaining() >= 6 {
            top = BorderLine::read(&mut reader)?;
        }
        if reader.remaining() >= 6 {
            bottom = BorderLine::read(&mut reader)?;
        }
        if reader.remaining() >= 6 {
            diagonal = BorderLine::read(&mut reader)?;
        }

        // Read fill info if available
        let fill_info = if reader.remaining() >= 4 {
            FillInfo::read(&mut reader)?
        } else {
            FillInfo {
                fill_type: 0,
                back_color: 0xFFFFFFFF,
                pattern_color: 0,
                pattern_type: 0,
                image_info: None,
                gradient_info: None,
            }
        };

        Ok(Self {
            properties,
            left,
            right,
            top,
            bottom,
            diagonal,
            fill_info,
        })
    }
}
impl BorderFill {
    /// Create a new default BorderFill for writing
    pub fn new_default() -> Self {
        let default_border = BorderLine {
            line_type: 0, // No border
            thickness: 0,
            color: 0x000000, // Black
        };

        Self {
            properties: 0,
            left: default_border.clone(),
            right: default_border.clone(),
            top: default_border.clone(),
            bottom: default_border.clone(),
            diagonal: default_border,
            fill_info: FillInfo {
                fill_type: 0,           // No fill
                back_color: 0xFFFFFFFF, // White background
                pattern_color: 0,
                pattern_type: 0,
                image_info: None,
                gradient_info: None,
            },
        }
    }

    /// Create a border fill with actual border lines
    pub fn new_with_border() -> Self {
        let border = BorderLine {
            line_type: 1,    // Solid line
            thickness: 20,   // 0.1mm thickness
            color: 0x000000, // Black
        };

        Self {
            properties: 0x0001, // Enable borders
            left: border.clone(),
            right: border.clone(),
            top: border.clone(),
            bottom: border.clone(),
            diagonal: BorderLine {
                line_type: 0, // No diagonal
                thickness: 0,
                color: 0x000000,
            },
            fill_info: FillInfo {
                fill_type: 1,           // Solid fill
                back_color: 0xFFFFFFF0, // Light cream background
                pattern_color: 0,
                pattern_type: 0,
                image_info: None,
                gradient_info: None,
            },
        }
    }
}

impl BorderLine {
    fn read(reader: &mut StreamReader) -> Result<Self> {
        if reader.remaining() < 6 {
            return Err(crate::error::HwpError::ParseError(
                "Insufficient data for BorderLine".to_string(),
            ));
        }

        Ok(Self {
            line_type: reader.read_u8()?,
            thickness: reader.read_u8()?,
            color: reader.read_u32()?,
        })
    }
}

impl FillInfo {
    /// 채우기 정보. 종류(u32)는 비트 묶음이다: bit0 단색, bit1 그림, bit2 그러데이션.
    /// 단색일 때만 배경색·무늬색·무늬 종류가 따라온다 — 예전에는 종류와 상관없이
    /// 바로 다음 4바이트를 배경색으로 읽어, 채우기 없는 칸이 엉뚱한 색이 되었다.
    /// 그러데이션 크기는 실제 파일(rhwp·hwplib 기준)을 따른다.
    fn read(reader: &mut StreamReader) -> Result<Self> {
        let fill_type = reader.read_u32()?;
        let mut back_color = 0xFFFFFFFF;
        let mut pattern_color = 0;
        let mut pattern_type = 0;
        if fill_type & 0x01 != 0 && reader.remaining() >= 12 {
            back_color = reader.read_u32()?;
            pattern_color = reader.read_u32()?;
            pattern_type = reader.read_u32()?;
        }

        let mut gradient_info = None;
        if fill_type & 0x04 != 0 && reader.remaining() >= 21 {
            let gradient_type = reader.read_u8()?;
            let angle = reader.read_u32()? as u16;
            let center_x = reader.read_u32()? as u16;
            let center_y = reader.read_u32()? as u16;
            let blur_degree = reader.read_u32()? as u16;
            let count = (reader.read_u32()? as usize).min(64);
            if count > 2 {
                for _ in 0..count {
                    if reader.remaining() < 4 {
                        break;
                    }
                    reader.read_u32()?;
                }
            }
            let mut colors = Vec::with_capacity(count);
            for _ in 0..count {
                if reader.remaining() < 4 {
                    break;
                }
                colors.push(reader.read_u32()?);
            }
            if let (Some(&start_color), Some(&end_color)) = (colors.first(), colors.last()) {
                gradient_info = Some(GradientInfo {
                    gradient_type,
                    start_color,
                    end_color,
                    angle,
                    center_x,
                    center_y,
                    blur_degree,
                });
            }
        }

        // 그림 채우기는 칸 배경에서는 그리지 않는다 (번호만 남긴다).
        let image_info = if fill_type & 0x02 != 0 && reader.remaining() >= 6 {
            let _mode = reader.read_u8()?;
            let brightness = reader.read_u8()?;
            let contrast = reader.read_u8()?;
            let effect = reader.read_u8()?;
            let bin_data_id = reader.read_u16()?;
            Some(ImageInfo { brightness, contrast, effect, bin_data_id })
        } else {
            None
        };

        Ok(Self {
            fill_type,
            back_color,
            pattern_color,
            pattern_type,
            image_info,
            gradient_info,
        })
    }
}
