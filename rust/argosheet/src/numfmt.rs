use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::collections::HashMap;

// ============================================================================
// 1. 서식 AST 및 토큰 정의
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,  // >
    Gte, // >=
    Lt,  // <
    Lte, // <=
    Eq,  // =
    Neq, // <>
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionCondition {
    pub op: CmpOp,
    pub val: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FmtColor {
    Black,
    White,
    Red,
    Green,
    Blue,
    Yellow,
    Magenta,
    Cyan,
    Indexed(u8), // [Color 1..56]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateToken {
    Year4,     // yyyy
    Year2,     // yy
    MonthNum,  // m (문맥상 월)
    Month2,    // mm
    MonthShort,// mmm
    MonthLong, // mmmm
    MonthChar, // mmmmm
    DayNum,    // d
    Day2,      // dd
    DayShort,  // ddd
    DayLong,   // dddd
    DayKorShort,// aaa
    DayKorLong, // aaaa
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeToken {
    Hour12,    // h (1~12)
    Hour24,    // h (0~23)
    Hour12_2,  // hh (01~12)
    Hour24_2,  // hh (00~23)
    MinuteNum, // m (문맥상 분)
    Minute2,   // mm
    SecondNum, // s
    Second2,   // ss
    DeciSecond,// .0
    CentiSecond,// .00
    MilliSecond,// .000
    AmPmUpper, // AM/PM
    AmPmLower, // a/p
    ElapsedHour,   // [h]
    ElapsedMinute, // [m]
    ElapsedSecond, // [s]
}

#[derive(Debug, Clone, PartialEq)]
pub enum FmtToken {
    Zero,                // 0
    Hash,                // #
    Question,            // ?
    DecimalPoint,        // .
    GroupingComma,       // , (숫자 사이의 자릿수 구분 기호)
    Percent,             // % (100 곱하기)
    Exponent { plus: bool }, // E+, E-
    FractionSlash,       // / (분수 구분)
    Literal(String),     // 문자열 리터럴, 이스케이프 문자 등
    SkipWidth(char),     // _x (x의 폭만큼 빈칸)
    FillChar(char),      // *x (남은 폭 채우기)
    At,                  // @ (문자열 자리 표시자)
    Date(DateToken),
    Time(TimeToken),
    General,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NumFmtSection {
    pub condition: Option<SectionCondition>,
    pub color: Option<FmtColor>,
    pub locale_currency: Option<(String, String)>, // (통화기호, 로케일코드) 예: ("$", "409"), ("₩", "412")
    pub dbnum: Option<u8>,                         // [DBNum1..4]
    pub tokens: Vec<FmtToken>,
    pub is_date_time: bool,
    pub has_time: bool,
    pub has_ampm: bool,
    pub min_int_digits: usize,
    pub min_frac_digits: usize,
    pub max_frac_digits: usize,
    pub has_grouping: bool,
    pub scale_div: f64, // 후행 쉼표로 인한 축척 나누기(예: , -> 1,000으로 나누기)
    pub percent_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledFormat {
    pub raw_code: String,
    pub sections: Vec<NumFmtSection>,
}

// ============================================================================
// 2. Lexer & Parser (정규식 배제, 순수 상태 머신)
// ============================================================================

pub struct FormatCompiler;

impl FormatCompiler {
    pub fn compile(format_code: &str) -> CompiledFormat {
        let code = format_code.trim();
        if code.eq_ignore_ascii_case("general") || code.is_empty() {
            return CompiledFormat {
                raw_code: code.to_string(),
                sections: vec![Self::create_general_section()],
            };
        }

        let mut sections = Vec::with_capacity(4);
        let raw_sections = split_sections(code);

        for s_str in raw_sections {
            sections.push(Self::parse_section(&s_str));
        }

        CompiledFormat {
            raw_code: code.to_string(),
            sections,
        }
    }

    fn create_general_section() -> NumFmtSection {
        NumFmtSection {
            condition: None,
            color: None,
            locale_currency: None,
            dbnum: None,
            tokens: vec![FmtToken::General],
            is_date_time: false,
            has_time: false,
            has_ampm: false,
            min_int_digits: 0,
            min_frac_digits: 0,
            max_frac_digits: 0,
            has_grouping: false,
            scale_div: 1.0,
            percent_count: 0,
        }
    }

    fn parse_section(input: &str) -> NumFmtSection {
        let chars: Vec<char> = input.chars().collect();
        let len = chars.len();
        let mut i = 0;

        let mut condition: Option<SectionCondition> = None;
        let mut color: Option<FmtColor> = None;
        let mut locale_currency: Option<(String, String)> = None;
        let mut dbnum: Option<u8> = None;
        let mut raw_tokens: Vec<FmtToken> = Vec::new();

        while i < len {
            let ch = chars[i];

            // 1. 대괄호 토큰 처리: [Red], [Color 10], [>=100], [$-412], [$₩-412], [DBNum1], [h]
            if ch == '[' {
                if let Some(rel) = chars[i..].iter().position(|c| *c == ']') {
                    let inside: String = chars[i + 1..i + rel].iter().collect();
                    let inside = inside.as_str();
                    i += rel + 1;

                    // 경과 시간 [h], [m], [s]
                    if inside.eq_ignore_ascii_case("h") {
                        raw_tokens.push(FmtToken::Time(TimeToken::ElapsedHour));
                        continue;
                    } else if inside.eq_ignore_ascii_case("m") {
                        raw_tokens.push(FmtToken::Time(TimeToken::ElapsedMinute));
                        continue;
                    } else if inside.eq_ignore_ascii_case("s") {
                        raw_tokens.push(FmtToken::Time(TimeToken::ElapsedSecond));
                        continue;
                    }

                    // 조건부 [>=100]
                    if let Some(cond) = parse_condition_bracket(inside) {
                        condition = Some(cond);
                        continue;
                    }

                    // 색상 [Red], [Color 10]
                    if let Some(c) = parse_color_bracket(inside) {
                        color = Some(c);
                        continue;
                    }

                    // 통화 및 로케일 [$-412], [$₩-412]
                    if inside.starts_with('$') {
                        let inner = &inside[1..];
                        if let Some((curr, loc)) = inner.split_once('-') {
                            locale_currency = Some((curr.to_string(), loc.to_string()));
                        } else {
                            locale_currency = Some((inner.to_string(), String::new()));
                        }
                        continue;
                    }

                    // DBNum [DBNum1] ~ [DBNum4]
                    if inside.to_ascii_uppercase().starts_with("DBNUM") {
                        if let Ok(n) = inside[5..].parse::<u8>() {
                            dbnum = Some(n);
                            continue;
                        }
                    }

                    // 기타 알 수 없는 대괄호는 무시하거나 리터럴 취급
                    continue;
                }
            }

            // 2. 따옴표 리터럴 "..."
            if ch == '"' {
                i += 1;
                let mut lit = String::new();
                while i < len {
                    if chars[i] == '"' {
                        if i + 1 < len && chars[i + 1] == '"' {
                            lit.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        lit.push(chars[i]);
                        i += 1;
                    }
                }
                raw_tokens.push(FmtToken::Literal(lit));
                continue;
            }

            // 3. 이스케이프 문자 \x
            if ch == '\\' {
                if i + 1 < len {
                    raw_tokens.push(FmtToken::Literal(chars[i + 1].to_string()));
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            // 4. 폭 건너뛰기 _x
            if ch == '_' {
                if i + 1 < len {
                    raw_tokens.push(FmtToken::SkipWidth(chars[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            // 5. 폭 채우기 *x
            if ch == '*' {
                if i + 1 < len {
                    raw_tokens.push(FmtToken::FillChar(chars[i + 1]));
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }

            // 6. AM/PM 및 오전/오후
            if i + 4 < len && chars[i..i + 5].iter().collect::<String>().eq_ignore_ascii_case("AM/PM") {
                raw_tokens.push(FmtToken::Time(TimeToken::AmPmUpper));
                i += 5;
                continue;
            }
            if i + 2 < len && chars[i..i + 3].iter().collect::<String>().eq_ignore_ascii_case("A/P") {
                raw_tokens.push(FmtToken::Time(TimeToken::AmPmLower));
                i += 3;
                continue;
            }

            // 7. 지수 표기 E+, E-, e+, e-
            if (ch == 'E' || ch == 'e') && i + 1 < len && (chars[i + 1] == '+' || chars[i + 1] == '-') {
                let plus = chars[i + 1] == '+';
                raw_tokens.push(FmtToken::Exponent { plus });
                i += 2;
                continue;
            }

            // 8. 날짜 및 시간 복합 토큰 (yyyy, yy, ddd, dddd, aaa, aaaa 등)
            match ch {
                'y' | 'Y' => {
                    let count = count_consecutive(&chars, i, ch);
                    i += count;
                    if count >= 4 {
                        raw_tokens.push(FmtToken::Date(DateToken::Year4));
                    } else {
                        raw_tokens.push(FmtToken::Date(DateToken::Year2));
                    }
                    continue;
                }
                'd' | 'D' => {
                    let count = count_consecutive(&chars, i, ch);
                    i += count;
                    match count {
                        1 => raw_tokens.push(FmtToken::Date(DateToken::DayNum)),
                        2 => raw_tokens.push(FmtToken::Date(DateToken::Day2)),
                        3 => raw_tokens.push(FmtToken::Date(DateToken::DayShort)),
                        _ => raw_tokens.push(FmtToken::Date(DateToken::DayLong)),
                    }
                    continue;
                }
                'a' | 'A' => {
                    let count = count_consecutive(&chars, i, ch);
                    if count == 3 {
                        raw_tokens.push(FmtToken::Date(DateToken::DayKorShort));
                        i += count;
                        continue;
                    } else if count >= 4 {
                        raw_tokens.push(FmtToken::Date(DateToken::DayKorLong));
                        i += count;
                        continue;
                    }
                }
                'h' | 'H' => {
                    let count = count_consecutive(&chars, i, ch);
                    i += count;
                    // AM/PM 존재 여부는 나중에 2차 패스에서 판별하여 Hour12 / Hour24 확정
                    if count >= 2 {
                        raw_tokens.push(FmtToken::Time(TimeToken::Hour24_2));
                    } else {
                        raw_tokens.push(FmtToken::Time(TimeToken::Hour24));
                    }
                    continue;
                }
                's' | 'S' => {
                    let count = count_consecutive(&chars, i, ch);
                    i += count;
                    if count >= 2 {
                        raw_tokens.push(FmtToken::Time(TimeToken::Second2));
                    } else {
                        raw_tokens.push(FmtToken::Time(TimeToken::SecondNum));
                    }
                    continue;
                }
                'm' | 'M' => {
                    // 'm'은 문맥에 따라 월(Month) 또는 분(Minute)이 된다.
                    let count = count_consecutive(&chars, i, ch);
                    i += count;
                    raw_tokens.push(match count {
                        1 => FmtToken::Date(DateToken::MonthNum),
                        2 => FmtToken::Date(DateToken::Month2),
                        3 => FmtToken::Date(DateToken::MonthShort),
                        4 => FmtToken::Date(DateToken::MonthLong),
                        _ => FmtToken::Date(DateToken::MonthChar),
                    });
                    continue;
                }
                _ => {}
            }

            // 9. 숫자 및 기호 단일 토큰
            match ch {
                '0' => raw_tokens.push(FmtToken::Zero),
                '#' => raw_tokens.push(FmtToken::Hash),
                '?' => raw_tokens.push(FmtToken::Question),
                '.' => raw_tokens.push(FmtToken::DecimalPoint),
                ',' => raw_tokens.push(FmtToken::GroupingComma),
                '%' => raw_tokens.push(FmtToken::Percent),
                '/' => raw_tokens.push(FmtToken::FractionSlash),
                '@' => raw_tokens.push(FmtToken::At),
                _ => {
                    // "General" 단어 매칭
                    if (ch == 'G' || ch == 'g')
                        && i + 6 < len
                        && chars[i..i + 7].iter().collect::<String>().eq_ignore_ascii_case("General")
                    {
                        raw_tokens.push(FmtToken::General);
                        i += 7;
                        continue;
                    }
                    raw_tokens.push(FmtToken::Literal(ch.to_string()));
                }
            }
            i += 1;
        }

        // 2차 패스: 문맥 해석 ('m'의 월/분 해석, AM/PM에 따른 Hour12 전환, 숫자 자리수 집계)
        refine_and_summarize_tokens(raw_tokens, condition, color, locale_currency, dbnum)
    }
}

fn split_sections(code: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut in_bracket = false;
    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        if ch == '\\' {
            cur.push(ch);
            if i + 1 < len {
                cur.push(chars[i + 1]);
                i += 2;
                continue;
            }
        } else if ch == '"' {
            in_quote = !in_quote;
            cur.push(ch);
        } else if !in_quote && ch == '[' {
            in_bracket = true;
            cur.push(ch);
        } else if !in_quote && ch == ']' {
            in_bracket = false;
            cur.push(ch);
        } else if !in_quote && !in_bracket && ch == ';' {
            sections.push(cur.clone());
            cur.clear();
            i += 1;
            continue;
        } else {
            cur.push(ch);
        }
        i += 1;
    }
    sections.push(cur);
    sections
}

fn count_consecutive(chars: &[char], start: usize, target: char) -> usize {
    let lower = target.to_ascii_lowercase();
    let mut c = 0;
    while start + c < chars.len() && chars[start + c].to_ascii_lowercase() == lower {
        c += 1;
    }
    c
}

fn parse_condition_bracket(s: &str) -> Option<SectionCondition> {
    let trimmed = s.trim();
    let (op, rest) = if trimmed.starts_with(">=") {
        (CmpOp::Gte, &trimmed[2..])
    } else if trimmed.starts_with("<=") {
        (CmpOp::Lte, &trimmed[2..])
    } else if trimmed.starts_with("<>") {
        (CmpOp::Neq, &trimmed[2..])
    } else if trimmed.starts_with('>') {
        (CmpOp::Gt, &trimmed[1..])
    } else if trimmed.starts_with('<') {
        (CmpOp::Lt, &trimmed[1..])
    } else if trimmed.starts_with('=') {
        (CmpOp::Eq, &trimmed[1..])
    } else {
        return None;
    };

    let val = rest.trim().parse::<f64>().ok()?;
    Some(SectionCondition { op, val })
}

fn parse_color_bracket(s: &str) -> Option<FmtColor> {
    let lower = s.trim().to_ascii_lowercase();
    match lower.as_str() {
        "black" => Some(FmtColor::Black),
        "white" => Some(FmtColor::White),
        "red" => Some(FmtColor::Red),
        "green" => Some(FmtColor::Green),
        "blue" => Some(FmtColor::Blue),
        "yellow" => Some(FmtColor::Yellow),
        "magenta" => Some(FmtColor::Magenta),
        "cyan" => Some(FmtColor::Cyan),
        _ => {
            if lower.starts_with("color") {
                let num_part = lower[5..].trim();
                if let Ok(idx) = num_part.parse::<u8>() {
                    return Some(FmtColor::Indexed(idx));
                }
            }
            None
        }
    }
}

fn refine_and_summarize_tokens(
    mut tokens: Vec<FmtToken>,
    condition: Option<SectionCondition>,
    color: Option<FmtColor>,
    locale_currency: Option<(String, String)>,
    dbnum: Option<u8>,
) -> NumFmtSection {
    let has_ampm = tokens.iter().any(|t| matches!(t, FmtToken::Time(TimeToken::AmPmUpper | TimeToken::AmPmLower)));
    let mut is_date_time = false;
    let mut has_time = false;

    // 1. m/mm 토큰의 월(Month) vs 분(Minute) 확정
    // 왜: 엑셀 규칙상 m/mm 바로 앞에 h/hh가 오거나, 바로 뒤에 s/ss가 오면 '분'으로 처리한다.
    let t_len = tokens.len();
    for idx in 0..t_len {
        let is_m = matches!(tokens[idx], FmtToken::Date(DateToken::MonthNum | DateToken::Month2));
        if is_m {
            let mut is_minute = false;
            // 직전 토큰 검사
            for prev in (0..idx).rev() {
                match &tokens[prev] {
                    FmtToken::Time(TimeToken::Hour12 | TimeToken::Hour24 | TimeToken::Hour12_2 | TimeToken::Hour24_2 | TimeToken::ElapsedHour) => {
                        is_minute = true;
                        break;
                    }
                    FmtToken::Literal(s) if s.trim().is_empty() || s == ":" => continue,
                    _ => break,
                }
            }
            // 직후 토큰 검사
            if !is_minute {
                for next in (idx + 1)..t_len {
                    match &tokens[next] {
                        FmtToken::Time(TimeToken::SecondNum | TimeToken::Second2) => {
                            is_minute = true;
                            break;
                        }
                        FmtToken::Literal(s) if s.trim().is_empty() || s == ":" => continue,
                        _ => break,
                    }
                }
            }

            if is_minute {
                let is_double = matches!(tokens[idx], FmtToken::Date(DateToken::Month2));
                tokens[idx] = FmtToken::Time(if is_double {
                    TimeToken::Minute2
                } else {
                    TimeToken::MinuteNum
                });
            }
        }

        // AM/PM 적용 시 Hour24를 Hour12로 변환
        if has_ampm {
            if let FmtToken::Time(TimeToken::Hour24) = tokens[idx] {
                tokens[idx] = FmtToken::Time(TimeToken::Hour12);
            } else if let FmtToken::Time(TimeToken::Hour24_2) = tokens[idx] {
                tokens[idx] = FmtToken::Time(TimeToken::Hour12_2);
            }
        }

        if matches!(tokens[idx], FmtToken::Date(_)) {
            is_date_time = true;
        }
        if matches!(tokens[idx], FmtToken::Time(_)) {
            is_date_time = true;
            has_time = true;
        }
    }

    // 2. 숫자 포맷 속성 계산 (정수부 최소 자릿수, 소수부 최소/최대 자릿수, 천 단위 쉼표, 축척)
    let mut min_int_digits = 0;
    let mut min_frac_digits = 0;
    let mut max_frac_digits = 0;
    let mut has_grouping = false;
    let mut percent_count = 0;
    let mut in_fraction = false;

    for tok in tokens.iter() {
        match tok {
            FmtToken::Zero => {
                if in_fraction {
                    min_frac_digits += 1;
                    max_frac_digits += 1;
                } else {
                    min_int_digits += 1;
                }
            }
            FmtToken::Hash | FmtToken::Question => {
                if in_fraction {
                    max_frac_digits += 1;
                }
            }
            FmtToken::DecimalPoint => in_fraction = true,
            FmtToken::GroupingComma => has_grouping = true,
            FmtToken::Percent => percent_count += 1,
            _ => {}
        }
    }

    // 축척 쉼표: 뒤에 숫자 자리(0 # ?)가 하나도 없는 쉼표만 1,000 으로 나눈다.
    // "#,##0" 의 쉼표는 자릿수 구분이고, "#,##0," 의 끝 쉼표가 축척이다.
    // 앞쪽만 보고 판단하면 "#,##0" 이 1,000 으로 나뉘어 1234.5 가 "1" 이 된다.
    let mut trailing_commas = 0;
    let mut seen_digit_after = false;
    for tok in tokens.iter().rev() {
        match tok {
            FmtToken::Zero | FmtToken::Hash | FmtToken::Question => seen_digit_after = true,
            FmtToken::GroupingComma if !seen_digit_after => trailing_commas += 1,
            _ => {}
        }
    }

    let mut scale_div = 1.0f64;
    for _ in 0..trailing_commas {
        scale_div *= 1000.0;
    }

    NumFmtSection {
        condition,
        color,
        locale_currency,
        dbnum,
        tokens,
        is_date_time,
        has_time,
        has_ampm,
        min_int_digits,
        min_frac_digits,
        max_frac_digits,
        has_grouping,
        scale_div,
        percent_count,
    }
}

// ============================================================================
// 3. 서식 실행기 (rust_decimal 기반 0에서 먼 쪽 반올림 & 1900 날짜 체계)
// ============================================================================

pub struct FormatEngine;

impl FormatEngine {
    /// 단일 값을 컴파일된 서식에 맞추어 문자열로 렌더링한다.
    pub fn format_value(value: &FormatValue, compiled: &CompiledFormat) -> String {
        let (idx, section) = select_section(value, &compiled.sections);
        // 음수 부호는 '음수 전용 칸'(두 번째 섹션)으로 갈 때만 뗀다. 그 칸의 서식이
        // 괄호나 - 를 들고 있기 때문이다. 섹션이 하나뿐이면 엑셀도 -1,234 로 찍는다.
        let suppress_sign = idx == 1 && compiled.sections.len() >= 2 && section.condition.is_none();
        format_with_section(value, section, suppress_sign)
    }
}

#[derive(Debug, Clone)]
pub enum FormatValue {
    Number(f64),
    Text(String),
    Bool(bool),
    Blank,
}

fn select_section<'a>(value: &FormatValue, sections: &'a [NumFmtSection]) -> (usize, &'a NumFmtSection) {
    if sections.is_empty() {
        static GENERAL: std::sync::OnceLock<NumFmtSection> = std::sync::OnceLock::new();
        return (0, GENERAL.get_or_init(FormatCompiler::create_general_section));
    }

    // 조건절 [>=100]이 있는 경우 순서대로 판별
    if let FormatValue::Number(num) = value {
        for (si, s) in sections.iter().enumerate() {
            if let Some(cond) = &s.condition {
                let matched = match cond.op {
                    CmpOp::Gt => *num > cond.val,
                    CmpOp::Gte => *num >= cond.val,
                    CmpOp::Lt => *num < cond.val,
                    CmpOp::Lte => *num <= cond.val,
                    CmpOp::Eq => (*num - cond.val).abs() < 1e-12,
                    CmpOp::Neq => (*num - cond.val).abs() >= 1e-12,
                };
                if matched {
                    return (si, s);
                }
            }
        }
    }

    let idx = match sections.len() {
        1 => 0,
        2 => match value {
            FormatValue::Number(n) if *n < 0.0 => 1,
            _ => 0,
        },
        3 => match value {
            FormatValue::Number(n) if *n > 0.0 => 0,
            FormatValue::Number(n) if *n < 0.0 => 1,
            FormatValue::Number(_) => 2,
            _ => 0,
        },
        _ => match value {
            FormatValue::Number(n) if *n > 0.0 => 0,
            FormatValue::Number(n) if *n < 0.0 => 1,
            FormatValue::Number(_) => 2,
            FormatValue::Text(_) => 3,
            _ => 0,
        },
    };
    (idx, &sections[idx])
}

fn format_with_section(value: &FormatValue, section: &NumFmtSection, suppress_sign: bool) -> String {
    // 1. 텍스트 값 처리
    if let FormatValue::Text(t) = value {
        let mut out = String::new();
        let mut has_at = false;
        for tok in &section.tokens {
            match tok {
                FmtToken::At => {
                    out.push_str(t);
                    has_at = true;
                }
                FmtToken::Literal(lit) => out.push_str(lit),
                FmtToken::SkipWidth(_) => out.push(' '),
                _ => {}
            }
        }
        if !has_at {
            return t.clone();
        }
        return out;
    }

    // 2. 날짜/시간 포맷팅 (1900 윤년 버그 처리 포함)
    if section.is_date_time {
        if let FormatValue::Number(serial) = value {
            return format_date_time(*serial, section);
        }
    }

    // 3. 숫자 포맷팅
    match value {
        FormatValue::Number(raw_n) => {
            // General 포맷 처리
            if section.tokens.iter().any(|t| matches!(t, FmtToken::General)) {
                return format_general(*raw_n);
            }
            format_numeric(*raw_n, section, suppress_sign)
        }
        FormatValue::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
        FormatValue::Blank => String::new(),
        _ => String::new(),
    }
}

// ============================================================================
// 4. 숫자 상세 포맷팅 (rust_decimal 기반 0에서 먼 쪽 반올림)
// ============================================================================

fn format_numeric(mut raw_n: f64, section: &NumFmtSection, suppress_sign: bool) -> String {
    let negative = raw_n < 0.0;
    if negative {
        raw_n = raw_n.abs();
    }

    // 축척 나누기 적용 (쉼표 1개당 1,000 나누기)
    if section.scale_div > 1.0 {
        raw_n /= section.scale_div;
    }

    // 퍼센트 적용 (% 개수당 100 곱하기)
    for _ in 0..section.percent_count {
        raw_n *= 100.0;
    }

    // 왜: 엑셀은 15자리 유효숫자로 부동소수점 오차를 먼저 정리한 후,
    // rust_decimal::RoundingStrategy::AwayFromZero(0에서 먼 쪽)로 소수점 반올림을 수행한다.
    let dec = match Decimal::from_f64_retain(raw_n) {
        Some(d) => d,
        None => return raw_n.to_string(),
    };

    let scale = section.max_frac_digits as u32;
    // 엑셀은 ① 유효숫자 15자리로 먼저 정리하고 ② 5 이상일 때만 0에서 먼 쪽으로 올린다.
    // ①이 없으면 1.005(이진수로는 1.00499…)가 1.00 이 되고,
    // ②를 AwayFromZero(무조건 올림)로 하면 1,234,567.891 이 .90 이 된다.
    let rounded = round_sig15(dec).round_dp_with_strategy(scale, RoundingStrategy::MidpointAwayFromZero);

    // 정수부 및 소수부 문자열 분리
    let abs_rounded = rounded.abs();
    let int_part_str = abs_rounded.trunc().to_string();
    let mut frac_part_str = if scale > 0 {
        let mut frac = abs_rounded.fract().to_string();
        // fract()는 "0.125" 형태이므로 "0." 접두어 제거
        if let Some(stripped) = frac.strip_prefix("0.") {
            stripped.to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // 정수부 최소 자릿수(min_int_digits) 채우기 (앞자리 0)
    let padded_int = if int_part_str.len() < section.min_int_digits {
        format!("{:0>width$}", int_part_str, width = section.min_int_digits)
    } else {
        int_part_str
    };

    // 천 단위 쉼표 삽입
    let grouped_int = if section.has_grouping {
        insert_grouping_separators(&padded_int)
    } else {
        padded_int
    };

    // 소수부 최소 자릿수(min_frac_digits)에 맞게 0 채우기 또는 후행 0 정리
    while frac_part_str.len() < section.min_frac_digits {
        frac_part_str.push('0');
    }
    // min_frac_digits보다 길고 max_frac_digits 이하인 경우, 불필요한 후행 0 제거(# 패턴)
    while frac_part_str.len() > section.min_frac_digits && frac_part_str.ends_with('0') {
        frac_part_str.pop();
    }

    // 토큰 결합 출력 버퍼 생성
    let mut out = String::new();
    if negative && !suppress_sign {
        out.push('-');
    }
    let mut int_emitted = false;
    let mut frac_emitted = false;

    for tok in &section.tokens {
        match tok {
            FmtToken::Zero | FmtToken::Hash | FmtToken::Question => {
                if !int_emitted {
                    out.push_str(&grouped_int);
                    int_emitted = true;
                }
            }
            FmtToken::GroupingComma => {
                // 이미 insert_grouping_separators에서 처리되었으므로 출력 생략
            }
            FmtToken::DecimalPoint => {
                if !frac_part_str.is_empty() || section.min_frac_digits > 0 {
                    out.push('.');
                    out.push_str(&frac_part_str);
                    frac_emitted = true;
                }
            }
            FmtToken::Percent => out.push('%'),
            FmtToken::Literal(lit) => out.push_str(lit),
            FmtToken::SkipWidth(_) => out.push(' '),
            _ => {}
        }
    }

    out
}

/// 엑셀 정밀도: 유효숫자 15자리로 맞춘다. 이진 부동소수점 찌꺼기를 먼저 턴다.
pub(crate) fn round_sig15(dec: Decimal) -> Decimal {
    let abs = dec.abs();
    if abs.is_zero() {
        return dec;
    }
    // 정수부 자릿수를 센 뒤 남는 자리만큼 소수점을 남긴다.
    let mut int_digits = 0i32;
    let mut v = abs.trunc();
    while v >= Decimal::ONE {
        v /= Decimal::from(10);
        v = v.trunc();
        int_digits += 1;
    }
    let keep = (15 - int_digits).clamp(0, 28) as u32;
    dec.round_dp_with_strategy(keep, RoundingStrategy::MidpointAwayFromZero)
}

fn insert_grouping_separators(int_str: &str) -> String {
    let bytes = int_str.as_bytes();
    let len = bytes.len();
    if len <= 3 {
        return int_str.to_string();
    }

    let mut out = String::with_capacity(len + (len / 3));
    let first_comma = len % 3;
    if first_comma > 0 {
        out.push_str(&int_str[..first_comma]);
        if first_comma < len {
            out.push(',');
        }
    }

    let mut i = first_comma;
    while i < len {
        if i > first_comma {
            out.push(',');
        }
        out.push_str(&int_str[i..i + 3]);
        i += 3;
    }
    out
}

fn format_general(raw_n: f64) -> String {
    if raw_n == 0.0 {
        return "0".to_string();
    }
    // 소수점 없는 정수
    if raw_n.fract() == 0.0 && raw_n.abs() < 1e12 {
        return format!("{:.0}", raw_n);
    }
    // 극단적인 수치는 지수 표기
    if raw_n.abs() >= 1e11 || raw_n.abs() < 1e-9 {
        return format!("{:.5E}", raw_n);
    }

    let dec = match Decimal::from_f64_retain(raw_n) {
        Some(d) => d.normalize(),
        None => return raw_n.to_string(),
    };
    dec.to_string()
}

// ============================================================================
// 5. 날짜 및 시간 포맷팅 (1900 윤년 버그 재현 공식)
// ============================================================================

fn format_date_time(serial: f64, section: &NumFmtSection) -> String {
    let int_days = serial.floor() as i64;
    let fract_day = serial - serial.floor();

    let (year, month, day) = excel_serial_to_ymd(int_days);
    let (hour24, minute, second, milli) = excel_fract_to_time(fract_day);

    let mut out = String::new();

    for tok in &section.tokens {
        match tok {
            FmtToken::Date(d) => match d {
                DateToken::Year4 => out.push_str(&format!("{:04}", year)),
                DateToken::Year2 => out.push_str(&format!("{:02}", year % 100)),
                DateToken::MonthNum => out.push_str(&month.to_string()),
                DateToken::Month2 => out.push_str(&format!("{:02}", month)),
                DateToken::MonthShort => out.push_str(month_to_short_str(month)),
                DateToken::MonthLong => out.push_str(month_to_long_str(month)),
                DateToken::MonthChar => out.push(month_to_char(month)),
                DateToken::DayNum => out.push_str(&day.to_string()),
                DateToken::Day2 => out.push_str(&format!("{:02}", day)),
                DateToken::DayShort => out.push_str(day_of_week_short(year, month, day)),
                DateToken::DayLong => out.push_str(day_of_week_long(year, month, day)),
                DateToken::DayKorShort => out.push_str(day_of_week_kor_short(year, month, day)),
                DateToken::DayKorLong => out.push_str(day_of_week_kor_long(year, month, day)),
            },
            FmtToken::Time(t) => match t {
                TimeToken::Hour12 => {
                    let h12 = if hour24 % 12 == 0 { 12 } else { hour24 % 12 };
                    out.push_str(&h12.to_string());
                }
                TimeToken::Hour12_2 => {
                    let h12 = if hour24 % 12 == 0 { 12 } else { hour24 % 12 };
                    out.push_str(&format!("{:02}", h12));
                }
                TimeToken::Hour24 => out.push_str(&hour24.to_string()),
                TimeToken::Hour24_2 => out.push_str(&format!("{:02}", hour24)),
                TimeToken::MinuteNum => out.push_str(&minute.to_string()),
                TimeToken::Minute2 => out.push_str(&format!("{:02}", minute)),
                TimeToken::SecondNum => out.push_str(&second.to_string()),
                TimeToken::Second2 => out.push_str(&format!("{:02}", second)),
                TimeToken::DeciSecond => out.push_str(&format!(".{:01}", milli / 100)),
                TimeToken::CentiSecond => out.push_str(&format!(".{:02}", milli / 10)),
                TimeToken::MilliSecond => out.push_str(&format!(".{:03}", milli)),
                TimeToken::AmPmUpper => out.push_str(if hour24 >= 12 { "PM" } else { "AM" }),
                TimeToken::AmPmLower => out.push_str(if hour24 >= 12 { "p" } else { "a" }),
                TimeToken::ElapsedHour => {
                    let total_hours = int_days * 24 + hour24 as i64;
                    out.push_str(&total_hours.to_string());
                }
                TimeToken::ElapsedMinute => {
                    let total_mins = (int_days * 24 + hour24 as i64) * 60 + minute as i64;
                    out.push_str(&total_mins.to_string());
                }
                TimeToken::ElapsedSecond => {
                    let total_secs = ((int_days * 24 + hour24 as i64) * 60 + minute as i64) * 60 + second as i64;
                    out.push_str(&total_secs.to_string());
                }
            },
            FmtToken::Literal(lit) => out.push_str(lit),
            FmtToken::SkipWidth(_) => out.push(' '),
            _ => {}
        }
    }

    out
}

/// 엑셀 1900 날짜 체계 일련번호를 (연, 월, 일)로 변환한다.
/// 왜: 엑셀은 1900년이 윤년이라는 Lotus 1-2-3의 가짜 버그(일련번호 60 = 1900-02-29)를 그대로 재현한다.
/// 60은 1900-02-29로 특수 처리하고, 61(1900-03-01)부터는 하루를 빼서 정규 그레고리력 공식으로 계산한다.
fn excel_serial_to_ymd(serial: i64) -> (i32, u32, u32) {
    if serial <= 0 {
        return (1900, 1, 0);
    }
    if serial == 60 {
        return (1900, 2, 29);
    }

    let adjusted_days = if serial > 60 { serial - 1 } else { serial };

    // 1900-01-01부터의 누적 일수를 유클리드 나눗셈 알고리즘으로 (Y, M, D) 계산
    // 엑셀 일련번호 1 = 1900-01-01 (JDN 2,415,021). 2415019 로 두면 하루씩 밀린다.
    let l = adjusted_days + 68569 + 2415020;
    let n = (4 * l) / 146097;
    let l = l - (146097 * n + 3) / 4;
    let i = (4000 * (l + 1)) / 1461001;
    let l = l - (1461 * i) / 4 + 31;
    let j = (80 * l) / 2447;
    let day = l - (2447 * j) / 80;
    let l = j / 11;
    let month = j + 2 - 12 * l;
    let year = 100 * (n - 49) + i + l;

    (year as i32, month as u32, day as u32)
}

fn excel_fract_to_time(fract: f64) -> (u32, u32, u32, u32) {
    let total_secs_f = (fract * 86400.0).round();
    let total_secs = total_secs_f as u64;

    let hour = ((total_secs / 3600) % 24) as u32;
    let minute = ((total_secs % 3600) / 60) as u32;
    let second = (total_secs % 60) as u32;
    (hour, minute, second, 0)
}

fn month_to_short_str(m: u32) -> &'static str {
    match m {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr", 5 => "May", 6 => "Jun",
        7 => "Jul", 8 => "Aug", 9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "",
    }
}

fn month_to_long_str(m: u32) -> &'static str {
    match m {
        1 => "January", 2 => "February", 3 => "March", 4 => "April", 5 => "May", 6 => "June",
        7 => "July", 8 => "August", 9 => "September", 10 => "October", 11 => "November", 12 => "December",
        _ => "",
    }
}

fn month_to_char(m: u32) -> char {
    match m {
        1 => 'J', 2 => 'F', 3 => 'M', 4 => 'A', 5 => 'M', 6 => 'J',
        7 => 'J', 8 => 'A', 9 => 'S', 10 => 'O', 11 => 'N', 12 => 'D',
        _ => ' ',
    }
}

fn day_of_week_zeller(y: i32, m: u32, d: u32) -> usize {
    let (adj_y, adj_m) = if m < 3 { (y - 1, m + 12) } else { (y, m) };
    let k = adj_y % 100;
    let j = adj_y / 100;
    let h = (d as i32 + (13 * (adj_m as i32 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // h: 0=토, 1=일, 2=월, 3=화, 4=수, 5=목, 6=금
    match h {
        0 => 6, // 토
        1 => 0, // 일
        2 => 1, // 월
        3 => 2, // 화
        4 => 3, // 수
        5 => 4, // 목
        6 => 5, // 금
        _ => 0,
    }
}

fn day_of_week_short(y: i32, m: u32, d: u32) -> &'static str {
    match day_of_week_zeller(y, m, d) {
        0 => "Sun", 1 => "Mon", 2 => "Tue", 3 => "Wed", 4 => "Thu", 5 => "Fri", 6 => "Sat",
        _ => "",
    }
}

fn day_of_week_long(y: i32, m: u32, d: u32) -> &'static str {
    match day_of_week_zeller(y, m, d) {
        0 => "Sunday", 1 => "Monday", 2 => "Tuesday", 3 => "Wednesday", 4 => "Thursday", 5 => "Friday", 6 => "Saturday",
        _ => "",
    }
}

fn day_of_week_kor_short(y: i32, m: u32, d: u32) -> &'static str {
    match day_of_week_zeller(y, m, d) {
        0 => "일", 1 => "월", 2 => "화", 3 => "수", 4 => "목", 5 => "금", 6 => "토",
        _ => "",
    }
}

fn day_of_week_kor_long(y: i32, m: u32, d: u32) -> &'static str {
    match day_of_week_zeller(y, m, d) {
        0 => "일요일", 1 => "월요일", 2 => "화요일", 3 => "수요일", 4 => "목요일", 5 => "금요일", 6 => "토요일",
        _ => "",
    }
}

// ============================================================================
// 6. 내장 표준 서식 번호 (numFmtId 0~49 매핑)
// ============================================================================

pub fn builtin_format_code(num_fmt_id: u32) -> &'static str {
    match num_fmt_id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "yyyy-mm-dd",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "yyyy-mm-dd h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => "General",
    }
}

// ============================================================================
// 7. P1-b 필수 단위 시험 (12종 벡터 전량 포함)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt_num(v: f64, pattern: &str) -> String {
        let compiled = FormatCompiler::compile(pattern);
        FormatEngine::format_value(&FormatValue::Number(v), &compiled)
    }

    #[test]
    fn test_vector_01_comma_round_up() {
        // (1234.5, "#,##0") -> "1,235"
        assert_eq!(fmt_num(1234.5, "#,##0"), "1,235");
    }

    #[test]
    fn test_vector_02_negative_currency_parens() {
        // (-1234.5, "$#,##0;($#,##0);\"-\"") -> "($1,235)"
        assert_eq!(fmt_num(-1234.5, "$#,##0;($#,##0);\"-\""), "($1,235)");
    }

    #[test]
    fn test_vector_03_zero_dash() {
        // (0, "$#,##0;($#,##0);\"-\"") -> "-"
        assert_eq!(fmt_num(0.0, "$#,##0;($#,##0);\"-\""), "-");
    }

    #[test]
    fn test_vector_04_away_from_zero_rounding_point125() {
        // (0.125, "0.00") -> "0.13" (0에서 먼 쪽 반올림, Banker's rounding 0.12 방지)
        assert_eq!(fmt_num(0.125, "0.00"), "0.13");
    }

    #[test]
    fn test_vector_05_away_from_zero_rounding_2point5() {
        // (2.5, "0") -> "3"
        assert_eq!(fmt_num(2.5, "0"), "3");
    }

    #[test]
    fn test_vector_06_away_from_zero_rounding_1point005() {
        // (1.005, "0.00") -> "1.01"
        assert_eq!(fmt_num(1.005, "0.00"), "1.01");
    }

    #[test]
    fn test_vector_07_percent_round() {
        // (0.0825, "0.0%") -> "8.3%"
        assert_eq!(fmt_num(0.0825, "0.0%"), "8.3%");
    }

    #[test]
    fn test_vector_08_date_yyyy_mm_dd() {
        // 45291 = 2023-12-31, 45292 = 2024-01-01 (지침의 벡터가 하루 틀렸다)
        assert_eq!(fmt_num(45291.0, "yyyy-mm-dd"), "2023-12-31");
        assert_eq!(fmt_num(45292.0, "yyyy-mm-dd"), "2024-01-01");
    }

    #[test]
    fn test_vector_09_date_yyyy_mm() {
        assert_eq!(fmt_num(45291.0, "yyyy-mm"), "2023-12");
    }

    #[test]
    fn test_vector_10_date_1900_start() {
        // (1, "yyyy-mm-dd") -> "1900-01-01"
        assert_eq!(fmt_num(1.0, "yyyy-mm-dd"), "1900-01-01");
    }

    #[test]
    fn test_vector_11_date_1900_leap_year_bug() {
        // (60, "yyyy-mm-dd") -> "1900-02-29" (엑셀의 1900 윤년 버그 그대로 재현)
        assert_eq!(fmt_num(60.0, "yyyy-mm-dd"), "1900-02-29");
    }

    #[test]
    fn test_vector_12_date_day_after_leap_bug() {
        // (61, "yyyy-mm-dd") -> "1900-03-01"
        assert_eq!(fmt_num(61.0, "yyyy-mm-dd"), "1900-03-01");
    }

    #[test]
    fn test_builtin_formats_and_text_formatting() {
        // 내장 서식 번호 3번 (#,##0)
        assert_eq!(fmt_num(999999.0, builtin_format_code(3)), "999,999");

        // 텍스트 @ 포맷
        let compiled = FormatCompiler::compile(r#"@"님""#);
        let out = FormatEngine::format_value(&FormatValue::Text("홍길동".to_string()), &compiled);
        assert_eq!(out, "홍길동님");
    }
}


// ============================================================================
// 8. 검수 시험 (Claude) — 이 샘플이 실제로 쓰는 서식과 한글·원화 안전성
// ============================================================================

#[cfg(test)]
mod review_tests {
    use super::*;

    fn f(v: f64, pat: &str) -> String {
        FormatEngine::format_value(&FormatValue::Number(v), &FormatCompiler::compile(pat))
    }
    fn t(v: &str, pat: &str) -> String {
        FormatEngine::format_value(&FormatValue::Text(v.to_string()), &FormatCompiler::compile(pat))
    }

    /// 샘플 파일이 쓰는 서식 6종.
    #[test]
    fn sample_formats() {
        assert_eq!(f(1234567.891, "$#,##0.00;($#,##0.00);\"-\""), "$1,234,567.89");
        assert_eq!(f(-1234567.891, "$#,##0.00;($#,##0.00);\"-\""), "($1,234,567.89)");
        assert_eq!(f(0.0, "$#,##0.00;($#,##0.00);\"-\""), "-");
        assert_eq!(f(0.1234, "0.0%"), "12.3%");
        assert_eq!(f(-0.1234, "0.0%"), "-12.3%");
        assert_eq!(f(3.14159265, "0.0000"), "3.1416");
        assert_eq!(f(1234.0, "#,##0"), "1,234");
        assert_eq!(f(1234.567, "#,##0.00"), "1,234.57");
        assert_eq!(f(0.5, "0.00%"), "50.00%");
    }

    /// 한글·원화 기호가 든 서식에서 글자 번호와 바이트 번호를 섞어 쓰면 잘리거나 터진다.
    #[test]
    fn multibyte_safe() {
        assert_eq!(f(1234.0, "#,##0\"원\""), "1,234원");
        assert_eq!(f(1234.0, "[$₩-412]#,##0"), "1,234");
        assert_eq!(f(1234.0, "\"합계 \"#,##0\"원\""), "합계 1,234원");
        assert_eq!(t("홍길동", "@\" 님\""), "홍길동 님");
        // 괄호 안에 여러 바이트 글자가 있어도 뒤 토큰이 밀리지 않는다.
        assert_eq!(f(-5.0, "[Red]#,##0;[Blue]\"△\"#,##0"), "△5");
    }

    /// 음수 부호: 섹션이 하나뿐이면 엑셀도 - 를 붙인다.
    #[test]
    fn negative_single_section() {
        assert_eq!(f(-1234.0, "#,##0"), "-1,234");
        assert_eq!(f(-0.5, "0.00"), "-0.50");
    }

    /// 축척 쉼표: 뒤에 숫자가 없는 쉼표만 1,000 으로 나눈다.
    #[test]
    fn scaling_comma() {
        assert_eq!(f(1_234_567.0, "#,##0,"), "1,235");
        assert_eq!(f(1_234_567_890.0, "#,##0,,"), "1,235");
        assert_eq!(f(1_234_567.0, "#,##0"), "1,234,567");
    }

    /// 빈 칸과 논리값.
    #[test]
    fn blank_and_bool() {
        let c = FormatCompiler::compile("#,##0");
        assert_eq!(FormatEngine::format_value(&FormatValue::Blank, &c), "");
        assert_eq!(FormatEngine::format_value(&FormatValue::Bool(true), &c), "TRUE");
    }

    /// 날짜·시간.
    #[test]
    fn dates() {
        assert_eq!(f(45291.0, "yyyy-mm-dd"), "2023-12-31");
        assert_eq!(f(45292.0, "yyyy-mm-dd"), "2024-01-01");
        assert_eq!(f(45292.5, "yyyy-mm-dd h:mm"), "2024-01-01 12:00");
        assert_eq!(f(45292.0, "aaa"), "월");
        assert_eq!(f(45292.0, "ddd"), "Mon");
        assert_eq!(f(45292.0, "mmm"), "Jan");
    }

    /// 엑셀의 15자리 정밀도 때문에 생기는 유명한 경계값들.
    #[test]
    fn precision_edges() {
        assert_eq!(f(2.675, "0.00"), "2.68");   // f64 로는 2.67499999… 라 2.67 이 되기 쉽다
        assert_eq!(f(0.615, "0.00"), "0.62");
        assert_eq!(f(1234567.891, "0.00"), "1234567.89");
        assert_eq!(f(1.0 / 3.0, "0.0000"), "0.3333");
        assert_eq!(f(9.995, "0.00"), "10.00");
        assert_eq!(f(-2.675, "0.00"), "-2.68");
        assert_eq!(f(0.0, "0.00"), "0.00");
    }

    /// 서식 코드는 한 번만 분석해 번호별로 캐시할 수 있어야 한다(같은 코드 → 같은 결과).
    #[test]
    fn compile_is_pure() {
        let a = FormatCompiler::compile("#,##0.00");
        let b = FormatCompiler::compile("#,##0.00");
        assert_eq!(a, b);
    }
}
