//! 채우기·선·그림자 읽기, 그리고 SVG 로 옮기기.

use roxmltree::Node;

use super::color::{color_child, read_color, Color, Palette};
use super::geom::f;
use super::svg::Svg;

#[derive(Clone, Debug)]
pub(crate) enum GradKind {
    /// 각(도, 시계 방향), 크기에 맞춰 늘릴지
    Linear(f64, bool),
    /// 원·사각형 모양, 초점 사각형 (l, t, r, b) 비율
    Path(String, (f64, f64, f64, f64)),
}

#[derive(Clone, Debug)]
pub(crate) struct Gradient {
    pub stops: Vec<(f64, Color)>,
    pub kind: GradKind,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Blip {
    pub rid: String,
    /// 자르기 l, t, r, b (비율, 음수는 여백)
    pub crop: (f64, f64, f64, f64),
    /// 늘이기 사각형 l, t, r, b (비율)
    pub stretch: (f64, f64, f64, f64),
    pub tile: bool,
    pub alpha: f32,
    pub gray: bool,
    /// SVG 판이 따로 있으면 그 관계 id
    pub svg_rid: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum Fill {
    None,
    Solid(Color),
    Grad(Gradient),
    Blip(Blip),
    Pattern(String, Color, Color),
    Group,
}

impl Fill {
    pub fn first_color(&self) -> Option<Color> {
        match self {
            Fill::Solid(c) => Some(*c),
            Fill::Grad(g) => g.stops.first().map(|s| s.1),
            Fill::Pattern(_, fg, _) => Some(*fg),
            _ => None,
        }
    }
}

const FILL_TAGS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

pub(crate) fn has_fill(parent: Node<'_, '_>) -> bool {
    parent.children().any(|c| c.is_element() && FILL_TAGS.contains(&c.tag_name().name()))
}

fn ratio(n: Node<'_, '_>, a: &str) -> f64 {
    n.attribute(a).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 100000.0
}

/// 부모 요소 안의 채우기 하나.
pub(crate) fn read_fill(parent: Node<'_, '_>, palette: &Palette, ph: Option<Color>) -> Option<Fill> {
    let node = parent.children().find(|c| c.is_element() && FILL_TAGS.contains(&c.tag_name().name()))?;
    Some(read_fill_node(node, palette, ph))
}

pub(crate) fn read_fill_node(node: Node<'_, '_>, palette: &Palette, ph: Option<Color>) -> Fill {
    match node.tag_name().name() {
        "noFill" => Fill::None,
        "solidFill" => color_child(node, palette, ph).map(Fill::Solid).unwrap_or(Fill::None),
        "gradFill" => {
            let mut stops: Vec<(f64, Color)> = node
                .children()
                .find(|c| c.has_tag_name("gsLst"))
                .map(|l| {
                    l.children()
                        .filter(|c| c.has_tag_name("gs"))
                        .filter_map(|gs| Some((ratio(gs, "pos"), color_child(gs, palette, ph)?)))
                        .collect()
                })
                .unwrap_or_default();
            stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            if stops.is_empty() {
                return Fill::None;
            }
            let kind = if let Some(path) = node.children().find(|c| c.has_tag_name("path")) {
                let rect = path
                    .children()
                    .find(|c| c.has_tag_name("fillToRect"))
                    .map(|r| (ratio(r, "l"), ratio(r, "t"), ratio(r, "r"), ratio(r, "b")))
                    .unwrap_or((0.5, 0.5, 0.5, 0.5));
                GradKind::Path(path.attribute("path").unwrap_or("circle").to_owned(), rect)
            } else {
                let lin = node.children().find(|c| c.has_tag_name("lin"));
                let ang = lin.and_then(|l| l.attribute("ang")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 60000.0;
                let scaled = lin.and_then(|l| l.attribute("scaled")).map(|v| v == "1" || v == "true").unwrap_or(false);
                GradKind::Linear(ang, scaled)
            };
            Fill::Grad(Gradient { stops, kind })
        }
        "blipFill" => Fill::Blip(read_blip(node)),
        "pattFill" => {
            let fg = node
                .children()
                .find(|c| c.has_tag_name("fgClr"))
                .and_then(|c| color_child(c, palette, ph))
                .unwrap_or(Color::BLACK);
            let bg = node
                .children()
                .find(|c| c.has_tag_name("bgClr"))
                .and_then(|c| color_child(c, palette, ph))
                .unwrap_or(Color::WHITE);
            Fill::Pattern(node.attribute("prst").unwrap_or("pct50").to_owned(), fg, bg)
        }
        "grpFill" => Fill::Group,
        _ => Fill::None,
    }
}

pub(crate) fn read_blip(node: Node<'_, '_>) -> Blip {
    let mut b = Blip { alpha: 1.0, stretch: (0.0, 0.0, 0.0, 0.0), ..Default::default() };
    if let Some(blip) = node.children().find(|c| c.has_tag_name("blip")) {
        b.rid = blip
            .attributes()
            .find(|a| a.name() == "embed")
            .or_else(|| blip.attributes().find(|a| a.name() == "link"))
            .map(|a| a.value().to_owned())
            .unwrap_or_default();
        for e in blip.descendants() {
            match e.tag_name().name() {
                "alphaModFix" => b.alpha = ratio(e, "amt") as f32,
                "grayscl" => b.gray = true,
                "svgBlip" => {
                    b.svg_rid = e.attributes().find(|a| a.name() == "embed").map(|a| a.value().to_owned());
                }
                _ => {}
            }
        }
        if blip.children().any(|c| c.has_tag_name("alphaModFix")) && b.alpha == 0.0 {
            b.alpha = 1.0;
        }
    }
    if let Some(src) = node.children().find(|c| c.has_tag_name("srcRect")) {
        b.crop = (ratio(src, "l"), ratio(src, "t"), ratio(src, "r"), ratio(src, "b"));
    }
    if let Some(stretch) = node.children().find(|c| c.has_tag_name("stretch")) {
        if let Some(r) = stretch.children().find(|c| c.has_tag_name("fillRect")) {
            b.stretch = (ratio(r, "l"), ratio(r, "t"), ratio(r, "r"), ratio(r, "b"));
        }
    }
    b.tile = node.children().any(|c| c.has_tag_name("tile"));
    b
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Arrow {
    pub kind: &'static str,
    pub w: f64,
    pub len: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub width: f64,
    pub fill: Fill,
    pub dash: Vec<f64>,
    pub cap: &'static str,
    pub join: &'static str,
    pub head: Option<Arrow>,
    pub tail: Option<Arrow>,
}

impl Default for Line {
    fn default() -> Self {
        Line { width: 0.75, fill: Fill::None, dash: Vec::new(), cap: "butt", join: "round", head: None, tail: None }
    }
}

fn arrow(node: Node<'_, '_>) -> Option<Arrow> {
    let kind = match node.attribute("type")? {
        "triangle" => "triangle",
        "stealth" => "stealth",
        "diamond" => "diamond",
        "oval" => "oval",
        "arrow" => "arrow",
        _ => return None,
    };
    let size = |a: &str| match node.attribute(a) {
        Some("sm") => 2.0,
        Some("lg") => 5.0,
        _ => 3.0,
    };
    Some(Arrow { kind, w: size("w"), len: size("len") })
}

/// 선을 읽어 `base` 위에 덮는다 (스타일 선 위에 도형의 선).
pub(crate) fn apply_line(base: &mut Line, ln: Node<'_, '_>, palette: &Palette, ph: Option<Color>) {
    if let Some(w) = ln.attribute("w").and_then(|v| v.parse::<f64>().ok()) {
        base.width = w / 12700.0;
    }
    if let Some(cap) = ln.attribute("cap") {
        base.cap = match cap {
            "rnd" => "round",
            "sq" => "square",
            _ => "butt",
        };
    }
    if let Some(fill) = read_fill(ln, palette, ph) {
        base.fill = fill;
    }
    for c in ln.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "prstDash" => {
                base.dash = match c.attribute("val").unwrap_or("solid") {
                    "dot" | "sysDot" => vec![1.0, 1.0],
                    "dash" => vec![4.0, 3.0],
                    "sysDash" => vec![3.0, 1.0],
                    "lgDash" => vec![8.0, 3.0],
                    "dashDot" => vec![4.0, 3.0, 1.0, 3.0],
                    "sysDashDot" => vec![3.0, 1.0, 1.0, 1.0],
                    "lgDashDot" => vec![8.0, 3.0, 1.0, 3.0],
                    "lgDashDotDot" => vec![8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
                    "sysDashDotDot" => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                    _ => Vec::new(),
                }
            }
            "custDash" => {
                base.dash = c
                    .children()
                    .filter(|d| d.has_tag_name("ds"))
                    .flat_map(|d| [ratio(d, "d"), ratio(d, "sp")])
                    .collect();
            }
            "round" => base.join = "round",
            "bevel" => base.join = "bevel",
            "miter" => base.join = "miter",
            "headEnd" => base.head = arrow(c),
            "tailEnd" => base.tail = arrow(c),
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Shadow {
    pub blur: f64,
    pub dx: f64,
    pub dy: f64,
    pub color: Color,
}

pub(crate) fn read_shadow(effect_lst: Node<'_, '_>, palette: &Palette, ph: Option<Color>) -> Option<Shadow> {
    let s = effect_lst.children().find(|c| c.has_tag_name("outerShdw"))?;
    let num = |a: &str| s.attribute(a).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let dist = num("dist") / 12700.0;
    let dir = num("dir") / 60000.0 * std::f64::consts::PI / 180.0;
    let color = color_child(s, palette, ph).unwrap_or(Color::BLACK.with_alpha(0.4));
    if color.a <= 0.01 {
        return None;
    }
    Some(Shadow { blur: num("blurRad") / 12700.0, dx: dist * dir.cos(), dy: dist * dir.sin(), color })
}

/// 테마 스타일 목록의 idx 번째(1부터) 요소.
pub(crate) fn style_entry<'a, 'i>(list: Option<Node<'a, 'i>>, idx: usize) -> Option<Node<'a, 'i>> {
    if idx == 0 {
        return None;
    }
    list?.children().filter(|c| c.is_element()).nth(idx - 1)
}

/// 스타일 참조(fillRef 등)의 색.
pub(crate) fn ref_color(r: Node<'_, '_>, palette: &Palette) -> Option<Color> {
    r.children().filter(|c| c.is_element()).find_map(|c| read_color(c, palette, None))
}

// ---------------------------------------------------------------------------
// SVG 로

/// 채우기를 SVG 칠(fill 값과 투명도)로. (w, h) 는 지역 좌표의 상자 크기.
/// 그림 채우기는 따로 그려야 하므로 None.
pub(crate) fn paint(svg: &mut Svg, fill: &Fill, x: f64, y: f64, w: f64, h: f64) -> Option<(String, f32)> {
    match fill {
        Fill::None | Fill::Group | Fill::Blip(_) => None,
        Fill::Solid(c) => Some((c.css(), c.a)),
        Fill::Pattern(prst, fg, bg) => Some(pattern(svg, prst, *fg, *bg)),
        Fill::Grad(g) => Some((gradient(svg, g, x, y, w, h), 1.0)),
    }
}

pub(crate) fn gradient(svg: &mut Svg, g: &Gradient, x: f64, y: f64, w: f64, h: f64) -> String {
    let id = svg.id("g");
    let mut stops = String::new();
    let push = |stops: &mut String, pos: f64, c: &Color| {
        stops.push_str(&format!(
            "<stop offset=\"{}\" stop-color=\"{}\"{}/>",
            f(pos.clamp(0.0, 1.0)),
            c.css(),
            if c.a < 1.0 { format!(" stop-opacity=\"{}\"", f(c.a as f64)) } else { String::new() }
        ));
    };
    match &g.kind {
        GradKind::Linear(ang, scaled) => {
            let _ = scaled;
            let a = ang.to_radians();
            let (dx, dy) = (a.cos(), a.sin());
            // 상자 네 모서리를 방향에 비춰 시작·끝을 잡는다.
            let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
            let proj: Vec<f64> = corners.iter().map(|(px, py)| px * dx + py * dy).collect();
            let (mn, mx) = proj.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
            let (cx, cy) = (x + w / 2.0, y + h / 2.0);
            let c0 = cx * dx + cy * dy;
            let (x1, y1) = (cx + (mn - c0) * dx, cy + (mn - c0) * dy);
            let (x2, y2) = (cx + (mx - c0) * dx, cy + (mx - c0) * dy);
            for (pos, c) in &g.stops {
                push(&mut stops, *pos, c);
            }
            svg.defs.push_str(&format!(
                "<linearGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient>",
                f(x1), f(y1), f(x2), f(y2)
            ));
        }
        GradKind::Path(kind, (l, t, r, b)) => {
            // 초점 사각형의 가운데에서 바깥으로.
            let fx = x + w * (l + (1.0 - l - r).max(0.0) / 2.0);
            let fy = y + h * (t + (1.0 - t - b).max(0.0) / 2.0);
            let far = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
                .iter()
                .map(|(px, py)| ((px - fx).powi(2) + (py - fy).powi(2)).sqrt())
                .fold(0.0f64, f64::max);
            let radius = if kind == "circle" { far } else { far * 0.85 };
            // pos 0 이 가운데(초점), 100000 이 바깥이다.
            for (pos, c) in &g.stops {
                push(&mut stops, *pos, c);
            }
            svg.defs.push_str(&format!(
                "<radialGradient id=\"{id}\" gradientUnits=\"userSpaceOnUse\" cx=\"{}\" cy=\"{}\" r=\"{}\">{stops}</radialGradient>",
                f(fx), f(fy), f(radius.max(0.1))
            ));
        }
    }
    format!("url(#{id})")
}

fn pattern(svg: &mut Svg, prst: &str, fg: Color, bg: Color) -> (String, f32) {
    // 점 비율 무늬는 두 색을 그 비율로 섞어 칠한다(화면에서는 그렇게 보인다).
    if let Some(p) = prst.strip_prefix("pct").and_then(|v| v.parse::<f32>().ok()) {
        let c = bg.mix(fg, p / 100.0);
        return (c.css(), c.a);
    }
    let id = svg.id("p");
    let s = 6.0;
    let line = |d: &str| format!("<path d=\"{d}\" stroke=\"{}\" stroke-width=\"1\" fill=\"none\"/>", fg.css());
    let body = match prst {
        p if p.contains("horz") || p == "ltHorz" || p == "dkHorz" || p == "narHorz" || p == "wdHorz" => line("M0 3H6"),
        p if p.contains("vert") || p.contains("Vert") => line("M3 0V6"),
        p if p.contains("dnDiag") || p.contains("DnDiag") => line("M0 0L6 6"),
        p if p.contains("upDiag") || p.contains("UpDiag") => line("M0 6L6 0"),
        p if p.contains("Cross") || p.contains("cross") || p.contains("Grid") || p.contains("grid") => line("M0 3H6M3 0V6"),
        p if p.contains("diagCross") => line("M0 0L6 6M0 6L6 0"),
        _ => {
            let c = bg.mix(fg, 0.5);
            return (c.css(), c.a);
        }
    };
    svg.defs.push_str(&format!(
        "<pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" width=\"{s}\" height=\"{s}\"><rect width=\"{s}\" height=\"{s}\" fill=\"{}\"/>{body}</pattern>",
        bg.css()
    ));
    (format!("url(#{id})"), 1.0)
}

/// 선 속성 문자열 (stroke, 굵기, 점선 …). 그릴 선이 없으면 None.
pub(crate) fn stroke_attrs(svg: &mut Svg, line: &Line, x: f64, y: f64, w: f64, h: f64) -> Option<String> {
    let (paint, alpha) = paint(svg, &line.fill, x, y, w, h)?;
    if alpha <= 0.0 {
        return None;
    }
    let width = line.width.max(0.25);
    let mut s = format!(" stroke=\"{paint}\" stroke-width=\"{}\"", f(width));
    if alpha < 1.0 {
        s.push_str(&format!(" stroke-opacity=\"{}\"", f(alpha as f64)));
    }
    if line.cap != "butt" {
        s.push_str(&format!(" stroke-linecap=\"{}\"", line.cap));
    }
    s.push_str(&format!(" stroke-linejoin=\"{}\"", line.join));
    if !line.dash.is_empty() {
        let scale = if line.cap == "butt" { width } else { width };
        let list: Vec<String> = line.dash.iter().map(|d| f((d * scale).max(0.1))).collect();
        s.push_str(&format!(" stroke-dasharray=\"{}\"", list.join(" ")));
    }
    Some(s)
}
