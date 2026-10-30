//! 슬라이드를 SVG 로: 테마·마스터·레이아웃 물려받기, 도형·그림·묶음·표·차트.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use super::chart;
use super::color::{Color, Palette};
use super::fill::{self, Blip, Fill, Line, Shadow};
use super::fontbook::FontBook;
use super::geom::{self, f, Cmd, FillMode, Geometry};
use super::package::{dir_of, Package, Rel};
use super::svg::{base64, esc, Svg};
use super::table;
use super::text::{self, Body, Levels, RunStyle, TextEnv, ThemeFonts};

pub(crate) const EMU: f64 = 12700.0;

pub(crate) fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

pub(crate) fn path_child<'a, 'i>(n: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    let mut cur = n;
    for name in names {
        cur = child(cur, name)?;
    }
    Some(cur)
}

fn num(n: Node<'_, '_>, a: &str) -> Option<f64> {
    n.attribute(a).and_then(|v| v.parse::<f64>().ok())
}

pub(crate) struct Theme<'a, 'i> {
    pub scheme: HashMap<String, Color>,
    pub fonts: ThemeFonts,
    pub fill_styles: Option<Node<'a, 'i>>,
    pub line_styles: Option<Node<'a, 'i>>,
    pub effect_styles: Option<Node<'a, 'i>>,
    pub bg_styles: Option<Node<'a, 'i>>,
}

impl<'a, 'i> Theme<'a, 'i> {
    pub fn read(doc: Option<&'a Document<'i>>) -> Theme<'a, 'i> {
        let mut t = Theme {
            scheme: default_scheme(),
            fonts: ThemeFonts {
                major_latin: "Calibri Light".into(),
                minor_latin: "Calibri".into(),
                ..Default::default()
            },
            fill_styles: None,
            line_styles: None,
            effect_styles: None,
            bg_styles: None,
        };
        let Some(doc) = doc else { return t };
        let root = doc.root_element();
        let elements = path_child(root, &["themeElements"]);
        if let Some(cs) = elements.and_then(|e| child(e, "clrScheme")) {
            for c in cs.children().filter(|c| c.is_element()) {
                let pal = Palette::default();
                if let Some(color) = super::color::color_child(c, &pal, None) {
                    t.scheme.insert(c.tag_name().name().to_owned(), color);
                }
            }
        }
        if let Some(fs) = elements.and_then(|e| child(e, "fontScheme")) {
            let read = |n: Option<Node<'_, '_>>| -> (String, String, Vec<(String, String)>) {
                let Some(n) = n else { return Default::default() };
                let tf = |name: &str| child(n, name).and_then(|c| c.attribute("typeface")).unwrap_or("").to_owned();
                let scripts = n
                    .children()
                    .filter(|c| c.has_tag_name("font"))
                    .filter_map(|c| Some((c.attribute("script")?.to_owned(), c.attribute("typeface")?.to_owned())))
                    .collect();
                (tf("latin"), tf("ea"), scripts)
            };
            let (ml, me, ms) = read(child(fs, "majorFont"));
            let (nl, ne, ns) = read(child(fs, "minorFont"));
            t.fonts = ThemeFonts {
                major_latin: ml,
                major_ea: me,
                minor_latin: nl,
                minor_ea: ne,
                major_script: ms,
                minor_script: ns,
            };
        }
        if let Some(fmt) = elements.and_then(|e| child(e, "fmtScheme")) {
            t.fill_styles = child(fmt, "fillStyleLst");
            t.line_styles = child(fmt, "lnStyleLst");
            t.effect_styles = child(fmt, "effectStyleLst");
            t.bg_styles = child(fmt, "bgFillStyleLst");
        }
        t
    }
}

fn default_scheme() -> HashMap<String, Color> {
    [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "44546A"),
        ("lt2", "E7E6E6"),
        ("accent1", "4472C4"),
        ("accent2", "ED7D31"),
        ("accent3", "A5A5A5"),
        ("accent4", "FFC000"),
        ("accent5", "5B9BD5"),
        ("accent6", "70AD47"),
        ("hlink", "0563C1"),
        ("folHlink", "954F72"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), Color::hex(v).unwrap()))
    .collect()
}

/// 한 쪽을 그리는 동안의 둘레.
pub(crate) struct Scope<'s, 'a, 'i> {
    pub part: &'s str,
    pub rels: &'s HashMap<String, Rel>,
    pub palette: &'s Palette,
    pub theme: &'s Theme<'a, 'i>,
    /// 자리표시자를 찾을 레이아웃·마스터의 도형 나무
    pub layout_tree: Option<Node<'a, 'i>>,
    pub master_tree: Option<Node<'a, 'i>>,
    pub master_root: Option<Node<'a, 'i>>,
    pub default_text: Option<Node<'a, 'i>>,
    /// 마스터·레이아웃 층이면 자리표시자는 그리지 않는다
    pub skip_placeholders: bool,
    pub slide_no: usize,
}

pub(crate) struct Renderer<'d, 'i> {
    pub pkg: &'d Package,
    pub docs: &'d HashMap<String, Document<'i>>,
    pub fonts: FontBook,
    pub images: usize,
    pub missing_images: usize,
    pub charts: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Xfrm {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

pub(crate) fn read_xfrm(n: Option<Node<'_, '_>>) -> Option<Xfrm> {
    let n = n?;
    let off = child(n, "off");
    let ext = child(n, "ext");
    if off.is_none() && ext.is_none() {
        return None;
    }
    Some(Xfrm {
        x: off.and_then(|o| num(o, "x")).unwrap_or(0.0),
        y: off.and_then(|o| num(o, "y")).unwrap_or(0.0),
        w: ext.and_then(|o| num(o, "cx")).unwrap_or(0.0),
        h: ext.and_then(|o| num(o, "cy")).unwrap_or(0.0),
        rot: num(n, "rot").unwrap_or(0.0) / 60000.0,
        flip_h: matches!(n.attribute("flipH"), Some("1") | Some("true")),
        flip_v: matches!(n.attribute("flipV"), Some("1") | Some("true")),
    })
}

/// 도형 자리의 변환 (지역 좌표 0..w, 0..h → 부모 좌표).
fn shape_transform(x: &Xfrm, mirror: bool) -> String {
    let (px, py, w, h) = (x.x / EMU, x.y / EMU, x.w / EMU, x.h / EMU);
    let mut t = format!("translate({} {})", f(px), f(py));
    if x.rot != 0.0 {
        t.push_str(&format!(" rotate({} {} {})", f(x.rot), f(w / 2.0), f(h / 2.0)));
    }
    if mirror {
        if x.flip_h {
            t.push_str(&format!(" translate({} 0) scale(-1 1)", f(w)));
        }
        if x.flip_v {
            t.push_str(&format!(" translate(0 {}) scale(1 -1)", f(h)));
        }
    }
    t
}

/// 자리표시자 (종류, 번호)
pub(crate) fn ph_of(sp: Node<'_, '_>) -> Option<(String, Option<String>)> {
    let nv = sp.children().find(|c| c.is_element() && c.tag_name().name().starts_with("nv"))?;
    let ph = child(child(nv, "nvPr")?, "ph")?;
    Some((ph.attribute("type").unwrap_or("obj").to_owned(), ph.attribute("idx").map(|s| s.to_owned())))
}

fn master_kind(t: &str) -> &str {
    match t {
        "title" | "ctrTitle" => "title",
        "dt" | "ftr" | "sldNum" | "hdr" => t,
        _ => "body",
    }
}

fn same_kind(a: &str, b: &str) -> bool {
    a == b || master_kind(a) == master_kind(b) && !matches!(master_kind(a), "dt" | "ftr" | "sldNum" | "hdr")
}

fn find_ph<'a, 'i>(tree: Option<Node<'a, 'i>>, ty: &str, idx: Option<&str>, by_idx: bool) -> Option<Node<'a, 'i>> {
    let tree = tree?;
    let phs: Vec<(Node<'a, 'i>, String, Option<String>)> = tree
        .descendants()
        .filter(|n| matches!(n.tag_name().name(), "sp" | "pic" | "graphicFrame"))
        .filter_map(|n| ph_of(n).map(|(t, i)| (n, t, i)))
        .collect();
    if by_idx {
        if let Some(idx) = idx {
            if let Some(hit) = phs.iter().find(|(_, _, i)| i.as_deref() == Some(idx)) {
                return Some(hit.0);
            }
        }
    }
    phs.iter().find(|(_, t, _)| t == ty).or_else(|| phs.iter().find(|(_, t, _)| same_kind(t, ty))).map(|h| h.0)
}

/// 도형과 그 조상 자리표시자 사슬 (마스터, 레이아웃, 도형 순).
fn chain<'a, 'i>(scope: &Scope<'_, 'a, 'i>, sp: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    let mut out = Vec::new();
    if let Some((ty, idx)) = ph_of(sp) {
        let layout = find_ph(scope.layout_tree, &ty, idx.as_deref(), true);
        let master_ty = layout.and_then(ph_of).map(|p| p.0).unwrap_or(ty.clone());
        let master = find_ph(scope.master_tree, master_kind(&master_ty), None, false);
        if let Some(m) = master {
            if m != sp {
                out.push(m);
            }
        }
        if let Some(l) = layout {
            if l != sp {
                out.push(l);
            }
        }
    }
    out.push(sp);
    out
}

fn sp_pr<'a, 'i>(n: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && matches!(c.tag_name().name(), "spPr" | "grpSpPr"))
}

impl<'d, 'i> Renderer<'d, 'i> {
    pub fn doc(&self, path: &str) -> Option<&'d Document<'i>> {
        self.docs.get(path)
    }

    // -----------------------------------------------------------------------
    // 배경

    pub fn background(&mut self, svg: &mut Svg, bg: Node<'_, '_>, scope: &Scope<'_, '_, '_>, w: f64, h: f64) {
        let fill = if let Some(pr) = child(bg, "bgPr") {
            fill::read_fill(pr, scope.palette, None)
        } else if let Some(r) = child(bg, "bgRef") {
            let idx = num(r, "idx").unwrap_or(0.0) as usize;
            let color = fill::ref_color(r, scope.palette);
            let entry = if idx >= 1001 {
                fill::style_entry(scope.theme.bg_styles, idx - 1000)
            } else {
                fill::style_entry(scope.theme.fill_styles, idx)
            };
            entry.map(|e| fill::read_fill_node(e, scope.palette, color)).or(color.map(Fill::Solid))
        } else {
            None
        };
        let Some(fill) = fill else { return };
        let rect = Geometry::rect(w, h);
        self.draw_fill(svg, &fill, &rect, w, h, scope);
    }

    // -----------------------------------------------------------------------
    // 나무

    pub fn tree(&mut self, svg: &mut Svg, tree: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>, group_fill: Option<&Fill>)
    {
        for n in tree.children().filter(|c| c.is_element()) {
            self.node(svg, n, scope, group_fill);
        }
    }

    fn node(&mut self, svg: &mut Svg, n: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>, group_fill: Option<&Fill>)
    {
        // 숨긴 도형
        if let Some(nv) = n.children().find(|c| c.is_element() && c.tag_name().name().starts_with("nv")) {
            if let Some(c) = child(nv, "cNvPr") {
                if matches!(c.attribute("hidden"), Some("1") | Some("true")) {
                    return;
                }
            }
        }
        match n.tag_name().name() {
            "sp" | "cxnSp" => self.shape(svg, n, scope, group_fill),
            "pic" => self.picture(svg, n, scope),
            "grpSp" => self.group(svg, n, scope, group_fill),
            "graphicFrame" => self.frame(svg, n, scope),
            "AlternateContent" => {
                let pick = child(n, "Fallback").or_else(|| child(n, "Choice"));
                if let Some(p) = pick {
                    for c in p.children().filter(|c| c.is_element()) {
                        self.node(svg, c, scope, group_fill);
                    }
                }
            }
            _ => {}
        }
    }

    fn group(&mut self, svg: &mut Svg, n: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>, group_fill: Option<&Fill>)
    {
        let pr = child(n, "grpSpPr");
        let xf = pr.and_then(|p| child(p, "xfrm"));
        let x = read_xfrm(xf).unwrap_or_default();
        let (cox, coy, cw, ch) = xf
            .map(|xf| {
                let o = child(xf, "chOff");
                let e = child(xf, "chExt");
                (
                    o.and_then(|o| num(o, "x")).unwrap_or(x.x),
                    o.and_then(|o| num(o, "y")).unwrap_or(x.y),
                    e.and_then(|e| num(e, "cx")).unwrap_or(x.w),
                    e.and_then(|e| num(e, "cy")).unwrap_or(x.h),
                )
            })
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
        let sx = if cw > 0.0 { x.w / cw } else { 1.0 };
        let sy = if ch > 0.0 { x.h / ch } else { 1.0 };
        let own = pr.and_then(|p| fill::read_fill(p, scope.palette, None));
        let gf = match &own {
            Some(Fill::Group) | None => group_fill.cloned(),
            Some(other) => Some(other.clone()),
        };
        let t = format!(
            "{} scale({} {}) translate({} {})",
            shape_transform(&x, true),
            f(sx),
            f(sy),
            f(-cox / EMU),
            f(-coy / EMU)
        );
        svg.body.push_str(&format!("<g transform=\"{t}\">"));
        self.tree(svg, n, scope, gf.as_ref());
        svg.body.push_str("</g>");
    }

    // -----------------------------------------------------------------------
    // 도형

    pub fn shape(&mut self, svg: &mut Svg, sp: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>, group_fill: Option<&Fill>)
    {
        let is_ph = ph_of(sp).is_some();
        if is_ph && scope.skip_placeholders {
            return;
        }
        let tx_body = child(sp, "txBody");
        // 빈 자리표시자는 쇼에서 보이지 않는다.
        if is_ph && !scope.skip_placeholders && !tx_body.map(text::has_text).unwrap_or(false) {
            return;
        }
        let chain = chain(scope, sp);
        let xfrm = chain.iter().rev().find_map(|n| read_xfrm(sp_pr(*n).and_then(|p| child(p, "xfrm"))));
        let Some(xf) = xfrm else { return };
        let (w, h) = (xf.w / EMU, xf.h / EMU);

        // 모양
        let geometry = chain
            .iter()
            .rev()
            .find_map(|n| {
                let pr = sp_pr(*n)?;
                if let Some(pg) = child(pr, "prstGeom") {
                    let adj = geom::read_adjust(pg, xf.w, xf.h);
                    return geom::preset(pg.attribute("prst").unwrap_or("rect"), xf.w, xf.h, &adj);
                }
                child(pr, "custGeom").map(|cg| geom::custom(cg, xf.w, xf.h))
            })
            .unwrap_or_else(|| Geometry::rect(w, h));

        let style = chain.iter().rev().find_map(|n| child(*n, "style"));
        // 채우기
        let fill = chain
            .iter()
            .rev()
            .find_map(|n| sp_pr(*n).and_then(|p| fill::read_fill(p, scope.palette, None)))
            .or_else(|| {
                let r = child(style?, "fillRef")?;
                let idx = num(r, "idx").unwrap_or(0.0) as usize;
                let color = fill::ref_color(r, scope.palette);
                let entry = if idx >= 1001 {
                    fill::style_entry(scope.theme.bg_styles, idx - 1000)
                } else {
                    fill::style_entry(scope.theme.fill_styles, idx)
                };
                entry.map(|e| fill::read_fill_node(e, scope.palette, color))
            })
            .unwrap_or(Fill::None);
        let fill = match fill {
            Fill::Group => group_fill.cloned().unwrap_or(Fill::None),
            other => other,
        };
        // 선
        let mut line = Line::default();
        let mut has_line = false;
        if let Some(r) = style.and_then(|s| child(s, "lnRef")) {
            let idx = num(r, "idx").unwrap_or(0.0) as usize;
            let color = fill::ref_color(r, scope.palette);
            if let Some(entry) = fill::style_entry(scope.theme.line_styles, idx) {
                fill::apply_line(&mut line, entry, scope.palette, color);
                has_line = true;
            }
        }
        for n in &chain {
            if let Some(ln) = sp_pr(*n).and_then(|p| child(p, "ln")) {
                fill::apply_line(&mut line, ln, scope.palette, None);
                has_line = true;
            }
        }
        if !has_line {
            line.fill = Fill::None;
        }
        // 그림자
        let shadow = chain
            .iter()
            .rev()
            .find_map(|n| sp_pr(*n).and_then(|p| child(p, "effectLst")).map(|e| fill::read_shadow(e, scope.palette, None)))
            .unwrap_or_else(|| {
                let r = child(style?, "effectRef")?;
                let idx = num(r, "idx").unwrap_or(0.0) as usize;
                let color = fill::ref_color(r, scope.palette);
                let entry = fill::style_entry(scope.theme.effect_styles, idx)?;
                fill::read_shadow(child(entry, "effectLst")?, scope.palette, color)
            });

        svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&xf, true)));
        if let Some(s) = &shadow {
            self.draw_shadow(svg, &geometry, &fill, &line, s);
        }
        self.draw_fill(svg, &fill, &geometry, w, h, scope);
        self.draw_line(svg, &geometry, &line, w, h);
        svg.body.push_str("</g>");

        // 글
        if let Some(tx) = tx_body {
            if text::has_text(tx) {
                let (levels, body) = self.text_style(scope, &chain, style, false);
                let (l, t, r, b) = geometry.text_rect;
                let tw = if xf.flip_v { xf.rot + 180.0 } else { xf.rot };
                let tx_xf = Xfrm { rot: tw, flip_h: false, flip_v: false, ..xf };
                svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&tx_xf, false)));
                let mut env = TextEnv { fonts: &mut self.fonts, theme: &scope.theme.fonts, palette: scope.palette, slide_no: scope.slide_no };
                text::render(svg, &mut env, tx, &levels, &body, l, t, (r - l).max(1.0), (b - t).max(1.0));
                svg.body.push_str("</g>");
            }
        }
    }

    /// 글자 모양 사슬을 만든다.
    pub fn text_style(&self, scope: &Scope<'_, '_, '_>, chain: &[Node<'_, '_>], style: Option<Node<'_, '_>>, table_cell: bool) -> (Levels, Body) {
        let mut levels = Levels::default();
        let base = RunStyle {
            size: Some(18.0),
            color: Some(Some(scope.palette.scheme_color("tx1").unwrap_or(Color::BLACK))),
            latin: Some("+mn-lt".into()),
            ea: Some("+mn-ea".into()),
            ..Default::default()
        };
        levels.merge_run_all(&base);
        levels.merge_list(scope.default_text, scope.palette);
        let ph = chain.last().and_then(|n| ph_of(*n));
        if let (Some((ty, _)), Some(master)) = (&ph, scope.master_root) {
            let styles = child(master, "txStyles");
            let which = match master_kind(ty) {
                "title" => "titleStyle",
                "body" => "bodyStyle",
                _ => "otherStyle",
            };
            levels.merge_list(styles.and_then(|s| child(s, which)), scope.palette);
        } else if !table_cell {
            if let Some(master) = scope.master_root {
                let _ = master;
            }
        }
        if let Some(fr) = style.and_then(|s| child(s, "fontRef")) {
            let mut r = RunStyle::default();
            match fr.attribute("idx") {
                Some("major") => {
                    r.latin = Some("+mj-lt".into());
                    r.ea = Some("+mj-ea".into());
                }
                Some("minor") => {
                    r.latin = Some("+mn-lt".into());
                    r.ea = Some("+mn-ea".into());
                }
                _ => {}
            }
            if let Some(c) = fill::ref_color(fr, scope.palette) {
                r.color = Some(Some(c));
            }
            levels.merge_run_all(&r);
        }
        let mut body = Body::default();
        for n in chain {
            if let Some(tx) = child(*n, "txBody") {
                levels.merge_list(child(tx, "lstStyle"), scope.palette);
                body.merge(child(tx, "bodyPr"));
            }
        }
        (levels, body)
    }

    pub fn draw_fill(&mut self, svg: &mut Svg, fill: &Fill, g: &Geometry, w: f64, h: f64, scope: &Scope<'_, '_, '_>) {
        match fill {
            Fill::None | Fill::Group => {}
            Fill::Blip(b) => {
                // 모양으로 자르고 그 안에 그림을 늘인다.
                let id = svg.id("c");
                let d: String = g.paths.iter().filter(|p| p.fill != FillMode::None).map(|p| Geometry::svg_d(&p.cmds)).collect();
                svg.defs.push_str(&format!("<clipPath id=\"{id}\"><path d=\"{d}\"/></clipPath>"));
                let (sl, st, sr, sb) = b.stretch;
                let (bx, by, bw, bh) = (w * sl, h * st, w * (1.0 - sl - sr), h * (1.0 - st - sb));
                svg.body.push_str(&format!("<g clip-path=\"url(#{id})\">"));
                self.image(svg, b, scope, bx, by, bw, bh);
                svg.body.push_str("</g>");
            }
            _ => {
                for p in &g.paths {
                    if p.fill == FillMode::None {
                        continue;
                    }
                    let shaded;
                    let use_fill = match (p.fill, fill) {
                        (FillMode::Norm, _) => fill,
                        (mode, Fill::Solid(c)) => {
                            let t = match mode {
                                FillMode::Lighten => (Color::WHITE, 0.4),
                                FillMode::LightenLess => (Color::WHITE, 0.2),
                                FillMode::Darken => (Color::BLACK, 0.4),
                                _ => (Color::BLACK, 0.2),
                            };
                            let mut m = c.mix(t.0.with_alpha(c.a), t.1);
                            m.a = c.a;
                            shaded = Fill::Solid(m);
                            &shaded
                        }
                        _ => fill,
                    };
                    if let Some((paint, alpha)) = fill::paint(svg, use_fill, 0.0, 0.0, w, h) {
                        if alpha <= 0.0 {
                            continue;
                        }
                        svg.body.push_str(&format!(
                            "<path d=\"{}\" fill=\"{paint}\"{}/>",
                            Geometry::svg_d(&p.cmds),
                            if alpha < 1.0 { format!(" fill-opacity=\"{}\"", f(alpha as f64)) } else { String::new() }
                        ));
                    }
                }
            }
        }
    }

    fn draw_line(&mut self, svg: &mut Svg, g: &Geometry, line: &Line, w: f64, h: f64) {
        let Some(attrs) = fill::stroke_attrs(svg, line, 0.0, 0.0, w, h) else { return };
        let color = line.fill.first_color().unwrap_or(Color::BLACK);
        for p in &g.paths {
            if !p.stroke {
                continue;
            }
            svg.body.push_str(&format!("<path d=\"{}\" fill=\"none\"{attrs}/>", Geometry::svg_d(&p.cmds)));
        }
        // 화살표
        if line.head.is_some() || line.tail.is_some() {
            if let Some(p) = g.paths.iter().find(|p| p.stroke) {
                let pts = endpoints(&p.cmds);
                if let Some(((s0, s1), (e0, e1))) = pts {
                    if let Some(a) = line.head {
                        arrow_head(svg, a, s0, s1, line.width, color);
                    }
                    if let Some(a) = line.tail {
                        arrow_head(svg, a, e1, e0, line.width, color);
                    }
                }
            }
        }
    }

    fn draw_shadow(&mut self, svg: &mut Svg, g: &Geometry, fill: &Fill, line: &Line, s: &Shadow) {
        let filled = !matches!(fill, Fill::None);
        let stroked = !matches!(line.fill, Fill::None);
        if !filled && !stroked {
            return;
        }
        // 흐림 필터는 PDF 에서 그림으로 굽혀 느리다(쪽마다 수백 ms).
        // 같은 모양을 조금씩 넓혀 옅게 여러 번 겹쳐 부드러운 가장자리를 흉내 낸다.
        let d: String = g.paths.iter().map(|p| Geometry::svg_d(&p.cmds)).collect();
        let c = s.color.css();
        let a = s.color.a as f64;
        let mut body = String::new();
        if s.blur > 0.5 {
            let steps = 4;
            for k in 0..steps {
                let t = (steps - k) as f64 / steps as f64; // 1 → 바깥
                let w = s.blur * t;
                let op = a / (steps as f64 + 1.0);
                if filled {
                    body.push_str(&format!(
                        "<path d=\"{d}\" fill=\"{c}\" fill-opacity=\"{}\" stroke=\"{c}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
                        f(op), f(op), f(w)
                    ));
                } else {
                    body.push_str(&format!(
                        "<path d=\"{d}\" fill=\"none\" stroke=\"{c}\" stroke-opacity=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
                        f(op), f(line.width + w)
                    ));
                }
            }
        } else if filled {
            body = format!("<path d=\"{d}\" fill=\"{c}\" fill-opacity=\"{}\"/>", f(a));
        } else {
            body = format!("<path d=\"{d}\" fill=\"none\" stroke=\"{c}\" stroke-opacity=\"{}\" stroke-width=\"{}\"/>", f(a), f(line.width));
        }
        svg.body.push_str(&format!("<g transform=\"translate({} {})\">{body}</g>", f(s.dx), f(s.dy)));
    }

    // -----------------------------------------------------------------------
    // 그림

    pub fn picture(&mut self, svg: &mut Svg, pic: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>)
    {
        if ph_of(pic).is_some() && scope.skip_placeholders {
            return;
        }
        let chain = chain(scope, pic);
        let Some(xf) = chain.iter().rev().find_map(|n| read_xfrm(sp_pr(*n).and_then(|p| child(p, "xfrm")))) else { return };
        let Some(bf) = child(pic, "blipFill") else { return };
        let blip = fill::read_blip(bf);
        let (w, h) = (xf.w / EMU, xf.h / EMU);
        let geometry = sp_pr(pic)
            .and_then(|pr| {
                if let Some(pg) = child(pr, "prstGeom") {
                    let adj = geom::read_adjust(pg, xf.w, xf.h);
                    return geom::preset(pg.attribute("prst").unwrap_or("rect"), xf.w, xf.h, &adj);
                }
                child(pr, "custGeom").map(|cg| geom::custom(cg, xf.w, xf.h))
            })
            .unwrap_or_else(|| Geometry::rect(w, h));
        let mut line = Line::default();
        let mut has_line = false;
        if let Some(r) = child(pic, "style").and_then(|s| child(s, "lnRef")) {
            let idx = num(r, "idx").unwrap_or(0.0) as usize;
            let color = fill::ref_color(r, scope.palette);
            if let Some(entry) = fill::style_entry(scope.theme.line_styles, idx) {
                fill::apply_line(&mut line, entry, scope.palette, color);
                has_line = true;
            }
        }
        if let Some(ln) = sp_pr(pic).and_then(|p| child(p, "ln")) {
            fill::apply_line(&mut line, ln, scope.palette, None);
            has_line = true;
        }
        if !has_line {
            line.fill = Fill::None;
        }
        let shadow = sp_pr(pic).and_then(|p| child(p, "effectLst")).and_then(|e| fill::read_shadow(e, scope.palette, None));
        svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&xf, true)));
        if let Some(s) = &shadow {
            self.draw_shadow(svg, &geometry, &Fill::Solid(Color::BLACK), &line, s);
        }
        let is_rect = sp_pr(pic).and_then(|p| child(p, "prstGeom")).map(|g| g.attribute("prst") == Some("rect")).unwrap_or(true);
        let (cl, ct, cr, cb) = blip.crop;
        let needs_clip = !is_rect || cl > 0.0 || ct > 0.0 || cr > 0.0 || cb > 0.0;
        let fw = (1.0 - cl - cr).max(0.001);
        let fh = (1.0 - ct - cb).max(0.001);
        let (iw, ih) = (w / fw, h / fh);
        let (ix, iy) = (-cl * iw, -ct * ih);
        if needs_clip {
            let id = svg.id("c");
            let d: String = geometry.paths.iter().map(|p| Geometry::svg_d(&p.cmds)).collect();
            svg.defs.push_str(&format!("<clipPath id=\"{id}\"><path d=\"{d}\"/></clipPath>"));
            svg.body.push_str(&format!("<g clip-path=\"url(#{id})\">"));
            self.image(svg, &blip, scope, ix, iy, iw, ih);
            svg.body.push_str("</g>");
        } else {
            self.image(svg, &blip, scope, ix, iy, iw, ih);
        }
        self.draw_line(svg, &geometry, &line, w, h);
        svg.body.push_str("</g>");
    }

    /// 그림 바이트를 SVG image 로.
    pub fn image(&mut self, svg: &mut Svg, b: &Blip, scope: &Scope<'_, '_, '_>, x: f64, y: f64, w: f64, h: f64) {
        let pick = |rid: &str| scope.rels.get(rid).filter(|r| !r.external).map(|r| r.target.clone());
        let target = b.svg_rid.as_deref().and_then(pick).or_else(|| pick(&b.rid));
        let Some(target) = target else {
            self.missing_images += 1;
            return;
        };
        let Some(bytes) = self.pkg.bytes(&target) else {
            self.missing_images += 1;
            return;
        };
        let Some((mime, data)) = image_data(bytes) else {
            self.missing_images += 1;
            return;
        };
        self.images += 1;
        let mut attrs = String::new();
        if b.alpha < 0.999 {
            attrs.push_str(&format!(" opacity=\"{}\"", f(b.alpha as f64)));
        }
        if b.gray {
            let id = svg.id("m");
            svg.defs.push_str(&format!("<filter id=\"{id}\"><feColorMatrix type=\"saturate\" values=\"0\"/></filter>"));
            attrs.push_str(&format!(" filter=\"url(#{id})\""));
        }
        if b.tile {
            // 바둑판 그림은 늘여서 채운다(타일 크기를 알기 어렵다).
        }
        svg.body.push_str(&format!(
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"{attrs} xlink:href=\"data:{mime};base64,{}\"/>",
            f(x),
            f(y),
            f(w.max(0.01)),
            f(h.max(0.01)),
            base64(&data)
        ));
    }

    // -----------------------------------------------------------------------
    // 틀 (표, 차트, SmartArt, OLE)

    fn frame(&mut self, svg: &mut Svg, n: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>)
    {
        if ph_of(n).is_some() && scope.skip_placeholders {
            return;
        }
        let chain = chain(scope, n);
        let xf = chain
            .iter()
            .rev()
            .find_map(|c| read_xfrm(child(*c, "xfrm")).or_else(|| read_xfrm(sp_pr(*c).and_then(|p| child(p, "xfrm")))));
        let Some(xf) = xf else { return };
        let Some(data) = path_child(n, &["graphic", "graphicData"]) else { return };
        let uri = data.attribute("uri").unwrap_or("");
        let (w, h) = (xf.w / EMU, xf.h / EMU);
        if let Some(tbl) = child(data, "tbl") {
            svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&xf, false)));
            table::render(self, svg, tbl, scope, w, h);
            svg.body.push_str("</g>");
        } else if uri.ends_with("/chart") {
            let rid = child(data, "chart").and_then(|c| c.attributes().find(|a| a.name() == "id").map(|a| a.value().to_owned()));
            if let Some(target) = rid.and_then(|r| scope.rels.get(&r).map(|r| r.target.clone())) {
                if let Some(doc) = self.doc(&target) {
                    self.charts += 1;
                    svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&xf, false)));
                    chart::render(self, svg, doc, &target, scope, w, h);
                    svg.body.push_str("</g>");
                }
            }
        } else if uri.ends_with("/diagram") {
            self.diagram(svg, data, scope, &xf);
        } else {
            // OLE 등: 안에 든 대체 그림
            for pic in data.descendants().filter(|d| d.has_tag_name("pic")) {
                let mut inner = pic;
                let _ = &mut inner;
                // 그림 자리가 틀과 같다고 보고 그린다.
                if read_xfrm(sp_pr(pic).and_then(|p| child(p, "xfrm"))).is_some() {
                    self.picture(svg, pic, scope);
                } else if let Some(bf) = child(pic, "blipFill") {
                    let blip = fill::read_blip(bf);
                    svg.body.push_str(&format!("<g transform=\"{}\">", shape_transform(&xf, false)));
                    self.image(svg, &blip, scope, 0.0, 0.0, w, h);
                    svg.body.push_str("</g>");
                }
                break;
            }
        }
    }

    fn diagram(&mut self, svg: &mut Svg, data: Node<'_, '_>, scope: &Scope<'_, 'd, 'i>, xf: &Xfrm)
    {
        let Some(ids) = child(data, "relIds") else { return };
        let dm = ids.attributes().find(|a| a.name() == "dm").map(|a| a.value().to_owned());
        // 데이터 부분이 가리키는 그림 부분(drawing)을 찾는다.
        let mut drawing: Option<String> = None;
        if let Some(dm_target) = dm.and_then(|r| scope.rels.get(&r).map(|r| r.target.clone())) {
            if let Some(doc) = self.doc(&dm_target) {
                if let Some(ext) = doc.descendants().find(|d| d.tag_name().name() == "dataModelExt") {
                    if let Some(rid) = ext.attribute("relId") {
                        drawing = scope.rels.get(rid).map(|r| r.target.clone());
                    }
                }
            }
        }
        if drawing.is_none() {
            let mut list: Vec<&Rel> = scope.rels.values().filter(|r| r.kind == "diagramDrawing").collect();
            list.sort_by(|a, b| a.target.cmp(&b.target));
            if list.len() == 1 {
                drawing = Some(list[0].target.clone());
            }
        }
        let Some(drawing) = drawing else { return };
        let Some(doc) = self.doc(&drawing) else { return };
        let Some(tree) = doc.descendants().find(|d| d.tag_name().name() == "spTree") else { return };
        let rels = self.pkg.rels(&drawing);
        let inner = Scope {
            part: &drawing,
            rels: &rels,
            palette: scope.palette,
            theme: scope.theme,
            layout_tree: None,
            master_tree: None,
            master_root: scope.master_root,
            default_text: scope.default_text,
            skip_placeholders: false,
            slide_no: scope.slide_no,
        };
        svg.body.push_str(&format!("<g transform=\"translate({} {})\">", f(xf.x / EMU), f(xf.y / EMU)));
        self.tree(svg, tree, &inner, None);
        svg.body.push_str("</g>");
    }
}

fn endpoints(cmds: &[Cmd]) -> Option<(((f64, f64), (f64, f64)), ((f64, f64), (f64, f64)))> {
    let mut pts: Vec<((f64, f64), (f64, f64))> = Vec::new(); // (끝점, 그 앞 방향점)
    let mut cur = (0.0, 0.0);
    let mut start: Option<((f64, f64), (f64, f64))> = None;
    for c in cmds {
        match *c {
            Cmd::M(x, y) => cur = (x, y),
            Cmd::L(x, y) => {
                if start.is_none() {
                    start = Some((cur, (x, y)));
                }
                pts.push(((x, y), cur));
                cur = (x, y);
            }
            Cmd::C(a, b, c2, d, x, y) => {
                if start.is_none() {
                    start = Some((cur, if (a, b) != cur { (a, b) } else { (c2, d) }));
                }
                pts.push(((x, y), if (c2, d) != (x, y) { (c2, d) } else { (a, b) }));
                cur = (x, y);
            }
            Cmd::Z => {}
        }
    }
    let s = start?;
    let e = *pts.last()?;
    Some(((s.0, s.1), (e.1, e.0)))
}

/// 화살표 머리. tip 이 끝점, from 은 선이 오는 쪽.
fn arrow_head(svg: &mut Svg, a: fill::Arrow, tip: (f64, f64), from: (f64, f64), lw: f64, color: Color) {
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let lw = lw.max(0.75);
    let l = lw * a.len;
    let w = lw * a.w / 2.0;
    let base = (tip.0 - ux * l, tip.1 - uy * l);
    let p = |x: f64, y: f64| format!("{} {}", f(x), f(y));
    let d = match a.kind {
        "triangle" => format!(
            "M{}L{}L{}Z",
            p(tip.0, tip.1),
            p(base.0 + px * w, base.1 + py * w),
            p(base.0 - px * w, base.1 - py * w)
        ),
        "stealth" => {
            let mid = (tip.0 - ux * l * 0.6, tip.1 - uy * l * 0.6);
            format!(
                "M{}L{}L{}L{}Z",
                p(tip.0, tip.1),
                p(base.0 + px * w, base.1 + py * w),
                p(mid.0, mid.1),
                p(base.0 - px * w, base.1 - py * w)
            )
        }
        "diamond" => {
            let c = (tip.0 - ux * l / 2.0, tip.1 - uy * l / 2.0);
            format!(
                "M{}L{}L{}L{}Z",
                p(tip.0 + ux * l / 2.0 - ux * l / 2.0, tip.1 + uy * l / 2.0 - uy * l / 2.0),
                p(c.0 + px * w, c.1 + py * w),
                p(base.0, base.1),
                p(c.0 - px * w, c.1 - py * w)
            )
        }
        "oval" => {
            let c = (tip.0 - ux * l / 2.0, tip.1 - uy * l / 2.0);
            svg.body.push_str(&format!(
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\"/>",
                f(c.0),
                f(c.1),
                f(l / 2.0),
                f(w),
                color.css()
            ));
            return;
        }
        _ => {
            // 열린 화살표
            svg.body.push_str(&format!(
                "<path d=\"M{}L{}L{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>",
                p(base.0 + px * w, base.1 + py * w),
                p(tip.0, tip.1),
                p(base.0 - px * w, base.1 - py * w),
                color.css(),
                f(lw)
            ));
            return;
        }
    };
    svg.body.push_str(&format!("<path d=\"{d}\" fill=\"{}\"{}/>", color.css(), if color.a < 1.0 { format!(" fill-opacity=\"{}\"", f(color.a as f64)) } else { String::new() }));
}

/// 그림 바이트 → (MIME, 바이트). EMF·WMF 는 SVG 로 옮긴다.
pub(crate) fn image_data(bytes: &[u8]) -> Option<(&'static str, Vec<u8>)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some(("image/png", bytes.to_vec()));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return Some(("image/jpeg", bytes.to_vec()));
    }
    if bytes.starts_with(b"GIF8") {
        return Some(("image/gif", bytes.to_vec()));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(("image/webp", bytes.to_vec()));
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
    if head.contains("<svg") {
        return Some(("image/svg+xml", bytes.to_vec()));
    }
    if bytes.len() > 44 && bytes.starts_with(&[1, 0, 0, 0]) && &bytes[40..44] == b" EMF" {
        return super::metafile::emf(bytes).map(|s| ("image/svg+xml", s));
    }
    if bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) || bytes.starts_with(&[1, 0, 9, 0]) || bytes.starts_with(&[2, 0, 9, 0]) {
        return super::metafile::wmf(bytes).map(|s| ("image/svg+xml", s));
    }
    super::metafile::raster(bytes).map(|p| ("image/png", p))
}

pub(crate) fn part_dir(part: &str) -> &str {
    dir_of(part)
}

#[allow(dead_code)]
pub(crate) fn esc_attr(s: &str) -> String {
    esc(s)
}
