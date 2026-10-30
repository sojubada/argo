//! pptx 꾸러미(ZIP) 읽기: 부분(part)과 관계(rels).

use std::collections::HashMap;
use std::io::{Cursor, Read};

pub(crate) struct Package {
    parts: HashMap<String, Vec<u8>>,
}

/// 한 부분이 가리키는 다른 부분. (종류 끝 이름, 대상 경로)
#[derive(Clone, Debug)]
pub(crate) struct Rel {
    pub kind: String,
    pub target: String,
    pub external: bool,
}

const MAX_PART: u64 = 256 * 1024 * 1024;

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package, String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;
        let mut parts = HashMap::new();
        let mut total: u64 = 0;
        for index in 0..archive.len() {
            let Ok(mut file) = archive.by_index(index) else { continue };
            if file.is_dir() || file.size() > MAX_PART {
                continue;
            }
            total += file.size();
            if total > 1024 * 1024 * 1024 {
                return Err("너무 큰 꾸러미".into());
            }
            let name = file.name().trim_start_matches('/').to_owned();
            let mut data = Vec::with_capacity(file.size() as usize);
            if file.read_to_end(&mut data).is_ok() {
                parts.insert(name, data);
            }
        }
        Ok(Package { parts })
    }

    /// 이미 갖춘 부분들로 (옛 ppt 를 옮긴 것).
    pub fn from_parts(parts: HashMap<String, Vec<u8>>) -> Package {
        Package { parts }
    }

    pub fn bytes(&self, path: &str) -> Option<&[u8]> {
        self.parts.get(path.trim_start_matches('/')).map(|v| v.as_slice()).or_else(|| {
            // 대소문자만 다른 경로를 쓰는 파일이 있다.
            let lower = path.trim_start_matches('/').to_lowercase();
            self.parts.iter().find(|(k, _)| k.to_lowercase() == lower).map(|(_, v)| v.as_slice())
        })
    }

    pub fn text(&self, path: &str) -> Option<String> {
        let bytes = self.bytes(path)?;
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        if bytes.starts_with(&[0xFF, 0xFE]) {
            let units: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            return Some(String::from_utf16_lossy(&units));
        }
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    pub fn names(&self) -> Vec<String> {
        self.parts.keys().cloned().collect()
    }

    pub fn has(&self, path: &str) -> bool {
        self.bytes(path).is_some()
    }

    /// 부분의 관계 목록. id → Rel
    pub fn rels(&self, part: &str) -> HashMap<String, Rel> {
        let (dir, file) = match part.rfind('/') {
            Some(i) => (&part[..i], &part[i + 1..]),
            None => ("", part),
        };
        let rels_path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let mut out = HashMap::new();
        let Some(xml) = self.text(&rels_path) else { return out };
        let Ok(doc) = roxmltree::Document::parse(&xml) else { return out };
        for node in doc.descendants().filter(|n| n.tag_name().name() == "Relationship") {
            let (Some(id), Some(target)) = (node.attribute("Id"), node.attribute("Target")) else { continue };
            let kind = node.attribute("Type").unwrap_or("").rsplit('/').next().unwrap_or("").to_owned();
            let external = node.attribute("TargetMode") == Some("External");
            let target = if external { target.to_owned() } else { resolve(dir, target) };
            out.insert(id.to_owned(), Rel { kind, target, external });
        }
        out
    }
}

/// 상대 경로를 꾸러미 안 절대 경로로.
pub(crate) fn resolve(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_owned();
    }
    let mut stack: Vec<&str> = if dir.is_empty() { Vec::new() } else { dir.split('/').collect() };
    for seg in target.split('/') {
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

pub(crate) fn dir_of(part: &str) -> &str {
    part.rfind('/').map(|i| &part[..i]).unwrap_or("")
}
