//! 차트(c:chartSpace): 막대·꺾은선·영역·원·도넛·분산·방사형, 축·눈금·범례·이름표.

use roxmltree::{Document, Node};

use super::color::{Color, Palette};
use super::fill::{self, Fill, Line};
use super::geom::{f, Geometry};
use super::render::{child, Renderer, Scope};
use super::svg::Svg;
use super::text;

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    Radar,
}

#[derive(Clone, Debug)]
struct Series {
    name: String,
    cats: Vec<String>,
    vals: Vec<Option<f64>>,
    xs: Vec<Option<f64>>,
    fill: Option<Fill>,
    line: Option<Line>,
    points: Vec<(usize, Option<Fill>)>,
    marker: Option<(String, f64, Option<Fill>)>,
    labels: Labels,
    smooth: bool,
    format: String,
}

#[derive(Clone, Debug, Default)]
struct Labels {
    val: bool,
    pct: bool,
    cat: bool,
    ser: bool,
    pos: String,
    size: Option<f64>,
    color: Option<Color>,
    format: Option<String>,
}

#[derive(Clone, Debug)]
struct Group {
    kind: Kind,
    horizontal: bool,
    grouping: String,
    gap: f64,
    overlap: f64,
    vary: bool,
    hole: f64,
    first_angle: f64,
    series: Vec<Series>,
    radar_filled: bool,
    ax_ids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct Axis {
    id: String,
    kind: String,
    deleted: bool,
    pos: String,
    min: Option<f64>,
    max: Option<f64>,
    major: Option<f64>,
    reverse: bool,
    grid: Option<Line>,
    line: Option<Line>,
    format: String,
    label_size: Option<f64>,
    label_color: Option<Color>,
    no_labels: bool,
    title: Option<String>,
}

fn strings(n: Option<Node<'_, '_>>) -> Vec<String> {
    let Some(n) = n else { return Vec::new() };
    // strRef/strCache, numRef/numCache, multiLvlStrRef (마지막 단계), strLit/numLit
    let cache = n
        .descendants()
        .find(|d| matches!(d.tag_name().name(), "strCache" | "numCache" | "strLit" | "numLit"))
        .or_else(|| n.descendants().find(|d| d.tag_name().name() == "lvl"));
    let Some(cache) = cache else { return Vec::new() };
    let count = child(cache, "ptCount").and_then(|c| c.attribute("val")).and_then(|v| v.parse::<usize>().ok());
    let pts: Vec<(usize, String)> = cache
        .children()
        .filter(|p| p.has_tag_name("pt"))
        .filter_map(|p| {
            let idx = p.attribute("idx")?.parse::<usize>().ok()?;
            Some((idx, child(p, "v").and_then(|v| v.text()).unwrap_or("").to_owned()))
        })
        .collect();
    let len = count.unwrap_or_else(|| pts.iter().map(|p| p.0 + 1).max().unwrap_or(0)).min(10000);
    let mut out = vec![String::new(); len];
    for (i, v) in pts {
        if i < len {
            out[i] = v;
        }
    }
    out
}

fn numbers(n: Option<Node<'_, '_>>) -> (Vec<Option<f64>>, String) {
    let format = n
        .and_then(|n| n.descendants().find(|d| d.has_tag_name("formatCode")))
        .and_then(|d| d.text())
        .unwrap_or("General")
        .to_owned();
    (strings(n).iter().map(|s| s.trim().parse::<f64>().ok()).collect(), format)
}

fn rich_text(n: Option<Node<'_, '_>>) -> Option<String> {
    let n = n?;
    let mut lines = Vec::new();
    for p in n.descendants().filter(|d| d.has_tag_name("p")) {
        let s: String = p.descendants().filter(|d| d.has_tag_name("t")).filter_map(|t| t.text()).collect();
        lines.push(s);
    }
    if lines.is_empty() {
        let s = strings(Some(n)).join(" ");
        return (!s.is_empty()).then_some(s);
    }
    Some(lines.join(" ")).filter(|s| !s.trim().is_empty())
}

fn tx_size_color(n: Option<Node<'_, '_>>, palette: &Palette) -> (Option<f64>, Option<Color>, Option<bool>) {
    let Some(n) = n else { return (None, None, None) };
    let rpr = n.descendants().find(|d| d.has_tag_name("defRPr") || d.has_tag_name("rPr"));
    let Some(rpr) = rpr else { return (None, None, None) };
    let size = rpr.attribute("sz").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0);
    let color = child(rpr, "solidFill").and_then(|s| super::color::color_child(s, palette, None));
    let bold = rpr.attribute("b").map(|v| v == "1");
    (size, color, bold)
}

fn sp_fill(n: Node<'_, '_>, palette: &Palette) -> (Option<Fill>, Option<Line>) {
    let Some(sp) = child(n, "spPr") else { return (None, None) };
    let fl = fill::read_fill(sp, palette, None);
    let ln = child(sp, "ln").map(|ln| {
        let mut l = Line { fill: Fill::None, width: 0.75, ..Default::default() };
        fill::apply_line(&mut l, ln, palette, None);
        l
    });
    (fl, ln)
}

fn read_labels(n: Option<Node<'_, '_>>, palette: &Palette, base: &Labels) -> Labels {
    let mut l = base.clone();
    let Some(n) = n else { return l };
    let flag = |name: &str| child(n, name).and_then(|c| c.attribute("val")).map(|v| v == "1" || v == "true");
    if child(n, "delete").and_then(|c| c.attribute("val")).map(|v| v == "1") == Some(true) {
        return Labels::default();
    }
    if let Some(v) = flag("showVal") {
        l.val = v;
    }
    if let Some(v) = flag("showPercent") {
        l.pct = v;
    }
    if let Some(v) = flag("showCatName") {
        l.cat = v;
    }
    if let Some(v) = flag("showSerName") {
        l.ser = v;
    }
    if let Some(p) = child(n, "dLblPos").and_then(|c| c.attribute("val")) {
        l.pos = p.to_owned();
    }
    if let Some(fc) = child(n, "numFmt").and_then(|c| c.attribute("formatCode")) {
        l.format = Some(fc.to_owned());
    }
    let (s, c, _) = tx_size_color(child(n, "txPr"), palette);
    if s.is_some() {
        l.size = s;
    }
    if c.is_some() {
        l.color = c;
    }
    l
}

pub(crate) fn format_number(v: f64, code: &str) -> String {
    let code = code.split(';').next().unwrap_or("General");
    let pct = code.contains('%');
    let v = if pct { v * 100.0 } else { v };
    let decimals = code.split('.').nth(1).map(|d| d.chars().take_while(|c| *c == '0' || *c == '#').count());
    let thousands = code.contains("#,##") || code.contains("#,#");
    let mut s = match decimals {
        Some(d) => format!("{:.*}", d, v),
        None if code.eq_ignore_ascii_case("general") || code.is_empty() => {
            let r = (v * 1e6).round() / 1e6;
            let mut t = format!("{r}");
            if t.contains('e') {
                t = format!("{v:.3}");
            }
            t
        }
        None => format!("{:.0}", v),
    };
    if thousands {
        let (int, frac) = match s.find('.') {
            Some(i) => (s[..i].to_owned(), s[i..].to_owned()),
            None => (s.clone(), String::new()),
        };
        let neg = int.starts_with('-');
        let digits: Vec<char> = int.trim_start_matches('-').chars().collect();
        let mut out = String::new();
        for (i, c) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(*c);
        }
        s = format!("{}{out}{frac}", if neg { "-" } else { "" });
    }
    if pct {
        s.push('%');
    }
    // 앞뒤 글자 ("원", "$" 등)
    let lit: String = code.chars().filter(|c| !"0#,.%?General\\ _*@;\"".contains(*c)).collect();
    if code.starts_with('$') || code.starts_with("\"$\"") {
        s = format!("${s}");
    } else if !lit.is_empty() && code.ends_with('"') {
        let tail = code.rsplit('"').nth(1).unwrap_or("");
        s.push_str(tail);
    }
    s
}

fn nice_step(range: f64, target: usize) -> f64 {
    if range <= 0.0 || !range.is_finite() {
        return 1.0;
    }
    let raw = range / target.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let nice = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 2.5 {
        2.5
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn axis_range(min_v: f64, max_v: f64, ax: Option<&Axis>, target: usize) -> (f64, f64, f64) {
    let (mut lo, mut hi) = (min_v.min(0.0), max_v.max(0.0));
    if min_v > 0.0 && (max_v - min_v) < max_v * 0.15 {
        lo = min_v;
    }
    if (hi - lo).abs() < 1e-12 {
        hi = lo + 1.0;
    }
    let step = ax.and_then(|a| a.major).unwrap_or_else(|| nice_step(hi - lo, target));
    let lo2 = ax.and_then(|a| a.min).unwrap_or_else(|| (lo / step).floor() * step);
    let mut hi2 = ax.and_then(|a| a.max).unwrap_or_else(|| (hi / step).ceil() * step);
    if hi2 <= lo2 {
        hi2 = lo2 + step;
    }
    (lo2, hi2, step)
}

const DEFAULT_ACCENTS: [&str; 6] = ["accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];

fn auto_color(palette: &Palette, i: usize) -> Color {
    let base = palette.scheme_color(DEFAULT_ACCENTS[i % 6]).unwrap_or(Color::rgb(68, 114, 196));
    let round = i / 6;
    match round {
        0 => base,
        1 => base.mix(Color::BLACK, 0.4),
        2 => base.mix(Color::WHITE, 0.4),
        _ => base.mix(Color::BLACK, 0.2),
    }
}

pub(crate) fn render<'d, 'i>(
    r: &mut Renderer<'d, 'i>,
    svg: &mut Svg,
    doc: &'d Document<'i>,
    part: &str,
    scope: &Scope<'_, 'd, 'i>,
    w: f64,
    h: f64,
) {
    let _ = part;
    let root = doc.root_element();
    let palette = scope.palette;
    let Some(chart) = child(root, "chart") else { return };
    let family = scope.theme.fonts.resolve(Some("+mn-lt"), false, None);
    let fam = family.as_deref();
    let (base_size, base_color, _) = tx_size_color(child(root, "txPr"), palette);
    let base_size = base_size.unwrap_or(10.0);
    let text_color = base_color.unwrap_or_else(|| palette.scheme_color("tx1").unwrap_or(Color::BLACK).mix(Color::WHITE, 0.35));

    // 차트 바탕
    let (bg, border) = sp_fill(root, palette);
    let rect = Geometry::rect(w, h);
    if let Some(bg) = &bg {
        r.draw_fill(svg, bg, &rect, w, h, scope);
    } else if child(root, "spPr").is_none() {
        r.draw_fill(svg, &Fill::Solid(Color::WHITE), &rect, w, h, scope);
    }
    if let Some(b) = &border {
        if let Some(a) = fill::stroke_attrs(svg, b, 0.0, 0.0, w, h) {
            svg.body.push_str(&format!("<rect width=\"{}\" height=\"{}\" fill=\"none\"{a}/>", f(w), f(h)));
        }
    }

    let Some(plot) = child(chart, "plotArea") else { return };

    // 축
    let mut axes: Vec<Axis> = Vec::new();
    for a in plot.children().filter(|c| matches!(c.tag_name().name(), "catAx" | "valAx" | "dateAx" | "serAx")) {
        let val = |name: &str| child(a, name).and_then(|c| c.attribute("val"));
        let scaling = child(a, "scaling");
        let sv = |name: &str| scaling.and_then(|s| child(s, name)).and_then(|c| c.attribute("val")).and_then(|v| v.parse::<f64>().ok());
        let (ls, lc, _) = tx_size_color(child(a, "txPr"), palette);
        let grid = child(a, "majorGridlines").map(|g| {
            let mut l = Line { fill: Fill::Solid(Color::rgb(217, 217, 217)), width: 0.75, ..Default::default() };
            if let Some(ln) = child(g, "spPr").and_then(|s| child(s, "ln")) {
                fill::apply_line(&mut l, ln, palette, None);
            }
            l
        });
        let line = child(a, "spPr").and_then(|s| child(s, "ln")).map(|ln| {
            let mut l = Line { fill: Fill::None, ..Default::default() };
            fill::apply_line(&mut l, ln, palette, None);
            l
        });
        axes.push(Axis {
            id: val("axId").unwrap_or("").to_owned(),
            kind: a.tag_name().name().to_owned(),
            deleted: val("delete").map(|v| v == "1" || v == "true").unwrap_or(false),
            pos: val("axPos").unwrap_or("b").to_owned(),
            min: sv("min"),
            max: sv("max"),
            major: val("majorUnit").and_then(|v| v.parse::<f64>().ok()),
            reverse: scaling.and_then(|s| child(s, "orientation")).and_then(|o| o.attribute("val")) == Some("maxMin"),
            grid,
            line,
            format: child(a, "numFmt").and_then(|n| n.attribute("formatCode")).unwrap_or("General").to_owned(),
            label_size: ls,
            label_color: lc,
            no_labels: val("tickLblPos") == Some("none"),
            title: child(a, "title").and_then(|t| rich_text(child(t, "tx").and_then(|x| child(x, "rich")))),
        });
    }

    // 계열 묶음
    let mut groups: Vec<Group> = Vec::new();
    for g in plot.children().filter(|c| c.is_element()) {
        let name = g.tag_name().name();
        let kind = match name {
            "barChart" | "bar3DChart" => Kind::Bar,
            "lineChart" | "line3DChart" | "stockChart" => Kind::Line,
            "areaChart" | "area3DChart" => Kind::Area,
            "pieChart" | "pie3DChart" | "ofPieChart" => Kind::Pie,
            "doughnutChart" => Kind::Doughnut,
            "scatterChart" | "bubbleChart" => Kind::Scatter,
            "radarChart" => Kind::Radar,
            _ => continue,
        };
        let val = |n: &str| child(g, n).and_then(|c| c.attribute("val"));
        let group_labels = read_labels(child(g, "dLbls"), palette, &Labels::default());
        let mut series = Vec::new();
        for s in g.children().filter(|c| c.has_tag_name("ser")) {
            let name = child(s, "tx").and_then(|t| rich_text(Some(t))).unwrap_or_default();
            let cats = strings(child(s, "cat").or_else(|| child(s, "xVal")));
            let (vals, format) = numbers(child(s, "val").or_else(|| child(s, "yVal")));
            let xs = numbers(child(s, "xVal")).0;
            let (fl, ln) = sp_fill(s, palette);
            let points = s
                .children()
                .filter(|c| c.has_tag_name("dPt"))
                .filter_map(|p| {
                    let idx = child(p, "idx")?.attribute("val")?.parse::<usize>().ok()?;
                    Some((idx, sp_fill(p, palette).0))
                })
                .collect();
            let marker = child(s, "marker").map(|m| {
                let sym = child(m, "symbol").and_then(|c| c.attribute("val")).unwrap_or("auto").to_owned();
                let size = child(m, "size").and_then(|c| c.attribute("val")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(5.0);
                (sym, size, sp_fill(m, palette).0)
            });
            series.push(Series {
                name,
                cats,
                vals,
                xs,
                fill: fl,
                line: ln,
                points,
                marker,
                labels: read_labels(child(s, "dLbls"), palette, &group_labels),
                smooth: child(s, "smooth").and_then(|c| c.attribute("val")).map(|v| v == "1").unwrap_or(false),
                format,
            });
        }
        series.sort_by_key(|_| 0);
        groups.push(Group {
            kind: kind.clone(),
            horizontal: val("barDir") == Some("bar"),
            grouping: val("grouping").unwrap_or(if kind == Kind::Bar { "clustered" } else { "standard" }).to_owned(),
            gap: val("gapWidth").and_then(|v| v.parse::<f64>().ok()).unwrap_or(150.0) / 100.0,
            overlap: val("overlap").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 100.0,
            vary: val("varyColors").map(|v| v == "1").unwrap_or(matches!(kind, Kind::Pie | Kind::Doughnut)),
            hole: val("holeSize").and_then(|v| v.parse::<f64>().ok()).unwrap_or(50.0) / 100.0,
            first_angle: val("firstSliceAng").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0),
            radar_filled: val("radarStyle") == Some("filled"),
            ax_ids: g.children().filter(|c| c.has_tag_name("axId")).filter_map(|c| c.attribute("val").map(|v| v.to_owned())).collect(),
            series,
        });
    }
    if groups.is_empty() {
        return;
    }

    // 제목
    let mut top = 6.0;
    let auto_deleted = child(chart, "autoTitleDeleted").and_then(|c| c.attribute("val")).map(|v| v == "1").unwrap_or(false);
    let title_node = child(chart, "title");
    let title = match title_node {
        Some(t) => rich_text(child(t, "tx").and_then(|x| child(x, "rich")).or_else(|| child(t, "tx"))).or_else(|| {
            let all: Vec<&Series> = groups.iter().flat_map(|g| g.series.iter()).collect();
            (all.len() == 1).then(|| all[0].name.clone())
        }),
        None if !auto_deleted => {
            let all: Vec<&Series> = groups.iter().flat_map(|g| g.series.iter()).collect();
            if all.len() == 1 && matches!(groups[0].kind, Kind::Pie | Kind::Doughnut) {
                Some(all[0].name.clone())
            } else {
                None
            }
        }
        None => None,
    };
    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        let (ts, tc, tb) = tx_size_color(title_node.and_then(|n| child(n, "tx")).or_else(|| title_node.and_then(|n| child(n, "txPr"))), palette);
        let size = ts.unwrap_or(base_size * 1.4);
        text::label(svg, &mut r.fonts, fam, &t, w / 2.0, top + size, size, tc.unwrap_or(text_color), tb.unwrap_or(false), 'c', 0.0);
        top += size * 1.6;
    }

    // 범례
    let legend = child(chart, "legend");
    let legend_pos = legend.and_then(|l| child(l, "legendPos")).and_then(|c| c.attribute("val")).unwrap_or("r").to_owned();
    let (lsize, lcolor, _) = tx_size_color(legend.and_then(|l| child(l, "txPr")), palette);
    let lsize = lsize.unwrap_or(base_size);
    let lcolor = lcolor.unwrap_or(text_color);
    let pie_like = matches!(groups[0].kind, Kind::Pie | Kind::Doughnut);
    let mut entries: Vec<(String, Fill, bool)> = Vec::new();
    if legend.is_some() {
        if pie_like || (groups.len() == 1 && groups[0].vary && groups[0].series.len() == 1) {
            let s = &groups[0].series[0];
            for (i, c) in s.cats.iter().enumerate() {
                entries.push((c.clone(), point_fill(s, i, palette, &groups[0], 0), false));
            }
        } else {
            let mut k = 0;
            for g in &groups {
                for s in &g.series {
                    let is_line = matches!(g.kind, Kind::Line | Kind::Scatter | Kind::Radar) && !g.radar_filled;
                    let fl = series_fill(s, palette, k, is_line);
                    entries.push((s.name.clone(), fl, is_line));
                    k += 1;
                }
            }
        }
    }
    let (mut left, mut right, mut bottom) = (8.0, w - 8.0, h - 6.0);
    if !entries.is_empty() {
        let item_w: Vec<f64> = entries.iter().map(|e| text::label_width(&mut r.fonts, fam, &e.0, lsize, false) + lsize * 1.6).collect();
        match legend_pos.as_str() {
            "b" | "t" => {
                let total: f64 = item_w.iter().sum();
                let y = if legend_pos == "b" { bottom - lsize * 0.5 } else { top + lsize };
                let mut x = (w - total.min(w - 16.0)) / 2.0;
                for (k, e) in entries.iter().enumerate() {
                    legend_key(svg, &e.1, e.2, x, y - lsize * 0.8, lsize * 0.8);
                    text::label(svg, &mut r.fonts, fam, &e.0, x + lsize * 1.1, y, lsize, lcolor, false, 'l', 0.0);
                    x += item_w[k];
                }
                if legend_pos == "b" {
                    bottom -= lsize * 2.0;
                } else {
                    top += lsize * 2.0;
                }
            }
            _ => {
                let maxw = item_w.iter().cloned().fold(0.0, f64::max).min(w * 0.4);
                let total_h = entries.len() as f64 * lsize * 1.5;
                let x = if legend_pos == "l" { 8.0 } else { w - maxw - 6.0 };
                let mut y = (top + bottom) / 2.0 - total_h / 2.0 + lsize;
                for e in &entries {
                    legend_key(svg, &e.1, e.2, x, y - lsize * 0.8, lsize * 0.8);
                    text::label(svg, &mut r.fonts, fam, &e.0, x + lsize * 1.1, y, lsize, lcolor, false, 'l', 0.0);
                    y += lsize * 1.5;
                }
                if legend_pos == "l" {
                    left += maxw + 8.0;
                } else {
                    right -= maxw + 10.0;
                }
            }
        }
    }

    // 손으로 잡은 그림 영역
    if let Some(ml) = child(plot, "layout").and_then(|l| child(l, "manualLayout")) {
        let v = |n: &str| child(ml, n).and_then(|c| c.attribute("val")).and_then(|v| v.parse::<f64>().ok());
        if let (Some(x), Some(y), Some(mw), Some(mh)) = (v("x"), v("y"), v("w"), v("h")) {
            left = x * w;
            top = y * h;
            right = (x + mw) * w;
            bottom = (y + mh) * h;
        }
    }

    let first = &groups[0];
    match first.kind {
        Kind::Pie | Kind::Doughnut => {
            let cx = (left + right) / 2.0;
            let cy = (top + bottom) / 2.0;
            let rad = ((right - left).min(bottom - top) / 2.0 - 4.0).max(4.0);
            draw_pie(r, svg, first, palette, cx, cy, rad, fam, base_size, text_color);
        }
        Kind::Radar => draw_radar(r, svg, &groups, &axes, palette, left, top, right, bottom, fam, base_size, text_color),
        _ => draw_xy(r, svg, &groups, &axes, palette, left, top, right, bottom, fam, base_size, text_color),
    }
}

fn series_fill(s: &Series, palette: &Palette, k: usize, is_line: bool) -> Fill {
    if is_line {
        if let Some(l) = &s.line {
            if !matches!(l.fill, Fill::None) {
                return l.fill.clone();
            }
        }
        return Fill::Solid(auto_color(palette, k));
    }
    s.fill.clone().unwrap_or_else(|| Fill::Solid(auto_color(palette, k)))
}

fn point_fill(s: &Series, i: usize, palette: &Palette, g: &Group, k: usize) -> Fill {
    if let Some((_, Some(fl))) = s.points.iter().find(|p| p.0 == i) {
        return fl.clone();
    }
    if g.vary {
        return Fill::Solid(auto_color(palette, i));
    }
    series_fill(s, palette, k, false)
}

fn legend_key(svg: &mut Svg, fl: &Fill, line: bool, x: f64, y: f64, s: f64) {
    let c = fl.first_color().unwrap_or(Color::BLACK);
    if line {
        svg.body.push_str(&format!(
            "<path d=\"M{} {}H{}\" stroke=\"{}\" stroke-width=\"2\"/>",
            f(x),
            f(y + s / 2.0),
            f(x + s),
            c.css()
        ));
    } else {
        svg.body.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>", f(x), f(y), f(s), f(s), c.css()));
    }
}

fn fill_attr(svg: &mut Svg, fl: &Fill, x: f64, y: f64, w: f64, h: f64) -> String {
    match fill::paint(svg, fl, x, y, w.max(0.1), h.max(0.1)) {
        Some((p, a)) => format!("fill=\"{p}\"{}", if a < 1.0 { format!(" fill-opacity=\"{}\"", f(a as f64)) } else { String::new() }),
        None => "fill=\"none\"".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pie(
    r: &mut Renderer<'_, '_>,
    svg: &mut Svg,
    g: &Group,
    palette: &Palette,
    cx: f64,
    cy: f64,
    rad: f64,
    fam: Option<&str>,
    size: f64,
    color: Color,
) {
    let Some(s) = g.series.first() else { return };
    let total: f64 = s.vals.iter().flatten().filter(|v| **v > 0.0).sum();
    if total <= 0.0 {
        return;
    }
    let mut a = (g.first_angle - 90.0).to_radians();
    let inner = if g.kind == Kind::Doughnut { rad * g.hole } else { 0.0 };
    for (i, v) in s.vals.iter().enumerate() {
        let Some(v) = v.filter(|v| *v > 0.0) else { continue };
        let sweep = v / total * std::f64::consts::TAU;
        let a2 = a + sweep;
        let large = if sweep > std::f64::consts::PI { 1 } else { 0 };
        let (x1, y1) = (cx + rad * a.cos(), cy + rad * a.sin());
        let (x2, y2) = (cx + rad * a2.cos(), cy + rad * a2.sin());
        let d = if sweep >= std::f64::consts::TAU - 1e-6 {
            if inner > 0.0 {
                format!(
                    "M{} {}A{r} {r} 0 1 1 {} {}A{r} {r} 0 1 1 {} {}ZM{} {}A{ir} {ir} 0 1 0 {} {}A{ir} {ir} 0 1 0 {} {}Z",
                    f(cx + rad), f(cy), f(cx - rad), f(cy), f(cx + rad), f(cy),
                    f(cx + inner), f(cy), f(cx - inner), f(cy), f(cx + inner), f(cy),
                    r = f(rad), ir = f(inner)
                )
            } else {
                format!("M{} {}A{r} {r} 0 1 1 {} {}A{r} {r} 0 1 1 {} {}Z", f(cx + rad), f(cy), f(cx - rad), f(cy), f(cx + rad), f(cy), r = f(rad))
            }
        } else if inner > 0.0 {
            let (x3, y3) = (cx + inner * a2.cos(), cy + inner * a2.sin());
            let (x4, y4) = (cx + inner * a.cos(), cy + inner * a.sin());
            format!(
                "M{} {}A{r} {r} 0 {large} 1 {} {}L{} {}A{ir} {ir} 0 {large} 0 {} {}Z",
                f(x1), f(y1), f(x2), f(y2), f(x3), f(y3), f(x4), f(y4), r = f(rad), ir = f(inner)
            )
        } else {
            format!("M{} {}L{} {}A{r} {r} 0 {large} 1 {} {}Z", f(cx), f(cy), f(x1), f(y1), f(x2), f(y2), r = f(rad))
        };
        let fl = point_fill(s, i, palette, g, 0);
        let fa = fill_attr(svg, &fl, cx - rad, cy - rad, rad * 2.0, rad * 2.0);
        svg.body.push_str(&format!("<path d=\"{d}\" {fa} stroke=\"#ffffff\" stroke-width=\"1\"/>"));
        // 이름표
        let lb = &s.labels;
        if lb.val || lb.pct || lb.cat {
            let mut parts = Vec::new();
            if lb.cat {
                parts.push(s.cats.get(i).cloned().unwrap_or_default());
            }
            if lb.val {
                parts.push(format_number(v, lb.format.as_deref().unwrap_or(&s.format)));
            }
            if lb.pct {
                parts.push(format!("{:.0}%", v / total * 100.0));
            }
            let mid = a + sweep / 2.0;
            let outside = lb.pos == "outEnd" || lb.pos == "bestFit" && sweep < 0.3;
            let rr = if outside { rad * 1.12 } else if inner > 0.0 { (rad + inner) / 2.0 } else { rad * 0.65 };
            let (lx, ly) = (cx + rr * mid.cos(), cy + rr * mid.sin());
            let ls = lb.size.unwrap_or(size);
            let lc = lb.color.unwrap_or(if outside { color } else { Color::WHITE });
            let anchor = if outside { if mid.cos() >= 0.0 { 'l' } else { 'r' } } else { 'c' };
            text::label(svg, &mut r.fonts, fam, &parts.join(" "), lx, ly + ls * 0.35, ls, lc, false, anchor, 0.0);
        }
        a = a2;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_xy(
    r: &mut Renderer<'_, '_>,
    svg: &mut Svg,
    groups: &[Group],
    axes: &[Axis],
    palette: &Palette,
    mut left: f64,
    top: f64,
    right: f64,
    mut bottom: f64,
    fam: Option<&str>,
    size: f64,
    color: Color,
) {
    let horizontal = groups.iter().any(|g| g.kind == Kind::Bar && g.horizontal);
    let scatter = groups.iter().all(|g| g.kind == Kind::Scatter);
    let ax_of = |kind: &[&str]| axes.iter().find(|a| kind.contains(&a.kind.as_str()));
    let val_ax = if scatter { axes.iter().filter(|a| a.kind == "valAx").nth(1).or(ax_of(&["valAx"])) } else { ax_of(&["valAx"]) };
    let cat_ax = if scatter { axes.iter().find(|a| a.kind == "valAx") } else { ax_of(&["catAx", "dateAx"]) };

    // 값 범위 (쌓기 고려)
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    let ncat = groups.iter().flat_map(|g| g.series.iter()).map(|s| s.vals.len().max(s.cats.len())).max().unwrap_or(0).max(1);
    let mut percent = false;
    for g in groups {
        if g.kind == Kind::Scatter {
            for s in &g.series {
                for v in s.vals.iter().flatten() {
                    lo = lo.min(*v);
                    hi = hi.max(*v);
                }
            }
            continue;
        }
        if g.grouping == "stacked" || g.grouping == "percentStacked" {
            percent |= g.grouping == "percentStacked";
            for i in 0..ncat {
                let (mut pos, mut neg) = (0.0, 0.0);
                for s in &g.series {
                    let v = s.vals.get(i).copied().flatten().unwrap_or(0.0);
                    if v >= 0.0 {
                        pos += v
                    } else {
                        neg += v
                    }
                }
                if g.grouping == "percentStacked" {
                    lo = lo.min(if neg < 0.0 { -1.0 } else { 0.0 });
                    hi = hi.max(1.0);
                } else {
                    lo = lo.min(neg);
                    hi = hi.max(pos);
                }
            }
        } else {
            for s in &g.series {
                for v in s.vals.iter().flatten() {
                    lo = lo.min(*v);
                    hi = hi.max(*v);
                }
            }
        }
    }
    if lo == f64::MAX {
        lo = 0.0;
        hi = 1.0;
    }
    let (vmin, vmax, step) = axis_range(lo, hi, val_ax, 6);
    let val_fmt = val_ax.map(|a| a.format.clone()).unwrap_or_else(|| "General".into());
    let val_fmt = if percent && !val_fmt.contains('%') { "0%".to_owned() } else { val_fmt };
    let lab_size = val_ax.and_then(|a| a.label_size).unwrap_or(size);
    let lab_color = val_ax.and_then(|a| a.label_color).unwrap_or(color);
    let cat_size = cat_ax.and_then(|a| a.label_size).unwrap_or(size);
    let cat_color = cat_ax.and_then(|a| a.label_color).unwrap_or(color);

    // 이름표 자리 만들기
    let ticks: Vec<f64> = {
        let mut v = Vec::new();
        let mut t = vmin;
        let mut n = 0;
        while t <= vmax + step * 1e-6 && n < 60 {
            v.push(t);
            t += step;
            n += 1;
        }
        v
    };
    let show_val = val_ax.map(|a| !a.deleted && !a.no_labels).unwrap_or(true);
    let show_cat = cat_ax.map(|a| !a.deleted && !a.no_labels).unwrap_or(true);
    let cats: Vec<String> = groups
        .iter()
        .flat_map(|g| g.series.iter())
        .map(|s| s.cats.clone())
        .find(|c| !c.is_empty())
        .unwrap_or_else(|| (1..=ncat).map(|i| i.to_string()).collect());
    if !horizontal {
        if show_val {
            let wmax = ticks.iter().map(|t| text::label_width(&mut r.fonts, fam, &format_number(*t, &val_fmt), lab_size, false)).fold(0.0, f64::max);
            left += wmax + 6.0;
        }
        if show_cat {
            bottom -= cat_size * 1.6;
        }
    } else {
        if show_cat {
            let wmax = cats.iter().map(|c| text::label_width(&mut r.fonts, fam, c, cat_size, false)).fold(0.0, f64::max);
            left += wmax.min((right - left) * 0.4) + 6.0;
        }
        if show_val {
            bottom -= lab_size * 1.6;
        }
    }
    let pw = (right - left).max(10.0);
    let ph = (bottom - top).max(10.0);
    let vmap = |v: f64| (v - vmin) / (vmax - vmin);
    // 값 → 좌표
    let to_val = |v: f64| if horizontal { left + vmap(v) * pw } else { bottom - vmap(v) * ph };

    // 눈금선과 값 이름표
    let grid = val_ax.and_then(|a| a.grid.clone()).or_else(|| val_ax.is_none().then(|| Line { fill: Fill::Solid(Color::rgb(217, 217, 217)), ..Default::default() }));
    for t in &ticks {
        let p = to_val(*t);
        if let Some(gl) = &grid {
            if let Some(a) = fill::stroke_attrs(svg, gl, left, top, pw, ph) {
                if horizontal {
                    svg.body.push_str(&format!("<path d=\"M{} {}V{}\" fill=\"none\"{a}/>", f(p), f(top), f(bottom)));
                } else {
                    svg.body.push_str(&format!("<path d=\"M{} {}H{}\" fill=\"none\"{a}/>", f(left), f(p), f(right)));
                }
            }
        }
        if show_val {
            let s = format_number(*t, &val_fmt);
            if horizontal {
                text::label(svg, &mut r.fonts, fam, &s, p, bottom + lab_size * 1.3, lab_size, lab_color, false, 'c', 0.0);
            } else {
                text::label(svg, &mut r.fonts, fam, &s, left - 4.0, p + lab_size * 0.35, lab_size, lab_color, false, 'r', 0.0);
            }
        }
    }

    // 분류 축
    let slot = if horizontal { ph / ncat as f64 } else { pw / ncat as f64 };
    let cat_pos = |i: usize| if horizontal { top + slot * (i as f64 + 0.5) } else { left + slot * (i as f64 + 0.5) };
    let line_on_cat = groups.iter().all(|g| matches!(g.kind, Kind::Line | Kind::Area)) && !scatter;
    let cat_pos_line = |i: usize| {
        if line_on_cat && groups.iter().any(|g| g.kind == Kind::Area) && ncat > 1 {
            left + pw * i as f64 / (ncat - 1) as f64
        } else {
            cat_pos(i)
        }
    };

    // 분산형: x 범위
    let (xmin, xmax) = if scatter {
        let mut a = f64::MAX;
        let mut b = f64::MIN;
        for g in groups {
            for s in &g.series {
                for (i, _) in s.vals.iter().enumerate() {
                    let x = s.xs.get(i).copied().flatten().unwrap_or(i as f64 + 1.0);
                    a = a.min(x);
                    b = b.max(x);
                }
            }
        }
        if a == f64::MAX {
            (0.0, 1.0)
        } else {
            let (l, h2, _) = axis_range(a, b, cat_ax, 6);
            (l, h2)
        }
    } else {
        (0.0, 1.0)
    };
    let to_x = |x: f64| left + (x - xmin) / (xmax - xmin).max(1e-9) * pw;

    if show_cat {
        if scatter {
            let (_, _, st) = axis_range(xmin, xmax, cat_ax, 6);
            let mut t = xmin;
            let mut n = 0;
            while t <= xmax + st * 1e-6 && n < 60 {
                let s = format_number(t, cat_ax.map(|a| a.format.as_str()).unwrap_or("General"));
                text::label(svg, &mut r.fonts, fam, &s, to_x(t), bottom + cat_size * 1.3, cat_size, cat_color, false, 'c', 0.0);
                t += st;
                n += 1;
            }
        } else {
            // 이름표가 겹치면 건너뛴다.
            let widths: Vec<f64> = cats.iter().map(|c| text::label_width(&mut r.fonts, fam, c, cat_size, false)).collect();
            let every = if horizontal {
                ((cat_size * 1.2) / slot).ceil().max(1.0) as usize
            } else {
                let maxw = widths.iter().cloned().fold(0.0, f64::max);
                ((maxw + 4.0) / slot).ceil().max(1.0) as usize
            };
            for (i, c) in cats.iter().enumerate().take(ncat) {
                if i % every != 0 {
                    continue;
                }
                if horizontal {
                    text::label(svg, &mut r.fonts, fam, c, left - 4.0, cat_pos(i) + cat_size * 0.35, cat_size, cat_color, false, 'r', 0.0);
                } else {
                    text::label(svg, &mut r.fonts, fam, c, cat_pos_line(i), bottom + cat_size * 1.3, cat_size, cat_color, false, 'c', 0.0);
                }
            }
        }
    }
    // 축선
    let axis_line = cat_ax
        .and_then(|a| a.line.clone())
        .unwrap_or(Line { fill: Fill::Solid(Color::rgb(191, 191, 191)), width: 0.75, ..Default::default() });
    if let Some(a) = fill::stroke_attrs(svg, &axis_line, left, top, pw, ph) {
        let z = to_val(0.0_f64.clamp(vmin, vmax));
        if horizontal {
            svg.body.push_str(&format!("<path d=\"M{} {}V{}\" fill=\"none\"{a}/>", f(z), f(top), f(bottom)));
        } else {
            svg.body.push_str(&format!("<path d=\"M{} {}H{}\" fill=\"none\"{a}/>", f(left), f(z), f(right)));
        }
    }

    // 계열 그리기
    let mut k = 0usize;
    let bar_groups = groups.iter().filter(|g| g.kind == Kind::Bar).count().max(1);
    let mut bar_index = 0usize;
    for g in groups {
        let n = g.series.len().max(1);
        let stacked = g.grouping == "stacked" || g.grouping == "percentStacked";
        let pct = g.grouping == "percentStacked";
        let totals: Vec<f64> = (0..ncat)
            .map(|i| g.series.iter().map(|s| s.vals.get(i).copied().flatten().unwrap_or(0.0).abs()).sum::<f64>().max(1e-12))
            .collect();
        let mut pos_acc = vec![0.0; ncat];
        let mut neg_acc = vec![0.0; ncat];
        match g.kind {
            Kind::Bar => {
                let overlap = if stacked { 1.0 } else { g.overlap };
                let bars = if stacked { 1.0 } else { n as f64 - (n as f64 - 1.0) * overlap };
                let gslot = slot / bar_groups as f64;
                let bw = gslot / (bars + g.gap);
                for (si, s) in g.series.iter().enumerate() {
                    let base_fill = series_fill(s, palette, k + si, false);
                    for i in 0..ncat {
                        let Some(mut v) = s.vals.get(i).copied().flatten() else { continue };
                        if pct {
                            v /= totals[i];
                        }
                        let (v0, v1) = if stacked {
                            let acc = if v >= 0.0 { &mut pos_acc[i] } else { &mut neg_acc[i] };
                            let a = *acc;
                            *acc += v;
                            (a, a + v)
                        } else {
                            (0.0_f64.clamp(vmin, vmax), v)
                        };
                        let off = if stacked { 0.0 } else { si as f64 * bw * (1.0 - overlap) };
                        let start = (if horizontal { top } else { left }) + slot * i as f64 + gslot * bar_index as f64 + bw * g.gap / 2.0 + off;
                        let (p0, p1) = (to_val(v0), to_val(v1));
                        let (x, y, ww, hh) = if horizontal {
                            (p0.min(p1), start, (p1 - p0).abs(), bw)
                        } else {
                            (start, p0.min(p1), bw, (p1 - p0).abs())
                        };
                        let fl = if g.vary && g.series.len() == 1 { point_fill(s, i, palette, g, k + si) } else {
                            s.points.iter().find(|p| p.0 == i).and_then(|p| p.1.clone()).unwrap_or_else(|| base_fill.clone())
                        };
                        let fa = fill_attr(svg, &fl, x, y, ww, hh);
                        let stroke = s.line.as_ref().and_then(|l| fill::stroke_attrs(svg, l, x, y, ww, hh)).unwrap_or_default();
                        svg.body.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {fa}{stroke}/>", f(x), f(y), f(ww.max(0.01)), f(hh.max(0.01))));
                        if s.labels.val {
                            let text_v = format_number(s.vals[i].unwrap_or(0.0), s.labels.format.as_deref().unwrap_or(&s.format));
                            let ls = s.labels.size.unwrap_or(size);
                            let lc = s.labels.color.unwrap_or(if stacked { Color::WHITE } else { color });
                            if horizontal {
                                let (lx, anchor) = if stacked { ((p0 + p1) / 2.0, 'c') } else { (p1.max(p0) + 3.0, 'l') };
                                text::label(svg, &mut r.fonts, fam, &text_v, lx, y + hh / 2.0 + ls * 0.35, ls, lc, false, anchor, 0.0);
                            } else {
                                let ly = if stacked { (p0 + p1) / 2.0 + ls * 0.35 } else { p1.min(p0) - 3.0 };
                                text::label(svg, &mut r.fonts, fam, &text_v, x + ww / 2.0, ly, ls, lc, false, 'c', 0.0);
                            }
                        }
                    }
                }
                bar_index += 1;
            }
            Kind::Line | Kind::Area => {
                let mut prev_top: Vec<(f64, f64)> = Vec::new();
                for (si, s) in g.series.iter().enumerate() {
                    let fl = series_fill(s, palette, k + si, g.kind == Kind::Line);
                    let mut pts: Vec<(f64, f64)> = Vec::new();
                    let mut base_pts: Vec<(f64, f64)> = Vec::new();
                    for i in 0..ncat {
                        let v = s.vals.get(i).copied().flatten();
                        let Some(mut v) = v else { continue };
                        if pct {
                            v /= totals[i];
                        }
                        let (v0, v1) = if stacked {
                            let a = pos_acc[i];
                            pos_acc[i] += v;
                            (a, a + v)
                        } else {
                            (0.0_f64.clamp(vmin, vmax), v)
                        };
                        let x = cat_pos_line(i);
                        pts.push((x, to_val(v1)));
                        base_pts.push((x, to_val(v0)));
                    }
                    if pts.is_empty() {
                        continue;
                    }
                    if g.kind == Kind::Area {
                        let mut d = format!("M{} {}", f(pts[0].0), f(pts[0].1));
                        for p in &pts[1..] {
                            d.push_str(&format!("L{} {}", f(p.0), f(p.1)));
                        }
                        for p in base_pts.iter().rev() {
                            d.push_str(&format!("L{} {}", f(p.0), f(p.1)));
                        }
                        d.push('Z');
                        let fa = fill_attr(svg, &fl, left, top, pw, ph);
                        svg.body.push_str(&format!("<path d=\"{d}\" {fa}/>"));
                        prev_top = pts.clone();
                    } else {
                        let lw = s.line.as_ref().map(|l| l.width).unwrap_or(2.25);
                        let c = fl.first_color().unwrap_or(Color::BLACK);
                        let d = poly(&pts, s.smooth);
                        let dash = s.line.as_ref().map(|l| l.dash.clone()).unwrap_or_default();
                        let dash = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{}\"", dash.iter().map(|d| f(d * lw)).collect::<Vec<_>>().join(" ")) };
                        svg.body.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"{dash}/>", c.css(), f(lw)));
                        draw_markers(svg, s, &pts, c);
                    }
                    if s.labels.val {
                        for (j, p) in pts.iter().enumerate() {
                            let v = s.vals.iter().flatten().nth(j).copied().unwrap_or(0.0);
                            let ls = s.labels.size.unwrap_or(size);
                            text::label(svg, &mut r.fonts, fam, &format_number(v, s.labels.format.as_deref().unwrap_or(&s.format)), p.0, p.1 - 5.0, ls, s.labels.color.unwrap_or(color), false, 'c', 0.0);
                        }
                    }
                }
                let _ = prev_top;
            }
            Kind::Scatter => {
                for (si, s) in g.series.iter().enumerate() {
                    let fl = series_fill(s, palette, k + si, true);
                    let c = fl.first_color().unwrap_or(Color::BLACK);
                    let pts: Vec<(f64, f64)> = s
                        .vals
                        .iter()
                        .enumerate()
                        .filter_map(|(i, v)| {
                            let v = (*v)?;
                            let x = s.xs.get(i).copied().flatten().unwrap_or(i as f64 + 1.0);
                            Some((to_x(x), to_val(v)))
                        })
                        .collect();
                    let has_line = s.line.as_ref().map(|l| !matches!(l.fill, Fill::None)).unwrap_or(false);
                    if has_line && pts.len() > 1 {
                        let lw = s.line.as_ref().map(|l| l.width).unwrap_or(2.0);
                        svg.body.push_str(&format!("<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\"/>", poly(&pts, s.smooth), c.css(), f(lw)));
                    }
                    let mut ms = s.clone();
                    if ms.marker.is_none() {
                        ms.marker = Some(("circle".into(), 5.0, None));
                    }
                    draw_markers(svg, &ms, &pts, c);
                }
            }
            _ => {}
        }
        k += g.series.len();
    }
}

fn poly(pts: &[(f64, f64)], smooth: bool) -> String {
    let mut d = format!("M{} {}", f(pts[0].0), f(pts[0].1));
    if smooth && pts.len() > 2 {
        for i in 0..pts.len() - 1 {
            let p0 = if i == 0 { pts[0] } else { pts[i - 1] };
            let (p1, p2) = (pts[i], pts[i + 1]);
            let p3 = if i + 2 < pts.len() { pts[i + 2] } else { p2 };
            let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
            let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
            d.push_str(&format!("C{} {} {} {} {} {}", f(c1.0), f(c1.1), f(c2.0), f(c2.1), f(p2.0), f(p2.1)));
        }
    } else {
        for p in &pts[1..] {
            d.push_str(&format!("L{} {}", f(p.0), f(p.1)));
        }
    }
    d
}

fn draw_markers(svg: &mut Svg, s: &Series, pts: &[(f64, f64)], c: Color) {
    let Some((sym, size, fl)) = &s.marker else { return };
    if sym == "none" {
        return;
    }
    let r = size / 2.0;
    let fc = fl.as_ref().and_then(|f| f.first_color()).unwrap_or(c);
    for p in pts {
        match sym.as_str() {
            "square" => svg.body.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>", f(p.0 - r), f(p.1 - r), f(r * 2.0), f(r * 2.0), fc.css())),
            "diamond" => svg.body.push_str(&format!(
                "<path d=\"M{} {}L{} {}L{} {}L{} {}Z\" fill=\"{}\"/>",
                f(p.0), f(p.1 - r), f(p.0 + r), f(p.1), f(p.0), f(p.1 + r), f(p.0 - r), f(p.1), fc.css()
            )),
            "triangle" => svg.body.push_str(&format!(
                "<path d=\"M{} {}L{} {}L{} {}Z\" fill=\"{}\"/>",
                f(p.0), f(p.1 - r), f(p.0 + r), f(p.1 + r), f(p.0 - r), f(p.1 + r), fc.css()
            )),
            _ => svg.body.push_str(&format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\"/>", f(p.0), f(p.1), f(r), fc.css())),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_radar(
    r: &mut Renderer<'_, '_>,
    svg: &mut Svg,
    groups: &[Group],
    axes: &[Axis],
    palette: &Palette,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    fam: Option<&str>,
    size: f64,
    color: Color,
) {
    let g = &groups[0];
    let ncat = g.series.iter().map(|s| s.vals.len()).max().unwrap_or(0);
    if ncat < 3 {
        return;
    }
    let hi = g.series.iter().flat_map(|s| s.vals.iter().flatten()).cloned().fold(0.0, f64::max);
    let val_ax = axes.iter().find(|a| a.kind == "valAx");
    let (vmin, vmax, step) = axis_range(0.0, hi, val_ax, 5);
    let cx = (left + right) / 2.0;
    let cy = (top + bottom) / 2.0 + size * 0.5;
    let rad = ((right - left).min(bottom - top) / 2.0 - size * 2.0).max(5.0);
    let ang = |i: usize| -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / ncat as f64;
    let mut t = vmin + step;
    while t <= vmax + 1e-9 {
        let rr = (t - vmin) / (vmax - vmin) * rad;
        let pts: Vec<(f64, f64)> = (0..ncat).map(|i| (cx + rr * ang(i).cos(), cy + rr * ang(i).sin())).collect();
        let mut d = poly(&pts, false);
        d.push('Z');
        svg.body.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"#d9d9d9\" stroke-width=\"0.75\"/>"));
        t += step;
    }
    let cats = g.series.first().map(|s| s.cats.clone()).unwrap_or_default();
    for i in 0..ncat {
        let (x, y) = (cx + rad * ang(i).cos(), cy + rad * ang(i).sin());
        svg.body.push_str(&format!("<path d=\"M{} {}L{} {}\" stroke=\"#d9d9d9\" stroke-width=\"0.75\"/>", f(cx), f(cy), f(x), f(y)));
        if let Some(c) = cats.get(i) {
            let (lx, ly) = (cx + (rad + size) * ang(i).cos(), cy + (rad + size) * ang(i).sin());
            text::label(svg, &mut r.fonts, fam, c, lx, ly + size * 0.35, size, color, false, 'c', 0.0);
        }
    }
    for (si, s) in g.series.iter().enumerate() {
        let fl = series_fill(s, palette, si, !g.radar_filled);
        let c = fl.first_color().unwrap_or(Color::BLACK);
        let pts: Vec<(f64, f64)> = (0..ncat)
            .map(|i| {
                let v = s.vals.get(i).copied().flatten().unwrap_or(0.0);
                let rr = (v - vmin) / (vmax - vmin) * rad;
                (cx + rr * ang(i).cos(), cy + rr * ang(i).sin())
            })
            .collect();
        let mut d = poly(&pts, false);
        d.push('Z');
        if g.radar_filled {
            svg.body.push_str(&format!("<path d=\"{d}\" fill=\"{}\" fill-opacity=\"0.6\"/>", c.css()));
        } else {
            svg.body.push_str(&format!("<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\"/>", c.css()));
            draw_markers(svg, s, &pts, c);
        }
    }
}
