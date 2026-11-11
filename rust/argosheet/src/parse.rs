use crate::package::{is_rel_type, read_part_relationships, resolve_part_target};
use crate::style::{
    default_column_width_chars, excel_column_width_to_points_with_mdw, parse_styles_xml,
    read_theme_colors, StyleSheet,
};
use crate::{
    Cell, CellKind, CellPayload, CellRange, CommentEntry, DataValidationEntry, DefinedName,
    FormulaEntry, FormulaKind, FreezePane, Rows, Sheet, SheetState, StrSpan, StringPool, Workbook,
    CELL_FLAG_FORMULA_ARRAY, CELL_FLAG_FORMULA_SHARED, CELL_FLAG_HAS_COMMENT,
    CELL_FLAG_HAS_FORMULA,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use zip::ZipArchive;

/// OOXML `_xHHHH_` 이스케이프 시퀀스를 해제한다.
pub fn unescape_ooxml_x(input: &str) -> String {
    if !input.contains("_x") && !input.contains("_X") {
        return input.to_string();
    }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 6 < bytes.len()
            && bytes[i] == b'_'
            && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
            && bytes[i + 6] == b'_'
        {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 2..i + 6]) {
                if let Ok(code) = u32::from_str_radix(hex_str, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                        i += 7;
                        continue;
                    }
                }
            }
        }
        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 1-based 셀 주소("A1" -> (1, 1), "$E$5" -> (5, 5))를 파싱한다.
pub fn parse_cell_ref_1based(s: &str) -> Option<(u32, u32)> {
    let mut col: u32 = 0;
    let mut row: u32 = 0;
    let mut has_col = false;
    let mut has_row = false;

    for b in s.trim().bytes() {
        match b {
            b'$' => continue,
            b'A'..=b'Z' if !has_row => {
                col = col.checked_mul(26)?.checked_add((b - b'A' + 1) as u32)?;
                has_col = true;
            }
            b'a'..=b'z' if !has_row => {
                col = col.checked_mul(26)?.checked_add((b - b'a' + 1) as u32)?;
                has_col = true;
            }
            b'0'..=b'9' => {
                row = row.checked_mul(10)?.checked_add((b - b'0') as u32)?;
                has_row = true;
            }
            _ => return None,
        }
    }

    if !has_col || !has_row || row == 0 || col == 0 || col > 16384 || row > 1_048_576 {
        return None;
    }
    Some((row, col))
}

/// 1-based 열 번호를 엑셀 열 문자("A", "Z", "AA")로 변환한다.
pub fn col_to_letters(mut col_1based: u32) -> String {
    if col_1based == 0 {
        return "A".to_string();
    }
    let mut chars = Vec::with_capacity(4);
    while col_1based > 0 {
        let rem = ((col_1based - 1) % 26) as u8;
        chars.push((b'A' + rem) as char);
        col_1based = (col_1based - 1) / 26;
    }
    chars.iter().rev().collect()
}

/// 1-based (row, col)을 "A1" 형식 문자열로 변환한다.
pub fn format_cell_address(row_1based: u32, col_1based: u32) -> String {
    format!("{}{}", col_to_letters(col_1based), row_1based)
}

/// 범위 문자열("A1:D2" 또는 "E5")을 1-based CellRange로 파싱한다.
pub fn parse_range_1based(ref_str: &str) -> Option<CellRange> {
    if let Some((left, right)) = ref_str.split_once(':') {
        let (r1, c1) = parse_cell_ref_1based(left)?;
        let (r2, c2) = parse_cell_ref_1based(right)?;
        Some(CellRange {
            start_row: r1.min(r2),
            start_col: c1.min(c2),
            end_row: r1.max(r2),
            end_col: c1.max(c2),
        })
    } else {
        let (r, c) = parse_cell_ref_1based(ref_str)?;
        Some(CellRange {
            start_row: r,
            start_col: c,
            end_row: r,
            end_col: c,
        })
    }
}

/// 공유 수식(t="shared")의 마스터 수식에서 상대 참조를 (d_row, d_col)만큼 이동시킨다.
/// 왜: 정규식으로 치환하면 `LOG10(`, `"A1"` 문자열 리터럴, `'A1'!B2` 시트 이름이 망가지므로
/// 어휘 토큰 단위로 문자열 리터럴·작은따옴표 시트명·함수 호출(`(` 뒤따름)을 구별해 이동시킨다.
pub fn shift_formula_references(master: &str, d_row: i32, d_col: i32) -> String {
    if d_row == 0 && d_col == 0 {
        return master.to_string();
    }
    let bytes = master.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 8);
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // 1. 큰따옴표 문자열 리터럴은 내부를 그대로 복사
        if b == b'"' {
            out.push('"');
            i += 1;
            while i < len {
                let ch = master[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
                if ch == '"' {
                    if i < len && bytes[i] == b'"' {
                        out.push('"');
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        // 2. 작은따옴표 시트 이름('01_시트'!)은 그대로 복사
        if b == b'\'' {
            out.push('\'');
            i += 1;
            while i < len {
                let ch = master[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
                if ch == '\'' {
                    if i < len && bytes[i] == b'\'' {
                        out.push('\'');
                        i += 1;
                    } else {
                        break;
                    }
                }
            }
            continue;
        }

        // 3. 식별자/셀 참조 후보 시작 ($ 또는 영문자/밑줄)
        if b == b'$' || b.is_ascii_alphabetic() || b == b'_' {
            // 직전 문자가 영숫자나 마침표면 이미 다른 토큰의 일부
            let start = i;
            while i < len {
                let c = bytes[i];
                if c == b'$' || c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
                    i += 1;
                } else {
                    break;
                }
            }
            let token = &master[start..i];
            // 뒤에 '(' 또는 '!' 가 바로 오면 함수 이름(LOG10 등)이거나 시트 이름이므로 치환 금지
            let next_byte = if i < len { Some(bytes[i]) } else { None };
            if next_byte == Some(b'(') || next_byte == Some(b'!') {
                out.push_str(token);
            } else if let Some(shifted) = try_shift_single_ref(token, d_row, d_col) {
                out.push_str(&shifted);
            } else {
                out.push_str(token);
            }
            continue;
        }

        let ch = master[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

fn try_shift_single_ref(token: &str, d_row: i32, d_col: i32) -> Option<String> {
    let bytes = token.as_bytes();
    let mut pos = 0;

    let col_abs = if pos < bytes.len() && bytes[pos] == b'$' {
        pos += 1;
        true
    } else {
        false
    };

    let col_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
        pos += 1;
    }
    let col_len = pos - col_start;
    if col_len == 0 || col_len > 3 {
        return None;
    }

    let row_abs = if pos < bytes.len() && bytes[pos] == b'$' {
        pos += 1;
        true
    } else {
        false
    };

    let row_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    let row_len = pos - row_start;
    if row_len == 0 || pos != bytes.len() {
        return None;
    }

    let mut col_idx: i32 = 0;
    for &c in &bytes[col_start..col_start + col_len] {
        let up = c.to_ascii_uppercase();
        col_idx = col_idx * 26 + (up - b'A' + 1) as i32;
    }
    if col_idx < 1 || col_idx > 16384 {
        return None;
    }

    let row_idx: i32 = token[row_start..].parse().ok()?;
    if row_idx < 1 || row_idx > 1_048_576 {
        return None;
    }

    let new_col = if col_abs { col_idx } else { col_idx + d_col };
    let new_row = if row_abs { row_idx } else { row_idx + d_row };

    if new_col < 1 || new_col > 16384 || new_row < 1 || new_row > 1_048_576 {
        return Some("#REF!".to_string());
    }

    let mut res = String::with_capacity(8);
    if col_abs {
        res.push('$');
    }
    res.push_str(&col_to_letters(new_col as u32));
    if row_abs {
        res.push('$');
    }
    res.push_str(&new_row.to_string());
    Some(res)
}

pub fn open_xlsx_workbook(path: &str) -> Result<Workbook, String> {
    let file = File::open(path).map_err(|e| format!("파일 열기 실패 ({}): {}", path, e))?;
    let mut archive =
        ZipArchive::new(BufReader::new(file)).map_err(|e| format!("손상된 ZIP 아카이브: {}", e))?;

    let mut pool = StringPool::default();

    // 1. 루트 .rels에서 workbook 경로 탐색
    let root_rels = read_part_relationships(&mut archive, "")?;
    let workbook_path = root_rels
        .values()
        .find(|r| is_rel_type(&r.rel_type, "/officeDocument"))
        .map(|r| resolve_part_target("", &r.target))
        .unwrap_or_else(|| "xl/workbook.xml".to_string());

    let wb_rels = read_part_relationships(&mut archive, &workbook_path)?;

    // 2. workbook.xml 파싱 (시트 목록, definedName, fullCalcOnLoad, date1904)
    let wb_meta = parse_workbook_xml(&mut archive, &workbook_path)?;

    // 3. 테마(theme1.xml) 및 스타일(styles.xml) 파싱
    let theme_path = wb_rels
        .values()
        .find(|r| is_rel_type(&r.rel_type, "/theme"))
        .map(|r| resolve_part_target(&workbook_path, &r.target))
        .unwrap_or_else(|| "xl/theme/theme1.xml".to_string());
    let theme_colors = read_theme_colors(&mut archive, &theme_path);

    let styles_path = wb_rels
        .values()
        .find(|r| is_rel_type(&r.rel_type, "/styles"))
        .map(|r| resolve_part_target(&workbook_path, &r.target))
        .unwrap_or_else(|| "xl/styles.xml".to_string());
    let styles = parse_styles_xml(&mut archive, &styles_path, theme_colors)?;

    // 4. sharedStrings.xml 파싱 (없어도 정상 진행)
    let sst_path = wb_rels
        .values()
        .find(|r| is_rel_type(&r.rel_type, "/sharedStrings"))
        .map(|r| resolve_part_target(&workbook_path, &r.target))
        .unwrap_or_else(|| "xl/sharedStrings.xml".to_string());

    let sst_spans = if archive.by_name(&sst_path).is_ok() {
        parse_shared_strings(&mut archive, &sst_path, &mut pool)?
    } else {
        Vec::new()
    };

    // 5. 각 워크시트 및 연결된 comments 파싱
    let mut sheets = Vec::with_capacity(wb_meta.sheets.len());
    for entry in wb_meta.sheets {
        let sheet_path = wb_rels
            .get(&entry.r_id)
            .map(|r| resolve_part_target(&workbook_path, &r.target))
            .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", entry.sheet_id));

        let sheet_rels = read_part_relationships(&mut archive, &sheet_path)?;
        let comments_path = sheet_rels
            .values()
            .find(|r| is_rel_type(&r.rel_type, "/comments"))
            .map(|r| resolve_part_target(&sheet_path, &r.target));

        let comments = if let Some(cp) = comments_path {
            if archive.by_name(&cp).is_ok() {
                parse_comments_xml(&mut archive, &cp, &mut pool)?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let sheet = parse_worksheet_xml(
            &mut archive,
            &sheet_path,
            entry.name,
            entry.sheet_id,
            entry.state,
            &sst_spans,
            &styles,
            comments,
            &mut pool,
        )?;
        sheets.push(sheet);
    }

    Ok(Workbook {
        path: path.to_string(),
        date1904: wb_meta.date1904,
        full_calc_on_load: wb_meta.full_calc_on_load,
        defined_names: wb_meta.defined_names,
        styles,
        pool,
        sheets,
    })
}

struct WorkbookMeta {
    sheets: Vec<WorkbookSheetEntry>,
    defined_names: Vec<DefinedName>,
    date1904: bool,
    full_calc_on_load: bool,
}

struct WorkbookSheetEntry {
    name: String,
    sheet_id: u32,
    r_id: String,
    state: SheetState,
}

fn parse_workbook_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    workbook_path: &str,
) -> Result<WorkbookMeta, String> {
    let file = archive
        .by_name(workbook_path)
        .map_err(|e| format!("{} 없음: {}", workbook_path, e))?;

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(1024);
    let mut sheets = Vec::new();
    let mut defined_names = Vec::new();
    let mut date1904 = false;
    let mut full_calc_on_load = false;

    let mut in_defined_name = false;
    let mut cur_dn_name = String::new();
    let mut cur_dn_sheet_id: Option<u32> = None;
    let mut cur_dn_formula = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"workbookPr" => {
                    extract_workbook_pr(&e, &mut date1904);
                }
                b"calcPr" => {
                    extract_calc_pr(&e, &mut full_calc_on_load);
                }
                b"sheet" => {
                    sheets.push(extract_sheet_entry(&e));
                }
                b"definedName" => {
                    in_defined_name = true;
                    cur_dn_name.clear();
                    cur_dn_sheet_id = None;
                    cur_dn_formula.clear();
                    for attr in e.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"name" => {
                                cur_dn_name =
                                    attr.unescape_value().unwrap_or_default().into_owned()
                            }
                            b"localSheetId" => {
                                cur_dn_sheet_id =
                                    attr.unescape_value().unwrap_or_default().parse().ok()
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"workbookPr" => extract_workbook_pr(&e, &mut date1904),
                b"calcPr" => extract_calc_pr(&e, &mut full_calc_on_load),
                b"sheet" => sheets.push(extract_sheet_entry(&e)),
                _ => {}
            },
            Ok(Event::Text(e)) if in_defined_name => {
                let t = e.unescape().map_err(|err| err.to_string())?;
                cur_dn_formula.push_str(&t);
            }
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"definedName" {
                    in_defined_name = false;
                    if !cur_dn_name.is_empty() {
                        defined_names.push(DefinedName {
                            name: cur_dn_name.clone(),
                            local_sheet_id: cur_dn_sheet_id,
                            formula: cur_dn_formula.trim().to_string(),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("workbook.xml 파싱 오류: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(WorkbookMeta {
        sheets,
        defined_names,
        date1904,
        full_calc_on_load,
    })
}

fn extract_workbook_pr(e: &quick_xml::events::BytesStart, date1904: &mut bool) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"date1904" {
            let v = attr.unescape_value().unwrap_or_default();
            *date1904 = v == "1" || v.eq_ignore_ascii_case("true");
        }
    }
}

fn extract_calc_pr(e: &quick_xml::events::BytesStart, full_calc_on_load: &mut bool) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"fullCalcOnLoad" {
            let v = attr.unescape_value().unwrap_or_default();
            *full_calc_on_load = v == "1" || v.eq_ignore_ascii_case("true");
        }
    }
}

fn extract_sheet_entry(e: &quick_xml::events::BytesStart) -> WorkbookSheetEntry {
    let mut name = String::new();
    let mut sheet_id = 1u32;
    let mut r_id = String::new();
    let mut state = SheetState::Visible;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"name" => {
                let raw = attr.unescape_value().unwrap_or_default();
                name = unescape_ooxml_x(&raw);
            }
            b"sheetId" => {
                sheet_id = attr.unescape_value().unwrap_or_default().parse().unwrap_or(1);
            }
            b"id" => {
                r_id = attr.unescape_value().unwrap_or_default().into_owned();
            }
            b"state" => {
                let v = attr.unescape_value().unwrap_or_default();
                state = match v.as_ref() {
                    "hidden" => SheetState::Hidden,
                    "veryHidden" => SheetState::VeryHidden,
                    _ => SheetState::Visible,
                };
            }
            _ => {}
        }
    }

    WorkbookSheetEntry {
        name,
        sheet_id,
        r_id,
        state,
    }
}

/// `xl/sharedStrings.xml`을 스트리밍으로 읽어 StringPool에 적재한다.
/// 왜: `trim_text(false)`를 유지해야 `xml:space="preserve"`가 붙은 원문 앞뒤 공백이 손실되지 않는다.
fn parse_shared_strings<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sst_path: &str,
    pool: &mut StringPool,
) -> Result<Vec<StrSpan>, String> {
    let file = archive
        .by_name(sst_path)
        .map_err(|e| format!("{} 없음: {}", sst_path, e))?;

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(2048);
    let mut spans = Vec::new();
    let mut in_si = false;
    let mut in_t = false;
    let mut in_rph = false;
    let mut current = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"si" => {
                    in_si = true;
                    current.clear();
                }
                b"rPh" => in_rph = true,
                b"t" if in_si && !in_rph => in_t = true,
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if e.local_name().as_ref() == b"si" {
                    spans.push(pool.intern(""));
                }
            }
            Ok(Event::Text(e)) if in_si && in_t && !in_rph => {
                let raw = e.unescape().map_err(|err| err.to_string())?;
                current.push_str(&unescape_ooxml_x(&raw));
            }
            Ok(Event::CData(e)) if in_si && in_t && !in_rph => {
                if let Ok(s) = std::str::from_utf8(&e) {
                    current.push_str(&unescape_ooxml_x(s));
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"t" => in_t = false,
                b"rPh" => in_rph = false,
                b"si" => {
                    in_si = false;
                    spans.push(pool.intern(&current));
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("sharedStrings.xml 파싱 오류: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(spans)
}

/// `xl/commentsN.xml`을 파싱하여 시트 메모 목록을 반환한다.
fn parse_comments_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    comments_path: &str,
    pool: &mut StringPool,
) -> Result<Vec<CommentEntry>, String> {
    let file = archive
        .by_name(comments_path)
        .map_err(|e| format!("{} 없음: {}", comments_path, e))?;

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(false);

    let mut buf = Vec::with_capacity(1024);
    let mut authors: Vec<String> = Vec::new();
    let mut comments: Vec<CommentEntry> = Vec::new();

    let mut in_author = false;
    let mut cur_author = String::new();

    let mut in_comment = false;
    let mut in_t = false;
    let mut in_rph = false;
    let mut c_row = 1u32;
    let mut c_col = 1u32;
    let mut c_author_id: Option<usize> = None;
    let mut c_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"author" => {
                    in_author = true;
                    cur_author.clear();
                }
                b"comment" => {
                    in_comment = true;
                    c_text.clear();
                    c_author_id = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"ref" => {
                                let r_str = attr.unescape_value().unwrap_or_default();
                                if let Some((r, c)) = parse_cell_ref_1based(&r_str) {
                                    c_row = r;
                                    c_col = c;
                                }
                            }
                            b"authorId" => {
                                c_author_id =
                                    attr.unescape_value().unwrap_or_default().parse().ok();
                            }
                            _ => {}
                        }
                    }
                }
                b"rPh" if in_comment => in_rph = true,
                b"t" if in_comment && !in_rph => in_t = true,
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_author {
                    cur_author.push_str(&e.unescape().unwrap_or_default());
                } else if in_comment && in_t && !in_rph {
                    let raw = e.unescape().unwrap_or_default();
                    c_text.push_str(&unescape_ooxml_x(&raw));
                }
            }
            Ok(Event::CData(e)) if in_comment && in_t && !in_rph => {
                if let Ok(s) = std::str::from_utf8(&e) {
                    c_text.push_str(&unescape_ooxml_x(s));
                }
            }
            Ok(Event::End(e)) => match e.local_name().as_ref() {
                b"author" => {
                    in_author = false;
                    authors.push(cur_author.trim().to_string());
                }
                b"t" => in_t = false,
                b"rPh" => in_rph = false,
                b"comment" => {
                    in_comment = false;
                    let author_span = c_author_id
                        .and_then(|id| authors.get(id))
                        .map(|a| pool.intern(a));
                    let text_span = pool.intern(&c_text);
                    comments.push(CommentEntry {
                        row: c_row,
                        col: c_col,
                        author: author_span,
                        text: text_span,
                    });
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("comments XML 파싱 오류 ({}): {}", comments_path, e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(comments)
}

#[allow(clippy::too_many_arguments)]
fn parse_worksheet_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sheet_path: &str,
    name: String,
    sheet_id: u32,
    state: SheetState,
    sst_spans: &[StrSpan],
    styles: &StyleSheet,
    comments: Vec<CommentEntry>,
    pool: &mut StringPool,
) -> Result<Sheet, String> {
    let file = archive
        .by_name(sheet_path)
        .map_err(|e| format!("워크시트 없음 ({}): {}", sheet_path, e))?;

    let mut reader = Reader::from_reader(BufReader::new(file));
    // 왜: 03 시트 A열의 앞뒤 공백(xml:space="preserve")을 보존하기 위해 trim_text(false) 필수
    reader.config_mut().trim_text(false);

    let mdw_px = styles.mdw_px;
    let mut default_col_width_pt =
        excel_column_width_to_points_with_mdw(default_column_width_chars(8.0, mdw_px), mdw_px);
    let mut default_row_height_pt: f32 = 15.0;
    let mut show_grid_lines = true;
    let mut freeze: Option<FreezePane> = None;
    let mut auto_filter: Option<CellRange> = None;
    let mut merges: Vec<CellRange> = Vec::new();
    let mut validations: Vec<DataValidationEntry> = Vec::new();

    let mut custom_col_widths_pt: HashMap<u32, f32> = HashMap::new();
    let mut custom_row_heights_pt: HashMap<u32, f32> = HashMap::new();

    let mut data = Rows::default();
    let mut formulas: HashMap<u64, FormulaEntry> = HashMap::new();
    // 공유 수식 마스터: si -> (master_row_1based, master_col_1based, master_formula_string)
    let mut shared_masters: HashMap<u32, (u32, u32, String)> = HashMap::new();

    let mut max_row_1based: u32 = 0;
    let mut max_col_1based: u32 = 0;
    let mut total_cells: u32 = 0;

    let mut current_row: u32 = 1;
    let mut next_col: u32 = 1;
    let mut row_cells: Vec<Cell> = Vec::new();

    let mut in_cell = false;
    let mut cell_col: u32 = 1;
    let mut cell_style: u16 = 0;
    let mut cell_type = String::new();

    let mut in_v = false;
    let mut has_v_tag = false;
    let mut val_buf = String::new();

    let mut in_is = false;
    let mut in_is_t = false;
    let mut in_is_rph = false;
    let mut is_buf = String::new();

    let mut in_f = false;
    let mut has_f_tag = false;
    let mut f_type = String::new();
    let mut f_ref: Option<CellRange> = None;
    let mut f_si: Option<u32> = None;
    let mut f_buf = String::new();

    let mut in_dv = false;
    let mut cur_dv: Option<DataValidationEntry> = None;
    let mut in_dv_f1 = false;
    let mut in_dv_f2 = false;
    let mut dv_f_buf = String::new();

    let mut buf = Vec::with_capacity(4096);

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"sheetView" => parse_sheet_view_attrs(&e, &mut show_grid_lines),
                    b"pane" => parse_pane_attrs(&e, &mut freeze),
                    b"sheetFormatPr" => parse_sheet_format_pr(
                        &e,
                        mdw_px,
                        &mut default_col_width_pt,
                        &mut default_row_height_pt,
                    ),
                    b"col" => parse_col_attrs(
                        &e,
                        mdw_px,
                        default_col_width_pt,
                        &mut custom_col_widths_pt,
                    ),
                    b"row" => {
                        row_cells.clear();
                        let mut r_1based = current_row;
                        let mut ht: Option<f32> = None;
                        let mut hidden = false;

                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"r" => {
                                    if let Ok(v) =
                                        attr.unescape_value().unwrap_or_default().parse::<u32>()
                                    {
                                        if v >= 1 {
                                            r_1based = v;
                                        }
                                    }
                                }
                                b"ht" => {
                                    ht = attr.unescape_value().unwrap_or_default().parse().ok();
                                }
                                b"hidden" => {
                                    let v = attr.unescape_value().unwrap_or_default();
                                    hidden = v == "1" || v.eq_ignore_ascii_case("true");
                                }
                                _ => {}
                            }
                        }
                        current_row = r_1based;
                        next_col = 1;
                        if hidden {
                            custom_row_heights_pt.insert(current_row, 0.0);
                        } else if let Some(h) = ht {
                            custom_row_heights_pt.insert(current_row, h);
                        }
                    }
                    b"c" => {
                        in_cell = true;
                        cell_col = next_col;
                        cell_style = 0;
                        cell_type.clear();
                        has_v_tag = false;
                        val_buf.clear();
                        is_buf.clear();
                        has_f_tag = false;
                        f_type.clear();
                        f_ref = None;
                        f_si = None;
                        f_buf.clear();

                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"r" => {
                                    let r_str = attr.unescape_value().unwrap_or_default();
                                    if let Some((r, c)) = parse_cell_ref_1based(&r_str) {
                                        current_row = r;
                                        cell_col = c;
                                    }
                                }
                                b"s" => {
                                    cell_style = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0);
                                }
                                b"t" => {
                                    cell_type =
                                        attr.unescape_value().unwrap_or_default().into_owned();
                                }
                                _ => {}
                            }
                        }
                        next_col = cell_col + 1;
                    }
                    b"f" if in_cell => {
                        in_f = true;
                        has_f_tag = true;
                        extract_f_attrs(&e, &mut f_type, &mut f_ref, &mut f_si);
                    }
                    b"v" if in_cell => {
                        in_v = true;
                        has_v_tag = true;
                    }
                    b"is" if in_cell => {
                        in_is = true;
                    }
                    b"rPh" if in_cell && in_is => {
                        in_is_rph = true;
                    }
                    b"t" if in_cell && in_is && !in_is_rph => {
                        in_is_t = true;
                    }
                    b"mergeCell" => {
                        if let Some(range) = extract_ref_range(&e) {
                            merges.push(range);
                        }
                    }
                    b"autoFilter" => {
                        if auto_filter.is_none() {
                            auto_filter = extract_ref_range(&e);
                        }
                    }
                    b"dataValidation" => {
                        in_dv = true;
                        cur_dv = Some(extract_dv_attrs(&e));
                    }
                    b"formula1" if in_dv => {
                        in_dv_f1 = true;
                        dv_f_buf.clear();
                    }
                    b"formula2" if in_dv => {
                        in_dv_f2 = true;
                        dv_f_buf.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"sheetView" => parse_sheet_view_attrs(&e, &mut show_grid_lines),
                    b"pane" => parse_pane_attrs(&e, &mut freeze),
                    b"sheetFormatPr" => parse_sheet_format_pr(
                        &e,
                        mdw_px,
                        &mut default_col_width_pt,
                        &mut default_row_height_pt,
                    ),
                    b"col" => parse_col_attrs(
                        &e,
                        mdw_px,
                        default_col_width_pt,
                        &mut custom_col_widths_pt,
                    ),
                    b"f" if in_cell => {
                        // 왜: 공유 수식 자식 셀은 <f t="shared" si="0"/> 처럼 빈 태그로 온다.
                        has_f_tag = true;
                        extract_f_attrs(&e, &mut f_type, &mut f_ref, &mut f_si);
                    }
                    b"v" if in_cell => {
                        // 왜: openpyxl 등이 만든 수식 셀은 <v/> 처럼 값이 비어 있다. 파싱 실패로 처리하면 안 된다.
                        has_v_tag = true;
                    }
                    b"c" => {
                        // 자식 요소가 없는 빈 셀 (<c r="B2" s="3"/>)
                        let mut c_col = next_col;
                        let mut c_style = 0u16;
                        for attr in e.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"r" => {
                                    let r_str = attr.unescape_value().unwrap_or_default();
                                    if let Some((r, c)) = parse_cell_ref_1based(&r_str) {
                                        current_row = r;
                                        c_col = c;
                                    }
                                }
                                b"s" => {
                                    c_style = attr
                                        .unescape_value()
                                        .unwrap_or_default()
                                        .parse()
                                        .unwrap_or(0);
                                }
                                _ => {}
                            }
                        }
                        next_col = c_col + 1;
                        if c_style != 0 {
                            max_row_1based = max_row_1based.max(current_row);
                            max_col_1based = max_col_1based.max(c_col);
                            total_cells += 1;
                            row_cells.push(Cell {
                                col: c_col,
                                kind: CellKind::Blank as u8,
                                flags: 0,
                                style_idx: c_style,
                                payload: CellPayload { raw: 0 },
                            });
                        }
                    }
                    b"mergeCell" => {
                        if let Some(range) = extract_ref_range(&e) {
                            merges.push(range);
                        }
                    }
                    b"autoFilter" => {
                        if auto_filter.is_none() {
                            auto_filter = extract_ref_range(&e);
                        }
                    }
                    b"dataValidation" => {
                        validations.push(extract_dv_attrs(&e));
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                if in_cell {
                    if in_f {
                        let raw = e.unescape().map_err(|err| err.to_string())?;
                        f_buf.push_str(&raw);
                    } else if in_v {
                        let raw = e.unescape().map_err(|err| err.to_string())?;
                        val_buf.push_str(&raw);
                    } else if in_is && in_is_t && !in_is_rph {
                        let raw = e.unescape().map_err(|err| err.to_string())?;
                        is_buf.push_str(&unescape_ooxml_x(&raw));
                    }
                } else if in_dv && (in_dv_f1 || in_dv_f2) {
                    let raw = e.unescape().map_err(|err| err.to_string())?;
                    dv_f_buf.push_str(&raw);
                }
            }
            Ok(Event::CData(e)) => {
                if in_cell && in_is && in_is_t && !in_is_rph {
                    if let Ok(s) = std::str::from_utf8(&e) {
                        is_buf.push_str(&unescape_ooxml_x(s));
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"f" => in_f = false,
                    b"v" => in_v = false,
                    b"t" if in_is => in_is_t = false,
                    b"rPh" if in_is => in_is_rph = false,
                    b"is" => in_is = false,
                    b"c" => {
                        in_cell = false;
                        let mut flags = 0u8;

                        // 1. 수식 처리 (일반 / 공유 / 배열)
                        if has_f_tag {
                            flags |= CELL_FLAG_HAS_FORMULA;
                            let trimmed_f = f_buf.trim().strip_prefix('=').unwrap_or(f_buf.trim());

                            let (f_kind, resolved_f) = match f_type.as_str() {
                                "shared" => {
                                    flags |= CELL_FLAG_FORMULA_SHARED;
                                    if !trimmed_f.is_empty() {
                                        if let Some(si) = f_si {
                                            shared_masters.insert(
                                                si,
                                                (current_row, cell_col, trimmed_f.to_string()),
                                            );
                                        }
                                        (FormulaKind::Shared, trimmed_f.to_string())
                                    } else if let Some(si) = f_si {
                                        if let Some((m_row, m_col, m_str)) = shared_masters.get(&si)
                                        {
                                            let d_row = current_row as i32 - *m_row as i32;
                                            let d_col = cell_col as i32 - *m_col as i32;
                                            (
                                                FormulaKind::Shared,
                                                shift_formula_references(m_str, d_row, d_col),
                                            )
                                        } else {
                                            (FormulaKind::Shared, String::new())
                                        }
                                    } else {
                                        (FormulaKind::Shared, String::new())
                                    }
                                }
                                "array" => {
                                    flags |= CELL_FLAG_FORMULA_ARRAY;
                                    (FormulaKind::Array, trimmed_f.to_string())
                                }
                                _ => (FormulaKind::Normal, trimmed_f.to_string()),
                            };

                            let f_span = pool.intern(&resolved_f);
                            let key = crate::cell_key(current_row, cell_col);
                            formulas.insert(
                                key,
                                FormulaEntry {
                                    kind: f_kind,
                                    text: f_span,
                                    ref_range: f_ref.take(),
                                    shared_index: f_si,
                                },
                            );
                        }

                        // 2. 값 종류 및 8바이트 union 페이로드 결정
                        let (kind, payload) = match cell_type.as_str() {
                            "s" => {
                                let idx = val_buf.trim().parse::<usize>().unwrap_or(0);
                                let span = sst_spans
                                    .get(idx)
                                    .copied()
                                    .unwrap_or(StrSpan { offset: 0, len: 0 });
                                (CellKind::Str, CellPayload { str_span: span })
                            }
                            "inlineStr" => {
                                let span = pool.intern(&is_buf);
                                (CellKind::Str, CellPayload { str_span: span })
                            }
                            "str" => {
                                let unescaped = unescape_ooxml_x(&val_buf);
                                let span = pool.intern(&unescaped);
                                (CellKind::Str, CellPayload { str_span: span })
                            }
                            "b" => {
                                let v = val_buf.trim();
                                let b_val = if v == "1" || v.eq_ignore_ascii_case("true") {
                                    1u64
                                } else {
                                    0u64
                                };
                                (CellKind::Bool, CellPayload { boolean: b_val })
                            }
                            "e" => {
                                let span = pool.intern(val_buf.trim());
                                (CellKind::Err, CellPayload { str_span: span })
                            }
                            _ => {
                                // t="n" 또는 t 생략: <v>가 비어 있으면(openpyxl 수식 셀 등) Blank로 둔다.
                                let v_trimmed = val_buf.trim();
                                if !has_v_tag || v_trimmed.is_empty() {
                                    (CellKind::Blank, CellPayload { raw: 0 })
                                } else if let Ok(num) = v_trimmed.parse::<f64>() {
                                    (CellKind::Num, CellPayload { num })
                                } else {
                                    let span = pool.intern(&unescape_ooxml_x(&val_buf));
                                    (CellKind::Str, CellPayload { str_span: span })
                                }
                            }
                        };

                        if kind != CellKind::Blank || flags != 0 || cell_style != 0 {
                            max_row_1based = max_row_1based.max(current_row);
                            max_col_1based = max_col_1based.max(cell_col);
                            total_cells += 1;
                            row_cells.push(Cell {
                                col: cell_col,
                                kind: kind as u8,
                                flags,
                                style_idx: cell_style,
                                payload,
                            });
                        }
                    }
                    b"row" => {
                        if !row_cells.is_empty() {
                            row_cells.sort_by_key(|c| c.col);
                            data.push_row(current_row, &row_cells);
                        }
                        current_row += 1;
                    }
                    b"formula1" if in_dv => {
                        in_dv_f1 = false;
                        if let Some(dv) = cur_dv.as_mut() {
                            // 왜: 02 시트 유효성 목록 수식 앞에 '='가 붙어 있는 비표준 케이스를 벗겨낸다.
                            let clean = dv_f_buf.trim().strip_prefix('=').unwrap_or(dv_f_buf.trim());
                            dv.formula1 = Some(clean.to_string());
                        }
                    }
                    b"formula2" if in_dv => {
                        in_dv_f2 = false;
                        if let Some(dv) = cur_dv.as_mut() {
                            let clean = dv_f_buf.trim().strip_prefix('=').unwrap_or(dv_f_buf.trim());
                            dv.formula2 = Some(clean.to_string());
                        }
                    }
                    b"dataValidation" => {
                        in_dv = false;
                        if let Some(dv) = cur_dv.take() {
                            validations.push(dv);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("워크시트 XML 파싱 오류 ({}): {}", sheet_path, e)),
            _ => {}
        }
        buf.clear();
    }

    // 메모가 달린 셀에 CELL_FLAG_HAS_COMMENT 표시
    for cm in &comments {
        if let Some(cell) = data.get_cell_mut(cm.row, cm.col) {
            cell.flags |= CELL_FLAG_HAS_COMMENT;
        }
    }

    // 1..=max_col_1based 및 1..=max_row_1based 열 너비·행 높이(pt) 배열 생성
    let mut col_widths_pt = Vec::with_capacity(max_col_1based as usize);
    for c in 1..=max_col_1based {
        let w = custom_col_widths_pt
            .get(&c)
            .copied()
            .unwrap_or(default_col_width_pt);
        col_widths_pt.push(w);
    }

    let mut row_heights_pt = Vec::with_capacity(max_row_1based as usize);
    for r in 1..=max_row_1based {
        let h = custom_row_heights_pt
            .get(&r)
            .copied()
            .unwrap_or(default_row_height_pt);
        row_heights_pt.push(h);
    }

    Ok(Sheet {
        name,
        sheet_id,
        state,
        max_row: max_row_1based,
        max_col: max_col_1based,
        cell_count: total_cells,
        col_widths_pt,
        row_heights_pt,
        default_col_width_pt,
        default_row_height_pt,
        data,
        formulas,
        merges,
        freeze,
        show_grid_lines,
        auto_filter,
        validations,
        comments,
    })
}

fn parse_sheet_view_attrs(e: &quick_xml::events::BytesStart, show_grid_lines: &mut bool) {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"showGridLines" {
            let v = attr.unescape_value().unwrap_or_default();
            *show_grid_lines = !(v == "0" || v.eq_ignore_ascii_case("false"));
        }
    }
}

fn parse_pane_attrs(e: &quick_xml::events::BytesStart, freeze: &mut Option<FreezePane>) {
    let mut x_split = 0u32;
    let mut y_split = 0u32;
    let mut top_left = (1u32, 1u32);
    let mut is_frozen = false;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"xSplit" => {
                x_split = attr
                    .unescape_value()
                    .unwrap_or_default()
                    .parse::<f32>()
                    .unwrap_or(0.0) as u32;
            }
            b"ySplit" => {
                y_split = attr
                    .unescape_value()
                    .unwrap_or_default()
                    .parse::<f32>()
                    .unwrap_or(0.0) as u32;
            }
            b"topLeftCell" => {
                let s = attr.unescape_value().unwrap_or_default();
                if let Some(rc) = parse_cell_ref_1based(&s) {
                    top_left = rc;
                }
            }
            b"state" => {
                let s = attr.unescape_value().unwrap_or_default();
                is_frozen = s == "frozen" || s == "frozenSplit";
            }
            _ => {}
        }
    }

    if is_frozen && (x_split > 0 || y_split > 0) {
        *freeze = Some(FreezePane {
            freeze_rows: y_split,
            freeze_cols: x_split,
            top_left_row: top_left.0,
            top_left_col: top_left.1,
        });
    }
}

fn parse_sheet_format_pr(
    e: &quick_xml::events::BytesStart,
    mdw_px: f32,
    default_col_width_pt: &mut f32,
    default_row_height_pt: &mut f32,
) {
    let mut base_col_w = 8.0f32;
    let mut explicit_default_col_w: Option<f32> = None;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"baseColWidth" => {
                base_col_w = attr
                    .unescape_value()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or(8.0);
            }
            b"defaultColWidth" => {
                explicit_default_col_w = attr.unescape_value().unwrap_or_default().parse().ok();
            }
            b"defaultRowHeight" => {
                if let Ok(h) = attr.unescape_value().unwrap_or_default().parse::<f32>() {
                    *default_row_height_pt = h;
                }
            }
            _ => {}
        }
    }

    let w_chars =
        explicit_default_col_w.unwrap_or_else(|| default_column_width_chars(base_col_w, mdw_px));
    *default_col_width_pt = excel_column_width_to_points_with_mdw(w_chars, mdw_px);
}

fn parse_col_attrs(
    e: &quick_xml::events::BytesStart,
    mdw_px: f32,
    default_col_width_pt: f32,
    custom_cols: &mut HashMap<u32, f32>,
) {
    let mut min_col = 1u32;
    let mut max_col = 1u32;
    let mut width_chars: Option<f32> = None;
    let mut hidden = false;

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"min" => min_col = attr.unescape_value().unwrap_or_default().parse().unwrap_or(1),
            b"max" => max_col = attr.unescape_value().unwrap_or_default().parse().unwrap_or(1),
            b"width" => width_chars = attr.unescape_value().unwrap_or_default().parse().ok(),
            b"hidden" => {
                let v = attr.unescape_value().unwrap_or_default();
                hidden = v == "1" || v.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }

    let pt = if hidden {
        0.0
    } else if let Some(w) = width_chars {
        excel_column_width_to_points_with_mdw(w, mdw_px)
    } else {
        default_col_width_pt
    };

    let end = max_col.min(16384);
    if min_col >= 1 && min_col <= end {
        for c in min_col..=end {
            custom_cols.insert(c, pt);
        }
    }
}

fn extract_f_attrs(
    e: &quick_xml::events::BytesStart,
    f_type: &mut String,
    f_ref: &mut Option<CellRange>,
    f_si: &mut Option<u32>,
) {
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"t" => *f_type = attr.unescape_value().unwrap_or_default().into_owned(),
            b"ref" => {
                let r_str = attr.unescape_value().unwrap_or_default();
                *f_ref = parse_range_1based(&r_str);
            }
            b"si" => *f_si = attr.unescape_value().unwrap_or_default().parse().ok(),
            _ => {}
        }
    }
}

fn extract_ref_range(e: &quick_xml::events::BytesStart) -> Option<CellRange> {
    for attr in e.attributes().flatten() {
        if attr.key.local_name().as_ref() == b"ref" {
            return parse_range_1based(&attr.unescape_value().unwrap_or_default());
        }
    }
    None
}

fn extract_dv_attrs(e: &quick_xml::events::BytesStart) -> DataValidationEntry {
    let mut dv = DataValidationEntry::default();
    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"type" => dv.validation_type = attr.unescape_value().unwrap_or_default().into_owned(),
            b"sqref" => {
                let sq = attr.unescape_value().unwrap_or_default();
                for part in sq.split_whitespace() {
                    if let Some(r) = parse_range_1based(part) {
                        dv.ranges.push(r);
                    }
                }
            }
            b"allowBlank" => {
                let v = attr.unescape_value().unwrap_or_default();
                dv.allow_blank = v == "1" || v.eq_ignore_ascii_case("true");
            }
            _ => {}
        }
    }
    dv
}

