//! DrawingML 색: 테마 색, 색 바꿈(clrMap), 변형(lumMod·tint·alpha …).

use std::collections::HashMap;

use roxmltree::Node;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }

    pub fn hex(s: &str) -> Option<Color> {
        let s = s.trim().trim_start_matches('#');
        if s.len() < 6 {
            return None;
        }
        let v = u32::from_str_radix(&s[..6], 16).ok()?;
        Some(Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    /// SVG 색 문자열 (투명도는 따로).
    pub fn css(&self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }

    pub fn with_alpha(mut self, a: f32) -> Color {
        self.a = a;
        self
    }

    pub fn mix(self, other: Color, t: f32) -> Color {
        Color {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn luminance(&self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }
}

/// 테마 색표와 이 쪽에 걸린 색 바꿈.
#[derive(Clone, Default)]
pub(crate) struct Palette {
    /// dk1, lt1, accent1 … (테마 이름)
    pub scheme: HashMap<String, Color>,
    /// bg1 → lt1, tx1 → dk1 …
    pub map: HashMap<String, String>,
}

impl Palette {
    pub fn scheme_color(&self, name: &str) -> Option<Color> {
        let key = self.map.get(name).map(|s| s.as_str()).unwrap_or(match name {
            "bg1" => "lt1",
            "tx1" => "dk1",
            "bg2" => "lt2",
            "tx2" => "dk2",
            other => other,
        });
        self.scheme.get(key).copied().or_else(|| self.scheme.get(name).copied())
    }

    pub fn read_map(node: Node<'_, '_>) -> HashMap<String, String> {
        node.attributes().map(|a| (a.name().to_owned(), a.value().to_owned())).collect()
    }
}

fn val(node: Node<'_, '_>) -> f32 {
    node.attribute("val").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100000.0
}

/// 색을 담은 요소(srgbClr, schemeClr …)를 읽는다. `ph` 는 스타일 참조의 phClr.
pub(crate) fn read_color(node: Node<'_, '_>, palette: &Palette, ph: Option<Color>) -> Option<Color> {
    let base = match node.tag_name().name() {
        "srgbClr" => Color::hex(node.attribute("val")?)?,
        "schemeClr" => {
            let name = node.attribute("val")?;
            if name == "phClr" {
                ph.unwrap_or(Color::BLACK)
            } else {
                palette.scheme_color(name)?
            }
        }
        "sysClr" => node
            .attribute("lastClr")
            .and_then(Color::hex)
            .or_else(|| match node.attribute("val") {
                Some("window") | Some("btnHighlight") | Some("3dLight") => Some(Color::WHITE),
                _ => Some(Color::BLACK),
            })?,
        "prstClr" => preset(node.attribute("val")?)?,
        "scrgbClr" => {
            let p = |n: &str| node.attribute(n).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100000.0;
            Color { r: to_srgb(p("r")), g: to_srgb(p("g")), b: to_srgb(p("b")), a: 1.0 }
        }
        "hslClr" => {
            let h = node.attribute("hue").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 60000.0;
            let s = node.attribute("sat").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100000.0;
            let l = node.attribute("lum").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100000.0;
            from_hsl(h, s, l, 1.0)
        }
        _ => return None,
    };
    Some(apply_mods(base, node))
}

/// 색 요소를 자식으로 둔 요소(solidFill 등)에서 색 하나.
pub(crate) fn color_child(parent: Node<'_, '_>, palette: &Palette, ph: Option<Color>) -> Option<Color> {
    parent.children().filter(|c| c.is_element()).find_map(|c| read_color(c, palette, ph))
}

fn apply_mods(mut c: Color, node: Node<'_, '_>) -> Color {
    for m in node.children().filter(|n| n.is_element()) {
        let v = val(m);
        match m.tag_name().name() {
            "alpha" => c.a = v,
            "alphaMod" => c.a *= v,
            "alphaOff" => c.a += v,
            "lumMod" | "lumOff" | "satMod" | "sat" | "satOff" | "hueMod" | "hue" | "hueOff" | "lum" => {
                let (mut h, mut s, mut l) = to_hsl(c);
                match m.tag_name().name() {
                    "lumMod" => l *= v,
                    "lumOff" => l += v,
                    "lum" => l = v,
                    "satMod" => s *= v,
                    "sat" => s = v,
                    "satOff" => s += v,
                    "hueMod" => h *= v,
                    "hue" => h = v * 100000.0 / 60000.0,
                    "hueOff" => h += v * 100000.0 / 60000.0,
                    _ => {}
                }
                c = from_hsl(h.rem_euclid(360.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0), c.a);
            }
            "tint" => {
                let f = |x: f32| to_srgb(to_linear(x) * v + (1.0 - v));
                c = Color { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a };
            }
            "shade" => {
                let f = |x: f32| to_srgb(to_linear(x) * v);
                c = Color { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a };
            }
            "comp" => {
                let (h, s, l) = to_hsl(c);
                c = from_hsl((h + 180.0).rem_euclid(360.0), s, l, c.a);
            }
            "inv" => c = Color { r: 1.0 - c.r, g: 1.0 - c.g, b: 1.0 - c.b, a: c.a },
            "gray" => {
                let y = c.luminance();
                c = Color { r: y, g: y, b: y, a: c.a };
            }
            "red" => c.r = to_srgb(v),
            "green" => c.g = to_srgb(v),
            "blue" => c.b = to_srgb(v),
            "redMod" => c.r = to_srgb(to_linear(c.r) * v),
            "greenMod" => c.g = to_srgb(to_linear(c.g) * v),
            "blueMod" => c.b = to_srgb(to_linear(c.b) * v),
            "redOff" => c.r = to_srgb(to_linear(c.r) + v),
            "greenOff" => c.g = to_srgb(to_linear(c.g) + v),
            "blueOff" => c.b = to_srgb(to_linear(c.b) + v),
            _ => {}
        }
    }
    c.r = c.r.clamp(0.0, 1.0);
    c.g = c.g.clamp(0.0, 1.0);
    c.b = c.b.clamp(0.0, 1.0);
    c.a = c.a.clamp(0.0, 1.0);
    c
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_hsl(c: Color) -> (f32, f32, f32) {
    let max = c.r.max(c.g).max(c.b);
    let min = c.r.min(c.g).min(c.b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == c.r {
        ((c.g - c.b) / d).rem_euclid(6.0)
    } else if max == c.g {
        (c.b - c.r) / d + 2.0
    } else {
        (c.r - c.g) / d + 4.0
    };
    (h * 60.0, s, l)
}

fn from_hsl(h: f32, s: f32, l: f32, a: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color { r: r + m, g: g + m, b: b + m, a }
}

fn preset(name: &str) -> Option<Color> {
    let hex = match name {
        "black" => "000000",
        "white" => "FFFFFF",
        "red" => "FF0000",
        "green" => "008000",
        "lime" => "00FF00",
        "blue" => "0000FF",
        "yellow" => "FFFF00",
        "cyan" | "aqua" => "00FFFF",
        "magenta" | "fuchsia" => "FF00FF",
        "gray" | "grey" => "808080",
        "darkGray" | "dkGray" => "A9A9A9",
        "lightGray" | "ltGray" => "D3D3D3",
        "silver" => "C0C0C0",
        "orange" => "FFA500",
        "purple" => "800080",
        "navy" => "000080",
        "maroon" => "800000",
        "olive" => "808000",
        "teal" => "008080",
        "brown" => "A52A2A",
        "pink" => "FFC0CB",
        "gold" => "FFD700",
        "darkBlue" | "dkBlue" => "00008B",
        "darkRed" | "dkRed" => "8B0000",
        "darkGreen" | "dkGreen" => "006400",
        "lightBlue" | "ltBlue" => "ADD8E6",
        "skyBlue" => "87CEEB",
        "violet" => "EE82EE",
        "indigo" => "4B0082",
        "coral" => "FF7F50",
        "salmon" => "FA8072",
        "tan" => "D2B48C",
        "beige" => "F5F5DC",
        "ivory" => "FFFFF0",
        _ => return None,
    };
    Color::hex(hex)
}
