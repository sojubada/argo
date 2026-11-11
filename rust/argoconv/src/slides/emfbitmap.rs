//! 그림(비트맵)이 주인공인 EMF 를 직접 그린다.
//!
//! 오피스에서 차트·표를 "그림으로 붙여넣기" 하면 EMF 안에 비트맵 하나
//! (EMR_STRETCHDIBITS / EMR_BITBLT)와 그 위의 간단한 도형 몇 개가 담긴다.
//! VISION2030 14쪽 차트가 그랬는데, 범용 변환기(rhwp)를 거치면 비트맵이
//! 빠지고 위에 얹힌 도형의 테두리만 남았다.
//!
//! 여기서는 그런 파일만 맡는다. 상태 기록(DC 저장·복원, 월드 변환, 자르기,
//! 펜·붓)과 도형(다각형·선·사각형·타원)·비트맵만 읽고, 그 밖의 그리기
//! 기록(글자, 경로, 곡선, 맵 모드 등)이 하나라도 있거나 비트맵이 없으면
//! None 을 돌려 예전 변환기에 맡긴다 — 이미 잘 나오는 로고·아이콘은 그대로다.

use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Clone, Copy)]
struct Pen {
    color: u32,
    width: f32,
    null: bool,
}

#[derive(Clone, Copy)]
struct Brush {
    color: u32,
    null: bool,
}

#[derive(Clone)]
struct Font {
    /// 논리 단위 글자 높이(음수면 글자 크기, 양수면 칸 높이). 절댓값을 쓴다.
    height: f32,
    weight: u32,
    italic: bool,
    face: String,
}

#[derive(Clone)]
enum Obj {
    Pen(Pen),
    Brush(Brush),
    Font(Font),
    Other,
}

#[derive(Clone)]
struct Dc {
    /// GDI 행벡터 규약: x' = m11 x + m21 y + dx, y' = m12 x + m22 y + dy
    world: [f32; 6],
    clip: Option<(f32, f32, f32, f32)>,
    pen: Pen,
    brush: Brush,
    winding: bool,
    /// 창(논리) 원점·크기와 뷰포트(장치) 원점·크기. 글자마다 원점을 옮기는 EMF 가 많다.
    win_org: (f32, f32),
    win_ext: (f32, f32),
    vp_org: (f32, f32),
    vp_ext: (f32, f32),
    font: Option<Font>,
    text_color: u32,
    text_align: u32,
}

impl Dc {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let m = &self.world;
        let (wx, wy) = (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        // 논리 좌표 → 장치 좌표(창·뷰포트). 둘 다 안 쓰면 그대로다.
        let sx = if self.win_ext.0 != 0.0 { self.vp_ext.0 / self.win_ext.0 } else { 1.0 };
        let sy = if self.win_ext.1 != 0.0 { self.vp_ext.1 / self.win_ext.1 } else { 1.0 };
        ((wx - self.win_org.0) * sx + self.vp_org.0, (wy - self.win_org.1) * sy + self.vp_org.1)
    }
    fn scale(&self) -> f32 {
        let m = &self.world;
        let w = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
        let sx = if self.win_ext.0 != 0.0 { (self.vp_ext.0 / self.win_ext.0).abs() } else { 1.0 };
        let sy = if self.win_ext.1 != 0.0 { (self.vp_ext.1 / self.win_ext.1).abs() } else { 1.0 };
        (w * (sx * sy).sqrt()).max(1e-6)
    }
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn i32_at(b: &[u8], o: usize) -> Option<i32> {
    u32_at(b, o).map(|v| v as i32)
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]]))
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

/// COLORREF(0x00BBGGRR) → "#RRGGBB"
/// XML 1.0 이 허용하는 글자인가. 제어문자가 하나라도 들어가면 SVG 를 통째로 못 읽는다.
fn xml_ok(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' '..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..)
}

/// 속성에 넣을 수 없는 글자를 뺀다(글꼴 이름용).
fn xml_attr(v: &str) -> String {
    v.chars().filter(|c| xml_ok(*c) && *c != '"' && *c != '<' && *c != '>' && *c != '&').collect()
}

fn css(c: u32) -> String {
    format!("#{:02X}{:02X}{:02X}", c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF)
}

fn mul(p: &[f32; 6], q: &[f32; 6]) -> [f32; 6] {
    [
        p[0] * q[0] + p[1] * q[2],
        p[0] * q[1] + p[1] * q[3],
        p[2] * q[0] + p[3] * q[2],
        p[2] * q[1] + p[3] * q[3],
        p[4] * q[0] + p[5] * q[2] + q[4],
        p[4] * q[1] + p[5] * q[3] + q[5],
    ]
}

fn stock(index: u32) -> Option<Obj> {
    let brush = |color, null| Some(Obj::Brush(Brush { color, null }));
    let pen = |color, null| Some(Obj::Pen(Pen { color, width: 0.0, null }));
    match index {
        0 => brush(0x00FF_FFFF, false),
        1 => brush(0x00C0_C0C0, false),
        2 => brush(0x0080_8080, false),
        3 => brush(0x0040_4040, false),
        4 => brush(0, false),
        5 => brush(0, true),
        6 => pen(0x00FF_FFFF, false),
        7 => pen(0, false),
        8 => pen(0, true),
        _ => Some(Obj::Other),
    }
}

/// DIB(BITMAPINFO + 비트) → PNG. 원본 일부(src)만 쓰면 잘라 낸다.
fn dib_png(bmi: &[u8], bits: &[u8], src: (i32, i32, i32, i32)) -> Option<Vec<u8>> {
    let mut bmp = Vec::with_capacity(14 + bmi.len() + bits.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&((14 + bmi.len() + bits.len()) as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&((14 + bmi.len()) as u32).to_le_bytes());
    bmp.extend_from_slice(bmi);
    bmp.extend_from_slice(bits);
    let w = i32_at(bmi, 4)?.unsigned_abs() as u64;
    let h = i32_at(bmi, 8)?.unsigned_abs() as u64;
    if w == 0 || h == 0 || w * h > 40_000_000 {
        return None;
    }
    let mut img = std::panic::catch_unwind(|| image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok())
        .ok()
        .flatten()?;
    let (sx, sy, sw, sh) = src;
    if sw > 0 && sh > 0 && (sx, sy, sw as u32, sh as u32) != (0, 0, img.width(), img.height()) {
        let sx = sx.max(0) as u32;
        let sy = sy.max(0) as u32;
        if sx < img.width() && sy < img.height() {
            let cw = (sw as u32).min(img.width() - sx);
            let ch = (sh as u32).min(img.height() - sy);
            img = img.crop_imm(sx, sy, cw, ch);
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

struct Out {
    body: String,
    clips: HashMap<String, String>,
    defs: String,
    bitmaps: usize,
}

impl Out {
    fn push(&mut self, dc: &Dc, node: &str) {
        match dc.clip {
            Some((l, t, r, b)) if r > l && b > t => {
                let key = format!("{l:.2},{t:.2},{r:.2},{b:.2}");
                let n = self.clips.len();
                let id = self
                    .clips
                    .entry(key)
                    .or_insert_with(|| {
                        let id = format!("ebc{n}");
                        let _ = write!(
                            self.defs,
                            "<clipPath id=\"{id}\"><rect x=\"{l:.2}\" y=\"{t:.2}\" width=\"{:.2}\" height=\"{:.2}\"/></clipPath>",
                            r - l,
                            b - t
                        );
                        id
                    })
                    .clone();
                let _ = write!(self.body, "<g clip-path=\"url(#{id})\">{node}</g>");
            }
            _ => self.body.push_str(node),
        }
    }

    fn shape(&mut self, dc: &Dc, d: &str, closed: bool) {
        let fill = if closed && !dc.brush.null { css(dc.brush.color) } else { "none".into() };
        let (stroke, width) = if dc.pen.null {
            ("none".to_string(), 0.0)
        } else {
            // 굵기 0 은 화면 1픽셀(코스메틱 펜)
            (css(dc.pen.color), (dc.pen.width * dc.scale()).max(1.0))
        };
        if fill == "none" && stroke == "none" {
            return;
        }
        let rule = if dc.winding { "nonzero" } else { "evenodd" };
        let node = format!(
            "<path d=\"{d}\" fill=\"{fill}\" fill-rule=\"{rule}\" stroke=\"{stroke}\" stroke-width=\"{width:.2}\"/>"
        );
        self.push(dc, &node);
    }

    /// 글자 한 줄. 글자 사이 간격(dx)이 있으면 글자마다 x 를 적어 GDI 와 같은 자리에 놓는다.
    fn text(&mut self, dc: &Dc, origin: (f32, f32), chars: &[char], dx: &[f32]) {
        let Some(font) = dc.font.clone() else { return };
        let size = font.height.abs() * dc.scale();
        if size < 0.3 || chars.is_empty() {
            return;
        }
        let (ox, oy) = origin;
        let mut xs = String::new();
        if dx.len() >= chars.len() {
            let mut at = 0.0f32;
            for (i, d) in dx.iter().take(chars.len()).enumerate() {
                if i > 0 {
                    xs.push(' ');
                }
                let _ = write!(xs, "{:.2}", ox + at);
                at += d * dc.scale();
            }
        } else {
            let _ = write!(xs, "{ox:.2}");
        }
        // 글자 맞춤: 가운데(6)·오른쪽(2). 기준선(24)이 아니면 y 는 글자 위쪽이다.
        let anchor = match dc.text_align & 6 {
            2 => " text-anchor=\"end\"",
            6 => " text-anchor=\"middle\"",
            _ => "",
        };
        let y = if dc.text_align & 24 == 24 { oy } else { oy + size * 0.88 };
        let weight = if font.weight >= 600 { " font-weight=\"bold\"" } else { "" };
        let italic = if font.italic { " font-style=\"italic\"" } else { "" };
        let body: String = chars
            .iter()
            .filter(|c| xml_ok(**c))
            .map(|c| match c {
                '&' => "&amp;".to_string(),
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                c => c.to_string(),
            })
            .collect();
        if body.trim().is_empty() {
            return;
        }
        let node = format!(
            "<text x=\"{xs}\" y=\"{y:.2}\" font-family=\"{}\" font-size=\"{size:.2}\" fill=\"{}\"{anchor}{weight}{italic} xml:space=\"preserve\">{body}</text>",
            xml_attr(&font.face),
            css(dc.text_color)
        );
        self.push(dc, &node);
    }

    fn image(&mut self, dc: &Dc, dest: (f32, f32, f32, f32), png: &[u8]) {
        let (x, y, w, h) = dest;
        let (x0, y0) = dc.map(x, y);
        let (x1, y1) = dc.map(x + w, y + h);
        let (l, r) = (x0.min(x1), x0.max(x1));
        let (t, b) = (y0.min(y1), y0.max(y1));
        if r - l < 0.5 || b - t < 0.5 {
            return;
        }
        // 뒤집힌 대상(음수 폭·높이)은 그림도 뒤집는다.
        let flip_x = (x1 < x0) as i32;
        let flip_y = (y1 < y0) as i32;
        let transform = if flip_x == 1 || flip_y == 1 {
            format!(
                " transform=\"translate({:.2} {:.2}) scale({} {}) translate({:.2} {:.2})\"",
                l + (r - l) / 2.0,
                t + (b - t) / 2.0,
                if flip_x == 1 { -1 } else { 1 },
                if flip_y == 1 { -1 } else { 1 },
                -(l + (r - l) / 2.0),
                -(t + (b - t) / 2.0)
            )
        } else {
            String::new()
        };
        let node = format!(
            "<image x=\"{l:.2}\" y=\"{t:.2}\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\"{transform} xlink:href=\"data:image/png;base64,{}\"/>",
            r - l,
            b - t,
            super::svg::base64(png)
        );
        self.push(dc, &node);
        self.bitmaps += 1;
    }
}

fn points16(p: &[u8], at: usize, n: usize, dc: &Dc) -> Option<Vec<(f32, f32)>> {
    (0..n)
        .map(|i| {
            let x = i16_at(p, at + i * 4)? as f32;
            let y = i16_at(p, at + i * 4 + 2)? as f32;
            Some(dc.map(x, y))
        })
        .collect()
}

fn points32(p: &[u8], at: usize, n: usize, dc: &Dc) -> Option<Vec<(f32, f32)>> {
    (0..n)
        .map(|i| {
            let x = i32_at(p, at + i * 8)? as f32;
            let y = i32_at(p, at + i * 8 + 4)? as f32;
            Some(dc.map(x, y))
        })
        .collect()
}

fn path_d(pts: &[(f32, f32)], close: bool) -> String {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let _ = write!(d, "{}{x:.2} {y:.2} ", if i == 0 { "M" } else { "L" });
    }
    if close {
        d.push('Z');
    }
    d
}

/// 그림이 주인공인 EMF 면 SVG, 아니면 None.
pub(crate) fn convert(bytes: &[u8]) -> Option<Vec<u8>> {
    if u32_at(bytes, 0)? != 1 || bytes.get(40..44)? != b" EMF" {
        return None;
    }
    let bl = i32_at(bytes, 8)? as f32;
    let bt = i32_at(bytes, 12)? as f32;
    let br = i32_at(bytes, 16)? as f32;
    let bb = i32_at(bytes, 20)? as f32;
    let fw = (i32_at(bytes, 32)? - i32_at(bytes, 24)?) as f32;
    let fh = (i32_at(bytes, 36)? - i32_at(bytes, 28)?) as f32;
    let (bw, bh) = (br - bl + 1.0, bb - bt + 1.0);
    if bw <= 1.0 || bh <= 1.0 {
        return None;
    }
    // 바깥 크기(pt): 프레임(0.01mm)이 있으면 그것, 없으면 장치 픽셀 96dpi
    let (w_pt, h_pt) = if fw > 0.0 && fh > 0.0 {
        (fw / 100.0 * 72.0 / 25.4, fh / 100.0 * 72.0 / 25.4)
    } else {
        (bw * 0.75, bh * 0.75)
    };

    let mut dc = Dc {
        world: IDENTITY,
        clip: None,
        pen: Pen { color: 0, width: 0.0, null: false },
        brush: Brush { color: 0x00FF_FFFF, null: false },
        winding: false,
        win_org: (0.0, 0.0),
        win_ext: (1.0, 1.0),
        vp_org: (0.0, 0.0),
        vp_ext: (1.0, 1.0),
        font: None,
        text_color: 0,
        text_align: 0,
    };
    let mut stack: Vec<Dc> = Vec::new();
    let mut objects: HashMap<u32, Obj> = HashMap::new();
    let mut out = Out { body: String::new(), clips: HashMap::new(), defs: String::new(), bitmaps: 0 };
    let mut cur = (0.0f32, 0.0f32);

    let mut off = u32_at(bytes, 4)? as usize;
    let mut steps = 0;
    while off + 8 <= bytes.len() {
        steps += 1;
        if steps > 200_000 {
            return None;
        }
        let kind = u32_at(bytes, off)?;
        let size = u32_at(bytes, off + 4)? as usize;
        if size < 8 || off + size > bytes.len() {
            return None;
        }
        let r = &bytes[off..off + size];
        match kind {
            14 => break, // EOF
            // 그리기와 상관없는 상태들
            18 | 20 | 21 | 25 | 26 | 28 | 58 | 70 | 98 | 115 | 13 | 16 => {}
            22 => dc.text_align = u32_at(r, 8)?,
            24 => dc.text_color = u32_at(r, 8)?,
            9 => dc.win_ext = (i32_at(r, 8)? as f32, i32_at(r, 12)? as f32),
            10 => dc.win_org = (i32_at(r, 8)? as f32, i32_at(r, 12)? as f32),
            11 => dc.vp_ext = (i32_at(r, 8)? as f32, i32_at(r, 12)? as f32),
            12 => dc.vp_org = (i32_at(r, 8)? as f32, i32_at(r, 12)? as f32),
            82 => {
                // EXTCREATEFONTINDIRECTW: ih, LogFontW(높이·기울기·굵기·이름)
                let ih = u32_at(r, 8)?;
                let face: String = (0..32)
                    .filter_map(|k| i16_at(r, 40 + k * 2).map(|v| v as u16))
                    .take_while(|v| *v != 0)
                    .filter_map(|v| char::from_u32(v as u32))
                    .collect();
                objects.insert(
                    ih,
                    Obj::Font(Font {
                        height: i32_at(r, 12)? as f32,
                        weight: u32_at(r, 28)?,
                        italic: r.get(32).copied().unwrap_or(0) != 0,
                        face,
                    }),
                );
            }
            19 => dc.winding = u32_at(r, 8)? == 2, // SETPOLYFILLMODE
            17 => {
                // SETMAPMODE: MM_TEXT 만 다룬다
                if u32_at(r, 8)? != 1 {
                    return None;
                }
            }
            33 => stack.push(dc.clone()),
            34 => {
                let rel = i32_at(r, 8)?;
                let idx = if rel < 0 { stack.len() as i64 + rel as i64 } else { rel as i64 - 1 };
                if idx >= 0 && (idx as usize) < stack.len() {
                    dc = stack[idx as usize].clone();
                    stack.truncate(idx as usize);
                }
            }
            35 => {
                dc.world = [f32_at(r, 8)?, f32_at(r, 12)?, f32_at(r, 16)?, f32_at(r, 20)?, f32_at(r, 24)?, f32_at(r, 28)?];
            }
            36 => {
                let x = [f32_at(r, 8)?, f32_at(r, 12)?, f32_at(r, 16)?, f32_at(r, 20)?, f32_at(r, 24)?, f32_at(r, 28)?];
                dc.world = match u32_at(r, 32)? {
                    1 => IDENTITY,
                    2 => mul(&x, &dc.world),
                    3 => mul(&dc.world, &x),
                    _ => x,
                };
            }
            30 => {
                // INTERSECTCLIPRECT. 월드 변환은 걸지 않는다 — GDI+ 가 남긴 폴백은
                // 장치 좌표로 적혀 있다(0.625배 변환 아래 734×339 클립이 그림을
                // 60%로 잘랐다). rhwp 도 같은 규칙이다.
                let (x0, y0) = (i32_at(r, 8)? as f32, i32_at(r, 12)? as f32);
                let (x1, y1) = (i32_at(r, 16)? as f32, i32_at(r, 20)? as f32);
                let c = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
                dc.clip = Some(match dc.clip {
                    Some((l, t, rr, b)) => (l.max(c.0), t.max(c.1), rr.min(c.2), b.min(c.3)),
                    None => c,
                });
            }
            75 => {
                // EXTSELECTCLIPRGN: RGN_COPY(5) + 영역 없음이면 해제, 영역이 있으면 그 테두리(장치 좌표)
                let cb = u32_at(r, 8)? as usize;
                let mode = u32_at(r, 12)?;
                if cb == 0 {
                    if mode == 5 {
                        dc.clip = None;
                    }
                } else if r.len() >= 16 + 32 {
                    let c = (
                        i32_at(r, 16 + 16)? as f32,
                        i32_at(r, 16 + 20)? as f32,
                        i32_at(r, 16 + 24)? as f32,
                        i32_at(r, 16 + 28)? as f32,
                    );
                    dc.clip = match (mode, dc.clip) {
                        (1, Some((l, t, rr, b))) => Some((l.max(c.0), t.max(c.1), rr.min(c.2), b.min(c.3))),
                        (5, _) | (1, None) => Some(c),
                        _ => dc.clip,
                    };
                }
            }
            38 => {
                // CREATEPEN: ih, style, width(x,y), color
                let ih = u32_at(r, 8)?;
                let style = u32_at(r, 12)?;
                objects.insert(
                    ih,
                    Obj::Pen(Pen { color: u32_at(r, 24)?, width: i32_at(r, 16)? as f32, null: style & 0xF == 5 }),
                );
            }
            95 => {
                // EXTCREATEPEN: ih, offBmi, cbBmi, offBits, cbBits, style, width, brushStyle, color
                let ih = u32_at(r, 8)?;
                let style = u32_at(r, 28)?;
                let brush_style = u32_at(r, 36)?;
                objects.insert(
                    ih,
                    Obj::Pen(Pen {
                        color: u32_at(r, 40)?,
                        width: u32_at(r, 32)? as f32,
                        null: style & 0xF == 5 || brush_style == 1,
                    }),
                );
            }
            39 => {
                // CREATEBRUSHINDIRECT: ih, style, color, hatch
                let ih = u32_at(r, 8)?;
                objects.insert(ih, Obj::Brush(Brush { color: u32_at(r, 16)?, null: u32_at(r, 12)? == 1 }));
            }
            37 => {
                let h = u32_at(r, 8)?;
                let obj = if h & 0x8000_0000 != 0 { stock(h & 0x7FFF_FFFF) } else { objects.get(&h).cloned() };
                match obj {
                    Some(Obj::Pen(p)) => dc.pen = p,
                    Some(Obj::Brush(b)) => dc.brush = b,
                    Some(Obj::Font(f)) => dc.font = Some(f),
                    _ => {}
                }
            }
            40 => {
                objects.remove(&u32_at(r, 8)?);
            }
            27 => cur = dc.map(i32_at(r, 8)? as f32, i32_at(r, 12)? as f32),
            54 => {
                let p = dc.map(i32_at(r, 8)? as f32, i32_at(r, 12)? as f32);
                let d = path_d(&[cur, p], false);
                out.shape(&dc, &d, false);
                cur = p;
            }
            86 | 87 => {
                // POLYGON16 / POLYLINE16: bounds(16), count, points
                let n = u32_at(r, 24)? as usize;
                let pts = points16(r, 28, n, &dc)?;
                out.shape(&dc, &path_d(&pts, kind == 86), kind == 86);
            }
            3 | 4 => {
                let n = u32_at(r, 24)? as usize;
                let pts = points32(r, 28, n, &dc)?;
                out.shape(&dc, &path_d(&pts, kind == 3), kind == 3);
            }
            90 | 91 => {
                // POLYPOLYLINE16 / POLYPOLYGON16: bounds, nPolys, total, counts[], points
                let polys = u32_at(r, 24)? as usize;
                let mut at = 32 + polys * 4;
                let mut d = String::new();
                for k in 0..polys {
                    let n = u32_at(r, 32 + k * 4)? as usize;
                    let pts = points16(r, at, n, &dc)?;
                    d.push_str(&path_d(&pts, kind == 91));
                    at += n * 4;
                }
                out.shape(&dc, &d, kind == 91);
            }
            43 | 42 => {
                // RECTANGLE / ELLIPSE
                let (x0, y0) = dc.map(i32_at(r, 8)? as f32, i32_at(r, 12)? as f32);
                let (x1, y1) = dc.map(i32_at(r, 16)? as f32, i32_at(r, 20)? as f32);
                let (l, t, rr, b) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
                let d = if kind == 43 {
                    format!("M{l:.2} {t:.2} L{rr:.2} {t:.2} L{rr:.2} {b:.2} L{l:.2} {b:.2} Z")
                } else {
                    let (cx, cy, rx, ry) = ((l + rr) / 2.0, (t + b) / 2.0, (rr - l) / 2.0, (b - t) / 2.0);
                    format!(
                        "M{:.2} {cy:.2} A{rx:.2} {ry:.2} 0 1 0 {:.2} {cy:.2} A{rx:.2} {ry:.2} 0 1 0 {:.2} {cy:.2} Z",
                        cx - rx,
                        cx + rx,
                        cx - rx
                    )
                };
                out.shape(&dc, &d, true);
            }
            81 => {
                // STRETCHDIBITS
                let (xd, yd) = (i32_at(r, 24)?, i32_at(r, 28)?);
                let src = (i32_at(r, 32)?, i32_at(r, 36)?, i32_at(r, 40)?, i32_at(r, 44)?);
                let (ob, cb, obits, cbits) =
                    (u32_at(r, 48)? as usize, u32_at(r, 52)? as usize, u32_at(r, 56)? as usize, u32_at(r, 60)? as usize);
                let (cx, cy) = (i32_at(r, 72)?, i32_at(r, 76)?);
                let bmi = r.get(ob..ob + cb)?;
                let bits = r.get(obits..obits + cbits)?;
                let png = dib_png(bmi, bits, src)?;
                out.image(&dc, (xd as f32, yd as f32, cx as f32, cy as f32), &png);
            }
            76 => {
                // BITBLT: bounds, xDest, yDest, cx, cy, rop, xSrc, ySrc, xform(24), bk, usage, offBmi, cbBmi, offBits, cbBits
                let (xd, yd, cx, cy) = (i32_at(r, 24)?, i32_at(r, 28)?, i32_at(r, 32)?, i32_at(r, 36)?);
                let rop = u32_at(r, 40)?;
                let (xs, ys) = (i32_at(r, 44)?, i32_at(r, 48)?);
                let (ob, cb, obits, cbits) =
                    (u32_at(r, 84)? as usize, u32_at(r, 88)? as usize, u32_at(r, 92)? as usize, u32_at(r, 96)? as usize);
                if cb == 0 {
                    // 비트맵 없는 블릿 = 붓으로 칠하기. NOP(0x00AA0029)은 아무것도 안 한다.
                    let color = match rop {
                        0x00F0_0021 if !dc.brush.null => Some(dc.brush.color),
                        0x0000_0042 => Some(0),
                        0x00FF_0062 => Some(0x00FF_FFFF),
                        _ => None,
                    };
                    if let Some(c) = color {
                        let (x0, y0) = dc.map(xd as f32, yd as f32);
                        let (x1, y1) = dc.map((xd + cx) as f32, (yd + cy) as f32);
                        let node = format!(
                            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{}\"/>",
                            x0.min(x1),
                            y0.min(y1),
                            (x1 - x0).abs(),
                            (y1 - y0).abs(),
                            css(c)
                        );
                        out.push(&dc, &node);
                    }
                } else {
                    let bmi = r.get(ob..ob + cb)?;
                    let bits = r.get(obits..obits + cbits)?;
                    let png = dib_png(bmi, bits, (xs, ys, cx, cy))?;
                    out.image(&dc, (xd as f32, yd as f32, cx as f32, cy as f32), &png);
                }
            }
            83 | 84 => {
                // EXTTEXTOUTA / EXTTEXTOUTW: bounds(16), 그리기 방식, 배율 2개, EmrText
                let at = 36;
                let (rx, ry) = (i32_at(r, at)? as f32, i32_at(r, at + 4)? as f32);
                let n = u32_at(r, at + 8)? as usize;
                let off_str = u32_at(r, at + 12)? as usize;
                let off_dx = u32_at(r, at + 36)? as usize;
                if n == 0 || n > 8192 {
                    off += size;
                    continue;
                }
                let chars: Vec<char> = if kind == 84 {
                    (0..n)
                        .filter_map(|k| i16_at(r, off_str + k * 2).map(|v| v as u16))
                        .filter_map(|v| char::from_u32(v as u32))
                        .collect()
                } else {
                    (0..n).filter_map(|k| r.get(off_str + k).map(|b| *b as char)).collect()
                };
                let dx: Vec<f32> = if off_dx >= 36 && off_dx + n * 4 <= r.len() {
                    (0..n).filter_map(|k| i32_at(r, off_dx + k * 4).map(|v| v as f32)).collect()
                } else {
                    Vec::new()
                };
                let origin = dc.map(rx, ry);
                out.text(&dc, origin, &chars, &dx);
            }
            // 그 밖의 그리기(경로·곡선·맵 모드…)는 예전 변환기에 맡긴다
            _ => return None,
        }
        off += size;
    }
    // 그린 것이 하나도 없으면 예전 변환기에 맡긴다.
    if out.body.is_empty() {
        return None;
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w_pt:.2}\" height=\"{h_pt:.2}\" viewBox=\"{bl} {bt} {bw} {bh}\" preserveAspectRatio=\"none\"><defs>{}</defs>{}</svg>",
        out.defs, out.body
    );
    Some(svg.into_bytes())
}
