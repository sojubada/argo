use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{BufReader, Read, Seek};
use zip::ZipArchive;

/// RGBA 색상 (0xRRGGBBAA)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };

    pub fn from_hex(hex: &str) -> Option<Self> {
        let s = hex.trim().trim_start_matches('#');
        // 왜: 엑셀 ARGB 8자리에서 앞 2자리 알파가 00으로 잘못 적힌 문서도 엑셀은 불투명으로 그린다.
        let rgb_str = if s.len() == 8 { &s[2..] } else if s.len() == 6 { s } else { return None };
        let val = u32::from_str_radix(rgb_str, 16).ok()?;
        Some(Self {
            r: ((val >> 16) & 0xFF) as u8,
            g: ((val >> 8) & 0xFF) as u8,
            b: (val & 0xFF) as u8,
            a: 255,
        })
    }
}

/// 엑셀 기본 64색 인덱스 팔레트 (0~63)
pub fn indexed_color(index: u32) -> Option<Rgba> {
    const TABLE: [&str; 64] = [
        "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF",
        "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF",
        "800000", "008000", "000080", "808000", "800080", "008080", "C0C0C0", "808080",
        "9999FF", "993366", "FFFFCC", "CCFFFF", "660066", "FF8080", "0066CC", "CCCCFF",
        "000080", "FF00FF", "FFFF00", "00FFFF", "800080", "800000", "008080", "0000FF",
        "00CCFF", "CCFFFF", "CCFFCC", "FFFF99", "99CCFF", "FF99CC", "CC99FF", "FFCC99",
        "3366FF", "33CCCC", "99CC00", "FFCC00", "FF9900", "FF6600", "666699", "969696",
        "003366", "339966", "003300", "333300", "993300", "993366", "333399", "333333",
    ];
    match index {
        0..=63 => Rgba::from_hex(TABLE[index as usize]),
        64 => Some(Rgba::BLACK), // 시스템 전경
        65 => Some(Rgba::WHITE), // 시스템 배경
        _ => None,
    }
}

/// minipdf/src/xlsx.rs 에서 옮긴 HLS 기반 밝기 보정(tint) 계산 함수.
/// 왜: 엑셀의 테마 색 tint는 RGB 단순 가감이 아니라 HLS 색공간의 Luminance(L) 채널만 선형 보간한다.
pub fn apply_color_tint(color: Rgba, tint: f64) -> Rgba {
    if tint.abs() < 1e-9 {
        return color;
    }
    let (h, l, s) = rgb_to_hls(color.r, color.g, color.b);
    let t = tint.clamp(-1.0, 1.0);
    let new_l = if t < 0.0 {
        l * (1.0 + t)
    } else {
        l * (1.0 - t) + t
    };
    let (r, g, b) = hls_to_rgb(h, new_l.clamp(0.0, 1.0), s);
    Rgba { r, g, b, a: color.a }
}

fn rgb_to_hls(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let rf = r as f64 / 255.0;
    let gf = g as f64 / 255.0;
    let bf = b as f64 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) * 0.5;

    if (max - min).abs() < 1e-12 {
        return (0.0, l, 0.0);
    }

    let d = max - min;
    let s = if l <= 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };

    let mut h = if (max - rf).abs() < 1e-12 {
        (gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }
    } else if (max - gf).abs() < 1e-12 {
        (bf - rf) / d + 2.0
    } else {
        (rf - gf) / d + 4.0
    };
    h /= 6.0;
    (h, l, s)
}

fn hls_to_rgb(h: f64, l: f64, s: f64) -> (u8, u8, u8) {
    if s.abs() < 1e-12 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 0.5 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let r = (hue_to_rgb(p, q, h + 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = (hue_to_rgb(p, q, h) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = (hue_to_rgb(p, q, h - 1.0 / 3.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    (r, g, b)
}

/// minipdf/src/xlsx.rs 에서 옮긴 열 너비(문자 단위) -> 엑셀 pt 환산 함수.
/// 왜: 엑셀의 열 너비는 기본 글꼴 최대 숫자 폭(MDW, 96dpi 픽셀)을 기준으로 픽셀로 먼저 환산한 뒤
/// 0.75를 곱해 pt로 바꾼다. 임의 상수를 곱하면 그림/차트 앵커와 셀 경계가 어긋난다.
pub fn excel_column_width_to_points_with_mdw(width_chars: f32, mdw_px: f32) -> f32 {
    if width_chars <= 0.0 || mdw_px <= 0.0 {
        return 0.0;
    }
    let px = (((256.0 * width_chars + (128.0 / mdw_px).trunc()) / 256.0) * mdw_px).trunc();
    px * 0.75
}

/// Normal 스타일 글꼴 크기(pt)로부터 기본 최대 숫자 폭(MDW, 96dpi px)을 산출한다.
pub fn excel_max_digit_width(font_name: &str, font_size_pt: f32) -> f32 {
    // MDW 는 Normal 글꼴로 '0' 을 그렸을 때의 픽셀 폭(96dpi)이다. 엑셀이 열 너비를
    // 픽셀로 바꿀 때 쓰는 값이라 1px 만 틀려도 열 폭이 어긋난다.
    // 아래 값은 널리 알려진 실측값이다. 모르는 글꼴·크기는 11pt 7px 비례로 본다.
    let lower = font_name.to_ascii_lowercase();
    let known = [
        ("calibri", 11.0, 7.0),
        ("맑은 고딕", 11.0, 7.0),
        ("malgun gothic", 11.0, 7.0),
        ("arial", 10.0, 7.0),
        ("verdana", 10.0, 7.55),
    ];
    for (name, size, mdw) in known {
        if lower.contains(name) && (font_size_pt - size).abs() < 0.01 {
            return mdw;
        }
    }
    (font_size_pt * 7.0 / 11.0).round().max(5.0)
}

/// 기본 열 너비(문자 단위)가 명시되지 않았을 때 baseColWidth(기본 8)로부터 계산한다.
pub fn default_column_width_chars(base_col_width: f32, mdw_px: f32) -> f32 {
    (((base_col_width * mdw_px + 5.0) / mdw_px) * 256.0).trunc() / 256.0
}

/// minipdf/src/xlsx.rs 에서 옮긴 테마 색상 파서.
/// 왜: theme1.xml의 clrScheme은 dk1, lt1, dk2, lt2 순서로 적혀 있으나,
/// styles.xml의 theme="0"은 lt1, "1"은 dk1, "2"는 lt2, "3"은 dk2를 가리키므로 앞 두 쌍을 맞바꿔야 한다.
pub fn read_theme_colors<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    theme_path: &str,
) -> Vec<Rgba> {
    let default_palette = vec![
        Rgba::from_hex("FFFFFF").unwrap(), // 0: lt1
        Rgba::from_hex("000000").unwrap(), // 1: dk1
        Rgba::from_hex("E7E6E6").unwrap(), // 2: lt2
        Rgba::from_hex("44546A").unwrap(), // 3: dk2
        Rgba::from_hex("4472C4").unwrap(), // 4: accent1
        Rgba::from_hex("ED7D31").unwrap(), // 5: accent2
        Rgba::from_hex("A5A5A5").unwrap(), // 6: accent3
        Rgba::from_hex("FFC000").unwrap(), // 7: accent4
        Rgba::from_hex("5B9BD5").unwrap(), // 8: accent5
        Rgba::from_hex("70AD47").unwrap(), // 9: accent6
        Rgba::from_hex("0563C1").unwrap(), // 10: hlink
        Rgba::from_hex("954F72").unwrap(), // 11: folHlink
    ];

    let Ok(file) = archive.by_name(theme_path) else {
        return default_palette;
    };

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(1024);

    let mut in_clr_scheme = false;
    let mut current_slot: Option<usize> = None;
    let mut palette = default_palette;

    // 주의: theme 번호 매핑 (lt1=0, dk1=1, lt2=2, dk2=3)
    fn slot_index(tag: &[u8]) -> Option<usize> {
        match tag {
            b"lt1" => Some(0),
            b"dk1" => Some(1),
            b"lt2" => Some(2),
            b"dk2" => Some(3),
            b"accent1" => Some(4),
            b"accent2" => Some(5),
            b"accent3" => Some(6),
            b"accent4" => Some(7),
            b"accent5" => Some(8),
            b"accent6" => Some(9),
            b"hlink" => Some(10),
            b"folHlink" => Some(11),
            _ => None,
        }
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                if local.as_ref() == b"clrScheme" {
                    in_clr_scheme = true;
                } else if in_clr_scheme {
                    if let Some(idx) = slot_index(local.as_ref()) {
                        current_slot = Some(idx);
                    } else if let Some(slot) = current_slot {
                        extract_theme_rgb(&e, &mut palette[slot]);
                    }
                }
            }
            Ok(Event::Empty(e)) if in_clr_scheme => {
                if let Some(slot) = current_slot {
                    extract_theme_rgb(&e, &mut palette[slot]);
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                if local.as_ref() == b"clrScheme" {
                    break;
                } else if slot_index(local.as_ref()).is_some() {
                    current_slot = None;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    palette
}

fn extract_theme_rgb(e: &quick_xml::events::BytesStart, out: &mut Rgba) {
    let local = e.local_name();
    if local.as_ref() == b"srgbClr" {
        for attr in e.attributes().flatten() {
            if attr.key.local_name().as_ref() == b"val" {
                if let Some(c) = Rgba::from_hex(&attr.unescape_value().unwrap_or_default()) {
                    *out = c;
                }
            }
        }
    } else if local.as_ref() == b"sysClr" {
        for attr in e.attributes().flatten() {
            if attr.key.local_name().as_ref() == b"lastClr" {
                if let Some(c) = Rgba::from_hex(&attr.unescape_value().unwrap_or_default()) {
                    *out = c;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ColorSpec {
    pub rgb: Option<Rgba>,
    pub theme: Option<u32>,
    pub indexed: Option<u32>,
    pub tint: f64,
    pub auto: bool,
}

impl ColorSpec {
    pub fn resolve(&self, theme_colors: &[Rgba], custom_indexed: &[Rgba]) -> Option<Rgba> {
        if let Some(rgb) = self.rgb {
            return Some(apply_color_tint(rgb, self.tint));
        }
        if let Some(t) = self.theme {
            if let Some(&base) = theme_colors.get(t as usize) {
                return Some(apply_color_tint(base, self.tint));
            }
        }
        if let Some(idx) = self.indexed {
            if let Some(&c) = custom_indexed.get(idx as usize) {
                return Some(c);
            }
            return indexed_color(idx);
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct FontStyle {
    pub name: String,
    pub size_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<Rgba>,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self {
            name: "Calibri".to_string(),
            size_pt: 11.0,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FillStyle {
    pub pattern_type: String,
    pub fg_color: Option<Rgba>,
    pub bg_color: Option<Rgba>,
}

#[derive(Debug, Clone, Default)]
pub struct BorderEdge {
    pub style: String,
    pub color: Option<Rgba>,
}

#[derive(Debug, Clone, Default)]
pub struct BorderStyle {
    pub left: BorderEdge,
    pub right: BorderEdge,
    pub top: BorderEdge,
    pub bottom: BorderEdge,
    pub diagonal: BorderEdge,
    pub diagonal_up: bool,
    pub diagonal_down: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AlignmentStyle {
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub wrap_text: bool,
    pub shrink_to_fit: bool,
    pub indent: u32,
    pub text_rotation: i32,
}

#[derive(Debug, Clone, Default)]
pub struct CellXf {
    pub num_fmt_id: u32,
    pub font_id: u32,
    pub fill_id: u32,
    pub border_id: u32,
    pub alignment: AlignmentStyle,
}

#[derive(Debug, Clone, Default)]
pub struct DxfStyle {
    pub font: Option<FontStyle>,
    pub fill: Option<FillStyle>,
    pub border: Option<BorderStyle>,
    pub num_fmt_code: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub num_fmts: HashMap<u32, String>,
    pub fonts: Vec<FontStyle>,
    pub fills: Vec<FillStyle>,
    pub borders: Vec<BorderStyle>,
    pub cell_xfs: Vec<CellXf>,
    pub dxfs: Vec<DxfStyle>,
    pub theme_colors: Vec<Rgba>,
    pub mdw_px: f32,
}

impl StyleSheet {
    pub fn default_with_theme(theme_colors: Vec<Rgba>) -> Self {
        Self {
            num_fmts: HashMap::new(),
            fonts: vec![FontStyle::default()],
            fills: vec![FillStyle::default()],
            borders: vec![BorderStyle::default()],
            cell_xfs: vec![CellXf::default()],
            dxfs: Vec::new(),
            theme_colors,
            mdw_px: 7.0,
        }
    }
}

/// `xl/styles.xml`을 스트리밍으로 읽어 StyleSheet 모델을 만든다.
pub fn parse_styles_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    styles_path: &str,
    theme_colors: Vec<Rgba>,
) -> Result<StyleSheet, String> {
    let Ok(file) = archive.by_name(styles_path) else {
        return Ok(StyleSheet::default_with_theme(theme_colors));
    };

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(2048);

    let mut sheet = StyleSheet::default_with_theme(theme_colors);
    sheet.fonts.clear();
    sheet.fills.clear();
    sheet.borders.clear();
    sheet.cell_xfs.clear();

    let mut custom_indexed: Vec<Rgba> = Vec::new();
    let mut in_cell_xfs = false;
    let mut in_dxfs = false;

    let mut cur_font: Option<FontStyle> = None;
    let mut cur_fill: Option<FillStyle> = None;
    let mut cur_border: Option<BorderStyle> = None;
    let mut cur_edge_name: Option<Vec<u8>> = None;
    let mut cur_xf: Option<CellXf> = None;
    let mut cur_dxf: Option<DxfStyle> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"numFmt" => {
                        if let Some((id, code)) = parse_num_fmt_attrs(&e) {
                            if let Some(dxf) = cur_dxf.as_mut() {
                                dxf.num_fmt_code = Some(code);
                            } else {
                                sheet.num_fmts.insert(id, code);
                            }
                        }
                    }
                    b"font" => cur_font = Some(FontStyle::default()),
                    b"fill" => cur_fill = Some(FillStyle::default()),
                    b"patternFill" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"patternType" {
                                    fill.pattern_type =
                                        attr.unescape_value().unwrap_or_default().into_owned();
                                }
                            }
                        }
                    }
                    b"fgColor" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            let spec = parse_color_attrs(&e);
                            fill.fg_color = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        }
                    }
                    b"bgColor" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            let spec = parse_color_attrs(&e);
                            fill.bg_color = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        }
                    }
                    b"border" => {
                        let mut b = BorderStyle::default();
                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"diagonalUp" => {
                                    let v = attr.unescape_value().unwrap_or_default();
                                    b.diagonal_up = v == "1" || v.eq_ignore_ascii_case("true");
                                }
                                b"diagonalDown" => {
                                    let v = attr.unescape_value().unwrap_or_default();
                                    b.diagonal_down = v == "1" || v.eq_ignore_ascii_case("true");
                                }
                                _ => {}
                            }
                        }
                        cur_border = Some(b);
                    }
                    b"left" | b"right" | b"top" | b"bottom" | b"diagonal" => {
                        if let Some(border) = cur_border.as_mut() {
                            let edge_style = parse_edge_style_attr(&e);
                            let target = match local.as_ref() {
                                b"left" => &mut border.left,
                                b"right" => &mut border.right,
                                b"top" => &mut border.top,
                                b"bottom" => &mut border.bottom,
                                _ => &mut border.diagonal,
                            };
                            target.style = edge_style;
                            cur_edge_name = Some(local.as_ref().to_vec());
                        }
                    }
                    b"color" => {
                        let spec = parse_color_attrs(&e);
                        let resolved = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        if let Some(font) = cur_font.as_mut() {
                            font.color = resolved;
                        } else if let (Some(border), Some(edge_tag)) =
                            (cur_border.as_mut(), cur_edge_name.as_deref())
                        {
                            match edge_tag {
                                b"left" => border.left.color = resolved,
                                b"right" => border.right.color = resolved,
                                b"top" => border.top.color = resolved,
                                b"bottom" => border.bottom.color = resolved,
                                b"diagonal" => border.diagonal.color = resolved,
                                _ => {}
                            }
                        }
                    }
                    b"cellXfs" => in_cell_xfs = true,
                    b"xf" if in_cell_xfs => {
                        cur_xf = Some(parse_xf_attrs(&e));
                    }
                    b"alignment" => {
                        if let Some(xf) = cur_xf.as_mut() {
                            xf.alignment = parse_alignment_attrs(&e);
                        }
                    }
                    b"dxfs" => in_dxfs = true,
                    b"dxf" if in_dxfs => cur_dxf = Some(DxfStyle::default()),
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"numFmt" => {
                        if let Some((id, code)) = parse_num_fmt_attrs(&e) {
                            if let Some(dxf) = cur_dxf.as_mut() {
                                dxf.num_fmt_code = Some(code);
                            } else {
                                sheet.num_fmts.insert(id, code);
                            }
                        }
                    }
                    b"name" | b"rFont" => {
                        if let Some(font) = cur_font.as_mut() {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"val" {
                                    font.name =
                                        attr.unescape_value().unwrap_or_default().into_owned();
                                }
                            }
                        }
                    }
                    b"sz" => {
                        if let Some(font) = cur_font.as_mut() {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"val" {
                                    font.size_pt = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(11.0);
                                }
                            }
                        }
                    }
                    b"b" => {
                        if let Some(font) = cur_font.as_mut() {
                            font.bold = parse_bool_flag_attr(&e);
                        }
                    }
                    b"i" => {
                        if let Some(font) = cur_font.as_mut() {
                            font.italic = parse_bool_flag_attr(&e);
                        }
                    }
                    b"u" => {
                        if let Some(font) = cur_font.as_mut() {
                            font.underline = true;
                        }
                    }
                    b"strike" => {
                        if let Some(font) = cur_font.as_mut() {
                            font.strike = parse_bool_flag_attr(&e);
                        }
                    }
                    b"color" => {
                        let spec = parse_color_attrs(&e);
                        let resolved = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        if let Some(font) = cur_font.as_mut() {
                            font.color = resolved;
                        } else if let (Some(border), Some(edge_tag)) =
                            (cur_border.as_mut(), cur_edge_name.as_deref())
                        {
                            match edge_tag {
                                b"left" => border.left.color = resolved,
                                b"right" => border.right.color = resolved,
                                b"top" => border.top.color = resolved,
                                b"bottom" => border.bottom.color = resolved,
                                b"diagonal" => border.diagonal.color = resolved,
                                _ => {}
                            }
                        }
                    }
                    b"patternFill" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"patternType" {
                                    fill.pattern_type =
                                        attr.unescape_value().unwrap_or_default().into_owned();
                                }
                            }
                        }
                    }
                    b"fgColor" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            let spec = parse_color_attrs(&e);
                            fill.fg_color = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        }
                    }
                    b"bgColor" => {
                        if let Some(fill) = cur_fill.as_mut() {
                            let spec = parse_color_attrs(&e);
                            fill.bg_color = spec.resolve(&sheet.theme_colors, &custom_indexed);
                        }
                    }
                    b"xf" if in_cell_xfs => {
                        sheet.cell_xfs.push(parse_xf_attrs(&e));
                    }
                    b"alignment" => {
                        if let Some(xf) = cur_xf.as_mut() {
                            xf.alignment = parse_alignment_attrs(&e);
                        }
                    }
                    b"rgbColor" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"rgb" {
                                if let Some(c) =
                                    Rgba::from_hex(&attr.unescape_value().unwrap_or_default())
                                {
                                    custom_indexed.push(c);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"font" => {
                        if let Some(f) = cur_font.take() {
                            if let Some(dxf) = cur_dxf.as_mut() {
                                dxf.font = Some(f);
                            } else {
                                sheet.fonts.push(f);
                            }
                        }
                    }
                    b"fill" => {
                        if let Some(f) = cur_fill.take() {
                            if let Some(dxf) = cur_dxf.as_mut() {
                                dxf.fill = Some(f);
                            } else {
                                sheet.fills.push(f);
                            }
                        }
                    }
                    b"left" | b"right" | b"top" | b"bottom" | b"diagonal" => {
                        cur_edge_name = None;
                    }
                    b"border" => {
                        if let Some(b) = cur_border.take() {
                            if let Some(dxf) = cur_dxf.as_mut() {
                                dxf.border = Some(b);
                            } else {
                                sheet.borders.push(b);
                            }
                        }
                    }
                    b"xf" if in_cell_xfs => {
                        if let Some(xf) = cur_xf.take() {
                            sheet.cell_xfs.push(xf);
                        }
                    }
                    b"cellXfs" => in_cell_xfs = false,
                    b"dxf" if in_dxfs => {
                        if let Some(dxf) = cur_dxf.take() {
                            sheet.dxfs.push(dxf);
                        }
                    }
                    b"dxfs" => in_dxfs = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("styles.xml parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    if let Some(f0) = sheet.fonts.first() {
        sheet.mdw_px = excel_max_digit_width(&f0.name, f0.size_pt);
    }
    if sheet.cell_xfs.is_empty() {
        sheet.cell_xfs.push(CellXf::default());
    }

    Ok(sheet)
}

fn parse_num_fmt_attrs(e: &quick_xml::events::BytesStart) -> Option<(u32, String)> {
    let mut id = None;
    let mut code = None;
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"numFmtId" => id = attr.unescape_value().unwrap_or_default().parse::<u32>().ok(),
            b"formatCode" => code = Some(attr.unescape_value().unwrap_or_default().into_owned()),
            _ => {}
        }
    }
    Some((id?, code?))
}

fn parse_bool_flag_attr(e: &quick_xml::events::BytesStart) -> bool {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"val" {
            let v = attr.unescape_value().unwrap_or_default();
            return !(v == "0" || v.eq_ignore_ascii_case("false"));
        }
    }
    true
}

fn parse_color_attrs(e: &quick_xml::events::BytesStart) -> ColorSpec {
    let mut spec = ColorSpec::default();
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"rgb" => spec.rgb = Rgba::from_hex(&attr.unescape_value().unwrap_or_default()),
            b"theme" => spec.theme = attr.unescape_value().unwrap_or_default().parse().ok(),
            b"indexed" => spec.indexed = attr.unescape_value().unwrap_or_default().parse().ok(),
            b"tint" => spec.tint = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0.0),
            b"auto" => {
                let v = attr.unescape_value().unwrap_or_default();
                spec.auto = v == "1" || v.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    spec
}

fn parse_edge_style_attr(e: &quick_xml::events::BytesStart) -> String {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"style" {
            return attr.unescape_value().unwrap_or_default().into_owned();
        }
    }
    String::new()
}

fn parse_xf_attrs(e: &quick_xml::events::BytesStart) -> CellXf {
    let mut xf = CellXf::default();
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"numFmtId" => xf.num_fmt_id = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0),
            b"fontId" => xf.font_id = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0),
            b"fillId" => xf.fill_id = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0),
            b"borderId" => xf.border_id = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0),
            _ => {}
        }
    }
    xf
}

fn parse_alignment_attrs(e: &quick_xml::events::BytesStart) -> AlignmentStyle {
    let mut al = AlignmentStyle::default();
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"horizontal" => {
                al.horizontal = Some(attr.unescape_value().unwrap_or_default().into_owned())
            }
            b"vertical" => {
                al.vertical = Some(attr.unescape_value().unwrap_or_default().into_owned())
            }
            b"wrapText" => {
                let v = attr.unescape_value().unwrap_or_default();
                al.wrap_text = v == "1" || v.eq_ignore_ascii_case("true");
            }
            b"shrinkToFit" => {
                let v = attr.unescape_value().unwrap_or_default();
                al.shrink_to_fit = v == "1" || v.eq_ignore_ascii_case("true");
            }
            b"indent" => {
                al.indent = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0);
            }
            b"textRotation" => {
                al.text_rotation = attr.unescape_value().unwrap_or_default().parse().unwrap_or(0);
            }
            _ => {}
        }
    }
    al
}

