//! 도형 모양: 표준 도형 정의(ECMA-376 presetShapeDefinitions)와 자유형(custGeom).
//!
//! 정의서의 공식(guide)을 그대로 계산해 경로로 만든다. 단위는 EMU 로 계산하고
//! 결과는 pt 로 돌려준다.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use roxmltree::{Document, Node};

const PRESETS: &str = include_str!("presets.xml");

fn presets() -> Option<&'static Document<'static>> {
    static DOC: OnceLock<Option<Document<'static>>> = OnceLock::new();
    DOC.get_or_init(|| Document::parse(PRESETS).ok()).as_ref()
}

const EMU_PER_PT: f64 = 12700.0;

#[derive(Clone, Debug)]
pub(crate) enum Cmd {
    M(f64, f64),
    L(f64, f64),
    C(f64, f64, f64, f64, f64, f64),
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FillMode {
    None,
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

#[derive(Clone, Debug)]
pub(crate) struct GPath {
    pub cmds: Vec<Cmd>,
    pub fill: FillMode,
    pub stroke: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub paths: Vec<GPath>,
    /// 글 상자 (l, t, r, b) pt
    pub text_rect: (f64, f64, f64, f64),
}

impl Geometry {
    pub fn rect(w: f64, h: f64) -> Geometry {
        Geometry {
            paths: vec![GPath {
                cmds: vec![Cmd::M(0.0, 0.0), Cmd::L(w, 0.0), Cmd::L(w, h), Cmd::L(0.0, h), Cmd::Z],
                fill: FillMode::Norm,
                stroke: true,
            }],
            text_rect: (0.0, 0.0, w, h),
        }
    }

    /// SVG 경로 문자열.
    pub fn svg_d(cmds: &[Cmd]) -> String {
        let mut s = String::with_capacity(cmds.len() * 16);
        for c in cmds {
            match *c {
                Cmd::M(x, y) => s.push_str(&format!("M{} {}", f(x), f(y))),
                Cmd::L(x, y) => s.push_str(&format!("L{} {}", f(x), f(y))),
                Cmd::C(a, b, c2, d, e, g) => {
                    s.push_str(&format!("C{} {} {} {} {} {}", f(a), f(b), f(c2), f(d), f(e), f(g)))
                }
                Cmd::Z => s.push('Z'),
            }
        }
        s
    }
}

pub(crate) fn f(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

struct Env {
    vars: HashMap<String, f64>,
}

impl Env {
    fn new(w: f64, h: f64) -> Env {
        let mut v = HashMap::new();
        let ss = w.min(h);
        let ls = w.max(h);
        for (k, x) in [
            ("w", w),
            ("h", h),
            ("l", 0.0),
            ("t", 0.0),
            ("r", w),
            ("b", h),
            ("hc", w / 2.0),
            ("vc", h / 2.0),
            ("wd2", w / 2.0),
            ("wd3", w / 3.0),
            ("wd4", w / 4.0),
            ("wd5", w / 5.0),
            ("wd6", w / 6.0),
            ("wd8", w / 8.0),
            ("wd10", w / 10.0),
            ("wd12", w / 12.0),
            ("wd32", w / 32.0),
            ("hd2", h / 2.0),
            ("hd3", h / 3.0),
            ("hd4", h / 4.0),
            ("hd5", h / 5.0),
            ("hd6", h / 6.0),
            ("hd8", h / 8.0),
            ("hd10", h / 10.0),
            ("hd12", h / 12.0),
            ("hd32", h / 32.0),
            ("ss", ss),
            ("ls", ls),
            ("ssd2", ss / 2.0),
            ("ssd4", ss / 4.0),
            ("ssd6", ss / 6.0),
            ("ssd8", ss / 8.0),
            ("ssd16", ss / 16.0),
            ("ssd32", ss / 32.0),
            ("cd2", 10800000.0),
            ("cd4", 5400000.0),
            ("cd8", 2700000.0),
            ("3cd4", 16200000.0),
            ("3cd8", 8100000.0),
            ("5cd8", 13500000.0),
            ("7cd8", 18900000.0),
        ] {
            v.insert(k.to_owned(), x);
        }
        Env { vars: v }
    }

    fn arg(&self, s: &str) -> f64 {
        if let Ok(n) = s.parse::<f64>() {
            return n;
        }
        self.vars.get(s).copied().unwrap_or(0.0)
    }

    fn eval(&self, fmla: &str) -> f64 {
        let parts: Vec<&str> = fmla.split_whitespace().collect();
        let a = |i: usize| parts.get(i).map(|s| self.arg(s)).unwrap_or(0.0);
        let ang = |v: f64| v / 60000.0 * PI / 180.0;
        match parts.first().copied().unwrap_or("") {
            "val" => a(1),
            "*/" => {
                let z = a(3);
                if z == 0.0 {
                    0.0
                } else {
                    a(1) * a(2) / z
                }
            }
            "+-" => a(1) + a(2) - a(3),
            "+/" => {
                let z = a(3);
                if z == 0.0 {
                    0.0
                } else {
                    (a(1) + a(2)) / z
                }
            }
            "?:" => {
                if a(1) > 0.0 {
                    a(2)
                } else {
                    a(3)
                }
            }
            "abs" => a(1).abs(),
            "at2" => a(2).atan2(a(1)) * 180.0 / PI * 60000.0,
            "cat2" => a(1) * a(3).atan2(a(2)).cos(),
            "sat2" => a(1) * a(3).atan2(a(2)).sin(),
            "cos" => a(1) * ang(a(2)).cos(),
            "sin" => a(1) * ang(a(2)).sin(),
            "tan" => a(1) * ang(a(2)).tan(),
            "max" => a(1).max(a(2)),
            "min" => a(1).min(a(2)),
            "mod" => (a(1) * a(1) + a(2) * a(2) + a(3) * a(3)).sqrt(),
            "pin" => {
                let (lo, x, hi) = (a(1), a(2), a(3));
                if x < lo {
                    lo
                } else if x > hi {
                    hi
                } else {
                    x
                }
            }
            "sqrt" => a(1).max(0.0).sqrt(),
            _ => 0.0,
        }
    }

    fn run_list(&mut self, list: Option<Node<'_, '_>>, overrides: Option<&HashMap<String, f64>>) {
        let Some(list) = list else { return };
        for gd in list.children().filter(|n| n.has_tag_name("gd")) {
            let Some(name) = gd.attribute("name") else { continue };
            if let Some(v) = overrides.and_then(|o| o.get(name)) {
                self.vars.insert(name.to_owned(), *v);
                continue;
            }
            let v = self.eval(gd.attribute("fmla").unwrap_or("val 0"));
            self.vars.insert(name.to_owned(), v);
        }
    }
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// 도형 요소(prstGeom/custGeom)의 avLst 를 읽는다. 값은 미리 계산해 둔다.
pub(crate) fn read_adjust(geom: Node<'_, '_>, w: f64, h: f64) -> HashMap<String, f64> {
    let env = Env::new(w, h);
    let mut out = HashMap::new();
    if let Some(av) = child(geom, "avLst") {
        for gd in av.children().filter(|n| n.has_tag_name("gd")) {
            if let (Some(name), Some(fmla)) = (gd.attribute("name"), gd.attribute("fmla")) {
                out.insert(name.to_owned(), env.eval(fmla));
            }
        }
    }
    out
}

/// 표준 도형. w, h 는 EMU.
pub(crate) fn preset(name: &str, w: f64, h: f64, adjust: &HashMap<String, f64>) -> Option<Geometry> {
    let doc = presets()?;
    let def = doc.root_element().children().find(|n| n.is_element() && n.tag_name().name() == name)?;
    Some(build(def, w, h, Some(adjust)))
}

/// 자유형. w, h 는 EMU.
pub(crate) fn custom(cust: Node<'_, '_>, w: f64, h: f64) -> Geometry {
    build(cust, w, h, None)
}

fn build(def: Node<'_, '_>, w: f64, h: f64, adjust: Option<&HashMap<String, f64>>) -> Geometry {
    let mut env = Env::new(w, h);
    env.run_list(child(def, "avLst"), adjust);
    env.run_list(child(def, "gdLst"), None);

    let pt = |v: f64| v / EMU_PER_PT;
    let mut paths = Vec::new();
    if let Some(list) = child(def, "pathLst") {
        for path in list.children().filter(|n| n.has_tag_name("path")) {
            let pw = path.attribute("w").and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0);
            let ph = path.attribute("h").and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0);
            let sx = pw.map(|p| w / p).unwrap_or(1.0);
            let sy = ph.map(|p| h / p).unwrap_or(1.0);
            let fill = match path.attribute("fill") {
                Some("none") => FillMode::None,
                Some("lighten") => FillMode::Lighten,
                Some("lightenLess") => FillMode::LightenLess,
                Some("darken") => FillMode::Darken,
                Some("darkenLess") => FillMode::DarkenLess,
                _ => FillMode::Norm,
            };
            let stroke = !matches!(path.attribute("stroke"), Some("0") | Some("false"));
            let mut cmds = Vec::new();
            let (mut cx, mut cy) = (0.0f64, 0.0f64);
            let point = |n: Node<'_, '_>, env: &Env| -> (f64, f64) {
                (env.arg(n.attribute("x").unwrap_or("0")) * sx, env.arg(n.attribute("y").unwrap_or("0")) * sy)
            };
            for cmd in path.children().filter(|n| n.is_element()) {
                let pts: Vec<(f64, f64)> =
                    cmd.children().filter(|n| n.has_tag_name("pt")).map(|p| point(p, &env)).collect();
                match cmd.tag_name().name() {
                    "moveTo" => {
                        if let Some(&(x, y)) = pts.first() {
                            cmds.push(Cmd::M(pt(x), pt(y)));
                            cx = x;
                            cy = y;
                        }
                    }
                    "lnTo" => {
                        if let Some(&(x, y)) = pts.first() {
                            cmds.push(Cmd::L(pt(x), pt(y)));
                            cx = x;
                            cy = y;
                        }
                    }
                    "cubicBezTo" => {
                        if pts.len() >= 3 {
                            cmds.push(Cmd::C(pt(pts[0].0), pt(pts[0].1), pt(pts[1].0), pt(pts[1].1), pt(pts[2].0), pt(pts[2].1)));
                            cx = pts[2].0;
                            cy = pts[2].1;
                        }
                    }
                    "quadBezTo" => {
                        if pts.len() >= 2 {
                            let (qx, qy) = pts[0];
                            let (ex, ey) = pts[1];
                            let c1 = (cx + 2.0 / 3.0 * (qx - cx), cy + 2.0 / 3.0 * (qy - cy));
                            let c2 = (ex + 2.0 / 3.0 * (qx - ex), ey + 2.0 / 3.0 * (qy - ey));
                            cmds.push(Cmd::C(pt(c1.0), pt(c1.1), pt(c2.0), pt(c2.1), pt(ex), pt(ey)));
                            cx = ex;
                            cy = ey;
                        }
                    }
                    "arcTo" => {
                        let g = |n: &str| env.arg(cmd.attribute(n).unwrap_or("0"));
                        let (wr, hr) = (g("wR") * sx, g("hR") * sy);
                        let (st, sw) = (g("stAng"), g("swAng"));
                        let (nx, ny) = arc(&mut cmds, cx, cy, wr, hr, st, sw, &pt);
                        cx = nx;
                        cy = ny;
                    }
                    "close" => cmds.push(Cmd::Z),
                    _ => {}
                }
            }
            if !cmds.is_empty() {
                paths.push(GPath { cmds, fill, stroke });
            }
        }
    }

    let text_rect = match child(def, "rect") {
        Some(r) => {
            let g = |n: &str, d: &str| pt(env.arg(r.attribute(n).unwrap_or(d)));
            (g("l", "l"), g("t", "t"), g("r", "r"), g("b", "b"))
        }
        None => (0.0, 0.0, pt(w), pt(h)),
    };
    Geometry { paths, text_rect }
}

/// 타원 호를 베지어로. 각은 '보이는 각'(60000분의 1도)이다.
#[allow(clippy::too_many_arguments)]
fn arc(cmds: &mut Vec<Cmd>, cx: f64, cy: f64, wr: f64, hr: f64, st: f64, sw: f64, pt: &dyn Fn(f64) -> f64) -> (f64, f64) {
    if wr <= 0.0 || hr <= 0.0 {
        return (cx, cy);
    }
    let rad = |v: f64| v / 60000.0 * PI / 180.0;
    let param = |a: f64| (wr * a.sin()).atan2(hr * a.cos());
    let a1 = rad(st);
    let a2 = rad(st + sw);
    let t1 = param(a1);
    let mut dt = param(a2) - t1;
    let full = sw.abs() >= 21_600_000.0 - 1.0;
    if full {
        dt = if sw > 0.0 { 2.0 * PI } else { -2.0 * PI };
    } else if sw > 0.0 {
        while dt <= 0.0 {
            dt += 2.0 * PI;
        }
        // 보이는 각이 한 바퀴를 넘으면 그만큼 더 돈다.
        while dt < rad(sw) - PI {
            dt += 2.0 * PI;
        }
    } else if sw < 0.0 {
        while dt >= 0.0 {
            dt -= 2.0 * PI;
        }
        while dt > rad(sw) + PI {
            dt -= 2.0 * PI;
        }
    } else {
        return (cx, cy);
    }
    // 시작점이 타원 위 t1 에 오도록 중심을 잡는다.
    let ox = cx - wr * t1.cos();
    let oy = cy - hr * t1.sin();
    let n = (dt.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
    let step = dt / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut t = t1;
    let (mut px, mut py) = (cx, cy);
    for _ in 0..n {
        let t2 = t + step;
        let (x1, y1) = (px - k * wr * t.sin(), py + k * hr * t.cos());
        let (ex, ey) = (ox + wr * t2.cos(), oy + hr * t2.sin());
        let (x2, y2) = (ex + k * wr * t2.sin(), ey - k * hr * t2.cos());
        cmds.push(Cmd::C(pt(x1), pt(y1), pt(x2), pt(y2), pt(ex), pt(ey)));
        px = ex;
        py = ey;
        t = t2;
    }
    (px, py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_build() {
        let doc = presets().expect("presets parse");
        let mut count = 0;
        for def in doc.root_element().children().filter(|n| n.is_element()) {
            let g = preset(def.tag_name().name(), 1_270_000.0, 635_000.0, &HashMap::new()).unwrap();
            for p in &g.paths {
                for c in &p.cmds {
                    if let Cmd::C(a, b, c2, d, e, f2) = c {
                        assert!([a, b, c2, d, e, f2].iter().all(|v| v.is_finite()), "{}", def.tag_name().name());
                    }
                }
            }
            count += 1;
        }
        assert_eq!(count, 187);
    }

    #[test]
    fn ellipse_is_closed_round() {
        let g = preset("ellipse", 1_270_000.0, 1_270_000.0, &HashMap::new()).unwrap();
        let d = Geometry::svg_d(&g.paths[0].cmds);
        assert!(d.starts_with("M0 50"), "{d}");
        assert!(d.contains("C"));
    }
}
