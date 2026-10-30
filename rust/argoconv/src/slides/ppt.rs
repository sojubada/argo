//! 옛 파워포인트(.ppt, PowerPoint 97-2003 바이너리) 읽기.
//!
//! 공개 명세 [MS-PPT]·[MS-ODRAW] 를 따라 레코드를 읽고, 새 렌더러가 읽는 pptx
//! 모양(XML 부분들)으로 옮긴다. 마스터의 글 모양은 각 글자에 풀어 적는다 —
//! 그러면 렌더러는 상속을 몰라도 된다. 마스터의 도형과 배경은 레이아웃으로 옮겨
//! 여러 쪽이 나눠 그린다.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use super::package::Package;

const MU_TO_EMU: f64 = 914400.0 / 576.0;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    u16_at(b, o).map(|v| v as i16)
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}
fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    u32_at(b, o).map(|v| v as i32)
}

#[derive(Clone, Copy)]
struct Rec<'a> {
    ver: u16,
    inst: u16,
    kind: u16,
    data: &'a [u8],
}

fn rec_at(buf: &[u8], off: usize) -> Option<Rec<'_>> {
    let vi = u16_at(buf, off)?;
    let kind = u16_at(buf, off + 2)?;
    let len = u32_at(buf, off + 4)? as usize;
    let start = off + 8;
    let end = start.checked_add(len)?.min(buf.len());
    Some(Rec { ver: vi & 0xF, inst: vi >> 4, kind, data: buf.get(start..end)? })
}

fn children(data: &[u8]) -> Vec<Rec<'_>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 8 <= data.len() && out.len() < 200_000 {
        let Some(r) = rec_at(data, off) else { break };
        let len = u32_at(data, off + 4).unwrap_or(0) as usize;
        out.push(r);
        off += 8 + len;
    }
    out
}

fn find<'a>(data: &'a [u8], kind: u16) -> Option<Rec<'a>> {
    children(data).into_iter().find(|r| r.kind == kind)
}

pub(crate) fn is_ppt(bytes: &[u8]) -> bool {
    if !bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return false;
    }
    cfb::CompoundFile::open(Cursor::new(bytes)).map(|f| f.is_stream("/PowerPoint Document")).unwrap_or(false)
}

fn stream(file: &mut cfb::CompoundFile<Cursor<&[u8]>>, name: &str) -> Option<Vec<u8>> {
    if !file.is_stream(name) {
        return None;
    }
    let mut s = file.open_stream(name).ok()?;
    let mut v = Vec::new();
    s.read_to_end(&mut v).ok()?;
    Some(v)
}

// ---------------------------------------------------------------------------
// 글 모양

#[derive(Clone, Copy, Debug, PartialEq)]
enum Col {
    Rgb(u8, u8, u8),
    Scheme(u8),
}

#[derive(Clone, Default, Debug)]
struct Pf {
    has_bullet: Option<bool>,
    bullet_char: Option<u16>,
    bullet_font: Option<u16>,
    bullet_size: Option<i16>,
    bullet_color: Option<Col>,
    bullet_has_color: Option<bool>,
    bullet_has_font: Option<bool>,
    bullet_has_size: Option<bool>,
    align: Option<u16>,
    line: Option<i16>,
    before: Option<i16>,
    after: Option<i16>,
    left: Option<i16>,
    indent: Option<i16>,
}

impl Pf {
    fn merge(&mut self, o: &Pf) {
        macro_rules! m { ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f; } )* }; }
        m!(has_bullet, bullet_char, bullet_font, bullet_size, bullet_color, bullet_has_color, bullet_has_font, bullet_has_size, align, line, before, after, left, indent);
    }
}

#[derive(Clone, Default, Debug)]
struct Cf {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    font: Option<u16>,
    ea_font: Option<u16>,
    size: Option<u16>,
    color: Option<Col>,
    position: Option<i16>,
}

impl Cf {
    fn merge(&mut self, o: &Cf) {
        macro_rules! m { ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f; } )* }; }
        m!(bold, italic, underline, font, ea_font, size, color, position);
    }
}

fn color_index(b: &[u8], o: usize) -> Option<Col> {
    let (r, g, bl, i) = (*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?);
    Some(match i {
        0xFE => Col::Rgb(r, g, bl),
        0..=7 => Col::Scheme(i),
        _ => Col::Rgb(r, g, bl),
    })
}

/// TextPFException. 읽은 뒤의 위치를 돌려준다.
fn read_pf(b: &[u8], mut o: usize) -> Option<(Pf, usize)> {
    let m = u32_at(b, o)?;
    o += 4;
    let mut p = Pf::default();
    if m & 0xF != 0 {
        let flags = u16_at(b, o)?;
        o += 2;
        if m & 1 != 0 {
            p.has_bullet = Some(flags & 1 != 0);
        }
        if m & 2 != 0 {
            p.bullet_has_font = Some(flags & 2 != 0);
        }
        if m & 4 != 0 {
            p.bullet_has_color = Some(flags & 4 != 0);
        }
        if m & 8 != 0 {
            p.bullet_has_size = Some(flags & 8 != 0);
        }
    }
    if m & 0x80 != 0 {
        p.bullet_char = u16_at(b, o);
        o += 2;
    }
    if m & 0x10 != 0 {
        p.bullet_font = u16_at(b, o);
        o += 2;
    }
    if m & 0x40 != 0 {
        p.bullet_size = i16_at(b, o);
        o += 2;
    }
    if m & 0x20 != 0 {
        p.bullet_color = color_index(b, o);
        o += 4;
    }
    if m & 0x800 != 0 {
        p.align = u16_at(b, o);
        o += 2;
    }
    if m & 0x1000 != 0 {
        p.line = i16_at(b, o);
        o += 2;
    }
    if m & 0x2000 != 0 {
        p.before = i16_at(b, o);
        o += 2;
    }
    if m & 0x4000 != 0 {
        p.after = i16_at(b, o);
        o += 2;
    }
    if m & 0x100 != 0 {
        p.left = i16_at(b, o);
        o += 2;
    }
    if m & 0x400 != 0 {
        p.indent = i16_at(b, o);
        o += 2;
    }
    if m & 0x8000 != 0 {
        o += 2;
    }
    if m & 0x100000 != 0 {
        let n = u16_at(b, o)? as usize;
        o += 2 + n * 4;
    }
    if m & 0x10000 != 0 {
        o += 2;
    }
    if m & 0xE0000 != 0 {
        o += 2;
    }
    if m & 0x200000 != 0 {
        o += 2;
    }
    Some((p, o))
}

fn read_cf(b: &[u8], mut o: usize) -> Option<(Cf, usize)> {
    let m = u32_at(b, o)?;
    o += 4;
    let mut c = Cf::default();
    if m & 0xFFFF != 0 && m & (0x1 | 0x2 | 0x4 | 0x10 | 0x20 | 0x80 | 0x200 | 0x3C00) != 0 {
        let st = u16_at(b, o)?;
        o += 2;
        if m & 1 != 0 {
            c.bold = Some(st & 1 != 0);
        }
        if m & 2 != 0 {
            c.italic = Some(st & 2 != 0);
        }
        if m & 4 != 0 {
            c.underline = Some(st & 4 != 0);
        }
    }
    if m & 0x10000 != 0 {
        c.font = u16_at(b, o);
        o += 2;
    }
    if m & 0x200000 != 0 {
        c.ea_font = u16_at(b, o);
        o += 2;
    }
    if m & 0x400000 != 0 {
        o += 2;
    }
    if m & 0x800000 != 0 {
        o += 2;
    }
    if m & 0x20000 != 0 {
        c.size = u16_at(b, o);
        o += 2;
    }
    if m & 0x40000 != 0 {
        c.color = color_index(b, o);
        o += 4;
    }
    if m & 0x80000 != 0 {
        c.position = i16_at(b, o);
        o += 2;
    }
    if m & 0x100000 != 0 {
        o += 4;
    }
    if m & 0x1000000 != 0 {
        if c.ea_font.is_none() {
            c.ea_font = u16_at(b, o);
        }
        o += 2;
    }
    if m & 0x2000000 != 0 {
        o += 2;
    }
    if m & 0x4000000 != 0 {
        o += 4;
    }
    Some((c, o))
}

/// 마스터 글 모양: 글 종류(instance) → 단계별 (Pf, Cf)
type MasterStyles = HashMap<u16, Vec<(Pf, Cf)>>;

fn read_master_style(r: &Rec<'_>) -> Vec<(Pf, Cf)> {
    let b = r.data;
    let Some(n) = u16_at(b, 0) else { return Vec::new() };
    let mut o = 2;
    let mut out = Vec::new();
    for _ in 0..n.min(5) {
        if r.inst >= 5 {
            o += 2;
        }
        let Some((pf, o2)) = read_pf(b, o) else { break };
        let Some((cf, o3)) = read_cf(b, o2) else { break };
        out.push((pf, cf));
        o = o3;
    }
    out
}

// ---------------------------------------------------------------------------
// 도형

#[derive(Default, Clone)]
struct Props {
    map: HashMap<u16, u32>,
    complex: HashMap<u16, Vec<u8>>,
}

impl Props {
    fn read(r: &Rec<'_>, into: &mut Props) {
        let n = r.inst as usize;
        let mut cplx_off = n * 6;
        for k in 0..n {
            let Some(id) = u16_at(r.data, k * 6) else { break };
            let Some(v) = u32_at(r.data, k * 6 + 2) else { break };
            let pid = id & 0x3FFF;
            into.map.insert(pid, v);
            if id & 0x8000 != 0 {
                let end = (cplx_off + v as usize).min(r.data.len());
                if let Some(d) = r.data.get(cplx_off..end) {
                    into.complex.insert(pid, d.to_vec());
                }
                cplx_off = end;
            }
        }
    }
    fn get(&self, id: u16) -> Option<u32> {
        self.map.get(&id).copied()
    }
    /// 불 값 묶음의 한 비트 (use 비트가 서 있을 때만)
    fn flag(&self, id: u16, bit: u32) -> Option<bool> {
        let v = self.get(id)?;
        if v & (bit << 16) != 0 {
            Some(v & bit != 0)
        } else {
            None
        }
    }
}

struct TextBlock<'a> {
    kind: u32,
    text: Vec<u16>,
    style: Option<&'a [u8]>,
    slide_num_at: Vec<usize>,
}

fn read_text_records<'a>(recs: &[Rec<'a>]) -> Vec<TextBlock<'a>> {
    let mut out: Vec<TextBlock> = Vec::new();
    for r in recs {
        match r.kind {
            0x0F9F => out.push(TextBlock { kind: u32_at(r.data, 0).unwrap_or(4), text: Vec::new(), style: None, slide_num_at: Vec::new() }),
            0x0FA0 => {
                if let Some(b) = out.last_mut() {
                    b.text = r.data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                }
            }
            0x0FA8 => {
                if let Some(b) = out.last_mut() {
                    b.text = r.data.iter().map(|&c| c as u16).collect();
                }
            }
            0x0FA1 => {
                if let Some(b) = out.last_mut() {
                    b.style = Some(r.data);
                }
            }
            0x0FF8 => {
                if let Some(b) = out.last_mut() {
                    if let Some(p) = u32_at(r.data, 0) {
                        b.slide_num_at.push(p as usize);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

struct Ctx<'a> {
    fonts: Vec<String>,
    scheme: [u32; 8],
    styles: &'a MasterStyles,
    blips: &'a [Option<(Vec<u8>, &'static str)>],
    media: &'a mut Vec<(String, Vec<u8>)>,
    rels: Vec<(String, String)>,
    next_id: u32,
    slide_texts: &'a [TextBlock<'a>],
    /// 마스터 도형의 속성 (spid → 속성). 쪽의 도형이 이것을 물려받는다.
    master_props: &'a HashMap<u32, Props>,
    /// 마스터를 읽는 중이면 여기에 모은다.
    collect: Option<HashMap<u32, Props>>,
}

fn scheme_rgb(scheme: &[u32; 8], i: u8) -> (u8, u8, u8) {
    let v = scheme[(i as usize).min(7)];
    (v as u8, (v >> 8) as u8, (v >> 16) as u8)
}

fn hex(c: (u8, u8, u8)) -> String {
    format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2)
}

/// OfficeArtCOLORREF → RGB
fn art_color(v: u32, scheme: &[u32; 8], fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    let flags = (v >> 24) as u8;
    let (r, g, b) = (v as u8, (v >> 8) as u8, (v >> 16) as u8);
    if flags & 0x08 != 0 {
        return scheme_rgb(scheme, r);
    }
    if flags & 0x10 != 0 {
        return fallback;
    }
    (r, g, b)
}

fn col_rgb(c: Col, scheme: &[u32; 8]) -> (u8, u8, u8) {
    match c {
        Col::Rgb(r, g, b) => (r, g, b),
        Col::Scheme(i) => scheme_rgb(scheme, i),
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            '\u{FFFE}' | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

fn prst_name(t: u16) -> &'static str {
    match t {
        1 | 75 | 202 | 109 => "rect",
        2 => "roundRect",
        3 | 120 => "ellipse",
        4 | 110 => "diamond",
        5 => "triangle",
        6 => "rtTriangle",
        7 | 111 => "parallelogram",
        8 => "trapezoid",
        9 => "hexagon",
        10 => "octagon",
        11 => "plus",
        12 => "star5",
        13 | 14 => "rightArrow",
        15 => "homePlate",
        16 => "cube",
        17 => "wedgeRoundRectCallout",
        18 => "star16",
        19 => "arc",
        20 => "line",
        21 => "plaque",
        22 => "can",
        23 => "donut",
        32 => "straightConnector1",
        33 => "bentConnector2",
        34 => "bentConnector3",
        35 => "bentConnector4",
        36 => "bentConnector5",
        37 => "curvedConnector2",
        38 => "curvedConnector3",
        39 => "curvedConnector4",
        40 => "curvedConnector5",
        41 => "callout1",
        42 => "callout2",
        43 => "callout3",
        44 => "accentCallout1",
        45 => "accentCallout2",
        46 => "accentCallout3",
        47 => "borderCallout1",
        48 => "borderCallout2",
        49 => "borderCallout3",
        50 => "accentBorderCallout1",
        51 => "accentBorderCallout2",
        52 => "accentBorderCallout3",
        53 => "ribbon",
        54 => "ribbon2",
        55 => "chevron",
        56 => "pentagon",
        57 => "noSmoking",
        58 => "star8",
        59 => "star16",
        60 => "star32",
        61 => "wedgeRectCallout",
        62 => "wedgeRoundRectCallout",
        63 => "wedgeEllipseCallout",
        64 => "wave",
        65 => "foldedCorner",
        66 => "leftArrow",
        67 => "downArrow",
        68 => "upArrow",
        69 => "leftRightArrow",
        70 => "upDownArrow",
        71 => "irregularSeal1",
        72 => "irregularSeal2",
        73 => "lightningBolt",
        74 => "heart",
        76 => "quadArrow",
        77 => "leftArrowCallout",
        78 => "rightArrowCallout",
        79 => "upArrowCallout",
        80 => "downArrowCallout",
        81 => "leftRightArrowCallout",
        82 => "upDownArrowCallout",
        83 => "quadArrowCallout",
        84 => "bevel",
        85 => "leftBracket",
        86 => "rightBracket",
        87 => "leftBrace",
        88 => "rightBrace",
        89 => "leftUpArrow",
        90 => "bentUpArrow",
        91 => "bentArrow",
        92 => "star24",
        93 => "stripedRightArrow",
        94 => "notchedRightArrow",
        95 => "blockArc",
        96 => "smileyFace",
        97 => "verticalScroll",
        98 => "horizontalScroll",
        99 | 100 => "circularArrow",
        101 => "uturnArrow",
        102 => "curvedRightArrow",
        103 => "curvedLeftArrow",
        104 => "curvedUpArrow",
        105 => "curvedDownArrow",
        106 => "cloudCallout",
        107 => "ellipseRibbon",
        108 => "ellipseRibbon2",
        112 => "flowChartPredefinedProcess",
        113 => "flowChartInternalStorage",
        114 => "flowChartDocument",
        115 => "flowChartMultidocument",
        116 => "flowChartTerminator",
        117 => "flowChartPreparation",
        118 => "flowChartManualInput",
        119 => "flowChartManualOperation",
        121 => "flowChartPunchedCard",
        122 => "flowChartPunchedTape",
        123 => "flowChartSummingJunction",
        124 => "flowChartOr",
        125 => "flowChartCollate",
        126 => "flowChartSort",
        127 => "flowChartExtract",
        128 => "flowChartMerge",
        129 => "flowChartOfflineStorage",
        130 => "flowChartOnlineStorage",
        131 => "flowChartMagneticTape",
        132 => "flowChartMagneticDisk",
        133 => "flowChartMagneticDrum",
        134 => "flowChartDisplay",
        135 => "flowChartDelay",
        176 => "flowChartAlternateProcess",
        177 => "flowChartOffpageConnector",
        178 => "callout1",
        179 => "accentCallout1",
        180 => "borderCallout1",
        181 => "accentBorderCallout1",
        182 => "leftRightUpArrow",
        183 => "sun",
        184 => "moon",
        185 => "bracketPair",
        186 => "bracePair",
        187 => "star4",
        188 => "doubleWave",
        189..=200 => "rect",
        _ => "rect",
    }
}

struct Anchor {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl<'a> Ctx<'a> {
    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    fn rel(&mut self, kind: &str, target: String) -> String {
        if let Some((id, _)) = self.rels.iter().find(|(_, t)| *t == format!("{kind}|{target}")) {
            return id.clone();
        }
        let id = format!("rId{}", self.rels.len() + 10);
        self.rels.push((id.clone(), format!("{kind}|{target}")));
        id
    }

    fn blip_rel(&mut self, pib: u32) -> Option<String> {
        let (data, ext) = self.blips.get(pib.checked_sub(1)? as usize)?.as_ref()?;
        let name = format!("media/ppt{pib}.{ext}");
        if !self.media.iter().any(|(n, _)| *n == name) {
            self.media.push((name.clone(), data.clone()));
        }
        Some(self.rel("image", format!("../{name}")))
    }

    fn fill_xml(&mut self, p: &Props, shape_type: u16) -> String {
        let filled = p.flag(0x01BF, 0x10).unwrap_or(!matches!(shape_type, 20 | 32..=40));
        if !filled {
            return "<a:noFill/>".into();
        }
        let ty = p.get(0x0180).unwrap_or(0);
        let c1 = art_color(p.get(0x0181).unwrap_or(0xFFFFFF), &self.scheme, (255, 255, 255));
        let c2 = art_color(p.get(0x0183).unwrap_or(0xFFFFFF), &self.scheme, (255, 255, 255));
        let alpha = p.get(0x0182).map(|v| v as f64 / 65536.0).unwrap_or(1.0).clamp(0.0, 1.0);
        let alpha2 = p.get(0x0184).map(|v| v as f64 / 65536.0).unwrap_or(1.0).clamp(0.0, 1.0);
        let clr = |c: (u8, u8, u8), a: f64| {
            if a < 0.999 {
                format!("<a:srgbClr val=\"{}\"><a:alpha val=\"{}\"/></a:srgbClr>", hex(c), (a * 100000.0) as i64)
            } else {
                format!("<a:srgbClr val=\"{}\"/>", hex(c))
            }
        };
        match ty {
            4..=8 => {
                let ang = p.get(0x018B).map(|v| (v as i32) as f64 / 65536.0).unwrap_or(0.0);
                let focus = p.get(0x018C).map(|v| v as i32).unwrap_or(0);
                let mut stops: Vec<(f64, String)> = if let Some(cx) = p.complex.get(&0x0197) {
                    let n = u16_at(cx, 0).unwrap_or(0) as usize;
                    (0..n.min(64))
                        .filter_map(|k| {
                            let c = u32_at(cx, 6 + k * 8)?;
                            let pos = u32_at(cx, 6 + k * 8 + 4)? as f64 / 65536.0;
                            Some((pos, clr(art_color(c, &self.scheme, (255, 255, 255)), 1.0)))
                        })
                        .collect()
                } else {
                    vec![(0.0, clr(c1, alpha)), (1.0, clr(c2, alpha2))]
                };
                if focus == 100 || focus == -100 {
                    stops = stops.into_iter().map(|(p, c)| (1.0 - p, c)).collect();
                    stops.reverse();
                } else if focus == 50 || focus == -50 {
                    let mut mirrored: Vec<(f64, String)> = stops.iter().map(|(p, c)| (p / 2.0, c.clone())).collect();
                    mirrored.extend(stops.iter().rev().map(|(p, c)| (1.0 - p / 2.0, c.clone())));
                    stops = mirrored;
                }
                let gs: String = stops
                    .iter()
                    .map(|(pos, c)| format!("<a:gs pos=\"{}\">{c}</a:gs>", (pos.clamp(0.0, 1.0) * 100000.0) as i64))
                    .collect();
                if ty == 4 || ty == 7 {
                    let lin = ((90.0 - ang).rem_euclid(360.0) * 60000.0) as i64;
                    format!("<a:gradFill rotWithShape=\"1\"><a:gsLst>{gs}</a:gsLst><a:lin ang=\"{lin}\" scaled=\"0\"/></a:gradFill>")
                } else {
                    format!("<a:gradFill rotWithShape=\"1\"><a:gsLst>{gs}</a:gsLst><a:path path=\"circle\"><a:fillToRect l=\"50000\" t=\"50000\" r=\"50000\" b=\"50000\"/></a:path></a:gradFill>")
                }
            }
            2 | 3 => match p.get(0x0186).and_then(|pib| self.blip_rel(pib)) {
                Some(rid) => format!(
                    "<a:blipFill><a:blip r:embed=\"{rid}\"/>{}</a:blipFill>",
                    if ty == 2 { "<a:tile/>" } else { "<a:stretch><a:fillRect/></a:stretch>" }
                ),
                None => format!("<a:solidFill>{}</a:solidFill>", clr(c1, alpha)),
            },
            1 => format!(
                "<a:pattFill prst=\"pct50\"><a:fgClr>{}</a:fgClr><a:bgClr>{}</a:bgClr></a:pattFill>",
                clr(c1, 1.0),
                clr(c2, 1.0)
            ),
            9 => "<a:noFill/>".into(),
            _ => format!("<a:solidFill>{}</a:solidFill>", clr(c1, alpha)),
        }
    }

    fn line_xml(&self, p: &Props, shape_type: u16, is_pic: bool) -> String {
        let on = p.flag(0x01FF, 0x08).unwrap_or(!is_pic && shape_type != 202);
        if !on {
            return "<a:ln><a:noFill/></a:ln>".into();
        }
        let c = art_color(p.get(0x01C0).unwrap_or(0), &self.scheme, (0, 0, 0));
        let alpha = p.get(0x01C1).map(|v| v as f64 / 65536.0).unwrap_or(1.0);
        let w = p.get(0x01CB).unwrap_or(9525);
        let dash = match p.get(0x01CE).unwrap_or(0) {
            1 => "sysDash",
            2 => "sysDot",
            3 => "sysDashDot",
            4 => "sysDashDotDot",
            5 => "dot",
            6 => "dash",
            7 => "lgDash",
            8 => "dashDot",
            9 => "lgDashDot",
            10 => "lgDashDotDot",
            _ => "solid",
        };
        let arrow = |v: Option<u32>| match v.unwrap_or(0) {
            1 => Some("triangle"),
            2 => Some("stealth"),
            3 => Some("diamond"),
            4 => Some("oval"),
            5 => Some("arrow"),
            _ => None,
        };
        let mut s = format!("<a:ln w=\"{w}\"><a:solidFill><a:srgbClr val=\"{}\"", hex(c));
        if alpha < 0.999 {
            s.push_str(&format!("><a:alpha val=\"{}\"/></a:srgbClr>", (alpha * 100000.0) as i64));
        } else {
            s.push_str("/>");
        }
        s.push_str(&format!("</a:solidFill><a:prstDash val=\"{dash}\"/>"));
        if let Some(a) = arrow(p.get(0x01D0)) {
            s.push_str(&format!("<a:headEnd type=\"{a}\"/>"));
        }
        if let Some(a) = arrow(p.get(0x01D1)) {
            s.push_str(&format!("<a:tailEnd type=\"{a}\"/>"));
        }
        s.push_str("</a:ln>");
        s
    }

    fn shadow_xml(&self, p: &Props) -> String {
        if p.flag(0x023F, 0x02) != Some(true) {
            return String::new();
        }
        let c = art_color(p.get(0x0201).unwrap_or(0x808080), &self.scheme, (128, 128, 128));
        let dx = p.get(0x0205).map(|v| v as i32).unwrap_or(25400) as f64;
        let dy = p.get(0x0206).map(|v| v as i32).unwrap_or(25400) as f64;
        let alpha = p.get(0x0204).map(|v| v as f64 / 65536.0).unwrap_or(1.0);
        let dist = (dx * dx + dy * dy).sqrt();
        let dir = (dy.atan2(dx).to_degrees().rem_euclid(360.0) * 60000.0) as i64;
        format!(
            "<a:effectLst><a:outerShdw dist=\"{}\" dir=\"{dir}\"><a:srgbClr val=\"{}\"><a:alpha val=\"{}\"/></a:srgbClr></a:outerShdw></a:effectLst>",
            dist as i64,
            hex(c),
            (alpha * 100000.0) as i64
        )
    }

    fn geometry_xml(&self, p: &Props, shape_type: u16) -> String {
        if shape_type == 0 || shape_type == 4095 {
            if let Some(g) = self.freeform(p) {
                return g;
            }
        }
        let name = prst_name(shape_type);
        let mut av = String::new();
        if shape_type == 2 {
            if let Some(v) = p.get(0x0147) {
                av = format!("<a:gd name=\"adj\" fmla=\"val {}\"/>", (v as i32 as f64 / 21600.0 * 100000.0) as i64);
            }
        }
        format!("<a:prstGeom prst=\"{name}\"><a:avLst>{av}</a:avLst></a:prstGeom>")
    }

    fn freeform(&self, p: &Props) -> Option<String> {
        let verts = p.complex.get(&0x0145)?;
        let n = u16_at(verts, 0)? as usize;
        let cb = u16_at(verts, 4)?;
        let size = if cb == 0xFFF0 { 4 } else { cb as usize };
        let pts: Vec<(i32, i32)> = (0..n.min(100_000))
            .filter_map(|k| {
                let o = 6 + k * size;
                if size == 4 {
                    Some((i16_at(verts, o)? as i32, i16_at(verts, o + 2)? as i32))
                } else {
                    Some((i32_at(verts, o)?, i32_at(verts, o + 4)?))
                }
            })
            .collect();
        if pts.is_empty() {
            return None;
        }
        let left = p.get(0x0140).map(|v| v as i32).unwrap_or(0);
        let top = p.get(0x0141).map(|v| v as i32).unwrap_or(0);
        let right = p.get(0x0142).map(|v| v as i32).filter(|v| *v > left).unwrap_or(21600);
        let bottom = p.get(0x0143).map(|v| v as i32).filter(|v| *v > top).unwrap_or(21600);
        // 공식으로 적힌 꼭짓점(0x8000 표시)은 읽을 수 없다 — 그런 도형은 사각형으로.
        if size == 8 && pts.iter().any(|(x, y)| (*x as u32) & 0x8000_0000 != 0 || (*y as u32) & 0x8000_0000 != 0 || (*x as u32 >> 16) == 0x8000 || (*y as u32 >> 16) == 0x8000) {
            return None;
        }
        let (w, h) = ((right - left).max(1), (bottom - top).max(1));
        let pt = |k: usize| pts.get(k).map(|(x, y)| format!("<a:pt x=\"{}\" y=\"{}\"/>", x - left, y - top)).unwrap_or_default();
        let mut path = String::new();
        let mut k = 0usize;
        if let Some(segs) = p.complex.get(&0x0146) {
            let ns = u16_at(segs, 0).unwrap_or(0) as usize;
            let scb = u16_at(segs, 4).unwrap_or(2);
            let ssize = if scb == 0xFFF0 { 2 } else { (scb as usize).max(2) };
            for s in 0..ns.min(100_000) {
                let Some(seg) = u16_at(segs, 6 + s * ssize) else { break };
                let ty = seg >> 13;
                let cnt = (seg & 0x1FFF) as usize;
                match ty {
                    0 => {
                        for _ in 0..cnt.max(1) {
                            path.push_str(&format!("<a:lnTo>{}</a:lnTo>", pt(k)));
                            k += 1;
                        }
                    }
                    1 => {
                        for _ in 0..cnt.max(1) {
                            path.push_str(&format!("<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(k), pt(k + 1), pt(k + 2)));
                            k += 3;
                        }
                    }
                    2 => {
                        path.push_str(&format!("<a:moveTo>{}</a:moveTo>", pt(k)));
                        k += 1;
                    }
                    3 => path.push_str("<a:close/>"),
                    4 => {}
                    _ => k += (seg & 0xFF) as usize,
                }
            }
        } else {
            path.push_str(&format!("<a:moveTo>{}</a:moveTo>", pt(0)));
            for i in 1..pts.len() {
                path.push_str(&format!("<a:lnTo>{}</a:lnTo>", pt(i)));
            }
        }
        if !path.contains("<a:lnTo>") && !path.contains("<a:cubicBezTo>") {
            return None;
        }
        Some(format!(
            "<a:custGeom><a:avLst/><a:gdLst/><a:rect l=\"l\" t=\"t\" r=\"r\" b=\"b\"/><a:pathLst><a:path w=\"{w}\" h=\"{h}\">{path}</a:path></a:pathLst></a:custGeom>"
        ))
    }

    fn font_name(&self, i: u16) -> Option<&str> {
        self.fonts.get(i as usize).map(|s| s.as_str()).filter(|s| !s.is_empty())
    }

    /// 글 덩어리를 문단 XML 로.
    fn text_xml(&self, block: &TextBlock<'_>, slide_no_field: bool) -> String {
        let kind = block.kind as u16;
        let text = &block.text;
        // 마스터 모양 사슬: 종류별 기본 → 그 종류
        let base_kind: &[u16] = match kind {
            0 => &[0],
            6 => &[0, 6],
            5 => &[1, 5],
            7 | 8 => &[1, kind],
            1 => &[1],
            _ => &[4, kind],
        };
        let level_style = |lvl: usize| -> (Pf, Cf) {
            let mut pf = Pf::default();
            let mut cf = Cf::default();
            for k in base_kind {
                if let Some(levels) = self.styles.get(k) {
                    if let Some((p, c)) = levels.get(0) {
                        if lvl > 0 {
                            pf.merge(p);
                            cf.merge(c);
                        }
                    }
                    if let Some((p, c)) = levels.get(lvl.min(levels.len().saturating_sub(1))) {
                        pf.merge(p);
                        cf.merge(c);
                    }
                }
            }
            (pf, cf)
        };
        // 문단·글자 구간
        let mut para_runs: Vec<(usize, u16, Pf)> = Vec::new();
        let mut char_runs: Vec<(usize, Cf)> = Vec::new();
        if let Some(st) = block.style {
            let total = text.len() + 1;
            let mut o = 0;
            let mut covered = 0;
            while covered < total && o < st.len() {
                let Some(count) = u32_at(st, o) else { break };
                let Some(lvl) = u16_at(st, o + 4) else { break };
                let Some((pf, o2)) = read_pf(st, o + 6) else { break };
                para_runs.push((count as usize, lvl, pf));
                covered += count.max(1) as usize;
                o = o2;
            }
            let mut covered = 0;
            while covered < total && o < st.len() {
                let Some(count) = u32_at(st, o) else { break };
                let Some((cf, o2)) = read_cf(st, o + 4) else { break };
                char_runs.push((count as usize, cf));
                covered += count.max(1) as usize;
                o = o2;
            }
        }
        let para_at = |pos: usize| -> (u16, Pf) {
            let mut acc = 0;
            for (n, l, p) in &para_runs {
                acc += n;
                if pos < acc {
                    return (*l, p.clone());
                }
            }
            para_runs.last().map(|r| (r.1, r.2.clone())).unwrap_or((0, Pf::default()))
        };
        let char_at = |pos: usize| -> Cf {
            let mut acc = 0;
            for (n, c) in &char_runs {
                acc += n;
                if pos < acc {
                    return c.clone();
                }
            }
            char_runs.last().map(|r| r.1.clone()).unwrap_or_default()
        };

        let mut xml = String::new();
        let mut start = 0usize;
        let len = text.len();
        let mut pos = 0usize;
        while pos <= len {
            let end_of_para = pos == len || text[pos] == 0x0D;
            if !end_of_para {
                pos += 1;
                continue;
            }
            let (lvl, own_pf) = para_at(start);
            let lvl = (lvl as usize).min(4);
            let (mut pf, base_cf) = level_style(lvl);
            pf.merge(&own_pf);
            xml.push_str("<a:p>");
            xml.push_str(&self.ppr_xml(&pf, &base_cf, lvl, &char_at(start)));
            // 글자 덩어리
            let mut i = start;
            while i < pos {
                let cf_here = char_at(i);
                let mut j = i;
                let mut chunk: Vec<u16> = Vec::new();
                let mut fields = false;
                while j < pos {
                    let cf_j = char_at(j);
                    if format!("{:?}", cf_j) != format!("{:?}", cf_here) {
                        break;
                    }
                    if slide_no_field && block.slide_num_at.contains(&j) {
                        if j == i {
                            fields = true;
                            j += 1;
                        }
                        break;
                    }
                    chunk.push(text[j]);
                    j += 1;
                }
                let mut cf = base_cf.clone();
                cf.merge(&cf_here);
                let rpr = self.rpr_xml(&cf, "rPr");
                if fields {
                    xml.push_str(&format!("<a:fld id=\"{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}\" type=\"slidenum\">{rpr}<a:t>#</a:t></a:fld>"));
                } else {
                    // 줄 바꿈(0x0B)은 a:br 로
                    let s = String::from_utf16_lossy(&chunk);
                    for (k, piece) in s.split('\u{b}').enumerate() {
                        if k > 0 {
                            xml.push_str(&format!("<a:br>{rpr}</a:br>"));
                        }
                        if !piece.is_empty() {
                            xml.push_str(&format!("<a:r>{rpr}<a:t>{}</a:t></a:r>", esc(piece)));
                        }
                    }
                }
                if j == i {
                    j = i + 1;
                }
                i = j;
            }
            let mut cf = base_cf.clone();
            cf.merge(&char_at(pos.min(len)));
            xml.push_str(&self.rpr_xml(&cf, "endParaRPr"));
            xml.push_str("</a:p>");
            pos += 1;
            start = pos;
        }
        if xml.is_empty() {
            xml.push_str("<a:p/>");
        }
        xml
    }

    fn rpr_xml(&self, cf: &Cf, tag: &str) -> String {
        let size = cf.size.unwrap_or(18).clamp(1, 4000);
        let mut s = format!("<a:{tag} lang=\"ko-KR\" sz=\"{}\"", size as u32 * 100);
        if cf.bold == Some(true) {
            s.push_str(" b=\"1\"");
        }
        if cf.italic == Some(true) {
            s.push_str(" i=\"1\"");
        }
        if cf.underline == Some(true) {
            s.push_str(" u=\"sng\"");
        }
        if let Some(p) = cf.position.filter(|p| *p != 0) {
            s.push_str(&format!(" baseline=\"{}\"", p as i32 * 1000));
        }
        s.push('>');
        let c = cf.color.map(|c| col_rgb(c, &self.scheme)).unwrap_or_else(|| scheme_rgb(&self.scheme, 1));
        s.push_str(&format!("<a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>", hex(c)));
        let latin = cf.font.and_then(|f| self.font_name(f));
        let ea = cf.ea_font.and_then(|f| self.font_name(f)).or(latin);
        if let Some(l) = latin {
            s.push_str(&format!("<a:latin typeface=\"{}\"/>", esc(l)));
        }
        if let Some(e) = ea {
            s.push_str(&format!("<a:ea typeface=\"{}\"/>", esc(e)));
        }
        s.push_str(&format!("</a:{tag}>"));
        s
    }

    fn ppr_xml(&self, pf: &Pf, cf: &Cf, lvl: usize, first: &Cf) -> String {
        let left = pf.left.unwrap_or(0) as f64 * MU_TO_EMU;
        let indent = pf.indent.unwrap_or(0) as f64 * MU_TO_EMU;
        let algn = match pf.align.unwrap_or(0) {
            1 => "ctr",
            2 => "r",
            3 => "just",
            4 => "dist",
            _ => "l",
        };
        let mut s = format!(
            "<a:pPr lvl=\"{lvl}\" marL=\"{}\" indent=\"{}\" algn=\"{algn}\">",
            left as i64,
            (indent - left) as i64
        );
        let sp = |v: Option<i16>, tag: &str| -> String {
            match v {
                Some(v) if v >= 0 => format!("<a:{tag}><a:spcPct val=\"{}\"/></a:{tag}>", v as i64 * 1000),
                Some(v) => format!("<a:{tag}><a:spcPts val=\"{}\"/></a:{tag}>", (-(v as i64) * 100) / 8),
                None => String::new(),
            }
        };
        if let Some(l) = pf.line {
            s.push_str(&sp(Some(l), "lnSpc"));
        }
        if pf.before.is_some() {
            s.push_str(&sp(pf.before, "spcBef"));
        }
        if pf.after.is_some() {
            s.push_str(&sp(pf.after, "spcAft"));
        }
        if pf.has_bullet == Some(true) {
            if pf.bullet_has_color == Some(true) {
                if let Some(c) = pf.bullet_color {
                    s.push_str(&format!("<a:buClr><a:srgbClr val=\"{}\"/></a:buClr>", hex(col_rgb(c, &self.scheme))));
                }
            }
            if pf.bullet_has_size == Some(true) {
                if let Some(v) = pf.bullet_size {
                    if v > 0 {
                        s.push_str(&format!("<a:buSzPct val=\"{}\"/>", v as i64 * 1000));
                    } else {
                        s.push_str(&format!("<a:buSzPts val=\"{}\"/>", -(v as i64) * 100));
                    }
                }
            }
            let font = if pf.bullet_has_font == Some(true) {
                pf.bullet_font.and_then(|f| self.font_name(f))
            } else {
                None
            };
            if let Some(fname) = font {
                s.push_str(&format!("<a:buFont typeface=\"{}\"/>", esc(fname)));
            }
            let ch = pf.bullet_char.and_then(|c| char::from_u32(c as u32)).filter(|c| *c != '\0').unwrap_or('•');
            s.push_str(&format!("<a:buChar char=\"{}\"/>", esc(&ch.to_string())));
        } else {
            s.push_str("<a:buNone/>");
        }
        let _ = (cf, first);
        s.push_str("</a:pPr>");
        s
    }
}

/// 자리표시자 종류 (OEPlaceholderAtom)
fn placeholder_kind(shape: &[Rec<'_>]) -> Option<u8> {
    let cd = shape.iter().find(|r| r.kind == 0xF011)?;
    let ph = find(cd.data, 0x0BC3)?;
    ph.data.get(4).copied()
}

fn read_anchor(shape: &[Rec<'_>], child_space: bool) -> Option<Anchor> {
    if let Some(a) = shape.iter().find(|r| r.kind == 0xF00F) {
        let (l, t, r, b) = (i32_at(a.data, 0)?, i32_at(a.data, 4)?, i32_at(a.data, 8)?, i32_at(a.data, 12)?);
        return Some(Anchor { x: l as f64, y: t as f64, w: (r - l) as f64, h: (b - t) as f64 });
    }
    if let Some(a) = shape.iter().find(|r| r.kind == 0xF010) {
        let (t, l, r, b) = if a.data.len() >= 16 {
            (i32_at(a.data, 0)?, i32_at(a.data, 4)?, i32_at(a.data, 8)?, i32_at(a.data, 12)?)
        } else {
            (i16_at(a.data, 0)? as i32, i16_at(a.data, 2)? as i32, i16_at(a.data, 4)? as i32, i16_at(a.data, 6)? as i32)
        };
        let k = if child_space { 1.0 } else { MU_TO_EMU };
        return Some(Anchor { x: l as f64 * k, y: t as f64 * k, w: (r - l) as f64 * k, h: (b - t) as f64 * k });
    }
    None
}

struct Drawn {
    xml: String,
}

/// 묶음 안 좌표 → EMU
#[derive(Clone, Copy)]
struct Map {
    ox: f64,
    oy: f64,
    sx: f64,
    sy: f64,
}

impl Map {
    const TOP: Map = Map { ox: 0.0, oy: 0.0, sx: 1.0, sy: 1.0 };
    fn apply(&self, a: Anchor) -> Anchor {
        Anchor { x: self.ox + a.x * self.sx, y: self.oy + a.y * self.sy, w: a.w * self.sx, h: a.h * self.sy }
    }
}

impl<'a> Ctx<'a> {
    /// spgrContainer 하나를 XML 로 (맨 위면 top=true).
    fn group(&mut self, spgr: &Rec<'_>, top: bool, map: Map, skip_placeholders: bool, out: &mut Drawn, background: &mut Option<String>) {
        let items = children(spgr.data);
        let mut first = true;
        for it in items {
            match it.kind {
                0xF004 => {
                    if first {
                        first = false;
                        // 묶음 자신(또는 맨 위 묶음)
                        continue;
                    }
                    self.shape(&it, top, map, skip_placeholders, out, background);
                }
                0xF003 => {
                    first = false;
                    // 안쪽 묶음
                    let inner = children(it.data);
                    let Some(head) = inner.first().filter(|r| r.kind == 0xF004) else { continue };
                    let head_recs = children(head.data);
                    let Some(fspgr) = head_recs.iter().find(|r| r.kind == 0xF009) else { continue };
                    let (cl, ct, cr, cb) = (
                        i32_at(fspgr.data, 0).unwrap_or(0),
                        i32_at(fspgr.data, 4).unwrap_or(0),
                        i32_at(fspgr.data, 8).unwrap_or(0),
                        i32_at(fspgr.data, 12).unwrap_or(0),
                    );
                    let Some(a) = read_anchor(&head_recs, !top).map(|a| map.apply(a)) else { continue };
                    let mut props = Props::default();
                    for r in head_recs.iter().filter(|r| matches!(r.kind, 0xF00B | 0xF121 | 0xF122)) {
                        Props::read(r, &mut props);
                    }
                    let fsp = head_recs.iter().find(|r| r.kind == 0xF00A);
                    let flags = fsp.and_then(|f| u32_at(f.data, 4)).unwrap_or(0);
                    let rot = props.get(0x0004).map(|v| (v as i32) as f64 / 65536.0).unwrap_or(0.0);
                    let id = self.id();
                    out.xml.push_str(&format!(
                        "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{id}\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm{}><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/><a:chOff x=\"{x}\" y=\"{y}\"/><a:chExt cx=\"{w}\" cy=\"{h}\"/></a:xfrm></p:grpSpPr>",
                        xfrm_attrs(rot, flags),
                        x = a.x as i64,
                        y = a.y as i64,
                        w = a.w.max(1.0) as i64,
                        h = a.h.max(1.0) as i64,
                    ));
                    let sx = a.w / (cr - cl).max(1) as f64;
                    let sy = a.h / (cb - ct).max(1) as f64;
                    let inner = Map { ox: a.x - cl as f64 * sx, oy: a.y - ct as f64 * sy, sx, sy };
                    self.group(&it, false, inner, skip_placeholders, out, background);
                    out.xml.push_str("</p:grpSp>");
                }
                _ => {}
            }
        }
    }

    fn shape(&mut self, sp: &Rec<'_>, top: bool, map: Map, skip_placeholders: bool, out: &mut Drawn, background: &mut Option<String>) {
        let recs = children(sp.data);
        let Some(fsp) = recs.iter().find(|r| r.kind == 0xF00A) else { return };
        let shape_type = fsp.inst;
        let flags = u32_at(fsp.data, 4).unwrap_or(0);
        if flags & 0x08 != 0 {
            return;
        }
        let mut props = Props::default();
        let mut own = Props::default();
        for r in recs.iter().filter(|r| matches!(r.kind, 0xF00B | 0xF121 | 0xF122)) {
            Props::read(r, &mut own);
        }
        // 마스터 도형을 물려받는 도형 (fHaveMaster)
        let has_master = flags & 0x20 != 0;
        if has_master {
            if let Some(base) = own.get(0x0301).and_then(|id| self.master_props.get(&id)) {
                props = base.clone();
            }
        }
        for (k, v) in own.map.iter() {
            props.map.insert(*k, *v);
        }
        for (k, v) in own.complex.iter() {
            props.complex.insert(*k, v.clone());
        }
        if let (Some(c), Some(spid)) = (self.collect.as_mut(), u32_at(fsp.data, 0)) {
            c.insert(spid, props.clone());
        }
        if flags & 0x400 != 0 {
            // 배경 도형
            *background = Some(self.fill_xml(&props, shape_type));
            return;
        }
        if props.flag(0x03BF, 0x02) == Some(true) {
            return; // 숨김
        }
        let ph = placeholder_kind(&recs);
        if ph.is_some() && skip_placeholders {
            return;
        }
        let Some(mut a) = read_anchor(&recs, !top).map(|a| map.apply(a)) else { return };
        let rot = props.get(0x0004).map(|v| (v as i32) as f64 / 65536.0).unwrap_or(0.0);
        // 45~135도 돌린 도형은 상자가 눕혀져 저장된다.
        let r = rot.rem_euclid(180.0);
        if (45.0..135.0).contains(&r) {
            let (cx, cy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
            std::mem::swap(&mut a.w, &mut a.h);
            a.x = cx - a.w / 2.0;
            a.y = cy - a.h / 2.0;
        }
        let xfrm = format!(
            "<a:xfrm{}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
            xfrm_attrs(rot, flags),
            a.x as i64,
            a.y as i64,
            a.w.max(0.0) as i64,
            a.h.max(0.0) as i64
        );
        let id = self.id();

        // 글
        let tb = recs.iter().find(|r| r.kind == 0xF00D);
        let mut text_xml = None;
        if let Some(tb) = tb {
            let inner = children(tb.data);
            let blocks = read_text_records(&inner);
            if let Some(b) = blocks.first() {
                if !b.text.is_empty() {
                    text_xml = Some(self.text_xml(b, true));
                }
            }
            if text_xml.is_none() {
                if let Some(o) = inner.iter().find(|r| r.kind == 0x0F9E) {
                    let idx = i32_at(o.data, 0).unwrap_or(-1);
                    if idx >= 0 {
                        if let Some(b) = self.slide_texts.get(idx as usize) {
                            if !b.text.is_empty() {
                                text_xml = Some(self.text_xml(b, true));
                            }
                        }
                    }
                }
            }
        }
        // 빈 자리표시자는 쇼에서 보이지 않는다.
        if (ph.is_some() || has_master) && text_xml.is_none() && props.get(0x0104).is_none() {
            return;
        }

        let pib = props.get(0x0104);
        if let Some(pib) = pib {
            if let Some(rid) = self.blip_rel(pib) {
                let crop = |id: u16| props.get(id).map(|v| (v as i32) as f64 / 65536.0).unwrap_or(0.0);
                let (t, b, l, rr) = (crop(0x0100), crop(0x0101), crop(0x0102), crop(0x0103));
                let src = if t != 0.0 || b != 0.0 || l != 0.0 || rr != 0.0 {
                    format!(
                        "<a:srcRect l=\"{}\" t=\"{}\" r=\"{}\" b=\"{}\"/>",
                        (l * 100000.0) as i64,
                        (t * 100000.0) as i64,
                        (rr * 100000.0) as i64,
                        (b * 100000.0) as i64
                    )
                } else {
                    String::new()
                };
                let line = self.line_xml(&props, shape_type, true);
                out.xml.push_str(&format!(
                    "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/>{src}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{xfrm}{}{line}</p:spPr></p:pic>",
                    self.geometry_xml(&props, if shape_type == 75 { 1 } else { shape_type })
                ));
                if text_xml.is_none() {
                    return;
                }
            }
        }

        let geom = self.geometry_xml(&props, shape_type);
        let fill = if pib.is_some() { "<a:noFill/>".to_owned() } else { self.fill_xml(&props, shape_type) };
        let line = self.line_xml(&props, shape_type, pib.is_some());
        let shadow = self.shadow_xml(&props);
        let mut xml = format!(
            "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{xfrm}{geom}{fill}{line}{shadow}</p:spPr>"
        );
        if let Some(t) = text_xml {
            let ins = |id: u16, d: u32| props.get(id).unwrap_or(d);
            let anchor = match props.get(0x0087).unwrap_or(0) {
                1 | 4 | 7 => "ctr",
                2 | 5 | 8 => "b",
                _ => "t",
            };
            let anchor_ctr = matches!(props.get(0x0087).unwrap_or(0), 3 | 4 | 5);
            let wrap = if props.get(0x0085) == Some(2) { "none" } else { "square" };
            let vert = match props.get(0x0088).unwrap_or(0) {
                1 | 5 => "vert",
                2 => "vert270",
                _ => "horz",
            };
            xml.push_str(&format!(
                "<p:txBody><a:bodyPr lIns=\"{}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" anchor=\"{anchor}\"{} wrap=\"{wrap}\" vert=\"{vert}\"><a:noAutofit/></a:bodyPr><a:lstStyle/>{t}</p:txBody>",
                ins(0x0081, 91440),
                ins(0x0082, 45720),
                ins(0x0083, 91440),
                ins(0x0084, 45720),
                if anchor_ctr { " anchorCtr=\"1\"" } else { "" }
            ));
        }
        xml.push_str("</p:sp>");
        out.xml.push_str(&xml);
    }
}

fn xfrm_attrs(rot: f64, flags: u32) -> String {
    let mut s = String::new();
    if rot != 0.0 {
        s.push_str(&format!(" rot=\"{}\"", (rot.rem_euclid(360.0) * 60000.0) as i64));
    }
    if flags & 0x40 != 0 {
        s.push_str(" flipH=\"1\"");
    }
    if flags & 0x80 != 0 {
        s.push_str(" flipV=\"1\"");
    }
    s
}

// ---------------------------------------------------------------------------
// 그림 창고

fn read_blip(rec: &Rec<'_>) -> Option<(Vec<u8>, &'static str)> {
    let uid = if rec.inst & 1 == 1 { 32 } else { 16 };
    match rec.kind {
        0xF01A | 0xF01B | 0xF01C => {
            // 메타파일: 머리 34바이트 뒤에 (대개 deflate 압축된) 본문
            let head = uid;
            let cb_size = u32_at(rec.data, head)? as usize;
            let bounds = (i32_at(rec.data, head + 4)?, i32_at(rec.data, head + 8)?, i32_at(rec.data, head + 12)?, i32_at(rec.data, head + 16)?);
            let compressed = *rec.data.get(head + 32)? == 0;
            let body = rec.data.get(head + 34..)?;
            let data = if compressed {
                let mut d = flate2::read::ZlibDecoder::new(body);
                let mut v = Vec::with_capacity(cb_size.min(64 << 20));
                d.read_to_end(&mut v).ok()?;
                v
            } else {
                body.to_vec()
            };
            if rec.kind == 0xF01A {
                Some((data, "emf"))
            } else if rec.kind == 0xF01B {
                // WMF 는 자리 머리(placeable header)를 붙여 준다.
                let mut h = vec![0xD7, 0xCD, 0xC6, 0x9A, 0, 0];
                for v in [bounds.0, bounds.1, bounds.2, bounds.3] {
                    h.extend_from_slice(&(v as i16).to_le_bytes());
                }
                h.extend_from_slice(&1440u16.to_le_bytes());
                h.extend_from_slice(&0u32.to_le_bytes());
                let mut sum: u16 = 0;
                for k in 0..10 {
                    sum ^= u16::from_le_bytes([h[k * 2], h[k * 2 + 1]]);
                }
                h.extend_from_slice(&sum.to_le_bytes());
                h.extend_from_slice(&data);
                Some((h, "wmf"))
            } else {
                None
            }
        }
        0xF01D | 0xF02A => Some((rec.data.get(uid + 1..)?.to_vec(), "jpeg")),
        0xF01E => Some((rec.data.get(uid + 1..)?.to_vec(), "png")),
        0xF029 => Some((rec.data.get(uid + 1..)?.to_vec(), "tiff")),
        0xF01F => {
            // DIB → BMP
            let dib = rec.data.get(uid + 1..)?;
            let header = u32_at(dib, 0)? as usize;
            let bits = u16_at(dib, 14).unwrap_or(24) as usize;
            let colors = u32_at(dib, 32).unwrap_or(0) as usize;
            let palette = if bits <= 8 { (if colors == 0 { 1 << bits } else { colors }) * 4 } else { 0 };
            let off = 14 + header + palette;
            let mut bmp = b"BM".to_vec();
            bmp.extend_from_slice(&((14 + dib.len()) as u32).to_le_bytes());
            bmp.extend_from_slice(&[0, 0, 0, 0]);
            bmp.extend_from_slice(&(off as u32).to_le_bytes());
            bmp.extend_from_slice(dib);
            Some((bmp, "bmp"))
        }
        _ => None,
    }
}

fn read_bstore(doc: &[Rec<'_>], pictures: &[u8]) -> Vec<Option<(Vec<u8>, &'static str)>> {
    let mut out = Vec::new();
    let Some(group) = doc.iter().find(|r| r.kind == 0x040B) else { return out };
    let Some(dgg) = find(group.data, 0xF000) else { return out };
    let Some(store) = find(dgg.data, 0xF001) else { return out };
    for bse in children(store.data) {
        if bse.kind != 0xF007 {
            out.push(None);
            continue;
        }
        let size = u32_at(bse.data, 20).unwrap_or(0) as usize;
        let fo = u32_at(bse.data, 28).unwrap_or(u32::MAX) as usize;
        let cb_name = bse.data.get(33).copied().unwrap_or(0) as usize;
        let mut blip = None;
        if size > 0 && fo < pictures.len() {
            if let Some(r) = rec_at(pictures, fo) {
                blip = read_blip(&r);
            }
        }
        if blip.is_none() && bse.data.len() > 36 + cb_name + 8 {
            if let Some(r) = rec_at(bse.data, 36 + cb_name) {
                blip = read_blip(&r);
            }
        }
        out.push(blip);
    }
    out
}

// ---------------------------------------------------------------------------

struct SlideRef {
    persist: u32,
    slide_id: u32,
    texts_start: usize,
    texts_end: usize,
}

pub(crate) fn convert(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let pkg = to_package(bytes)?;
    // 시험용: 옮긴 부분들을 폴더에 쏟아 본다.
    #[cfg(not(any(target_os = "tvos", target_os = "ios")))]
    if let Ok(dir) = std::env::var("ARGO_PPT_DUMP") {
        for name in pkg.names() {
            let path = std::path::Path::new(&dir).join(&name);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Some(b) = pkg.bytes(&name) {
                let _ = std::fs::write(path, b);
            }
        }
    }
    super::convert_package(&pkg)
}

fn to_package(bytes: &[u8]) -> Result<Package, String> {
    let mut file = cfb::CompoundFile::open(Cursor::new(bytes)).map_err(|e| format!("cfb: {e}"))?;
    if file.is_stream("/EncryptedSummary") {
        return Err("암호가 걸린 ppt".into());
    }
    let doc = stream(&mut file, "/PowerPoint Document").ok_or("PowerPoint Document 흐름 없음")?;
    let pictures = stream(&mut file, "/Pictures").unwrap_or_default();
    let current = stream(&mut file, "/Current User").unwrap_or_default();

    // 영속 목록 (persistId → 위치)
    let mut persist: HashMap<u32, usize> = HashMap::new();
    let mut doc_ref: Option<u32> = None;
    let mut off = u32_at(&current, 16).map(|v| v as usize);
    let mut seen = 0;
    while let Some(o) = off {
        seen += 1;
        if seen > 2000 {
            break;
        }
        let Some(edit) = rec_at(&doc, o).filter(|r| r.kind == 0x0FF5) else { break };
        if doc_ref.is_none() {
            doc_ref = u32_at(edit.data, 16);
        }
        if let Some(pd) = u32_at(edit.data, 12).and_then(|p| rec_at(&doc, p as usize)).filter(|r| r.kind == 0x1772) {
            let mut i = 0;
            while i + 4 <= pd.data.len() {
                let w = u32_at(pd.data, i).unwrap_or(0);
                let start = w & 0xFFFFF;
                let count = (w >> 20) as usize;
                i += 4;
                for k in 0..count {
                    if let Some(v) = u32_at(pd.data, i + k * 4) {
                        persist.entry(start + k as u32).or_insert(v as usize);
                    }
                }
                i += count * 4;
            }
        }
        let last = u32_at(edit.data, 8).unwrap_or(0) as usize;
        off = if last == 0 || last == o { None } else { Some(last) };
    }

    let doc_rec = doc_ref
        .and_then(|r| persist.get(&r))
        .and_then(|&o| rec_at(&doc, o))
        .filter(|r| r.kind == 0x03E8)
        .or_else(|| children(&doc).into_iter().find(|r| r.kind == 0x03E8))
        .ok_or("Document 레코드 없음")?;
    let doc_children = children(doc_rec.data);

    // 크기
    let atom = doc_children.iter().find(|r| r.kind == 0x03E9);
    let sw = atom.and_then(|a| i32_at(a.data, 0)).unwrap_or(5760) as f64 * MU_TO_EMU;
    let sh = atom.and_then(|a| i32_at(a.data, 4)).unwrap_or(4320) as f64 * MU_TO_EMU;

    // 글꼴 목록
    let mut fonts = Vec::new();
    if let Some(env) = doc_children.iter().find(|r| r.kind == 0x03F2) {
        if let Some(fc) = find(env.data, 0x07D6) {
            for fe in children(fc.data).into_iter().filter(|r| r.kind == 0x0FB7) {
                let units: Vec<u16> = (0..32).filter_map(|k| u16_at(fe.data, k * 2)).take_while(|c| *c != 0).collect();
                fonts.push(String::from_utf16_lossy(&units));
            }
        }
    }
    let blips = read_bstore(&doc_children, &pictures);

    // 슬라이드·마스터 목록
    let mut slides: Vec<SlideRef> = Vec::new();
    let mut masters: Vec<SlideRef> = Vec::new();
    let mut all_texts: Vec<TextBlock> = Vec::new();
    for lst in doc_children.iter().filter(|r| r.kind == 0x0FF0) {
        let recs = children(lst.data);
        let target = match lst.inst {
            0 => 0,
            1 => 1,
            _ => continue,
        };
        let mut i = 0;
        while i < recs.len() {
            if recs[i].kind == 0x03F3 {
                let r = &recs[i];
                let mut j = i + 1;
                while j < recs.len() && recs[j].kind != 0x03F3 {
                    j += 1;
                }
                let start = all_texts.len();
                all_texts.extend(read_text_records(&recs[i + 1..j]));
                let sr = SlideRef {
                    persist: u32_at(r.data, 0).unwrap_or(0),
                    slide_id: u32_at(r.data, 12).unwrap_or(0),
                    texts_start: start,
                    texts_end: all_texts.len(),
                };
                if target == 0 {
                    slides.push(sr);
                } else {
                    masters.push(sr);
                }
                i = j;
            } else {
                i += 1;
            }
        }
    }
    if slides.is_empty() {
        return Err("슬라이드 목록 없음".into());
    }

    let mut parts: HashMap<String, Vec<u8>> = HashMap::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
    let rel_type = |k: &str| format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{k}");
    let rels_xml = |rels: &[(String, String)]| -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
        for (id, kt) in rels {
            let (k, t) = kt.split_once('|').unwrap_or(("", kt));
            s.push_str(&format!("<Relationship Id=\"{id}\" Type=\"{}\" Target=\"{}\"/>", rel_type(k), esc(t)));
        }
        s.push_str("</Relationships>");
        s
    };

    // 마스터마다 글 모양·색·도형
    struct MasterOut {
        slide_id: u32,
        styles: MasterStyles,
        scheme: [u32; 8],
        layout_path: String,
        background: Option<String>,
        props: HashMap<u32, Props>,
    }
    let mut master_out: Vec<MasterOut> = Vec::new();
    let read_scheme = |recs: &[Rec<'_>]| -> Option<[u32; 8]> {
        let cs = recs.iter().find(|r| r.kind == 0x07F0 && r.inst == 1).or_else(|| recs.iter().find(|r| r.kind == 0x07F0))?;
        let mut s = [0u32; 8];
        for (k, v) in s.iter_mut().enumerate() {
            *v = u32_at(cs.data, k * 4)?;
        }
        Some(s)
    };
    let default_scheme = [0xFFFFFF, 0x000000, 0x808080, 0x000000, 0xBBE0E3, 0x333399, 0x009999, 0x99CC00];
    let empty_styles: MasterStyles = HashMap::new();
    // 1차: 주 마스터들의 모양
    for (mi, m) in masters.iter().enumerate() {
        let Some(rec) = persist.get(&m.persist).and_then(|&o| rec_at(&doc, o)) else { continue };
        let recs = children(rec.data);
        let mut styles: MasterStyles = HashMap::new();
        for r in recs.iter().filter(|r| r.kind == 0x0FA3) {
            styles.insert(r.inst, read_master_style(r));
        }
        let scheme = read_scheme(&recs).unwrap_or(default_scheme);
        master_out.push(MasterOut {
            slide_id: m.slide_id,
            styles,
            scheme,
            layout_path: format!("ppt/slideLayouts/slideLayout{}.xml", mi + 1),
            background: None,
            props: HashMap::new(),
        });
    }
    // 제목 마스터(Slide 레코드)는 주 마스터의 글 모양을 빌린다.
    for mi in 0..master_out.len() {
        if master_out[mi].styles.is_empty() {
            if let Some(main) = master_out.iter().find(|m| !m.styles.is_empty()) {
                let s = main.styles.clone();
                master_out[mi].styles = s;
            }
        }
    }
    // 2차: 마스터 도형 → 레이아웃
    for (mi, m) in masters.iter().enumerate() {
        let Some(rec) = persist.get(&m.persist).and_then(|&o| rec_at(&doc, o)) else { continue };
        if mi >= master_out.len() {
            break;
        }
        let recs = children(rec.data);
        let mut drawn = Drawn { xml: String::new() };
        let mut background = None;
        let texts = &all_texts[m.texts_start..m.texts_end];
        let no_props: HashMap<u32, Props> = HashMap::new();
        let collected;
        {
            let mut ctx = Ctx {
                master_props: &no_props,
                collect: Some(HashMap::new()),
                fonts: fonts.clone(),
                scheme: master_out[mi].scheme,
                styles: &master_out[mi].styles,
                blips: &blips,
                media: &mut media,
                rels: vec![("rId1".into(), "slideMaster|../slideMasters/slideMaster1.xml".into())],
                next_id: 1,
                slide_texts: texts,
            };
            if let Some(dg) = recs.iter().find(|r| r.kind == 0x040C).and_then(|d| find(d.data, 0xF002)) {
                for item in children(dg.data) {
                    match item.kind {
                        0xF003 => ctx.group(&item, true, Map::TOP, true, &mut drawn, &mut background),
                        0xF004 => ctx.shape(&item, true, Map::TOP, true, &mut drawn, &mut background),
                        _ => {}
                    }
                }
            }
            let bg = background.clone().map(|f| format!("<p:bg><p:bgPr>{f}<a:effectLst/></p:bgPr></p:bg>")).unwrap_or_default();
            let layout = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout {NS}><p:cSld>{bg}<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:spTree></p:cSld></p:sldLayout>",
                drawn.xml
            );
            parts.insert(master_out[mi].layout_path.clone(), layout.into_bytes());
            let rels_path = master_out[mi].layout_path.replace("slideLayouts/", "slideLayouts/_rels/") + ".rels";
            parts.insert(rels_path, rels_xml(&ctx.rels).into_bytes());
            collected = ctx.collect.take().unwrap_or_default();
        }
        master_out[mi].background = background;
        master_out[mi].props = collected;
    }
    if master_out.is_empty() {
        master_out.push(MasterOut {
            slide_id: 0,
            styles: HashMap::new(),
            scheme: default_scheme,
            layout_path: "ppt/slideLayouts/slideLayout1.xml".into(),
            background: None,
            props: HashMap::new(),
        });
        parts.insert(
            "ppt/slideLayouts/slideLayout1.xml".into(),
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>").into_bytes(),
        );
        parts.insert(
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
            rels_xml(&[("rId1".into(), "slideMaster|../slideMasters/slideMaster1.xml".into())]).into_bytes(),
        );
    }
    // 마스터 바탕 없는 빈 레이아웃 (마스터 도형을 따르지 않는 쪽)
    parts.insert(
        "ppt/slideLayouts/slideLayout0.xml".into(),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld></p:sldLayout>").into_bytes(),
    );
    parts.insert(
        "ppt/slideLayouts/_rels/slideLayout0.xml.rels".into(),
        rels_xml(&[("rId1".into(), "slideMaster|../slideMasters/slideMaster1.xml".into())]).into_bytes(),
    );

    // 슬라이드
    let mut pres_rels: Vec<(String, String)> = vec![("rId1".into(), "slideMaster|slideMasters/slideMaster1.xml".into())];
    let mut sld_ids = String::new();
    let mut made = 0;
    for s in slides.iter() {
        let Some(rec) = persist.get(&s.persist).and_then(|&o| rec_at(&doc, o)).filter(|r| r.kind == 0x03EE) else { continue };
        let recs = children(rec.data);
        let atom = recs.iter().find(|r| r.kind == 0x03EF);
        let master_id = atom.and_then(|a| u32_at(a.data, 12)).unwrap_or(0);
        let sflags = atom.and_then(|a| u16_at(a.data, 20)).unwrap_or(0x07);
        let follow_objects = sflags & 0x01 != 0;
        let follow_scheme = sflags & 0x02 != 0;
        let follow_bg = sflags & 0x04 != 0;
        let mi = master_out.iter().position(|m| m.slide_id == master_id).unwrap_or(0);
        let scheme = if follow_scheme { master_out[mi].scheme } else { read_scheme(&recs).unwrap_or(master_out[mi].scheme) };
        let texts = &all_texts[s.texts_start..s.texts_end];
        let styles = if master_out[mi].styles.is_empty() { &empty_styles } else { &master_out[mi].styles };
        let layout_target = if follow_objects {
            master_out[mi].layout_path.trim_start_matches("ppt/slideLayouts/").to_owned()
        } else {
            "slideLayout0.xml".to_owned()
        };
        let mut drawn = Drawn { xml: String::new() };
        let mut background = None;
        let mut ctx = Ctx {
            master_props: &master_out[mi].props,
            collect: None,
            fonts: fonts.clone(),
            scheme,
            styles,
            blips: &blips,
            media: &mut media,
            rels: vec![("rId1".into(), format!("slideLayout|../slideLayouts/{layout_target}"))],
            next_id: 1,
            slide_texts: texts,
        };
        if let Some(dg) = recs.iter().find(|r| r.kind == 0x040C).and_then(|d| find(d.data, 0xF002)) {
            for item in children(dg.data) {
                match item.kind {
                    0xF003 => ctx.group(&item, true, Map::TOP, false, &mut drawn, &mut background),
                    0xF004 => ctx.shape(&item, true, Map::TOP, false, &mut drawn, &mut background),
                    _ => {}
                }
            }
        }
        let bg_fill = if follow_bg { master_out[mi].background.clone() } else { background.or(master_out[mi].background.clone()) };
        // 마스터 바탕을 따르면 레이아웃에 있으므로 쪽에는 쓰지 않는다 (같은 레이아웃이 나눠 그린다).
        let bg = if follow_bg && follow_objects {
            String::new()
        } else {
            bg_fill.map(|f| format!("<p:bg><p:bgPr>{f}<a:effectLst/></p:bgPr></p:bg>")).unwrap_or_default()
        };
        made += 1;
        let slide_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld {NS}><p:cSld>{bg}<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:spTree></p:cSld></p:sld>",
            drawn.xml
        );
        parts.insert(format!("ppt/slides/slide{made}.xml"), slide_xml.into_bytes());
        parts.insert(format!("ppt/slides/_rels/slide{made}.xml.rels"), rels_xml(&ctx.rels).into_bytes());
        let rid = format!("rIdS{made}");
        pres_rels.push((rid.clone(), format!("slide|slides/slide{made}.xml")));
        sld_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"{rid}\"/>", 255 + made));
    }
    if made == 0 {
        return Err("그릴 슬라이드가 없다".into());
    }
    // 마스터 바탕을 레이아웃 배경으로도 적는다(따르는 쪽).
    for m in &master_out {
        if let Some(bg) = &m.background {
            if let Some(xml) = parts.get_mut(&m.layout_path) {
                let s = String::from_utf8_lossy(xml).into_owned();
                if !s.contains("<p:bg>") {
                    let s = s.replacen("<p:cSld>", &format!("<p:cSld><p:bg><p:bgPr>{bg}<a:effectLst/></p:bgPr></p:bg>"), 1);
                    *xml = s.into_bytes();
                }
            }
        }
    }

    let first_font = fonts.first().cloned().unwrap_or_else(|| "Arial".into());
    let theme = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"ppt\"><a:themeElements><a:clrScheme name=\"ppt\"><a:dk1><a:srgbClr val=\"{d1}\"/></a:dk1><a:lt1><a:srgbClr val=\"{l1}\"/></a:lt1><a:dk2><a:srgbClr val=\"{d2}\"/></a:dk2><a:lt2><a:srgbClr val=\"{l2}\"/></a:lt2><a:accent1><a:srgbClr val=\"{a1}\"/></a:accent1><a:accent2><a:srgbClr val=\"{a2}\"/></a:accent2><a:accent3><a:srgbClr val=\"{a3}\"/></a:accent3><a:accent4><a:srgbClr val=\"A5A5A5\"/></a:accent4><a:accent5><a:srgbClr val=\"5B9BD5\"/></a:accent5><a:accent6><a:srgbClr val=\"70AD47\"/></a:accent6><a:hlink><a:srgbClr val=\"{a2}\"/></a:hlink><a:folHlink><a:srgbClr val=\"{a3}\"/></a:folHlink></a:clrScheme><a:fontScheme name=\"ppt\"><a:majorFont><a:latin typeface=\"{f}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"{f}\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"ppt\"><a:fillStyleLst/><a:lnStyleLst/><a:effectStyleLst/><a:bgFillStyleLst/></a:fmtScheme></a:themeElements></a:theme>",
        d1 = hex(scheme_rgb(&master_out[0].scheme, 1)),
        l1 = hex(scheme_rgb(&master_out[0].scheme, 0)),
        d2 = hex(scheme_rgb(&master_out[0].scheme, 3)),
        l2 = hex(scheme_rgb(&master_out[0].scheme, 4)),
        a1 = hex(scheme_rgb(&master_out[0].scheme, 5)),
        a2 = hex(scheme_rgb(&master_out[0].scheme, 6)),
        a3 = hex(scheme_rgb(&master_out[0].scheme, 7)),
        f = esc(&first_font)
    );
    parts.insert("ppt/theme/theme1.xml".into(), theme.into_bytes());
    parts.insert(
        "ppt/slideMasters/slideMaster1.xml".into(),
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldMaster {NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>").into_bytes(),
    );
    parts.insert(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
        rels_xml(&[("rId1".into(), "theme|../theme/theme1.xml".into())]).into_bytes(),
    );
    parts.insert(
        "ppt/presentation.xml".into(),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\"/></p:presentation>",
            sw as i64, sh as i64
        )
        .into_bytes(),
    );
    parts.insert("ppt/_rels/presentation.xml.rels".into(), rels_xml(&pres_rels).into_bytes());
    for (name, data) in media {
        parts.insert(format!("ppt/{name}"), data);
    }
    crate::note(format!("ppt: 슬라이드 {made}장, 마스터 {}개, 그림 {}개", master_out.len(), blips.iter().filter(|b| b.is_some()).count()));
    Ok(Package::from_parts(parts))
}

/// .ppt 가 쓰는 글꼴 이름들 (글꼴 맞춤용).
pub(crate) fn font_names(bytes: &[u8]) -> Vec<String> {
    let Ok(mut file) = cfb::CompoundFile::open(Cursor::new(bytes)) else { return Vec::new() };
    let Some(doc) = stream(&mut file, "/PowerPoint Document") else { return Vec::new() };
    let mut names = Vec::new();
    // 맨 위 Document 레코드 (덧저장이면 뒤의 것이 새것)
    let Some(d) = children(&doc).into_iter().filter(|r| r.kind == 0x03E8).last() else { return names };
    if let Some(env) = find(d.data, 0x03F2) {
        if let Some(fc) = find(env.data, 0x07D6) {
            for fe in children(fc.data).into_iter().filter(|r| r.kind == 0x0FB7) {
                let units: Vec<u16> = (0..32).filter_map(|k| u16_at(fe.data, k * 2)).take_while(|c| *c != 0).collect();
                let n = String::from_utf16_lossy(&units);
                if !n.is_empty() && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    names
}
