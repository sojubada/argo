use crate::calc::{Value, WorkbookCalcResult};
use crate::cf::read_evaluated_cell_value;
use crate::draw::{
    ASColor, ASDrawCmd, ASRect, DrawList, SheetRenderCache, ALIGN_H_CENTER, ALIGN_H_LEFT,
    ALIGN_H_RIGHT, ALIGN_V_CENTER, LINE_STYLE_DASHED, LINE_STYLE_THIN, TEXT_FLAG_BOLD,
};
use crate::package::{read_part_relationships, resolve_part_target};
use crate::parse::parse_range_1based;
use crate::style::{Rgba, StyleSheet};
use crate::Workbook;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::sync::Arc;
use zip::ZipArchive;

/// 1 엑셀 포인트(pt, 1/72 inch) = 12,700 EMU
pub const EMU_PER_PT: f32 = 12_700.0;

/// ZIP 안 부품 하나를 읽는 상한. 압축을 풀면 수 GB 가 되는 파일(압축 폭탄)로 앱이
/// 메모리 부족으로 죽지 않게 한다. 넘으면 오류로 보고 그 부품은 건너뛴다.
pub const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;

fn read_capped<R: Read>(r: &mut R, out: &mut Vec<u8>) -> std::io::Result<usize> {
    let n = Read::take(r, MAX_PART_BYTES + 1).read_to_end(out)?;
    if n as u64 > MAX_PART_BYTES {
        out.clear();
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "부품이 너무 큼"));
    }
    Ok(n)
}

/// 시트 하나에서 받아들일 그림·차트·도형 수. 넘는 것은 버린다(앵커 수십만 개짜리 악성 파일 대비).
pub const MAX_DRAWINGS_PER_SHEET: usize = 1000;
/// 차트 하나의 계열 수 상한.
pub const MAX_SERIES_PER_CHART: usize = 64;
/// 차트 하나에 그릴 점 총수 상한(계열 전부 합쳐서).
pub const MAX_POINTS_PER_CHART: usize = 200_000;
/// 통합 문서 하나에서 메모리에 올릴 그림 원본 총량. 같은 그림 파일은 한 번만 읽어 함께 쓴다.
pub const MAX_MEDIA_TOTAL_BYTES: u64 = 192 * 1024 * 1024;

/// 그림 원본 캐시. 같은 media 파일을 여러 앵커·시트가 가리키면 하나의 Arc 를 나눠 쓰고,
/// 총량이 `MAX_MEDIA_TOTAL_BYTES` 를 넘으면 더 읽지 않는다.
#[derive(Default)]
pub struct MediaCache {
    map: HashMap<String, Option<Arc<Vec<u8>>>>,
    total: u64,
}

impl MediaCache {
    pub fn get<R: Read + Seek>(&mut self, archive: &mut ZipArchive<R>, path: &str) -> Option<Arc<Vec<u8>>> {
        if let Some(v) = self.map.get(path) {
            return v.clone();
        }
        let mut loaded = None;
        if self.total < MAX_MEDIA_TOTAL_BYTES {
            if let Ok(mut mf) = archive.by_name(path) {
                let mut bytes = Vec::new();
                if read_capped(&mut mf, &mut bytes).is_ok()
                    && !bytes.is_empty()
                    && self.total + bytes.len() as u64 <= MAX_MEDIA_TOTAL_BYTES
                {
                    self.total += bytes.len() as u64;
                    loaded = Some(Arc::new(bytes));
                }
            }
        }
        self.map.insert(path.to_string(), loaded.clone());
        loaded
    }
}

#[inline]
pub fn emu_to_pt(emu: i64) -> f32 {
    (emu as f32) / EMU_PER_PT
}

// ============================================================================
// 1. 앵커(oneCellAnchor / twoCellAnchor / absoluteAnchor) 및 오버레이 모델
// ============================================================================

#[derive(Debug, Clone, Copy, Default)]
pub struct AnchorMarker {
    pub col_0based: u32,
    pub col_off_emu: i64,
    pub row_0based: u32,
    pub row_off_emu: i64,
}

#[derive(Debug, Clone, Copy)]
pub enum CellAnchorSpec {
    TwoCell {
        from: AnchorMarker,
        to: AnchorMarker,
    },
    OneCell {
        from: AnchorMarker,
        ext_cx_emu: i64,
        ext_cy_emu: i64,
    },
    Absolute {
        x_emu: i64,
        y_emu: i64,
        ext_cx_emu: i64,
        ext_cy_emu: i64,
    },
}

impl CellAnchorSpec {
    /// 앵커 좌표를 시트 단일 좌표계(엑셀 pt)로 정밀 환산한다.
    /// 왜: 열·행 안의 EMU 오프셋이 해당 열 너비·행 높이보다 크면 그 열·행 크기로 자르고,
    /// 숨은 행·열(크기 0pt)에 걸친 앵커는 엑셀과 동일하게 0pt로 처리한다.
    pub fn to_sheet_rect_pt(&self, rc: &SheetRenderCache) -> (ASRect, u32, u32) {
        match *self {
            CellAnchorSpec::TwoCell { from, to } => {
                let (x1, y1) = marker_to_pt(from, rc);
                let (x2, y2) = marker_to_pt(to, rc);
                let rect = ASRect {
                    x: x1.min(x2),
                    y: y1.min(y2),
                    w: (x2 - x1).abs().max(1.0),
                    h: (y2 - y1).abs().max(1.0),
                };
                (rect, from.row_0based + 1, from.col_0based + 1)
            }
            CellAnchorSpec::OneCell {
                from,
                ext_cx_emu,
                ext_cy_emu,
            } => {
                let (x1, y1) = marker_to_pt(from, rc);
                let rect = ASRect {
                    x: x1,
                    y: y1,
                    w: emu_to_pt(ext_cx_emu).max(1.0),
                    h: emu_to_pt(ext_cy_emu).max(1.0),
                };
                (rect, from.row_0based + 1, from.col_0based + 1)
            }
            CellAnchorSpec::Absolute {
                x_emu,
                y_emu,
                ext_cx_emu,
                ext_cy_emu,
            } => {
                let rect = ASRect {
                    x: emu_to_pt(x_emu).max(0.0),
                    y: emu_to_pt(y_emu).max(0.0),
                    w: emu_to_pt(ext_cx_emu).max(1.0),
                    h: emu_to_pt(ext_cy_emu).max(1.0),
                };
                (rect, 1, 1)
            }
        }
    }
}

fn marker_to_pt(m: AnchorMarker, rc: &SheetRenderCache) -> (f32, f32) {
    let max_c = rc.col_offsets_pt.len().saturating_sub(1);
    let max_r = rc.row_offsets_pt.len().saturating_sub(1);

    let c_idx = (m.col_0based as usize).min(max_c);
    let base_x = rc.col_offsets_pt.get(c_idx).copied().unwrap_or(0.0);
    let col_w = if c_idx + 1 < rc.col_offsets_pt.len() {
        (rc.col_offsets_pt[c_idx + 1] - base_x).max(0.0)
    } else {
        50.0
    };
    let off_x = if col_w <= 0.0 {
        0.0
    } else {
        emu_to_pt(m.col_off_emu).clamp(0.0, col_w)
    };

    let r_idx = (m.row_0based as usize).min(max_r);
    let base_y = rc.row_offsets_pt.get(r_idx).copied().unwrap_or(0.0);
    let row_h = if r_idx + 1 < rc.row_offsets_pt.len() {
        (rc.row_offsets_pt[r_idx + 1] - base_y).max(0.0)
    } else {
        15.0
    };
    let off_y = if row_h <= 0.0 {
        0.0
    } else {
        emu_to_pt(m.row_off_emu).clamp(0.0, row_h)
    };

    (base_x + off_x, base_y + off_y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartPlotKind {
    BarClustered,
    BarStacked,
    BarPercentStacked,
    Line,
    Area,
    Pie,
    Scatter,
}

#[derive(Debug, Clone)]
pub struct ChartSeriesSpec {
    pub name_formula: Option<String>,
    pub name_cached: Option<String>,
    pub cat_formula: Option<String>,
    pub cat_cached: Vec<String>,
    pub val_formula: Option<String>,
    pub val_cached: Vec<f64>,
    pub color: Option<Rgba>,
    pub plot_kind: ChartPlotKind,
    pub use_secondary_axis: bool,
    pub axis_id: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ChartSpec {
    pub title: String,
    pub series: Vec<ChartSeriesSpec>,
    pub primary_axis_ids: Vec<u32>,
    pub secondary_axis_ids: Vec<u32>,
    pub is_horizontal_bar: bool, // barDir == "bar" 이면 가로 막대, "col" 이면 세로 열 막대
}

#[derive(Debug, Clone)]
pub enum RawDrawingPayload {
    Chart(ChartSpec),
    Image {
        media_bytes: Arc<Vec<u8>>,
        is_emf: bool,
        crop: (f32, f32, f32, f32), // l, t, r, b (0.0 ~ 1.0)
        rotation_deg: f32,
    },
    Shape {
        text: String,
        fill_color: Option<Rgba>,
        stroke_color: Option<Rgba>,
        rotation_deg: f32,
    },
}

#[derive(Debug, Clone)]
pub struct RawSheetDrawing {
    pub anchor: CellAnchorSpec,
    pub payload: RawDrawingPayload,
}

/// C ABI를 통해 Swift `OverlayLayer`에 전달되는 해석 완료 오버레이 객체
pub struct ResolvedOverlay {
    pub kind: u8, // 0=Chart, 1=ImageRaster, 2=ImageRgba(EMF), 3=Shape
    pub frame_pt: ASRect,
    pub start_row: u32,
    pub start_col: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub crop: (f32, f32, f32, f32),
    pub rotation_deg: f32,
    pub title: String,
    pub draw_list: Option<DrawList>,
    pub raw_bytes: Arc<Vec<u8>>,
}

// ============================================================================
// 2. 드로잉(drawingN.xml) · 차트(chartN.xml) · 레거시 VML 파서
// ============================================================================

pub fn parse_sheet_drawings<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    drawing_paths: &[String],
    vml_paths: &[String],
    styles: &StyleSheet,
    media: &mut MediaCache,
) -> Vec<RawSheetDrawing> {
    let mut out = Vec::new();

    for d_path in drawing_paths {
        if out.len() >= MAX_DRAWINGS_PER_SHEET {
            break;
        }
        if let Ok(items) = parse_one_drawing_xml(archive, d_path, styles, media) {
            out.extend(items);
        }
    }

    for v_path in vml_paths {
        if out.len() >= MAX_DRAWINGS_PER_SHEET {
            break;
        }
        if let Ok(vml_imgs) = read_legacy_drawing_images(archive, v_path, media) {
            out.extend(vml_imgs);
        }
    }

    out.truncate(MAX_DRAWINGS_PER_SHEET);
    out
}

fn parse_one_drawing_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    drawing_path: &str,
    styles: &StyleSheet,
    media: &mut MediaCache,
) -> Result<Vec<RawSheetDrawing>, String> {
    let d_rels = read_part_relationships(archive, drawing_path)?;
    let xml_bytes = {
        let mut f = archive
            .by_name(drawing_path)
            .map_err(|e| format!("missing {}: {}", drawing_path, e))?;
        let mut b = Vec::new();
        read_capped(&mut f, &mut b).map_err(|e| e.to_string())?;
        b
    };

    let mut reader = Reader::from_reader(xml_bytes.as_slice());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(2048);

    // 1단계: drawing.xml 스트리밍 스캔으로 앵커와 참조 r:id 목록 수집
    enum PendingKind {
        None,
        ChartRId(String),
        ImageRId {
            r_id: String,
            crop: (f32, f32, f32, f32),
            rot_deg: f32,
        },
        Shape {
            text: String,
            fill: Option<Rgba>,
            stroke: Option<Rgba>,
            rot_deg: f32,
        },
    }

    struct PendingItem {
        anchor: CellAnchorSpec,
        kind: PendingKind,
    }

    let mut pending: Vec<PendingItem> = Vec::new();
    let mut anchor_mode: u8 = 0; // 1=twoCell, 2=oneCell, 3=absolute
    let mut from_mk = AnchorMarker::default();
    let mut to_mk = AnchorMarker::default();
    let mut abs_pos = (0i64, 0i64);
    let mut ext_size = (0i64, 0i64);
    let mut in_from = false;
    let mut in_to = false;
    let mut cur_coord_tag: Vec<u8> = Vec::new();

    let mut cur_chart_rid: Option<String> = None;
    let mut cur_blip_rid: Option<String> = None;
    let mut cur_crop = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut cur_rot_deg = 0.0f32;
    let mut in_sp = false;
    let mut sp_text = String::new();
    let mut in_a_t = false;
    let mut sp_fill: Option<Rgba> = None;
    let mut sp_stroke: Option<Rgba> = None;
    let mut in_ln = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"twoCellAnchor" => {
                        anchor_mode = 1;
                        from_mk = AnchorMarker::default();
                        to_mk = AnchorMarker::default();
                        cur_chart_rid = None;
                        cur_blip_rid = None;
                        cur_crop = (0.0, 0.0, 0.0, 0.0);
                        cur_rot_deg = 0.0;
                        in_sp = false;
                        sp_text.clear();
                        sp_fill = None;
                        sp_stroke = None;
                    }
                    b"oneCellAnchor" => {
                        anchor_mode = 2;
                        from_mk = AnchorMarker::default();
                        ext_size = (1_270_000, 1_270_000);
                        cur_chart_rid = None;
                        cur_blip_rid = None;
                        cur_crop = (0.0, 0.0, 0.0, 0.0);
                        cur_rot_deg = 0.0;
                        in_sp = false;
                        sp_text.clear();
                        sp_fill = None;
                        sp_stroke = None;
                    }
                    b"absoluteAnchor" => {
                        anchor_mode = 3;
                        abs_pos = (0, 0);
                        ext_size = (1_270_000, 1_270_000);
                        cur_chart_rid = None;
                        cur_blip_rid = None;
                        cur_crop = (0.0, 0.0, 0.0, 0.0);
                        cur_rot_deg = 0.0;
                        in_sp = false;
                        sp_text.clear();
                        sp_fill = None;
                        sp_stroke = None;
                    }
                    b"from" => in_from = true,
                    b"to" => in_to = true,
                    b"col" | b"colOff" | b"row" | b"rowOff" if in_from || in_to => {
                        cur_coord_tag = local.as_ref().to_vec();
                    }
                    b"sp" => in_sp = true,
                    b"ln" if in_sp => in_ln = true,
                    b"t" if in_sp => in_a_t = true,
                    b"xfrm" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"rot" {
                                let rot_60k: i64 = attr
                                    .unescape_value()
                                    .unwrap_or_default()
                                    .parse()
                                    .unwrap_or(0);
                                cur_rot_deg = (rot_60k as f32) / 60_000.0;
                            }
                        }
                    }
                    b"chart" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"id" {
                                cur_chart_rid =
                                    Some(attr.unescape_value().unwrap_or_default().into_owned());
                            }
                        }
                    }
                    b"blip" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"embed" {
                                cur_blip_rid =
                                    Some(attr.unescape_value().unwrap_or_default().into_owned());
                            }
                        }
                    }
                    b"srcRect" => {
                        cur_crop = parse_src_rect_crop(&e);
                    }
                    b"srgbClr" if in_sp => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                if let Some(c) =
                                    Rgba::from_hex(&attr.unescape_value().unwrap_or_default())
                                {
                                    if in_ln {
                                        sp_stroke = Some(c);
                                    } else if sp_fill.is_none() {
                                        sp_fill = Some(c);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"pos" if anchor_mode == 3 => {
                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"x" => {
                                    abs_pos.0 = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0)
                                }
                                b"y" => {
                                    abs_pos.1 = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0)
                                }
                                _ => {}
                            }
                        }
                    }
                    b"ext" if anchor_mode == 2 || anchor_mode == 3 => {
                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"cx" => {
                                    ext_size.0 = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0)
                                }
                                b"cy" => {
                                    ext_size.1 = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0)
                                }
                                _ => {}
                            }
                        }
                    }
                    b"chart" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"id" {
                                cur_chart_rid =
                                    Some(attr.unescape_value().unwrap_or_default().into_owned());
                            }
                        }
                    }
                    b"blip" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"embed" {
                                cur_blip_rid =
                                    Some(attr.unescape_value().unwrap_or_default().into_owned());
                            }
                        }
                    }
                    b"srcRect" => {
                        cur_crop = parse_src_rect_crop(&e);
                    }
                    b"srgbClr" if in_sp => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                if let Some(c) =
                                    Rgba::from_hex(&attr.unescape_value().unwrap_or_default())
                                {
                                    if in_ln {
                                        sp_stroke = Some(c);
                                    } else if sp_fill.is_none() {
                                        sp_fill = Some(c);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if (in_from || in_to) && !cur_coord_tag.is_empty() {
                    let v: i64 = e
                        .unescape()
                        .unwrap_or_default()
                        .trim()
                        .parse()
                        .unwrap_or(0);
                    let target = if in_from { &mut from_mk } else { &mut to_mk };
                    match cur_coord_tag.as_slice() {
                        b"col" => target.col_0based = v.max(0) as u32,
                        b"colOff" => target.col_off_emu = v,
                        b"row" => target.row_0based = v.max(0) as u32,
                        b"rowOff" => target.row_off_emu = v,
                        _ => {}
                    }
                } else if in_sp && in_a_t {
                    if let Ok(t) = e.unescape() {
                        if !sp_text.is_empty() {
                            sp_text.push(' ');
                        }
                        sp_text.push_str(&t);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"from" => in_from = false,
                    b"to" => in_to = false,
                    b"col" | b"colOff" | b"row" | b"rowOff" => cur_coord_tag.clear(),
                    b"t" => in_a_t = false,
                    b"ln" => in_ln = false,
                    b"sp" => in_sp = false,
                    b"twoCellAnchor" | b"oneCellAnchor" | b"absoluteAnchor" => {
                        let anchor = match anchor_mode {
                            1 => CellAnchorSpec::TwoCell {
                                from: from_mk,
                                to: to_mk,
                            },
                            2 => CellAnchorSpec::OneCell {
                                from: from_mk,
                                ext_cx_emu: ext_size.0,
                                ext_cy_emu: ext_size.1,
                            },
                            _ => CellAnchorSpec::Absolute {
                                x_emu: abs_pos.0,
                                y_emu: abs_pos.1,
                                ext_cx_emu: ext_size.0,
                                ext_cy_emu: ext_size.1,
                            },
                        };
                        let kind = if let Some(cid) = cur_chart_rid.take() {
                            PendingKind::ChartRId(cid)
                        } else if let Some(bid) = cur_blip_rid.take() {
                            PendingKind::ImageRId {
                                r_id: bid,
                                crop: cur_crop,
                                rot_deg: cur_rot_deg,
                            }
                        } else if !sp_text.is_empty() || sp_fill.is_some() || sp_stroke.is_some() {
                            PendingKind::Shape {
                                text: sp_text.clone(),
                                fill: sp_fill,
                                stroke: sp_stroke,
                                rot_deg: cur_rot_deg,
                            }
                        } else {
                            PendingKind::None
                        };
                        if !matches!(kind, PendingKind::None) {
                            if pending.len() < MAX_DRAWINGS_PER_SHEET {
                                pending.push(PendingItem { anchor, kind });
                            }
                        }
                        anchor_mode = 0;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // 2단계: 수집된 r:id로 실제 차트 XML 및 미디어 이미지 로드
    let mut results = Vec::new();
    for item in pending {
        match item.kind {
            PendingKind::ChartRId(r_id) => {
                if let Some(rel) = d_rels.get(&r_id) {
                    let chart_path = resolve_part_target(drawing_path, &rel.target);
                    if let Ok(chart_spec) = parse_chart_xml(archive, &chart_path, styles) {
                        results.push(RawSheetDrawing {
                            anchor: item.anchor,
                            payload: RawDrawingPayload::Chart(chart_spec),
                        });
                    }
                }
            }
            PendingKind::ImageRId {
                r_id,
                crop,
                rot_deg,
            } => {
                if let Some(rel) = d_rels.get(&r_id) {
                    let media_path = resolve_part_target(drawing_path, &rel.target);
                    {
                        if let Some(bytes) = media.get(archive, &media_path) {
                            let lower = media_path.to_ascii_lowercase();
                            let is_emf = lower.ends_with(".emf") || lower.ends_with(".wmf");
                            results.push(RawSheetDrawing {
                                anchor: item.anchor,
                                payload: RawDrawingPayload::Image {
                                    media_bytes: bytes,
                                    is_emf,
                                    crop,
                                    rotation_deg: rot_deg,
                                },
                            });
                        }
                    }
                }
            }
            PendingKind::Shape {
                text,
                fill,
                stroke,
                rot_deg,
            } => {
                results.push(RawSheetDrawing {
                    anchor: item.anchor,
                    payload: RawDrawingPayload::Shape {
                        text,
                        fill_color: fill,
                        stroke_color: stroke,
                        rotation_deg: rot_deg,
                    },
                });
            }
            PendingKind::None => {}
        }
    }

    Ok(results)
}

fn parse_src_rect_crop(e: &quick_xml::events::BytesStart) -> (f32, f32, f32, f32) {
    let mut l = 0.0f32;
    let mut t = 0.0f32;
    let mut r = 0.0f32;
    let mut b = 0.0f32;
    for attr in e.attributes().flatten() {
        let v: f32 = attr
            .unescape_value()
            .unwrap_or_default()
            .parse()
            .unwrap_or(0.0);
        // OOXML srcRect 단위는 1/1000 퍼센트 (100,000 = 100%)
        let ratio = (v / 100_000.0).clamp(0.0, 0.95);
        match attr.key.local_name().as_ref() {
            b"l" => l = ratio,
            b"t" => t = ratio,
            b"r" => r = ratio,
            b"b" => b = ratio,
            _ => {}
        }
    }
    (l, t, r, b)
}

/// `xl/charts/chartN.xml`을 파싱하여 제목, 계열 수식(`c:f`), 캐시 값, 주축/보조축 구성을 추출한다.
fn parse_chart_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    chart_path: &str,
    styles: &StyleSheet,
) -> Result<ChartSpec, String> {
    let mut file = archive
        .by_name(chart_path)
        .map_err(|e| format!("missing chart {}: {}", chart_path, e))?;
    let mut xml_bytes = Vec::new();
    read_capped(&mut file, &mut xml_bytes)
        .map_err(|e| e.to_string())?;

    let mut reader = Reader::from_reader(xml_bytes.as_slice());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(2048);

    let mut title = String::new();
    let mut in_title = false;
    let mut in_plot_area = false;
    let mut in_title_text = false;

    let mut cur_plot_kind = ChartPlotKind::BarClustered;
    let mut is_horizontal_bar = false;
    let mut cur_grouping = String::from("clustered");
    let mut plot_group_index = 0usize;
    let mut group_ax_ids: Vec<Vec<u32>> = Vec::new();

    let mut series: Vec<ChartSeriesSpec> = Vec::new();
    let mut cur_ser: Option<ChartSeriesSpec> = None;
    let mut cur_ser_group_idx: Vec<usize> = Vec::new();

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SerSubSection {
        None,
        Tx,
        Cat,
        Val,
    }
    let mut ser_sec = SerSubSection::None;
    let mut in_f = false;
    let mut in_v = false;
    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"plotArea" => in_plot_area = true,
                    b"title" if !in_plot_area && cur_ser.is_none() => in_title = true,
                    b"t" | b"v" if in_title => {
                        in_title_text = true;
                        text_buf.clear();
                    }
                    b"barChart" | b"bar3DChart" => {
                        cur_plot_kind = ChartPlotKind::BarClustered;
                        cur_grouping = String::from("clustered");
                        group_ax_ids.push(Vec::new());
                    }
                    b"lineChart" | b"line3DChart" => {
                        cur_plot_kind = ChartPlotKind::Line;
                        group_ax_ids.push(Vec::new());
                    }
                    b"areaChart" | b"area3DChart" => {
                        cur_plot_kind = ChartPlotKind::Area;
                        group_ax_ids.push(Vec::new());
                    }
                    b"pieChart" | b"doughnutChart" => {
                        cur_plot_kind = ChartPlotKind::Pie;
                        group_ax_ids.push(Vec::new());
                    }
                    b"scatterChart" => {
                        cur_plot_kind = ChartPlotKind::Scatter;
                        group_ax_ids.push(Vec::new());
                    }
                    b"ser" => {
                        let pk = match (cur_plot_kind, cur_grouping.as_str()) {
                            (ChartPlotKind::BarClustered, "stacked") => ChartPlotKind::BarStacked,
                            (ChartPlotKind::BarClustered, "percentStacked") => {
                                ChartPlotKind::BarPercentStacked
                            }
                            (other, _) => other,
                        };
                        cur_ser = Some(ChartSeriesSpec {
                            name_formula: None,
                            name_cached: None,
                            cat_formula: None,
                            cat_cached: Vec::new(),
                            val_formula: None,
                            val_cached: Vec::new(),
                            color: None,
                            plot_kind: pk,
                            use_secondary_axis: plot_group_index > 0,
                            axis_id: None,
                        });
                    }
                    b"tx" if cur_ser.is_some() => ser_sec = SerSubSection::Tx,
                    b"cat" | b"xVal" if cur_ser.is_some() => ser_sec = SerSubSection::Cat,
                    b"val" | b"yVal" if cur_ser.is_some() => ser_sec = SerSubSection::Val,
                    b"f" if cur_ser.is_some() => {
                        in_f = true;
                        text_buf.clear();
                    }
                    b"v" if cur_ser.is_some() => {
                        in_v = true;
                        text_buf.clear();
                    }
                    b"srgbClr" if cur_ser.is_some() => {
                        if let Some(ser) = cur_ser.as_mut() {
                            if ser.color.is_none() {
                                for attr in e.attributes().flatten() {
                                    if attr.key.local_name().as_ref() == b"val" {
                                        ser.color = Rgba::from_hex(
                                            &attr.unescape_value().unwrap_or_default(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    b"schemeClr" if cur_ser.is_some() => {
                        if let Some(ser) = cur_ser.as_mut() {
                            if ser.color.is_none() {
                                ser.color = resolve_scheme_clr(&e, &styles.theme_colors);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"barDir" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                let v = attr.unescape_value().unwrap_or_default();
                                is_horizontal_bar = v == "bar";
                            }
                        }
                    }
                    b"grouping" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                cur_grouping =
                                    attr.unescape_value().unwrap_or_default().into_owned();
                            }
                        }
                    }
                    b"axId" if cur_ser.is_none() => {
                        if let Some(last_grp) = group_ax_ids.last_mut() {
                            for attr in e.attributes().flatten() {
                                if attr.key.local_name().as_ref() == b"val" {
                                    if let Ok(id) =
                                        attr.unescape_value().unwrap_or_default().parse::<u32>()
                                    {
                                        last_grp.push(id);
                                    }
                                }
                            }
                        }
                    }
                    b"srgbClr" if cur_ser.is_some() => {
                        if let Some(ser) = cur_ser.as_mut() {
                            if ser.color.is_none() {
                                for attr in e.attributes().flatten() {
                                    if attr.key.local_name().as_ref() == b"val" {
                                        ser.color = Rgba::from_hex(
                                            &attr.unescape_value().unwrap_or_default(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    b"schemeClr" if cur_ser.is_some() => {
                        if let Some(ser) = cur_ser.as_mut() {
                            if ser.color.is_none() {
                                ser.color = resolve_scheme_clr(&e, &styles.theme_colors);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_title_text || in_f || in_v {
                    if let Ok(t) = e.unescape() {
                        text_buf.push_str(&t);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"plotArea" => in_plot_area = false,
                    b"title" => in_title = false,
                    b"t" | b"v" if in_title_text => {
                        in_title_text = false;
                        if !text_buf.trim().is_empty() {
                            if !title.is_empty() {
                                title.push(' ');
                            }
                            title.push_str(text_buf.trim());
                        }
                    }
                    b"f" if in_f => {
                        in_f = false;
                        if let Some(ser) = cur_ser.as_mut() {
                            let f_clean = text_buf.trim().to_string();
                            match ser_sec {
                                SerSubSection::Tx => ser.name_formula = Some(f_clean),
                                SerSubSection::Cat => ser.cat_formula = Some(f_clean),
                                SerSubSection::Val => ser.val_formula = Some(f_clean),
                                SerSubSection::None => {}
                            }
                        }
                    }
                    b"v" if in_v => {
                        in_v = false;
                        if let Some(ser) = cur_ser.as_mut() {
                            let v_clean = text_buf.trim();
                            match ser_sec {
                                SerSubSection::Tx => {
                                    if ser.name_cached.is_none() && !v_clean.is_empty() {
                                        ser.name_cached = Some(v_clean.to_string());
                                    }
                                }
                                SerSubSection::Cat => {
                                    ser.cat_cached.push(v_clean.to_string());
                                }
                                SerSubSection::Val => {
                                    if let Ok(n) = v_clean.parse::<f64>() {
                                        ser.val_cached.push(n);
                                    }
                                }
                                SerSubSection::None => {}
                            }
                        }
                    }
                    b"tx" | b"cat" | b"xVal" | b"val" | b"yVal" => {
                        ser_sec = SerSubSection::None;
                    }
                    b"ser" => {
                        if let Some(mut s) = cur_ser.take() {
                            if matches!(cur_plot_kind, ChartPlotKind::BarClustered) {
                                if cur_grouping == "stacked" {
                                    s.plot_kind = ChartPlotKind::BarStacked;
                                } else if cur_grouping == "percentStacked" {
                                    s.plot_kind = ChartPlotKind::BarPercentStacked;
                                }
                            }
                            cur_ser_group_idx.push(plot_group_index);
                            if series.len() < MAX_SERIES_PER_CHART {
                                series.push(s);
                            }
                        }
                    }
                    b"barChart" | b"bar3DChart" | b"lineChart" | b"line3DChart" | b"areaChart"
                    | b"area3DChart" | b"pieChart" | b"doughnutChart" | b"scatterChart" => {
                        plot_group_index += 1;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // 콤보 차트에서 두 번째 차트 그룹이 다른 축 ID를 참조하면 보조 축(use_secondary_axis = true)으로 확정
    let primary_ids = group_ax_ids.first().cloned().unwrap_or_default();
    let mut secondary_ids = Vec::new();
    for (s_idx, ser) in series.iter_mut().enumerate() {
        let g_idx = cur_ser_group_idx.get(s_idx).copied().unwrap_or(0);
        if g_idx > 0 {
            if let Some(ids) = group_ax_ids.get(g_idx) {
                if !ids.is_empty() && ids != &primary_ids {
                    ser.use_secondary_axis = true;
                    secondary_ids = ids.clone();
                }
            } else {
                ser.use_secondary_axis = true;
            }
        }
    }

    Ok(ChartSpec {
        title,
        series,
        primary_axis_ids: primary_ids,
        secondary_axis_ids: secondary_ids,
        is_horizontal_bar,
    })
}

fn resolve_scheme_clr(e: &quick_xml::events::BytesStart, theme: &[Rgba]) -> Option<Rgba> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"val" {
            let v = attr.unescape_value().unwrap_or_default();
            let idx = match v.as_ref() {
                "lt1" | "bg1" => 0,
                "dk1" | "tx1" => 1,
                "lt2" | "bg2" => 2,
                "dk2" | "tx2" => 3,
                "accent1" => 4,
                "accent2" => 5,
                "accent3" => 6,
                "accent4" => 7,
                "accent5" => 8,
                "accent6" => 9,
                _ => return None,
            };
            return theme.get(idx).copied();
        }
    }
    None
}

// ============================================================================
// 3. 시트 수식 계산값 연동 + argoconv/src/slides/chart.rs 재사용 차트 렌더러
// ============================================================================

/// 차트 항목 이름용: 참조 범위 칸을 그 칸의 숫자 서식으로 표시한 글자.
pub fn resolve_formula_range_display(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    default_sheet_idx: usize,
    formula_ref: &str,
) -> Vec<String> {
    use crate::numfmt::{builtin_format_code, FormatCompiler, FormatEngine, FormatValue};
    let cells = resolve_formula_range_cells(wb, default_sheet_idx, formula_ref);
    let mut compiled: HashMap<u32, crate::numfmt::CompiledFormat> = HashMap::new();
    let mut out = Vec::with_capacity(cells.len());
    for (s_idx, r, c) in cells {
        let Some(sheet) = wb.sheets.get(s_idx) else { continue };
        let v = read_evaluated_cell_value(wb, calc_res, sheet, s_idx, r, c);
        let fmt_id = sheet
            .data
            .get_cell(r, c)
            .and_then(|cell| wb.styles.cell_xfs.get(cell.style_idx as usize))
            .map(|xf| xf.num_fmt_id)
            .unwrap_or(0);
        let cf = compiled.entry(fmt_id).or_insert_with(|| {
            let code = wb
                .styles
                .num_fmts
                .get(&fmt_id)
                .map(|s| s.as_str())
                .unwrap_or_else(|| builtin_format_code(fmt_id));
            FormatCompiler::compile(code)
        });
        let s = match v {
            Value::Num(n) => FormatEngine::format_value(&FormatValue::Number(n), cf),
            Value::Bool(b) => FormatEngine::format_value(&FormatValue::Bool(b), cf),
            other => other.coerce_str().unwrap_or_default(),
        };
        out.push(s);
    }
    out
}

/// 참조 수식이 가리키는 칸 목록(시트 번호, 행, 열). 쓰는 영역으로 자르고 10만 칸까지.
fn resolve_formula_range_cells(wb: &Workbook, default_sheet_idx: usize, formula_ref: &str) -> Vec<(usize, u32, u32)> {
    const MAX_POINTS: usize = 100_000;
    let mut out = Vec::new();
    let trimmed = formula_ref.trim().strip_prefix('=').unwrap_or(formula_ref.trim());
    let clean = trimmed.strip_prefix('(').and_then(|s| s.strip_suffix(')')).unwrap_or(trimmed);
    for part in clean.split(',') {
        let (sheet_idx, range_str) = if let Some((sh, r_str)) = part.rsplit_once('!') {
            let s_name = sh
                .trim()
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .unwrap_or(sh.trim())
                .replace("''", "'");
            let found = wb
                .sheets
                .iter()
                .position(|s| s.name == s_name || s.name.eq_ignore_ascii_case(&s_name))
                .unwrap_or(default_sheet_idx);
            (found, r_str.trim())
        } else {
            (default_sheet_idx, part.trim())
        };
        let Some(sheet) = wb.sheets.get(sheet_idx) else { continue };
        let Some(rg) = parse_range_1based(range_str) else { continue };
        let end_row = rg.end_row.min(sheet.max_row.max(1));
        let end_col = rg.end_col.min(sheet.max_col.max(1));
        for r in rg.start_row..=end_row {
            for c in rg.start_col..=end_col {
                if out.len() >= MAX_POINTS {
                    return out;
                }
                out.push((sheet_idx, r, c));
            }
        }
    }
    out
}

/// 차트 계열 참조 수식(예: `'00_Overview'!$B$5:$B$10` 또는 `04_시트!$C$4`)을 해석해
/// **수식 엔진이 계산한 시트 값(`WorkbookCalcResult`)**에서 직접 값을 꺼낸다.
/// 왜: openpyxl 등이 만든 엑셀 파일은 차트 내부에 `<c:numCache>`가 비어 있어 시트 계산값을 읽어야만 차트가 그려진다.
pub fn resolve_formula_range_values(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    default_sheet_idx: usize,
    formula_ref: &str,
) -> Vec<Value> {
    let mut out = Vec::new();
    let trimmed = formula_ref.trim().strip_prefix('=').unwrap_or(formula_ref.trim());
    let clean = trimmed
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(trimmed);

    for part in clean.split(',') {
        let (sheet_idx, range_str) = if let Some((sh, r_str)) = part.rsplit_once('!') {
            let s_name = sh
                .trim()
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .unwrap_or(sh.trim())
                .replace("''", "'");
            let found = wb
                .sheets
                .iter()
                .position(|s| s.name == s_name || s.name.eq_ignore_ascii_case(&s_name))
                .unwrap_or(default_sheet_idx);
            (found, r_str.trim())
        } else {
            (default_sheet_idx, part.trim())
        };

        let Some(sheet) = wb.sheets.get(sheet_idx) else {
            continue;
        };
        let Some(rg) = parse_range_1based(range_str) else {
            continue;
        };

        // 차트 범위가 시트 전체(A1:XFD1048576)여도 멈추지 않게, 쓰는 영역으로 자르고
        // 계열 하나당 점 10만 개까지만 읽는다.
        const MAX_POINTS: usize = 100_000;
        let end_row = rg.end_row.min(sheet.max_row.max(1));
        let end_col = rg.end_col.min(sheet.max_col.max(1));
        'outer: for r in rg.start_row..=end_row {
            for c in rg.start_col..=end_col {
                if out.len() >= MAX_POINTS {
                    break 'outer;
                }
                out.push(read_evaluated_cell_value(wb, calc_res, sheet, sheet_idx, r, c));
            }
        }
    }
    out
}

/// `rust/argoconv/src/slides/chart.rs`의 눈금 간격(Nice Step: $1, 2, 5 \times 10^k$) 계산 규칙.
/// 왜: 데이터 최솟값·최댓값을 그대로 축 끝값으로 쓰면 눈금이 소수점 난수로 깨지므로,
/// 항상 0을 포함하고 사람이 읽기 좋은 깔끔한 $1, 2, 5 \times 10^k$ 간격으로 축 범위와 주 눈금을 산출한다.
pub fn compute_nice_axis_ticks(min_val: f64, max_val: f64, target_steps: usize) -> (f64, f64, f64) {
    let lo = min_val.min(0.0);
    let hi = max_val.max(0.0);
    let raw_span = hi - lo;
    if raw_span.abs() < 1e-12 || !raw_span.is_finite() {
        return (0.0, 10.0, 2.0);
    }

    let steps = target_steps.max(2) as f64;
    let rough_step = raw_span / steps;
    let mag = 10.0f64.powf(rough_step.abs().log10().floor());
    let residual = rough_step / mag;

    let nice_factor = if residual <= 1.0 {
        1.0
    } else if residual <= 2.0 {
        2.0
    } else if residual <= 2.5 {
        2.5
    } else if residual <= 5.0 {
        5.0
    } else {
        10.0
    };
    let step = (nice_factor * mag).max(1e-9);

    let nice_min = (lo / step).floor() * step;
    let mut nice_max = (hi / step).ceil() * step;
    if (nice_max - nice_min).abs() < 1e-12 {
        nice_max = nice_min + step * 5.0;
    }
    (nice_min, nice_max, step)
}

const DEFAULT_CHART_PALETTE: [(u8, u8, u8); 6] = [
    (59, 130, 246),  // #3B82F6 (파랑)
    (16, 185, 129),  // #10B981 (초록)
    (245, 158, 11),  // #F59E0B (주황)
    (239, 68, 68),   // #EF4444 (빨강)
    (139, 92, 246),  // #8B5CF6 (보라)
    (15, 118, 110),  // #0F766E (청록)
];

struct ResolvedSeriesData {
    name: String,
    cats: Vec<String>,
    vals: Vec<f64>,
    color: ASColor,
    plot_kind: ChartPlotKind,
    use_secondary_axis: bool,
}

/// 차트 명세(`ChartSpec`)와 워크북 수식 계산 결과를 결합해 벡터 그리기 목록(`DrawList`)을 생성한다.
/// 00·04 막대 차트(묶은/누적)와 06 선 차트(보조 축 포함)를 PC 화면과 동일하게 렌더링한다.
pub fn render_chart_to_drawlist(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    sheet_idx: usize,
    spec: &ChartSpec,
    frame_pt: ASRect,
) -> DrawList {
    let w = frame_pt.w.max(120.0);
    let h = frame_pt.h.max(80.0);
    let local_rect = ASRect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    };
    let mut dl = DrawList::with_capacity(256, 1024);

    // 1. 차트 카드 배경 및 외곽 테두리
    dl.push_fill(
        local_rect,
        ASColor {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    );
    let border_col = ASColor {
        r: 203,
        g: 213,
        b: 225,
        a: 255,
    };
    dl.push_line(0.0, 0.0, w, 0.0, 1.0, border_col, LINE_STYLE_THIN);
    dl.push_line(0.0, h, w, h, 1.0, border_col, LINE_STYLE_THIN);
    dl.push_line(0.0, 0.0, 0.0, h, 1.0, border_col, LINE_STYLE_THIN);
    dl.push_line(w, 0.0, w, h, 1.0, border_col, LINE_STYLE_THIN);

    // 2. 계열 값을 시트 수식 계산 결과에서 추출 (시트 값이 비어 있을 때만 XML 캐시 사용)
    let mut resolved_series: Vec<ResolvedSeriesData> = Vec::with_capacity(spec.series.len());
    let mut points_left = MAX_POINTS_PER_CHART;
    for (idx, s) in spec.series.iter().enumerate().take(MAX_SERIES_PER_CHART) {
        if points_left == 0 {
            break;
        }
        let name = s
            .name_formula
            .as_deref()
            .and_then(|f| {
                resolve_formula_range_values(wb, calc_res, sheet_idx, f)
                    .into_iter()
                    .next()
            })
            .and_then(|v| v.coerce_str().ok())
            .filter(|st| !st.is_empty())
            .or_else(|| s.name_cached.clone())
            .unwrap_or_else(|| format!("계열 {}", idx + 1));

        // 항목 이름은 칸에 보이는 그대로(날짜 서식이면 "2025-01", 일련번호 45658 이 아니라).
        let mut cats: Vec<String> = if let Some(cf) = &s.cat_formula {
            resolve_formula_range_display(wb, calc_res, sheet_idx, cf)
        } else {
            Vec::new()
        };
        if cats.is_empty() {
            cats = s.cat_cached.clone();
        }

        let mut vals: Vec<f64> = if let Some(vf) = &s.val_formula {
            resolve_formula_range_values(wb, calc_res, sheet_idx, vf)
                .into_iter()
                .map(|v| v.coerce_num().unwrap_or(0.0))
                .collect()
        } else {
            Vec::new()
        };
        if vals.is_empty() {
            vals = s.val_cached.clone();
        }

        let (pr, pg, pb) = DEFAULT_CHART_PALETTE[idx % DEFAULT_CHART_PALETTE.len()];
        let color = s
            .color
            .map(ASColor::from)
            .unwrap_or(ASColor {
                r: pr,
                g: pg,
                b: pb,
                a: 255,
            });

        // 캐시 값(<c:pt>)도 파일에서 온 것이라 끝없이 클 수 있다 — 차트 전체 점 예산으로 자른다.
        vals.truncate(points_left);
        cats.truncate(MAX_POINTS_PER_CHART);
        points_left -= vals.len().min(points_left);

        resolved_series.push(ResolvedSeriesData {
            name,
            cats,
            vals,
            color,
            plot_kind: s.plot_kind,
            use_secondary_axis: s.use_secondary_axis,
        });
    }

    // 3. 차트 제목 출력
    let title_h = if !spec.title.is_empty() {
        push_chart_text(
            &mut dl,
            &spec.title,
            ASRect {
                x: 8.0,
                y: 6.0,
                w: w - 16.0,
                h: 20.0,
            },
            11.5,
            true,
            ALIGN_H_CENTER,
            ASColor {
                r: 30,
                g: 41,
                b: 59,
                a: 255,
            },
        );
        26.0
    } else {
        10.0
    };

    if resolved_series.is_empty() || resolved_series.iter().all(|s| s.vals.is_empty()) {
        return dl;
    }

    let has_secondary = resolved_series.iter().any(|s| s.use_secondary_axis);
    let legend_h = if resolved_series.len() > 1 { 20.0 } else { 4.0 };
    let pad_left = 44.0f32;
    let pad_right = if has_secondary { 44.0f32 } else { 14.0f32 };
    let pad_bottom = 24.0f32 + legend_h;

    let plot_x = pad_left;
    let plot_y = title_h;
    let plot_w = (w - pad_left - pad_right).max(40.0);
    let plot_h = (h - title_h - pad_bottom).max(30.0);

    // 4. 주축(Primary) 및 보조축(Secondary) 최솟값/최댓값 및 눈금 간격(Nice Step) 산출
    let (p_min_raw, p_max_raw) = compute_axis_min_max(&resolved_series, false);
    let (p_min, p_max, p_step) = compute_nice_axis_ticks(p_min_raw, p_max_raw, 5);

    let (s_min, s_max, s_step) = if has_secondary {
        let (s_min_raw, s_max_raw) = compute_axis_min_max(&resolved_series, true);
        compute_nice_axis_ticks(s_min_raw, s_max_raw, 5)
    } else {
        (0.0, 10.0, 2.0)
    };

    // 5. 수평 눈금선 및 좌/우 Y축 레이블 출력
    let grid_col = ASColor {
        r: 226,
        g: 232,
        b: 240,
        a: 255,
    };
    let label_col = ASColor {
        r: 100,
        g: 116,
        b: 139,
        a: 255,
    };

    let mut tick_v = p_min;
    let mut guard = 0;
    while tick_v <= p_max + p_step * 0.01 && guard < 20 {
        let ratio = ((tick_v - p_min) / (p_max - p_min)).clamp(0.0, 1.0) as f32;
        let y_pt = plot_y + plot_h - ratio * plot_h;
        let is_zero = tick_v.abs() < p_step * 1e-4;
        dl.push_line(
            plot_x,
            y_pt,
            plot_x + plot_w,
            y_pt,
            if is_zero { 1.0 } else { 0.6 },
            if is_zero {
                ASColor {
                    r: 148,
                    g: 163,
                    b: 184,
                    a: 255,
                }
            } else {
                grid_col
            },
            if is_zero {
                LINE_STYLE_THIN
            } else {
                LINE_STYLE_DASHED
            },
        );
        let lbl = format_axis_number(tick_v, p_step);
        push_chart_text(
            &mut dl,
            &lbl,
            ASRect {
                x: 2.0,
                y: y_pt - 6.0,
                w: pad_left - 5.0,
                h: 12.0,
            },
            8.0,
            false,
            ALIGN_H_RIGHT,
            label_col,
        );
        tick_v += p_step;
        guard += 1;
    }

    if has_secondary {
        let mut s_v = s_min;
        let mut s_guard = 0;
        while s_v <= s_max + s_step * 0.01 && s_guard < 20 {
            let ratio = ((s_v - s_min) / (s_max - s_min)).clamp(0.0, 1.0) as f32;
            let y_pt = plot_y + plot_h - ratio * plot_h;
            let lbl = format_axis_number(s_v, s_step);
            push_chart_text(
                &mut dl,
                &lbl,
                ASRect {
                    x: plot_x + plot_w + 4.0,
                    y: y_pt - 6.0,
                    w: pad_right - 6.0,
                    h: 12.0,
                },
                8.0,
                false,
                ALIGN_H_LEFT,
                label_col,
            );
            s_v += s_step;
            s_guard += 1;
        }
    }

    // 6. 카테고리 개수 및 X축 카테고리 레이블 출력
    let n_points = resolved_series
        .iter()
        .map(|s| s.vals.len())
        .max()
        .unwrap_or(0);
    if n_points == 0 {
        return dl;
    }

    let primary_cats = resolved_series
        .iter()
        .find(|s| !s.cats.is_empty())
        .map(|s| s.cats.as_slice())
        .unwrap_or(&[]);
    let group_w = plot_w / (n_points as f32);
    let label_stride = ((n_points / 10) + 1).max(1);

    for i in (0..n_points).step_by(label_stride) {
        let cat_lbl = primary_cats
            .get(i)
            .cloned()
            .unwrap_or_else(|| format!("{}", i + 1));
        let cx = plot_x + (i as f32 + 0.5) * group_w;
        let box_w = (group_w * (label_stride as f32)).max(24.0);
        push_chart_text(
            &mut dl,
            &cat_lbl,
            ASRect {
                x: cx - box_w * 0.5,
                y: plot_y + plot_h + 3.0,
                w: box_w,
                h: 14.0,
            },
            8.0,
            false,
            ALIGN_H_CENTER,
            label_col,
        );
    }

    // 7. 막대(Clustered / Stacked) 계열 그리기
    let bar_indices: Vec<usize> = resolved_series
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            matches!(
                s.plot_kind,
                ChartPlotKind::BarClustered
                    | ChartPlotKind::BarStacked
                    | ChartPlotKind::BarPercentStacked
            )
        })
        .map(|(i, _)| i)
        .collect();

    if !bar_indices.is_empty() {
        let is_stacked = resolved_series[bar_indices[0]].plot_kind != ChartPlotKind::BarClustered;
        if is_stacked {
            let bar_w = (group_w * 0.58).clamp(3.0, 38.0);
            for pt_i in 0..n_points {
                let mut pos_acc = 0.0f64;
                let mut neg_acc = 0.0f64;
                for &s_i in &bar_indices {
                    let ser = &resolved_series[s_i];
                    let (ax_min, ax_max) = if ser.use_secondary_axis {
                        (s_min, s_max)
                    } else {
                        (p_min, p_max)
                    };
                    let v = ser.vals.get(pt_i).copied().unwrap_or(0.0);
                    let (v_start, v_end) = if v >= 0.0 {
                        let s = pos_acc;
                        pos_acc += v;
                        (s, pos_acc)
                    } else {
                        let s = neg_acc;
                        neg_acc += v;
                        (neg_acc, s)
                    };
                    let y_top = val_to_y(v_end, ax_min, ax_max, plot_y, plot_h);
                    let y_bot = val_to_y(v_start, ax_min, ax_max, plot_y, plot_h);
                    let bx = plot_x + (pt_i as f32 + 0.5) * group_w - bar_w * 0.5;
                    dl.push_fill(
                        ASRect {
                            x: bx,
                            y: y_top.min(y_bot),
                            w: bar_w,
                            h: (y_bot - y_top).abs().max(1.0),
                        },
                        ser.color,
                    );
                }
            }
        } else {
            let n_bars = bar_indices.len() as f32;
            let cluster_w = (group_w * 0.68).clamp(4.0, 56.0);
            let single_w = (cluster_w / n_bars).clamp(2.0, 32.0);
            for (b_slot, &s_i) in bar_indices.iter().enumerate() {
                let ser = &resolved_series[s_i];
                let (ax_min, ax_max) = if ser.use_secondary_axis {
                    (s_min, s_max)
                } else {
                    (p_min, p_max)
                };
                let zero_y = val_to_y(0.0f64.clamp(ax_min, ax_max), ax_min, ax_max, plot_y, plot_h);
                for (pt_i, &v) in ser.vals.iter().enumerate() {
                    let val_y = val_to_y(v, ax_min, ax_max, plot_y, plot_h);
                    let start_x = plot_x + (pt_i as f32 + 0.5) * group_w - (single_w * n_bars) * 0.5;
                    let bx = start_x + (b_slot as f32) * single_w;
                    dl.push_fill(
                        ASRect {
                            x: bx,
                            y: val_y.min(zero_y),
                            w: (single_w - 1.0).max(1.5),
                            h: (zero_y - val_y).abs().max(1.0),
                        },
                        ser.color,
                    );
                }
            }
        }
    }

    // 8. 선(Line) 및 분산(Scatter) 계열 그리기 (06 시트 선 차트 및 콤보 차트 보조축 선)
    for ser in &resolved_series {
        if !matches!(
            ser.plot_kind,
            ChartPlotKind::Line | ChartPlotKind::Area | ChartPlotKind::Scatter
        ) {
            continue;
        }
        let (ax_min, ax_max) = if ser.use_secondary_axis {
            (s_min, s_max)
        } else {
            (p_min, p_max)
        };
        let mut prev_pt: Option<(f32, f32)> = None;
        for (pt_i, &v) in ser.vals.iter().enumerate() {
            let cx = plot_x + (pt_i as f32 + 0.5) * group_w;
            let cy = val_to_y(v, ax_min, ax_max, plot_y, plot_h);
            if let Some((px, py)) = prev_pt {
                dl.push_line(px, py, cx, cy, 2.0, ser.color, LINE_STYLE_THIN);
            }
            // 데이터 포인트 마커 사각형
            dl.push_fill(
                ASRect {
                    x: cx - 2.2,
                    y: cy - 2.2,
                    w: 4.4,
                    h: 4.4,
                },
                ser.color,
            );
            prev_pt = Some((cx, cy));
        }
    }

    // 9. 하단 범례(Legend) 출력
    if resolved_series.len() > 1 {
        let leg_y = h - legend_h + 2.0;
        let item_w = ((w - 20.0) / (resolved_series.len() as f32)).clamp(50.0, 140.0);
        let start_x = (w - item_w * (resolved_series.len() as f32)) * 0.5;
        for (idx, ser) in resolved_series.iter().enumerate() {
            let lx = start_x + (idx as f32) * item_w;
            dl.push_fill(
                ASRect {
                    x: lx,
                    y: leg_y + 3.0,
                    w: 8.0,
                    h: 8.0,
                },
                ser.color,
            );
            push_chart_text(
                &mut dl,
                &ser.name,
                ASRect {
                    x: lx + 11.0,
                    y: leg_y,
                    w: item_w - 14.0,
                    h: 14.0,
                },
                8.0,
                false,
                ALIGN_H_LEFT,
                ASColor {
                    r: 51,
                    g: 65,
                    b: 85,
                    a: 255,
                },
            );
        }
    }

    dl
}

fn compute_axis_min_max(series: &[ResolvedSeriesData], secondary: bool) -> (f64, f64) {
    let mut min_v = 0.0f64;
    let mut max_v = 0.0f64;
    let mut any = false;

    let is_stacked = series
        .iter()
        .filter(|s| s.use_secondary_axis == secondary)
        .any(|s| matches!(s.plot_kind, ChartPlotKind::BarStacked));

    if is_stacked {
        let max_len = series
            .iter()
            .filter(|s| s.use_secondary_axis == secondary)
            .map(|s| s.vals.len())
            .max()
            .unwrap_or(0);
        for i in 0..max_len {
            let mut pos_sum = 0.0;
            let mut neg_sum = 0.0;
            for s in series.iter().filter(|s| s.use_secondary_axis == secondary) {
                let v = s.vals.get(i).copied().unwrap_or(0.0);
                if v >= 0.0 {
                    pos_sum += v;
                } else {
                    neg_sum += v;
                }
            }
            min_v = min_v.min(neg_sum);
            max_v = max_v.max(pos_sum);
        }
        return (min_v, max_v);
    }

    for s in series.iter().filter(|s| s.use_secondary_axis == secondary) {
        for &v in &s.vals {
            if !v.is_finite() {
                continue;
            }
            if !any || v < min_v {
                min_v = v;
            }
            if !any || v > max_v {
                max_v = v;
            }
            any = true;
        }
    }
    (min_v.min(0.0), max_v.max(0.0))
}

#[inline]
fn val_to_y(v: f64, ax_min: f64, ax_max: f64, plot_y: f32, plot_h: f32) -> f32 {
    let span = (ax_max - ax_min).max(1e-12);
    let ratio = ((v - ax_min) / span).clamp(0.0, 1.0) as f32;
    plot_y + plot_h - ratio * plot_h
}

fn format_axis_number(v: f64, step: f64) -> String {
    if v.abs() < 1e-12 {
        return "0".to_string();
    }
    let raw = if step >= 1.0 && v.fract().abs() < 1e-6 {
        format!("{:.0}", v)
    } else if step >= 0.1 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    };
    // 엑셀 기본처럼 천 단위 쉼표(30000000 → 30,000,000)
    let (sign, body) = match raw.strip_prefix('-') {
        Some(b) => ("-", b),
        None => ("", raw.as_str()),
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    match frac_part {
        Some(f) => format!("{}{}.{}", sign, grouped, f),
        None => format!("{}{}", sign, grouped),
    }
}

fn push_chart_text(
    dl: &mut DrawList,
    text: &str,
    rect: ASRect,
    font_size_pt: f32,
    bold: bool,
    align_h: u8,
    color: ASColor,
) {
    let (off, len) = dl.intern_str(text);
    dl.commands.push(ASDrawCmd {
        cmd_type: crate::draw::DrawCmdType::Text as u8,
        sub_type: 0,
        align_horiz: align_h,
        align_vert: ALIGN_V_CENTER,
        rect,
        clip_rect: rect,
        color1: color,
        color2: ASColor::default(),
        stroke_width: 0.0,
        text_offset: off,
        text_len: len,
        font_size_pt,
        rotation: 0,
        indent: 0,
        text_flags: if bold { TEXT_FLAG_BOLD } else { 0 },
    });
}

// ============================================================================
// 4. minipdf/src/xlsx.rs 재사용: rasterize_emf 및 read_legacy_drawing_images
// ============================================================================

/// `rust/vendor/minipdf/src/xlsx.rs` 에서 옮긴 EMF 메타파일 래스터라이저.
/// 왜: tvOS의 ImageIO(`UIImage`)는 Windows 전용 벡터 형식인 `.emf` / `.wmf`를 디코딩하지 못하므로,
/// EMF 헤더 바운딩 박스와 폴리곤/사각형/선 레코드를 직접 파싱해 RGBA8888 비트맵으로 변환한다.
pub fn rasterize_emf(data: &[u8], target_w: u32, target_h: u32) -> Option<(u32, u32, Vec<u8>)> {
    // EMR_HEADER (type == 1) 최소 길이 88바이트 확인
    if data.len() < 88 {
        return None;
    }
    let rec_type = u32::from_le_bytes(data[0..4].try_into().ok()?);
    let sig = u32::from_le_bytes(data[40..44].try_into().ok()?);
    if rec_type != 1 || sig != 0x464D4520 {
        // ' EMF' 시그니처 불일치
        return None;
    }

    let bounds_l = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let bounds_t = i32::from_le_bytes(data[12..16].try_into().ok()?);
    let bounds_r = i32::from_le_bytes(data[16..20].try_into().ok()?);
    let bounds_b = i32::from_le_bytes(data[20..24].try_into().ok()?);

    // 경계값이 이상하면 i32 빼기가 넘친다 — i64 로 계산.
    let src_w = (bounds_r as i64 - bounds_l as i64).max(1) as f32;
    let src_h = (bounds_b as i64 - bounds_t as i64).max(1) as f32;
    let out_w = target_w.clamp(16, 1024);
    let out_h = target_h.clamp(16, 1024);

    let mut rgba = vec![255u8; (out_w * out_h * 4) as usize];
    let mut cur_brush: (u8, u8, u8) = (240, 244, 248);
    let mut cur_pen: (u8, u8, u8) = (30, 41, 59);

    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let r_type = u32::from_le_bytes(data[pos..pos + 4].try_into().ok()?);
        let r_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().ok()?) as usize;
        if r_size < 8 || pos + r_size > data.len() {
            break;
        }
        let rec = &data[pos..pos + r_size];
        match r_type {
            14 => break, // EMR_EOF
            38 if rec.len() >= 24 => {
                // EMR_CREATEPEN
                cur_pen = (rec[20], rec[21], rec[22]);
            }
            39 if rec.len() >= 20 => {
                // EMR_CREATEBRUSHINDIRECT
                cur_brush = (rec[16], rec[17], rec[18]);
            }
            43 if rec.len() >= 24 => {
                // EMR_RECTANGLE (left, top, right, bottom)
                let l = i32::from_le_bytes(rec[8..12].try_into().unwrap());
                let t = i32::from_le_bytes(rec[12..16].try_into().unwrap());
                let r = i32::from_le_bytes(rec[16..20].try_into().unwrap());
                let b = i32::from_le_bytes(rec[20..24].try_into().unwrap());

                let x1 = (((l as i64 - bounds_l as i64) as f32 / src_w) * (out_w as f32)).clamp(0.0, out_w as f32)
                    as u32;
                let y1 = (((t as i64 - bounds_t as i64) as f32 / src_h) * (out_h as f32)).clamp(0.0, out_h as f32)
                    as u32;
                let x2 = (((r as i64 - bounds_l as i64) as f32 / src_w) * (out_w as f32)).clamp(0.0, out_w as f32)
                    as u32;
                let y2 = (((b as i64 - bounds_t as i64) as f32 / src_h) * (out_h as f32)).clamp(0.0, out_h as f32)
                    as u32;

                for py in y1.min(y2)..y1.max(y2).min(out_h) {
                    for px in x1.min(x2)..x1.max(x2).min(out_w) {
                        let idx = ((py * out_w + px) * 4) as usize;
                        let is_border =
                            px == x1.min(x2) || px + 1 == x1.max(x2) || py == y1.min(y2) || py + 1 == y1.max(y2);
                        let (cr, cg, cb) = if is_border { cur_pen } else { cur_brush };
                        rgba[idx] = cr;
                        rgba[idx + 1] = cg;
                        rgba[idx + 2] = cb;
                        rgba[idx + 3] = 255;
                    }
                }
            }
            _ => {}
        }
        pos += r_size;
    }

    Some((out_w, out_h, rgba))
}

/// `rust/vendor/minipdf/src/xlsx.rs` 에서 옮긴 레거시 VML(`vmlDrawingN.vml`) 그림 판독기.
pub fn read_legacy_drawing_images<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    vml_path: &str,
    media: &mut MediaCache,
) -> Result<Vec<RawSheetDrawing>, String> {
    let vml_rels = read_part_relationships(archive, vml_path)?;
    let xml_bytes = {
        let mut f = archive
            .by_name(vml_path)
            .map_err(|e| format!("missing vml {}: {}", vml_path, e))?;
        let mut b = Vec::new();
        read_capped(&mut f, &mut b).map_err(|e| e.to_string())?;
        b
    };

    let mut reader = Reader::from_reader(xml_bytes.as_slice());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::with_capacity(1024);

    let mut pending: Vec<(Option<String>, String)> = Vec::new(); // (r_id, anchor_text)
    let mut cur_rid: Option<String> = None;
    let mut cur_anchor_text = String::new();
    let mut in_anchor = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"shape" => {
                    cur_rid = None;
                    cur_anchor_text.clear();
                }
                b"imagedata" => {
                    for attr in e.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"relid"
                            || attr.key.local_name().as_ref() == b"id"
                        {
                            cur_rid =
                                Some(attr.unescape_value().unwrap_or_default().into_owned());
                        }
                    }
                }
                b"Anchor" => {
                    in_anchor = true;
                    cur_anchor_text.clear();
                }
                _ => {}
            },
            Ok(Event::Text(e)) if in_anchor => {
                if let Ok(t) = e.unescape() {
                    cur_anchor_text.push_str(&t);
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"Anchor" => in_anchor = false,
                b"shape" => {
                    if cur_rid.is_some() && !cur_anchor_text.is_empty() {
                        if pending.len() < MAX_DRAWINGS_PER_SHEET {
                            pending.push((cur_rid.take(), cur_anchor_text.clone()));
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    let mut out = Vec::new();
    for (rid_opt, anchor_csv) in pending {
        let Some(rid) = rid_opt else { continue };
        let Some(rel) = vml_rels.get(&rid) else {
            continue;
        };
        let nums: Vec<i64> = anchor_csv
            .split(',')
            .filter_map(|s| s.trim().parse::<i64>().ok())
            .collect();
        if nums.len() < 8 {
            continue;
        }
        let anchor = CellAnchorSpec::TwoCell {
            from: AnchorMarker {
                col_0based: nums[0].max(0) as u32,
                col_off_emu: nums[1].saturating_mul(9525), // VML 픽셀(96dpi) -> EMU
                row_0based: nums[2].max(0) as u32,
                row_off_emu: nums[3].saturating_mul(9525),
            },
            to: AnchorMarker {
                col_0based: nums[4].max(0) as u32,
                col_off_emu: nums[5].saturating_mul(9525),
                row_0based: nums[6].max(0) as u32,
                row_off_emu: nums[7].saturating_mul(9525),
            },
        };
        let media_path = resolve_part_target(vml_path, &rel.target);
        {
            if let Some(bytes) = media.get(archive, &media_path) {
                let lower = media_path.to_ascii_lowercase();
                let is_emf = lower.ends_with(".emf") || lower.ends_with(".wmf");
                out.push(RawSheetDrawing {
                    anchor,
                    payload: RawDrawingPayload::Image {
                        media_bytes: bytes,
                        is_emf,
                        crop: (0.0, 0.0, 0.0, 0.0),
                        rotation_deg: 0.0,
                    },
                });
            }
        }
    }
    Ok(out)
}

/// 시트의 원본 드로잉 목록(`RawSheetDrawing`)을 수식 계산 결과 및 `SheetRenderCache`와 결합해
/// 최종 `ResolvedOverlay` 배열로 변환한다.
pub fn build_sheet_overlays(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    render_cache: &SheetRenderCache,
    sheet_idx: usize,
    raw_drawings: &[RawSheetDrawing],
) -> Vec<ResolvedOverlay> {
    let mut out = Vec::with_capacity(raw_drawings.len());

    for d in raw_drawings {
        let (frame_pt, start_row, start_col) = d.anchor.to_sheet_rect_pt(render_cache);
        if frame_pt.w <= 1.0 || frame_pt.h <= 1.0 {
            continue;
        }

        match &d.payload {
            RawDrawingPayload::Chart(spec) => {
                let dl = render_chart_to_drawlist(wb, calc_res, sheet_idx, spec, frame_pt);
                out.push(ResolvedOverlay {
                    kind: 0, // AS_OVERLAY_CHART
                    frame_pt,
                    start_row,
                    start_col,
                    pixel_width: 0,
                    pixel_height: 0,
                    crop: (0.0, 0.0, 0.0, 0.0),
                    rotation_deg: 0.0,
                    title: spec.title.clone(),
                    draw_list: Some(dl),
                    raw_bytes: Arc::new(Vec::new()),
                });
            }
            RawDrawingPayload::Image {
                media_bytes,
                is_emf,
                crop,
                rotation_deg,
            } => {
                if *is_emf {
                    let target_w = (frame_pt.w * 2.0).round().clamp(32.0, 1024.0) as u32;
                    let target_h = (frame_pt.h * 2.0).round().clamp(32.0, 1024.0) as u32;
                    if let Some((pw, ph, rgba)) = rasterize_emf(media_bytes, target_w, target_h) {
                        out.push(ResolvedOverlay {
                            kind: 2, // AS_OVERLAY_IMAGE_RGBA
                            frame_pt,
                            start_row,
                            start_col,
                            pixel_width: pw,
                            pixel_height: ph,
                            crop: *crop,
                            rotation_deg: *rotation_deg,
                            title: String::new(),
                            draw_list: None,
                            raw_bytes: Arc::new(rgba),
                        });
                    }
                } else {
                    out.push(ResolvedOverlay {
                        kind: 1, // AS_OVERLAY_IMAGE_RASTER
                        frame_pt,
                        start_row,
                        start_col,
                        pixel_width: 0,
                        pixel_height: 0,
                        crop: *crop,
                        rotation_deg: *rotation_deg,
                        title: String::new(),
                        draw_list: None,
                        raw_bytes: Arc::clone(media_bytes),
                    });
                }
            }
            RawDrawingPayload::Shape {
                text,
                fill_color,
                stroke_color,
                rotation_deg,
            } => {
                let mut dl = DrawList::with_capacity(8, 128);
                let local_r = ASRect {
                    x: 0.0,
                    y: 0.0,
                    w: frame_pt.w,
                    h: frame_pt.h,
                };
                if let Some(fc) = fill_color {
                    dl.push_fill(local_r, (*fc).into());
                }
                if let Some(sc) = stroke_color {
                    let c: ASColor = (*sc).into();
                    dl.push_line(0.0, 0.0, local_r.w, 0.0, 1.0, c, LINE_STYLE_THIN);
                    dl.push_line(0.0, local_r.h, local_r.w, local_r.h, 1.0, c, LINE_STYLE_THIN);
                    dl.push_line(0.0, 0.0, 0.0, local_r.h, 1.0, c, LINE_STYLE_THIN);
                    dl.push_line(local_r.w, 0.0, local_r.w, local_r.h, 1.0, c, LINE_STYLE_THIN);
                }
                if !text.is_empty() {
                    push_chart_text(
                        &mut dl,
                        text,
                        ASRect {
                            x: 4.0,
                            y: 2.0,
                            w: (local_r.w - 8.0).max(1.0),
                            h: (local_r.h - 4.0).max(1.0),
                        },
                        10.0,
                        false,
                        ALIGN_H_CENTER,
                        ASColor {
                            r: 15,
                            g: 23,
                            b: 42,
                            a: 255,
                        },
                    );
                }
                out.push(ResolvedOverlay {
                    kind: 3, // AS_OVERLAY_SHAPE
                    frame_pt,
                    start_row,
                    start_col,
                    pixel_width: 0,
                    pixel_height: 0,
                    crop: (0.0, 0.0, 0.0, 0.0),
                    rotation_deg: *rotation_deg,
                    title: text.clone(),
                    draw_list: Some(dl),
                    raw_bytes: Arc::new(Vec::new()),
                });
            }
        }
    }

    out
}

// ============================================================================
// 5. P5 단위 시험 (#[test])
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_cell_and_two_cell_anchor_to_excel_pt() {
        let rc = SheetRenderCache {
            col_offsets_pt: vec![0.0, 60.0, 140.0, 220.0],
            row_offsets_pt: vec![0.0, 20.0, 45.0, 70.0],
            merge_map: HashMap::new(),
            big_merges: Vec::new(),
        };

        // 1pt = 12,700 EMU -> 10pt = 127,000 EMU
        let two_cell = CellAnchorSpec::TwoCell {
            from: AnchorMarker {
                col_0based: 1, // x 시작 = 60pt
                col_off_emu: 127_000, // +10pt -> 70pt
                row_0based: 1, // y 시작 = 20pt
                row_off_emu: 63_500,  // +5pt -> 25pt
            },
            to: AnchorMarker {
                col_0based: 2, // x = 140pt
                col_off_emu: 254_000, // +20pt -> 160pt
                row_0based: 2, // y = 45pt
                row_off_emu: 127_000, // +10pt -> 55pt
            },
        };
        let (rect, sr, sc) = two_cell.to_sheet_rect_pt(&rc);
        assert_eq!((sr, sc), (2, 2));
        assert!((rect.x - 70.0).abs() < 1e-4);
        assert!((rect.y - 25.0).abs() < 1e-4);
        assert!((rect.w - 90.0).abs() < 1e-4);
        assert!((rect.h - 30.0).abs() < 1e-4);
    }

    #[test]
    fn test_nice_axis_ticks_rule() {
        let (min_v, max_v, step) = compute_nice_axis_ticks(3.2, 87.4, 5);
        assert_eq!(min_v, 0.0);
        assert_eq!(step, 20.0);
        assert_eq!(max_v, 100.0);

        let (min_neg, max_pos, step2) = compute_nice_axis_ticks(-18.0, 42.0, 5);
        assert!(min_neg <= -18.0);
        assert!(max_pos >= 42.0);
        assert!(step2 == 10.0 || step2 == 20.0);
    }

    #[test]
    fn test_emf_rasterizer_valid_header_and_rectangle() {
        let mut emf = vec![0u8; 128];
        emf[0..4].copy_from_slice(&1u32.to_le_bytes()); // EMR_HEADER
        emf[4..8].copy_from_slice(&88u32.to_le_bytes());
        emf[8..12].copy_from_slice(&0i32.to_le_bytes());
        emf[12..16].copy_from_slice(&0i32.to_le_bytes());
        emf[16..20].copy_from_slice(&100i32.to_le_bytes());
        emf[20..24].copy_from_slice(&50i32.to_le_bytes());
        emf[40..44].copy_from_slice(&0x464D4520u32.to_le_bytes()); // ' EMF'

        // EMR_RECTANGLE (type=43, size=24)
        emf[88..92].copy_from_slice(&43u32.to_le_bytes());
        emf[92..96].copy_from_slice(&24u32.to_le_bytes());
        emf[96..100].copy_from_slice(&10i32.to_le_bytes());
        emf[100..104].copy_from_slice(&10i32.to_le_bytes());
        emf[104..108].copy_from_slice(&90i32.to_le_bytes());
        emf[108..112].copy_from_slice(&40i32.to_le_bytes());

        // EMR_EOF (type=14, size=16)
        emf[112..116].copy_from_slice(&14u32.to_le_bytes());
        emf[116..120].copy_from_slice(&16u32.to_le_bytes());

        let (w, h, rgba) = rasterize_emf(&emf, 64, 32).unwrap();
        assert_eq!((w, h), (64, 32));
        assert_eq!(rgba.len(), (64 * 32 * 4) as usize);
    }
}