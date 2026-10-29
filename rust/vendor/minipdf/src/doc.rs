//! 옛 워드(.doc, Word 97-2003 바이너리) 읽기.
//!
//! .doc 를 그리는 일을 따로 만들지 않는다. 본문·글자 모양·문단 모양·표·그림을
//! 읽어 docx 의 `word/document.xml` 모양으로 옮기고, 이미 있는 docx 경로에
//! 넘긴다. 그러면 쪽 나눔·표 그리기·글꼴 맞춤을 docx 와 똑같이 쓴다.
//!
//! 근거는 공개 명세 [MS-DOC] 다. 읽는 것:
//! - FIB: 본문 길이, 표 흐름(0Table/1Table) 자리
//! - CLX(조각표): 문자 위치(CP) → 파일 위치(FC), 8비트/16비트 구분
//! - CHPX/PAPX FKP: 글자 모양, 문단 모양(표 안인지, 행 끝인지 포함)
//! - STSH: 스타일(기본 글꼴·크기, 제목 스타일)
//! - SttbfFfn: 글꼴 이름표
//! - 첫 구역의 SEPX: 쪽 크기와 여백
//! - 그림: Data 흐름의 PICF 안 PNG/JPEG
//!
//! 안 읽는 것(그냥 건너뜀): 머리말·꼬리말, 각주, 글상자·떠 있는 도형, 번호 매기기.

use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, Write};

use crate::{MiniPdfError, Result};

const MAX_CHARS: u32 = 8_000_000;

fn bad(message: &str) -> MiniPdfError {
    MiniPdfError::InvalidInput(message.to_owned())
}

fn u8_at(b: &[u8], o: usize) -> Option<u8> {
    b.get(o).copied()
}
fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    u16_at(b, o).map(|v| v as i16)
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// CFB 안에 워드 본문 흐름이 있는지.
pub(crate) fn is_word_document(input: &[u8]) -> bool {
    match cfb::CompoundFile::open(Cursor::new(input)) {
        Ok(file) => file.is_stream("/WordDocument"),
        Err(_) => false,
    }
}

fn read_stream<F: Read + Seek>(file: &mut cfb::CompoundFile<F>, name: &str) -> Option<Vec<u8>> {
    if !file.is_stream(name) {
        return None;
    }
    let mut stream = file.open_stream(name).ok()?;
    let mut out = Vec::new();
    stream.read_to_end(&mut out).ok()?;
    Some(out)
}

// ---------------------------------------------------------------------------
// 모양

#[derive(Clone, Default, PartialEq)]
struct Chp {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    hidden: bool,
    /// 반 포인트 단위.
    size: u16,
    ftc_ascii: u16,
    ftc_east: u16,
    color: Option<[u8; 3]>,
    /// 그림 자리(Data 흐름 위치).
    pic: Option<u32>,
    special: bool,
    char_style: Option<u16>,
}

#[derive(Clone, Default)]
struct Pap {
    jc: u8,
    left: i32,
    right: i32,
    first: i32,
    before: u16,
    after: u16,
    /// (값, 배수 여부)
    line: Option<(i16, bool)>,
    in_table: bool,
    ttp: bool,
    itap: u16,
    /// 행 끝 문단에 붙은 칸 경계(트윕).
    centers: Vec<i16>,
    list: bool,
}

/// sprm 하나씩 넘겨 준다. (sprm, 피연산자)
fn each_sprm(grpprl: &[u8], mut f: impl FnMut(u16, &[u8])) {
    let mut i = 0usize;
    while i + 2 <= grpprl.len() {
        let sprm = u16::from_le_bytes([grpprl[i], grpprl[i + 1]]);
        i += 2;
        let size = match sprm >> 13 {
            0 | 1 => 1,
            2 | 4 | 5 => 2,
            3 => 4,
            7 => 3,
            _ => {
                if sprm == 0xD608 || sprm == 0xD606 {
                    // 표 정의는 길이가 두 바이트다(그 값 - 1 만큼 뒤따른다).
                    let Some(cb) = u16_at(grpprl, i) else { return };
                    i += 2;
                    (cb as usize).saturating_sub(1)
                } else {
                    let Some(cb) = u8_at(grpprl, i) else { return };
                    i += 1;
                    cb as usize
                }
            }
        };
        if i + size > grpprl.len() {
            return;
        }
        f(sprm, &grpprl[i..i + size]);
        i += size;
    }
}

fn toggle(current: bool, style: bool, value: u8) -> bool {
    match value {
        0 => false,
        1 => true,
        0x80 => style,
        0x81 => !style,
        _ => current,
    }
}

const ICO: [[u8; 3]; 17] = [
    [0, 0, 0], [0, 0, 0], [0, 0, 255], [0, 255, 255], [0, 255, 0], [255, 0, 255],
    [255, 0, 0], [255, 255, 0], [255, 255, 255], [0, 0, 128], [0, 128, 128],
    [0, 128, 0], [128, 0, 128], [128, 0, 0], [128, 128, 0], [128, 128, 128],
    [192, 192, 192],
];

fn apply_chp(chp: &mut Chp, base: &Chp, grpprl: &[u8]) {
    each_sprm(grpprl, |sprm, op| {
        let b = op.first().copied().unwrap_or(0);
        match sprm {
            0x0835 => chp.bold = toggle(chp.bold, base.bold, b),
            0x0836 => chp.italic = toggle(chp.italic, base.italic, b),
            0x0837 => chp.strike = toggle(chp.strike, base.strike, b),
            0x083C => chp.hidden = toggle(chp.hidden, base.hidden, b),
            0x2A3E => chp.underline = b != 0,
            0x4A43 => {
                if let Some(v) = u16_at(op, 0) {
                    if (2..=3276).contains(&v) {
                        chp.size = v;
                    }
                }
            }
            0x4A4F => chp.ftc_ascii = u16_at(op, 0).unwrap_or(chp.ftc_ascii),
            0x4A50 => chp.ftc_east = u16_at(op, 0).unwrap_or(chp.ftc_east),
            0x2A42 => {
                chp.color = if b == 0 { None } else { ICO.get(b as usize).copied() };
            }
            0x6870 => {
                if op.len() >= 4 {
                    // 넷째 바이트가 0xFF 면 '자동'.
                    chp.color = if op[3] == 0xFF { None } else { Some([op[0], op[1], op[2]]) };
                }
            }
            0x6A03 => chp.pic = u32_at(op, 0),
            0x0855 => chp.special = b != 0,
            0x4A30 => chp.char_style = u16_at(op, 0),
            _ => {}
        }
    });
}

fn apply_pap(pap: &mut Pap, grpprl: &[u8]) {
    each_sprm(grpprl, |sprm, op| {
        let b = op.first().copied().unwrap_or(0);
        match sprm {
            0x2403 | 0x2461 => pap.jc = b,
            0x840F | 0x845E => pap.left = i16_at(op, 0).unwrap_or(0) as i32,
            0x840E | 0x845D => pap.right = i16_at(op, 0).unwrap_or(0) as i32,
            0x8411 | 0x8460 => pap.first = i16_at(op, 0).unwrap_or(0) as i32,
            0xA413 => pap.before = u16_at(op, 0).unwrap_or(0),
            0xA414 => pap.after = u16_at(op, 0).unwrap_or(0),
            0x6412 => {
                if let (Some(v), Some(m)) = (i16_at(op, 0), i16_at(op, 2)) {
                    pap.line = Some((v, m != 0));
                }
            }
            0x2416 => pap.in_table = b != 0,
            0x2417 => pap.ttp = b != 0,
            0x244C => {
                if b != 0 {
                    pap.ttp = true;
                }
            }
            0x6649 => pap.itap = u32_at(op, 0).unwrap_or(0).min(64) as u16,
            0x460B => pap.list = u16_at(op, 0).unwrap_or(0) != 0,
            0xD608 => {
                // 칸 수, 그리고 칸 경계 (칸 수 + 1)개.
                if let Some(count) = op.first() {
                    let count = (*count as usize).min(63);
                    pap.centers = (0..=count).filter_map(|k| i16_at(op, 1 + k * 2)).collect();
                }
            }
            _ => {}
        }
    });
}

// ---------------------------------------------------------------------------
// 스타일

#[derive(Clone, Default)]
struct Style {
    #[allow(dead_code)]
    name: String,
    kind: u8,
    base: u16,
    papx: Vec<u8>,
    chpx: Vec<u8>,
}

struct Styles {
    styles: Vec<Option<Style>>,
    default_ftc: [u16; 3],
    resolved_chp: HashMap<u16, Chp>,
    resolved_pap: HashMap<u16, Pap>,
}

impl Styles {
    fn parse(table: &[u8], fc: u32, lcb: u32) -> Styles {
        let mut out = Styles { styles: Vec::new(), default_ftc: [0; 3], resolved_chp: HashMap::new(), resolved_pap: HashMap::new() };
        let start = fc as usize;
        let end = start.saturating_add(lcb as usize).min(table.len());
        if lcb < 4 || start >= end {
            return out;
        }
        let s = &table[start..end];
        let Some(cb_stshi) = u16_at(s, 0) else { return out };
        let stshi = 2usize;
        let cstd = u16_at(s, stshi).unwrap_or(0) as usize;
        let cb_base = u16_at(s, stshi + 2).unwrap_or(10) as usize;
        for k in 0..3 {
            out.default_ftc[k] = u16_at(s, stshi + 12 + k * 2).unwrap_or(0);
        }
        let mut pos = stshi + cb_stshi as usize;
        for _ in 0..cstd.min(4096) {
            let Some(cb) = u16_at(s, pos) else { break };
            pos += 2;
            let std_start = pos;
            let std_end = (pos + cb as usize).min(s.len());
            pos += cb as usize;
            if cb == 0 || std_end <= std_start {
                out.styles.push(None);
                continue;
            }
            let std = &s[std_start..std_end];
            let w2 = u16_at(std, 2).unwrap_or(0);
            let w3 = u16_at(std, 4).unwrap_or(0);
            let kind = (w2 & 0x000F) as u8;
            let base = w2 >> 4;
            let cupx = (w3 & 0x000F) as usize;
            let mut p = cb_base;
            let cch = u16_at(std, p).unwrap_or(0) as usize;
            p += 2;
            let name: Vec<u16> = (0..cch.min(255)).filter_map(|k| u16_at(std, p + k * 2)).collect();
            p += cch * 2 + 2;
            let mut upx: Vec<Vec<u8>> = Vec::new();
            for _ in 0..cupx.min(3) {
                if p % 2 == 1 {
                    p += 1;
                }
                let Some(len) = u16_at(std, p) else { break };
                p += 2;
                let end = (p + len as usize).min(std.len());
                upx.push(std.get(p..end).unwrap_or(&[]).to_vec());
                p += len as usize;
            }
            let mut style = Style { name: String::from_utf16_lossy(&name), kind, base, ..Default::default() };
            match kind {
                1 => {
                    // 문단 스타일: 첫째는 istd + 문단 sprm, 둘째는 글자 sprm.
                    if let Some(first) = upx.first() {
                        style.papx = first.get(2..).unwrap_or(&[]).to_vec();
                    }
                    if let Some(second) = upx.get(1) {
                        style.chpx = second.clone();
                    }
                }
                2 => {
                    if let Some(first) = upx.first() {
                        style.chpx = first.clone();
                    }
                }
                _ => {}
            }
            out.styles.push(Some(style));
        }
        out
    }

    fn default_chp(&self) -> Chp {
        Chp { size: 20, ftc_ascii: self.default_ftc[0], ftc_east: self.default_ftc[1], ..Default::default() }
    }

    fn chain(&self, istd: u16) -> Vec<&Style> {
        let mut chain = Vec::new();
        let mut cur = istd;
        for _ in 0..12 {
            let Some(Some(style)) = self.styles.get(cur as usize) else { break };
            chain.push(style);
            if style.base == 0x0FFF || style.base == cur {
                break;
            }
            cur = style.base;
        }
        chain.reverse();
        chain
    }

    fn chp(&mut self, istd: u16) -> Chp {
        if let Some(c) = self.resolved_chp.get(&istd) {
            return c.clone();
        }
        let mut chp = self.default_chp();
        for style in self.chain(istd) {
            let base = chp.clone();
            apply_chp(&mut chp, &base, &style.chpx);
        }
        chp.pic = None;
        self.resolved_chp.insert(istd, chp.clone());
        chp
    }

    fn pap(&mut self, istd: u16) -> Pap {
        if let Some(p) = self.resolved_pap.get(&istd) {
            return p.clone();
        }
        let mut pap = Pap::default();
        for style in self.chain(istd) {
            apply_pap(&mut pap, &style.papx);
        }
        pap.in_table = false;
        pap.ttp = false;
        pap.itap = 0;
        pap.centers.clear();
        self.resolved_pap.insert(istd, pap.clone());
        pap
    }

    fn char_style_chpx(&self, istd: u16) -> Vec<Vec<u8>> {
        self.chain(istd).into_iter().filter(|s| s.kind == 2).map(|s| s.chpx.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// FKP

/// (fc 시작, fc 끝, grpprl)
type Runs = Vec<(u32, u32, Vec<u8>)>;

fn plc_pns(table: &[u8], fc: u32, lcb: u32) -> Vec<u32> {
    let start = fc as usize;
    let lcb = lcb as usize;
    if lcb < 12 || start.saturating_add(lcb) > table.len() {
        return Vec::new();
    }
    let plc = &table[start..start + lcb];
    let n = (lcb - 4) / 8;
    (0..n).filter_map(|k| u32_at(plc, (n + 1) * 4 + k * 4).map(|v| v & 0x3F_FFFF)).collect()
}

fn chpx_runs(word: &[u8], pns: &[u32]) -> Runs {
    let mut runs = Runs::new();
    for &pn in pns {
        let off = pn as usize * 512;
        let Some(page) = word.get(off..off + 512) else { continue };
        let crun = page[511] as usize;
        for k in 0..crun.min(101) {
            let (Some(a), Some(b)) = (u32_at(page, k * 4), u32_at(page, k * 4 + 4)) else { break };
            let at = page.get((crun + 1) * 4 + k).copied().unwrap_or(0) as usize * 2;
            let grpprl = if at == 0 || at >= 511 {
                Vec::new()
            } else {
                let cb = page[at] as usize;
                page.get(at + 1..(at + 1 + cb).min(511)).unwrap_or(&[]).to_vec()
            };
            runs.push((a, b, grpprl));
        }
    }
    runs.sort_by_key(|r| r.0);
    runs
}

/// (fc 시작, fc 끝, istd, grpprl)
fn papx_runs(word: &[u8], pns: &[u32]) -> Vec<(u32, u32, u16, Vec<u8>)> {
    let mut runs = Vec::new();
    for &pn in pns {
        let off = pn as usize * 512;
        let Some(page) = word.get(off..off + 512) else { continue };
        let cpara = page[511] as usize;
        for k in 0..cpara.min(101) {
            let (Some(a), Some(b)) = (u32_at(page, k * 4), u32_at(page, k * 4 + 4)) else { break };
            let at = page.get((cpara + 1) * 4 + k * 13).copied().unwrap_or(0) as usize * 2;
            if at == 0 || at >= 511 {
                runs.push((a, b, 0, Vec::new()));
                continue;
            }
            let cb = page[at] as usize;
            let (start, len) = if cb == 0 {
                (at + 2, page.get(at + 1).copied().unwrap_or(0) as usize * 2)
            } else {
                (at + 1, (cb * 2).saturating_sub(1))
            };
            let body = page.get(start..(start + len).min(511)).unwrap_or(&[]);
            let istd = u16_at(body, 0).unwrap_or(0);
            runs.push((a, b, istd, body.get(2..).unwrap_or(&[]).to_vec()));
        }
    }
    runs.sort_by_key(|r| r.0);
    runs
}

fn find_run<T>(runs: &[(u32, u32, T)], fc: u32) -> Option<&T> {
    let idx = runs.partition_point(|r| r.0 <= fc);
    let r = runs.get(idx.checked_sub(1)?)?;
    (fc < r.1).then_some(&r.2)
}

// ---------------------------------------------------------------------------
// 글꼴 이름표

fn font_names(table: &[u8], fc: u32, lcb: u32) -> Vec<String> {
    let start = fc as usize;
    let end = start.saturating_add(lcb as usize).min(table.len());
    if lcb < 4 || start >= end {
        return Vec::new();
    }
    let s = &table[start..end];
    let mut count = u16_at(s, 0).unwrap_or(0) as usize;
    let mut i = 4;
    if count == 0xFFFF {
        count = u16_at(s, 2).unwrap_or(0) as usize;
        i = 6;
    }
    let mut names = Vec::new();
    for _ in 0..count.min(1024) {
        let Some(cb) = u8_at(s, i) else { break };
        let ffn_end = (i + 1 + cb as usize).min(s.len());
        let mut name = Vec::new();
        let mut p = i + 40;
        while p + 1 < ffn_end {
            let ch = u16_at(s, p).unwrap_or(0);
            if ch == 0 {
                break;
            }
            name.push(ch);
            p += 2;
        }
        names.push(String::from_utf16_lossy(&name));
        i += 1 + cb as usize;
    }
    names
}

// ---------------------------------------------------------------------------
// 본문

struct Word {
    word: Vec<u8>,
    table: Vec<u8>,
    data: Option<Vec<u8>>,
    fclcb: usize,
    cb_fclcb: usize,
    ccp_text: u32,
}

impl Word {
    fn open(input: &[u8]) -> Result<Word> {
        let mut file = cfb::CompoundFile::open(Cursor::new(input)).map_err(|_| bad("not a CFB file"))?;
        let word = read_stream(&mut file, "/WordDocument").ok_or_else(|| bad("no WordDocument stream"))?;
        if u16_at(&word, 0) != Some(0xA5EC) {
            return Err(bad("not a Word 97+ document"));
        }
        let flags = u16_at(&word, 0x0A).unwrap_or(0);
        if flags & 0x0100 != 0 {
            crate::notice("암호가 걸린 문서는 열 수 없습니다.".to_owned());
            return Err(bad("encrypted document"));
        }
        let table_name = if flags & 0x0200 != 0 { "/1Table" } else { "/0Table" };
        let table = read_stream(&mut file, table_name).ok_or_else(|| bad("no table stream"))?;
        let data = read_stream(&mut file, "/Data");

        let mut pos = 32usize;
        let csw = u16_at(&word, pos).ok_or_else(|| bad("short FIB"))? as usize;
        pos += 2 + csw * 2;
        let cslw = u16_at(&word, pos).ok_or_else(|| bad("short FIB"))? as usize;
        let lw = pos + 2;
        let ccp_text = u32_at(&word, lw + 12).unwrap_or(0);
        pos = lw + cslw * 4;
        let cb_fclcb = u16_at(&word, pos).ok_or_else(|| bad("short FIB"))? as usize;
        Ok(Word { word, table, data, fclcb: pos + 2, cb_fclcb, ccp_text })
    }

    fn fc_lcb(&self, index: usize) -> (u32, u32) {
        if index >= self.cb_fclcb {
            return (0, 0);
        }
        let at = self.fclcb + index * 8;
        (u32_at(&self.word, at).unwrap_or(0), u32_at(&self.word, at + 4).unwrap_or(0))
    }

    /// 본문 글자와 그 파일 위치.
    fn characters(&self) -> Result<Vec<(u16, u32)>> {
        let (fc, lcb) = self.fc_lcb(33);
        let start = fc as usize;
        let end = start.saturating_add(lcb as usize);
        let clx = self.table.get(start..end).ok_or_else(|| bad("bad CLX"))?;
        let mut i = 0usize;
        let plc = loop {
            match clx.get(i) {
                Some(1) => {
                    let cb = i16_at(clx, i + 1).unwrap_or(0).max(0) as usize;
                    i += 3 + cb;
                }
                Some(2) => {
                    let len = u32_at(clx, i + 1).ok_or_else(|| bad("bad Pcdt"))? as usize;
                    break clx.get(i + 5..i + 5 + len).ok_or_else(|| bad("bad Pcdt"))?;
                }
                _ => return Err(bad("bad CLX")),
            }
        };
        let n = plc.len().saturating_sub(4) / 12;
        let limit = self.ccp_text.min(MAX_CHARS);
        let mut chars = Vec::with_capacity(limit as usize);
        for k in 0..n {
            let (Some(cp0), Some(cp1)) = (u32_at(plc, k * 4), u32_at(plc, k * 4 + 4)) else { break };
            let pcd = (n + 1) * 4 + k * 8;
            let raw = u32_at(plc, pcd + 2).unwrap_or(0);
            let compressed = raw & 0x4000_0000 != 0;
            let base = if compressed { (raw & 0x3FFF_FFFF) / 2 } else { raw };
            for cp in cp0..cp1 {
                if cp >= limit {
                    break;
                }
                let offset = cp - cp0;
                if compressed {
                    let at = base + offset;
                    let Some(b) = self.word.get(at as usize) else { break };
                    chars.push((cp1252(*b), at));
                } else {
                    let at = base + offset * 2;
                    let Some(ch) = u16_at(&self.word, at as usize) else { break };
                    chars.push((ch, at));
                }
            }
        }
        Ok(chars)
    }

    fn page_setup(&self) -> (i32, i32, [i32; 4]) {
        // 기본: A4, 위아래 1440, 좌우 1800(워드 기본값).
        let mut size = (11906, 16838);
        let mut margins = [1440, 1800, 1440, 1800];
        let (fc, lcb) = self.fc_lcb(6);
        let start = fc as usize;
        if lcb >= 16 {
            if let Some(plc) = self.table.get(start..start + lcb as usize) {
                let n = (plc.len() - 4) / 16;
                if n > 0 {
                    let sed = (n + 1) * 4;
                    let fc_sepx = u32_at(plc, sed + 2).unwrap_or(u32::MAX);
                    if fc_sepx != u32::MAX {
                        let at = fc_sepx as usize;
                        let cb = i16_at(&self.word, at).unwrap_or(0).max(0) as usize;
                        if let Some(g) = self.word.get(at + 2..at + 2 + cb) {
                            each_sprm(g, |sprm, op| {
                                let v = i16_at(op, 0).map(|v| v as i32);
                                let u = u16_at(op, 0).map(|v| v as i32);
                                match sprm {
                                    0xB01F => size.0 = u.unwrap_or(size.0),
                                    0xB020 => size.1 = u.unwrap_or(size.1),
                                    0xB021 => margins[3] = u.unwrap_or(margins[3]),
                                    0xB022 => margins[1] = u.unwrap_or(margins[1]),
                                    0x9023 => margins[0] = v.unwrap_or(margins[0]).abs(),
                                    0x9024 => margins[2] = v.unwrap_or(margins[2]).abs(),
                                    _ => {}
                                }
                            });
                        }
                    }
                }
            }
        }
        if !(1440..=40000).contains(&size.0) || !(1440..=40000).contains(&size.1) {
            size = (11906, 16838);
        }
        (size.0, size.1, margins)
    }

    /// PICF 안에서 PNG 나 JPEG 를 꺼낸다. (바이트, 확장자, 폭, 높이 트윕)
    fn picture(&self, at: u32) -> Option<(Vec<u8>, &'static str, i32, i32)> {
        let data = self.data.as_ref()?;
        let start = at as usize;
        let lcb = u32_at(data, start)? as usize;
        let cb_header = u16_at(data, start + 4)? as usize;
        let end = start.checked_add(lcb)?.min(data.len());
        let pic = data.get(start..end)?;
        let dxa = i16_at(pic, 28)? as i32;
        let dya = i16_at(pic, 30)? as i32;
        let mx = u16_at(pic, 32).unwrap_or(1000) as i32;
        let my = u16_at(pic, 34).unwrap_or(1000) as i32;
        let mx = if mx == 0 { 1000 } else { mx };
        let my = if my == 0 { 1000 } else { my };
        let w = dxa * mx / 1000;
        let h = dya * my / 1000;
        let mut i = cb_header;
        // mm == 0x66 이면 이름이 한 칸 들어 있다.
        if u16_at(pic, 6) == Some(0x66) {
            i += 1 + pic.get(i).copied().unwrap_or(0) as usize;
        }
        // 오피스아트 레코드를 따라가며 BLIP 을 찾는다.
        let mut depth = 0;
        while i + 8 <= pic.len() && depth < 4096 {
            depth += 1;
            let ver_inst = u16_at(pic, i)?;
            let kind = u16_at(pic, i + 2)?;
            let len = u32_at(pic, i + 4)? as usize;
            let ver = ver_inst & 0x000F;
            let inst = ver_inst >> 4;
            let body = i + 8;
            match kind {
                0xF01D | 0xF01E | 0xF02A => {
                    let uids = if inst & 1 == 1 { 32 } else { 16 };
                    let img = pic.get(body + uids + 1..(body + len).min(pic.len()))?;
                    let ext = if kind == 0xF01E { "png" } else { "jpeg" };
                    return Some((img.to_vec(), ext, w, h));
                }
                _ if ver == 0xF => i = body, // 담는 레코드: 안으로
                0xF007 => i = body + 36 + pic.get(body + 33).copied().unwrap_or(0) as usize,
                _ => i = body + len,
            }
        }
        None
    }
}

fn cp1252(b: u8) -> u16 {
    const HIGH: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&b) { HIGH[(b - 0x80) as usize] } else { b as u16 }
}

// ---------------------------------------------------------------------------
// docx 로 쓰기

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

struct Writer<'a> {
    fonts: &'a [String],
    images: Vec<(String, Vec<u8>)>,
}

impl Writer<'_> {
    fn rpr(&self, chp: &Chp) -> String {
        let mut s = String::from("<w:rPr>");
        let ascii = self.fonts.get(chp.ftc_ascii as usize).filter(|n| !n.is_empty());
        let east = self.fonts.get(chp.ftc_east as usize).filter(|n| !n.is_empty()).or(ascii);
        if ascii.is_some() || east.is_some() {
            s.push_str("<w:rFonts");
            if let Some(a) = ascii {
                s.push_str(&format!(" w:ascii=\"{0}\" w:hAnsi=\"{0}\"", esc(a)));
            }
            if let Some(e) = east {
                s.push_str(&format!(" w:eastAsia=\"{}\"", esc(e)));
            }
            s.push_str("/>");
        }
        if chp.bold {
            s.push_str("<w:b/>");
        }
        if chp.italic {
            s.push_str("<w:i/>");
        }
        if chp.strike {
            s.push_str("<w:strike/>");
        }
        if chp.underline {
            s.push_str("<w:u w:val=\"single\"/>");
        }
        if let Some([r, g, b]) = chp.color {
            s.push_str(&format!("<w:color w:val=\"{r:02X}{g:02X}{b:02X}\"/>"));
        }
        s.push_str(&format!("<w:sz w:val=\"{0}\"/><w:szCs w:val=\"{0}\"/>", chp.size));
        s.push_str("</w:rPr>");
        s
    }

    fn ppr(pap: &Pap) -> String {
        let mut s = String::from("<w:pPr>");
        let jc = match pap.jc {
            1 => Some("center"),
            2 => Some("right"),
            3 | 4 => Some("both"),
            _ => None,
        };
        if pap.before > 0 || pap.after > 0 || pap.line.is_some() {
            s.push_str(&format!("<w:spacing w:before=\"{}\" w:after=\"{}\"", pap.before, pap.after));
            match pap.line {
                Some((v, true)) if v > 0 => s.push_str(&format!(" w:line=\"{v}\" w:lineRule=\"auto\"")),
                Some((v, false)) if v < 0 => s.push_str(&format!(" w:line=\"{}\" w:lineRule=\"exact\"", -(v as i32))),
                Some((v, false)) if v > 0 => s.push_str(&format!(" w:line=\"{v}\" w:lineRule=\"atLeast\"")),
                _ => {}
            }
            s.push_str("/>");
        }
        if pap.left != 0 || pap.right != 0 || pap.first != 0 {
            s.push_str(&format!("<w:ind w:left=\"{}\" w:right=\"{}\"", pap.left.max(0), pap.right.max(0)));
            if pap.first >= 0 {
                s.push_str(&format!(" w:firstLine=\"{}\"/>", pap.first));
            } else {
                s.push_str(&format!(" w:hanging=\"{}\"/>", -pap.first));
            }
        }
        if let Some(jc) = jc {
            s.push_str(&format!("<w:jc w:val=\"{jc}\"/>"));
        }
        s.push_str("</w:pPr>");
        s
    }

    fn image(&mut self, bytes: Vec<u8>, ext: &str, w: i32, h: i32) -> String {
        let id = format!("rIdImg{}", self.images.len() + 1);
        let path = format!("media/image{}.{ext}", self.images.len() + 1);
        self.images.push((path, bytes));
        let cx = (w.max(1) as i64) * 635;
        let cy = (h.max(1) as i64) * 635;
        format!(
            "<w:r><w:drawing><wp:inline><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><a:graphic><a:graphicData><a:blip r:embed=\"{id}\"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Field {
    Code,
    Result,
}

struct Paragraph {
    xml: String,
}

/// .doc 를 docx 바이트로.
pub fn doc_to_docx(input: &[u8]) -> Result<Vec<u8>> {
    let doc = Word::open(input)?;
    let chars = doc.characters()?;
    let fonts = {
        let (fc, lcb) = doc.fc_lcb(15);
        font_names(&doc.table, fc, lcb)
    };
    let mut styles = {
        let (fc, lcb) = doc.fc_lcb(1);
        Styles::parse(&doc.table, fc, lcb)
    };
    let chpx = {
        let (fc, lcb) = doc.fc_lcb(12);
        chpx_runs(&doc.word, &plc_pns(&doc.table, fc, lcb))
    };
    let papx = {
        let (fc, lcb) = doc.fc_lcb(13);
        papx_runs(&doc.word, &plc_pns(&doc.table, fc, lcb))
    };

    let mut writer = Writer { fonts: &fonts, images: Vec::new() };
    let mut body = String::new();

    // 지금 문단의 글자들 (글자, 모양)
    let mut pending: Vec<(u16, Chp)> = Vec::new();
    let mut fields: Vec<Field> = Vec::new();
    // 표: 칸들(각 칸은 문단 XML 묶음), 행들
    let mut cell: Vec<Paragraph> = Vec::new();
    let mut row: Vec<Vec<Paragraph>> = Vec::new();
    let mut rows: Vec<(Vec<Vec<Paragraph>>, Vec<i16>)> = Vec::new();
    let mut pictures = 0usize;
    let mut tables = 0usize;

    let flush_table = |rows: &mut Vec<(Vec<Vec<Paragraph>>, Vec<i16>)>, body: &mut String, tables: &mut usize| {
        if rows.is_empty() {
            return;
        }
        *tables += 1;
        let cols = rows.iter().map(|r| r.0.len()).max().unwrap_or(1).max(1);
        // 가장 칸이 많은 행의 경계로 격자를 만든다.
        let grid: Vec<i32> = rows
            .iter()
            .find(|r| r.0.len() == cols && r.1.len() == cols + 1)
            .map(|r| r.1.windows(2).map(|w| (w[1] as i32 - w[0] as i32).max(200)).collect())
            .unwrap_or_else(|| vec![9000 / cols as i32; cols]);
        let total: i32 = grid.iter().sum();
        body.push_str(&format!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"{total}\" w:type=\"dxa\"/><w:tblBorders>\
             <w:top w:val=\"single\" w:sz=\"4\"/><w:left w:val=\"single\" w:sz=\"4\"/>\
             <w:bottom w:val=\"single\" w:sz=\"4\"/><w:right w:val=\"single\" w:sz=\"4\"/>\
             <w:insideH w:val=\"single\" w:sz=\"4\"/><w:insideV w:val=\"single\" w:sz=\"4\"/>\
             </w:tblBorders></w:tblPr><w:tblGrid>"
        ));
        for g in &grid {
            body.push_str(&format!("<w:gridCol w:w=\"{g}\"/>"));
        }
        body.push_str("</w:tblGrid>");
        for (cells, centers) in rows.drain(..) {
            body.push_str("<w:tr>");
            let widths: Vec<i32> = if centers.len() == cells.len() + 1 {
                centers.windows(2).map(|w| (w[1] as i32 - w[0] as i32).max(200)).collect()
            } else {
                vec![total / cells.len().max(1) as i32; cells.len()]
            };
            let n = cells.len();
            for (k, paragraphs) in cells.into_iter().enumerate() {
                let w = widths.get(k).copied().unwrap_or(1000);
                // 칸이 모자라는 행은 마지막 칸을 넓혀 격자를 채운다.
                let span = if k + 1 == n && n < cols { cols - n + 1 } else { 1 };
                body.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/>"));
                if span > 1 {
                    body.push_str(&format!("<w:gridSpan w:val=\"{span}\"/>"));
                }
                body.push_str("</w:tcPr>");
                if paragraphs.is_empty() {
                    body.push_str("<w:p/>");
                }
                for p in paragraphs {
                    body.push_str(&p.xml);
                }
                body.push_str("</w:tc>");
            }
            body.push_str("</w:tr>");
        }
        body.push_str("</w:tbl>");
    };

    let mut i = 0usize;
    while i < chars.len() {
        let (ch, fc) = chars[i];
        i += 1;
        let is_mark = ch == 0x0D || ch == 0x07;
        if !is_mark {
            match ch {
                0x13 => {
                    fields.push(Field::Code);
                    continue;
                }
                0x14 => {
                    if let Some(last) = fields.last_mut() {
                        *last = Field::Result;
                    }
                    continue;
                }
                0x15 => {
                    fields.pop();
                    continue;
                }
                _ => {}
            }
            if fields.iter().any(|f| *f == Field::Code) {
                continue;
            }
            // 글자 모양: 문단 스타일 → 글자 스타일 → 직접 모양
            let istd = find_papx_istd(&papx, fc);
            let base = styles.chp(istd);
            let mut chp = base.clone();
            if let Some(g) = find_run(&chpx, fc) {
                let mut probe = Chp::default();
                apply_chp(&mut probe, &base, g);
                if let Some(cs) = probe.char_style {
                    for g2 in styles.char_style_chpx(cs) {
                        let b = chp.clone();
                        apply_chp(&mut chp, &b, &g2);
                    }
                }
                apply_chp(&mut chp, &base, g);
            }
            if chp.hidden {
                continue;
            }
            pending.push((ch, chp));
            continue;
        }

        // 문단 끝
        let (istd, grpprl) = find_papx(&papx, fc);
        let mut pap = styles.pap(istd);
        if let Some(g) = grpprl {
            apply_pap(&mut pap, g);
        }
        let in_table = pap.in_table || pap.itap > 0;
        let xml = paragraph_xml(&mut writer, &doc, &pap, std::mem::take(&mut pending), &mut pictures);

        if in_table {
            if ch == 0x07 && pap.ttp {
                // 행 끝
                if !cell.is_empty() {
                    row.push(std::mem::take(&mut cell));
                }
                if !row.is_empty() {
                    rows.push((std::mem::take(&mut row), pap.centers.clone()));
                }
            } else if ch == 0x07 {
                cell.push(Paragraph { xml });
                row.push(std::mem::take(&mut cell));
            } else {
                cell.push(Paragraph { xml });
            }
            continue;
        }
        if !row.is_empty() || !cell.is_empty() {
            if !cell.is_empty() {
                row.push(std::mem::take(&mut cell));
            }
            rows.push((std::mem::take(&mut row), Vec::new()));
        }
        flush_table(&mut rows, &mut body, &mut tables);
        body.push_str(&xml);
    }
    if !pending.is_empty() {
        let pap = Pap::default();
        let xml = paragraph_xml(&mut writer, &doc, &pap, std::mem::take(&mut pending), &mut pictures);
        body.push_str(&xml);
    }
    if !cell.is_empty() {
        row.push(std::mem::take(&mut cell));
    }
    if !row.is_empty() {
        rows.push((std::mem::take(&mut row), Vec::new()));
    }
    flush_table(&mut rows, &mut body, &mut tables);

    let (pw, ph, m) = doc.page_setup();
    crate::stat(format!("doc 글자 {}개 · 표 {tables}개 · 그림 {pictures}개", chars.len()));

    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{body}\
<w:sectPr><w:pgSz w:w=\"{pw}\" w:h=\"{ph}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\"/></w:sectPr>\
</w:body></w:document>",
        m[0], m[1], m[2], m[3]
    );

    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (k, (path, _)) in writer.images.iter().enumerate() {
        rels.push_str(&format!(
            "<Relationship Id=\"rIdImg{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{path}\"/>",
            k + 1
        ));
    }
    rels.push_str("</Relationships>");

    let mut output = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut output);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", options)?;
        zip.write_all(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>",
        )?;
        zip.start_file("word/document.xml", options)?;
        zip.write_all(document.as_bytes())?;
        zip.start_file("word/_rels/document.xml.rels", options)?;
        zip.write_all(rels.as_bytes())?;
        for (path, bytes) in &writer.images {
            zip.start_file(format!("word/{path}"), options)?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
    }
    Ok(output.into_inner())
}

fn find_papx(runs: &[(u32, u32, u16, Vec<u8>)], fc: u32) -> (u16, Option<&[u8]>) {
    let idx = runs.partition_point(|r| r.0 <= fc);
    match idx.checked_sub(1).and_then(|k| runs.get(k)) {
        Some(r) if fc < r.1 => (r.2, Some(&r.3)),
        _ => (0, None),
    }
}

fn find_papx_istd(runs: &[(u32, u32, u16, Vec<u8>)], fc: u32) -> u16 {
    find_papx(runs, fc).0
}

fn paragraph_xml(writer: &mut Writer<'_>, doc: &Word, pap: &Pap, chars: Vec<(u16, Chp)>, pictures: &mut usize) -> String {
    let mut xml = String::from("<w:p>");
    xml.push_str(&Writer::ppr(pap));
    let mut run_chp: Option<Chp> = None;
    let mut text: Vec<u16> = Vec::new();
    let mut runs = String::new();

    let flush = |runs: &mut String, run_chp: &Option<Chp>, text: &mut Vec<u16>, writer: &Writer<'_>| {
        if text.is_empty() {
            return;
        }
        if let Some(chp) = run_chp {
            let s = String::from_utf16_lossy(text);
            runs.push_str("<w:r>");
            runs.push_str(&writer.rpr(chp));
            runs.push_str(&format!("<w:t xml:space=\"preserve\">{}</w:t></w:r>", esc(&s)));
        }
        text.clear();
    };

    for (ch, chp) in chars {
        // 그림 자리
        if (ch == 0x01 || ch == 0x08) && chp.special {
            flush(&mut runs, &run_chp, &mut text, writer);
            if ch == 0x01 {
                if let Some(at) = chp.pic {
                    if let Some((bytes, ext, w, h)) = doc.picture(at) {
                        *pictures += 1;
                        let img = writer.image(bytes, ext, w, h);
                        runs.push_str(&img);
                    }
                }
            }
            continue;
        }
        let breaker = match ch {
            0x0B => Some("<w:br/>"),
            0x0C => Some("<w:br w:type=\"page\"/>"),
            0x09 => Some("<w:tab/>"),
            _ => None,
        };
        if let Some(tag) = breaker {
            flush(&mut runs, &run_chp, &mut text, writer);
            let chp = run_chp.clone().unwrap_or_else(|| chp.clone());
            runs.push_str(&format!("<w:r>{}{tag}</w:r>", writer.rpr(&chp)));
            continue;
        }
        let ch = match ch {
            0x1E => 0x2011,
            0x1F | 0x00..=0x08 | 0x0E..=0x1D => continue,
            0xF000..=0xF0FF => continue, // 기호 글꼴 자리(대부분 Symbol/Wingdings)
            _ => ch,
        };
        if run_chp.as_ref() != Some(&chp) {
            flush(&mut runs, &run_chp, &mut text, writer);
            run_chp = Some(chp);
        }
        text.push(ch);
    }
    flush(&mut runs, &run_chp, &mut text, writer);
    xml.push_str(&runs);
    xml.push_str("</w:p>");
    xml
}

/// .doc 가 쓰는 글꼴 이름들.
pub fn doc_font_names(input: &[u8]) -> Vec<String> {
    let Ok(doc) = Word::open(input) else { return Vec::new() };
    let (fc, lcb) = doc.fc_lcb(15);
    font_names(&doc.table, fc, lcb).into_iter().filter(|n| !n.is_empty()).collect()
}
