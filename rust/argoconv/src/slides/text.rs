//! 글 상자: 문단·글자 모양 물려받기, 줄 나누기, SVG 로 그리기.

use roxmltree::Node;

use super::color::{color_child, Color, Palette};
use super::fontbook::{is_east_asian, FontBook};
use super::geom::f;
use super::svg::{esc, Svg};

const EMU: f64 = 12700.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Align {
    L,
    Ctr,
    R,
    Just,
    Dist,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Spacing {
    Pct(f64),
    Pts(f64),
}

#[derive(Clone, Debug)]
pub(crate) enum Bullet {
    None,
    Char(String),
    Auto(String, u32),
}

#[derive(Clone, Debug)]
pub(crate) enum BuSize {
    Text,
    Pct(f64),
    Pts(f64),
}

#[derive(Clone, Default, Debug)]
pub(crate) struct RunStyle {
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub latin: Option<String>,
    pub ea: Option<String>,
    pub sym: Option<String>,
    /// Some(None) = 글자 채우기 없음
    pub color: Option<Option<Color>>,
    pub caps: Option<bool>,
    pub spc: Option<f64>,
    pub baseline: Option<f64>,
    pub highlight: Option<Color>,
    pub lang: Option<String>,
}

impl RunStyle {
    pub fn merge(&mut self, o: &RunStyle) {
        macro_rules! m {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        m!(size, bold, italic, underline, strike, latin, ea, sym, color, caps, spc, baseline, highlight, lang);
    }
}

#[derive(Clone, Default, Debug)]
pub(crate) struct ParaStyle {
    pub mar_l: Option<f64>,
    pub indent: Option<f64>,
    pub algn: Option<Align>,
    pub ln_spc: Option<Spacing>,
    pub spc_bef: Option<Spacing>,
    pub spc_aft: Option<Spacing>,
    pub bullet: Option<Bullet>,
    /// None = 글자 색을 따른다
    pub bu_color: Option<Option<Color>>,
    pub bu_size: Option<BuSize>,
    /// None = 글자 글꼴을 따른다
    pub bu_font: Option<Option<String>>,
    pub def_tab: Option<f64>,
    pub tabs: Option<Vec<(f64, char)>>,
    pub run: RunStyle,
}

impl ParaStyle {
    pub fn merge(&mut self, o: &ParaStyle) {
        macro_rules! m {
            ($($f:ident),*) => { $( if o.$f.is_some() { self.$f = o.$f.clone(); } )* };
        }
        m!(mar_l, indent, algn, ln_spc, spc_bef, spc_aft, bullet, bu_color, bu_size, bu_font, def_tab, tabs);
        self.run.merge(&o.run);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Levels(pub Vec<ParaStyle>);

impl Default for Levels {
    fn default() -> Self {
        Levels(vec![ParaStyle::default(); 9])
    }
}

impl Levels {
    /// lstStyle·titleStyle 같은 목록을 위에 덮는다.
    pub fn merge_list(&mut self, list: Option<Node<'_, '_>>, palette: &Palette) {
        let Some(list) = list else { return };
        if let Some(def) = list.children().find(|c| c.has_tag_name("defPPr")) {
            let p = read_ppr(def, palette);
            for lvl in &mut self.0 {
                lvl.merge(&p);
            }
        }
        for (i, lvl) in self.0.iter_mut().enumerate() {
            let name = format!("lvl{}pPr", i + 1);
            if let Some(n) = list.children().find(|c| c.is_element() && c.tag_name().name() == name) {
                lvl.merge(&read_ppr(n, palette));
            }
        }
    }

    pub fn merge_run_all(&mut self, run: &RunStyle) {
        for lvl in &mut self.0 {
            lvl.run.merge(run);
        }
    }
}

fn attr_f(n: Node<'_, '_>, a: &str) -> Option<f64> {
    n.attribute(a).and_then(|v| v.parse::<f64>().ok())
}

fn attr_b(n: Node<'_, '_>, a: &str) -> Option<bool> {
    n.attribute(a).map(|v| v == "1" || v == "true")
}

fn spacing(n: Option<Node<'_, '_>>) -> Option<Spacing> {
    let n = n?;
    if let Some(p) = n.children().find(|c| c.has_tag_name("spcPct")) {
        return Some(Spacing::Pct(attr_f(p, "val")? / 100000.0));
    }
    if let Some(p) = n.children().find(|c| c.has_tag_name("spcPts")) {
        return Some(Spacing::Pts(attr_f(p, "val")? / 100.0));
    }
    None
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

pub(crate) fn read_ppr(n: Node<'_, '_>, palette: &Palette) -> ParaStyle {
    let mut p = ParaStyle {
        mar_l: attr_f(n, "marL").map(|v| v / EMU),
        indent: attr_f(n, "indent").map(|v| v / EMU),
        algn: n.attribute("algn").map(|a| match a {
            "ctr" => Align::Ctr,
            "r" => Align::R,
            "just" | "justLow" => Align::Just,
            "dist" | "thaiDist" => Align::Dist,
            _ => Align::L,
        }),
        ln_spc: spacing(child(n, "lnSpc")),
        spc_bef: spacing(child(n, "spcBef")),
        spc_aft: spacing(child(n, "spcAft")),
        def_tab: attr_f(n, "defTabSz").map(|v| v / EMU),
        ..Default::default()
    };
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "buNone" => p.bullet = Some(Bullet::None),
            "buChar" => p.bullet = Some(Bullet::Char(c.attribute("char").unwrap_or("•").to_owned())),
            "buAutoNum" => {
                p.bullet = Some(Bullet::Auto(
                    c.attribute("type").unwrap_or("arabicPeriod").to_owned(),
                    attr_f(c, "startAt").unwrap_or(1.0) as u32,
                ))
            }
            "buBlip" => p.bullet = Some(Bullet::Char("•".into())),
            "buClrTx" => p.bu_color = Some(None),
            "buClr" => p.bu_color = Some(color_child(c, palette, None)),
            "buSzTx" => p.bu_size = Some(BuSize::Text),
            "buSzPct" => p.bu_size = attr_f(c, "val").map(|v| BuSize::Pct(v / 100000.0)),
            "buSzPts" => p.bu_size = attr_f(c, "val").map(|v| BuSize::Pts(v / 100.0)),
            "buFontTx" => p.bu_font = Some(None),
            "buFont" => p.bu_font = Some(c.attribute("typeface").map(|s| s.to_owned())),
            "tabLst" => {
                p.tabs = Some(
                    c.children()
                        .filter(|t| t.has_tag_name("tab"))
                        .filter_map(|t| {
                            let pos = attr_f(t, "pos")? / EMU;
                            let al = match t.attribute("algn") {
                                Some("r") => 'r',
                                Some("ctr") => 'c',
                                Some("dec") => 'd',
                                _ => 'l',
                            };
                            Some((pos, al))
                        })
                        .collect(),
                )
            }
            "defRPr" => p.run = read_rpr(c, palette),
            _ => {}
        }
    }
    p
}

pub(crate) fn read_rpr(n: Node<'_, '_>, palette: &Palette) -> RunStyle {
    let mut r = RunStyle {
        size: attr_f(n, "sz").map(|v| v / 100.0),
        bold: attr_b(n, "b"),
        italic: attr_b(n, "i"),
        underline: n.attribute("u").map(|v| v != "none"),
        strike: n.attribute("strike").map(|v| v != "noStrike"),
        caps: n.attribute("cap").map(|v| v == "all"),
        spc: attr_f(n, "spc").map(|v| v / 100.0),
        baseline: attr_f(n, "baseline").map(|v| v / 100000.0),
        lang: n.attribute("lang").map(|s| s.to_owned()),
        ..Default::default()
    };
    for c in n.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "latin" => r.latin = c.attribute("typeface").map(|s| s.to_owned()),
            "ea" => r.ea = c.attribute("typeface").map(|s| s.to_owned()),
            "sym" => r.sym = c.attribute("typeface").map(|s| s.to_owned()),
            "solidFill" => r.color = Some(color_child(c, palette, None)),
            "noFill" => r.color = Some(None),
            "gradFill" => {
                r.color = Some(
                    c.descendants()
                        .find(|g| g.has_tag_name("gs"))
                        .and_then(|gs| color_child(gs, palette, None)),
                )
            }
            "highlight" => r.highlight = color_child(c, palette, None),
            _ => {}
        }
    }
    r
}

// ---------------------------------------------------------------------------
// 글 상자 모양

#[derive(Clone, Debug)]
pub(crate) struct Body {
    pub l: f64,
    pub t: f64,
    pub r: f64,
    pub b: f64,
    pub anchor: char,
    pub wrap: bool,
    pub vert: String,
    pub rot: f64,
    pub cols: usize,
    pub col_gap: f64,
    pub font_scale: Option<f64>,
    pub ln_reduce: f64,
    pub norm_autofit: bool,
}

impl Default for Body {
    fn default() -> Self {
        Body {
            l: 7.2,
            t: 3.6,
            r: 7.2,
            b: 3.6,
            anchor: 't',
            wrap: true,
            vert: "horz".into(),
            rot: 0.0,
            cols: 1,
            col_gap: 0.0,
            font_scale: None,
            ln_reduce: 0.0,
            norm_autofit: false,
        }
    }
}

impl Body {
    pub fn merge(&mut self, n: Option<Node<'_, '_>>) {
        let Some(n) = n else { return };
        if let Some(v) = attr_f(n, "lIns") {
            self.l = v / EMU;
        }
        if let Some(v) = attr_f(n, "tIns") {
            self.t = v / EMU;
        }
        if let Some(v) = attr_f(n, "rIns") {
            self.r = v / EMU;
        }
        if let Some(v) = attr_f(n, "bIns") {
            self.b = v / EMU;
        }
        if let Some(a) = n.attribute("anchor") {
            self.anchor = match a {
                "ctr" => 'c',
                "b" => 'b',
                "just" | "dist" => 'c',
                _ => 't',
            };
        }
        if let Some(w) = n.attribute("wrap") {
            self.wrap = w != "none";
        }
        if let Some(v) = n.attribute("vert") {
            self.vert = v.to_owned();
        }
        if let Some(v) = attr_f(n, "rot") {
            self.rot = v / 60000.0;
        }
        if let Some(v) = attr_f(n, "numCol") {
            self.cols = (v as usize).clamp(1, 16);
        }
        if let Some(v) = attr_f(n, "spcCol") {
            self.col_gap = v / EMU;
        }
        if let Some(a) = child(n, "normAutofit") {
            self.norm_autofit = true;
            self.font_scale = attr_f(a, "fontScale").map(|v| v / 100000.0);
            self.ln_reduce = attr_f(a, "lnSpcReduction").map(|v| v / 100000.0).unwrap_or(0.0);
        } else if child(n, "noAutofit").is_some() || child(n, "spAutoFit").is_some() {
            self.norm_autofit = false;
            self.font_scale = None;
            self.ln_reduce = 0.0;
        }
    }
}

/// 테마 글꼴.
#[derive(Clone, Default, Debug)]
pub(crate) struct ThemeFonts {
    pub major_latin: String,
    pub major_ea: String,
    pub minor_latin: String,
    pub minor_ea: String,
    /// 문자 체계별 (Hang, Jpan, Hans …)
    pub major_script: Vec<(String, String)>,
    pub minor_script: Vec<(String, String)>,
}

impl ThemeFonts {
    fn ea_default(&self, major: bool, lang: Option<&str>) -> String {
        let (ea, scripts) =
            if major { (&self.major_ea, &self.major_script) } else { (&self.minor_ea, &self.minor_script) };
        if !ea.is_empty() {
            return ea.clone();
        }
        let want = match lang.unwrap_or("ko") {
            l if l.starts_with("ja") => "Jpan",
            l if l.starts_with("zh-TW") || l.starts_with("zh-HK") => "Hant",
            l if l.starts_with("zh") => "Hans",
            _ => "Hang",
        };
        scripts
            .iter()
            .find(|(s, _)| s == want)
            .or_else(|| scripts.iter().find(|(s, _)| s == "Hang"))
            .map(|(_, f)| f.clone())
            .unwrap_or_default()
    }

    pub fn resolve(&self, name: Option<&str>, ea: bool, lang: Option<&str>) -> Option<String> {
        let name = name?;
        let out = match name {
            "+mj-lt" => self.major_latin.clone(),
            "+mn-lt" => self.minor_latin.clone(),
            "+mj-ea" => self.ea_default(true, lang),
            "+mn-ea" => self.ea_default(false, lang),
            "+mj-cs" => self.major_latin.clone(),
            "+mn-cs" => self.minor_latin.clone(),
            other => other.to_owned(),
        };
        let _ = ea;
        (!out.is_empty()).then_some(out)
    }
}

// ---------------------------------------------------------------------------
// 배치

pub(crate) struct TextEnv<'a> {
    pub fonts: &'a mut FontBook,
    pub theme: &'a ThemeFonts,
    pub palette: &'a Palette,
    pub slide_no: usize,
}

#[derive(Clone, Debug)]
struct Glyph {
    ch: char,
    face: usize,
    size: f64,
    adv: f64,
    color: Option<Color>,
    underline: bool,
    strike: bool,
    shift: f64,
    spc: f64,
    highlight: Option<Color>,
    newline: bool,
    tab: bool,
    /// 얼굴에 굵은·기울인 꼴이 없으면 흉내 낸다
    fake_bold: bool,
    fake_italic: bool,
}

struct Para {
    style: ParaStyle,
    glyphs: Vec<Glyph>,
    bullet: Option<(String, usize, f64, Option<Color>, f64)>,
    /// 빈 문단의 높이를 잡을 글자 크기와 얼굴
    end_size: f64,
    end_face: usize,
}

struct LineOut {
    start: usize,
    end: usize,
    x0: f64,
    width: f64,
    height: f64,
    descent: f64,
    first: bool,
    last: bool,
    spaces: usize,
}

fn symbol_glyph(c: char, font: &str) -> Option<char> {
    let lower = font.to_ascii_lowercase();
    let symbol = lower.starts_with("wingdings")
        || lower.starts_with("webdings")
        || lower.starts_with("symbol")
        || lower.starts_with("monotype sorts");
    if !symbol {
        return None;
    }
    let code = c as u32;
    let code = if (0xF000..=0xF0FF).contains(&code) { code - 0xF000 } else { code };
    Some(match code {
        0xA7 => '\u{25AA}',
        0x6E => '\u{25A0}',
        0x6C => '\u{25CF}',
        0x75 => '\u{25C6}',
        0x71 | 0x70 => '\u{2751}',
        0xFC => '\u{2713}',
        0xFE => '\u{2611}',
        0xD8 | 0xE0 | 0xE8 => '\u{27A2}',
        0xF0 => '\u{21E8}',
        0xB7 => '\u{2022}',
        0x76 => '\u{2756}',
        0x9F => '\u{2022}',
        0xA8 => '\u{25FB}',
        0x6F => '\u{25A1}',
        0x3E => '\u{2192}',
        0xAE => '\u{25C6}',
        _ => '\u{2022}',
    })
}

fn no_break_before(c: char) -> bool {
    ")]}>,.!?;:%、。，．！？」』】〕）〉》›»’”…‥ー々〜〟·・ゝゞヽヾ".contains(c)
}

fn no_break_after(c: char) -> bool {
    "([{<「『【〔（〈《‹«‘“$₩￦".contains(c)
}

fn ideographic(c: char) -> bool {
    is_east_asian(c) && !matches!(c as u32, 0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F)
}

fn can_break(a: char, b: char) -> bool {
    if a == ' ' || a == '\u{3000}' {
        return b != ' ';
    }
    if b == ' ' {
        return false;
    }
    if (ideographic(a) || ideographic(b)) && !no_break_before(b) && !no_break_after(a) {
        return true;
    }
    a == '-' && b.is_alphanumeric()
}

fn number_text(scheme: &str, n: u32) -> String {
    let roman = |mut n: u32| {
        const T: [(u32, &str); 13] = [
            (1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"),
            (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I"),
        ];
        let mut s = String::new();
        for (v, r) in T {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    };
    let alpha = |n: u32| {
        let mut n = n.max(1);
        let mut v = Vec::new();
        while n > 0 {
            v.push((b'A' + ((n - 1) % 26) as u8) as char);
            n = (n - 1) / 26;
        }
        v.iter().rev().collect::<String>()
    };
    let circled = |n: u32| char::from_u32(if (1..=20).contains(&n) { 0x2460 + n - 1 } else { 0x2460 }).unwrap_or('①').to_string();
    let hangul = ["가", "나", "다", "라", "마", "바", "사", "아", "자", "차", "카", "타", "파", "하"];
    let ganada = |n: u32| hangul[((n.max(1) - 1) as usize) % hangul.len()].to_string();
    let body = if scheme.starts_with("romanUc") {
        roman(n)
    } else if scheme.starts_with("romanLc") {
        roman(n).to_lowercase()
    } else if scheme.starts_with("alphaUc") {
        alpha(n)
    } else if scheme.starts_with("alphaLc") {
        alpha(n).to_lowercase()
    } else if scheme.starts_with("circleNum") {
        return circled(n);
    } else if scheme.starts_with("hanjaKor") || scheme.starts_with("ea1Kor") || scheme.contains("Ganada") || scheme.starts_with("hangul") {
        ganada(n)
    } else {
        n.to_string()
    };
    if scheme.ends_with("ParenBoth") {
        format!("({body})")
    } else if scheme.ends_with("ParenR") {
        format!("{body})")
    } else if scheme.ends_with("Period") || scheme.ends_with("Comma") {
        format!("{body}.")
    } else if scheme.ends_with("Minus") {
        format!("- {body} -")
    } else {
        body
    }
}

/// 글 상자 하나를 그린다. (x, y, w, h) 는 지역 좌표의 글 사각형(여백 넣기 전).
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    svg: &mut Svg,
    env: &mut TextEnv<'_>,
    tx_body: Node<'_, '_>,
    levels: &Levels,
    body: &Body,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) {
    let vertical = matches!(body.vert.as_str(), "vert" | "eaVert" | "vert270" | "wordArtVert" | "mongolianVert" | "wordArtVertRtl");
    let (bw, bh) = if vertical { (h, w) } else { (w, h) };
    let inner_w = (bw - body.l - body.r).max(1.0);
    let inner_h = (bh - body.t - body.b).max(1.0);

    // 맞춤 줄이기: 저장된 배율이 없고 넘치면 직접 줄인다.
    let mut scale = body.font_scale.unwrap_or(1.0);
    let mut reduce = body.ln_reduce;
    let mut paras = collect(env, tx_body, levels, scale);
    let col_w = if body.cols > 1 { (inner_w - body.col_gap * (body.cols - 1) as f64) / body.cols as f64 } else { inner_w };
    let mut lines = layout_all(env, &paras, col_w, body.wrap, reduce);
    if body.norm_autofit && body.font_scale.is_none() {
        let mut tries = 0;
        while total_height(&lines) > inner_h * body.cols as f64 + 0.5 && tries < 12 {
            scale *= 0.9;
            reduce = (reduce + 0.05).min(0.2);
            paras = collect(env, tx_body, levels, scale);
            lines = layout_all(env, &paras, col_w, body.wrap, reduce);
            tries += 1;
        }
    }

    let mut out = String::new();
    // 단 나누기
    let mut columns: Vec<Vec<(usize, usize)>> = vec![Vec::new()];
    if body.cols > 1 {
        let mut used = 0.0;
        for (pi, pl) in lines.iter().enumerate() {
            for li in 0..pl.1.len() {
                let lh = pl.1[li].height + pl.1[li].extra_before + pl.1[li].extra_after;
                if used + lh > inner_h + 0.5 && !columns.last().unwrap().is_empty() && columns.len() < body.cols {
                    columns.push(Vec::new());
                    used = 0.0;
                }
                used += lh;
                columns.last_mut().unwrap().push((pi, li));
            }
        }
    } else {
        for (pi, pl) in lines.iter().enumerate() {
            for li in 0..pl.1.len() {
                columns[0].push((pi, li));
            }
        }
    }

    for (ci, col) in columns.iter().enumerate() {
        let height: f64 = col
            .iter()
            .enumerate()
            .map(|(k, &(pi, li))| {
                let l = &lines[pi].1[li];
                l.height + if k == 0 { 0.0 } else { l.extra_before } + if k + 1 == col.len() { 0.0 } else { l.extra_after }
            })
            .sum();
        let top = match body.anchor {
            'c' => body.t + (inner_h - height) / 2.0,
            'b' => body.t + inner_h - height,
            _ => body.t,
        };
        let left = body.l + ci as f64 * (col_w + body.col_gap);
        let mut cy = top;
        for (k, &(pi, li)) in col.iter().enumerate() {
            let pl = &lines[pi];
            let line = &pl.1[li];
            if k > 0 {
                cy += line.extra_before;
            }
            let baseline = cy + line.height - line.descent;
            emit_line(&mut out, env, &paras[pi], line, left, baseline, col_w, cy, line.height, body.vert == "eaVert");
            cy += line.height;
            if k + 1 < col.len() {
                cy += line.extra_after;
            }
        }
    }

    if out.is_empty() {
        return;
    }
    let mut transform = format!("translate({} {})", f(x), f(y));
    match body.vert.as_str() {
        "vert" | "eaVert" | "wordArtVert" | "mongolianVert" | "wordArtVertRtl" => {
            transform = format!("translate({} {}) rotate(90)", f(x + w), f(y));
        }
        "vert270" => transform = format!("translate({} {}) rotate(-90)", f(x), f(y + h)),
        _ => {}
    }
    if body.rot != 0.0 {
        transform = format!("rotate({} {} {}) {transform}", f(body.rot), f(x + w / 2.0), f(y + h / 2.0));
    }
    svg.body.push_str(&format!("<g transform=\"{transform}\">{out}</g>"));
}

struct LaidLine {
    line: LineOut,
    height: f64,
    descent: f64,
    extra_before: f64,
    extra_after: f64,
}

impl std::ops::Deref for LaidLine {
    type Target = LineOut;
    fn deref(&self) -> &LineOut {
        &self.line
    }
}

fn total_height(lines: &[(usize, Vec<LaidLine>)]) -> f64 {
    let mut first = true;
    let mut total = 0.0;
    let n: usize = lines.iter().map(|l| l.1.len()).sum();
    let mut k = 0;
    for (_, pl) in lines {
        for l in pl {
            total += l.height;
            if !first {
                total += l.extra_before;
            }
            k += 1;
            if k < n {
                total += l.extra_after;
            }
            first = false;
        }
    }
    total
}

fn collect(env: &mut TextEnv<'_>, tx_body: Node<'_, '_>, levels: &Levels, scale: f64) -> Vec<Para> {
    let mut paras = Vec::new();
    let mut counters: Vec<Option<(String, u32)>> = vec![None; 9];
    for p in tx_body.children().filter(|c| c.has_tag_name("p")) {
        let ppr = child(p, "pPr");
        let lvl = ppr.and_then(|n| attr_f(n, "lvl")).unwrap_or(0.0).clamp(0.0, 8.0) as usize;
        let mut style = levels.0[lvl].clone();
        if let Some(ppr) = ppr {
            style.merge(&read_ppr(ppr, env.palette));
        }
        let mut glyphs = Vec::new();
        let mut first_run: Option<RunStyle> = None;
        for r in p.children().filter(|c| c.is_element()) {
            let name = r.tag_name().name();
            if name != "r" && name != "br" && name != "fld" {
                continue;
            }
            let mut rs = style.run.clone();
            let rpr = child(r, "rPr");
            if let Some(rpr) = rpr {
                rs.merge(&read_rpr(rpr, env.palette));
            }
            // 하이퍼링크: 밑줄, 그리고 (글자 색을 따르라는 표시가 없으면) 테마의 링크 색.
            if let Some(h) = rpr.and_then(|n| child(n, "hlinkClick")).filter(|h| h.attributes().any(|a| a.name() == "id" && !a.value().is_empty()) || h.attribute("action").is_some()) {
                let keep = h.descendants().any(|d| d.tag_name().name() == "hlinkClr" && d.attribute("val") == Some("tx"));
                if !keep {
                    if let Some(c) = env.palette.scheme_color("hlink") {
                        rs.color = Some(Some(c));
                    }
                }
                rs.underline = Some(true);
            }
            if name == "br" {
                push_text(env, &mut glyphs, &rs, "\n", scale);
                continue;
            }
            let mut text: String = child(r, "t").and_then(|t| t.text()).unwrap_or("").to_owned();
            if name == "fld" && r.attribute("type") == Some("slidenum") {
                text = env.slide_no.to_string();
            }
            if text.is_empty() {
                continue;
            }
            if first_run.is_none() {
                first_run = Some(rs.clone());
            }
            push_text(env, &mut glyphs, &rs, &text, scale);
        }
        let mut end = style.run.clone();
        if let Some(e) = child(p, "endParaRPr") {
            end.merge(&read_rpr(e, env.palette));
        }
        let base_run = first_run.clone().unwrap_or(end.clone());
        let end_size = base_run.size.unwrap_or(18.0) * scale;
        let end_face = {
            let fam = env.theme.resolve(base_run.latin.as_deref().or(Some("+mn-lt")), false, base_run.lang.as_deref());
            env.fonts.resolve(fam.as_deref(), base_run.bold.unwrap_or(false), base_run.italic.unwrap_or(false))
        };

        // 글머리
        for c in counters.iter_mut().skip(lvl + 1) {
            *c = None;
        }
        let visible = glyphs.iter().any(|g| !g.newline);
        let mut bullet = None;
        if visible {
            match style.bullet.clone().unwrap_or(Bullet::None) {
                Bullet::None => counters[lvl] = None,
                kind => {
                    let text = match &kind {
                        Bullet::Char(c) => c.clone(),
                        Bullet::Auto(scheme, start) => {
                            let n = match &counters[lvl] {
                                Some((s, n)) if s == scheme => n + 1,
                                _ => *start,
                            };
                            counters[lvl] = Some((scheme.clone(), n));
                            number_text(scheme, n)
                        }
                        Bullet::None => String::new(),
                    };
                    if !matches!(kind, Bullet::Auto(..)) {
                        counters[lvl] = None;
                    }
                    let first_size = glyphs.first().map(|g| g.size).unwrap_or(end_size);
                    let size = match style.bu_size.clone().unwrap_or(BuSize::Text) {
                        BuSize::Text => first_size,
                        BuSize::Pct(p) => first_size * p,
                        BuSize::Pts(p) => p * scale,
                    };
                    let color = match style.bu_color {
                        Some(Some(c)) => Some(c),
                        _ => glyphs.first().and_then(|g| g.color),
                    };
                    let bu_font = match style.bu_font.clone() {
                        Some(Some(fnt)) => Some(fnt),
                        _ => None,
                    };
                    let mut text_out = text.clone();
                    let mut family = bu_font.clone();
                    if let Some(fnt) = &bu_font {
                        if matches!(kind, Bullet::Char(_)) {
                            let mapped: String = text.chars().map(|c| symbol_glyph(c, fnt).unwrap_or(c)).collect();
                            if mapped != text {
                                text_out = mapped;
                                family = None;
                            }
                        }
                    }
                    let face = match family {
                        Some(fam) => {
                            let fam = env.theme.resolve(Some(&fam), false, None);
                            env.fonts.resolve(fam.as_deref(), false, false)
                        }
                        None => glyphs.first().map(|g| g.face).unwrap_or(end_face),
                    };
                    let c0 = text_out.chars().next().unwrap_or('•');
                    let face = env.fonts.face_for_char(face, c0);
                    let width: f64 = text_out.chars().map(|c| env.fonts.advance(face, c, size)).sum::<f64>() + size * 0.25;
                    bullet = Some((text_out, face, size, color, width));
                }
            }
        }
        paras.push(Para { style, glyphs, bullet, end_size, end_face });
    }
    paras
}

fn push_text(env: &mut TextEnv<'_>, glyphs: &mut Vec<Glyph>, rs: &RunStyle, text: &str, scale: f64) {
    let bold = rs.bold.unwrap_or(false);
    let italic = rs.italic.unwrap_or(false);
    let mut size = rs.size.unwrap_or(18.0) * scale;
    let shift = rs.baseline.unwrap_or(0.0);
    if shift != 0.0 {
        size *= 2.0 / 3.0;
    }
    let lang = rs.lang.as_deref();
    let latin = env.theme.resolve(rs.latin.as_deref().or(Some("+mn-lt")), false, lang);
    let ea = env.theme.resolve(rs.ea.as_deref().or(Some("+mn-ea")), true, lang).or_else(|| latin.clone());
    let sym = rs.sym.clone();
    let latin_face = env.fonts.resolve(latin.as_deref(), bold, italic);
    let ea_face = env.fonts.resolve(ea.as_deref(), bold, italic);
    let color = match rs.color {
        Some(c) => c,
        None => Some(Color::BLACK),
    };
    let upper = rs.caps.unwrap_or(false);
    let spc = rs.spc.unwrap_or(0.0) * scale;
    for mut c in text.chars() {
        if upper {
            c = c.to_uppercase().next().unwrap_or(c);
        }
        let newline = c == '\n' || c == '\u{b}' || c == '\r';
        let tab = c == '\t';
        // 기호 글꼴의 사설 영역 글자는 뜻이 같은 유니코드로.
        if (0xF000..=0xF0FF).contains(&(c as u32)) {
            let font = sym.clone().or_else(|| latin.clone()).unwrap_or_default();
            c = symbol_glyph(c, &font).unwrap_or('\u{2022}');
        } else if let Some(m) = latin.as_deref().and_then(|l| symbol_glyph(c, l)) {
            c = m;
        }
        let want = if is_east_asian(c) { ea_face } else { latin_face };
        let face = if newline || tab { want } else { env.fonts.face_for_char(want, c) };
        let adv = if newline { 0.0 } else { env.fonts.advance(face, if c == '\u{a0}' { ' ' } else { c }, size) + spc };
        let fake_bold = bold && !env.fonts.is_bold(face);
        let fake_italic = italic && !env.fonts.is_italic(face);
        glyphs.push(Glyph {
            fake_bold,
            fake_italic,
            ch: if c == '\u{a0}' { ' ' } else { c },
            face,
            size,
            adv,
            color,
            underline: rs.underline.unwrap_or(false),
            strike: rs.strike.unwrap_or(false),
            shift,
            spc,
            highlight: rs.highlight,
            newline,
            tab,
        });
    }
}

fn tab_stop(style: &ParaStyle, x: f64) -> f64 {
    if let Some(tabs) = &style.tabs {
        if let Some((pos, _)) = tabs.iter().find(|(p, _)| *p > x + 0.01) {
            return *pos;
        }
    }
    let d = style.def_tab.unwrap_or(72.0).max(6.0);
    ((x / d).floor() + 1.0) * d
}

fn layout_all(env: &mut TextEnv<'_>, paras: &[Para], width: f64, wrap: bool, reduce: f64) -> Vec<(usize, Vec<LaidLine>)> {
    let mut out = Vec::new();
    for (pi, p) in paras.iter().enumerate() {
        let lines = break_lines(p, width, wrap);
        let mut laid = Vec::new();
        let n = lines.len();
        for (li, l) in lines.into_iter().enumerate() {
            let (mut asc, mut desc, mut size) = (0.0f64, 0.0f64, 0.0f64);
            let slice = &p.glyphs[l.start..l.end];
            for g in slice.iter().filter(|g| !g.newline) {
                let m = env.fonts.metrics(g.face);
                asc = asc.max(m.ascent as f64 * g.size);
                desc = desc.max(m.descent as f64 * g.size);
                size = size.max(g.size);
            }
            if size == 0.0 {
                let m = env.fonts.metrics(p.end_face);
                asc = m.ascent as f64 * p.end_size;
                desc = m.descent as f64 * p.end_size;
                size = p.end_size;
            }
            if let Some((_, face, bsize, _, _)) = &p.bullet {
                if l.first {
                    let m = env.fonts.metrics(*face);
                    asc = asc.max(m.ascent as f64 * bsize);
                    desc = desc.max(m.descent as f64 * bsize);
                }
            }
            let natural = asc + desc;
            let height = match p.style.ln_spc {
                Some(Spacing::Pts(v)) => v * (1.0 - reduce),
                Some(Spacing::Pct(v)) => natural * (v - reduce).max(0.1),
                None => natural * (1.0 - reduce),
            };
            let spc = |s: Option<Spacing>| match s {
                Some(Spacing::Pts(v)) => v,
                Some(Spacing::Pct(v)) => v * size * 1.2,
                None => 0.0,
            };
            let extra_before = if li == 0 { spc(p.style.spc_bef) } else { 0.0 };
            let extra_after = if li + 1 == n { spc(p.style.spc_aft) } else { 0.0 };
            let descent = if height < natural { desc * height / natural } else { desc };
            laid.push(LaidLine { line: l, height, descent, extra_before, extra_after });
        }
        out.push((pi, laid));
    }
    out
}

fn break_lines(p: &Para, width: f64, wrap: bool) -> Vec<LineOut> {
    let mar_l = p.style.mar_l.unwrap_or(0.0);
    let indent = p.style.indent.unwrap_or(0.0);
    let bullet_w = p.bullet.as_ref().map(|b| b.4).unwrap_or(0.0);
    let first_x = if p.bullet.is_some() {
        if indent < 0.0 && bullet_w <= -indent + 0.1 {
            mar_l
        } else {
            (mar_l + indent + bullet_w).max(mar_l)
        }
    } else {
        (mar_l + indent).max(0.0)
    };
    let mut lines = Vec::new();
    let g = &p.glyphs;
    if g.is_empty() {
        lines.push(LineOut { start: 0, end: 0, x0: first_x, width: 0.0, height: 0.0, descent: 0.0, first: true, last: true, spaces: 0 });
        return lines;
    }
    let mut start = 0;
    let mut first = true;
    while start < g.len() {
        let x0 = if first { first_x } else { mar_l };
        let avail = (width - x0).max(1.0);
        let mut x = 0.0;
        let mut i = start;
        let mut last_break: Option<usize> = None;
        let mut end = g.len();
        let mut forced = false;
        while i < g.len() {
            let gl = &g[i];
            if gl.newline {
                end = i + 1;
                forced = true;
                break;
            }
            let adv = if gl.tab { tab_stop(&p.style, x0 + x) - (x0 + x) } else { gl.adv };
            let trailing_space = gl.ch == ' ';
            if wrap && x + adv > avail + 0.01 && !trailing_space && i > start {
                end = match last_break {
                    Some(b) if b > start => b,
                    _ => i,
                };
                break;
            }
            x += adv;
            if i + 1 < g.len() && can_break(gl.ch, g[i + 1].ch) {
                last_break = Some(i + 1);
            }
            i += 1;
        }
        if i >= g.len() && !forced {
            end = g.len();
        }
        // 폭 다시 재기 (끝 공백 빼고)
        let mut w = 0.0;
        let mut content_end = end;
        while content_end > start && (g[content_end - 1].ch == ' ' || g[content_end - 1].newline) {
            content_end -= 1;
        }
        let mut cx = x0;
        let mut spaces = 0;
        for gl in &g[start..content_end] {
            let adv = if gl.tab { tab_stop(&p.style, cx) - cx } else { gl.adv };
            cx += adv;
            w += adv;
            if gl.ch == ' ' {
                spaces += 1;
            }
        }
        let last = end >= g.len();
        lines.push(LineOut { start, end, x0, width: w, height: 0.0, descent: 0.0, first, last: last || forced, spaces });
        if end <= start {
            break;
        }
        start = end;
        first = false;
        if start >= g.len() && forced {
            // 줄바꿈으로 끝나면 빈 줄 하나가 더 있다.
            lines.push(LineOut { start, end: start, x0: mar_l, width: 0.0, height: 0.0, descent: 0.0, first: false, last: true, spaces: 0 });
        }
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn emit_line(
    out: &mut String,
    env: &mut TextEnv<'_>,
    p: &Para,
    line: &LaidLine,
    left: f64,
    baseline: f64,
    width: f64,
    top: f64,
    height: f64,
    upright_cjk: bool,
) {
    let g = &p.glyphs;
    let mut content_end = line.end;
    while content_end > line.start && (g[content_end - 1].ch == ' ' || g[content_end - 1].newline) {
        content_end -= 1;
    }
    let align = p.style.algn.unwrap_or(Align::L);
    let avail = width - line.x0;
    let mut x = left
        + line.x0
        + match align {
            Align::Ctr => (avail - line.width) / 2.0,
            Align::R => avail - line.width,
            _ => 0.0,
        };
    let justify = (align == Align::Just && !line.last || align == Align::Dist) && line.spaces > 0;
    let extra = if justify { (avail - line.width) / line.spaces as f64 } else { 0.0 };

    // 글머리
    if line.first {
        if let Some((text, face, size, color, _)) = &p.bullet {
            let mar_l = p.style.mar_l.unwrap_or(0.0);
            let indent = p.style.indent.unwrap_or(0.0);
            let bx = left + (mar_l + indent).max(0.0);
            let fill = color.unwrap_or(Color::BLACK);
            out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"f{face}\" font-size=\"{}\" fill=\"{}\"{} font-kerning=\"none\">{}</text>",
                f(bx),
                f(baseline),
                f(*size),
                fill.css(),
                opacity(fill.a),
                esc(text)
            ));
        }
    }

    let mut i = line.start;
    let mut cx_rel = line.x0;
    while i < content_end {
        let gl = &g[i];
        if gl.tab {
            let stop = tab_stop(&p.style, cx_rel);
            x += stop - cx_rel;
            cx_rel = stop;
            i += 1;
            continue;
        }
        // 같은 모양이 이어지는 동안 한 덩어리.
        let mut j = i;
        let mut seg = String::new();
        let mut seg_w = 0.0;
        while j < content_end {
            let h = &g[j];
            if h.tab || h.face != gl.face || h.size != gl.size || h.color != gl.color || h.shift != gl.shift
                || h.fake_bold != gl.fake_bold || h.fake_italic != gl.fake_italic
                || h.underline != gl.underline || h.strike != gl.strike || h.highlight != gl.highlight
                || (justify && h.ch == ' ' && j > i)
                || (upright_cjk && is_east_asian(h.ch) && j > i)
                || (upright_cjk && is_east_asian(gl.ch) && j > i)
            {
                break;
            }
            seg.push(h.ch);
            seg_w += h.adv;
            j += 1;
            if justify && h.ch == ' ' {
                break;
            }
        }
        if j == i {
            j = i + 1;
            seg.push(gl.ch);
            seg_w = gl.adv;
        }
        let y = baseline - gl.shift * gl.size * 1.5;
        if let Some(hl) = gl.highlight {
            out.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                f(x), f(top), f(seg_w), f(height), hl.css()
            ));
        }
        if let Some(color) = gl.color {
            let text = seg.trim_end_matches(' ');
            if !text.trim().is_empty() {
                let mut attrs = format!(
                    "x=\"{}\" y=\"{}\" font-family=\"f{}\" font-size=\"{}\" fill=\"{}\"{} font-kerning=\"none\"",
                    f(x),
                    f(y),
                    gl.face,
                    f(gl.size),
                    color.css(),
                    opacity(color.a)
                );
                if gl.spc != 0.0 {
                    attrs.push_str(&format!(" letter-spacing=\"{}\"", f(gl.spc)));
                }
                if gl.fake_bold {
                    attrs.push_str(&format!(
                        " stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"{}",
                        color.css(),
                        f(gl.size * 0.035),
                        if color.a < 0.999 { format!(" stroke-opacity=\"{}\"", f(color.a as f64)) } else { String::new() }
                    ));
                }
                if upright_cjk && is_east_asian(gl.ch) {
                    let cxm = x + seg_w / 2.0;
                    let cym = y - gl.size * 0.38;
                    attrs.push_str(&format!(" transform=\"rotate(-90 {} {})\"", f(cxm), f(cym)));
                } else if gl.fake_italic {
                    attrs.push_str(&format!(" transform=\"translate({} {}) skewX(-11) translate({} {})\"", f(x), f(y), f(-x), f(-y)));
                }
                if text.starts_with(' ') || text.contains("  ") {
                    attrs.push_str(" xml:space=\"preserve\"");
                }
                out.push_str(&format!("<text {attrs}>{}</text>", esc(text)));
            }
            if gl.underline {
                let th = (gl.size * 0.06).max(0.5);
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}/>",
                    f(x), f(y + gl.size * 0.11), f(seg_w), f(th), color.css(), opacity(color.a)
                ));
            }
            if gl.strike {
                let th = (gl.size * 0.05).max(0.5);
                out.push_str(&format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{}/>",
                    f(x), f(y - gl.size * 0.3), f(seg_w), f(th), color.css(), opacity(color.a)
                ));
            }
        }
        x += seg_w;
        cx_rel += seg_w;
        if justify && seg.ends_with(' ') {
            x += extra;
        }
        let _ = env.slide_no;
        i = j;
    }
}

fn opacity(a: f32) -> String {
    if a < 0.999 {
        format!(" fill-opacity=\"{}\"", f(a as f64))
    } else {
        String::new()
    }
}

/// 글 상자의 글 높이(여백 포함). 표 칸 높이를 잡을 때 쓴다.
pub(crate) fn measure(env: &mut TextEnv<'_>, tx_body: Node<'_, '_>, levels: &Levels, body: &Body, w: f64) -> f64 {
    let inner_w = (w - body.l - body.r).max(1.0);
    let paras = collect(env, tx_body, levels, body.font_scale.unwrap_or(1.0));
    let lines = layout_all(env, &paras, inner_w, body.wrap, body.ln_reduce);
    total_height(&lines) + body.t + body.b
}

/// 글 상자에 보이는 글자가 있는지 (빈 자리표시자 거르기).
pub(crate) fn has_text(tx_body: Node<'_, '_>) -> bool {
    tx_body
        .descendants()
        .filter(|n| n.has_tag_name("t"))
        .any(|t| t.text().map(|s| !s.trim().is_empty()).unwrap_or(false))
        || tx_body.descendants().any(|n| n.has_tag_name("fld"))
}

/// 한 줄짜리 글(차트 이름표 등). 글꼴은 글자마다 가진 얼굴로 나눈다.
/// anchor: 'l' 왼쪽, 'c' 가운데, 'r' 오른쪽 기준. 폭을 돌려준다.
#[allow(clippy::too_many_arguments)]
pub(crate) fn label(
    svg: &mut Svg,
    fonts: &mut FontBook,
    family: Option<&str>,
    text: &str,
    x: f64,
    y: f64,
    size: f64,
    color: Color,
    bold: bool,
    anchor: char,
    rotate: f64,
) -> f64 {
    let base = fonts.resolve(family, bold, false);
    let mut segs: Vec<(usize, String, f64)> = Vec::new();
    for c in text.chars() {
        let face = fonts.face_for_char(base, c);
        let adv = fonts.advance(face, c, size);
        match segs.last_mut() {
            Some(s) if s.0 == face => {
                s.1.push(c);
                s.2 += adv;
            }
            _ => segs.push((face, c.to_string(), adv)),
        }
    }
    let width: f64 = segs.iter().map(|s| s.2).sum();
    let mut cx = match anchor {
        'c' => x - width / 2.0,
        'r' => x - width,
        _ => x,
    };
    let mut out = String::new();
    for (face, s, w) in &segs {
        if !s.trim().is_empty() {
            out.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-family=\"f{face}\" font-size=\"{}\" fill=\"{}\"{} font-kerning=\"none\" xml:space=\"preserve\">{}</text>",
                f(cx),
                f(y),
                f(size),
                color.css(),
                opacity(color.a),
                esc(s)
            ));
        }
        cx += w;
    }
    if rotate != 0.0 {
        svg.body.push_str(&format!("<g transform=\"rotate({} {} {})\">{out}</g>", f(rotate), f(x), f(y)));
    } else {
        svg.body.push_str(&out);
    }
    width
}

pub(crate) fn label_width(fonts: &mut FontBook, family: Option<&str>, text: &str, size: f64, bold: bool) -> f64 {
    let base = fonts.resolve(family, bold, false);
    text.chars()
        .map(|c| {
            let face = fonts.face_for_char(base, c);
            fonts.advance(face, c, size)
        })
        .sum()
}
