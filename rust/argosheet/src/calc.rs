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

    pub fn error_type_num(self) -> f64 {
        match self {
            CalcErr::Null => 1.0,
            CalcErr::Div0 => 2.0,
            CalcErr::Value => 3.0,
            CalcErr::Ref => 4.0,
            CalcErr::Name => 5.0,
            CalcErr::Num => 6.0,
            CalcErr::Na => 7.0,
        }
    }
}

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

    pub fn coerce_bool(&self) -> Result<bool, CalcErr> {
        match self {
            Value::Bool(b) => Ok(*b),
            Value::Blank => Ok(false),
            Value::Num(n) => Ok(*n != 0.0),
            Value::Str(s) => {
                let lower = s.trim().to_ascii_lowercase();
                if lower == "true" {
                    Ok(true)
                } else if lower == "false" {
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
// 2. 파서 및 AST
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Num(f64),
    Str(String),
    Bool(bool),
    ErrorLit(CalcErr),
    Ident(String),
    Colon,
    Intersect,
    Comma,
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
    Range,
    Intersect,
    Union,
    Pow,
    Mul,
    Div,
    Add,
    Sub,
    Concat,
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
    Percent,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Err(CalcErr),
    EmptyArg,
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

        if had_whitespace {
            if let Some(prev) = tokens.last() {
                let prev_can_end_ref = matches!(prev, Token::Ident(_) | Token::RParen);
                let next_can_start_ref =
                    ch == '(' || ch == '\'' || ch == '$' || ch.is_ascii_alphabetic() || ch == '_';
                if prev_can_end_ref && next_can_start_ref && !looks_like_func_call(&chars, i) {
                    tokens.push(Token::Intersect);
                }
            }
            had_whitespace = false;
        }

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

        if ch.is_ascii_digit() || (ch == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < len && (chars[i] == 'e' || chars[i] == 'E') {
                if i + 1 < len
                    && (chars[i + 1] == '+' || chars[i + 1] == '-' || chars[i + 1].is_ascii_digit())
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
                    self.next();
                    let saved_depth = self.paren_depth;
                    self.paren_depth = 0;
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
// 3. 의존 그래프 & 계산기 코어
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellAddr {
    pub sheet: usize,
    pub row: u32,
    pub col: u32,
}

pub struct WorkbookCalcResult {
    pub values: HashMap<CellAddr, Value>,
    pub dynamic_recalc_cells: HashSet<CellAddr>,
    pub circular_warnings: Vec<CellAddr>,
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
                    deps.retain(|d| {
                        wb.sheets
                            .get(d.sheet)
                            .map(|sh| sh.formulas.contains_key(&crate::cell_key(d.row, d.col)))
                            .unwrap_or(false)
                    });
                    deps.sort_by_key(|a| (a.sheet, a.row, a.col));
                    deps.dedup();
                    deps.shrink_to_fit();
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

    let mut evaluator = Evaluator {
        wb,
        ast_map: &ast_map,
        defined_ast: &defined_ast,
        values: &mut values,
        visiting: HashSet::new(),
        circular_warnings: &mut circular_warnings,
        range_cache: RangeCache::default(),
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

/// 큰 스택(64MB) 스레드에서 f 를 돌린다. 수식 계산은 재귀가 깊어 tvOS 보조 스레드의
/// 기본 스택(512KB)에서는 넘칠 수 있다. 스레드를 못 만들거나 f 가 패닉하면 None.
pub fn run_on_big_stack<T: Send>(f: impl FnOnce() -> T + Send) -> Option<T> {
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("argosheet-work".into())
            .stack_size(CALC_STACK_BYTES)
            .spawn_scoped(s, f)
            .ok()
            .and_then(|h| h.join().ok())
    })
}

struct StandaloneState {
    values: HashMap<CellAddr, Value>,
    defined_ast: HashMap<String, (Option<usize>, Expr)>,
}

thread_local! {
    static STANDALONE: std::cell::RefCell<Option<StandaloneState>> = const { std::cell::RefCell::new(None) };
}

/// 조건부 서식처럼 "시트 밖 수식"을 여러 번 계산할 때 여는 세션.
/// 계산값 표와 이름 정의를 한 번만 준비해 두고, 닫힐 때(Drop) 버린다.
/// 세션 없이 eval_standalone_expr 를 부르면 그때마다 준비한다(느리지만 결과는 같다).
pub struct StandaloneSession;

impl StandaloneSession {
    pub fn begin(wb: &Workbook, calc_res: &WorkbookCalcResult) -> Self {
        let state = build_standalone_state(wb, calc_res);
        STANDALONE.with(|s| *s.borrow_mut() = Some(state));
        StandaloneSession
    }
}

impl Drop for StandaloneSession {
    fn drop(&mut self) {
        STANDALONE.with(|s| *s.borrow_mut() = None);
    }
}

fn build_standalone_state(wb: &Workbook, calc_res: &WorkbookCalcResult) -> StandaloneState {
    let mut defined_ast: HashMap<String, (Option<usize>, Expr)> = HashMap::new();
    for dn in &wb.defined_names {
        if let Ok(e) = FormulaParser::parse_str(&dn.formula) {
            defined_ast.insert(
                dn.name.to_ascii_uppercase(),
                (dn.local_sheet_id.map(|i| i as usize), e),
            );
        }
    }
    StandaloneState {
        values: calc_res.values.clone(),
        defined_ast,
    }
}

/// 조건부 서식(expression, cellIs)에서 이동된 수식 구문 트리를 즉시 계산한다.
/// 왜: 시트 셀에 저장된 수식과 달리 조건부 서식 수식은 범위 내 각 셀 좌표를 기준으로 상대 평가되어야 한다.
pub fn eval_standalone_expr(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    caller: CellAddr,
    expr: &Expr,
) -> Value {
    // 세션 상태를 잠깐 꺼내 쓰고 되돌린다(계산 도중 재진입해도 빌림 충돌이 없게).
    let taken = STANDALONE.with(|s| s.borrow_mut().take());
    let had_session = taken.is_some();
    let mut state = taken.unwrap_or_else(|| build_standalone_state(wb, calc_res));
    let empty_ast: HashMap<CellAddr, Expr> = HashMap::new();
    let mut warnings = Vec::new();
    let out = {
        let mut ev = Evaluator {
            wb,
            ast_map: &empty_ast,
            defined_ast: &state.defined_ast,
            values: &mut state.values,
            visiting: HashSet::new(),
            circular_warnings: &mut warnings,
            range_cache: RangeCache::default(),
        };
        ev.eval_scalar(caller, expr)
    };
    if had_session {
        STANDALONE.with(|s| *s.borrow_mut() = Some(state));
    }
    out
}

fn contains_dynamic_ref_func(expr: &Expr) -> bool {
    match expr {
        Expr::Func { name, args } => {
            if matches!(
                name.as_str(),
                "OFFSET" | "INDIRECT" | "RAND" | "RANDBETWEEN" | "TODAY" | "NOW"
            ) {
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

/// 범위 안의 **수식 칸만** 의존성으로 넣는다(값 칸은 어차피 걸러 낸다).
/// 예전에는 범위 칸을 전부 넣고 나중에 걸렀는데, `SUMIFS(D:D,B:B,..)` 400개 × 2만 행이면
/// 걸러도 Vec 용량이 남아 377MB 를 먹었다. 범위가 수식 수보다 크면 수식 목록 쪽을 훑는다.
fn push_range_formula_deps(wb: &Workbook, rr: &RefRange, out: &mut Vec<CellAddr>) {
    let Some(sheet) = wb.sheets.get(rr.sheet_idx) else { return };
    if sheet.formulas.is_empty() {
        return;
    }
    let cells = (rr.end_row.saturating_sub(rr.start_row) as u64 + 1)
        * (rr.end_col.saturating_sub(rr.start_col) as u64 + 1);
    if cells <= sheet.formulas.len() as u64 {
        for r in rr.start_row..=rr.end_row {
            for c in rr.start_col..=rr.end_col {
                if sheet.formulas.contains_key(&crate::cell_key(r, c)) {
                    out.push(CellAddr { sheet: rr.sheet_idx, row: r, col: c });
                }
            }
        }
    } else {
        for &key in sheet.formulas.keys() {
            let r = (key >> 32) as u32;
            let c = (key & 0xFFFF_FFFF) as u32;
            if r >= rr.start_row && r <= rr.end_row && c >= rr.start_col && c <= rr.end_col {
                out.push(CellAddr { sheet: rr.sheet_idx, row: r, col: c });
            }
        }
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
                push_range_formula_deps(wb, &rr, out);
                return;
            }
            collect_static_dependencies(wb, cur_sheet, left, defined_ast, out);
            collect_static_dependencies(wb, cur_sheet, right, defined_ast, out);
        }
        Expr::RefOrName { sheet, name } => {
            if let Some(rr) = resolve_ref_token(wb, cur_sheet, sheet.as_deref(), name) {
                push_range_formula_deps(wb, &rr, out);
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

fn resolve_sheet_index(
    wb: &Workbook,
    current_sheet: usize,
    sheet_name: Option<&str>,
) -> Option<usize> {
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

    if let Some((r, c)) = parse_cell_ref_1based(clean) {
        return Some(RefRange {
            sheet_idx: s_idx,
            start_row: r,
            start_col: c,
            end_row: r,
            end_col: c,
        });
    }

    let no_dollar = clean.replace('$', "");
    if !no_dollar.is_empty() && no_dollar.chars().all(|c| c.is_ascii_alphabetic()) {
        if no_dollar.len() > 3 {
            return None; // XFD(16384) 보다 긴 열 이름 — 곱하다 넘치지 않게 먼저 거른다
        }
        let mut col = 0u32;
        for b in no_dollar.bytes() {
            col = col * 26 + (b.to_ascii_uppercase() - b'A' + 1) as u32;
        }
        if col > 16384 {
            return None;
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
        if row == 0 || row > 1_048_576 {
            return None;
        }
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
// 4. 평가기 & 120개 함수 완전 디스패처
// ============================================================================

struct Evaluator<'a> {
    wb: &'a Workbook,
    ast_map: &'a HashMap<CellAddr, Expr>,
    defined_ast: &'a HashMap<String, (Option<usize>, Expr)>,
    values: &'a mut HashMap<CellAddr, Value>,
    visiting: HashSet<CellAddr>,
    circular_warnings: &'a mut Vec<CellAddr>,
    range_cache: RangeCache,
}

/// SUMIFS·COUNTIFS 가 같은 범위를 수식마다 다시 펼치지 않도록 펼친 값을 들고 있는다.
/// 큰 범위(1,024칸 이상)는 조건 후보를 빨리 고르는 색인도 만든다.
/// 색인은 "후보"만 준다 — 최종 판정은 언제나 CriteriaMatcher 로 다시 한다(엑셀과 결과가 같도록).
#[derive(Default)]
struct RangeCache {
    flats: HashMap<RefRange, std::rc::Rc<Vec<Value>>>,
    indexes: HashMap<RefRange, std::rc::Rc<ColCriteriaIndex>>,
    cached_cells: usize,
}

/// 캐시에 들고 있을 칸 수 상한(메모리 보호). 넘으면 캐시 없이 매번 펼친다.
const RANGE_CACHE_MAX_CELLS: usize = 4_000_000;
const RANGE_INDEX_MIN_LEN: usize = 1024;

/// 조건 후보 색인. 같음(숫자·글자) 묶음과 숫자 정렬 목록.
pub struct ColCriteriaIndex {
    len: usize,
    eq_buckets: HashMap<String, Vec<u32>>,
    sorted_nums: Vec<(f64, u32)>,
}

impl ColCriteriaIndex {
    fn num_key(n: f64) -> String {
        // 15자리로 맞춘 키 — 같은 값이면 같은 키(후보만 고르므로 약간 넓어도 된다)
        format!("N:{:.14e}", n)
    }

    pub fn build(values: &[Value]) -> Self {
        let mut eq_buckets: HashMap<String, Vec<u32>> = HashMap::new();
        let mut sorted_nums: Vec<(f64, u32)> = Vec::new();
        for (idx, v) in values.iter().enumerate() {
            let off = idx as u32;
            match v {
                Value::Str(s) if s.is_empty() => {}
                Value::Num(n) if n.is_finite() => {
                    eq_buckets.entry(Self::num_key(*n)).or_default().push(off);
                    sorted_nums.push((*n, off));
                }
                Value::Str(s) => {
                    if let Ok(n) = s.trim().parse::<f64>() {
                        if n.is_finite() {
                            eq_buckets.entry(Self::num_key(n)).or_default().push(off);
                            sorted_nums.push((n, off));
                        }
                    }
                    eq_buckets.entry(format!("S:{}", s.to_lowercase())).or_default().push(off);
                }
                _ => {}
            }
        }
        sorted_nums.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
        Self { len: values.len(), eq_buckets, sorted_nums }
    }

    /// 조건에 맞을 수 있는 오프셋들(상위 집합). 자신 없으면 None → 전체를 훑는다.
    pub fn candidates(&self, crit: &Value) -> Option<Vec<u32>> {
        let num_eq = |n: f64| -> Vec<u32> {
            self.eq_buckets.get(&Self::num_key(n)).cloned().unwrap_or_default()
        };
        match crit {
            Value::Num(n) if n.is_finite() => Some(num_eq(*n)),
            Value::Str(s) => {
                if s.is_empty() || s.contains('*') || s.contains('?') || s.contains('~') || s.starts_with("<>") {
                    return None;
                }
                let range_q = |op: &str, rest: &str| -> Option<Vec<u32>> {
                    let target = rest.trim().parse::<f64>().ok()?;
                    if !target.is_finite() {
                        return None;
                    }
                    // 경계는 넉넉히(±1e-9 상대) 잡고 정확한 판정은 matcher 에 맡긴다.
                    let eps = target.abs().max(1.0) * 1e-9;
                    let v = &self.sorted_nums;
                    let out: Vec<u32> = match op {
                        ">" | ">=" => v[v.partition_point(|&(x, _)| x < target - eps)..].iter().map(|&(_, o)| o).collect(),
                        _ => v[..v.partition_point(|&(x, _)| x <= target + eps)].iter().map(|&(_, o)| o).collect(),
                    };
                    Some(out)
                };
                for op in [">=", "<=", ">", "<"] {
                    if let Some(rest) = s.strip_prefix(op) {
                        return range_q(op, rest);
                    }
                }
                let clean = s.strip_prefix('=').unwrap_or(s.as_str());
                if clean.is_empty() {
                    return None; // "=" 은 빈 칸 판정이 미묘하다 — 전체를 훑는다
                }
                if let Ok(n) = clean.trim().parse::<f64>() {
                    if n.is_finite() {
                        let mut out = num_eq(n);
                        out.extend(self.eq_buckets.get(&format!("S:{}", clean.to_lowercase())).cloned().unwrap_or_default());
                        out.sort_unstable();
                        out.dedup();
                        return Some(out);
                    }
                }
                Some(self.eq_buckets.get(&format!("S:{}", clean.to_lowercase())).cloned().unwrap_or_default())
            }
            _ => None, // 논리값·빈 칸·오류 조건은 전체를 훑는다
        }
    }
}

impl<'a> Evaluator<'a> {
    /// 범위 인수를 펼친다. 같은 범위는 한 번만 펼쳐 캐시에서 준다.
    fn cached_range_flat(
        &mut self,
        caller: CellAddr,
        expr: &Expr,
    ) -> Result<(Option<RefRange>, std::rc::Rc<Vec<Value>>), CalcErr> {
        let v = self.eval_expr(caller, expr);
        if let Value::Err(e) = v {
            return Err(e);
        }
        if let Value::Ref(rr) = v {
            if let Some(f) = self.range_cache.flats.get(&rr) {
                return Ok((Some(rr), f.clone()));
            }
            let flat = std::rc::Rc::new(self.flatten_value(Value::Ref(rr)));
            if self.range_cache.cached_cells + flat.len() <= RANGE_CACHE_MAX_CELLS {
                self.range_cache.cached_cells += flat.len();
                self.range_cache.flats.insert(rr, flat.clone());
            }
            return Ok((Some(rr), flat));
        }
        Ok((None, std::rc::Rc::new(self.flatten_value(v))))
    }

    /// 큰 범위의 조건 후보(상위 집합). 색인을 만들 수 없거나 조건이 애매하면 None.
    fn criteria_candidates(&mut self, rr: Option<RefRange>, flat: &[Value], crit: &Value) -> Option<Vec<u32>> {
        let rr = rr?;
        if flat.len() < RANGE_INDEX_MIN_LEN || !self.range_cache.flats.contains_key(&rr) {
            return None;
        }
        let idx = match self.range_cache.indexes.get(&rr) {
            Some(i) => i.clone(),
            None => {
                let i = std::rc::Rc::new(ColCriteriaIndex::build(flat));
                self.range_cache.indexes.insert(rr, i.clone());
                i
            }
        };
        if idx.len != flat.len() {
            return None;
        }
        idx.candidates(crit)
    }

    fn ensure_evaluated(&mut self, addr: CellAddr) -> Value {
        if let Some(v) = self.values.get(&addr) {
            return v.clone();
        }
        if self.visiting.contains(&addr) {
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
        // 열 전체(A:A)·시트 전체(A:XFD) 같은 큰 범위는 그대로 펼치면 수십억 칸이 되어
        // 메모리가 터진다. 100만 칸을 넘으면 시트에서 실제로 쓰는 영역까지만 자른다
        // (그 밖은 빈 칸이라 집계 결과는 같다). 최소 1칸은 남긴다.
        let mut rr = rr;
        let cells = (rr.end_row.saturating_sub(rr.start_row) as u64 + 1)
            * (rr.end_col.saturating_sub(rr.start_col) as u64 + 1);
        if cells > 1_000_000 {
            let (mr, mc) = self
                .wb
                .sheets
                .get(rr.sheet_idx)
                .map(|s| (s.max_row.max(1), s.max_col.max(1)))
                .unwrap_or((1, 1));
            rr.end_row = rr.end_row.min(mr).max(rr.start_row);
            rr.end_col = rr.end_col.min(mc).max(rr.start_col);
            let clipped = (rr.end_row - rr.start_row + 1) as u64 * (rr.end_col - rr.start_col + 1) as u64;
            if clipped > 20_000_000 {
                rr.end_row = rr.start_row;
                rr.end_col = rr.start_col;
            }
        }
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

    fn eval_function(&mut self, caller: CellAddr, name: &str, args: &[Expr]) -> Value {
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
        let clean_name = name;
        match name {
            // ----------------------------------------------------------------
            // [1] 논리·정보 (11개)
            // ----------------------------------------------------------------
            "IF" => {
                if args.is_empty() || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let cond = self.eval_scalar(caller, &args[0]).coerce_bool()?;
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
            "IFS" => {
                if args.is_empty() || args.len() % 2 != 0 {
                    return Err(CalcErr::Value);
                }
                for chunk in args.chunks(2) {
                    let cond = self.eval_scalar(caller, &chunk[0]).coerce_bool()?;
                    if cond {
                        return Ok(self.eval_expr(caller, &chunk[1]));
                    }
                }
                Err(CalcErr::Na)
            }
            "SWITCH" => {
                if args.len() < 3 {
                    return Err(CalcErr::Value);
                }
                let target = self.eval_scalar(caller, &args[0]);
                let has_default = (args.len() - 1) % 2 != 0;
                let pairs_end = if has_default {
                    args.len() - 1
                } else {
                    args.len()
                };
                for i in (1..pairs_end).step_by(2) {
                    let case_val = self.eval_scalar(caller, &args[i]);
                    if excel_compare_values(&target, &case_val) == Ordering::Equal {
                        return Ok(self.eval_expr(caller, &args[i + 1]));
                    }
                }
                if has_default {
                    Ok(self.eval_expr(caller, args.last().unwrap()))
                } else {
                    Err(CalcErr::Na)
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
            "NOT" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let b = self.eval_scalar(caller, &args[0]).coerce_bool()?;
                Ok(Value::Bool(!b))
            }
            "XOR" => {
                if args.is_empty() {
                    return Err(CalcErr::Value);
                }
                let mut true_count = 0usize;
                let mut found_any = false;
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        match item {
                            Value::Err(e) => return Err(e),
                            Value::Bool(b) => {
                                found_any = true;
                                if b {
                                    true_count += 1;
                                }
                            }
                            Value::Num(n) => {
                                found_any = true;
                                if n != 0.0 {
                                    true_count += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if !found_any {
                    Err(CalcErr::Value)
                } else {
                    Ok(Value::Bool(true_count % 2 == 1))
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
            "IFNA" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                if matches!(v, Value::Err(CalcErr::Na)) {
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
            "ISBLANK" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Blank)))
            }
            "ISERR" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(
                    matches!(v, Value::Err(e) if e != CalcErr::Na),
                ))
            }
            "ISERROR" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Err(_))))
            }
            "ISNA" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Err(CalcErr::Na))))
            }
            "ISLOGICAL" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                Ok(Value::Bool(matches!(v, Value::Bool(_))))
            }
            "NA" => Ok(Value::Err(CalcErr::Na)),
            "ERROR.TYPE" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                match v {
                    Value::Err(e) => Ok(Value::Num(e.error_type_num())),
                    _ => Err(CalcErr::Na),
                }
            }

            // ----------------------------------------------------------------
            // [2] 수학·삼각 (23개)
            // ----------------------------------------------------------------
            "SUM" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                Ok(Value::Num(nums.into_iter().sum()))
            }
            "PRODUCT" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                if nums.is_empty() {
                    Ok(Value::Num(0.0))
                } else {
                    Ok(Value::Num(nums.into_iter().product()))
                }
            }
            "ABS" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                Ok(Value::Num(n.abs()))
            }
            "SIGN" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                if n > 0.0 {
                    Ok(Value::Num(1.0))
                } else if n < 0.0 {
                    Ok(Value::Num(-1.0))
                } else {
                    Ok(Value::Num(0.0))
                }
            }
            "ROUND" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let digits = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                Ok(Value::Num(excel_round_away_from_zero(n, digits)?))
            }
            "ROUNDUP" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let digits = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                Ok(Value::Num(excel_round_up(n, digits)?))
            }
            "ROUNDDOWN" | "TRUNC" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let digits = if args.len() == 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32
                } else {
                    0
                };
                Ok(Value::Num(excel_round_down(n, digits)?))
            }
            "INT" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                Ok(Value::Num(n.floor()))
            }
            "MOD" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let d = self.eval_scalar(caller, &args[1]).coerce_num()?;
                if d == 0.0 {
                    return Err(CalcErr::Div0);
                }
                Ok(Value::Num(n - d * (n / d).floor()))
            }
            "POWER" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let base = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let exp = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let res = base.powf(exp);
                if res.is_nan() || res.is_infinite() {
                    Err(CalcErr::Num)
                } else {
                    Ok(Value::Num(res))
                }
            }
            "SQRT" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                if n < 0.0 {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(n.sqrt()))
            }
            "EXP" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let r = n.exp();
                if r.is_infinite() {
                    Err(CalcErr::Num)
                } else {
                    Ok(Value::Num(r))
                }
            }
            "LN" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                if n <= 0.0 {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(n.ln()))
            }
            "LOG" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let base = if args.len() == 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?
                } else {
                    10.0
                };
                if n <= 0.0 || base <= 0.0 || base == 1.0 {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(n.log(base)))
            }
            "LOG10" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                if n <= 0.0 {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num(n.log10()))
            }
            "PI" => Ok(Value::Num(std::f64::consts::PI)),
            "CEILING" | "FLOOR" => {
                // 옛 함수: 기준값 생략 불가(엑셀은 CEILING(x) 를 받지 않음), 양수에 음수 기준이면 #NUM!
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let sig = self.eval_scalar(caller, &args[1]).coerce_num()?;
                if sig == 0.0 || n == 0.0 {
                    return Ok(Value::Num(0.0));
                }
                if n > 0.0 && sig < 0.0 {
                    return Err(CalcErr::Num);
                }
                let up = clean_quotient(n / sig);
                let q = if clean_name == "CEILING" { up.ceil() } else { up.floor() };
                Ok(Value::Num(clean_product(q, sig)))
            }
            "CEILING.MATH" | "FLOOR.MATH" => {
                // 기준값은 절댓값, 음수는 mode 0 이면 (CEILING: 0 쪽 / FLOOR: 0 에서 먼 쪽),
                // mode 가 0 이 아니면 반대.
                if args.is_empty() || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let sig = if args.len() >= 2 && !matches!(args[1], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.abs()
                } else {
                    1.0
                };
                let mode = if args.len() == 3 && !matches!(args[2], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[2]).coerce_num()?
                } else {
                    0.0
                };
                if sig == 0.0 || n == 0.0 {
                    return Ok(Value::Num(0.0));
                }
                let q = clean_quotient(n / sig);
                let ceiling = clean_name == "CEILING.MATH";
                let r = if n >= 0.0 || mode == 0.0 {
                    if ceiling { q.ceil() } else { q.floor() }
                } else if ceiling {
                    q.floor()
                } else {
                    q.ceil()
                };
                Ok(Value::Num(clean_product(r, sig)))
            }
            "MROUND" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let m = self.eval_scalar(caller, &args[1]).coerce_num()?;
                if m == 0.0 {
                    return Ok(Value::Num(0.0));
                }
                if (n > 0.0 && m < 0.0) || (n < 0.0 && m > 0.0) {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num((n / m).round() * m))
            }
            "RAND" => Ok(Value::Num(next_random())),
            "RANDBETWEEN" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let b = self.eval_scalar(caller, &args[0]).coerce_num()?.ceil();
                let t = self.eval_scalar(caller, &args[1]).coerce_num()?.floor();
                if b > t || !b.is_finite() || !t.is_finite() {
                    return Err(CalcErr::Num);
                }
                Ok(Value::Num((b + (next_random() * (t - b + 1.0)).floor()).min(t)))
            }
            "MAX" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
                Ok(Value::Num(nums.into_iter().fold(f64::NEG_INFINITY, f64::max)).map_inf_to_zero())
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
            // [3] 조건 집계 (11개)
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
            "COUNTBLANK" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_expr(caller, &args[0]);
                let cnt = self
                    .flatten_value(v)
                    .into_iter()
                    .filter(|item| match item {
                        Value::Blank => true,
                        Value::Str(s) => s.is_empty(),
                        _ => false,
                    })
                    .count();
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
            "AVERAGEA" => {
                let mut sum = 0.0;
                let mut cnt = 0usize;
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        match item {
                            Value::Num(n) => {
                                sum += n;
                                cnt += 1;
                            }
                            Value::Bool(b) => {
                                sum += if b { 1.0 } else { 0.0 };
                                cnt += 1;
                            }
                            Value::Str(_) => {
                                cnt += 1;
                            }
                            Value::Blank => {}
                            Value::Err(e) => return Err(e),
                            _ => {}
                        }
                    }
                }
                if cnt == 0 {
                    Err(CalcErr::Div0)
                } else {
                    Ok(Value::Num(sum / (cnt as f64)))
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
                let mut best: Option<Vec<u32>> = None;
                for p in 0..pair_cnt {
                    let (rr, flat) = self.cached_range_flat(caller, &args[p * 2])?;
                    let crit = self.eval_scalar(caller, &args[p * 2 + 1]);
                    if let Some(c) = self.criteria_candidates(rr, &flat, &crit) {
                        if best.as_ref().map(|b| c.len() < b.len()).unwrap_or(true) {
                            best = Some(c);
                        }
                    }
                    cols_data.push(flat);
                    matchers.push(CriteriaMatcher::parse(&crit));
                }
                let len = cols_data[0].len();
                for c in &cols_data[1..] {
                    if c.len() != len {
                        return Err(CalcErr::Value);
                    }
                }
                let ok = |i: usize| (0..pair_cnt).all(|p| matchers[p].matches(&cols_data[p][i]));
                let count = match best {
                    Some(cands) => cands.iter().filter(|&&i| (i as usize) < len && ok(i as usize)).count(),
                    None => (0..len).filter(|&i| ok(i)).count(),
                };
                Ok(Value::Num(count as f64))
            }
            "SUMIFS" => {
                if args.len() < 3 || (args.len() - 1) % 2 != 0 {
                    return Err(CalcErr::Value);
                }
                let (_, sum_vals) = self.cached_range_flat(caller, &args[0])?;
                let pair_cnt = (args.len() - 1) / 2;
                let mut cols_data = Vec::with_capacity(pair_cnt);
                let mut matchers = Vec::with_capacity(pair_cnt);
                let mut best: Option<Vec<u32>> = None;
                for p in 0..pair_cnt {
                    let (rr, flat) = self.cached_range_flat(caller, &args[1 + p * 2])?;
                    let crit = self.eval_scalar(caller, &args[2 + p * 2]);
                    if let Some(c) = self.criteria_candidates(rr, &flat, &crit) {
                        if best.as_ref().map(|b| c.len() < b.len()).unwrap_or(true) {
                            best = Some(c);
                        }
                    }
                    cols_data.push(flat);
                    matchers.push(CriteriaMatcher::parse(&crit));
                }
                let len = sum_vals.len();
                for c in &cols_data {
                    if c.len() != len {
                        return Err(CalcErr::Value);
                    }
                }
                let ok = |i: usize| (0..pair_cnt).all(|p| matchers[p].matches(&cols_data[p][i]));
                // 더하는 순서를 원래(행 순서)와 같게 해서 부동소수 결과가 바뀌지 않게 한다.
                let mut total = 0.0;
                match best {
                    Some(mut cands) => {
                        cands.sort_unstable();
                        for i in cands {
                            let i = i as usize;
                            if i < len && ok(i) {
                                if let Value::Num(n) = sum_vals[i] {
                                    total += n;
                                }
                            }
                        }
                    }
                    None => {
                        for i in 0..len {
                            if ok(i) {
                                if let Value::Num(n) = sum_vals[i] {
                                    total += n;
                                }
                            }
                        }
                    }
                }
                Ok(Value::Num(total))
            }
            "MAXIFS" | "MINIFS" => {
                if args.len() < 3 || (args.len() - 1) % 2 != 0 {
                    return Err(CalcErr::Value);
                }
                let target_vals = self.eval_range_flat(caller, &args[0])?;
                let pair_cnt = (args.len() - 1) / 2;
                let mut cols_data = Vec::with_capacity(pair_cnt);
                let mut matchers = Vec::with_capacity(pair_cnt);
                for p in 0..pair_cnt {
                    cols_data.push(self.eval_range_flat(caller, &args[1 + p * 2])?);
                    let crit = self.eval_scalar(caller, &args[2 + p * 2]);
                    matchers.push(CriteriaMatcher::parse(&crit));
                }
                let len = target_vals.len();
                for c in &cols_data {
                    if c.len() != len {
                        return Err(CalcErr::Value);
                    }
                }
                let mut matched_nums = Vec::new();
                for i in 0..len {
                    if (0..pair_cnt).all(|p| matchers[p].matches(&cols_data[p][i])) {
                        if let Value::Num(n) = target_vals[i] {
                            matched_nums.push(n);
                        }
                    }
                }
                if name == "MAXIFS" {
                    Ok(Value::Num(
                        matched_nums.into_iter().fold(f64::NEG_INFINITY, f64::max),
                    )
                    .map_inf_to_zero())
                } else if matched_nums.is_empty() {
                    Ok(Value::Num(0.0))
                } else {
                    Ok(Value::Num(
                        matched_nums.into_iter().fold(f64::INFINITY, f64::min),
                    ))
                }
            }
            "SUBTOTAL" => self.eval_subtotal(caller, args),

            // ----------------------------------------------------------------
            // [4] 조회·참조 (11개)
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
            "HLOOKUP" => {
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
                let row_idx = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as isize;
                let approx = if args.len() == 4 && !matches!(args[3], Expr::EmptyArg) {
                    self.eval_scalar(caller, &args[3]).coerce_bool()?
                } else {
                    true
                };
                if row_idx < 1 || (row_idx as usize) > table.len() {
                    return Err(CalcErr::Ref);
                }
                let first_row = table[0].clone();
                let m_type = if approx { 1 } else { 0 };
                match match_index(&lookup, &first_row, m_type) {
                    Some(c_1based) => Ok(table[(row_idx as usize) - 1][c_1based - 1].clone()),
                    None => Err(CalcErr::Na),
                }
            }
            "LOOKUP" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let lookup = self.eval_scalar(caller, &args[0]);
                let lookup_vec = self.eval_range_flat(caller, &args[1])?;
                let res_vec = if args.len() == 3 {
                    self.eval_range_flat(caller, &args[2])?
                } else {
                    lookup_vec.clone()
                };
                match match_index(&lookup, &lookup_vec, 1) {
                    Some(idx) => Ok(res_vec.get(idx - 1).cloned().unwrap_or(Value::Blank)),
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
            "ROW" => {
                if args.is_empty() {
                    Ok(Value::Num(caller.row as f64))
                } else {
                    match self.eval_expr(caller, &args[0]) {
                        Value::Ref(rr) => Ok(Value::Num(rr.start_row as f64)),
                        _ => Err(CalcErr::Value),
                    }
                }
            }
            "COLUMN" => {
                if args.is_empty() {
                    Ok(Value::Num(caller.col as f64))
                } else {
                    match self.eval_expr(caller, &args[0]) {
                        Value::Ref(rr) => Ok(Value::Num(rr.start_col as f64)),
                        _ => Err(CalcErr::Value),
                    }
                }
            }
            "ROWS" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                match self.eval_expr(caller, &args[0]) {
                    Value::Ref(rr) => Ok(Value::Num((rr.end_row - rr.start_row + 1) as f64)),
                    Value::Array(rows) => Ok(Value::Num(rows.len() as f64)),
                    _ => Ok(Value::Num(1.0)),
                }
            }
            "COLUMNS" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                match self.eval_expr(caller, &args[0]) {
                    Value::Ref(rr) => Ok(Value::Num((rr.end_col - rr.start_col + 1) as f64)),
                    Value::Array(rows) => {
                        Ok(Value::Num(rows.first().map(|r| r.len()).unwrap_or(0) as f64))
                    }
                    _ => Ok(Value::Num(1.0)),
                }
            }

            // ----------------------------------------------------------------
            // [5] 문자열 (19개)
            // ----------------------------------------------------------------
            "LEN" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                Ok(Value::Num(s.chars().count() as f64))
            }
            "TRIM" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
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
            "SEARCH" => {
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
                if start_num < 1 || (start_num as usize) > within_chars.len().max(1) {
                    return Err(CalcErr::Value);
                }
                let start_idx = (start_num as usize) - 1;
                for pos in start_idx..within_chars.len() {
                    let sub: String = within_chars[pos..].iter().collect();
                    if wildcard_match_ci(&format!("{}*", find_s), &sub) {
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
            "REPLACE" => {
                if args.len() != 4 {
                    return Err(CalcErr::Value);
                }
                let old_t = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let start = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as isize;
                let count = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as isize;
                let new_t = self.eval_scalar(caller, &args[3]).coerce_str()?;
                if start < 1 || count < 0 {
                    return Err(CalcErr::Value);
                }
                let chars: Vec<char> = old_t.chars().collect();
                let st = ((start - 1) as usize).min(chars.len());
                let en = (st + (count as usize)).min(chars.len());
                let mut out: String = chars[..st].iter().collect();
                out.push_str(&new_t);
                out.extend(&chars[en..]);
                Ok(Value::Str(out))
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
            "UPPER" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                Ok(Value::Str(s.to_uppercase()))
            }
            "LOWER" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                Ok(Value::Str(s.to_lowercase()))
            }
            "CONCATENATE" | "CONCAT" => {
                let mut out = String::new();
                for arg in args {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        out.push_str(&item.coerce_str()?);
                    }
                }
                Ok(Value::Str(out))
            }
            "TEXTJOIN" => {
                if args.len() < 3 {
                    return Err(CalcErr::Value);
                }
                let delim = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let ignore_empty = self.eval_scalar(caller, &args[1]).coerce_bool()?;
                let mut pieces = Vec::new();
                for arg in &args[2..] {
                    let v = self.eval_expr(caller, arg);
                    for item in self.flatten_value(v) {
                        let s = item.coerce_str()?;
                        if ignore_empty && s.is_empty() {
                            continue;
                        }
                        pieces.push(s);
                    }
                }
                Ok(Value::Str(pieces.join(&delim)))
            }
            "EXACT" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let s1 = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let s2 = self.eval_scalar(caller, &args[1]).coerce_str()?;
                Ok(Value::Bool(s1 == s2))
            }
            "VALUE" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                if let Value::Num(n) = v {
                    return Ok(Value::Num(n));
                }
                let s = v.coerce_str()?;
                let t = s.trim();
                let (body, pct) = match t.strip_suffix('%') {
                    Some(b) => (b.trim_end(), true),
                    None => (t, false),
                };
                let body = body.replace(',', "");
                if let Ok(n) = body.parse::<f64>() {
                    if n.is_finite() {
                        return Ok(Value::Num(if pct { n / 100.0 } else { n }));
                    }
                }
                // 날짜 글자("2026-11-12")도 엑셀은 숫자로 바꾼다
                parse_date_text(t).map(|d| Value::Num(d as f64)).ok_or(CalcErr::Value)
            }
            "FIXED" => {
                if args.is_empty() || args.len() > 3 {
                    return Err(CalcErr::Value);
                }
                let n = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let decimals = if args.len() >= 2 {
                    self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32
                } else {
                    2
                };
                // 엑셀 한도: 소수 자릿수 127. 넘으면 #VALUE!(서식 문자열이 기가바이트로 커지는 것도 막음)
                if decimals > 127 {
                    return Err(CalcErr::Value);
                }
                let n = excel_round_away_from_zero(n, decimals.max(-127))?;
                let no_commas = if args.len() == 3 {
                    self.eval_scalar(caller, &args[2]).coerce_bool()?
                } else {
                    false
                };
                let pat = if no_commas {
                    if decimals > 0 {
                        format!("0.{}", "0".repeat(decimals as usize))
                    } else {
                        "0".into()
                    }
                } else if decimals > 0 {
                    format!("#,##0.{}", "0".repeat(decimals as usize))
                } else {
                    "#,##0".into()
                };
                let comp = FormatCompiler::compile(&pat);
                Ok(Value::Str(FormatEngine::format_value(
                    &FormatValue::Number(n),
                    &comp,
                )))
            }
            "CHAR" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let code = self.eval_scalar(caller, &args[0]).coerce_num()?.trunc() as u32;
                if !(1..=255).contains(&code) {
                    return Err(CalcErr::Value);
                }
                let ch = char::from_u32(code).ok_or(CalcErr::Value)?;
                Ok(Value::Str(ch.to_string()))
            }
            "CODE" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                let first = s.chars().next().ok_or(CalcErr::Value)?;
                Ok(Value::Num((first as u32) as f64))
            }
            "T" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                match v {
                    Value::Str(s) => Ok(Value::Str(s)),
                    _ => Ok(Value::Str(String::new())),
                }
            }
            "N" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let v = self.eval_scalar(caller, &args[0]);
                match v {
                    Value::Num(n) => Ok(Value::Num(n)),
                    Value::Bool(b) => Ok(Value::Num(if b { 1.0 } else { 0.0 })),
                    Value::Err(e) => Err(e),
                    _ => Ok(Value::Num(0.0)),
                }
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
            // [6] 날짜·시간 (13개)
            // ----------------------------------------------------------------
            "DATE" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let y = self.eval_scalar(caller, &args[0]).coerce_num()?.trunc() as i32;
                let m = self.eval_scalar(caller, &args[1]).coerce_num()?.trunc() as i32;
                let d = self.eval_scalar(caller, &args[2]).coerce_num()?.trunc() as i64;
                let year = if (0..=1899).contains(&y) { 1900 + y } else { y };
                let (ny, nm) = add_months(year, m, 0);
                let base_serial = ymd_to_serial_1900(ny, nm as u32, 1);
                Ok(Value::Num((base_serial + d - 1) as f64))
            }
            "TIME" => {
                if args.len() != 3 {
                    return Err(CalcErr::Value);
                }
                let h = self.eval_scalar(caller, &args[0]).coerce_num()?;
                let m = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let s = self.eval_scalar(caller, &args[2]).coerce_num()?;
                let total_secs = h * 3600.0 + m * 60.0 + s;
                let fract = (total_secs / 86400.0).rem_euclid(1.0);
                Ok(Value::Num(fract))
            }
            "YEAR" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let (y, _, _) = serial_to_ymd_1900(serial);
                Ok(Value::Num(y as f64))
            }
            "MONTH" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let (_, m, _) = serial_to_ymd_1900(serial);
                Ok(Value::Num(m as f64))
            }
            "DAY" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let (_, _, d) = serial_to_ymd_1900(serial);
                Ok(Value::Num(d as f64))
            }
            "HOUR" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let f = self.eval_scalar(caller, &args[0]).coerce_num()?.rem_euclid(1.0);
                let secs = (f * 86400.0).round() as u64;
                Ok(Value::Num(((secs / 3600) % 24) as f64))
            }
            "MINUTE" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let f = self.eval_scalar(caller, &args[0]).coerce_num()?.rem_euclid(1.0);
                let secs = (f * 86400.0).round() as u64;
                Ok(Value::Num(((secs % 3600) / 60) as f64))
            }
            "SECOND" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let f = self.eval_scalar(caller, &args[0]).coerce_num()?.rem_euclid(1.0);
                let secs = (f * 86400.0).round() as u64;
                Ok(Value::Num((secs % 60) as f64))
            }
            "TODAY" => Ok(Value::Num(now_serial().floor())),
            "NOW" => Ok(Value::Num(now_serial())),
            "DAYS" => {
                if args.len() != 2 {
                    return Err(CalcErr::Value);
                }
                let end = self.eval_scalar(caller, &args[0]).coerce_num()?.floor();
                let start = self.eval_scalar(caller, &args[1]).coerce_num()?.floor();
                Ok(Value::Num(end - start))
            }
            "DATEVALUE" => {
                if args.len() != 1 {
                    return Err(CalcErr::Value);
                }
                let s = self.eval_scalar(caller, &args[0]).coerce_str()?;
                parse_date_text(s.trim()).map(|d| Value::Num(d as f64)).ok_or(CalcErr::Value)
            }
            "WEEKNUM" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(CalcErr::Value);
                }
                let serial = self.eval_scalar(caller, &args[0]).coerce_num()?.floor() as i64;
                let (y, _, _) = serial_to_ymd_1900(serial);
                let jan1_serial = ymd_to_serial_1900(y, 1, 1);
                let days = serial - jan1_serial;
                let jan1_sun0 = (jan1_serial - 1).rem_euclid(7);
                Ok(Value::Num(((days + jan1_sun0) / 7 + 1) as f64))
            }
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
            "NETWORKDAYS" | "NETWORKDAYS.INTL" | "WORKDAY" | "WORKDAY.INTL" => {
                let intl = clean_name.ends_with(".INTL");
                let max_args = if intl { 4 } else { 3 };
                if args.len() < 2 || args.len() > max_args {
                    return Err(CalcErr::Value);
                }
                let a = check_serial(self.eval_scalar(caller, &args[0]).coerce_num()?)?;
                let b_raw = self.eval_scalar(caller, &args[1]).coerce_num()?;
                let weekend = if intl && args.len() >= 3 && !matches!(args[2], Expr::EmptyArg) {
                    let v = self.eval_scalar(caller, &args[2]);
                    if let Value::Err(e) = v {
                        return Err(e);
                    }
                    parse_weekend(Some(&v))?
                } else {
                    parse_weekend(None)?
                };
                let hol_idx = if intl { 3 } else { 2 };
                let mut holidays = HashSet::new();
                if args.len() > hol_idx && !matches!(args[hol_idx], Expr::EmptyArg) {
                    for h in self.collect_numbers_from_args(caller, &args[hol_idx..=hol_idx])? {
                        holidays.insert(check_serial(h)?);
                    }
                }
                if clean_name.starts_with("NETWORKDAYS") {
                    let b = check_serial(b_raw)?;
                    return Ok(Value::Num(calc_networkdays_intl(a, b, &weekend, &holidays) as f64));
                }
                // WORKDAY: 시작일 다음 날부터 근무일을 days 개 센다.
                if !b_raw.is_finite() || b_raw.abs() > MAX_SERIAL as f64 {
                    return Err(CalcErr::Num);
                }
                let days = b_raw.trunc() as i64;
                let step = if days >= 0 { 1 } else { -1 };
                let mut cur = a;
                let mut left = days.abs();
                while left > 0 {
                    cur += step;
                    if cur < 0 || cur > MAX_SERIAL {
                        return Err(CalcErr::Num);
                    }
                    if !weekend[weekday_mon0(cur)] && !holidays.contains(&cur) {
                        left -= 1;
                    }
                }
                Ok(Value::Num(cur as f64))
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
                let sun0 = ((serial - 1).rem_euclid(7)) as i32;
                let ans = match ret_type {
                    1 | 17 => sun0 + 1,
                    2 | 11 => ((sun0 + 6) % 7) + 1,
                    3 => (sun0 + 6) % 7,
                    12 => ((sun0 + 5) % 7) + 1,
                    13 => ((sun0 + 4) % 7) + 1,
                    14 => ((sun0 + 3) % 7) + 1,
                    15 => ((sun0 + 2) % 7) + 1,
                    16 => (sun0 % 7) + 1,
                    _ => return Err(CalcErr::Num),
                };
                Ok(Value::Num(ans as f64))
            }

            // ----------------------------------------------------------------
            // [7] 재무 (10개)
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
            // [8] 통계 (11개)
            // ----------------------------------------------------------------
            "STDEV.S" | "STDEV" | "STDEVS" => {
                let nums = self.collect_numbers_from_args(caller, args)?;
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
            // [9] 기타 (1개)
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
// 5. 연산자·보조 수학 함수
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

pub fn excel_round_up(val: f64, digits: i32) -> Result<f64, CalcErr> {
    excel_round_with(val, digits, RoundingStrategy::AwayFromZero)
}

pub fn excel_round_down(val: f64, digits: i32) -> Result<f64, CalcErr> {
    excel_round_with(val, digits, RoundingStrategy::ToZero)
}

/// ROUNDUP/ROUNDDOWN 공통: 15자리 유효숫자로 정리한 뒤(0.1+0.2 → 0.3) 자릿수에서 자른다.
/// 여기서만 RoundingStrategy::AwayFromZero(무조건 올림)가 맞다 — ROUND 에는 쓰지 말 것.
fn excel_round_with(val: f64, digits: i32, strategy: RoundingStrategy) -> Result<f64, CalcErr> {
    if !val.is_finite() {
        return Err(CalcErr::Num);
    }
    if val == 0.0 {
        return Ok(0.0);
    }
    let d = crate::numfmt::round_sig15(Decimal::from_f64_retain(val).ok_or(CalcErr::Num)?);
    if digits >= 0 {
        d.round_dp_with_strategy(digits.min(28) as u32, strategy).to_f64().ok_or(CalcErr::Num)
    } else {
        let p = (-digits).min(28) as u32;
        let factor = Decimal::from_i128_with_scale(10i128.pow(p), 0);
        ((d / factor).round_dp_with_strategy(0, strategy) * factor).to_f64().ok_or(CalcErr::Num)
    }
}

/// n/sig 같은 몫을 15자리로 정리(4.2/0.1 = 41.99999… → 42)
fn clean_quotient(q: f64) -> f64 {
    Decimal::from_f64_retain(q)
        .map(crate::numfmt::round_sig15)
        .and_then(|d| d.to_f64())
        .unwrap_or(q)
}

fn clean_product(q: f64, sig: f64) -> f64 {
    clean_quotient(q * sig)
}

/// "2026-11-12" "2024/2/29" "2024.2.29" 꼴만 받는다(연-월-일). 범위 밖이면 None.
fn parse_date_text(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '/' || c == '.').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].trim().parse().ok()?;
    let m: u32 = parts[1].trim().parse().ok()?;
    let d: u32 = parts[2].trim().parse().ok()?;
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || d == 0 {
        return None;
    }
    if !(y == 1900 && m == 2 && d == 29) && d > days_in_month_gregorian(y, m) {
        return None;
    }
    Some(ymd_to_serial_1900(y, m, d))
}

/// 기기 시간대(초). Swift 쪽에서 TimeZone.current.secondsFromGMT() 로 넣어 준다.
static TZ_OFFSET_SECS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

pub fn set_timezone_offset_seconds(secs: i64) {
    TZ_OFFSET_SECS.store(secs.clamp(-14 * 3600, 14 * 3600), std::sync::atomic::Ordering::Relaxed);
}

/// 지금 시각의 엑셀 일련번호(1970-01-01 = 25569).
fn now_serial() -> f64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
        + TZ_OFFSET_SECS.load(std::sync::atomic::Ordering::Relaxed) as f64;
    25569.0 + secs / 86400.0
}

/// 0 이상 1 미만 난수(xorshift64*, 시각으로 씨앗). 보기용이라 암호 품질은 필요 없다.
fn next_random() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering as AO};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut x = STATE.load(AO::Relaxed);
    if x == 0 {
        x = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
    }
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    STATE.store(x, AO::Relaxed);
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
}

/// 주말 표: 월=0 … 일=6. true 면 쉬는 날.
fn parse_weekend(v: Option<&Value>) -> Result<[bool; 7], CalcErr> {
    let mut w = [false; 7];
    match v {
        None | Some(Value::Blank) => {
            w[5] = true;
            w[6] = true;
        }
        Some(Value::Str(s)) => {
            let b = s.as_bytes();
            if b.len() != 7 || !b.iter().all(|c| *c == b'0' || *c == b'1') {
                return Err(CalcErr::Value);
            }
            for i in 0..7 {
                w[i] = b[i] == b'1';
            }
            if w.iter().all(|x| *x) {
                return Err(CalcErr::Value);
            }
        }
        Some(other) => {
            let n = other.coerce_num()?;
            if n.fract() != 0.0 {
                return Err(CalcErr::Num);
            }
            match n as i64 {
                // 1: 토·일, 2: 일·월, … 7: 금·토
                k @ 1..=7 => {
                    let first = ((k + 4) % 7) as usize; // 1 → 토(5), 7 → 금(4)
                    w[first] = true;
                    w[(first + 1) % 7] = true;
                }
                // 11: 일만, 12: 월만, … 17: 토만
                k @ 11..=17 => w[((k - 11 + 6) % 7) as usize] = true,
                _ => return Err(CalcErr::Num),
            }
        }
    }
    Ok(w)
}

/// 일련번호의 요일(월=0 … 일=6). 엑셀은 일련번호 1(1900-01-01)을 일요일로 본다.
fn weekday_mon0(serial: i64) -> usize {
    ((serial - 1).rem_euclid(7) as usize + 6) % 7
}

const MAX_SERIAL: i64 = 2_958_465; // 9999-12-31

fn check_serial(v: f64) -> Result<i64, CalcErr> {
    if !v.is_finite() || v < 0.0 || v > MAX_SERIAL as f64 + 1.0 {
        return Err(CalcErr::Num);
    }
    Ok(v.floor() as i64)
}

/// NETWORKDAYS(.INTL): 주 단위로 셈하고 남은 날만 하나씩 본다(긴 구간에서도 즉시 끝남).
fn calc_networkdays_intl(start: i64, end: i64, weekend: &[bool; 7], holidays: &HashSet<i64>) -> i64 {
    let sign = if start <= end { 1 } else { -1 };
    let (a, b) = if start <= end { (start, end) } else { (end, start) };
    let per_week = weekend.iter().filter(|x| !**x).count() as i64;
    let total = b - a + 1;
    let mut count = (total / 7) * per_week;
    let rem_start = a + (total / 7) * 7;
    for s in rem_start..=b {
        if !weekend[weekday_mon0(s)] {
            count += 1;
        }
    }
    for h in holidays {
        if *h >= a && *h <= b && !weekend[weekday_mon0(*h)] {
            count -= 1;
        }
    }
    count * sign
}

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
            } else if same_variant_type(lookup, v)
                && excel_compare_values(lookup, v) == Ordering::Equal
            {
                return Some(i + 1);
            }
        }
        None
    } else if match_type > 0 {
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

fn calc_ipmt(
    rate: f64,
    per: f64,
    nper: f64,
    pv: f64,
    fv: f64,
    p_type: i32,
) -> Result<f64, CalcErr> {
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
// 6. golden_values 비교기
// ============================================================================

#[derive(Debug, Clone)]
pub struct SheetComparisonReport {
    pub sheet_name: String,
    pub matched: usize,
    pub total: usize,
    pub diffs: Vec<(String, String, String)>,
}

pub fn compare_with_golden(
    wb: &Workbook,
    calc_res: &WorkbookCalcResult,
    golden: &HashMap<String, HashMap<String, Value>>,
) -> (usize, usize, Vec<SheetComparisonReport>) {
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
// 7. P1b 단위 시험 (신규 확장된 함수 검증)
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
            links: HashMap::new(),
            cf_blocks: Vec::new(),
            tables: Vec::new(),
            raw_drawings: Vec::new(),
            tab_color: None,
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

    #[test]
    fn review_range_deps_only_formula_cells_both_paths() {
        // 범위 안 수식 칸이 먼저 계산돼야 한다 — 범위가 작을 때(칸을 훑음)와 클 때(수식 목록을 훑음) 둘 다.
        let addrs: Vec<String> = (1..=3000u32).map(|r| format!("B{}", r)).collect();
        let vref: Vec<(&str, Value)> = addrs.iter().map(|a| (a.as_str(), Value::Num(1.0))).collect();
        let wb = make_single_sheet_wb(
            &vref,
            &[
                // 큰 범위(3001칸 > 수식 5개) → 수식 목록 경로
                ("A1", "SUM(B:B)+SUM(C1:C3001)"),
                ("C3001", "C2+100"),
                ("C2", "10"),
                // 작은 범위(2칸 < 수식 5개) → 칸 경로
                ("D1", "SUM(E1:E2)"),
                ("E2", "C2*2"),
            ],
        );
        let res = evaluate_workbook(&wb);
        let get = |r: u32, c: u32| res.values.get(&CellAddr { sheet: 0, row: r, col: c }).cloned();
        assert_eq!(get(1, 1), Some(Value::Num(3000.0 + 10.0 + 110.0)));
        assert_eq!(get(1, 4), Some(Value::Num(20.0)));
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

    // ---- 제미나이 P1b 추가 시험 ----
    fn make_test_wb(cells: &[(&str, Value)], formulas: &[(&str, &str)]) -> Workbook {
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
            name: "P1b_Test".to_string(),
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
            links: HashMap::new(),
            cf_blocks: Vec::new(),
            tables: Vec::new(),
            raw_drawings: Vec::new(),
            tab_color: None,
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

    fn eval_formula(f: &str) -> Value {
        let wb = make_test_wb(&[], &[("A1", f)]);
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
    fn test_new_math_functions() {
        assert!(values_match_golden(&Value::Num(24.0), &eval_formula("PRODUCT(2, 3, 4)")));
        assert!(values_match_golden(&Value::Num(3.2), &eval_formula("ROUNDUP(3.14159, 1)")));
        assert!(values_match_golden(&Value::Num(3.1), &eval_formula("ROUNDDOWN(3.14159, 1)")));
        assert!(values_match_golden(&Value::Num(3.0), &eval_formula("TRUNC(3.9)")));
        assert!(values_match_golden(&Value::Num(-4.0), &eval_formula("INT(-3.2)")));
        assert!(values_match_golden(&Value::Num(1.0), &eval_formula("MOD(-3, 2)"))); // 엑셀 식 MOD(-3,2)=1
        assert!(values_match_golden(&Value::Num(1.0), &eval_formula("SIGN(42)")));
        assert!(values_match_golden(&Value::Num(8.0), &eval_formula("POWER(2, 3)")));
        assert!(values_match_golden(&Value::Num(5.0), &eval_formula("SQRT(25)")));
        assert!(values_match_golden(&Value::Num(5.0), &eval_formula("CEILING(4.2, 1)")));
        assert!(values_match_golden(&Value::Num(4.0), &eval_formula("FLOOR(4.8, 1)")));
        assert!(values_match_golden(&Value::Num(10.0), &eval_formula("MROUND(11, 5)")));
        assert!(values_match_golden(&Value::Num(1.0), &eval_formula("LN(EXP(1))")));
        assert!(values_match_golden(&Value::Num(3.0), &eval_formula("LOG(1000, 10)")));
        assert!(values_match_golden(&Value::Num(2.0), &eval_formula("LOG10(100)")));
    }

    #[test]
    fn test_new_logic_and_info() {
        assert_eq!(eval_formula("IFS(1=2, \"A\", 2=2, \"B\")"), Value::Str("B".into()));
        assert_eq!(eval_formula("SWITCH(2, 1, \"one\", 2, \"two\", \"def\")"), Value::Str("two".into()));
        assert_eq!(eval_formula("NOT(TRUE)"), Value::Bool(false));
        assert_eq!(eval_formula("XOR(TRUE, FALSE, TRUE)"), Value::Bool(false));
        assert_eq!(eval_formula("XOR(TRUE, FALSE, FALSE)"), Value::Bool(true));
        assert_eq!(eval_formula("IFNA(NA(), 123)"), Value::Num(123.0));
        assert_eq!(eval_formula("ISBLANK(B1)"), Value::Bool(true));
        assert_eq!(eval_formula("ISERR(1/0)"), Value::Bool(true));
        assert_eq!(eval_formula("ISERR(NA())"), Value::Bool(false));
        assert_eq!(eval_formula("ISERROR(NA())"), Value::Bool(true));
        assert_eq!(eval_formula("ERROR.TYPE(1/0)"), Value::Num(2.0));
    }

    #[test]
    fn test_new_lookup_and_reference() {
        let wb = make_test_wb(
            &[
                ("A1", Value::Num(10.0)), ("B1", Value::Num(20.0)), ("C1", Value::Num(30.0)),
                ("A2", Value::Num(100.0)), ("B2", Value::Num(200.0)), ("C2", Value::Num(300.0)),
            ],
            &[
                ("D1", "HLOOKUP(20, A1:C2, 2, FALSE)"),
                ("D2", "LOOKUP(25, A1:C1, A2:C2)"),
                ("D3", "ROWS(A1:C2)"),
                ("D4", "COLUMNS(A1:C2)"),
                ("D5", "ROW(B2)"),
                ("D6", "COLUMN(B2)"),
            ],
        );
        let res = evaluate_workbook(&wb);
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 1, col: 4 }], Value::Num(200.0));
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 2, col: 4 }], Value::Num(200.0)); // LOOKUP 근사
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 3, col: 4 }], Value::Num(2.0));
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 4, col: 4 }], Value::Num(3.0));
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 5, col: 4 }], Value::Num(2.0));
        assert_eq!(res.values[&CellAddr { sheet: 0, row: 6, col: 4 }], Value::Num(2.0));
    }

    #[test]
    fn test_new_text_functions() {
        assert_eq!(eval_formula("CONCATENATE(\"A\", \"B\", \"C\")"), Value::Str("ABC".into()));
        assert_eq!(eval_formula("CONCAT(\"A\", \"B\", \"C\")"), Value::Str("ABC".into()));
        assert_eq!(eval_formula("TEXTJOIN(\"-\", TRUE, \"A\", \"\", \"B\")"), Value::Str("A-B".into()));
        assert_eq!(eval_formula("UPPER(\"hello\")"), Value::Str("HELLO".into()));
        assert_eq!(eval_formula("LOWER(\"WORLD\")"), Value::Str("world".into()));
        assert_eq!(eval_formula("REPLACE(\"abcdef\", 3, 2, \"XX\")"), Value::Str("abXXef".into()));
        assert_eq!(eval_formula("SEARCH(\"B*D\", \"aBcDe\")"), Value::Num(2.0));
        assert_eq!(eval_formula("VALUE(\"1,234.5\")"), Value::Num(1234.5));
        assert_eq!(eval_formula("EXACT(\"case\", \"CASE\")"), Value::Bool(false));
        assert_eq!(eval_formula("CHAR(65)"), Value::Str("A".into()));
        assert_eq!(eval_formula("CODE(\"A\")"), Value::Num(65.0));
    }

    #[test]
    fn test_new_date_and_time_functions() {
        assert_eq!(eval_formula("DATE(2026, 11, 12)"), Value::Num(46338.0));
        assert_eq!(eval_formula("YEAR(46338)"), Value::Num(2026.0));
        assert_eq!(eval_formula("MONTH(46338)"), Value::Num(11.0));
        assert_eq!(eval_formula("DAY(46338)"), Value::Num(12.0));
        assert!(values_match_golden(&Value::Num(0.5), &eval_formula("TIME(12, 0, 0)")));
        assert_eq!(eval_formula("DAYS(46340, 46338)"), Value::Num(2.0));
        assert_eq!(eval_formula("DATEVALUE(\"2026-11-12\")"), Value::Num(46338.0));
        // WORKDAY 검증 (2026-11-06 금요일 + 1영업일 = 2026-11-09 월요일)
        // 2026-11-06 = 46332, 2026-11-09 = 46335
        assert_eq!(eval_formula("WORKDAY(46332, 1)"), Value::Num(46335.0));
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
        // P1b 검수
        // ROWS(A:A) 는 엑셀이면 1048576 이지만, 열 전체 참조를 쓰는 영역까지로 자르므로 사용 행 수가 나온다(알려진 차이).
        assert_eq!(n("ROUNDUP(0.1+0.2,1)"), Value::Num(0.3));
        assert_eq!(n("ROUNDDOWN(1.005*1000,0)"), Value::Num(1005.0));
        assert_eq!(n("CEILING(4.3,0.1)"), Value::Num(4.3));
        assert_eq!(n("CEILING.MATH(-4.2)"), Value::Num(-4.0));
        assert_eq!(n("FLOOR.MATH(-4.8)"), Value::Num(-5.0));
        assert_eq!(n("FIXED(1234.567,-1)"), Value::Str("1,230".into()));
        assert_eq!(n("VALUE(\"50%\")"), Value::Num(0.5));
        assert_eq!(n("WORKDAY.INTL(46332,1,7)"), Value::Num(46334.0));
        assert_eq!(n("NETWORKDAYS.INTL(46332,46345,11)"), Value::Num(12.0));
        assert_eq!(n("NETWORKDAYS(46332,46345,{46335,46336})"), Value::Num(8.0));
        assert_eq!(n("DATEVALUE(\"2025-2-29\")"), Value::Err(CalcErr::Value));
        assert!(matches!(n("TODAY()"), Value::Num(d) if d > 46000.0 && d.fract() == 0.0));
        assert!(matches!(n("RAND()"), Value::Num(r) if (0.0..1.0).contains(&r)));
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
            // P1b: 긴 반복·큰 수로 멈추거나 죽지 않을 것
            "NETWORKDAYS(1,1E15)", "NETWORKDAYS(1,2958465)", "WORKDAY(1,1E15)", "WORKDAY(1,2900000)",
            "WORKDAY.INTL(1,100,\"1111110\")", "NETWORKDAYS.INTL(1,10,\"abc\")", "WORKDAY(-1,1)",
            "DATEVALUE(\"99999-1-1\")", "DATEVALUE(\"--\")", "VALUE(\"%\")", "VALUE(\"1e999\")",
            "FIXED(1E300,-300)", "FIXED(1,1E9)", "ROUNDUP(1E308,2)", "ROUNDDOWN(-1E-300,300)",
            "CEILING(1,1E-300)", "CEILING.MATH(1E300,1E-300)", "MROUND(1E300,1E-300)", "CHAR(0)", "CHAR(1E20)",
            "CODE(\"\")", "REPLACE(\"a\",1E10,1,\"b\")", "SEARCH(\"\",\"\")", "TEXTJOIN(\",\",TRUE,A:A)",
            "RANDBETWEEN(1E308,-1E308)", "RANDBETWEEN(-1E308,1E308)", "SWITCH()", "IFS()", "LOOKUP(1,{})",
            "HLOOKUP(1,A1:B2,99)", "TIME(1E20,0,0)", "SUM(A:XFD)", "SUMPRODUCT(A:XFD,A:XFD)",
            "COUNTIF(1:1048576,1)", "INDEX(A:XFD,1,1)", "MATCH(1,1:1,0)", "ROWS(A:A)", "SUM(ZZZZZZZZ:ZZZZZZZZ)", "SUM(0:0)", "SUM(9999999:9999999)", "WEEKNUM(1,99)", "ERROR.TYPE(1)", "LOG(1,1)", "LOG(-1)",
        ];
        let t0 = std::time::Instant::now();
        for f in junk {
            if std::env::var("JUNK_TRACE").is_ok() { eprintln!("JUNK {}", f); }
            let _ = ev1(f, &[]);
        }
        assert!(t0.elapsed().as_secs() < 20, "쓰레기 입력 계산이 너무 오래 걸림: {:?}", t0.elapsed());
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

    #[test]
    fn review_criteria_index_is_superset_of_matcher() {
        // 섞인 값 3,000칸 × 조건 40종: 색인 후보가 matcher 일치를 빠짐없이 품어야 한다.
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
        let words = ["a", "B", "abc", "ABC", "", " 5", "5", "05", "1e1", "TRUE", "가", "-3", "x*y"];
        let mut vals = Vec::new();
        for _ in 0..3000 {
            let k = rnd() % 6;
            vals.push(match k {
                0 => Value::Num(((rnd() % 41) as f64 - 20.0) / 2.0),
                1 => Value::Num((rnd() % 7) as f64 + 0.1 + 0.2),
                2 => Value::Str(words[(rnd() % words.len() as u64) as usize].to_string()),
                3 => Value::Blank,
                4 => Value::Bool(rnd() % 2 == 0),
                _ => Value::Err(CalcErr::Na),
            });
        }
        let idx = ColCriteriaIndex::build(&vals);
        let crits: Vec<Value> = vec![
            Value::Num(5.0), Value::Num(-3.0), Value::Num(0.30000000000000004), Value::Num(10.0),
            Value::Str("5".into()), Value::Str("=5".into()), Value::Str(">5".into()), Value::Str(">=5".into()),
            Value::Str("<-3".into()), Value::Str("<=0".into()), Value::Str(">=0.3".into()), Value::Str("<0.3".into()),
            Value::Str("abc".into()), Value::Str("=ABC".into()), Value::Str("a".into()), Value::Str("가".into()),
            Value::Str("TRUE".into()), Value::Str("1e1".into()), Value::Str("10".into()), Value::Str("05".into()),
            Value::Str(" 5".into()), Value::Str(">abc".into()), Value::Str("<b".into()), Value::Str("x*y".into()),
            Value::Str("=".into()), Value::Str("<>".into()), Value::Str("<>5".into()), Value::Str("a*".into()),
            Value::Bool(true), Value::Blank, Value::Err(CalcErr::Na), Value::Str("#N/A".into()),
            Value::Str(">-1E+300".into()), Value::Str("<1e999".into()), Value::Str("".into()), Value::Num(f64::NAN),
            Value::Str(">=".into()), Value::Str(">x".into()), Value::Num(1e10), Value::Str("=0".into()),
        ];
        for crit in &crits {
            let m = CriteriaMatcher::parse(crit);
            let truth: Vec<u32> = (0..vals.len()).filter(|&i| m.matches(&vals[i])).map(|i| i as u32).collect();
            if let Some(c) = idx.candidates(crit) {
                let set: std::collections::HashSet<u32> = c.into_iter().collect();
                for i in &truth {
                    assert!(set.contains(i), "조건 {:?} 에서 {} 번째 값 {:?} 이 후보에서 빠짐", crit, i, vals[*i as usize]);
                }
            }
        }
    }
}
