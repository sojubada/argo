//! 글꼴 고르기와 글자 폭 재기.
//!
//! 글자 배치를 우리가 직접 하므로, 잰 글꼴과 그린 글꼴이 같아야 한다.
//! 그래서 SVG 에는 글꼴 이름 대신 `f<번호>` 를 적고, usvg 의 글꼴 고르기를
//! 그 번호로 바로 그 얼굴(face)을 돌려주게 바꾼다.

use std::collections::HashMap;
use std::sync::Arc;

use svg2pdf::usvg;
use usvg::fontdb::{Database, Family, Query, Style, Weight, ID};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Metrics {
    /// em 비율
    pub ascent: f32,
    pub descent: f32,
}

struct Face {
    id: ID,
    names: Vec<String>,
    bold: bool,
    italic: bool,
    metrics: Metrics,
}

pub(crate) struct FontBook {
    pub db: Arc<Database>,
    faces: Vec<Face>,
    aliases: HashMap<String, String>,
    pick_cache: HashMap<(String, bool, bool), Option<usize>>,
    glyph_cache: HashMap<(usize, char), Option<f32>>,
    fallback_cache: HashMap<(usize, char), usize>,
}

pub(crate) fn is_east_asian(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3130..=0x318F
        | 0x3200..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

impl FontBook {
    pub fn new(fonts: &[Vec<u8>], aliases: &[(String, String)]) -> FontBook {
        let mut db = Database::new();
        for bytes in fonts {
            db.load_font_data(bytes.clone());
        }
        let mut faces = Vec::new();
        for info in db.faces() {
            let mut names: Vec<String> = info.families.iter().map(|(n, _)| n.to_lowercase()).collect();
            names.push(info.post_script_name.to_lowercase());
            let metrics = db
                .with_face_data(info.id, |data, index| {
                    let face = ttf_parser::Face::parse(data, index).ok()?;
                    let upem = face.units_per_em() as f32;
                    // 윈도우 값(파워포인트가 쓰는 값)이 있으면 그것을 쓴다.
                    let (asc, desc) = match face.tables().os2 {
                        Some(os2) if os2.windows_ascender() > 0 => {
                            (os2.windows_ascender() as f32, os2.windows_descender().unsigned_abs() as f32)
                        }
                        _ => (face.ascender() as f32, (face.descender() as f32).abs()),
                    };
                    Some(Metrics { ascent: asc / upem, descent: desc / upem })
                })
                .flatten()
                .unwrap_or(Metrics { ascent: 0.9, descent: 0.25 });
            faces.push(Face {
                id: info.id,
                names,
                bold: info.weight.0 >= 600,
                italic: info.style != Style::Normal,
                metrics,
            });
        }
        let aliases = aliases.iter().map(|(a, b)| (a.to_lowercase(), b.clone())).collect();
        FontBook {
            db: Arc::new(db),
            faces,
            aliases,
            pick_cache: HashMap::new(),
            glyph_cache: HashMap::new(),
            fallback_cache: HashMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn id(&self, face: usize) -> Option<ID> {
        self.faces.get(face).map(|f| f.id)
    }

    pub fn metrics(&self, face: usize) -> Metrics {
        self.faces.get(face).map(|f| f.metrics).unwrap_or(Metrics { ascent: 0.9, descent: 0.25 })
    }

    pub fn is_bold(&self, face: usize) -> bool {
        self.faces.get(face).map(|f| f.bold).unwrap_or(false)
    }

    pub fn is_italic(&self, face: usize) -> bool {
        self.faces.get(face).map(|f| f.italic).unwrap_or(false)
    }

    /// 이름과 굵기·기울임에 맞는 얼굴.
    pub fn pick(&mut self, family: &str, bold: bool, italic: bool) -> usize {
        let key = (family.to_lowercase(), bold, italic);
        if let Some(hit) = self.pick_cache.get(&key) {
            return hit.unwrap_or(0);
        }
        let found = self.lookup(&key.0, bold, italic);
        self.pick_cache.insert(key, found);
        found.unwrap_or(0)
    }

    fn lookup(&self, family: &str, bold: bool, italic: bool) -> Option<usize> {
        if self.faces.is_empty() {
            return None;
        }
        // 굵게 원하면 "X bold" 별칭부터 본다(앱이 굵은 파일을 그 이름으로 넣는다).
        let mut candidates = Vec::new();
        if bold {
            if let Some(a) = self.aliases.get(&format!("{family} bold")) {
                candidates.push((a.to_lowercase(), true));
            }
        }
        if let Some(a) = self.aliases.get(family) {
            candidates.push((a.to_lowercase(), false));
        }
        candidates.push((family.to_owned(), false));
        for (name, bold_alias) in candidates {
            let matches: Vec<usize> =
                (0..self.faces.len()).filter(|&i| self.faces[i].names.iter().any(|n| *n == name)).collect();
            if matches.is_empty() {
                continue;
            }
            let want_bold = bold && !bold_alias || bold_alias;
            let best = matches
                .iter()
                .copied()
                .min_by_key(|&i| {
                    let f = &self.faces[i];
                    (f.bold != want_bold) as u8 * 2 + (f.italic != italic) as u8
                })
                .unwrap();
            if bold_alias {
                return Some(best);
            }
            return Some(best);
        }
        // 이름을 못 찾으면 fontdb 의 기본 질의로(같은 계열 이름이 다를 수 있다).
        let q = Query {
            families: &[Family::Name(family)],
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
            style: if italic { Style::Italic } else { Style::Normal },
            ..Default::default()
        };
        if let Some(id) = self.db.query(&q) {
            return self.faces.iter().position(|f| f.id == id);
        }
        None
    }

    /// 기본 얼굴(이름을 못 찾았을 때). 굵기를 맞춘다.
    pub fn default_face(&mut self, bold: bool, italic: bool) -> usize {
        let key = ("\u{0}default".to_owned(), bold, italic);
        if let Some(hit) = self.pick_cache.get(&key) {
            return hit.unwrap_or(0);
        }
        // 처음 넣은 글꼴(앱의 기본 글꼴)과 같은 이름 중 굵기가 맞는 것.
        let found = self.faces.first().map(|first| {
            let base = first.names.first().cloned().unwrap_or_default();
            (0..self.faces.len())
                .filter(|&i| self.faces[i].names.first() == Some(&base))
                .min_by_key(|&i| (self.faces[i].bold != bold) as u8 * 2 + (self.faces[i].italic != italic) as u8)
                .unwrap_or(0)
        });
        self.pick_cache.insert(key, found);
        found.unwrap_or(0)
    }

    pub fn resolve(&mut self, family: Option<&str>, bold: bool, italic: bool) -> usize {
        match family.filter(|f| !f.is_empty()) {
            Some(f) => {
                let key = (f.to_lowercase(), bold, italic);
                let found = match self.pick_cache.get(&key) {
                    Some(v) => *v,
                    None => {
                        let v = self.lookup(&key.0, bold, italic);
                        self.pick_cache.insert(key, v);
                        v
                    }
                };
                found.unwrap_or_else(|| self.default_face(bold, italic))
            }
            None => self.default_face(bold, italic),
        }
    }

    /// 글자 폭(em). 이 얼굴에 그 글자가 없으면 None.
    fn glyph(&mut self, face: usize, c: char) -> Option<f32> {
        if let Some(v) = self.glyph_cache.get(&(face, c)) {
            return *v;
        }
        let id = self.faces.get(face)?.id;
        let v = self
            .db
            .with_face_data(id, |data, index| {
                let f = ttf_parser::Face::parse(data, index).ok()?;
                let g = f.glyph_index(c)?;
                let adv = f.glyph_hor_advance(g).unwrap_or(0) as f32;
                Some(adv / f.units_per_em() as f32)
            })
            .flatten();
        self.glyph_cache.insert((face, c), v);
        v
    }

    /// 이 글자를 그릴 얼굴: 원하는 얼굴에 없으면 가진 얼굴로.
    pub fn face_for_char(&mut self, face: usize, c: char) -> usize {
        if c.is_whitespace() || (c as u32) < 0x20 {
            return face;
        }
        if self.glyph(face, c).is_some() {
            return face;
        }
        if let Some(&f) = self.fallback_cache.get(&(face, c)) {
            return f;
        }
        let (bold, italic) = (self.is_bold(face), self.is_italic(face));
        let mut best: Option<(u8, usize)> = None;
        for i in 0..self.faces.len() {
            if self.glyph(i, c).is_some() {
                let score = (self.faces[i].bold != bold) as u8 * 2 + (self.faces[i].italic != italic) as u8;
                if best.map(|b| score < b.0).unwrap_or(true) {
                    best = Some((score, i));
                    if score == 0 {
                        break;
                    }
                }
            }
        }
        let pick = best.map(|b| b.1).unwrap_or(face);
        self.fallback_cache.insert((face, c), pick);
        pick
    }

    /// 글자 폭 pt.
    pub fn advance(&mut self, face: usize, c: char, size: f64) -> f64 {
        if c == '\t' {
            return 0.0;
        }
        match self.glyph(face, c) {
            Some(v) => v as f64 * size,
            None => {
                if is_east_asian(c) {
                    size
                } else if c == ' ' {
                    size * 0.28
                } else {
                    size * 0.55
                }
            }
        }
    }

    /// usvg 가 `f<번호>` 를 그 얼굴로 고르게 하는 해결사.
    pub fn resolver(&self) -> usvg::FontResolver<'static> {
        let ids: Vec<ID> = self.faces.iter().map(|f| f.id).collect();
        let ids2 = ids.clone();
        usvg::FontResolver {
            select_font: Box::new(move |font, db| {
                for family in font.families() {
                    if let usvg::FontFamily::Named(name) = family {
                        if let Some(n) = name.strip_prefix('f').and_then(|s| s.parse::<usize>().ok()) {
                            if let Some(id) = ids.get(n) {
                                return Some(*id);
                            }
                        }
                    }
                }
                db.faces().next().map(|f| f.id)
            }),
            select_fallback: Box::new(move |c, used, db| {
                for id in &ids2 {
                    if used.contains(id) {
                        continue;
                    }
                    let has = db
                        .with_face_data(*id, |data, index| {
                            ttf_parser::Face::parse(data, index).ok().and_then(|f| f.glyph_index(c)).is_some()
                        })
                        .unwrap_or(false);
                    if has {
                        return Some(*id);
                    }
                }
                None
            }),
        }
    }
}
