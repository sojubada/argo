//! 표: 격자·병합·표 스타일(머리 행, 줄무늬 …)·칸 테두리·칸 글.

use roxmltree::{Document, Node};

use super::color::{color_child, Color, Palette};
use super::fill::{self, Fill, Line};
use super::geom::{f, Geometry};
use super::render::{child, Renderer, Scope, EMU};
use super::svg::Svg;
use super::text::{self, Body, Levels, RunStyle, TextEnv};

/// 기본 표 스타일(보통 스타일 2 - 강조 N). 파일에 정의가 없을 때 쓴다.
fn builtin(accent: &str) -> String {
    let a = |extra: &str| format!("<a:solidFill><a:schemeClr val=\"{accent}\">{extra}</a:schemeClr></a:solidFill>");
    let ln = |w: u32| format!("<a:ln w=\"{w}\"><a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill></a:ln>");
    format!(
        "<a:tblStyle xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
<a:wholeTbl><a:tcTxStyle><a:fontRef idx=\"minor\"/><a:schemeClr val=\"dk1\"/></a:tcTxStyle><a:tcStyle><a:tcBdr>\
<a:left>{l}</a:left><a:right>{l}</a:right><a:top>{l}</a:top><a:bottom>{l}</a:bottom><a:insideH>{l}</a:insideH><a:insideV>{l}</a:insideV>\
</a:tcBdr><a:fill>{w}</a:fill></a:tcStyle></a:wholeTbl>\
<a:band1H><a:tcStyle><a:tcBdr/><a:fill>{b}</a:fill></a:tcStyle></a:band1H>\
<a:band1V><a:tcStyle><a:tcBdr/><a:fill>{b}</a:fill></a:tcStyle></a:band1V>\
<a:lastCol><a:tcTxStyle b=\"on\"><a:schemeClr val=\"lt1\"/></a:tcTxStyle><a:tcStyle><a:tcBdr/><a:fill>{s}</a:fill></a:tcStyle></a:lastCol>\
<a:firstCol><a:tcTxStyle b=\"on\"><a:schemeClr val=\"lt1\"/></a:tcTxStyle><a:tcStyle><a:tcBdr/><a:fill>{s}</a:fill></a:tcStyle></a:firstCol>\
<a:lastRow><a:tcTxStyle b=\"on\"><a:schemeClr val=\"lt1\"/></a:tcTxStyle><a:tcStyle><a:tcBdr><a:top>{t}</a:top></a:tcBdr><a:fill>{s}</a:fill></a:tcStyle></a:lastRow>\
<a:firstRow><a:tcTxStyle b=\"on\"><a:schemeClr val=\"lt1\"/></a:tcTxStyle><a:tcStyle><a:tcBdr><a:bottom>{t}</a:bottom></a:tcBdr><a:fill>{s}</a:fill></a:tcStyle></a:firstRow>\
</a:tblStyle>",
        l = ln(12700),
        t = ln(38100),
        w = a("<a:tint val=\"20000\"/>"),
        b = a("<a:tint val=\"40000\"/>"),
        s = a("")
    )
}

fn builtin_accent(id: &str) -> &'static str {
    match id.to_uppercase().as_str() {
        "{21E4AEA4-8DFA-4A89-87EB-49C32662AFE8}" => "accent2",
        "{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}" => "accent3",
        "{00A15C55-8517-42AA-B614-E9B94910E393}" => "accent4",
        "{7DF18680-E054-41AD-8BC1-D1AEF772440D}" => "accent5",
        "{93296810-A885-4BE3-A3E7-6D5BEEA58F35}" => "accent6",
        "{073A0DAA-6AF3-43AB-8588-CEC1D06C72B9}" => "dk1",
        _ => "accent1",
    }
}

#[derive(Clone, Default)]
struct CellLook {
    fill: Option<Fill>,
    run: RunStyle,
    left: Option<Line>,
    right: Option<Line>,
    top: Option<Line>,
    bottom: Option<Line>,
}

fn border(n: Option<Node<'_, '_>>, scope: &Scope<'_, '_, '_>) -> Option<Line> {
    let n = n?;
    let mut line = Line::default();
    if let Some(ln) = child(n, "ln") {
        line.fill = Fill::None;
        fill::apply_line(&mut line, ln, scope.palette, None);
        return Some(line);
    }
    if let Some(r) = child(n, "lnRef") {
        let idx = r.attribute("idx").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        let color = fill::ref_color(r, scope.palette);
        if let Some(entry) = fill::style_entry(scope.theme.line_styles, idx) {
            fill::apply_line(&mut line, entry, scope.palette, color);
            return Some(line);
        }
    }
    None
}

/// 표 스타일의 한 부분을 칸 모양에 덮는다. (첫/끝 행·열인지) 로 바깥 변을 고른다.
fn apply_part(look: &mut CellLook, part: Option<Node<'_, '_>>, scope: &Scope<'_, '_, '_>, edges: (bool, bool, bool, bool)) {
    let Some(part) = part else { return };
    if let Some(tx) = child(part, "tcTxStyle") {
        match tx.attribute("b") {
            Some("on") => look.run.bold = Some(true),
            Some("off") => look.run.bold = Some(false),
            _ => {}
        }
        match tx.attribute("i") {
            Some("on") => look.run.italic = Some(true),
            Some("off") => look.run.italic = Some(false),
            _ => {}
        }
        if let Some(c) = color_child(tx, scope.palette, None) {
            look.run.color = Some(Some(c));
        }
        if let Some(fr) = child(tx, "fontRef") {
            if let Some(c) = fill::ref_color(fr, scope.palette) {
                look.run.color = Some(Some(c));
            }
            match fr.attribute("idx") {
                Some("major") => look.run.latin = Some("+mj-lt".into()),
                Some("minor") => look.run.latin = Some("+mn-lt".into()),
                _ => {}
            }
        }
        if let Some(font) = child(tx, "font") {
            if let Some(l) = child(font, "latin").and_then(|l| l.attribute("typeface")) {
                look.run.latin = Some(l.to_owned());
            }
            if let Some(e) = child(font, "ea").and_then(|l| l.attribute("typeface")) {
                look.run.ea = Some(e.to_owned());
            }
        }
    }
    if let Some(st) = child(part, "tcStyle") {
        if let Some(bdr) = child(st, "tcBdr") {
            let (is_left, is_right, is_top, is_bottom) = edges;
            let pick = |outer: &str, inner: &str, at_edge: bool| {
                border(child(bdr, if at_edge { outer } else { inner }), scope)
            };
            if let Some(l) = pick("left", "insideV", is_left) {
                look.left = Some(l);
            }
            if let Some(l) = pick("right", "insideV", is_right) {
                look.right = Some(l);
            }
            if let Some(l) = pick("top", "insideH", is_top) {
                look.top = Some(l);
            }
            if let Some(l) = pick("bottom", "insideH", is_bottom) {
                look.bottom = Some(l);
            }
        }
        if let Some(fl) = child(st, "fill") {
            if let Some(fl) = fill::read_fill(fl, scope.palette, None) {
                look.fill = Some(fl);
            }
        } else if let Some(r) = child(st, "fillRef") {
            let idx = r.attribute("idx").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let color = fill::ref_color(r, scope.palette);
            if let Some(e) = fill::style_entry(scope.theme.fill_styles, idx) {
                look.fill = Some(fill::read_fill_node(e, scope.palette, color));
            }
        }
    }
}

struct Cell<'a, 'i> {
    node: Node<'a, 'i>,
    row: usize,
    col: usize,
    rows: usize,
    cols: usize,
}

pub(crate) fn render<'d, 'i>(r: &mut Renderer<'d, 'i>, svg: &mut Svg, tbl: Node<'d, 'i>, scope: &Scope<'_, 'd, 'i>, _w: f64, _h: f64) {
    let grid: Vec<f64> = child(tbl, "tblGrid")
        .map(|g| {
            g.children()
                .filter(|c| c.has_tag_name("gridCol"))
                .map(|c| c.attribute("w").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / EMU)
                .collect()
        })
        .unwrap_or_default();
    if grid.is_empty() {
        return;
    }
    let trs: Vec<Node> = tbl.children().filter(|c| c.has_tag_name("tr")).collect();
    let mut heights: Vec<f64> =
        trs.iter().map(|tr| tr.attribute("h").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / EMU).collect();
    let nrows = trs.len();
    let ncols = grid.len();

    // 칸 모으기
    let mut cells: Vec<Cell> = Vec::new();
    for (ri, tr) in trs.iter().enumerate() {
        let mut ci = 0;
        for tc in tr.children().filter(|c| c.has_tag_name("tc")) {
            if ci >= ncols {
                break;
            }
            let covered = tc.attribute("hMerge").is_some_and(|v| v == "1" || v == "true")
                || tc.attribute("vMerge").is_some_and(|v| v == "1" || v == "true");
            let span = |a: &str| tc.attribute(a).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
            if !covered {
                cells.push(Cell { node: tc, row: ri, col: ci, rows: span("rowSpan").min(nrows - ri), cols: span("gridSpan").min(ncols - ci) });
            }
            ci += 1;
        }
    }

    // 표 스타일
    let pr = child(tbl, "tblPr");
    let flag = |a: &str| pr.and_then(|p| p.attribute(a)).is_some_and(|v| v == "1" || v == "true");
    let (first_row, first_col, last_row, last_col, band_row, band_col) =
        (flag("firstRow"), flag("firstCol"), flag("lastRow"), flag("lastCol"), flag("bandRow"), flag("bandCol"));
    let style_id = pr.and_then(|p| child(p, "tableStyleId")).and_then(|n| n.text()).map(|s| s.trim().to_owned());
    let builtin_xml;
    let builtin_doc;
    let styles_doc = r.doc("ppt/tableStyles.xml");
    let mut style_node: Option<Node> = None;
    let wanted = style_id.clone().or_else(|| styles_doc.and_then(|d| d.root_element().attribute("def").map(|s| s.to_owned())));
    if let (Some(id), Some(doc)) = (&wanted, styles_doc) {
        style_node = doc
            .descendants()
            .find(|n| n.has_tag_name("tblStyle") && n.attribute("styleId").map(|s| s.eq_ignore_ascii_case(id)).unwrap_or(false));
    }
    if style_node.is_none() {
        if let Some(id) = &wanted {
            if id != "{2D5ABB26-0587-4C30-8999-92F81FD0307C}" {
                builtin_xml = builtin(builtin_accent(id));
                builtin_doc = Document::parse(&builtin_xml).ok();
                style_node = builtin_doc.as_ref().map(|d| d.root_element());
            }
        }
    }
    // 스타일 노드 수명을 맞추기 위해 여기서 한 번 더 묶는다.
    let style_ref = style_node;

    let looks: Vec<CellLook> = cells
        .iter()
        .map(|c| {
            let mut look = CellLook::default();
            let last_r = c.row + c.rows >= nrows;
            let last_c = c.col + c.cols >= ncols;
            let part = |name: &str| style_ref.and_then(|s| child(s, name));
            apply_part(&mut look, part("wholeTbl"), scope, (c.col == 0, last_c, c.row == 0, last_r));
            if band_row {
                let data_row = c.row as i64 - first_row as i64;
                if data_row >= 0 && !(last_row && last_r) {
                    let name = if data_row % 2 == 0 { "band1H" } else { "band2H" };
                    apply_part(&mut look, part(name), scope, (c.col == 0, last_c, true, true));
                }
            }
            if band_col {
                let data_col = c.col as i64 - first_col as i64;
                if data_col >= 0 && !(last_col && last_c) {
                    let name = if data_col % 2 == 0 { "band1V" } else { "band2V" };
                    apply_part(&mut look, part(name), scope, (true, true, c.row == 0, last_r));
                }
            }
            if first_col && c.col == 0 {
                apply_part(&mut look, part("firstCol"), scope, (true, true, c.row == 0, last_r));
            }
            if last_col && last_c {
                apply_part(&mut look, part("lastCol"), scope, (true, true, c.row == 0, last_r));
            }
            if last_row && last_r {
                apply_part(&mut look, part("lastRow"), scope, (c.col == 0, last_c, true, true));
            }
            if first_row && c.row == 0 {
                apply_part(&mut look, part("firstRow"), scope, (c.col == 0, last_c, true, true));
            }
            // 칸 자신의 모양
            if let Some(tcpr) = child(c.node, "tcPr") {
                if let Some(fl) = fill::read_fill(tcpr, scope.palette, None) {
                    look.fill = Some(fl);
                }
                for (name, slot) in [("lnL", 0), ("lnR", 1), ("lnT", 2), ("lnB", 3)] {
                    if let Some(ln) = child(tcpr, name) {
                        let mut line = Line { fill: Fill::None, ..Default::default() };
                        fill::apply_line(&mut line, ln, scope.palette, None);
                        let target = match slot {
                            0 => &mut look.left,
                            1 => &mut look.right,
                            2 => &mut look.top,
                            _ => &mut look.bottom,
                        };
                        *target = Some(line);
                    }
                }
            }
            look
        })
        .collect();

    // 칸 글과 높이
    let mut prepared: Vec<Option<(Levels, Body)>> = Vec::new();
    for (k, c) in cells.iter().enumerate() {
        let tx = child(c.node, "txBody");
        let Some(tx) = tx else {
            prepared.push(None);
            continue;
        };
        let (mut levels, _) = r.text_style(scope, &[], None, true);
        levels.merge_run_all(&looks[k].run);
        levels.merge_list(child(tx, "lstStyle"), scope.palette);
        let mut body = Body { l: 7.2, r: 7.2, t: 3.6, b: 3.6, ..Default::default() };
        if let Some(tcpr) = child(c.node, "tcPr") {
            let g = |a: &str| tcpr.attribute(a).and_then(|v| v.parse::<f64>().ok()).map(|v| v / EMU);
            if let Some(v) = g("marL") {
                body.l = v;
            }
            if let Some(v) = g("marR") {
                body.r = v;
            }
            if let Some(v) = g("marT") {
                body.t = v;
            }
            if let Some(v) = g("marB") {
                body.b = v;
            }
            if let Some(a) = tcpr.attribute("anchor") {
                body.anchor = match a {
                    "ctr" => 'c',
                    "b" => 'b',
                    _ => 't',
                };
            }
            if let Some(v) = tcpr.attribute("vert") {
                body.vert = v.to_owned();
            }
        }
        if c.rows == 1 && text::has_text(tx) {
            let w: f64 = grid[c.col..c.col + c.cols].iter().sum();
            let mut env = TextEnv { fonts: &mut r.fonts, theme: &scope.theme.fonts, palette: scope.palette, slide_no: scope.slide_no };
            let need = text::measure(&mut env, tx, &levels, &body, w);
            if need > heights[c.row] {
                heights[c.row] = need;
            }
        }
        prepared.push(Some((levels, body)));
    }

    let col_x: Vec<f64> = std::iter::once(0.0).chain(grid.iter().scan(0.0, |s, w| {
        *s += w;
        Some(*s)
    })).collect();
    let row_y: Vec<f64> = std::iter::once(0.0).chain(heights.iter().scan(0.0, |s, h| {
        *s += h;
        Some(*s)
    })).collect();

    let rect = |c: &Cell| {
        let x = col_x[c.col];
        let y = row_y[c.row];
        (x, y, col_x[c.col + c.cols] - x, row_y[c.row + c.rows] - y)
    };

    // 채우기
    for (k, c) in cells.iter().enumerate() {
        let (x, y, w, h) = rect(c);
        if let Some(fl) = &looks[k].fill {
            svg.body.push_str(&format!("<g transform=\"translate({} {})\">", f(x), f(y)));
            r.draw_fill(svg, fl, &Geometry::rect(w, h), w, h, scope);
            svg.body.push_str("</g>");
        }
    }
    // 글
    for (k, c) in cells.iter().enumerate() {
        let (x, y, w, h) = rect(c);
        if let (Some(tx), Some((levels, body))) = (child(c.node, "txBody"), &prepared[k]) {
            if text::has_text(tx) {
                let mut env = TextEnv { fonts: &mut r.fonts, theme: &scope.theme.fonts, palette: scope.palette, slide_no: scope.slide_no };
                text::render(svg, &mut env, tx, levels, body, x, y, w, h);
            }
        }
    }
    // 테두리
    for (k, c) in cells.iter().enumerate() {
        let (x, y, w, h) = rect(c);
        let look = &looks[k];
        for (line, x1, y1, x2, y2) in [
            (&look.top, x, y, x + w, y),
            (&look.bottom, x, y + h, x + w, y + h),
            (&look.left, x, y, x, y + h),
            (&look.right, x + w, y, x + w, y + h),
        ] {
            if let Some(line) = line {
                if let Some(attrs) = fill::stroke_attrs(svg, line, x, y, w.max(1.0), h.max(1.0)) {
                    svg.body.push_str(&format!(
                        "<path d=\"M{} {}L{} {}\" fill=\"none\"{attrs}/>",
                        f(x1), f(y1), f(x2), f(y2)
                    ));
                }
            }
        }
    }
    let _ = Color::BLACK;
    let _: Option<&Palette> = None;
}
