use crate::numfmt::{FormatCompiler, FormatEngine, FormatValue};
use crate::parse::{col_to_letters, parse_cell_ref_1based};
use crate::{CellKind, Workbook};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

// ============================================================================
// 1. 값 모델 및 오류 정의
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalcErr {
    Div0,  // #DIV/0!
    Value, // #VALUE!
    Ref,   // #REF!
    Name,  // #NAME?
    Num,   // #NUM!
    Na,    // #N/A
    Null,  // #NULL!
}

impl CalcErr {
    pub fn as_str(self) -> &'static str {
        match self {
            CalcErr::Div0 => "#DIV/0!",
            CalcErr::Value => "#VALUE!",
            CalcErr::Ref => "#REF!",
            CalcErr::Name => "#NAME?",
            CalcErr::Num => "#NUM!",
            CalcErr::Na => "#N/A",
            CalcErr::Null => "#NULL!",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "#DIV/0!" => Some(CalcErr::Div0),
            "#VALUE!" => Some(CalcErr::Value),
            "#REF!" => Some(CalcErr::Ref),
            "#NAME?" => Some(CalcErr::Name),
            "#NUM!" => Some(CalcErr::Num),
            "#N/A" | "#N/A!" => Some(CalcErr::Na),
            "#NULL!" => Some(CalcErr::Null),
            _ => None,
        }
    }
}

/// 참조 주소 (시트 인덱스, 시작 행, 시작 열, 끝 행, 끝 열 — 모두 1-based)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefRange {
    pub sheet_idx: usize,
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
}

impl RefRange {
    pub fn is_single(self) -> bool {
        self.start_row == self.end_row && self.start_col == self.end_col
    }
}

/// 수식 값 모델: 빈 칸(Blank)은 수식 문맥에 따라 0 이자 "" 로 동작한다.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(CalcErr),
    Blank,
    Ref(RefRange),
    Union(Vec<RefRange>),
    Array(Vec<Vec<Value>>),
}

impl Value {
    /// 산술 연산용 숫자 강제 변환 (빈 칸은 0, TRUE=1, FALSE=0, 숫자 문자열은 파싱)
    pub fn coerce_num(&self) -> Result<f64, CalcErr> {
        match self {
            Value::Num(n) => {
                if n.is_nan() || n.is_infinite() {
                    Err(CalcErr::Num)
                } else {
                    Ok(*n)
                }
            }
            Value::Blank => Ok(0.0),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Str(s) => {
                let t = s.trim();
                if t.is_empty() {
                    Err(CalcErr::Value)
                } else {
                    t.parse::<f64>().map_err(|_| CalcErr::Value)
                }
            }
            Value::Err(e) => Err(*e),
            Value::Array(rows) => rows
                .first()
                .and_then(|r| r.first())
                .unwrap_or(&Value::Blank)
                .coerce_num(),
            _ => Err(CalcErr::Value),
        }
    }

    /// 문자열 연산용 강제 변환 (빈 칸은 "")
    pub fn coerce_str(&self) -> Result<String, CalcErr> {
        match self {
            Value::Str(s) => Ok(s.clone()),
            Value::Blank => Ok(String::new()),
            Value::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    Ok(format!("{:.0}", n))
                } else if let Some(d) = Decimal::from_f64_retain(*n) {
                    Ok(d.normalize().to_string())
                } else {
                    Ok(n.to_string())
                }
            }
            Value::Bool(b) => Ok(if *b { "TRUE".to_string() } else { "FALSE".to_string() }),
            Value::Err(e) => Err(*e),
            Value::Array(rows) => rows
                .first()
                .and_then(|r| r.first())
                .unwrap_or(&Value::Blank)
                .coerce_str(),
            _ => Err(CalcErr::Value),
        }
    }

    /// 논리 연산용 강제 변환 (빈 칸은 FALSE, 0은 FALSE, 0이 아닌 숫자는 TRUE)
    pub fn coerce_bool(&self) -> Result<bool, CalcErr> {
        match self {
            Value::Bool(b) => Ok(*b),
            Value::Blank => Ok(false),
            Value::Num(n) => Ok(*n != 0.0),
            Value::Str(s) => {
                if s.eq_ignore_ascii_case("true") {
                    Ok(true)
                } else if s.eq_ignore_ascii_case("false") {
                    Ok(false)
                } else {
                    Err(CalcErr::Value)
                }
            }
            Value::Err(e) => Err(*e),
            Value::Array(rows) => rows
                .first()
                .and_then(|r| r.first())
                .unwrap_or(&Value::Blank)
                .coerce_bool(),
            _ => Err(CalcErr::Value),
        }
    }
}

// ============================================================================
// 2. 토크나이저 & Pratt 파서 (참조 연산자 `:`, 공백 교집합, `,` 합집합 포함)
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Num(f64),
    Str(String),
    Bool(bool),
    ErrorLit(CalcErr),
    Ident(String), // 함수 이름, 참조 토큰, 정의된 이름, '시트'!A1 등
    Colon,         // : 범위
    Intersect,     // 공백 교집합
    Comma,         // , 인수 구분 또는 합집합
    Plus,
    Minus,
    Mul,
    Div,
    Pow,
    Percent,
    Amp,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Range,     // :
    Intersect, // (공백)
    Union,     // , (괄호 안 합집합)
    Pow,       // ^
    Mul,       // *
    Div,       // /
    Add,       // +
    Sub,       // -
    Concat,    // &
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Percent, // 후위 %
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(CalcErr),
    EmptyArg, // IF(A1, , 0) 처럼 비어 있는 인수
    RefOrName {
        sheet: Option<String>,
        name: String,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Func {
        name: String,
        args: Vec<Expr>,
    },
    ArrayLit(Vec<Vec<Expr>>),
}

/// 수식 문자열을 토큰열로 분해한다.
/// 왜: 공백이 일반 공백인지 참조 교집합 연산자인지는 앞뒤 토큰 문맥으로만 판별할 수 있다.
pub fn tokenize(input: &str) -> Result<Vec<Token>, CalcErr> {
    let s = input.trim().strip_prefix('=').unwrap_or(input.trim());
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut tokens: Vec<Token> = Vec::new();
    let mut had_whitespace = false;

    while i < len {
        let ch = chars[i];
        if ch.is_whitespace() {
            had_whitespace = true;
            i += 1;
            continue;
        }

        // 앞 토큰이 참조/닫는괄호이고 현재 토큰이 참조/여는괄호이면 공백 교집합 토큰 삽입
        if had_whitespace {
            if let Some(prev) = tokens.last() {
                let prev_can_end_ref = matches!(prev, Token::Ident(_) | Token::RParen);
                let next_can_start_ref =
                    ch == '(' || ch == '\'' || ch == '$' || ch.is_ascii_alphabetic() || ch == '_';
                // 단, 뒤따르는 식별자가 함수 호출인 경우는 제외하기 위해 미리 살펴본다
                if prev_can_end_ref && next_can_start_ref && !looks_like_func_call(&chars, i) {
                    tokens.push(Token::Intersect);
                }
            }
            had_whitespace = false;
        }

        // 1. 문자열 리터럴 ("..." 내부 ""는 " 하나로)
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
            tokens.push(Token::Str(lit));
            continue;
        }

        // 2. 오류 리터럴 (#DIV/0!, #N/A 등)
        if ch == '#' {
            let start = i;
            i += 1;
            while i < len {
                let c = chars[i];
                if c.is_ascii_alphanumeric() || c == '/' || c == '!' || c == '?' {
                    i += 1;
                    if c == '!' || c == '?' {
                        break;
                    }
                } else {
                    break;
                }
            }
            let err_str: String = chars[start..i].iter().collect();
            let err = CalcErr::from_str(&err_str).unwrap_or(CalcErr::Value);
            tokens.push(Token::ErrorLit(err));
            continue;
        }

        // 3. 숫자 리터럴 (소수 및 지수 1.5E-3 포함)
        if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < len && (chars[i] == 'e' || chars[i] == 'E') {
                if i + 1 < len && (chars[i + 1] == '+' || chars[i + 1] == '-' || chars[i + 1].is_ascii_digit())
                {
                    i += 1;
                    if chars[i] == '+' || chars[i] == '-' {
                        i += 1;
                    }
                    while i < len && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let num_s: String = chars[start..i].iter().collect();
            let n = num_s.parse::<f64>().map_err(|_| CalcErr::Value)?;
            // 만약 숫자 바로 뒤에 `:` 이 오고 앞 토큰도 `:` 이 아니거나 `1:1` 행 전체 참조인 경우 처리
            if i < len && chars[i] == ':' && num_s.chars().all(|c| c.is_ascii_digit()) {
                tokens.push(Token::Ident(num_s));
            } else if matches!(tokens.last(), Some(Token::Colon))
                && num_s.chars().all(|c| c.is_ascii_digit())
            {
                tokens.push(Token::Ident(num_s));
            } else {
                tokens.push(Token::Num(n));
            }
            continue;
        }

        // 4. 작은따옴표 시트 참조 ('01_시트'!$A$5) 또는 일반 식별자/셀 참조
        if ch == '\'' || ch.is_alphabetic() || ch == '$' || ch == '_' {
            let start = i;
            if ch == '\'' {
                i += 1;
                while i < len {
                    if chars[i] == '\'' {
                        if i + 1 < len && chars[i + 1] == '\'' {
                            i += 2;
                        } else {
                            i += 1;
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                if i < len && chars[i] == '!' {
                    i += 1;
                }
            }
            while i < len {
                let c = chars[i];
                if c.is_alphanumeric() || c == '$' || c == '_' || c == '.' || c == '!' {
                    i += 1;
                } else {
                    break;
                }
            }
            let raw_ident: String = chars[start..i].iter().collect();
            let is_func = i < len && chars[i] == '(';
            if !is_func && raw_ident.eq_ignore_ascii_case("TRUE") {
                tokens.push(Token::Bool(true));
            } else if !is_func && raw_ident.eq_ignore_ascii_case("FALSE") {
                tokens.push(Token::Bool(false));
            } else {
                tokens.push(Token::Ident(raw_ident));
            }
            continue;
        }

        // 5. 연산자 및 괄호
        match ch {
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                i += 1;
            }
            '+' => {
                tokens.push(Token::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Mul);
                i += 1;
            }
            '/' => {
                tokens.push(Token::Div);
                i += 1;
            }
            '^' => {
                tokens.push(Token::Pow);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '&' => {
                tokens.push(Token::Amp);
                i += 1;
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '{' => {
                tokens.push(Token::LBrace);
                i += 1;
            }
            '}' => {
                tokens.push(Token::RBrace);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '<' => {
                if i + 1 < len && chars[i + 1] == '>' {
                    tokens.push(Token::Neq);
                    i += 2;
                } else if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::Lte);
                    i += 2;
                } else {
                    tokens.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::Gte);
                    i += 2;
                } else {
                    tokens.push(Token::Gt);
                    i += 1;
                }
            }
            _ => return Err(CalcErr::Value),
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

fn looks_like_func_call(chars: &[char], mut pos: usize) -> bool {
    while pos < chars.len() {
        let c = chars[pos];
        if c.is_alphanumeric() || c == '_' || c == '.' {
            pos += 1;
        } else {
            break;
        }
    }
    pos < chars.len() && chars[pos] == '('
}

pub struct FormulaParser {
    tokens: Vec<Token>,
    pos: usize,
    paren_depth: usize,
    /// 구문 트리 깊이(재귀 + 왼쪽 결합 사슬). 너무 깊으면 계산·해제 때 스택이 터진다.
    depth: usize,
}

/// 엑셀 수식 최대 길이(8,192자)와 넉넉한 트리 깊이 한도.
const MAX_FORMULA_CHARS: usize = 8192;
const MAX_AST_DEPTH: usize = 1024;

impl FormulaParser {
    pub fn parse_str(formula: &str) -> Result<Expr, CalcErr> {
        if formula.chars().count() > MAX_FORMULA_CHARS {
            return Err(CalcErr::Value);
        }
        let tokens = tokenize(formula)?;
        let mut p = Self {
            tokens,
            pos: 0,
            paren_depth: 0,
            depth: 0,
        };
        p.parse_expr(0)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn next(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// Pratt 파서 본체
    /// 우선순위:
    /// 90: `:` 범위
    /// 85: 공백 교집합
    /// 80: `,` 합집합 (일반 괄호 내부에서만 동작, 함수 인수와 구분)
    /// 75: 단항 `-`, `+` (엑셀 규칙: `-2^2 = 4` 이므로 `^`보다 단항 부호가 높다)
    /// 70: 후위 `%`
    /// 60: `^` 거듭제곱 (엑셀은 좌측 결합: `2^3^2 = 64`)
    /// 50: `*`, `/`
    /// 40: `+`, `-`
    /// 30: `&`
    /// 20: `=`, `<>`, `<`, `<=`, `>`, `>=`
    fn bump_depth(&mut self) -> Result<(), CalcErr> {
        self.depth += 1;
        if self.depth > MAX_AST_DEPTH {
            Err(CalcErr::Value)
        } else {
            Ok(())
        }
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, CalcErr> {
        let base = self.depth;
        self.bump_depth()?;
        let res = self.parse_expr_inner(min_bp);
        self.depth = base;
        res
    }

    fn parse_expr_inner(&mut self, min_bp: u8) -> Result<Expr, CalcErr> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // 후위 연산자 `%`
            if matches!(self.peek(), Token::Percent) && 70 > min_bp {
                self.next();
                self.bump_depth()?;
                lhs = Expr::Unary {
                    op: UnaryOp::Percent,
                    expr: Box::new(lhs),
                };
                continue;
            }

            let (op, l_bp, r_bp) = match self.peek() {
                Token::Colon => (BinOp::Range, 90, 91),
                Token::Intersect => (BinOp::Intersect, 85, 86),
                Token::Comma if self.paren_depth > 0 => (BinOp::Union, 80, 81),
                Token::Pow => (BinOp::Pow, 60, 61),
                Token::Mul => (BinOp::Mul, 50, 51),
                Token::Div => (BinOp::Div, 50, 51),
                Token::Plus => (BinOp::Add, 40, 41),
                Token::Minus => (BinOp::Sub, 40, 41),
                Token::Amp => (BinOp::Concat, 30, 31),
                Token::Eq => (BinOp::Eq, 20, 21),
                Token::Neq => (BinOp::Neq, 20, 21),
                Token::Lt => (BinOp::Lt, 20, 21),
                Token::Lte => (BinOp::Lte, 20, 21),
                Token::Gt => (BinOp::Gt, 20, 21),
                Token::Gte => (BinOp::Gte, 20, 21),
                _ => break,
            };

            if l_bp < min_bp {
                break;
            }
            self.next();
            self.bump_depth()?;
            let rhs = self.parse_expr(r_bp)?;
            lhs = Expr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, CalcErr> {
        match self.next() {
            Token::Num(n) => Ok(Expr::Num(n)),
            Token::Str(s) => Ok(Expr::Str(s)),
            Token::Bool(b) => Ok(Expr::Bool(b)),
            Token::ErrorLit(e) => Ok(Expr::Err(e)),
            Token::Plus => {
                let rhs = self.parse_expr(75)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Plus,
                    expr: Box::new(rhs),
                })
            }
            Token::Minus => {
                let rhs = self.parse_expr(75)?;
                Ok(Expr::Unary {
                    op: UnaryOp::Minus,
                    expr: Box::new(rhs),
                })
            }
            Token::LParen => {
                self.paren_depth += 1;
                let inner = self.parse_expr(0)?;
                self.paren_depth = self.paren_depth.saturating_sub(1);
                if matches!(self.peek(), Token::RParen) {
                    self.next();
                    Ok(inner)
                } else {
                    Err(CalcErr::Value)
                }
            }
            Token::LBrace => self.parse_array_literal(),
            Token::Ident(raw) => {
                if matches!(self.peek(), Token::LParen) {
                    self.next(); // consume '('
                    let saved_depth = self.paren_depth;
                    self.paren_depth = 0; // 함수 인수 내부 쉼표는 Union이 아니라 인수 구분자
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        loop {
                            if matches!(self.peek(), Token::Comma) {
                                args.push(Expr::EmptyArg);
                            } else if matches!(self.peek(), Token::RParen) {
                                args.push(Expr::EmptyArg);
                                break;
                            } else {
                                args.push(self.parse_expr(0)?);
                            }
                            if matches!(self.peek(), Token::Comma) {
                                self.next();
                            } else {
                                break;
                            }
                        }
                    }
                    self.paren_depth = saved_depth;
                    if matches!(self.peek(), Token::RParen) {
                        self.next();
                    } else {
                        return Err(CalcErr::Value);
                    }
                    Ok(Expr::Func {
                        name: normalize_func_name(&raw),
                        args,
                    })
                } else {
                    Ok(split_sheet_and_ref(&raw))
                }
            }
            _ => Err(CalcErr::Value),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expr, CalcErr> {
        let mut rows = Vec::new();
        let mut cur_row = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            cur_row.push(self.parse_expr(85)?);
            match self.peek() {
                Token::Comma => {
                    self.next();
                }
                Token::Semicolon => {
                    self.next();
                    rows.push(std::mem::take(&mut cur_row));
                }
                _ => break,
            }
        }
        if !cur_row.is_empty() {
            rows.push(cur_row);
        }
        if matches!(self.peek(), Token::RBrace) {
            self.next();
            Ok(Expr::ArrayLit(rows))
        } else {
            Err(CalcErr::Value)
        }
    }
}

/// `_xlfn.`, `_xlws.` 접두어를 제거하고 대문자로 통일한다.
/// 왜: 06 시트 STDEV.S·STDEV.P·VAR.S는 `_xlfn.` 유무와 상관없이 모두 동일하게 계산해야 한다(Rob 결정).
pub fn normalize_func_name(raw: &str) -> String {
    let up = raw.trim().to_ascii_uppercase();
    let stripped = up
        .strip_prefix("_XLFN.")
        .or_else(|| up.strip_prefix("_XLWS."))
        .unwrap_or(&up);
    stripped.to_string()
}

fn split_sheet_and_ref(raw: &str) -> Expr {
    if let Some((sh, rest)) = raw.rsplit_once('!') {
        let clean_sheet = sh
            .trim()
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .unwrap_or(sh.trim())
            .replace("''", "'");
        // `'01_시트'!$A$5:$A$64` 처럼 `:` 뒤쪽만 있는 것이 아니라 전체가 한 토큰으로 안 묶였을 때 대비
        Expr::RefOrName {
            sheet: Some(clean_sheet),
            name: rest.to_string(),
        }
    } else {
        Expr::RefOrName {
            sheet: None,
            name: raw.to_string(),
        }
    }
}

// ============================================================================
// 3. 의존 그래프 · 위상 정렬 · 동적 참조(OFFSET) 재계산 표시기
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellAddr {
    pub sheet: usize,
    pub row: u32, // 1-based
    pub col: u32, // 1-based
}

pub struct WorkbookCalcResult {
    pub values: HashMap<CellAddr, Value>,
    pub dynamic_recalc_cells: HashSet<CellAddr>, // OFFSET 등 실행 중 참조가 정해지는 칸
    pub circular_warnings: Vec<CellAddr>,        // 순환 참조로 인해 0으로 처리된 칸
}

/// 계산 전용 스레드의 스택 크기. tvOS 의 보조 스레드 기본값은 512KB 라서
/// 수식 재귀(함수 호출 프레임이 크다)와 셀 참조 사슬에 모자란다.
const CALC_STACK_BYTES: usize = 64 * 1024 * 1024;

/// 워크북 전체를 계산한다. 큰 스택을 가진 전용 스레드에서 돌리고,
/// 스레드를 못 만들면 호출한 스레드에서 그대로 돌린다. 계산 중 패닉이 나도
/// 앱을 죽이지 않고 빈 결과를 돌려준다.
pub fn evaluate_workbook(wb: &Workbook) -> WorkbookCalcResult {
    let empty = || WorkbookCalcResult {
        values: HashMap::new(),
        dynamic_recalc_cells: HashSet::new(),
        circular_warnings: Vec::new(),
    };
    std::thread::scope(|s| {
        match std::thread::Builder::new()
            .name("argosheet-calc".into())
            .stack_size(CALC_STACK_BYTES)
            .spawn_scoped(s, || evaluate_workbook_on_this_thread(wb))
        {
            Ok(h) => h.join().unwrap_or_else(|_| empty()),
            Err(_) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                evaluate_workbook_on_this_thread(wb)
            }))
            .unwrap_or_else(|_| empty()),
        }
    })
}

/// 워크북 전체의 수식을 파싱하고 위상 정렬(Kahn 알고리즘) 순서로 한 번에 계산한다.
fn evaluate_workbook_on_this_thread(wb: &Workbook) -> WorkbookCalcResult {
    let mut ast_map: HashMap<CellAddr, Expr> = HashMap::new();
    let mut deps_map: HashMap<CellAddr, Vec<CellAddr>> = HashMap::new();
    let mut dynamic_recalc_cells: HashSet<CellAddr> = HashSet::new();
    let mut defined_ast: HashMap<String, (Option<usize>, Expr)> = HashMap::new();

    for dn in &wb.defined_names {
        if let Ok(expr) = FormulaParser::parse_str(&dn.formula) {
            defined_ast.insert(
                dn.name.to_ascii_uppercase(),
                (dn.local_sheet_id.map(|i| i as usize), expr),
            );
        }
    }

    // 1. 모든 수식 셀 AST 파싱 및 정적 의존성 추출
    for (s_idx, sheet) in wb.sheets.iter().enumerate() {
        for (&key, f_entry) in &sheet.formulas {
            let row = (key >> 32) as u32;
            let col = (key & 0xFFFF_FFFF) as u32;
            let addr = CellAddr {
                sheet: s_idx,
                row,
                col,
            };
            let f_text = wb.pool.get(f_entry.text);
            if f_text.is_empty() {
                continue;
            }
            match FormulaParser::parse_str(f_text) {
                Ok(expr) => {
                    if contains_dynamic_ref_func(&expr) {
                        dynamic_recalc_cells.insert(addr);
                    }
                    let mut deps = Vec::new();
                    collect_static_dependencies(wb, s_idx, &expr, &defined_ast, &mut deps);
                    // 실제로 수식이 있는 셀만 의존 간선으로 유지
                    deps.retain(|d| {
                        wb.sheets
                            .get(d.sheet)
                            .map(|sh| sh.formulas.contains_key(&crate::cell_key(d.row, d.col)))
                            .unwrap_or(false)
                    });
                    deps.sort_by_key(|a| (a.sheet, a.row, a.col));
                    deps.dedup();
                    deps_map.insert(addr, deps);
                    ast_map.insert(addr, expr);
                }
                Err(e) => {
                    ast_map.insert(addr, Expr::Err(e));
                    deps_map.insert(addr, Vec::new());
                }
            }
        }
    }

    // 2. 위상 정렬 (Kahn's Algorithm)
    let mut in_degree: HashMap<CellAddr, usize> = HashMap::new();
    let mut reverse_edges: HashMap<CellAddr, Vec<CellAddr>> = HashMap::new();

    for (&node, deps) in &deps_map {
        in_degree.insert(node, deps.len());
        for &dep in deps {
            reverse_edges.entry(dep).or_default().push(node);
        }
    }

    let mut queue: VecDeque<CellAddr> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&node, _)| node)
        .collect();

    let mut order: Vec<CellAddr> = Vec::with_capacity(ast_map.len());
    while let Some(u) = queue.pop_front() {
        order.push(u);
        if let Some(dependents) = reverse_edges.get(&u) {
            for &v in dependents {
                if let Some(deg) = in_degree.get_mut(&v) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    // 3. 위상 정렬에 포함되지 못한 노드는 순환 참조(Cycle) -> 0.0으로 설정하고 경고 목록에 기록
    let mut values: HashMap<CellAddr, Value> = HashMap::new();
    let mut circular_warnings: Vec<CellAddr> = Vec::new();

    if order.len() < ast_map.len() {
        let ordered_set: HashSet<CellAddr> = order.iter().copied().collect();
        for &node in ast_map.keys() {
            if !ordered_set.contains(&node) {
                circular_warnings.push(node);
                values.insert(node, Value::Num(0.0));
            }
        }
        circular_warnings.sort_by_key(|a| (a.sheet, a.row, a.col));
    }

    // 4. 위상 정렬 순서대로 계산 수행 (OFFSET 등 동적 참조가 중간에 미계산 셀을 부르면 즉시 재귀 계산)
    let mut evaluator = Evaluator {
        wb,
        ast_map: &ast_map,
        defined_ast: &defined_ast,
        values: &mut values,
        visiting: HashSet::new(),
        circular_warnings: &mut circular_warnings,
    };

    for addr in order {
        evaluator.ensure_evaluated(addr);
    }

    WorkbookCalcResult {
        values,
        dynamic_recalc_cells,
        circular_warnings,
    }
}

fn contains_dynamic_ref_func(expr: &Expr) -> bool {
    match expr {
        Expr::Func { name, args } => {
            if name == "OFFSET" || name == "INDIRECT" {
                return true;
            }
            args.iter().any(contains_dynamic_ref_func)
        }
        Expr::Unary { expr, .. } => contains_dynamic_ref_func(expr),
        Expr::Binary { left, right, .. } => {
            contains_dynamic_ref_func(left) || contains_dynamic_ref_func(right)
        }
        Expr::ArrayLit(rows) => rows.iter().flatten().any(contains_dynamic_ref_func),
        _ => false,
    }
}

fn collect_static_dependencies(
    wb: &Workbook,
    cur_sheet: usize,
    expr: &Expr,
    defined_ast: &HashMap<String, (Option<usize>, Expr)>,
    out: &mut Vec<CellAddr>,
) {
    match expr {
        Expr::Binary {
            op: BinOp::Range,
            left,
            right,
        } => {
            if let Some(rr) = resolve_static_range_pair(wb, cur_sheet, left, right) {
                for r in rr.start_row..=rr.end_row {
                    for c in rr.start_col..=rr.end_col {
                        out.push(CellAddr {
                            sheet: rr.sheet_idx,
                            row: r,
                            col: c,
                        });
                    }
                }
                return;
            }
            collect_static_dependencies(wb, cur_sheet, left, defined_ast, out);
            collect_static_dependencies(wb, cur_sheet, right, defined_ast, out);
        }
        Expr::RefOrName { sheet, name } => {
            if let Some(rr) = resolve_ref_token(wb, cur_sheet, sheet.as_deref(), name) {
                for r in rr.start_row..=rr.end_row {
                    for c in rr.start_col..=rr.end_col {
                        out.push(CellAddr {
                            sheet: rr.sheet_idx,
                            row: r,
                            col: c,
                        });
                    }
                }
            } else if let Some((dn_sheet, dn_expr)) = defined_ast.get(&name.to_ascii_uppercase()) {
                collect_static_dependencies(
                    wb,
                    dn_sheet.unwrap_or(cur_sheet),
                    dn_expr,
                    defined_ast,
                    out,
                );
            }
        }
        Expr::Unary { expr, .. } => {
            collect_static_dependencies(wb, cur_sheet, expr, defined_ast, out)
        }
        Expr::Binary { left, right, .. } => {
            collect_static_dependencies(wb, cur_sheet, left, defined_ast, out);
            collect_static_dependencies(wb, cur_sheet, right, defined_ast, out);
        }
        Expr::Func { args, .. } => {
            for a in args {
                collect_static_dependencies(wb, cur_sheet, a, defined_ast, out);
            }
        }
        Expr::ArrayLit(rows) => {
            for r in rows {
                for e in r {
                    collect_static_dependencies(wb, cur_sheet, e, defined_ast, out);
                }
            }
        }
        _ => {}
    }
}

fn resolve_sheet_index(wb: &Workbook, current_sheet: usize, sheet_name: Option<&str>) -> Option<usize> {
    match sheet_name {
        None => Some(current_sheet),
        Some(target) => wb
            .sheets
            .iter()
            .position(|s| s.name == target || s.name.eq_ignore_ascii_case(target)),
    }
}

fn resolve_ref_token(
    wb: &Workbook,
    current_sheet: usize,
    sheet_name: Option<&str>,
    token: &str,
) -> Option<RefRange> {
    let s_idx = resolve_sheet_index(wb, current_sheet, sheet_name)?;
    let sheet = wb.sheets.get(s_idx)?;
    let clean = token.trim();

    // 1. 일반 셀 주소 (A1, $B$12)
    if let Some((r, c)) = parse_cell_ref_1based(clean) {
        return Some(RefRange {
            sheet_idx: s_idx,
            start_row: r,
            start_col: c,
            end_row: r,
            end_col: c,
        });
    }

    // 2. 전체 열 참조 (A, $C) 또는 전체 행 참조 (1, $5) — `A:A`, `1:1` 의 양변으로 등장할 때
    let no_dollar = clean.replace('$', "");
    if !no_dollar.is_empty() && no_dollar.chars().all(|c| c.is_ascii_alphabetic()) {
        let mut col = 0u32;
        for b in no_dollar.bytes() {
            col = col * 26 + (b.to_ascii_uppercase() - b'A' + 1) as u32;
        }
        let max_r = sheet.max_row.max(1);
        return Some(RefRange {
            sheet_idx: s_idx,
            start_row: 1,
            start_col: col,
            end_row: max_r,
            end_col: col,
        });
    }
    if !no_dollar.is_empty() && no_dollar.chars().all(|c| c.is_ascii_digit()) {
        let row = no_dollar.parse::<u32>().ok()?;
        let max_c = sheet.max_col.max(1);
        return Some(RefRange {
            sheet_idx: s_idx,
            start_row: row,
            start_col: 1,
            end_row: row,
            end_col: max_c,
        });
    }

    None
}

fn resolve_static_range_pair(
    wb: &Workbook,
    current_sheet: usize,
    left: &Expr,
    right: &Expr,
) -> Option<RefRange> {
    if let (
        Expr::RefOrName {
            sheet: sh1,
            name: n1,
        },
        Expr::RefOrName {
            sheet: sh2,
            name: n2,
        },
    ) = (left, right)
    {
        let r1 = resolve_ref_token(wb, current_sheet, sh1.as_deref(), n1)?;
        let r2 = resolve_ref_token(wb, r1.sheet_idx, sh2.as_deref(), n2)?;
        return Some(RefRange {
            sheet_idx: r1.sheet_idx,
            start_row: r1.start_row.min(r2.start_row),
            start_col: r1.start_col.min(r2.start_col),
            end_row: r1.end_row.max(r2.end_row),
            end_col: r1.end_col.max(r2.end_col),
        });
    }
    None
}

// ============================================================================
// 4. 평가기 (Evaluator) & 64개 함수 전체 구현
// ============================================================================

struct Evaluator<'a> {
    wb: &'a Workbook,
    ast_map: &'a HashMap<CellAddr, Expr>,
    defined_ast: &'a HashMap<String, (Option<usize>, Expr)>,
    values: &'a mut HashMap<CellAddr, Value>,
    visiting: HashSet<CellAddr>,
    circular_warnings: &'a mut Vec<CellAddr>,
}

impl<'a> Evaluator<'a> {
    fn ensure_evaluated(&mut self, addr: CellAddr) -> Value {
        if let Some(v) = self.values.get(&addr) {
            return v.clone();
        }
        if self.visiting.contains(&addr) {
            // 실행 중 순환 참조 감지 -> #VALUE! 대신 0으로 두고 경고 목록에 추가
            if !self.circular_warnings.contains(&addr) {
                self.circular_warnings.push(addr);
            }
            self.values.insert(addr, Value::Num(0.0));
            return Value::Num(0.0);
        }

        if let Some(expr) = self.ast_map.get(&addr) {
            self.visiting.insert(addr);
            let raw_val = self.eval_expr(addr, expr);
            let scalar = self.unwrap_to_scalar(addr, raw_val);
            self.visiting.remove(&addr);
            self.values.insert(addr, scalar.clone());
            return scalar;
        }

        // 수식이 없는 일반 셀 읽기
        self.read_raw_cell(addr)
    }

    fn read_raw_cell(&self, addr: CellAddr) -> Value {
        let Some(sheet) = self.wb.sheets.get(addr.sheet) else {
            return Value::Err(CalcErr::Ref);
        };
        let Some(cell) = sheet.data.get_cell(addr.row, addr.col) else {
            return Value::Blank;
        };
        unsafe {
            match cell.kind_enum() {
                CellKind::Blank => Value::Blank,
                CellKind::Num => Value::Num(cell.payload.num),
                CellKind::Str => Value::Str(self.wb.pool.get(cell.payload.str_span).to_string()),
                CellKind::Bool => Value::Bool(cell.payload.boolean != 0),
                CellKind::Err => {
                    let s = self.wb.pool.get(cell.payload.str_span);
                    Value::Err(CalcErr::from_str(s).unwrap_or(CalcErr::Value))
                }
            }
        }
    }

    fn unwrap_to_scalar(&mut self, caller: CellAddr, v: Value) -> Value {
        match v {
            Value::Ref(rr) => {
                if rr.is_single() {
                    self.ensure_evaluated(CellAddr {
                        sheet: rr.sheet_idx,
                        row: rr.start_row,
                        col: rr.start_col,
                    })
                } else {
                    // 암묵적 교차 또는 첫 칸(배열 수식 결과 한 칸만 쓰는 꼴)
                    let r = if caller.sheet == rr.sheet_idx
                        && caller.row >= rr.start_row
                        && caller.row <= rr.end_row
                        && rr.start_col == rr.end_col
                    {
                        caller.row
                    } else {
                        rr.start_row
                    };
                    let c = if caller.sheet == rr.sheet_idx
                        && caller.col >= rr.start_col
                        && caller.col <= rr.end_col
                        && rr.start_row == rr.end_row
                    {
                        caller.col
                    } else {
                        rr.start_col
                    };
                    self.ensure_evaluated(CellAddr {
                        sheet: rr.sheet_idx,
                        row: r,
                        col: c,
                    })
                }
            }
            Value::Array(rows) => rows
                .into_iter()
                .next()
                .and_then(|mut r| if r.is_empty() { None } else { Some(r.remove(0)) })
                .unwrap_or(Value::Blank),
            other => other,
        }
    }

    fn eval_scalar(&mut self, caller: CellAddr, expr: &Expr) -> Value {
        let v = self.eval_expr(caller, expr);
        self.unwrap_to_scalar(caller, v)
    }

    fn eval_expr(&mut self, caller: CellAddr, expr: &Expr) -> Value {
        match expr {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Err(e) => Value::Err(*e),
            Expr::EmptyArg => Value::Blank,
            Expr::ArrayLit(rows) => {
                let mut out = Vec::with_capacity(rows.len());
                for r in rows {
                    let mut row_vals = Vec::with_capacity(r.len());
                    for item in r {
                        row_vals.push(self.eval_scalar(caller, item));
                    }
                    out.push(row_vals);
                }
                Value::Array(out)
            }
            Expr::RefOrName { sheet, name } => {
                if let Some(rr) = resolve_ref_token(self.wb, caller.sheet, sheet.as_deref(), name) {
                    return Value::Ref(rr);
                }
                if let Some((dn_sheet, dn_expr)) = self.defined_ast.get(&name.to_ascii_uppercase())
                {
                    let target_caller = CellAddr {
                        sheet: dn_sheet.unwrap_or(caller.sheet),
                        row: caller.row,
                        col: caller.col,
                    };
                    return self.eval_expr(target_caller, dn_expr);
                }
                Value::Err(CalcErr::Name)
            }
            Expr::Unary { op, expr: inner } => {
                let v = self.eval_scalar(caller, inner);
                match v.coerce_num() {
                    Ok(n) => match op {
                        UnaryOp::Plus => Value::Num(n),
                        UnaryOp::Minus => Value::Num(-n),
                        UnaryOp::Percent => Value::Num(n / 100.0),
                    },
                    Err(e) => Value::Err(e),
                }
            }
            Expr::Binary { op, left, right } => match op {
                BinOp::Range => {
                    let lv = self.eval_expr(caller, left);
                    let rv = self.eval_expr(caller, right);
                    match (lv, rv) {
                        (Value::Ref(r1), Value::Ref(r2)) => Value::Ref(RefRange {
                            sheet_idx: r1.sheet_idx,
                            start_row: r1.start_row.min(r2.start_row),
                            start_col: r1.start_col.min(r2.start_col),
                            end_row: r1.end_row.max(r2.end_row),
                            end_col: r1.end_col.max(r2.end_col),
                        }),
                        (Value::Err(e), _) | (_, Value::Err(e)) => Value::Err(e),
                        _ => Value::Err(CalcErr::Ref),
                    }
                }
                BinOp::Intersect => {
                    let lv = self.eval_expr(caller, left);
                    let rv = self.eval_expr(caller, right);
                    match (lv, rv) {
                        (Value::Ref(r1), Value::Ref(r2)) if r1.sheet_idx == r2.sheet_idx => {
                            let sr = r1.start_row.max(r2.start_row);
                            let sc = r1.start_col.max(r2.start_col);
                            let er = r1.end_row.min(r2.end_row);
                            let ec = r1.end_col.min(r2.end_col);
                            if sr <= er && sc <= ec {
                                Value::Ref(RefRange {
                                    sheet_idx: r1.sheet_idx,
                                    start_row: sr,
                                    start_col: sc,
                                    end_row: er,
                                    end_col: ec,
                                })
                            } else {
                                Value::Err(CalcErr::Null)
                            }
                        }
                        _ => Value::Err(CalcErr::Null),
                    }
                }
                BinOp::Union => {
                    let lv = self.eval_expr(caller, left);
                    let rv = self.eval_expr(caller, right);
                    let mut list = Vec::new();
                    match lv {
                        Value::Ref(r) => list.push(r),
                        Value::Union(u) => list.extend(u),
                        _ => return Value::Err(CalcErr::Value),
                    }
                    match rv {
                        Value::Ref(r) => list.push(r),
                        Value::Union(u) => list.extend(u),
                        _ => return Value::Err(CalcErr::Value),
                    }
                    Value::Union(list)
                }
                _ => {
                    let lv = self.eval_expr(caller, left);
                    let rv = self.eval_expr(caller, right);
                    // 배열 수식(예: A1:A5 * B1:B5 또는 A1:A5 - 10) 지원
                    if matches!(lv, Value::Ref(_) | Value::Array(_))
                        || matches!(rv, Value::Ref(_) | Value::Array(_))
                    {
                        if let Some(arr) = self.eval_binary_array(caller, *op, &lv, &rv) {
                            return arr;
                        }
                    }
                    let ls = self.unwrap_to_scalar(caller, lv);
                    let rs = self.unwrap_to_scalar(caller, rv);
                    eval_binary_scalar(*op, &ls, &rs)
                }
            },
            Expr::Func { name, args } => self.eval_function(caller, name, args),
        }
    }

    fn eval_binary_array(
        &mut self,
        caller: CellAddr,
        op: BinOp,
        lv: &Value,
        rv: &Value,
    ) -> Option<Value> {
        let la = self.value_to_2d_opt(lv);
        let ra = self.value_to_2d_opt(rv);
        if la.is_none() && ra.is_none() {
            return None;
        }
        let l_grid = la.unwrap_or_else(|| vec![vec![self.unwrap_to_scalar(caller, lv.clone())]]);
        let r_grid = ra.unwrap_or_else(|| vec![vec![self.unwrap_to_scalar(caller, rv.clone())]]);

        let rows = l_grid.len().max(r_grid.len());
        let cols = l_grid
            .first()
            .map(|r| r.len())
            .unwrap_or(1)
            .max(r_grid.first().map(|r| r.len()).unwrap_or(1));

        let mut out = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut row_vec = Vec::with_capacity(cols);
            for c in 0..cols {
                let l_item = l_grid
                    .get(r)
                    .or_else(|| l_grid.first())
                    .and_then(|row| row.get(c).or_else(|| row.first()))
                    .unwrap_or(&Value::Blank);
                let r_item = r_grid
                    .get(r)
                    .or_else(|| r_grid.first())
                    .and_then(|row| row.get(c).or_else(|| row.first()))
                    .unwrap_or(&Value::Blank);
                row_vec.push(eval_binary_scalar(op, l_item, r_item));
            }
            out.push(row_vec);
        }
        Some(Value::Array(out))
    }

    fn value_to_2d_opt(&mut self, v: &Value) -> Option<Vec<Vec<Value>>> {
        match v {
            Value::Ref(rr) if !rr.is_single() => Some(self.range_to_2d(*rr)),
            Value::Array(rows) => Some(rows.clone()),
            _ => None,
        }
    }

    fn range_to_2d(&mut self, rr: RefRange) -> Vec<Vec<Value>> {
        let mut rows = Vec::with_capacity((rr.end_row - rr.start_row + 1) as usize);
        for r in rr.start_row..=rr.end_row {
            let mut row_vec = Vec::with_capacity((rr.end_col - rr.start_col + 1) as usize);
            for c in rr.start_col..=rr.end_col {
                row_vec.push(self.ensure_evaluated(CellAddr {
                    sheet: rr.sheet_idx,
                    row: r,
                    col: c,
                }));
            }
            rows.push(row_vec);
        }
        rows
    }

    /// 범위 내 숫자만 추출한다 (글·논리·빈칸 제외, 오류는 즉시 전파).
    fn collect_numbers_from_args(
        &mut self,
        caller: CellAddr,
        args: &[Expr],
    ) -> Result<Vec<f64>, CalcErr> {
        let mut nums = Vec::new();
        for arg in args {
            let v = self.eval_expr(caller, arg);
            self.append_numbers_from_value(v, true, &mut nums)?;
        }
        Ok(nums)
    }

    fn append_numbers_from_value(
        &mut self,
        v: Value,
        is_direct_arg: bool,
        out: &mut Vec<f64>,
    ) -> Result<(), CalcErr> {
        match v {
            Value::Err(e) => return Err(e),
            Value::Num(n) => out.push(n),
            Value::Bool(b) if is_direct_arg => out.push(if b { 1.0 } else { 0.0 }),
            Value::Str(s) if is_direct_arg => {
                if let Ok(n) = s.trim().parse::<f64>() {
                    out.push(n);
                } else {
                    return Err(CalcErr::Value);
                }
            }
            Value::Ref(rr) => {
                for r in rr.start_row..=rr.end_row {
                    for c in rr.start_col..=rr.end_col {
                        let cv = self.ensure_evaluated(CellAddr {
                            sheet: rr.sheet_idx,
                            row: r,
                            col: c,
                        });
                        match cv {
                            Value::Err(e) => return Err(e),
                            Value::Num(n) => out.push(n),
                            // 범위 내부의 글·논리·빈칸은 무시한다 (SUM, AVERAGE, STDEV.S 공통 규칙)
                            _ => {}
                        }
                    }
                }
            }
            Value::Union(list) => {
                for rr in list {
                    self.append_numbers_from_value(Value::Ref(rr), false, out)?;
                }
            }
            Value::Array(rows) => {
                for row in rows {
                    for item in row {
                        match item {
                            Value::Err(e) => return Err(e),
                            Value::Num(n) => out.push(n),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    // ========================================================================
    // 64개 함수 디스패처
    // ========================================================================

    fn eval_function(&mut self, caller: CellAddr, name: &str, args: &[Expr]) -> Value {
        // 미지원 동적 배열 함수 (이번 범위 제외 -> 지정된 문구 반환)
        if matches!(
            name,
            "XLOOKUP" | "FILTER" | "SORT" | "UNIQUE" | "SEQUENCE"
        ) {
            return Value::Str("아직 계산하지 않는 함수".to_string());
        }

        match self.dispatch_func(caller, name, args) {
            Ok(v) => v,
            Err(e) => Value::Err(e),
        }
    }

    fn dispatch_func(
        &mut self,
        caller: CellAddr,
        name: &str,
        args: &[Expr],
    ) -> Result<Value, CalcErr> {
        match name {
            // ----------------------------------------------------------------
            // [1] 논리·정보 (6개): IF, AND, OR, IFERROR, ISNUMBER, ISTEXT
            // ----------------------------------------------------------------
            "IF" => {
                if args.is_empty() || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let cond = self.eval_scalar(caller, &args[0]).coerce_bool()?;
                // 지연 평가: 고르지 않은 가지는 계산하지 않는다.
                if cond {
                    if args.len() >= 2 && !matches!(args[1], Expr::EmptyArg) {
                        Ok(self.eval_expr(caller, &args[1]))
                    } else {
                        Ok(Value::Num(0.0))
                    }
                } else if args.len() == 3 {
                    if matches!(args[2], Expr::EmptyArg) {
                        Ok(Value::Num(0.0))
                    } else {
                        Ok(self.eval_expr(caller, &args[2]))
                    }
                } else {
                    Ok(Value::Bool(false))
                }
            }
            "AND" => {
                if args.is_empty() {
                    return Err(CalcErr::Value);
                }
                let mut found_any = false;
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        match item {
                            Value::Err(e) => return Err(e),
                            Value::Bool(b) => {
                                found_any = true;
                                if !b {
                                    return Ok(Value::Bool(false));
                                }
                            }
                            Value::Num(n) => {
                                found_any = true;
                                if n == 0.0 {
                                    return Ok(Value::Bool(false));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !found_any {
                    Err(CalcErr::Value)
                } else {
                    Ok(Value::Bool(true))
                }
            }
            "OR" => {
                if args.is_empty() {
                    return Err(CalcErr::Value);
                }
                let mut found_any = false;
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        match item {
                            Value::Err(e) => return Err(e),
                            Value::Bool(b) => {
                                found_any = true;
                                if b {
                                    return Ok(Value::Bool(true));
                                }
                            }
                            Value::Num(n) => {
                                found_any = true;
                                if n != 0.0 {
                                    return Ok(Value::Bool(true));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !found_any {
                    Err(CalcErr::Value)
                } else {
                    Ok(Value::Bool(false))
                }
            }
            "IFERROR" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                if matches!(v, Value::Err(_)) {
                    Ok(self.eval_expr(caller, &args[1]))
                } else {
                    Ok(v)
                }
            }
            "ISNUMBER" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Num(_))))
            }
            "ISTEXT" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Str(_))))
            }

            // ----------------------------------------------------------------
            // [2] 수학 (7개): SUM, ABS, ROUND, MAX, SUMPRODUCT, LARGE, RANK
            // (+ 보조 MIN)
            // ----------------------------------------------------------------
            "SUM" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                Ok(Value::Num(nums.into_iter().sum()))
            }
            "ABS" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                Ok(Value::Num(n.abs()))
            }
            "ROUND" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let digits = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                Ok(Value::Num(excel_round_away_from_zero(n, digits)?))
            }
            "MAX" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                Ok(Value::Num(
                    nums.into_iter().fold(f64::NEG_INFINITY, f64::max).max(if false {
                        0.0
                    } else {
                        f64::NEG_INFINITY
                    }),
                )
                .map_inf_to_zero())
            }
            "MIN" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    Ok(Value::Num(0.0))
                } else {
                    Ok(Value::Num(nums.into_iter().fold(f64::INFINITY, f64::min)))
                }
            }
            "SUMPRODUCT" => {
                if args.is_empty() {
                    return Err(CalcErr::Value);
                }
                let mut grids = Vec::with_capacity(args.len());
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    let g = match v {
                        Value::Ref(rr) => self.range_to_2d(rr),
                        Value::Array(rows) => rows,
                        other => vec![vec![other]],
                    };
                    grids.push(g);
                }
                let r_len = grids[0].len();
                let c_len = grids[0].first().map(|r| r.len()).unwrap_or(0);
                for g in &grids[1..] {
                    if g.len() != r_len || g.first().map(|r| r.len()).unwrap_or(0) != c_len {
                        return Err(CalcErr::Value);
                    }
                }
                let mut total = 0.0;
                for r in 0..r_len {
                    for c in 0..c_len {
                        let mut prod = 1.0;
                        for g in &grids {
                            let item = &g[r][c];
                            match item {
                                Value::Err(e) => return Err(*e),
                                Value::Num(n) => prod *= *n,
                                // SUMPRODUCT에서 비교식 결과 등 논리값은 1/0으로 센다, 문자는 0
                                Value::Bool(b) => prod *= if *b { 1.0 } else { 0.0 },
                                _ => prod *= 0.0,
                            }
                        }
                        total += prod;
                    }
                }
                Ok(Value::Num(total))
            }
            "LARGE" | "SMALL" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let mut nums = self.collect_numbers_from_args(caller, &args[0..1])?;
                let k = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize;
                if k < 1 || (k as usize) > nums.len() {
                    return Err(CalcErr::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                let idx = if name == "LARGE" {
                    nums.len() - (k as usize)
                } else {
                    (k as usize) - 1
                };
                Ok(Value::Num(nums[idx]))
            }
            "RANK" | "RANK.EQ" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let target = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let nums = self.collect_numbers_from_args(caller, &args[1..2])?;
                let order = if args.len() == 3 {
                    self.eval_scalar(caller, &args[2]).coerce_num()? as i32
                } else {
                    0
                };
                if !nums.iter().any(|&x| (x - target).abs() < 1e-12) {
                    return Err(CalcErr::Na);
                }
                let better_count = if order == 0 {
                    nums.iter().filter(|&&x| x > target + 1e-12).count()
                } else {
                    nums.iter().filter(|&&x| x < target - 1e-12).count()
                };
                Ok(Value::Num((better_count + 1) as f64))
            }

            // ----------------------------------------------------------------
            // [3] 조건 집계 (9개): SUMIF, SUMIFS, COUNTIF, COUNTIFS, AVERAGEIF,
            //                    SUBTOTAL, COUNT, COUNTA, AVERAGE
            // ----------------------------------------------------------------
            "COUNT" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                Ok(Value::Num(nums.len() as f64))
            }
            "COUNTA" => {
                let mut cnt = 0usize;
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        if !matches!(item, Value::Blank) {
                            cnt += 1;
                        }
                    }
                }
                Ok(Value::Num(cnt as f64))
            }
            "AVERAGE" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    Err(CalcErr::Div0)
                } else {
                    Ok(Value::Num(nums.iter().sum::<f64>() / (nums.len() as f64)))
                }
            }
            "SUMIF" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let range_vals = self.eval_range_flat(caller, &args[0])?;
                let crit = self.eval_scalar(caller, &args[1]);
                let sum_vals = if args.len() == 3 {
                    self.eval_range_flat_matching_shape(caller, &args[0], &args[2])?
                } else {
                    range_vals.clone()
                };
                let matcher = CriteriaMatcher::parse(&crit);
                let mut total = 0.0;
                for (cv, sv) in range_vals.iter().zip(sum_vals.iter()) {
                    if matcher.matches(cv) {
                        if let Value::Num(n) = sv {
                            total += *n;
                        }
                    }
                }
                Ok(Value::Num(total))
            }
            "AVERAGEIF" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let range_vals = self.eval_range_flat(caller, &args[0])?;
                let crit = self.eval_scalar(caller, &args[1]);
                let avg_vals = if args.len() == 3 {
                    self.eval_range_flat_matching_shape(caller, &args[0], &args[2])?
                } else {
                    range_vals.clone()
                };
                let matcher = CriteriaMatcher::parse(&crit);
                let mut sum = 0.0;
                let mut cnt = 0usize;
                for (cv, av) in range_vals.iter().zip(avg_vals.iter()) {
                    if matcher.matches(cv) {
                        if let Value::Num(n) = av {
                            sum += *n;
                            cnt += 1;
                        }
                    }
                }
                if cnt == 0 {
                    Err(CalcErr::Div0)
                } else {
                    Ok(Value::Num(sum / (cnt as f64)))
                }
            }
            "COUNTIF" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let range_vals = self.eval_range_flat(caller, &args[0])?;
                let crit = self.eval_scalar(caller, &args[1]);
                let matcher = CriteriaMatcher::parse(&crit);
                let cnt = range_vals.iter().filter(|v| matcher.matches(v)).count();
                Ok(Value::Num(cnt as f64))
            }
            "COUNTIFS" => {
                if args.len() < 2 || args.len() % 2 != 0 {
                    return Err(CalcErr::Value);
                }
                let pair_cnt = args.len() / 2;
                let mut cols_data = Vec::with_capacity(pair_cnt);
                let mut matchers = Vec::with_capacity(pair_cnt);
                for p in 0..pair_cnt {
                    cols_data.push(self.eval_range_flat(caller, &args[p * 2])?);
                    let crit = self.eval_scalar(caller, &args[p * 2 + 1]);
                    matchers.push(CriteriaMatcher::parse(&crit));
                }
                let len = cols_data[0].len();
                for c in &cols_data[1..] {
                    if c.len() != len {
                        return Err(CalcErr::Value);
                    }
                }
                let mut count = 0usize;
                for i in 0..len {
                    if (0..pair_cnt).all(|p| matchers[p].matches(&cols_data[p][i])) {
                        count += 1;
                    }
                }
                Ok(Value::Num(count as f64))
            }
            "SUMIFS" => {
                if args.len() < 3 || (args.len() - 1) % 2 != 0 {
                    return Err(CalcErr::Value);
                }
                let sum_vals = self.eval_range_flat(caller, &args[0])?;
                let pair_cnt = (args.len() - 1) / 2;
                let mut cols_data = Vec::with_capacity(pair_cnt);
                let mut matchers = Vec::with_capacity(pair_cnt);
                for p in 0..pair_cnt {
                    cols_data.push(self.eval_range_flat(caller, &args[1 + p * 2])?);
                    let crit = self.eval_scalar(caller, &args[2 + p * 2]);
                    matchers.push(CriteriaMatcher::parse(&crit));
                }
                let len = sum_vals.len();
                for c in &cols_data {
                    if c.len() != len {
                        return Err(CalcErr::Value);
                    }
                }
                let mut total = 0.0;
                for i in 0..len {
                    if (0..pair_cnt).all(|p| matchers[p].matches(&cols_data[p][i])) {
                        if let Value::Num(n) = sum_vals[i] {
                            total += n;
                        }
                    }
                }
                Ok(Value::Num(total))
            }
            "SUBTOTAL" => self.eval_subtotal(caller, args),

            // ----------------------------------------------------------------
            // [4] 조회 (6개): INDEX, MATCH, VLOOKUP, CHOOSE, OFFSET, ADDRESS
            // ----------------------------------------------------------------
            "INDEX" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let arr_val = self.eval_expr(caller, &args[0]);
                let row_num = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as usize;
                let col_num = if args.len() == 3 {
                    self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as usize
                } else {
                    1
                };
                match arr_val {
                    Value::Ref(rr) => {
                        let r_cnt = (rr.end_row - rr.start_row + 1) as usize;
                        let c_cnt = (rr.end_col - rr.start_col + 1) as usize;
                        let (r_idx, c_idx) = if r_cnt == 1 && c_cnt > 1 && args.len() == 2 {
                            (1, row_num)
                        } else {
                            (row_num.max(1), col_num.max(1))
                        };
                        if r_idx > r_cnt || c_idx > c_cnt {
                            return Err(CalcErr::Ref);
                        }
                        Ok(Value::Ref(RefRange {
                            sheet_idx: rr.sheet_idx,
                            start_row: rr.start_row + (r_idx as u32) - 1,
                            start_col: rr.start_col + (c_idx as u32) - 1,
                            end_row: rr.start_row + (r_idx as u32) - 1,
                            end_col: rr.start_col + (c_idx as u32) - 1,
                        }))
                    }
                    Value::Array(rows) => {
                        let r_cnt = rows.len();
                        let c_cnt = rows.first().map(|r| r.len()).unwrap_or(0);
                        let (r_idx, c_idx) = if r_cnt == 1 && c_cnt > 1 && args.len() == 2 {
                            (1, row_num)
                        } else {
                            (row_num.max(1), col_num.max(1))
                        };
                        if r_idx == 0 || c_idx == 0 || r_idx > r_cnt || c_idx > c_cnt {
                            return Err(CalcErr::Ref);
                        }
                        Ok(rows[r_idx - 1][c_idx - 1].clone())
                    }
                    other => {
                        if row_num <= 1 && col_num <= 1 {
                            Ok(other)
                        } else {
                            Err(CalcErr::Ref)
                        }
                    }
                }
            }
            "MATCH" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let lookup = self.eval_scalar(caller, &args[0]);
                if let Value::Err(e) = lookup {
                    return Err(e);
                }
                let vec_vals = self.eval_range_flat(caller, &args[1])?;
                let match_type = if args.len() == 3 {
                    self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as i32
                } else {
                    1
                };
                match match_index(&lookup, &vec_vals, match_type) {
                    Some(idx_1based) => Ok(Value::Num(idx_1based as f64)),
                    None => Err(CalcErr::Na),
                }
            }
            "VLOOKUP" => {
                if args.len() < 3 || args.len() > 4 {
                    return Err(CalcErr::Value);
                }
                let lookup = self.eval_scalar(caller, &args[0]);
                if let Value::Err(e) = lookup {
                    return Err(e);
                }
                let table_val = self.eval_expr(caller, &args[1]);
                let table = match table_val {
                    Value::Ref(rr) => self.range_to_2d(rr),
                    Value::Array(rows) => rows,
                    _ => return Err(CalcErr::Value),
                };
                let col_idx = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as isize;
                // 네 번째 인수를 생략하면 기본값 TRUE(근사 일치 = 1)
                let approx = if args.len() == 4 && !matches!(args[3], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[3]).coerce_bool()?
                } else {
                    true
                };
                if col_idx < 1 {
                    return Err(CalcErr::Value);
                }
                let c_u = (col_idx as usize) - 1;
                if table.first().map(|r| c_u >= r.len()).unwrap_or(true) {
                    return Err(CalcErr::Ref);
                }
                let first_col: Vec<Value> = table.iter().map(|r| r[0].clone()).collect();
                let m_type = if approx { 1 } else { 0 };
                match match_index(&lookup, &first_col, m_type) {
                    Some(r_1based) => Ok(table[r_1based - 1][c_u].clone()),
                    None => Err(CalcErr::Na),
                }
            }
            "CHOOSE" => {
                if args.len() < 2 {
                    return Err(CalcErr::Value);
                }
                let idx = self.eval_scalar(caller, &args[0]).coerce_num()?.trunc() as isize;
                if idx < 1 || (idx as usize) >= args.len() {
                    return Err(CalcErr::Value);
                }
                Ok(self.eval_expr(caller, &args[idx as usize]))
            }
            "OFFSET" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(CalcErr::Value);
                }
                let base = match self.eval_expr(caller, &args[0]) {
                    Value::Ref(rr) => rr,
                    _ => return Err(CalcErr::Value),
                };
                let d_rows = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i64;
                let d_cols = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as i64;
                let base_h = (base.end_row - base.start_row + 1) as i64;
                let base_w = (base.end_col - base.start_col + 1) as i64;
                let height = if args.len() >= 4 && !matches!(args[3], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[3]).coerce_num()?.trunc() as i64
                } else {
                    base_h
                };
                let width = if args.len() == 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()?.trunc() as i64
                } else {
                    base_w
                };
                if height <= 0 || width <= 0 {
                    return Err(CalcErr::Ref);
                }
                let sr = (base.start_row as i64) + d_rows;
                let sc = (base.start_col as i64) + d_cols;
                let er = sr + height - 1;
                let ec = sc + width - 1;
                if sr < 1 || sc < 1 || er > 1_048_576 || ec > 16384 {
                    return Err(CalcErr::Ref);
                }
                Ok(Value::Ref(RefRange {
                    sheet_idx: base.sheet_idx,
                    start_row: sr as u32,
                    start_col: sc as u32,
                    end_row: er as u32,
                    end_col: ec as u32,
                }))
            }
            "ADDRESS" => {
                if args.len() < 2 || args.len() > 5 {
                    return Err(CalcErr::Value);
                }
                let r = self.eval_scalar(caller, &args[0]).coerce_num()?.trunc() as i64;
                let c = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i64;
                if r < 1 || c < 1 || r > 1_048_576 || c > 16384 {
                    return Err(CalcErr::Value);
                }
                let abs_num = if args.len() >= 3 && !matches!(args[2], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as i32
                } else {
                    1
                };
                let a1_style = if args.len() >= 4 && !matches!(args[3], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[3]).coerce_bool()?
                } else {
                    true
                };
                let sheet_text = if args.len() == 5 && !matches!(args[4], Expr::EmptyArg) {
                    Some(self.eval_scalar(caller, &args[4]).coerce_str()?)
                } else {
                    None
                };

                let coord = if a1_style {
                    let col_s = col_to_letters(c as u32);
                    match abs_num {
                        1 => format!("${}${}", col_s, r),
                        2 => format!("{}${}", col_s, r),
                        3 => format!("${}{}", col_s, r),
                        4 => format!("{}{}", col_s, r),
                        _ => return Err(CalcErr::Value),
                    }
                } else {
                    match abs_num {
                        1 => format!("R{}C{}", r, c),
                        2 => format!("R{}C[{}]", r, c),
                        3 => format!("R[{}]C{}", r, c),
                        4 => format!("R[{}]C[{}]", r, c),
                        _ => return Err(CalcErr::Value),
                    }
                };

                if let Some(sh) = sheet_text {
                    if sh.contains(' ') || sh.contains('!') || sh.contains('\'') {
                        Ok(Value::Str(format!("'{}'!{}", sh.replace('\'', "''"), coord)))
                    } else {
                        Ok(Value::Str(format!("{}!{}", sh, coord)))
                    }
                } else {
                    Ok(Value::Str(coord))
                }
            }

            // ----------------------------------------------------------------
            // [5] 문자 (9개): LEN, TRIM, MID, RIGHT, FIND, SUBSTITUTE, REPT,
            //                PROPER, TEXT (+ 보조 LEFT, UPPER, LOWER)
            // ----------------------------------------------------------------
            "LEN" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                // 왜: 한글 한 글자는 바이트(3)가 아니라 문자(char) 개수 1로 센다.
                Ok(Value::Num(s.chars().count() as f64))
            }
            "TRIM" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                // 왜: 엑셀 TRIM은 ASCII 공백(0x20)에 대해 앞뒤 공백을 제거하고 안쪽 연속 공백을 한 칸으로 줄인다.
                let mut out = String::with_capacity(s.len());
                let mut prev_space = true;
                for ch in s.chars() {
                    if ch == ' ' {
                        if !prev_space {
                            out.push(' ');
                            prev_space = true;
                        }
                    } else {
                        out.push(ch);
                        prev_space = false;
                    }
                }
                if out.ends_with(' ') {
                    out.pop();
                }
                Ok(Value::Str(out))
            }
            "MID" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let start = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize;
                let count = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as isize;
                if start < 1 || count < 0 {
                    return Err(CalcErr::Value);
                }
                let chars: Vec<char> = s.chars().collect();
                let st = ((start - 1) as usize).min(chars.len());
                let en = (st + (count as usize)).min(chars.len());
                Ok(Value::Str(chars[st..en].iter().collect()))
            }
            "RIGHT" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let count = if args.len() == 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize
                } else {
                    1
                };
                if count < 0 {
                    return Err(CalcErr::Value);
                }
                let chars: Vec<char> = s.chars().collect();
                let take = (count as usize).min(chars.len());
                Ok(Value::Str(chars[chars.len() - take..].iter().collect()))
            }
            "LEFT" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let count = if args.len() == 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize
                } else {
                    1
                };
                if count < 0 {
                    return Err(CalcErr::Value);
                }
                let chars: Vec<char> = s.chars().collect();
                let take = (count as usize).min(chars.len());
                Ok(Value::Str(chars[..take].iter().collect()))
            }
            "FIND" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let find_s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let within_s = self.eval_scalar(caller, &args[1]).coerce_str()?;
                let start_num = if args.len() == 3 {
                    self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as isize
                } else {
                    1
                };
                let within_chars: Vec<char> = within_s.chars().collect();
                let find_chars: Vec<char> = find_s.chars().collect();
                if start_num < 1 || (start_num as usize) > within_chars.len().max(1) {
                    return Err(CalcErr::Value);
                }
                let start_idx = (start_num as usize) - 1;
                if find_chars.is_empty() {
                    return Ok(Value::Num(start_num as f64));
                }
                if find_chars.len() + start_idx > within_chars.len() {
                    return Err(CalcErr::Value);
                }
                for pos in start_idx..=(within_chars.len() - find_chars.len()) {
                    if within_chars[pos..pos + find_chars.len()] == find_chars[..] {
                        return Ok(Value::Num((pos + 1) as f64));
                    }
                }
                Err(CalcErr::Value)
            }
            "SUBSTITUTE" => {
                if args.len() < 3 || args.len() > 4 {
                    return Err(CalcErr::Value);
                }
                let text = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let old_t = self.eval_scalar(caller, &args[1]).coerce_str()?;
                let new_t = self.eval_scalar(caller, &args[2]).coerce_str()?;
                if old_t.is_empty() {
                    return Ok(Value::Str(text));
                }
                if args.len() == 4 {
                    let inst = self.eval_scalar(caller, &args[3]).coerce_num()?.trunc() as isize;
                    if inst < 1 {
                        return Err(CalcErr::Value);
                    }
                    let mut out = String::with_capacity(text.len());
                    let mut rest = text.as_str();
                    let mut current = 0isize;
                    while let Some(pos) = rest.find(&old_t) {
                        current += 1;
                        out.push_str(&rest[..pos]);
                        if current == inst {
                            out.push_str(&new_t);
                            out.push_str(&rest[pos + old_t.len()..]);
                            return Ok(Value::Str(out));
                        } else {
                            out.push_str(&old_t);
                            rest = &rest[pos + old_t.len()..];
                        }
                    }
                    out.push_str(rest);
                    Ok(Value::Str(out))
                } else {
                    Ok(Value::Str(text.replace(&old_t, &new_t)))
                }
            }
            "REPT" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let text = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let times = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize;
                if times < 0 || text.len().saturating_mul(times as usize) > 32767 {
                    return Err(CalcErr::Value);
                }
                Ok(Value::Str(text.repeat(times as usize)))
            }
            "PROPER" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let mut out = String::with_capacity(s.len());
                // 왜: 엑셀 PROPER는 영문자가 아닌 모든 문자(공백, 숫자, 문장부호 등) 뒤의 첫 글자를 대문자로 만든다.
                let mut cap_next = true;
                for ch in s.chars() {
                    if ch.is_alphabetic() {
                        if cap_next {
                            for u in ch.to_uppercase() {
                                out.push(u);
                            }
                            cap_next = false;
                        } else {
                            for l in ch.to_lowercase() {
                                out.push(l);
                            }
                        }
                    } else {
                        out.push(ch);
                        cap_next = true;
                    }
                }
                Ok(Value::Str(out))
            }
            "TEXT" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let val = self.eval_scalar(caller, &args[0]);
                let fmt_str = self.eval_scalar(caller, &args[1]).coerce_str()?;
                let compiled = FormatCompiler::compile(&fmt_str);
                let fv = match val {
                    Value::Num(n) => FormatValue::Number(n),
                    Value::Str(s) => FormatValue::Text(s),
                    Value::Bool(b) => FormatValue::Bool(b),
                    Value::Blank => FormatValue::Number(0.0),
                    Value::Err(e) => return Err(e),
                    _ => FormatValue::Blank,
                };
                Ok(Value::Str(FormatEngine::format_value(&fv, &compiled)))
            }

            // ----------------------------------------------------------------
            // [6] 날짜 (5개): DATEDIF, NETWORKDAYS, EOMONTH, EDATE, WEEKDAY
            // ----------------------------------------------------------------
            "DATEDIF" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let start = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let end = self.eval_scalar(caller, &args[1]).coerce_num()?.floor() as i64;
                let unit = self
                    .eval_scalar(caller, &args[2])
                    .coerce_str()?
                    .trim()
                    .to_ascii_uppercase();
                if start > end {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(calc_datedif(start, end, &unit)? as f64))
            }
            "NETWORKDAYS" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let e = self.eval_scalar(caller, &args[1]).coerce_num()?.floor() as i64;
                Ok(Value::Num(calc_networkdays(s, e) as f64))
            }
            "EDATE" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let months = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                let (y, m, d) = serial_to_ymd_1900(serial);
                let (ny, nm) = add_months(y, m as i32, months);
                let max_d = days_in_month_gregorian(ny, nm as u32);
                let nd = d.min(max_d);
                Ok(Value::Num(ymd_to_serial_1900(ny, nm as u32, nd) as f64))
            }
            "EOMONTH" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let months = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                let (y, m, _) = serial_to_ymd_1900(serial);
                let (ny, nm) = add_months(y, m as i32, months);
                let end_d = days_in_month_gregorian(ny, nm as u32);
                Ok(Value::Num(ymd_to_serial_1900(ny, nm as u32, end_d) as f64))
            }
            "WEEKDAY" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let ret_type = if args.len() == 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32
                } else {
                    1
                };
                // 1900-01-01 (serial 1, 윤년 버그 보정 후) 요일 계산:
                // 엑셀은 serial >= 61 기준 실제 요일과 일치하며, (serial - 1) % 7 에서 0=일, 1=월, ..., 6=토
                let sun0 = ((serial - 1).rem_euclid(7)) as i32; // 0=Sun, 1=Mon, ..., 6=Sat
                let ans = match ret_type {
                    1 | 17 => sun0 + 1,                       // 1(Sun)..7(Sat)
                    2 | 11 => ((sun0 + 6) % 7) + 1,           // 1(Mon)..7(Sun)
                    3 => (sun0 + 6) % 7,                      // 0(Mon)..6(Sun)
                    12 => ((sun0 + 5) % 7) + 1,               // 1(Tue)..7(Mon)
                    13 => ((sun0 + 4) % 7) + 1,               // 1(Wed)..7(Tue)
                    14 => ((sun0 + 3) % 7) + 1,               // 1(Thu)..7(Wed)
                    15 => ((sun0 + 2) % 7) + 1,               // 1(Fri)..7(Thu)
                    16 => (sun0 % 7) + 1,                     // 1(Sat)..7(Fri)
                    _ => return Err(CalcErr::Num),
                };
                Ok(Value::Num(ans as f64))
            }

            // ----------------------------------------------------------------
            // [7] 재무 (10개): NPV, IRR, MIRR, SLN, DB, DDB, SYD, PMT, IPMT, PPMT
            // ----------------------------------------------------------------
            "NPV" => {
                if args.len() < 2 {
                    return Err(CalcErr::Value);
                }
                let rate = self.eval_scalar(caller, &args[0]).coerce_num()?;
                if (1.0 + rate).abs() < 1e-15 {
                    return Err(CalcErr::Div0);
                }
                let flows = self.collect_numbers_from_args(caller, &args[1..])?;
                // 왜: 엑셀 NPV는 첫 번째 현금흐름을 0기가 아니라 1기 말(t=1)로 할인한다.
                let mut total = 0.0;
                for (i, &cf) in flows.iter().enumerate() {
                    total += cf / (1.0 + rate).powi((i + 1) as i32);
                }
                Ok(Value::Num(total))
            }
            "IRR" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let flows = self.collect_numbers_from_args(caller, &args[0..1])?;
                let guess = if args.len() == 2 && !matches!(args[1], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[1]).coerce_num()?
                } else {
                    0.1
                };
                Ok(Value::Num(solve_irr(&flows, guess)?))
            }
            "MIRR" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let flows = self.collect_numbers_from_args(caller, &args[0..1])?;
                let fin_rate = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let reinv_rate = self.eval_scalar(caller, &args[2]).coerce_num()?;
                Ok(Value::Num(calc_mirr(&flows, fin_rate, reinv_rate)?))
            }
            "SLN" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let cost = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let salvage = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let life = self.eval_scalar(caller, &args[2]).coerce_num()?;
                if life == 0.0 {
                    return Err(CalcErr::Div0);
                }
                Ok(Value::Num((cost - salvage) / life))
            }
            "SYD" => {
                if args.len() != 4 {
                    return Err(CalcErr::Value);
                }
                let cost = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let salvage = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let life = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let per = self.eval_scalar(caller, &args[3]).coerce_num()?;
                if life <= 0.0 || per <= 0.0 || per > life {
                    return Err(CalcErr::Num);
                }
                let denom = life * (life + 1.0) / 2.0;
                Ok(Value::Num((cost - salvage) * (life - per + 1.0) / denom))
            }
            "DB" => {
                if args.len() < 4 || args.len() > 5 {
                    return Err(CalcErr::Value);
                }
                let cost = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let salvage = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let life = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let period = self.eval_scalar(caller, &args[3]).coerce_num()?;
                let month = if args.len() == 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()?
                } else {
                    12.0
                };
                Ok(Value::Num(calc_db(cost, salvage, life, period, month)?))
            }
            "DDB" => {
                if args.len() < 4 || args.len() > 5 {
                    return Err(CalcErr::Value);
                }
                let cost = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let salvage = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let life = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let period = self.eval_scalar(caller, &args[3]).coerce_num()?;
                let factor = if args.len() == 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()?
                } else {
                    2.0
                };
                Ok(Value::Num(calc_ddb(cost, salvage, life, period, factor)?))
            }
            "PMT" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(CalcErr::Value);
                }
                let rate = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let nper = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let pv = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let fv = if args.len() >= 4 && !matches!(args[3], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[3]).coerce_num()?
                } else {
                    0.0
                };
                let p_type = if args.len() == 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()? as i32
                } else {
                    0
                };
                Ok(Value::Num(calc_pmt(rate, nper, pv, fv, p_type)?))
            }
            "IPMT" => {
                if args.len() < 4 || args.len() > 6 {
                    return Err(CalcErr::Value);
                }
                let rate = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let per = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let nper = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let pv = self.eval_scalar(caller, &args[3]).coerce_num()?;
                let fv = if args.len() >= 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()?
                } else {
                    0.0
                };
                let p_type = if args.len() == 6 && !matches!(args[5], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[5]).coerce_num()? as i32
                } else {
                    0
                };
                Ok(Value::Num(calc_ipmt(rate, per, nper, pv, fv, p_type)?))
            }
            "PPMT" => {
                if args.len() < 4 || args.len() > 6 {
                    return Err(CalcErr::Value);
                }
                let rate = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let per = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let nper = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let pv = self.eval_scalar(caller, &args[3]).coerce_num()?;
                let fv = if args.len() >= 5 && !matches!(args[4], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[4]).coerce_num()?
                } else {
                    0.0
                };
                let p_type = if args.len() == 6 && !matches!(args[5], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[5]).coerce_num()? as i32
                } else {
                    0
                };
                let pmt = calc_pmt(rate, nper, pv, fv, p_type)?;
                let ipmt = calc_ipmt(rate, per, nper, pv, fv, p_type)?;
                Ok(Value::Num(pmt - ipmt))
            }

            // ----------------------------------------------------------------
            // [8] 통계 (11개): FORECAST, STDEV.S, STDEV.P, VAR.S, CORREL,
            //                 SLOPE, INTERCEPT, RSQ, MEDIAN, QUARTILE, PERCENTILE
            // ----------------------------------------------------------------
            "STDEV.S" | "STDEV" | "STDEVS" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                // Rob 지정 셈법: ① 숫자만 고름 ② n < 2 이면 #DIV/0! ③ 평균 ④ (값-평균)^2 합 / (n-1) 제곱근
                if nums.len() < 2 {
                    return Err(CalcErr::Div0);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let sq_sum: f64 = nums.iter().map(|&v| (v - mean) * (v - mean)).sum();
                Ok(Value::Num((sq_sum / ((nums.len() - 1) as f64)).sqrt()))
            }
            "STDEV.P" | "STDEVP" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    return Err(CalcErr::Div0);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let sq_sum: f64 = nums.iter().map(|&v| (v - mean) * (v - mean)).sum();
                Ok(Value::Num((sq_sum / (nums.len() as f64)).sqrt()))
            }
            "VAR.S" | "VAR" | "VARS" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.len() < 2 {
                    return Err(CalcErr::Div0);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let sq_sum: f64 = nums.iter().map(|&v| (v - mean) * (v - mean)).sum();
                Ok(Value::Num(sq_sum / ((nums.len() - 1) as f64)))
            }
            "VAR.P" | "VARP" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    return Err(CalcErr::Div0);
                }
                let mean = nums.iter().sum::<f64>() / (nums.len() as f64);
                let sq_sum: f64 = nums.iter().map(|&v| (v - mean) * (v - mean)).sum();
                Ok(Value::Num(sq_sum / (nums.len() as f64)))
            }
            "MEDIAN" => {
                let mut nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    return Err(CalcErr::Num);
                }
                nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                let mid = nums.len() / 2;
                if nums.len() % 2 == 1 {
                    Ok(Value::Num(nums[mid]))
                } else {
                    Ok(Value::Num((nums[mid - 1] + nums[mid]) * 0.5))
                }
            }
            "PERCENTILE" | "PERCENTILE.INC" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let mut nums = self.collect_numbers_from_args(caller, &args[0..1])?;
                let k = self.eval_scalar(caller, &args[1]).coerce_num()?;
                Ok(Value::Num(percentile_inc(&mut nums, k)?))
            }
            "QUARTILE" | "QUARTILE.INC" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let mut nums = self.collect_numbers_from_args(caller, &args[0..1])?;
                let q = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                if !(0..=4).contains(&q) {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(percentile_inc(&mut nums, (q as f64) * 0.25)?))
            }
            "CORREL" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let (xs, ys) = self.collect_paired_numbers(caller, &args[0], &args[1])?;
                let (_, _, r) = linear_regression(&xs, &ys)?;
                Ok(Value::Num(r))
            }
            "SLOPE" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let (ys, xs) = self.collect_paired_numbers(caller, &args[0], &args[1])?;
                let (slope, _, _) = linear_regression(&xs, &ys)?;
                Ok(Value::Num(slope))
            }
            "INTERCEPT" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let (ys, xs) = self.collect_paired_numbers(caller, &args[0], &args[1])?;
                let (_, intercept, _) = linear_regression(&xs, &ys)?;
                Ok(Value::Num(intercept))
            }
            "RSQ" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let (ys, xs) = self.collect_paired_numbers(caller, &args[0], &args[1])?;
                let (_, _, r) = linear_regression(&xs, &ys)?;
                Ok(Value::Num(r * r))
            }
            "FORECAST" | "FORECAST.LINEAR" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let x = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let (ys, xs) = self.collect_paired_numbers(caller, &args[1], &args[2])?;
                let (slope, intercept, _) = linear_regression(&xs, &ys)?;
                Ok(Value::Num(intercept + slope * x))
            }

            // ----------------------------------------------------------------
            // [9] 기타 (1개): HYPERLINK
            // ----------------------------------------------------------------
            "HYPERLINK" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let link_loc = self.eval_scalar(caller, &args[0]);
                if args.len() == 2 && !matches!(args[1], Expr::EmptyArg) {
                    Ok(self.eval_scalar(caller, &args[1]))
                } else {
                    Ok(link_loc)
                }
            }

            _ => Err(CalcErr::Name),
        }
    }

    // ========================================================================
    // 보조 평가 헬퍼 (SUBTOTAL, 범위 평탄화, 쌍 데이터 추출)
    // ========================================================================

    fn eval_subtotal(&mut self, caller: CellAddr, args: &[Expr]) -> Result<Value, CalcErr> {
        if args.len() < 2 {
            return Err(CalcErr::Value);
        }
        let fn_num = self.eval_scalar(caller, &args[0]).coerce_num()?.trunc() as i32;
        let skip_hidden = fn_num >= 100;
        let base_code = if skip_hidden { fn_num - 100 } else { fn_num };

        let mut nums = Vec::new();
        let mut counta = 0usize;

        for arg in &args[1..] {
            let v = self.eval_expr(caller, arg);
            if let Value::Ref(rr) = v {
                let sheet = self.wb.sheets.get(rr.sheet_idx).ok_or(CalcErr::Ref)?;
                for r in rr.start_row..=rr.end_row {
                    // SUBTOTAL 101~111은 숨은 행(row_height == 0.0)을 제외한다.
                    if skip_hidden {
                        let rh = sheet
                            .row_heights_pt
                            .get((r - 1) as usize)
                            .copied()
                            .unwrap_or(sheet.default_row_height_pt);
                        if rh <= 0.0 {
                            continue;
                        }
                    }
                    for c in rr.start_col..=rr.end_col {
                        // 중첩된 SUBTOTAL 셀은 이중 합산을 막기 위해 건너뛴다.
                        let f_str = sheet.formula_str(&self.wb.pool, r, c);
                        if f_str.to_ascii_uppercase().contains("SUBTOTAL(") {
                            continue;
                        }
                        let cv = self.ensure_evaluated(CellAddr {
                            sheet: rr.sheet_idx,
                            row: r,
                            col: c,
                        });
                        match cv {
                            Value::Err(e) => return Err(e),
                            Value::Num(n) => {
                                nums.push(n);
                                counta += 1;
                            }
                            Value::Blank => {}
                            _ => counta += 1,
                        }
                    }
                }
            } else {
                self.append_numbers_from_value(v, true, &mut nums)?;
            }
        }

        match base_code {
            1 => {
                if nums.is_empty() {
                    Err(CalcErr::Div0)
                } else {
                    Ok(Value::Num(nums.iter().sum::<f64>() / (nums.len() as f64)))
                }
            }
            2 => Ok(Value::Num(nums.len() as f64)),
            3 => Ok(Value::Num(counta as f64)),
            4 => Ok(Value::Num(
                nums.into_iter().fold(f64::NEG_INFINITY, f64::max),
            )
            .map_inf_to_zero()),
            5 => {
                if nums.is_empty() {
                    Ok(Value::Num(0.0))
                } else {
                    Ok(Value::Num(nums.into_iter().fold(f64::INFINITY, f64::min)))
                }
            }
            6 => Ok(Value::Num(if nums.is_empty() {
                0.0
            } else {
                nums.into_iter().product()
            })),
            7 => {
                if nums.len() < 2 {
                    Err(CalcErr::Div0)
                } else {
                    let m = nums.iter().sum::<f64>() / (nums.len() as f64);
                    let s: f64 = nums.iter().map(|&x| (x - m).powi(2)).sum();
                    Ok(Value::Num((s / ((nums.len() - 1) as f64)).sqrt()))
                }
            }
            8 => {
                if nums.is_empty() {
                    Err(CalcErr::Div0)
                } else {
                    let m = nums.iter().sum::<f64>() / (nums.len() as f64);
                    let s: f64 = nums.iter().map(|&x| (x - m).powi(2)).sum();
                    Ok(Value::Num((s / (nums.len() as f64)).sqrt()))
                }
            }
            9 => Ok(Value::Num(nums.into_iter().sum())),
            10 => {
                if nums.len() < 2 {
                    Err(CalcErr::Div0)
                } else {
                    let m = nums.iter().sum::<f64>() / (nums.len() as f64);
                    let s: f64 = nums.iter().map(|&x| (x - m).powi(2)).sum();
                    Ok(Value::Num(s / ((nums.len() - 1) as f64)))
                }
            }
            11 => {
                if nums.is_empty() {
                    Err(CalcErr::Div0)
                } else {
                    let m = nums.iter().sum::<f64>() / (nums.len() as f64);
                    let s: f64 = nums.iter().map(|&x| (x - m).powi(2)).sum();
                    Ok(Value::Num(s / (nums.len() as f64)))
                }
            }
            _ => Err(CalcErr::Value),
        }
    }

    fn flatten_value(&mut self, v: Value) -> Vec<Value> {
        let mut out = Vec::new();
        match v {
            Value::Ref(rr) => {
                for r in rr.start_row..=rr.end_row {
                    for c in rr.start_col..=rr.end_col {
                        out.push(self.ensure_evaluated(CellAddr {
                            sheet: rr.sheet_idx,
                            row: r,
                            col: c,
                        }));
                    }
                }
            }
            Value::Union(list) => {
                for rr in list {
                    out.extend(self.flatten_value(Value::Ref(rr)));
                }
            }
            Value::Array(rows) => {
                for r in rows {
                    out.extend(r);
                }
            }
            other => out.push(other),
        }
        out
    }

    fn eval_range_flat(&mut self, caller: CellAddr, expr: &Expr) -> Result<Vec<Value>, CalcErr> {
        let v = self.eval_expr(caller, expr);
        if let Value::Err(e) = v {
            return Err(e);
        }
        Ok(self.flatten_value(v))
    }

    fn eval_range_flat_matching_shape(
        &mut self,
        caller: CellAddr,
        base_expr: &Expr,
        sum_expr: &Expr,
    ) -> Result<Vec<Value>, CalcErr> {
        let b = self.eval_expr(caller, base_expr);
        let s = self.eval_expr(caller, sum_expr);
        if let (Value::Ref(br), Value::Ref(sr)) = (&b, &s) {
            let h = br.end_row - br.start_row;
            let w = br.end_col - br.start_col;
            let adjusted = RefRange {
                sheet_idx: sr.sheet_idx,
                start_row: sr.start_row,
                start_col: sr.start_col,
                end_row: sr.start_row + h,
                end_col: sr.start_col + w,
            };
            return Ok(self.flatten_value(Value::Ref(adjusted)));
        }
        Ok(self.flatten_value(s))
    }

    fn collect_paired_numbers(
        &mut self,
        caller: CellAddr,
        arg1: &Expr,
        arg2: &Expr,
    ) -> Result<(Vec<f64>, Vec<f64>), CalcErr> {
        let v1 = self.eval_range_flat(caller, arg1)?;
        let v2 = self.eval_range_flat(caller, arg2)?;
        if v1.len() != v2.len() || v1.is_empty() {
            return Err(CalcErr::Na);
        }
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for (a, b) in v1.into_iter().zip(v2.into_iter()) {
            if let Value::Err(e) = a {
                return Err(e);
            }
            if let Value::Err(e) = b {
                return Err(e);
            }
            if let (Value::Num(x), Value::Num(y)) = (a, b) {
                xs.push(x);
                ys.push(y);
            }
        }
        if xs.is_empty() {
            return Err(CalcErr::Div0);
        }
        Ok((xs, ys))
    }
}

trait MapInfToZero {
    fn map_inf_to_zero(self) -> Self;
}

impl MapInfToZero for Value {
    fn map_inf_to_zero(self) -> Self {
        match self {
            Value::Num(n) if n.is_infinite() => Value::Num(0.0),
            other => other,
        }
    }
}

// ============================================================================
// 5. 산술·비교·조건 매칭·재무·통계·날짜 수학 구현부
// ============================================================================

fn eval_binary_scalar(op: BinOp, left: &Value, right: &Value) -> Value {
    if let Value::Err(e) = left {
        return Value::Err(*e);
    }
    if let Value::Err(e) = right {
        return Value::Err(*e);
    }

    match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
            let l = match left.coerce_num() {
                Ok(v) => v,
                Err(e) => return Value::Err(e),
            };
            let r = match right.coerce_num() {
                Ok(v) => v,
                Err(e) => return Value::Err(e),
            };
            let res = match op {
                BinOp::Add => approx_add(l, r),
                BinOp::Sub => approx_add(l, -r),
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r == 0.0 {
                        return Value::Err(CalcErr::Div0);
                    }
                    l / r
                }
                BinOp::Pow => l.powf(r),
                _ => unreachable!(),
            };
            if res.is_nan() || res.is_infinite() {
                Value::Err(CalcErr::Num)
            } else {
                Value::Num(res)
            }
        }
        BinOp::Concat => {
            let ls = match left.coerce_str() {
                Ok(s) => s,
                Err(e) => return Value::Err(e),
            };
            let rs = match right.coerce_str() {
                Ok(s) => s,
                Err(e) => return Value::Err(e),
            };
            Value::Str(format!("{}{}", ls, rs))
        }
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte => {
            let ord = excel_compare_values(left, right);
            let ans = match op {
                BinOp::Eq => ord == Ordering::Equal,
                BinOp::Neq => ord != Ordering::Equal,
                BinOp::Lt => ord == Ordering::Less,
                BinOp::Lte => ord != Ordering::Greater,
                BinOp::Gt => ord == Ordering::Greater,
                BinOp::Gte => ord != Ordering::Less,
                _ => false,
            };
            Value::Bool(ans)
        }
        _ => Value::Err(CalcErr::Value),
    }
}

/// 엑셀 값 비교 규칙:
/// 빈 칸(Blank)은 숫자와 비교할 때 0, 문자와 비교할 때 "", 논리와 비교할 때 FALSE.
/// 서로 다른 타입끼리는 Number < Text < Bool 순서이며 대소문자를 구분하지 않는다.
pub fn excel_compare_values(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Blank, Value::Blank) => Ordering::Equal,
        (Value::Blank, Value::Num(n)) => 0.0f64.partial_cmp(n).unwrap_or(Ordering::Equal),
        (Value::Num(n), Value::Blank) => n.partial_cmp(&0.0).unwrap_or(Ordering::Equal),
        (Value::Blank, Value::Str(s)) => "".cmp(s.as_str()),
        (Value::Str(s), Value::Blank) => s.as_str().cmp(""),
        (Value::Blank, Value::Bool(bv)) => false.cmp(bv),
        (Value::Bool(bv), Value::Blank) => bv.cmp(&false),
        (Value::Num(x), Value::Num(y)) => {
            if (x - y).abs() <= 1e-12 * x.abs().max(y.abs()).max(1.0) {
                Ordering::Equal
            } else {
                x.partial_cmp(y).unwrap_or(Ordering::Equal)
            }
        }
        (Value::Str(x), Value::Str(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Num(_), Value::Str(_)) | (Value::Num(_), Value::Bool(_)) => Ordering::Less,
        (Value::Str(_), Value::Bool(_)) => Ordering::Less,
        (Value::Str(_), Value::Num(_)) | (Value::Bool(_), Value::Num(_)) => Ordering::Greater,
        (Value::Bool(_), Value::Str(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// 엑셀·LibreOffice 처럼 더하기/빼기 결과가 피연산자의 15자리 정밀도 아래로
/// 떨어지면 0 으로 본다(예: 대출 잔액 5,000,000 - 5,000,000.0000000042 → 0).
fn approx_add(a: f64, b: f64) -> f64 {
    let s = a + b;
    if s != 0.0 && (a > 0.0) != (b > 0.0) && s.abs() < a.abs().max(b.abs()) * 1e-15 * 8.0 {
        0.0
    } else {
        s
    }
}

/// 엑셀 ROUND: 15자리 유효숫자로 정리한 뒤 반 올림(0에서 먼 쪽).
/// 주의: RoundingStrategy::AwayFromZero 는 "항상 올림"이라 쓰면 안 된다.
pub fn excel_round_away_from_zero(val: f64, digits: i32) -> Result<f64, CalcErr> {
    if val == 0.0 || !val.is_finite() {
        return if val.is_finite() { Ok(0.0) } else { Err(CalcErr::Num) };
    }
    let d = Decimal::from_f64_retain(val).ok_or(CalcErr::Num)?;
    let d = crate::numfmt::round_sig15(d);
    if digits >= 0 {
        let r = d.round_dp_with_strategy(digits.min(28) as u32, RoundingStrategy::MidpointAwayFromZero);
        r.to_f64().ok_or(CalcErr::Num)
    } else {
        let p = (-digits).min(28) as u32;
        let factor = Decimal::from_i128_with_scale(10i128.pow(p), 0);
        let r = (d / factor).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero) * factor;
        r.to_f64().ok_or(CalcErr::Num)
    }
}

/// SUMIF / COUNTIF / SUMIFS 조건 매처 (`>=100`, `<>`, 와일드카드 `*`, `?`, `~*`)
struct CriteriaMatcher {
    op: BinOp,
    num_target: Option<f64>,
    str_target: String,
    has_wildcard: bool,
    match_blank: bool,
}

impl CriteriaMatcher {
    fn parse(crit: &Value) -> Self {
        match crit {
            Value::Num(n) => Self {
                op: BinOp::Eq,
                num_target: Some(*n),
                str_target: n.to_string(),
                has_wildcard: false,
                match_blank: false,
            },
            Value::Bool(b) => Self {
                op: BinOp::Eq,
                num_target: None,
                str_target: if *b { "TRUE".into() } else { "FALSE".into() },
                has_wildcard: false,
                match_blank: false,
            },
            Value::Blank => Self {
                op: BinOp::Eq,
                num_target: None,
                str_target: String::new(),
                has_wildcard: false,
                match_blank: true,
            },
            Value::Str(s) => {
                let (op, rest) = if let Some(r) = s.strip_prefix(">=") {
                    (BinOp::Gte, r)
                } else if let Some(r) = s.strip_prefix("<=") {
                    (BinOp::Lte, r)
                } else if let Some(r) = s.strip_prefix("<>") {
                    (BinOp::Neq, r)
                } else if let Some(r) = s.strip_prefix('>') {
                    (BinOp::Gt, r)
                } else if let Some(r) = s.strip_prefix('<') {
                    (BinOp::Lt, r)
                } else if let Some(r) = s.strip_prefix('=') {
                    (BinOp::Eq, r)
                } else {
                    (BinOp::Eq, s.as_str())
                };

                let match_blank = rest.is_empty() && matches!(op, BinOp::Eq | BinOp::Neq);
                let num_target = rest.trim().parse::<f64>().ok();
                let has_wildcard = matches!(op, BinOp::Eq | BinOp::Neq)
                    && (rest.contains('*') || rest.contains('?') || rest.contains('~'));

                Self {
                    op,
                    num_target,
                    str_target: rest.to_string(),
                    has_wildcard,
                    match_blank,
                }
            }
            _ => Self {
                op: BinOp::Eq,
                num_target: None,
                str_target: String::new(),
                has_wildcard: false,
                match_blank: false,
            },
        }
    }

    fn matches(&self, cell: &Value) -> bool {
        if self.match_blank {
            let is_empty = match cell {
                Value::Blank => true,
                Value::Str(s) => s.is_empty(),
                _ => false,
            };
            return if self.op == BinOp::Eq {
                is_empty
            } else {
                !is_empty
            };
        }

        if let Some(target_n) = self.num_target {
            let cell_n = match cell {
                Value::Num(n) => Some(*n),
                Value::Str(s) => s.trim().parse::<f64>().ok(),
                _ => None,
            };
            if let Some(cn) = cell_n {
                let ord = if (cn - target_n).abs() <= 1e-12 * cn.abs().max(target_n.abs()).max(1.0)
                {
                    Ordering::Equal
                } else {
                    cn.partial_cmp(&target_n).unwrap_or(Ordering::Equal)
                };
                return match self.op {
                    BinOp::Eq => ord == Ordering::Equal,
                    BinOp::Neq => ord != Ordering::Equal,
                    BinOp::Lt => ord == Ordering::Less,
                    BinOp::Lte => ord != Ordering::Greater,
                    BinOp::Gt => ord == Ordering::Greater,
                    BinOp::Gte => ord != Ordering::Less,
                    _ => false,
                };
            }
            return self.op == BinOp::Neq;
        }

        let cell_s = match cell {
            Value::Str(s) => s.as_str(),
            Value::Blank => "",
            _ => return self.op == BinOp::Neq,
        };

        if self.has_wildcard {
            let matched = wildcard_match_ci(&self.str_target, cell_s);
            return if self.op == BinOp::Eq {
                matched
            } else {
                !matched
            };
        }

        let ord = cell_s.to_lowercase().cmp(&self.str_target.to_lowercase());
        match self.op {
            BinOp::Eq => ord == Ordering::Equal,
            BinOp::Neq => ord != Ordering::Equal,
            BinOp::Lt => ord == Ordering::Less,
            BinOp::Lte => ord != Ordering::Greater,
            BinOp::Gt => ord == Ordering::Greater,
            BinOp::Gte => ord != Ordering::Less,
            _ => false,
        }
    }
}

fn wildcard_match_ci(pattern: &str, text: &str) -> bool {
    let p_chars: Vec<char> = pattern.to_lowercase().chars().collect();
    let t_chars: Vec<char> = text.to_lowercase().chars().collect();
    wildcard_helper(&p_chars, 0, &t_chars, 0)
}

fn wildcard_helper(p: &[char], pi: usize, t: &[char], ti: usize) -> bool {
    if pi == p.len() {
        return ti == t.len();
    }
    if p[pi] == '~' && pi + 1 < p.len() {
        return ti < t.len() && p[pi + 1] == t[ti] && wildcard_helper(p, pi + 2, t, ti + 1);
    }
    if p[pi] == '*' {
        for skip in ti..=t.len() {
            if wildcard_helper(p, pi + 1, t, skip) {
                return true;
            }
        }
        return false;
    }
    if p[pi] == '?' {
        return ti < t.len() && wildcard_helper(p, pi + 1, t, ti + 1);
    }
    ti < t.len() && p[pi] == t[ti] && wildcard_helper(p, pi + 1, t, ti + 1)
}

/// MATCH / VLOOKUP 인덱스 검색 (1-based 반환)
fn match_index(lookup: &Value, values: &[Value], match_type: i32) -> Option<usize> {
    if match_type == 0 {
        let is_wild = if let Value::Str(s) = lookup {
            s.contains('*') || s.contains('?') || s.contains('~')
        } else {
            false
        };
        for (i, v) in values.iter().enumerate() {
            if is_wild {
                if let (Value::Str(pat), Value::Str(txt)) = (lookup, v) {
                    if wildcard_match_ci(pat, txt) {
                        return Some(i + 1);
                    }
                }
            } else if same_variant_type(lookup, v) && excel_compare_values(lookup, v) == Ordering::Equal
            {
                return Some(i + 1);
            }
        }
        None
    } else if match_type > 0 {
        // 오름차순 근사 일치: lookup 이하인 가장 큰 값
        let mut best_idx: Option<usize> = None;
        for (i, v) in values.iter().enumerate() {
            if !same_variant_type(lookup, v) {
                continue;
            }
            if excel_compare_values(v, lookup) != Ordering::Greater {
                best_idx = Some(i + 1);
            } else {
                break;
            }
        }
        best_idx
    } else {
        // 내림차순 근사 일치(-1): lookup 이상인 가장 작은 값
        let mut best_idx: Option<usize> = None;
        for (i, v) in values.iter().enumerate() {
            if !same_variant_type(lookup, v) {
                continue;
            }
            if excel_compare_values(v, lookup) != Ordering::Less {
                best_idx = Some(i + 1);
            } else {
                break;
            }
        }
        best_idx
    }
}

fn same_variant_type(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Num(_), Value::Num(_))
            | (Value::Str(_), Value::Str(_))
            | (Value::Bool(_), Value::Bool(_))
    )
}

// ============================================================================
// 6. 날짜 계산 헬퍼 (1900 윤년 버그 포함)
// ============================================================================

fn serial_to_ymd_1900(serial: i64) -> (i32, u32, u32) {
    if serial <= 0 {
        return (1900, 1, 1);
    }
    if serial == 60 {
        return (1900, 2, 29);
    }
    // 일련번호 1 = 1900-01-01 (JDN 2415021). 60 이후는 가짜 2/29 때문에 하루 당긴다.
    let adj = if serial > 60 { serial - 1 } else { serial };
    let l = adj + 68569 + 2415020;
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

fn ymd_to_serial_1900(year: i32, month: u32, day: u32) -> i64 {
    if year == 1900 && month == 2 && day == 29 {
        return 60;
    }
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    let a = (14 - m) / 12;
    let y_adj = y + 4800 - a;
    let m_adj = m + 12 * a - 3;
    let jdn = d + (153 * m_adj + 2) / 5 + 365 * y_adj + y_adj / 4 - y_adj / 100 + y_adj / 400 - 32045;
    // 1900-03-01(JDN 2415080) = 61. 그 이전은 가짜 2/29 가 없으므로 하루 적다.
    let raw = jdn - 2415019;
    if raw <= 60 {
        raw - 1
    } else {
        raw
    }
}

fn is_leap_year_gregorian(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month_gregorian(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year_gregorian(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn add_months(y: i32, m: i32, delta: i32) -> (i32, i32) {
    let total = (y * 12 + (m - 1)) + delta;
    let ny = total.div_euclid(12);
    let nm = total.rem_euclid(12) + 1;
    (ny, nm)
}

fn calc_datedif(start: i64, end: i64, unit: &str) -> Result<i64, CalcErr> {
    let (y1, m1, d1) = serial_to_ymd_1900(start);
    let (y2, m2, d2) = serial_to_ymd_1900(end);

    match unit {
        "D" => Ok(end - start),
        "M" => {
            let mut months = (y2 - y1) * 12 + (m2 as i32 - m1 as i32);
            if d2 < d1 {
                months -= 1;
            }
            Ok(months as i64)
        }
        "Y" => {
            let mut years = y2 - y1;
            if (m2, d2) < (m1, d1) {
                years -= 1;
            }
            Ok(years as i64)
        }
        "YM" => {
            let mut months = m2 as i32 - m1 as i32;
            if d2 < d1 {
                months -= 1;
            }
            if months < 0 {
                months += 12;
            }
            Ok(months as i64)
        }
        "YD" => {
            let mut ref_y = y2;
            if (m2, d2) < (m1, d1) {
                ref_y -= 1;
            }
            let max_d = days_in_month_gregorian(ref_y, m1);
            let ref_serial = ymd_to_serial_1900(ref_y, m1, d1.min(max_d));
            Ok(end - ref_serial)
        }
        "MD" => {
            if d2 >= d1 {
                Ok((d2 - d1) as i64)
            } else {
                let (prev_y, prev_m) = add_months(y2, m2 as i32, -1);
                let prev_serial = ymd_to_serial_1900(prev_y, prev_m as u32, d1);
                Ok(end - prev_serial)
            }
        }
        _ => Err(CalcErr::Num),
    }
}

fn calc_networkdays(start: i64, end: i64) -> i64 {
    let sign = if start <= end { 1 } else { -1 };
    let (a, b) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    let mut workdays = 0i64;
    for s in a..=b {
        let sun0 = (s - 1).rem_euclid(7); // 0=Sun, 6=Sat
        if sun0 != 0 && sun0 != 6 {
            workdays += 1;
        }
    }
    workdays * sign
}

// ============================================================================
// 7. 재무·통계 수학 함수 구현부 (DB 3자리 반올림, DDB 잔존가 하한, IRR 뉴턴+이분법)
// ============================================================================

/// DB (정률법 감가상각):
/// 왜: 엑셀 DB 함수는 감가율 `1 - (salvage / cost)^(1 / life)`를 반드시 **소수 셋째 자리에서 반올림**한다.
fn calc_db(cost: f64, salvage: f64, life: f64, period: f64, month: f64) -> Result<f64, CalcErr> {
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || month <= 0.0 || month > 12.0 {
        return Err(CalcErr::Num);
    }
    let max_per = if (month - 12.0).abs() < 1e-9 {
        life
    } else {
        life + 1.0
    };
    if period > max_per {
        return Err(CalcErr::Num);
    }
    if cost == 0.0 {
        return Ok(0.0);
    }

    let raw_rate = if salvage == 0.0 {
        1.0
    } else {
        1.0 - (salvage / cost).powf(1.0 / life)
    };
    let rate = excel_round_away_from_zero(raw_rate, 3)?;

    let p_target = period.round() as i32;
    let life_i = life.round() as i32;

    let first_dep = cost * rate * month / 12.0;
    if p_target == 1 {
        return Ok(first_dep);
    }

    let mut total_dep = first_dep;
    for p in 2..=p_target {
        let dep = if p == life_i + 1 && (month - 12.0).abs() > 1e-9 {
            (cost - total_dep) * rate * (12.0 - month) / 12.0
        } else {
            (cost - total_dep) * rate
        };
        if p == p_target {
            return Ok(dep);
        }
        total_dep += dep;
    }
    Ok(0.0)
}

/// DDB (이중체감법 감가상각): 장부가액이 잔존가(salvage) 아래로 내려가지 않는다.
fn calc_ddb(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> Result<f64, CalcErr> {
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || period > life || factor <= 0.0
    {
        return Err(CalcErr::Num);
    }
    let rate = (factor / life).min(1.0);
    let p_int = period.round() as i32;
    let mut book_val = cost;
    let mut dep = 0.0;

    for _ in 1..=p_int {
        let candidate = book_val * rate;
        dep = if book_val - candidate < salvage {
            (book_val - salvage).max(0.0)
        } else {
            candidate
        };
        book_val -= dep;
    }
    Ok(dep)
}

/// IRR: 초기값(기본 0.1) 뉴턴-랩슨법 + 실패 시 구간 탐색 이분법 보강, 허용오차 1e-7
fn solve_irr(flows: &[f64], guess: f64) -> Result<f64, CalcErr> {
    let has_pos = flows.iter().any(|&v| v > 0.0);
    let has_neg = flows.iter().any(|&v| v < 0.0);
    if !has_pos || !has_neg {
        return Err(CalcErr::Num);
    }

    let npv_at = |r: f64| -> (f64, f64) {
        let base = 1.0 + r;
        let mut f = 0.0;
        let mut df = 0.0;
        let mut p = 1.0;
        for (t, &cf) in flows.iter().enumerate() {
            f += cf / p;
            if t > 0 {
                df -= (t as f64) * cf / (p * base);
            }
            p *= base;
        }
        (f, df)
    };

    // 1단계: 뉴턴법
    let mut r = guess.max(-0.99);
    for _ in 0..100 {
        let (f, df) = npv_at(r);
        if f.abs() < 1e-9 {
            return Ok(r);
        }
        if df.abs() < 1e-14 {
            break;
        }
        let next_r = r - f / df;
        if (next_r - r).abs() < 1e-8 {
            return Ok(next_r);
        }
        r = next_r.clamp(-0.9999, 100.0);
    }

    // 2단계: 이분법(Bisection) 보강
    let mut prev_r = -0.95;
    let (mut prev_f, _) = npv_at(prev_r);
    let mut step = 0.05;
    let mut cur_r = prev_r + step;
    while cur_r <= 20.0 {
        let (cur_f, _) = npv_at(cur_r);
        if prev_f * cur_f <= 0.0 {
            let mut lo = prev_r;
            let mut hi = cur_r;
            let mut f_lo = prev_f;
            for _ in 0..120 {
                let mid = 0.5 * (lo + hi);
                let (f_mid, _) = npv_at(mid);
                if f_mid.abs() < 1e-9 || (hi - lo).abs() < 1e-9 {
                    return Ok(mid);
                }
                if f_lo * f_mid <= 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                    f_lo = f_mid;
                }
            }
            return Ok(0.5 * (lo + hi));
        }
        prev_r = cur_r;
        prev_f = cur_f;
        cur_r += step;
        if cur_r > 2.0 {
            step = 0.5;
        }
    }

    Err(CalcErr::Num)
}

fn calc_mirr(flows: &[f64], fin_rate: f64, reinv_rate: f64) -> Result<f64, CalcErr> {
    let n = flows.len();
    if n < 2 || (1.0 + fin_rate) <= 0.0 || (1.0 + reinv_rate) <= 0.0 {
        return Err(CalcErr::Div0);
    }
    let mut pv_neg = 0.0;
    let mut fv_pos = 0.0;
    for (t, &cf) in flows.iter().enumerate() {
        if cf < 0.0 {
            pv_neg += cf / (1.0 + fin_rate).powi(t as i32);
        } else if cf > 0.0 {
            fv_pos += cf * (1.0 + reinv_rate).powi((n - 1 - t) as i32);
        }
    }
    if pv_neg == 0.0 || fv_pos == 0.0 {
        return Err(CalcErr::Div0);
    }
    Ok((-fv_pos / pv_neg).powf(1.0 / ((n - 1) as f64)) - 1.0)
}

fn calc_pmt(rate: f64, nper: f64, pv: f64, fv: f64, p_type: i32) -> Result<f64, CalcErr> {
    if nper == 0.0 {
        return Err(CalcErr::Num);
    }
    let t = if p_type != 0 { 1.0 } else { 0.0 };
    if rate.abs() < 1e-12 {
        return Ok(-(pv + fv) / nper);
    }
    let pvif = (1.0 + rate).powf(nper);
    Ok(-rate * (pv * pvif + fv) / ((1.0 + rate * t) * (pvif - 1.0)))
}

fn calc_fv(rate: f64, nper: f64, pmt: f64, pv: f64, p_type: i32) -> f64 {
    if rate.abs() < 1e-12 {
        return -(pv + pmt * nper);
    }
    let t = if p_type != 0 { 1.0 } else { 0.0 };
    let pvif = (1.0 + rate).powf(nper);
    -(pv * pvif + pmt * (1.0 + rate * t) * (pvif - 1.0) / rate)
}

fn calc_ipmt(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, p_type: i32) -> Result<f64, CalcErr> {
    if per < 1.0 || per > nper + 1e-9 {
        return Err(CalcErr::Num);
    }
    if p_type != 0 && (per - 1.0).abs() < 1e-9 {
        return Ok(0.0);
    }
    let pmt = calc_pmt(rate, nper, pv, fv, p_type)?;
    let n_prev = if p_type != 0 { per - 2.0 } else { per - 1.0 };
    let balance = calc_fv(rate, n_prev, pmt, pv, p_type);
    let ipmt = balance * rate;
    if p_type != 0 {
        Ok(ipmt / (1.0 + rate))
    } else {
        Ok(ipmt)
    }
}

/// PERCENTILE.INC / QUARTILE.INC (포함 방식 선형 보간)
fn percentile_inc(nums: &mut [f64], k: f64) -> Result<f64, CalcErr> {
    if nums.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(CalcErr::Num);
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    if nums.len() == 1 {
        return Ok(nums[0]);
    }
    let pos = k * ((nums.len() - 1) as f64);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - (lo as f64);
    Ok(nums[lo] + frac * (nums[hi] - nums[lo]))
}

fn linear_regression(xs: &[f64], ys: &[f64]) -> Result<(f64, f64, f64), CalcErr> {
    if xs.len() < 2 || xs.len() != ys.len() {
        return Err(CalcErr::Div0);
    }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    let mut sxy = 0.0;
    for (&x, &y) in xs.iter().zip(ys.iter()) {
        let dx = x - mx;
        let dy = y - my;
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    if sxx == 0.0 || syy == 0.0 {
        return Err(CalcErr::Div0);
    }
    let slope = sxy / sxx;
    let intercept = my - slope * mx;
    let r = sxy / (sxx * syy).sqrt();
    Ok((slope, intercept, r))
}

// ============================================================================
// 8. golden_values.json 검증기 (시트별 일치 수 / 전체 수 + 최대 20개 오차 리포트)
// ============================================================================

#[derive(Debug, Clone)]
pub struct SheetComparisonReport {
    pub sheet_name: String,
    pub matched: usize,
    pub total: usize,
    pub diffs: Vec<(String, String, String)>, // (주소, 기대, 실제) 최대 20개
}

/// 워크북 계산 결과를 기대값 맵(`시트명 -> (셀주소 -> 기대값)`)과 비교한다.
/// 숫자는 상대오차 `1e-9` (0 근처는 절대오차 `1e-9`) 기준, 문자열은 완전 일치로 판정한다.
pub fn compare_with_golden(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    golden: &HashMap<String, HashMap<String, Value>>,
) -> (usize, usize, Vec<SheetComparisonReport>) {
    // 맞춰 가는 순서: 03 -> 05 -> 06 -> 02 -> 04 -> 01 -> 00
    let priority = ["03", "05", "06", "02", "04", "01", "00"];
    let mut sheet_indices: Vec<usize> = (0..wb.sheets.len()).collect();
    sheet_indices.sort_by_key(|&idx| {
        let name = &wb.sheets[idx].name;
        priority
            .iter()
            .position(|p| name.starts_with(p))
            .unwrap_or(99)
    });

    let mut total_matched = 0usize;
    let mut total_cells = 0usize;
    let mut reports = Vec::new();

    for s_idx in sheet_indices {
        let sheet = &wb.sheets[s_idx];
        let Some(expected_map) = golden.get(&sheet.name) else {
            continue;
        };

        let mut s_matched = 0usize;
        let mut s_total = 0usize;
        let mut diffs = Vec::new();

        let mut keys: Vec<&String> = expected_map.keys().collect();
        keys.sort_by_key(|addr| parse_cell_ref_1based(addr).unwrap_or((u32::MAX, u32::MAX)));

        for addr_str in keys {
            let Some((r, c)) = parse_cell_ref_1based(addr_str) else {
                continue;
            };
            let exp = &expected_map[addr_str];
            let actual = calc_res
                .values
                .get(&CellAddr {
                    sheet: s_idx,
                    row: r,
                    col: c,
                })
                .cloned()
                .unwrap_or(Value::Blank);

            s_total += 1;
            if values_match_golden(exp, &actual) {
                s_matched += 1;
            } else if diffs.len() < diff_limit() {
                diffs.push((
                    addr_str.clone(),
                    format_debug_val(exp),
                    format_debug_val(&actual),
                ));
            }
        }

        total_matched += s_matched;
        total_cells += s_total;
        reports.push(SheetComparisonReport {
            sheet_name: sheet.name.clone(),
            matched: s_matched,
            total: s_total,
            diffs,
        });
    }

    (total_matched, total_cells, reports)
}

fn diff_limit() -> usize {
    std::env::var("ARGOSHEET_DIFF_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20)
}

/// 정답 파일의 날짜는 "2025-01-09T00:00:00" 꼴 문자열이다(openpyxl 이 datetime 으로 읽음).
/// 엔진은 1900 체계 일련번호를 내므로 일련번호로 바꿔 비교한다.
fn iso_to_serial(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let mut frac = 0.0;
    if let Some(tp) = s.get(11..19) {
        let hh: f64 = tp.get(0..2)?.parse().ok()?;
        let mm: f64 = tp.get(3..5)?.parse().ok()?;
        let ss: f64 = tp.get(6..8)?.parse().ok()?;
        frac = (hh * 3600.0 + mm * 60.0 + ss) / 86400.0;
    }
    // 1899-12-30 기준 일수(1900-03-01 이후는 엑셀 일련번호와 같다)
    let (yy, mm) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let jdn = d + (153 * (mm - 3) + 2) / 5 + 365 * yy + yy / 4 - yy / 100 + yy / 400 + 1721119;
    Some((jdn - 2415019) as f64 + frac)
}

pub fn values_match_golden(expected: &Value, actual: &Value) -> bool {
    if let (Value::Str(e), Value::Num(_)) = (expected, actual) {
        if let Some(serial) = iso_to_serial(e) {
            return values_match_golden(&Value::Num(serial), actual);
        }
    }
    match (expected, actual) {
        (Value::Num(e), Value::Num(a)) => {
            let diff = (e - a).abs();
            let scale = e.abs().max(a.abs()).max(1.0);
            diff <= 1e-9 * scale
        }
        (Value::Str(e), Value::Str(a)) => e == a,
        (Value::Bool(e), Value::Bool(a)) => e == a,
        (Value::Err(e), Value::Err(a)) => e == a,
        (Value::Blank, Value::Blank) => true,
        _ => false,
    }
}

fn format_debug_val(v: &Value) -> String {
    match v {
        Value::Num(n) => format!("{:.10}", n),
        Value::Str(s) => format!("{:?}", s),
        Value::Bool(b) => b.to_string(),
        Value::Err(e) => e.as_str().to_string(),
        Value::Blank => "BLANK".to_string(),
        other => format!("{:?}", other),
    }
}

// ============================================================================
// 9. P1-c 단위 시험 (64개 함수 전체 및 특수 함정 검증)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::StyleSheet;
    use crate::{Cell, CellPayload, FormulaEntry, FormulaKind, Rows, Sheet, SheetState, StringPool, CELL_FLAG_HAS_FORMULA};

    fn make_single_sheet_wb(cells: &[(&str, Value)], formulas: &[(&str, &str)]) -> Workbook {
        let mut pool = StringPool::default();
        let mut row_map: HashMap<u32, Vec<Cell>> = HashMap::new();
        let mut formula_map: HashMap<u64, FormulaEntry> = HashMap::new();
        let mut max_r = 1u32;
        let mut max_c = 1u32;

        for (addr, val) in cells {
            let (r, c) = parse_cell_ref_1based(addr).unwrap();
            max_r = max_r.max(r);
            max_c = max_c.max(c);
            let (kind, payload) = match val {
                Value::Num(n) => (CellKind::Num as u8, CellPayload { num: *n }),
                Value::Str(s) => (
                    CellKind::Str as u8,
                    CellPayload {
                        str_span: pool.intern(s),
                    },
                ),
                Value::Bool(b) => (
                    CellKind::Bool as u8,
                    CellPayload {
                        boolean: if *b { 1 } else { 0 },
                    },
                ),
                _ => (CellKind::Blank as u8, CellPayload { raw: 0 }),
            };
            row_map.entry(r).or_default().push(Cell {
                col: c,
                kind,
                flags: 0,
                style_idx: 0,
                payload,
            });
        }

        for (addr, f_str) in formulas {
            let (r, c) = parse_cell_ref_1based(addr).unwrap();
            max_r = max_r.max(r);
            max_c = max_c.max(c);
            let span = pool.intern(f_str);
            formula_map.insert(
                crate::cell_key(r, c),
                FormulaEntry {
                    kind: FormulaKind::Normal,
                    text: span,
                    ref_range: None,
                    shared_index: None,
                },
            );
            let list = row_map.entry(r).or_default();
            if !list.iter().any(|cell| cell.col == c) {
                list.push(Cell {
                    col: c,
                    kind: CellKind::Blank as u8,
                    flags: CELL_FLAG_HAS_FORMULA,
                    style_idx: 0,
                    payload: CellPayload { raw: 0 },
                });
            }
        }

        let mut rows = Rows::default();
        let mut sorted_r: Vec<u32> = row_map.keys().copied().collect();
        sorted_r.sort_unstable();
        for r in sorted_r {
            let mut cs = row_map.remove(&r).unwrap();
            cs.sort_by_key(|c| c.col);
            rows.push_row(r, &cs);
        }

        let sheet = Sheet {
            name: "01_Test".to_string(),
            sheet_id: 1,
            state: SheetState::Visible,
            max_row: max_r,
            max_col: max_c,
            cell_count: rows.cells.len() as u32,
            col_widths_pt: vec![50.0; max_c as usize],
            row_heights_pt: vec![15.0; max_r as usize],
            default_col_width_pt: 50.0,
            default_row_height_pt: 15.0,
            data: rows,
            formulas: formula_map,
            merges: Vec::new(),
            freeze: None,
            show_grid_lines: true,
            auto_filter: None,
            validations: Vec::new(),
            comments: Vec::new(),
        };

        Workbook {
            path: String::new(),
            date1904: false,
            full_calc_on_load: true,
            defined_names: Vec::new(),
            styles: StyleSheet::default_with_theme(Vec::new()),
            pool,
            sheets: vec![sheet],
        }
    }

    fn eval_one(formula: &str) -> Value {
        let wb = make_single_sheet_wb(&[], &[("A1", formula)]);
        let res = evaluate_workbook(&wb);
        res.values
            .get(&CellAddr {
                sheet: 0,
                row: 1,
                col: 1,
            })
            .cloned()
            .unwrap()
    }

    #[test]
    fn test_precedence_and_operators() {
        // 엑셀 단항 마이너스 우선순위: -2^2 = 4, 2^3^2 = 64
        assert!(values_match_golden(&Value::Num(4.0), &eval_one("-2^2")));
        assert!(values_match_golden(&Value::Num(64.0), &eval_one("2^3^2")));
        assert!(values_match_golden(&Value::Num(5.0), &eval_one("10%*50")));
        assert_eq!(eval_one("\"한글\"&123"), Value::Str("한글123".to_string()));
    }

    #[test]
    fn test_03_sheet_text_functions_korean_and_trim() {
        assert_eq!(eval_one("LEN(\"가나다AB\")"), Value::Num(5.0));
        assert_eq!(
            eval_one("TRIM(\"  한국   전력  기술   \")"),
            Value::Str("한국 전력 기술".to_string())
        );
        assert_eq!(
            eval_one("MID(\"대한민국만세\", 3, 2)"),
            Value::Str("민국".to_string())
        );
        assert_eq!(
            eval_one("RIGHT(\"2026-11-05\", 2)"),
            Value::Str("05".to_string())
        );
        assert_eq!(eval_one("FIND(\"B\", \"abBcd\")"), Value::Num(3.0));
        assert_eq!(eval_one("FIND(\"Z\", \"abBcd\")"), Value::Err(CalcErr::Value));
        assert_eq!(
            eval_one("SUBSTITUTE(\"A-B-C\", \"-\", \"/\", 2)"),
            Value::Str("A-B/C".to_string())
        );
        assert_eq!(eval_one("REPT(\"★\", 3)"), Value::Str("★★★".to_string()));
        assert_eq!(
            eval_one("PROPER(\"hELLO wORLD-test_case\")"),
            Value::Str("Hello World-Test_Case".to_string())
        );
        assert_eq!(
            eval_one("TEXT(1234.5, \"$#,##0\")"),
            Value::Str("$1,235".to_string())
        );
    }

    #[test]
    fn test_05_sheet_financial_functions_npv_db_ddb_irr_pmt() {
        // DB: 감가율 소수 셋째 자리 반올림 검증 (cost=1,000,000, salvage=100,000, life=6, period=1, month=7)
        // rate = round(1 - (0.1)^(1/6), 3) = 0.319 -> 1,000,000 * 0.319 * 7 / 12 = 186083.33333333334
        assert!(values_match_golden(
            &Value::Num(186083.33333333334),
            &eval_one("DB(1000000, 100000, 6, 1, 7)")
        ));

        // DDB: 마지막 기에 잔존가 아래로 내려가지 않는지 검증
        // cost=2400, salvage=300, life=10, period=10 -> 22.1225472
        assert!(values_match_golden(
            &Value::Num(22.1225472),
            &eval_one("DDB(2400, 300, 10, 10)")
        ));

        // NPV, IRR, MIRR, PMT, IPMT, PPMT
        let wb = make_single_sheet_wb(
            &[
                ("A1", Value::Num(-1000.0)),
                ("A2", Value::Num(300.0)),
                ("A3", Value::Num(420.0)),
                ("A4", Value::Num(680.0)),
            ],
            &[
                ("B1", "NPV(0.1, A2:A4) + A1"),
                ("B2", "IRR(A1:A4)"),
                ("B3", "PMT(0.05/12, 60, 10000)"),
                ("B4", "IPMT(0.05/12, 1, 60, 10000) + PPMT(0.05/12, 1, 60, 10000)"),
            ],
        );
        let res = evaluate_workbook(&wb);
        let irr = res.values[&CellAddr { sheet: 0, row: 2, col: 2 }].coerce_num().unwrap();
        assert!((irr - 0.1634056).abs() < 1e-5);

        let pmt = res.values[&CellAddr { sheet: 0, row: 3, col: 2 }].coerce_num().unwrap();
        let sum_ip_pp = res.values[&CellAddr { sheet: 0, row: 4, col: 2 }].coerce_num().unwrap();
        assert!((pmt - sum_ip_pp).abs() < 1e-9);
    }

    #[test]
    fn test_06_sheet_statistics_and_no_xlfn_prefix() {
        let wb = make_single_sheet_wb(
            &[
                ("A1", Value::Num(10.0)),
                ("A2", Value::Num(20.0)),
                ("A3", Value::Num(30.0)),
                ("A4", Value::Str("무시될문자".to_string())),
                ("B1", Value::Num(15.0)),
                ("B2", Value::Num(25.0)),
                ("B3", Value::Num(35.0)),
            ],
            &[
                ("C1", "STDEV.S(A1:A4)"),
                ("C2", "_xlfn.STDEV.S(A1:A4)"),
                ("C3", "VAR.S(A1:A4)"),
                ("C4", "PERCENTILE(A1:A3, 0.25)"),
                ("C5", "CORREL(A1:A3, B1:B3)"),
                ("C6", "FORECAST(40, B1:B3, A1:A3)"),
            ],
        );
        let res = evaluate_workbook(&wb);
        assert!(values_match_golden(
            &Value::Num(10.0),
            &res.values[&CellAddr { sheet: 0, row: 1, col: 3 }]
        ));
        assert!(values_match_golden(
            &Value::Num(10.0),
            &res.values[&CellAddr { sheet: 0, row: 2, col: 3 }]
        ));
        assert!(values_match_golden(
            &Value::Num(100.0),
            &res.values[&CellAddr { sheet: 0, row: 3, col: 3 }]
        ));
        assert!(values_match_golden(
            &Value::Num(15.0),
            &res.values[&CellAddr { sheet: 0, row: 4, col: 3 }]
        ));
        assert!(values_match_golden(
            &Value::Num(1.0),
            &res.values[&CellAddr { sheet: 0, row: 5, col: 3 }]
        ));
        assert!(values_match_golden(
            &Value::Num(45.0),
            &res.values[&CellAddr { sheet: 0, row: 6, col: 3 }]
        ));
    }

    #[test]
    fn test_dates_circular_ref_and_dynamic_offset() {
        let wb = make_single_sheet_wb(
            &[
                ("A1", Value::Num(100.0)),
                ("A2", Value::Num(250.0)),
            ],
            &[
                ("B1", "SUM(OFFSET(A1, 0, 0, 2, 1))"),
                ("B2", "EDATE(45322, 1)"),   // 2024-01-31 + 1개월 = 2024-02-29 (윤년) = 45351
                ("B3", "EOMONTH(44957, 1)"), // 2023-01-31 + 1개월 말일 = 2023-02-28 = 44985
                ("C1", "C2 + 10"),           // 순환 참조 C1 <-> C2
                ("C2", "C1 + 10"),
                ("D1", "XLOOKUP(1, A1:A2, A1:A2)"),
            ],
        );
        let res = evaluate_workbook(&wb);

        // OFFSET 동적 재계산 플래그 확인
        let b1 = CellAddr { sheet: 0, row: 1, col: 2 };
        assert!(res.dynamic_recalc_cells.contains(&b1));
        assert!(values_match_golden(&Value::Num(350.0), &res.values[&b1]));

        // EDATE / EOMONTH 월말 보정 확인
        assert!(values_match_golden(
            &Value::Num(45351.0),
            &res.values[&CellAddr { sheet: 0, row: 2, col: 2 }]
        ));
        assert!(values_match_golden(
            &Value::Num(44985.0),
            &res.values[&CellAddr { sheet: 0, row: 3, col: 2 }]
        ));

        // 순환 참조는 #VALUE! 대신 0으로 두고 circular_warnings에 기록
        let c1 = CellAddr { sheet: 0, row: 1, col: 3 };
        assert_eq!(res.values[&c1], Value::Num(0.0));
        assert!(res.circular_warnings.contains(&c1));

        // 동적 배열 미지원 표시 확인
        assert_eq!(
            res.values[&CellAddr { sheet: 0, row: 1, col: 4 }],
            Value::Str("아직 계산하지 않는 함수".to_string())
        );
    }

    // ---- Claude 검수 추가 (2026-11-11) ----
    #[test]
    fn review_round_is_half_away_not_ceiling() {
        assert_eq!(excel_round_away_from_zero(460595.2, 2).unwrap(), 460595.2);
        assert_eq!(excel_round_away_from_zero(200.0 * 2662.4 * (1.0 - 0.135), 2).unwrap(), 460595.2);
        assert_eq!(excel_round_away_from_zero(1.005, 2).unwrap(), 1.01);
        assert_eq!(excel_round_away_from_zero(0.285, 2).unwrap(), 0.29);
        assert_eq!(excel_round_away_from_zero(2.5, 0).unwrap(), 3.0);
        assert_eq!(excel_round_away_from_zero(-2.5, 0).unwrap(), -3.0);
        assert_eq!(excel_round_away_from_zero(2.4, 0).unwrap(), 2.0);
        assert_eq!(excel_round_away_from_zero(1234.5678, -2).unwrap(), 1200.0);
        assert_eq!(excel_round_away_from_zero(1250.0, -2).unwrap(), 1300.0);
    }

    #[test]
    fn review_serial_dates_1900() {
        for (s, ymd) in [
            (1i64, (1900, 1, 1)), (59, (1900, 2, 28)), (60, (1900, 2, 29)), (61, (1900, 3, 1)),
            (45291, (2023, 12, 31)), (45717, (2025, 3, 1)), (45686, (2025, 1, 29)),
        ] {
            assert_eq!(serial_to_ymd_1900(s), ymd, "serial {}", s);
            assert_eq!(ymd_to_serial_1900(ymd.0, ymd.1, ymd.2), s, "ymd {:?}", ymd);
        }
        assert_eq!(iso_to_serial("2025-01-09T00:00:00"), Some(45666.0));
    }

    #[test]
    fn review_approx_sub_snaps_noise() {
        assert_eq!(approx_add(5_000_000.0, -4_999_999.999_999_995_8), 0.0);
        assert_eq!(approx_add(0.1 + 0.2, -0.3), 0.0);
        assert_eq!(approx_add(1.0, -0.999_999), 1.0 - 0.999_999);
        assert_eq!(approx_add(1e-20, 0.0), 1e-20);
    }

    fn ev1(f: &str, cells: &[(&str, Value)]) -> Value {
        let wb = make_single_sheet_wb(cells, &[("Z99", f)]);
        let res = evaluate_workbook(&wb);
        res.values.get(&CellAddr { sheet: 0, row: 99, col: 26 }).cloned().unwrap_or(Value::Blank)
    }

    #[test]
    fn review_excel_rules_from_guide() {
        let n = |f: &str| ev1(f, &[]);
        assert_eq!(n("-2^2"), Value::Num(4.0));
        assert_eq!(n("2^3^2"), Value::Num(64.0));
        assert_eq!(n("1-2-3"), Value::Num(-4.0));
        assert_eq!(n("5%"), Value::Num(0.05));
        assert_eq!(n("50%*2"), Value::Num(1.0));
        assert_eq!(n("\"a\"=\"A\""), Value::Bool(true));
        assert_eq!(n("1<\"a\""), Value::Bool(true));
        assert_eq!(n("TRUE>\"a\""), Value::Bool(true));
        assert_eq!(n("IF(FALSE,1/0,2)"), Value::Num(2.0));
        assert_eq!(n("SUM(\"3\",4)"), Value::Num(7.0));
        assert_eq!(n("LEN(\"한글ABC\")"), Value::Num(5.0));
        assert_eq!(n("MID(\"가나다라\",2,2)"), Value::Str("나다".into()));
        assert_eq!(n("FIND(\"a\",\"Abca\")"), Value::Num(4.0));
        assert_eq!(n("PROPER(\"hELLO wORLD-foo\")"), Value::Str("Hello World-Foo".into()));
        assert_eq!(n("TEXT(1234.5,\"#,##0.0\")"), Value::Str("1,234.5".into()));
        assert_eq!(n("ROUND(PMT(0.05/12,60,-10000),2)"), Value::Num(188.71));
        assert_eq!(n("ROUND(IRR({-100,30,40,50,60}),4)"), Value::Num(0.2489));
        assert_eq!(n("ROUND(DB(1000000,100000,6,1,7),2)"), Value::Num(186083.33));
        assert_eq!(n("ROUND(DB(1000000,100000,6,2,7),2)"), Value::Num(259639.42));
        assert_eq!(n("ROUND(DB(1000000,100000,6,7,7),2)"), Value::Num(15845.10));
        assert_eq!(n("ROUND(SYD(30000,7500,10,1),2)"), Value::Num(4090.91));
        assert_eq!(n("QUARTILE({1,2,3,4},1)"), Value::Num(1.75));
        assert_eq!(n("ROUND(PERCENTILE({1,2,3,4},0.3),10)"), Value::Num(1.9));
        assert_eq!(n("STDEV.S(1)"), Value::Err(CalcErr::Div0));
        assert_eq!(n("ROUND(1234.5678,-2)"), Value::Num(1200.0));
        assert_eq!(n("EDATE(DATE(2025,1,31),1)"), n("DATE(2025,2,28)"));
    }

    #[test]
    fn review_garbage_never_panics() {
        let junk = [
            "", "(", ")", "1+", "SUM(", "A1:", "'", "'시트", "\"", "{", "{1,2", "=", "==1",
            "#N/A+1", "A1:A", "ZZZZZZ1", "A0", "A1048577", "SUM(A:A)", "SUM(1:1)", "1E999",
            "MID(\"가\",0,1)", "MID(\"가\",1,-1)", "REPT(\"a\",-1)", "REPT(\"a\",1e9)", "ADDRESS(0,0)",
            "INDEX(A1:B2,9,9)", "OFFSET(A1,-5,-5)", "OFFSET(A1,0,0,1e9,1e9)", "LARGE({1},0)",
            "IRR({1,2,3})", "IRR({})", "DB(0,0,0,0)", "DATEDIF(5,1,\"Y\")", "TEXT(1,\"[\")",
            "LOG10(-1)", "1/0", "0^-1", "(-8)^(1/3)", "WEEKDAY(-1)", "EOMONTH(-1,-99999)",
            "Sheet9!A1", "없는이름", "SUM(((((((((((1)))))))))))", "1 2", "A1 B1", "(A1,B1)",
        ];
        for f in junk {
            let _ = ev1(f, &[]);
        }
        let deep = format!("{}1{}", "(".repeat(20000), ")".repeat(20000));
        assert!(matches!(ev1(&deep, &[]), Value::Err(_)));
        let long = "1+".repeat(200_000) + "1";
        assert!(matches!(ev1(&long, &[]), Value::Err(_)));
        // 엑셀 한도 안의 긴 덧셈(1,000항)은 계산돼야 한다
        let ok = "1+".repeat(999) + "1";
        assert_eq!(ev1(&ok, &[]), Value::Num(1000.0));
        // 긴 참조 사슬: A1=A2+1 ... A6000=1
        let mut fs: Vec<(String, String)> = (1..6000).map(|i| (format!("A{}", i), format!("A{}+1", i + 1))).collect();
        fs.push(("A6000".into(), "1".into()));
        let refs: Vec<(&str, &str)> = fs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let wb = make_single_sheet_wb(&[], &refs);
        let _ = evaluate_workbook(&wb);
    }
}
