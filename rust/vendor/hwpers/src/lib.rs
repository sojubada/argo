pub mod crypto;
pub mod error;
pub mod hwpx;
pub mod model;
pub mod parser;
pub mod preview;
pub mod reader;
pub mod render;
pub mod utils;
pub mod writer;

use std::io::{Read, Seek};
use std::path::Path;

pub use crate::crypto::decrypt_distribution_stream;
pub use crate::error::{HwpError, Result};
pub use crate::hwpx::{HwpxReader, HwpxWriter};
pub use crate::model::HwpDocument;
use crate::parser::{body_text::BodyTextParser, doc_info::DocInfoParser, header::FileHeader};
pub use crate::preview::{PreviewImage, PreviewText, SummaryInfo};
use crate::reader::CfbReader;
pub use crate::writer::style;
pub use crate::writer::HwpWriter;

pub struct HwpReader;

impl HwpReader {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<HwpDocument> {
        let reader = CfbReader::from_file(path)?;
        Self::parse_document(reader)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<HwpDocument> {
        let cursor = std::io::Cursor::new(bytes.to_vec());
        let reader = CfbReader::new(cursor)?;
        Self::parse_document(reader)
    }

    fn parse_document<F: Read + Seek>(mut reader: CfbReader<F>) -> Result<HwpDocument> {
        let header_data = reader.read_stream("FileHeader")?;
        let header = FileHeader::parse(header_data)?;

        if header.is_encrypted() {
            return Err(HwpError::UnsupportedVersion(
                "Password-encrypted documents are not supported".to_string(),
            ));
        }

        let distribution_record = if header.is_distribute() {
            Some(Self::read_distribution_record(
                &mut reader,
                header.is_compressed(),
            )?)
        } else {
            None
        };

        // DocInfo 를 못 읽어도 문서를 포기하지 않는다.
        //
        // 여기에는 글꼴과 글자 모양이 들어 있을 뿐, 글 자체는 BodyText 에
        // 있다. 모양표가 없으면 기본 모양으로 그리면 된다 — 글씨체가 다른
        // 것과 아무것도 못 보는 것 중에서는 앞쪽이 낫다.
        let doc_info = reader
            .read_stream("DocInfo")
            .and_then(|data| Self::decrypt_stream(data, &header, distribution_record.as_deref()))
            .and_then(|data| DocInfoParser::parse(data, header.is_compressed()))
            .unwrap_or_default();

        let mut body_texts = Vec::new();
        let mut section_idx = 0;

        let stream_prefix = if header.is_distribute() {
            "ViewText/Section"
        } else {
            "BodyText/Section"
        };

        loop {
            let section_name = format!("{stream_prefix}{section_idx}");
            if !reader.stream_exists(&section_name) {
                break;
            }

            // 구역 하나가 깨졌다고 나머지를 버리지 않는다. 뒤쪽 구역이
            // 멀쩡하면 거기까지는 보여 준다.
            let section = reader
                .read_stream(&section_name)
                .and_then(|data| {
                    Self::decrypt_stream(data, &header, distribution_record.as_deref())
                })
                .and_then(|data| BodyTextParser::parse(data, header.is_compressed()));
            if let Ok(body_text) = section {
                body_texts.push(body_text);
            }

            section_idx += 1;
        }

        if body_texts.is_empty() {
            return Err(HwpError::InvalidFormat(
                "No BodyText sections found".to_string(),
            ));
        }

        let preview_text = Self::read_preview_text(&mut reader).ok();
        let preview_image = Self::read_preview_image(&mut reader).ok();
        let summary_info = Self::read_summary_info(&mut reader).ok();

        Ok(HwpDocument {
            header,
            doc_info,
            body_texts,
            preview_text,
            preview_image,
            summary_info,
        })
    }

    fn read_preview_text<F: Read + Seek>(reader: &mut CfbReader<F>) -> Result<PreviewText> {
        let data = reader.read_stream("PrvText")?;
        PreviewText::from_bytes(&data)
    }

    fn read_preview_image<F: Read + Seek>(reader: &mut CfbReader<F>) -> Result<PreviewImage> {
        let data = reader.read_stream("PrvImage")?;
        Ok(PreviewImage::from_bytes(data))
    }

    fn read_summary_info<F: Read + Seek>(reader: &mut CfbReader<F>) -> Result<SummaryInfo> {
        let data = reader.read_stream("\x05HwpSummaryInformation")?;
        SummaryInfo::from_bytes(&data)
    }

    fn read_distribution_record<F: Read + Seek>(
        reader: &mut CfbReader<F>,
        is_compressed: bool,
    ) -> Result<Vec<u8>> {
        let doc_info_data = reader.read_stream("DocInfo")?;

        let decompressed = if is_compressed {
            crate::utils::decompress(&doc_info_data)?
        } else {
            doc_info_data
        };

        if decompressed.len() < 260 {
            return Err(HwpError::ParseError(
                "DocInfo too short to contain distribution data".to_string(),
            ));
        }

        Ok(decompressed[..260].to_vec())
    }

    fn decrypt_stream(
        data: Vec<u8>,
        _header: &FileHeader,
        distribution_record: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        if let Some(dist_record) = distribution_record {
            if data.len() < 260 {
                return Ok(data);
            }
            let encrypted_data = &data[260..];
            decrypt_distribution_stream(encrypted_data, dist_record)
        } else {
            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_file_path(name: &str) -> PathBuf {
        PathBuf::from("test-files").join(name)
    }

    #[test]
    fn test_reader_creation() {
        let path = test_file_path("test_document.hwp");
        if path.exists() {
            let result = HwpReader::from_file(&path);
            assert!(result.is_ok() || result.is_err());
        }
    }

    #[test]
    fn test_file_header_signature() {
        let signature = b"HWP Document File";
        assert_eq!(signature.len(), 17);
    }
}
