//! 대체 글꼴 글자 폭 보정 (지침 A).
//!
//! 기기에 문서의 원래 글꼴(맑은 고딕·Arial·함초롬 등)이 없으면 Pretendard 로
//! 그린다. Pretendard 는 한글이 0.865em 으로 맑은 고딕(1.0em)보다 13% 좁아
//! 한 줄에 글자가 더 들어가고 문서가 짧아진다(춘천 29쪽 → 27쪽).
//!
//! 여기서는 원래 글꼴의 글자 종류별 대표 폭(em)을 들고 있다가, 대체 글꼴의
//! 같은 종류 폭과의 비율을 돌려준다. 배치(줄 나눔)는 이 비율만큼 넓게 재고,
//! 그리기는 가로 배율을 `MAX_GLYPH_X` 까지만 쓰고 나머지는 글자 사이로 채운다.
//!
//! 값은 PC 기준 PDF 에서 잰 것만 넣는다(`tools/measure_advances.py`).
//! 모르는 글꼴은 한글만 1.00 으로 보고, 나머지 종류는 건드리지 않는다.
//! 원래 글꼴이 실제로 있으면(글꼴 이름표에 그 이름이 있으면) 보정하지 않는다.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// 그리기에서 글자 모양을 늘리는 상한. 이보다 크면 글자가 뭉개져 보인다.
pub const MAX_GLYPH_X: f32 = 1.08;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CharClass {
    Hangul,
    Latin,
    Digit,
    Space,
    Other,
}

pub fn class_of(c: char) -> CharClass {
    match c as u32 {
        0xAC00..=0xD7A3 | 0x1100..=0x11FF | 0x3130..=0x318F => CharClass::Hangul,
        _ if c == ' ' || c == '\u{a0}' => CharClass::Space,
        _ if c.is_ascii_digit() => CharClass::Digit,
        _ if c.is_ascii_alphabetic() => CharClass::Latin,
        _ => CharClass::Other,
    }
}

/// 종류별 배율. 1.0 이면 보정 없음.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClassScale {
    pub hangul: f32,
    pub latin: f32,
    pub digit: f32,
    pub space: f32,
}

impl ClassScale {
    pub fn of(&self, c: char) -> f32 {
        match class_of(c) {
            CharClass::Hangul => self.hangul,
            CharClass::Latin => self.latin,
            CharClass::Digit => self.digit,
            CharClass::Space => self.space,
            CharClass::Other => 1.0,
        }
    }
}

/// 원래 글꼴의 대표 폭(em). None 이면 그 종류는 모른다(보정 안 함).
#[derive(Clone, Copy, Debug)]
struct Targets {
    hangul: Option<f32>,
    latin: Option<f32>,
    digit: Option<f32>,
    space: Option<f32>,
}

pub fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 측정값(2026-11-08, PC 기준 PDF 의 글자 폭 중앙값).
///  - 맑은 고딕 한글 1.000 (NDA·춘천, 워드)
///  - Arial 영문 0.556 · 숫자 0.556 · 공백 0.278 (VISION, 파워포인트)
///  - 함초롬 한글 0.970 (월성 hwp)
fn targets_for(requested: &str) -> Targets {
    let n = normalize(requested);
    let none = Targets { hangul: None, latin: None, digit: None, space: None };
    if n.contains("맑은고딕") || n.contains("malgun") {
        return Targets { hangul: Some(1.0), ..none };
    }
    if n == "arial" || n.starts_with("arial") && !n.contains("unicode") && !n.contains("narrow") {
        return Targets { hangul: Some(1.0), latin: Some(0.556), digit: Some(0.556), space: Some(0.278) };
    }
    if n.contains("함초롬") || n.contains("hcr") || n.contains("hamchorom") {
        return Targets { hangul: Some(0.97), ..none };
    }
    // 모르는 글꼴: 한글만 전각(1.00)으로 본다. 한글 문서 글꼴은 거의 모두 전각이다.
    Targets { hangul: Some(1.0), ..none }
}

/// 영문 대표 폭을 잴 글자들. 본문에 흔한 소문자. 중앙값을 쓴다
/// (PC 측정도 글자 상자 폭의 중앙값이었다).
const LATIN_SAMPLE: &str = "etaoinsrhldcum";

fn face_names(face: &ttf_parser::Face) -> Vec<String> {
    face.names()
        .into_iter()
        .filter(|n| matches!(n.name_id, 1 | 4 | 6 | 16 | 21))
        .filter_map(|n| n.to_string())
        .map(|s| normalize(&s))
        .collect()
}

fn adv(face: &ttf_parser::Face, c: char) -> Option<f32> {
    let g = face.glyph_index(c)?;
    Some(face.glyph_hor_advance(g)? as f32 / face.units_per_em() as f32)
}

fn ratio(target: Option<f32>, actual: Option<f32>) -> f32 {
    match (target, actual) {
        (Some(t), Some(a)) if a > 0.05 => {
            let r = t / a;
            // 2% 안쪽은 재는 오차로 본다. 너무 큰 값은 잘못 잰 것이다.
            if (r - 1.0).abs() < 0.02 { 1.0 } else { r.clamp(0.85, 1.25) }
        }
        _ => 1.0,
    }
}

fn compute(requested: &str, data: &[u8], index: u32) -> Option<ClassScale> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    let want = normalize(requested);
    if want.is_empty() {
        return None;
    }
    // 원래 글꼴이 실제로 있다 — 보정하지 않는다.
    if face_names(&face).iter().any(|n| *n == want || n.starts_with(&want) && want.len() >= 4) {
        return None;
    }
    let t = targets_for(requested);
    let mut latin: Vec<f32> = LATIN_SAMPLE.chars().filter_map(|c| adv(&face, c)).collect();
    latin.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let latin_median = latin.get(latin.len() / 2).copied();
    let s = ClassScale {
        hangul: ratio(t.hangul, adv(&face, '가')),
        latin: ratio(t.latin, latin_median),
        digit: ratio(t.digit, adv(&face, '0')),
        space: ratio(t.space, adv(&face, ' ')),
    };
    if s.hangul == 1.0 && s.latin == 1.0 && s.digit == 1.0 && s.space == 1.0 {
        None
    } else {
        Some(s)
    }
}

type Key = (String, usize, usize, u32);
static CACHE: OnceLock<Mutex<HashMap<Key, Option<ClassScale>>>> = OnceLock::new();

/// 켜고 끄기. 기본 켜짐. 시험에서 보정 전후를 비교할 때 끈다.
static ENABLED: OnceLock<Mutex<bool>> = OnceLock::new();

pub fn set_enabled(on: bool) {
    *ENABLED.get_or_init(|| Mutex::new(true)).lock().expect("fontprofile lock") = on;
}

fn enabled() -> bool {
    std::env::var("ARGO_NO_FONT_PROFILE").is_err()
        && *ENABLED.get_or_init(|| Mutex::new(true)).lock().expect("fontprofile lock")
}

/// `requested` 글꼴을 `data` 글꼴로 대신 그릴 때의 종류별 배율.
/// 보정할 것이 없으면 None.
pub fn scale_for(requested: &str, data: &[u8], index: u32) -> Option<ClassScale> {
    if !enabled() {
        return None;
    }
    let key = (normalize(requested), data.as_ptr() as usize, data.len(), index);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().expect("fontprofile lock").get(&key) {
        return *v;
    }
    let v = compute(requested, data, index);
    cache.lock().expect("fontprofile lock").insert(key, v);
    v
}

/// 글꼴 이름이 적히지 않은 글자(문서 기본 글꼴)를 무엇으로 볼지.
/// 워드는 docDefaults·테마 글꼴에서 정한다. 모르면 None — '모르는 글꼴'(한글만 1.00).
static DEFAULT_FAMILY: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub fn set_default_family(name: Option<String>) {
    *DEFAULT_FAMILY.get_or_init(|| Mutex::new(None)).lock().expect("fontprofile lock") = name;
}

/// 이름이 없을 때 쓸 이름. 모르면 빈 이름 대신 '?' (어느 글꼴 이름표와도 맞지 않는다).
pub fn requested_or_default(name: Option<&str>) -> String {
    match name.filter(|n| !n.trim().is_empty()) {
        Some(n) => n.to_owned(),
        None => DEFAULT_FAMILY
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("fontprofile lock")
            .clone()
            .unwrap_or_else(|| "?".to_owned()),
    }
}

/// 이번 변환에서 실제로 보정한 글꼴 이름들(진단 로그용).
static APPLIED: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

pub fn note_applied(requested: &str) {
    let mut v = APPLIED.get_or_init(|| Mutex::new(Vec::new())).lock().expect("fontprofile lock");
    if !v.iter().any(|n| n == requested) {
        v.push(requested.to_owned());
    }
}

pub fn take_applied() -> Vec<String> {
    std::mem::take(&mut *APPLIED.get_or_init(|| Mutex::new(Vec::new())).lock().expect("fontprofile lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(class_of('가'), CharClass::Hangul);
        assert_eq!(class_of('a'), CharClass::Latin);
        assert_eq!(class_of('7'), CharClass::Digit);
        assert_eq!(class_of(' '), CharClass::Space);
        assert_eq!(class_of('('), CharClass::Other);
    }

    #[test]
    fn targets() {
        assert_eq!(targets_for("맑은 고딕").hangul, Some(1.0));
        assert_eq!(targets_for("Malgun Gothic").latin, None);
        assert_eq!(targets_for("Arial").latin, Some(0.556));
        assert_eq!(targets_for("함초롬바탕").hangul, Some(0.97));
        assert_eq!(targets_for("Arial Unicode MS").latin, None);
    }

    #[test]
    fn ratio_bounds() {
        assert_eq!(ratio(Some(1.0), Some(0.995)), 1.0);
        assert!((ratio(Some(1.0), Some(0.865)) - 1.156).abs() < 0.01);
        assert_eq!(ratio(Some(2.0), Some(0.5)), 1.25);
        assert_eq!(ratio(None, Some(0.5)), 1.0);
    }
}
