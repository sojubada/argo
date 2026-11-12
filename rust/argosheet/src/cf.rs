use crate::calc::{
    excel_compare_values, CellAddr, FormulaParser, Value, WorkbookCalcResult,
};
use crate::parse::shift_formula_references;
use crate::style::{apply_color_tint, BorderStyle, DxfStyle, Rgba, StyleSheet};
use crate::{CellKind, CellRange, Sheet, Workbook, CELL_FLAG_HAS_FORMULA};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

// ============================================================================
// 1. 조건부 서식 데이터 모델 (CFVO, 데이터 막대, 색조, 아이콘 집합, 규칙)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfvoType {
    Min,
    Max,
    Num,
    Percent,
    Percentile,
    Formula,
    AutoMin,
    AutoMax,
}

impl CfvoType {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "min" => CfvoType::Min,
            "max" => CfvoType::Max,
            "num" => CfvoType::Num,
            "percent" => CfvoType::Percent,
            "percentile" => CfvoType::Percentile,
            "formula" => CfvoType::Formula,
            "autoMin" => CfvoType::AutoMin,
            "autoMax" => CfvoType::AutoMax,
            _ => CfvoType::Num,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Cfvo {
    pub cfvo_type: CfvoType,
    pub val: Option<String>,
    pub gte: bool, // 아이콘 집합 임계값에서 >= (true) 인지 > (false) 인지
}

impl Default for Cfvo {
    fn default() -> Self {
        Self {
            cfvo_type: CfvoType::Min,
            val: None,
            gte: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBarAxisPos {
    Automatic,
    Midpoint,
    None,
}

#[derive(Debug, Clone)]
pub struct DataBarSpec {
    pub min_cfvo: Cfvo,
    pub max_cfvo: Cfvo,
    pub color: Rgba,
    pub negative_color: Option<Rgba>,
    pub axis_color: Option<Rgba>,
    pub axis_pos: DataBarAxisPos,
    pub min_length: u32, // 기본 0 (x14 확장 또는 명시값 반영)
    pub max_length: u32, // 기본 100
    pub show_value: bool,
}

impl Default for DataBarSpec {
    fn default() -> Self {
        Self {
            min_cfvo: Cfvo {
                cfvo_type: CfvoType::Min,
                val: None,
                gte: true,
            },
            max_cfvo: Cfvo {
                cfvo_type: CfvoType::Max,
                val: None,
                gte: true,
            },
            color: Rgba {
                r: 59,
                g: 130,
                b: 246,
                a: 255,
            }, // 기본 파랑 (#3B82F6)
            negative_color: None,
            axis_color: None,
            axis_pos: DataBarAxisPos::Automatic,
            min_length: 0,
            max_length: 100,
            show_value: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ColorScaleSpec {
    pub cfvos: Vec<Cfvo>,
    pub colors: Vec<Rgba>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconSetKind {
    Arrows3      = 0,
    Arrows3Gray  = 1,
    Traffic3_1   = 2,
    Signs3       = 3,
    Symbols3     = 4,
    Flags3       = 5,
    Triangles3   = 6,
    Arrows4      = 7,
    Arrows5      = 8,
}

impl IconSetKind {
    pub fn from_str(s: &str) -> Self {
        match s.trim() {
            "3Arrows" => IconSetKind::Arrows3,
            "3ArrowsGray" => IconSetKind::Arrows3Gray,
            "3TrafficLights1" | "3TrafficLights2" => IconSetKind::Traffic3_1,
            "3Signs" => IconSetKind::Signs3,
            "3Symbols" | "3Symbols2" => IconSetKind::Symbols3,
            "3Flags" => IconSetKind::Flags3,
            "3Triangles" | "3Stars" => IconSetKind::Triangles3,
            "4Arrows" | "4ArrowsGray" | "4RedToBlack" | "4Rating" | "4TrafficLights" => {
                IconSetKind::Arrows4
            }
            "5Arrows" | "5ArrowsGray" | "5Rating" | "5Quarters" | "5Boxes" => IconSetKind::Arrows5,
            _ => IconSetKind::Arrows3,
        }
    }

    pub fn icon_count(self) -> usize {
        match self {
            IconSetKind::Arrows4 => 4,
            IconSetKind::Arrows5 => 5,
            _ => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IconSetSpec {
    pub kind: IconSetKind,
    pub cfvos: Vec<Cfvo>,
    pub reverse: bool,
    pub show_value: bool,
}

impl Default for IconSetSpec {
    fn default() -> Self {
        Self {
            kind: IconSetKind::Arrows3,
            cfvos: Vec::new(),
            reverse: false,
            show_value: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CfRuleKind {
    DataBar(DataBarSpec),
    ColorScale(ColorScaleSpec),
    IconSet(IconSetSpec),
    CellIs {
        operator: String,
        formulas: Vec<String>,
    },
    Expression {
        formula: String,
    },
    Top10 {
        bottom: bool,
        percent: bool,
        rank: u32,
    },
    AboveAverage {
        above: bool,
        equal_average: bool,
        std_dev: Option<i32>,
    },
    DuplicateValues,
    UniqueValues,
    ContainsText {
        text: String,
        operator: String, // containsText, notContains, beginsWith, endsWith
    },
    ContainsBlanks,
    NotContainsBlanks,
    ContainsErrors,
    NotContainsErrors,
}

#[derive(Debug, Clone)]
pub struct CfRule {
    pub priority: i32,
    pub stop_if_true: bool,
    pub dxf_id: Option<u32>,
    pub dxf_inline: Option<DxfStyle>, // x14 확장에서 인라인 dxf가 올 때 사용
    pub kind: CfRuleKind,
}

#[derive(Debug, Clone)]
pub struct CfBlock {
    pub ranges: Vec<CellRange>,
    pub rules: Vec<CfRule>,
}

// ============================================================================
// 2. 표(Table / ListObject) 모델
// ============================================================================

#[derive(Debug, Clone)]
pub struct TableDef {
    pub name: String,
    pub range: CellRange,
    pub header_row_count: u32,
    pub totals_row_count: u32,
    pub has_auto_filter: bool,
    pub style_name: String,
    pub show_first_column: bool,
    pub show_last_column: bool,
    pub show_row_stripes: bool,
    pub show_column_stripes: bool,
}

// ============================================================================
// 3. 셀별 최종 계산된 시각 효과 결과 (타일 렌더러가 즉시 참조)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ResolvedDataBar {
    pub start_ratio: f32, // 셀 왼쪽 기준 시작 비율 (0.0 ~ 1.0)
    pub end_ratio: f32,   // 셀 왼쪽 기준 끝 비율 (0.0 ~ 1.0)
    pub color: Rgba,
    pub axis_ratio: Option<f32>, // 음수/양수 혼합 시 0점 수직 점선 위치 비율
    pub axis_color: Rgba,
    pub show_value: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedIcon {
    pub set_kind: IconSetKind,
    pub index: u8, // 0..count-1
    pub show_value: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CellCfOverlay {
    pub bg_color: Option<Rgba>,
    pub text_color: Option<Rgba>,
    pub font_bold: Option<bool>,
    pub font_italic: Option<bool>,
    pub font_underline: Option<bool>,
    pub font_strike: Option<bool>,
    pub border: Option<BorderStyle>,
    pub num_fmt_code: Option<String>,
    pub data_bar: Option<ResolvedDataBar>,
    pub icon: Option<ResolvedIcon>,
    pub hide_value: bool,
}

/// 시트 하나에 대한 조건부 서식 + 표 스타일 사전 평가 결과
#[derive(Default)]
pub struct SheetCfCache {
    pub cells: HashMap<u64, CellCfOverlay>,
    pub filter_buttons: HashSet<u64>, // 표 머리글 또는 시트 autoFilter 우하단 화살표 단추 표시 셀
    /// 시간·칸 수 상한에 걸려 일부 규칙을 그리지 못했으면 true (진단 표시용)
    pub truncated: bool,
}

// ============================================================================
// 4. 범위 통계 및 CF 평가 엔진
// ============================================================================

#[derive(Default)]
pub struct RangeStats {
    nums: Vec<f64>,
    sorted_nums: Vec<f64>,
    min: f64,
    max: f64,
    mean: f64,
    std_dev: f64,
    value_counts: HashMap<String, usize>,
}

impl RangeStats {
    fn compute(
        wb: &Workbook,
        calc_res: &WorkbookCalcResult,
        sheet: &Sheet,
        sheet_idx: usize,
        ranges: &[CellRange],
    ) -> Self {
        let mut nums = Vec::new();
        let mut value_counts: HashMap<String, usize> = HashMap::new();
        let mut visited = HashSet::new();

        for rg in ranges {
            for r in rg.start_row..=rg.end_row {
                for c in rg.start_col..=rg.end_col {
                    if !visited.insert((r, c)) {
                        continue;
                    }
                    let v = read_evaluated_cell_value(wb, calc_res, sheet, sheet_idx, r, c);
                    match &v {
                        Value::Num(n) if !n.is_nan() && !n.is_infinite() => {
                            nums.push(*n);
                            let key = format!("N:{:.12}", n);
                            *value_counts.entry(key).or_insert(0) += 1;
                        }
                        Value::Str(s) if !s.is_empty() => {
                            let key = format!("S:{}", s.to_lowercase());
                            *value_counts.entry(key).or_insert(0) += 1;
                        }
                        Value::Bool(b) => {
                            let key = format!("B:{}", b);
                            *value_counts.entry(key).or_insert(0) += 1;
                        }
                        _ => {}
                    }
                }
            }
        }

        if nums.is_empty() {
            return Self {
                nums,
                sorted_nums: Vec::new(),
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                std_dev: 0.0,
                value_counts,
            };
        }

        let mut sorted = nums.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let min = sorted[0];
        let max = *sorted.last().unwrap();
        let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
        let var = nums.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (nums.len() as f64);
        let std_dev = var.sqrt();

        Self {
            nums,
            sorted_nums: sorted,
            min,
            max,
            mean,
            std_dev,
            value_counts,
        }
    }

    /// PERCENTILE.INC 방식 선형 보간 (p: 0.0 ~ 100.0)
    fn percentile(&self, p: f64) -> f64 {
        if self.sorted_nums.is_empty() {
            return 0.0;
        }
        let k = (p / 100.0).clamp(0.0, 1.0);
        if self.sorted_nums.len() == 1 {
            return self.sorted_nums[0];
        }
        let pos = k * ((self.sorted_nums.len() - 1) as f64);
        let lo = pos.floor() as usize;
        let hi = pos.ceil() as usize;
        let frac = pos - (lo as f64);
        self.sorted_nums[lo] + frac * (self.sorted_nums[hi] - self.sorted_nums[lo])
    }
}

pub fn read_evaluated_cell_value(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    sheet: &Sheet,
    sheet_idx: usize,
    row: u32,
    col: u32,
) -> Value {
    let Some(cell) = sheet.data.get_cell(row, col) else {
        return Value::Blank;
    };
    unsafe {
        match cell.kind_enum() {
            CellKind::Num => Value::Num(cell.payload.num),
            CellKind::Str => Value::Str(wb.pool.get(cell.payload.str_span).to_string()),
            CellKind::Bool => Value::Bool(cell.payload.boolean != 0),
            CellKind::Err => {
                let s = wb.pool.get(cell.payload.str_span);
                Value::Err(crate::calc::CalcErr::from_str(s).unwrap_or(crate::calc::CalcErr::Value))
            }
            CellKind::Blank => {
                if (cell.flags & CELL_FLAG_HAS_FORMULA) != 0 {
                    calc_res
                        .values
                        .get(&CellAddr {
                            sheet: sheet_idx,
                            row,
                            col,
                        })
                        .cloned()
                        .unwrap_or(Value::Blank)
                } else {
                    Value::Blank
                }
            }
        }
    }
}

/// 시트 전체의 표(Table) 스타일과 조건부 서식을 계산하여 `SheetCfCache`를 만든다.
/// 범위를 시트 사용 영역으로 자르고 남은 예산 안으로 줄인다. 남는 것이 없으면 None.
fn bound_range(sheet: &Sheet, rg: CellRange, budget: &mut u64) -> Option<CellRange> {
    if rg.start_row == 0 || rg.start_col == 0 || rg.start_row > rg.end_row || rg.start_col > rg.end_col {
        return None;
    }
    let mut r = rg;
    r.end_row = r.end_row.min(sheet.max_row.max(1));
    r.end_col = r.end_col.min(sheet.max_col.max(1));
    if r.start_row > r.end_row || r.start_col > r.end_col || *budget == 0 {
        return None;
    }
    let width = (r.end_col - r.start_col + 1) as u64;
    if width > *budget {
        r.end_col = r.start_col + (*budget as u32) - 1;
    }
    let width = (r.end_col - r.start_col + 1) as u64;
    let rows = (r.end_row - r.start_row + 1) as u64;
    let max_rows = (*budget / width).max(1);
    if rows > max_rows {
        r.end_row = r.start_row + max_rows as u32 - 1;
    }
    let used = width * (r.end_row - r.start_row + 1) as u64;
    *budget = budget.saturating_sub(used);
    Some(r)
}

pub fn build_sheet_cf_cache(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    sheet_idx: usize,
) -> SheetCfCache {
    let mut cache = SheetCfCache::default();
    let Some(sheet) = wb.sheets.get(sheet_idx) else {
        return cache;
    };

    // 계산 엔진 세션: 수식 규칙을 칸마다 계산할 때 계산값 표를 매번 복사하지 않게 한다.
    let _session = crate::calc::StandaloneSession::begin(wb, calc_res);
    // 범위를 칸마다 도는 곳의 상한. sqref="A1:XFD1048576" 같은 범위를 그대로 돌면
    // 170억 칸이라 앱이 멈춘다. 시트에서 쓰는 영역으로 자르고, 시트당 칸 수 예산을 둔다.
    let mut cf_budget: u64 = 500_000;
    let mut table_budget: u64 = 200_000;

    // 1. 시트 기본 autoFilter 머리글 단추 등록
    if let Some(af) = sheet.auto_filter {
        for c in af.start_col..=af.end_col.min(sheet.max_col.max(1)).min(16_384) {
            cache.filter_buttons.insert(crate::cell_key(af.start_row, c));
        }
    }

    // 2. 표(Table) 스타일 및 필터 단추 적용
    //    (셀 자체에 이미 배경 채우기가 있으면 셀 서식이 이기므로, draw.rs에서 셀 채우기가 없을 때 표 배경을 쓴다)
    for tbl in &sheet.tables {
        let Some(rg) = bound_range(sheet, tbl.range, &mut table_budget) else {
            continue;
        };
        let mut bounded = tbl.clone();
        bounded.range = rg;
        apply_table_to_cache(&wb.styles, sheet, &bounded, &mut cache);
    }

    // 3. 조건부 서식 블록 수집 및 priority 오름차순 정렬
    let bounded_blocks: Vec<Vec<CellRange>> = sheet
        .cf_blocks
        .iter()
        .map(|b| {
            b.ranges
                .iter()
                .filter_map(|rg| bound_range(sheet, *rg, &mut cf_budget))
                .collect()
        })
        .collect();
    let mut all_rules: Vec<(&[CellRange], &CfRule, RangeStats)> = Vec::new();
    for (block, ranges) in sheet.cf_blocks.iter().zip(bounded_blocks.iter()) {
        if ranges.is_empty() || block.rules.is_empty() {
            continue;
        }
        let stats = RangeStats::compute(wb, calc_res, sheet, sheet_idx, ranges);
        for rule in &block.rules {
            all_rules.push((ranges.as_slice(), rule, clone_stats(&stats)));
        }
    }
    all_rules.sort_by_key(|(_, rule, _)| {
        if rule.priority <= 0 {
            i32::MAX
        } else {
            rule.priority
        }
    });

    let mut stopped_cells: HashSet<u64> = HashSet::new();

    // 시간 상한: 수식 규칙이 열 전체(예: COUNTIF($A:$A,A1))를 칸마다 다시 훑으면 큰 시트에서
    // 몇 분이 걸린다. 시트당 1.5초를 넘기면 남은 규칙은 그리지 않는다(뷰어가 멈추는 것보다 낫다).
    let cf_deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    let mut cf_tick: u32 = 0;

    'rules: for (ranges, rule, stats) in &all_rules {
        let anchor_row = ranges.first().map(|r| r.start_row).unwrap_or(1);
        let anchor_col = ranges.first().map(|r| r.start_col).unwrap_or(1);

        for rg in *ranges {
            for r in rg.start_row..=rg.end_row {
                for c in rg.start_col..=rg.end_col {
                    let key = crate::cell_key(r, c);
                    cf_tick = cf_tick.wrapping_add(1);
                    if cf_tick % 16 == 0 && std::time::Instant::now() > cf_deadline {
                        cache.truncated = true;
                        break 'rules;
                    }
                    if stopped_cells.contains(&key) {
                        continue;
                    }
                    let val = read_evaluated_cell_value(wb, calc_res, sheet, sheet_idx, r, c);

                    match &rule.kind {
                        CfRuleKind::DataBar(spec) => {
                            if let Value::Num(n) = val {
                                if let Some(bar) = evaluate_data_bar(spec, stats, n) {
                                    let entry = cache.cells.entry(key).or_default();
                                    if entry.data_bar.is_none() {
                                        entry.data_bar = Some(bar);
                                        if !bar.show_value {
                                            entry.hide_value = true;
                                        }
                                    }
                                }
                            }
                        }
                        CfRuleKind::ColorScale(spec) => {
                            if let Value::Num(n) = val {
                                if let Some(color) = evaluate_color_scale(spec, stats, n) {
                                    let entry = cache.cells.entry(key).or_default();
                                    if entry.bg_color.is_none() {
                                        entry.bg_color = Some(color);
                                    }
                                }
                            }
                        }
                        CfRuleKind::IconSet(spec) => {
                            if let Value::Num(n) = val {
                                if let Some(icon) = evaluate_icon_set(spec, stats, n) {
                                    let entry = cache.cells.entry(key).or_default();
                                    if entry.icon.is_none() {
                                        entry.icon = Some(icon);
                                        if !icon.show_value {
                                            entry.hide_value = true;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {
                            if evaluate_boolean_cf_rule(
                                wb,
                                calc_res,
                                sheet_idx,
                                r,
                                c,
                                anchor_row,
                                anchor_col,
                                &val,
                                &rule.kind,
                                stats,
                            ) {
                                let dxf = rule
                                    .dxf_inline
                                    .as_ref()
                                    .or_else(|| {
                                        rule.dxf_id.and_then(|id| wb.styles.dxfs.get(id as usize))
                                    });
                                if let Some(d) = dxf {
                                    let entry = cache.cells.entry(key).or_default();
                                    merge_dxf_into_overlay(entry, d);
                                }
                                if rule.stop_if_true {
                                    stopped_cells.insert(key);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    cache
}

fn clone_stats(s: &RangeStats) -> RangeStats {
    RangeStats {
        nums: s.nums.clone(),
        sorted_nums: s.sorted_nums.clone(),
        min: s.min,
        max: s.max,
        mean: s.mean,
        std_dev: s.std_dev,
        value_counts: s.value_counts.clone(),
    }
}

fn merge_dxf_into_overlay(overlay: &mut CellCfOverlay, dxf: &DxfStyle) {
    // 왜: 상위 우선순위 규칙이 이미 정한 속성은 덮어쓰지 않고 빈 속성만 채운다.
    if overlay.bg_color.is_none() {
        if let Some(fill) = &dxf.fill {
            overlay.bg_color = fill.dxf_solid_color();
        }
    }
    if let Some(font) = &dxf.font {
        if overlay.text_color.is_none() {
            overlay.text_color = font.color;
        }
        if overlay.font_bold.is_none() && font.has_bold {
            overlay.font_bold = Some(font.bold);
        }
        if overlay.font_italic.is_none() && font.has_italic {
            overlay.font_italic = Some(font.italic);
        }
        if overlay.font_underline.is_none() && font.has_underline {
            overlay.font_underline = Some(font.underline);
        }
        if overlay.font_strike.is_none() && font.has_strike {
            overlay.font_strike = Some(font.strike);
        }
    }
    if overlay.border.is_none() {
        overlay.border = dxf.border.clone();
    }
    if overlay.num_fmt_code.is_none() {
        overlay.num_fmt_code = dxf.num_fmt_code.clone();
    }
}

// ============================================================================
// 5. CFVO 해석 · 데이터 막대 · 3색 색조 · 아이콘 집합 계산 함수
// ============================================================================

fn resolve_cfvo_value(cfvo: &Cfvo, stats: &RangeStats, is_max: bool) -> f64 {
    let raw_num = cfvo
        .val
        .as_deref()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0);

    match cfvo.cfvo_type {
        CfvoType::Min => stats.min,
        CfvoType::Max => stats.max,
        CfvoType::AutoMin => stats.min.min(0.0),
        CfvoType::AutoMax => stats.max.max(0.0),
        CfvoType::Num | CfvoType::Formula => raw_num,
        CfvoType::Percent => stats.min + (stats.max - stats.min) * (raw_num / 100.0),
        CfvoType::Percentile => stats.percentile(raw_num),
    }
    .min(if is_max { f64::INFINITY } else { f64::INFINITY })
}

/// 데이터 막대 비율 및 음수/기준축 위치를 계산한다.
pub fn evaluate_data_bar(
    spec: &DataBarSpec,
    stats: &RangeStats,
    val: f64,
) -> Option<ResolvedDataBar> {
    if stats.nums.is_empty() && matches!(spec.min_cfvo.cfvo_type, CfvoType::Min | CfvoType::AutoMin)
    {
        return None;
    }

    let v_min = resolve_cfvo_value(&spec.min_cfvo, stats, false);
    let v_max = resolve_cfvo_value(&spec.max_cfvo, stats, true);

    let default_neg_color = Rgba {
        r: 239,
        g: 68,
        b: 68,
        a: 255,
    }; // 음수 기본 빨강 (#EF4444)
    let axis_color = spec.axis_color.unwrap_or(Rgba::BLACK);

    // 1. 범위 내에 음수가 있고 axis_pos가 None이 아닌 경우: 0 기준축 중심으로 좌(음수)/우(양수) 분리
    let has_negative_domain = v_min < 0.0
        && spec.axis_pos != DataBarAxisPos::None
        && !matches!(spec.min_cfvo.cfvo_type, CfvoType::Num if v_min >= 0.0);

    if has_negative_domain {
        let min_neg = v_min.min(0.0);
        let max_pos = v_max.max(0.0);
        let span = (max_pos - min_neg).max(1e-12);
        let axis_ratio = match spec.axis_pos {
            DataBarAxisPos::Midpoint => 0.5f32,
            _ => ((-min_neg) / span).clamp(0.0, 1.0) as f32,
        };

        if val >= 0.0 {
            let pos_frac = if max_pos > 0.0 {
                (val / max_pos).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let end_r = axis_ratio + pos_frac * (1.0 - axis_ratio);
            return Some(ResolvedDataBar {
                start_ratio: axis_ratio,
                end_ratio: end_r,
                color: spec.color,
                axis_ratio: Some(axis_ratio),
                axis_color,
                show_value: spec.show_value,
            });
        } else {
            let neg_frac = if min_neg < 0.0 {
                (val / min_neg).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let start_r = axis_ratio - neg_frac * axis_ratio;
            return Some(ResolvedDataBar {
                start_ratio: start_r,
                end_ratio: axis_ratio,
                color: spec.negative_color.unwrap_or(default_neg_color),
                axis_ratio: Some(axis_ratio),
                axis_color,
                show_value: spec.show_value,
            });
        }
    }

    // 2. 양수 전용 구간 (예: 0~1 고정 구간 또는 min~max 양수 구간)
    let span = v_max - v_min;
    let raw_t = if span.abs() < 1e-12 {
        0.5
    } else {
        ((val - v_min) / span).clamp(0.0, 1.0)
    };

    let min_r = (spec.min_length as f64 / 100.0).clamp(0.0, 1.0);
    let max_r = (spec.max_length as f64 / 100.0).clamp(min_r, 1.0);
    let end_ratio = (min_r + raw_t * (max_r - min_r)) as f32;

    Some(ResolvedDataBar {
        start_ratio: 0.0,
        end_ratio,
        color: if val < 0.0 {
            spec.negative_color.unwrap_or(spec.color)
        } else {
            spec.color
        },
        axis_ratio: None,
        axis_color,
        show_value: spec.show_value,
    })
}

/// 2색 또는 3색 색조(Color Scale)를 계산한다.
/// 왜: 3색 색조는 전체를 하나로 보간하지 않고 왼쪽(`min..mid`)과 오른쪽(`mid..max`)을 각각 따로 선형 보간한다.
pub fn evaluate_color_scale(
    spec: &ColorScaleSpec,
    stats: &RangeStats,
    val: f64,
) -> Option<Rgba> {
    if spec.cfvos.len() < 2 || spec.colors.len() < 2 {
        return None;
    }

    if spec.cfvos.len() >= 3 && spec.colors.len() >= 3 {
        let v0 = resolve_cfvo_value(&spec.cfvos[0], stats, false);
        let v1 = resolve_cfvo_value(&spec.cfvos[1], stats, false);
        let v2 = resolve_cfvo_value(&spec.cfvos[2], stats, true);
        let c0 = spec.colors[0];
        let c1 = spec.colors[1];
        let c2 = spec.colors[2];

        if val <= v0 {
            return Some(c0);
        }
        if val >= v2 {
            return Some(c2);
        }
        if val <= v1 {
            let span = v1 - v0;
            let t = if span.abs() < 1e-12 {
                0.0
            } else {
                (val - v0) / span
            };
            return Some(Rgba::lerp(c0, c1, t));
        } else {
            let span = v2 - v1;
            let t = if span.abs() < 1e-12 {
                1.0
            } else {
                (val - v1) / span
            };
            return Some(Rgba::lerp(c1, c2, t));
        }
    }

    let v0 = resolve_cfvo_value(&spec.cfvos[0], stats, false);
    let v1 = resolve_cfvo_value(&spec.cfvos[1], stats, true);
    let c0 = spec.colors[0];
    let c1 = spec.colors[1];
    let span = v1 - v0;
    let t = if span.abs() < 1e-12 {
        0.5
    } else {
        (val - v0) / span
    };
    Some(Rgba::lerp(c0, c1, t))
}

/// 아이콘 집합(3Arrows 등)에서 현재 값의 단계 인덱스(0..count-1)를 판정한다.
pub fn evaluate_icon_set(
    spec: &IconSetSpec,
    stats: &RangeStats,
    val: f64,
) -> Option<ResolvedIcon> {
    let count = spec.kind.icon_count();
    // 표준은 cfvo 가 아이콘 수만큼 있고 첫째는 쓰지 않는다(cfvos[1..] 이 하한).
    // 그런데 첫째를 빼고 하한만 적은 파일도 있다(이 샘플 06 시트: 3Arrows 에 -15, 15 두 개).
    // 둘 다 받는다. 그보다 적으면 규칙을 버린다.
    let first = if spec.cfvos.len() >= count {
        1
    } else if spec.cfvos.len() + 1 == count {
        0
    } else {
        return None;
    };

    let mut bucket = 0usize;
    for i in 1..count {
        let cfvo = &spec.cfvos[i - 1 + first];
        let threshold = resolve_cfvo_value(cfvo, stats, false);
        let passed = if cfvo.gte {
            val >= threshold - 1e-12
        } else {
            val > threshold + 1e-12
        };
        if passed {
            bucket = i;
        } else {
            break;
        }
    }

    let final_idx = if spec.reverse {
        (count - 1) - bucket
    } else {
        bucket
    };

    Some(ResolvedIcon {
        set_kind: spec.kind,
        index: final_idx as u8,
        show_value: spec.show_value,
    })
}

// ============================================================================
// 6. 논리형 조건부 서식 평가기 (cellIs, expression, top10, 평균, 중복, 텍스트, 빈칸/오류)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn evaluate_boolean_cf_rule(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    sheet_idx: usize,
    row: u32,
    col: u32,
    anchor_row: u32,
    anchor_col: u32,
    cell_val: &Value,
    kind: &CfRuleKind,
    stats: &RangeStats,
) -> bool {
    let d_row = row as i32 - anchor_row as i32;
    let d_col = col as i32 - anchor_col as i32;

    match kind {
        CfRuleKind::CellIs { operator, formulas } => {
            if matches!(cell_val, Value::Blank | Value::Err(_)) {
                return false;
            }
            let Some(f1_raw) = formulas.first() else {
                return false;
            };
            let v1 = eval_cf_formula(wb, calc_res, sheet_idx, row, col, f1_raw, d_row, d_col);
            let ord1 = excel_compare_values(cell_val, &v1);

            match operator.as_str() {
                "equal" => ord1 == Ordering::Equal,
                "notEqual" => ord1 != Ordering::Equal,
                "greaterThan" => ord1 == Ordering::Greater,
                "lessThan" => ord1 == Ordering::Less,
                "greaterThanOrEqual" => ord1 != Ordering::Less,
                "lessThanOrEqual" => ord1 != Ordering::Greater,
                "between" | "notBetween" => {
                    let Some(f2_raw) = formulas.get(1) else {
                        return false;
                    };
                    let v2 =
                        eval_cf_formula(wb, calc_res, sheet_idx, row, col, f2_raw, d_row, d_col);
                    let (lo, hi) = if excel_compare_values(&v1, &v2) != Ordering::Greater {
                        (&v1, &v2)
                    } else {
                        (&v2, &v1)
                    };
                    let in_between = excel_compare_values(cell_val, lo) != Ordering::Less
                        && excel_compare_values(cell_val, hi) != Ordering::Greater;
                    if operator == "between" {
                        in_between
                    } else {
                        !in_between
                    }
                }
                _ => false,
            }
        }
        CfRuleKind::Expression { formula } => {
            let v = eval_cf_formula(wb, calc_res, sheet_idx, row, col, formula, d_row, d_col);
            v.coerce_bool().unwrap_or(false)
        }
        CfRuleKind::Top10 {
            bottom,
            percent,
            rank,
        } => {
            let Value::Num(n) = cell_val else {
                return false;
            };
            if stats.sorted_nums.is_empty() || *rank == 0 {
                return false;
            }
            let total = stats.sorted_nums.len();
            let k = if *percent {
                ((total as f64 * (*rank as f64) / 100.0).floor() as usize).max(1)
            } else {
                (*rank as usize).min(total)
            };
            if *bottom {
                let cutoff = stats.sorted_nums[k - 1];
                *n <= cutoff + 1e-12
            } else {
                let cutoff = stats.sorted_nums[total - k];
                *n >= cutoff - 1e-12
            }
        }
        CfRuleKind::AboveAverage {
            above,
            equal_average,
            std_dev,
        } => {
            let Value::Num(n) = cell_val else {
                return false;
            };
            if stats.nums.is_empty() {
                return false;
            }
            let threshold = if let Some(sd) = std_dev {
                if *above {
                    stats.mean + (*sd as f64) * stats.std_dev
                } else {
                    stats.mean - (*sd as f64) * stats.std_dev
                }
            } else {
                stats.mean
            };
            match (*above, *equal_average) {
                (true, true) => *n >= threshold - 1e-12,
                (true, false) => *n > threshold + 1e-12,
                (false, true) => *n <= threshold + 1e-12,
                (false, false) => *n < threshold - 1e-12,
            }
        }
        CfRuleKind::DuplicateValues | CfRuleKind::UniqueValues => {
            let key = match cell_val {
                Value::Num(n) => format!("N:{:.12}", n),
                Value::Str(s) if !s.is_empty() => format!("S:{}", s.to_lowercase()),
                Value::Bool(b) => format!("B:{}", b),
                _ => return false,
            };
            let cnt = stats.value_counts.get(&key).copied().unwrap_or(0);
            if matches!(kind, CfRuleKind::DuplicateValues) {
                cnt >= 2
            } else {
                cnt == 1
            }
        }
        CfRuleKind::ContainsText { text, operator } => {
            if matches!(cell_val, Value::Blank | Value::Err(_)) {
                return false;
            }
            let s = cell_val.coerce_str().unwrap_or_default().to_lowercase();
            let pat = text.to_lowercase();
            match operator.as_str() {
                "notContains" => !s.contains(&pat),
                "beginsWith" => s.starts_with(&pat),
                "endsWith" => s.ends_with(&pat),
                _ => s.contains(&pat),
            }
        }
        CfRuleKind::ContainsBlanks => match cell_val {
            Value::Blank => true,
            Value::Str(s) => s.trim().is_empty(),
            _ => false,
        },
        CfRuleKind::NotContainsBlanks => match cell_val {
            Value::Blank => false,
            Value::Str(s) => !s.trim().is_empty(),
            _ => true,
        },
        CfRuleKind::ContainsErrors => matches!(cell_val, Value::Err(_)),
        CfRuleKind::NotContainsErrors => !matches!(cell_val, Value::Err(_)),
        _ => false,
    }
}

fn eval_cf_formula(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    sheet_idx: usize,
    row: u32,
    col: u32,
    raw_formula: &str,
    d_row: i32,
    d_col: i32,
) -> Value {
    let shifted = shift_formula_references(raw_formula, d_row, d_col);
    // 단순 숫자나 따옴표 문자열이면 즉시 반환
    if let Ok(n) = shifted.trim().parse::<f64>() {
        return Value::Num(n);
    }
    if let Some(s) = shifted
        .trim()
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
    {
        return Value::Str(s.replace("\"\"", "\""));
    }
    let Ok(expr) = FormulaParser::parse_str(&shifted) else {
        return Value::Blank;
    };
    crate::calc::eval_standalone_expr(
        wb,
        calc_res,
        CellAddr {
            sheet: sheet_idx,
            row,
            col,
        },
        &expr,
    )
}

// ============================================================================
// 7. 표(Table) 스타일 팔레트 및 줄무늬/머리글/요약행/필터 단추 적용
// ============================================================================

fn resolve_builtin_table_accent(style_name: &str, styles: &StyleSheet) -> (Rgba, bool, bool) {
    // (기준 테마 색, is_dark, is_medium)
    let lower = style_name.to_ascii_lowercase();
    let is_dark = lower.contains("dark");
    let is_medium = lower.contains("medium");
    let num: usize = style_name
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(2);

    // 엑셀 표 스타일 번호별 테마 슬롯 매핑 (1=dk1, 2=accent1, 3=accent2, 4=accent3, 5=accent4, 6=accent5, 7=accent6 순환)
    let slot = if num <= 1 {
        1
    } else {
        4 + ((num - 2) % 6)
    };
    let base = styles
        .theme_colors
        .get(slot)
        .copied()
        .unwrap_or(Rgba {
            r: 68,
            g: 114,
            b: 196,
            a: 255,
        });
    (base, is_dark, is_medium)
}

fn apply_table_to_cache(
    styles: &StyleSheet,
    sheet: &Sheet,
    tbl: &TableDef,
    cache: &mut SheetCfCache,
) {
    let rg = tbl.range;
    if rg.start_row > rg.end_row || rg.start_col > rg.end_col {
        return;
    }

    // 머리글 행 필터 단추 등록 (보기 전용 표시)
    if tbl.header_row_count > 0 && tbl.has_auto_filter {
        let hdr_r = rg.start_row;
        for c in rg.start_col..=rg.end_col {
            cache.filter_buttons.insert(crate::cell_key(hdr_r, c));
        }
    }

    let (accent, is_dark, is_medium) = resolve_builtin_table_accent(&tbl.style_name, styles);
    let header_bg = if is_dark {
        apply_color_tint(accent, -0.35)
    } else if is_medium {
        accent
    } else {
        apply_color_tint(accent, 0.80)
    };
    let header_fg = if is_dark || is_medium {
        Rgba::WHITE
    } else {
        Rgba::BLACK
    };
    let stripe_bg = apply_color_tint(accent, 0.85);
    let totals_bg = apply_color_tint(accent, 0.72);

    let data_start_r = rg.start_row + tbl.header_row_count;
    let data_end_r = rg.end_row.saturating_sub(tbl.totals_row_count);

    for r in rg.start_row..=rg.end_row {
        let is_header = r < data_start_r;
        let is_totals = tbl.totals_row_count > 0 && r > data_end_r;
        let data_row_offset = r.saturating_sub(data_start_r);

        for c in rg.start_col..=rg.end_col {
            // 셀 자체에 이미 명시적 채우기(fill_id != 0)가 지정되어 있으면 셀 서식이 이긴다.
            let cell_has_own_fill = sheet
                .data
                .get_cell(r, c)
                .and_then(|cell| styles.cell_xfs.get(cell.style_idx as usize))
                .and_then(|xf| styles.fills.get(xf.fill_id as usize))
                .map(|f| f.pattern_type == "solid" && f.fg_color.is_some())
                .unwrap_or(false);

            let key = crate::cell_key(r, c);
            let entry = cache.cells.entry(key).or_default();

            if is_header {
                if !cell_has_own_fill && entry.bg_color.is_none() {
                    entry.bg_color = Some(header_bg);
                }
                if entry.text_color.is_none() && (is_dark || is_medium) {
                    entry.text_color = Some(header_fg);
                }
                if entry.font_bold.is_none() {
                    entry.font_bold = Some(true);
                }
            } else if is_totals {
                if !cell_has_own_fill && entry.bg_color.is_none() {
                    entry.bg_color = Some(totals_bg);
                }
                if entry.font_bold.is_none() {
                    entry.font_bold = Some(true);
                }
            } else {
                let col_offset = c - rg.start_col;
                let row_striped = tbl.show_row_stripes && (data_row_offset % 2 == 0);
                let col_striped = tbl.show_column_stripes && (col_offset % 2 == 0);
                if (row_striped || col_striped) && !cell_has_own_fill && entry.bg_color.is_none() {
                    entry.bg_color = Some(stripe_bg);
                }
                if (tbl.show_first_column && c == rg.start_col)
                    || (tbl.show_last_column && c == rg.end_col)
                {
                    if entry.font_bold.is_none() {
                        entry.font_bold = Some(true);
                    }
                }
            }
        }
    }
}

// ============================================================================
// 8. P4 필수 단위 시험 (#[test])
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stats(values: &[f64]) -> RangeStats {
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let min = *sorted.first().unwrap_or(&0.0);
        let max = *sorted.last().unwrap_or(&0.0);
        let mean = if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f64>() / (values.len() as f64)
        };
        let var = if values.is_empty() {
            0.0
        } else {
            values.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (values.len() as f64)
        };
        let mut vc = HashMap::new();
        for &v in values {
            *vc.entry(format!("N:{:.12}", v)).or_insert(0) += 1;
        }
        RangeStats {
            nums: values.to_vec(),
            sorted_nums: sorted,
            min,
            max,
            mean,
            std_dev: var.sqrt(),
            value_counts: vc,
        }
    }

    #[test]
    fn test_00_and_06_sheet_databar_num_0_to_1() {
        // 샘플 00 시트 I10:I15 (0~1, #0F766E) 및 06 시트 J열 (0~1, #10B981)
        let stats = make_stats(&[0.2, 0.5, 0.8]);
        let spec = DataBarSpec {
            min_cfvo: Cfvo {
                cfvo_type: CfvoType::Num,
                val: Some("0".into()),
                gte: true,
            },
            max_cfvo: Cfvo {
                cfvo_type: CfvoType::Num,
                val: Some("1".into()),
                gte: true,
            },
            color: Rgba::from_hex("0F766E").unwrap(),
            ..Default::default()
        };

        let bar = evaluate_data_bar(&spec, &stats, 0.65).unwrap();
        assert!((bar.start_ratio - 0.0).abs() < 1e-6);
        assert!((bar.end_ratio - 0.65).abs() < 1e-6);
        assert_eq!(bar.color, Rgba::from_hex("0F766E").unwrap());
        assert!(bar.axis_ratio.is_none());
    }

    #[test]
    fn test_01_sheet_databar_min_to_max_and_negative_axis() {
        // 1) 양수 min~max (01 시트 N열 #3B82F6)
        let stats_pos = make_stats(&[10.0, 30.0, 50.0]);
        let spec = DataBarSpec {
            min_cfvo: Cfvo {
                cfvo_type: CfvoType::Min,
                val: None,
                gte: true,
            },
            max_cfvo: Cfvo {
                cfvo_type: CfvoType::Max,
                val: None,
                gte: true,
            },
            color: Rgba::from_hex("3B82F6").unwrap(),
            ..Default::default()
        };
        let bar_mid = evaluate_data_bar(&spec, &stats_pos, 30.0).unwrap();
        assert!((bar_mid.end_ratio - 0.5).abs() < 1e-6);

        // 2) 음수 포함 시 (-20 ~ +80) -> 0 기준축은 0.2 위치, -10은 0.1..0.2 구간
        let stats_neg = make_stats(&[-20.0, -10.0, 40.0, 80.0]);
        let bar_neg = evaluate_data_bar(&spec, &stats_neg, -10.0).unwrap();
        assert_eq!(bar_neg.axis_ratio, Some(0.2));
        assert!((bar_neg.start_ratio - 0.1).abs() < 1e-6);
        assert!((bar_neg.end_ratio - 0.2).abs() < 1e-6);
    }

    #[test]
    fn test_01_04_06_sheet_3color_scale_separate_interpolation() {
        // 샘플 06 시트 G열: min(-10) · 0(num=0, 흰색) · max(20)
        // 양쪽 구간의 길이가 달라도 왼쪽(-10..0)과 오른쪽(0..20)을 따로 보간해야 한다.
        let stats = make_stats(&[-10.0, -5.0, 0.0, 10.0, 20.0]);
        let c_min = Rgba::from_hex("FCA5A5").unwrap(); // (252, 165, 165)
        let c_mid = Rgba::from_hex("FFFFFF").unwrap(); // (255, 255, 255)
        let c_max = Rgba::from_hex("86EFAC").unwrap(); // (134, 239, 172)

        let spec = ColorScaleSpec {
            cfvos: vec![
                Cfvo {
                    cfvo_type: CfvoType::Min,
                    val: None,
                    gte: true,
                },
                Cfvo {
                    cfvo_type: CfvoType::Num,
                    val: Some("0".into()),
                    gte: true,
                },
                Cfvo {
                    cfvo_type: CfvoType::Max,
                    val: None,
                    gte: true,
                },
            ],
            colors: vec![c_min, c_mid, c_max],
        };

        // 0.0 은 정확히 중간 색(흰색)이어야 한다
        assert_eq!(evaluate_color_scale(&spec, &stats, 0.0).unwrap(), c_mid);

        // -5.0 은 min(-10)과 0의 정중앙(t=0.5)
        let neg_half = evaluate_color_scale(&spec, &stats, -5.0).unwrap();
        assert_eq!(neg_half, Rgba::lerp(c_min, c_mid, 0.5));

        // +10.0 은 0과 max(20)의 정중앙(t=0.5)
        let pos_half = evaluate_color_scale(&spec, &stats, 10.0).unwrap();
        assert_eq!(pos_half, Rgba::lerp(c_mid, c_max, 0.5));
    }

    #[test]
    fn test_3arrows_icon_set_thresholds_and_reverse() {
        // 임계값 2개(-15, 15) 기준 3Arrows 판정
        let stats = make_stats(&[-30.0, 0.0, 25.0]);
        let mut spec = IconSetSpec {
            kind: IconSetKind::Arrows3,
            cfvos: vec![
                Cfvo {
                    cfvo_type: CfvoType::Percent,
                    val: Some("0".into()),
                    gte: true,
                },
                Cfvo {
                    cfvo_type: CfvoType::Num,
                    val: Some("-15".into()),
                    gte: true,
                },
                Cfvo {
                    cfvo_type: CfvoType::Num,
                    val: Some("15".into()),
                    gte: true,
                },
            ],
            reverse: false,
            show_value: true,
        };

        // -20 < -15 -> 0(아래 화살표)
        assert_eq!(evaluate_icon_set(&spec, &stats, -20.0).unwrap().index, 0);
        // -15 <= 0 < 15 -> 1(오른쪽 화살표)
        assert_eq!(evaluate_icon_set(&spec, &stats, -15.0).unwrap().index, 1);
        assert_eq!(evaluate_icon_set(&spec, &stats, 0.0).unwrap().index, 1);
        // 15 >= 15 -> 2(위 화살표)
        assert_eq!(evaluate_icon_set(&spec, &stats, 15.0).unwrap().index, 2);

        // reverse=true 이면 위/아래가 뒤집힌다
        spec.reverse = true;
        assert_eq!(evaluate_icon_set(&spec, &stats, -20.0).unwrap().index, 2);
        assert_eq!(evaluate_icon_set(&spec, &stats, 15.0).unwrap().index, 0);
    }

    #[test]
    fn test_top10_above_average_and_duplicate_rules() {
        let stats = make_stats(&[10.0, 20.0, 20.0, 40.0, 60.0]); // 평균 = 30.0
        let dummy_wb = Workbook {
            path: String::new(),
            date1904: false,
            full_calc_on_load: false,
            defined_names: Vec::new(),
            styles: StyleSheet::default_with_theme(Vec::new()),
            pool: crate::StringPool::default(),
            sheets: Vec::new(),
        };
        let dummy_calc = WorkbookCalcResult {
            values: HashMap::new(),
            dynamic_recalc_cells: HashSet::new(),
            circular_warnings: Vec::new(),
        };

        // Top 2: 40, 60 만 통과
        let top2 = CfRuleKind::Top10 {
            bottom: false,
            percent: false,
            rank: 2,
        };
        assert!(evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(40.0),
            &top2,
            &stats
        ));
        assert!(!evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(20.0),
            &top2,
            &stats
        ));

        // AboveAverage (평균 30 초과)
        let above_avg = CfRuleKind::AboveAverage {
            above: true,
            equal_average: false,
            std_dev: None,
        };
        assert!(evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(40.0),
            &above_avg,
            &stats
        ));
        assert!(!evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(20.0),
            &above_avg,
            &stats
        ));

        // DuplicateValues: 20.0 은 2번 등장하므로 true, 10.0 은 false
        assert!(evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(20.0),
            &CfRuleKind::DuplicateValues,
            &stats
        ));
        assert!(!evaluate_boolean_cf_rule(
            &dummy_wb,
            &dummy_calc,
            0,
            1,
            1,
            1,
            1,
            &Value::Num(10.0),
            &CfRuleKind::DuplicateValues,
            &stats
        ));
    }
}

