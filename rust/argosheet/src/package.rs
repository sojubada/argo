use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{BufReader, Read, Seek};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    pub target_mode: Option<String>,
}

/// 기준 부품 경로(예: "xl/workbook.xml")를 기준으로 상대/절대 Target 경로를 정규화한다.
/// 왜: 엑셀 작성 도구마다 Target을 "worksheets/sheet1.xml", "/xl/worksheets/sheet1.xml",
/// "../comments1.xml" 등 다르게 기록하므로 ZIP 항목 이름과 일치시키려면 정규화가 필수다.
pub fn resolve_part_target(base_part: &str, target: &str) -> String {
    let clean = target.trim().replace('\\', "/");
    if let Some(stripped) = clean.strip_prefix('/') {
        return normalize_path_segments(stripped);
    }

    let base_dir = match base_part.rfind('/') {
        Some(idx) => &base_part[..idx],
        None => "",
    };

    let joined = if base_dir.is_empty() {
        clean
    } else {
        format!("{}/{}", base_dir, clean)
    };
    normalize_path_segments(&joined)
}

fn normalize_path_segments(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

/// 부품 경로에 대응하는 `.rels` 파일 경로를 생성한다.
pub fn rels_path_for_part(part_path: &str) -> String {
    if part_path.is_empty() {
        return "_rels/.rels".to_string();
    }
    match part_path.rfind('/') {
        Some(idx) => format!("{}/_rels/{}.rels", &part_path[..idx], &part_path[idx + 1..]),
        None => format!("_rels/{}.rels", part_path),
    }
}

/// 지정한 부품의 `.rels` 파일을 스트리밍으로 읽어 `r:id -> Relationship` 맵을 반환한다.
/// 왜: `.rels`가 없는 부품(예: 댓글이나 드로잉이 없는 시트)은 오류가 아니므로 빈 맵을 돌려준다.
pub fn read_part_relationships<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    part_path: &str,
) -> Result<HashMap<String, Relationship>, String> {
    let rels_path = rels_path_for_part(part_path);
    let file = match archive.by_name(&rels_path) {
        Ok(f) => f,
        Err(_) => return Ok(HashMap::new()),
    };

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::with_capacity(512);
    let mut rels = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                if e.local_name().as_ref() == b"Relationship" {
                    let mut id = String::new();
                    let mut rel_type = String::new();
                    let mut target = String::new();
                    let mut target_mode = None;

                    for attr in e.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"Id" => id = attr.unescape_value().unwrap_or_default().into_owned(),
                            b"Type" => {
                                rel_type = attr.unescape_value().unwrap_or_default().into_owned()
                            }
                            b"Target" => {
                                target = attr.unescape_value().unwrap_or_default().into_owned()
                            }
                            b"TargetMode" => {
                                target_mode =
                                    Some(attr.unescape_value().unwrap_or_default().into_owned())
                            }
                            _ => {}
                        }
                    }
                    if !id.is_empty() {
                        rels.insert(
                            id.clone(),
                            Relationship {
                                id,
                                rel_type,
                                target,
                                target_mode,
                            },
                        );
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("rels XML parse error ({}): {}", rels_path, e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(rels)
}

/// Transitional OOXML과 Strict OOXML의 네임스페이스 URI 차이를 흡수하기 위해 접미사로 판별한다.
pub fn is_rel_type(rel_type: &str, suffix: &str) -> bool {
    rel_type.ends_with(suffix)
}

