use crate::cf::{
    CfBlock, CfRule, CfRuleKind, Cfvo, CfvoType, ColorScaleSpec, DataBarAxisPos, DataBarSpec,
    IconSetKind, IconSetSpec, TableDef,
};
use crate::package::{is_rel_type, read_part_relationships, resolve_part_target, Relationship};
use crate::style::{
    default_column_width_chars, excel_column_width_to_points_with_mdw, parse_color_attrs,
    parse_styles_xml, read_theme_colors, Rgba, StyleSheet,
};
use crate::{
    Cell, CellKind, CellPayload, CellRange, CommentEntry, DataValidationEntry, DefinedName,
    FormulaEntry, FormulaKind, FreezePane, HyperlinkEntry, Rows, Sheet, SheetState, StrSpan,
    StringPool, Workbook, CELL_FLAG_FORMULA_ARRAY, CELL_FLAG_FORMULA_SHARED,
    CELL_FLAG_HAS_COMMENT, CELL_FLAG_HAS_FORMULA, CELL_FLAG_HAS_LINK,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use zip::ZipArchive;

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

pub fn format_cell_address(row_1based: u32, col_1based: u32) -> String {
    format!("{}{}", col_to_letters(col_1based), row_1based)
}

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

pub fn parse_sqref_ranges(sqref: &str) -> Vec<CellRange> {
    sqref
        .split_whitespace()
        .filter_map(parse_range_1based)
        .collect()
}

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
        if b == b'$' || b.is_ascii_alphabetic() || b == b'_' {
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
    if !(1..=16384).contains(&col_idx) {
        return None;
    }

    let row_idx: i32 = token[row_start..].parse().ok()?;
    if !(1..=1_048_576).contains(&row_idx) {
        return None;
    }

    let new_col = if col_abs { col_idx } else { col_idx + d_col };
    let new_row = if row_abs { row_idx } else { row_idx + d_row };

    if !(1..=16384).contains(&new_col) || !(1..=1_048_576).contains(&new_row) {
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

    let root_rels = read_part_relationships(&mut archive, "")?;
    let workbook_path = root_rels
        .values()
        .find(|r| is_rel_type(&r.rel_type, "/officeDocument"))
        .map(|r| resolve_part_target("", &r.target))
        .unwrap_or_else(|| "xl/workbook.xml".to_string());

    let wb_rels = read_part_relationships(&mut archive, &workbook_path)?;
    let wb_meta = parse_workbook_xml(&mut archive, &workbook_path)?;

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

        // 시트에 연결된 모든 표(xl/tables/tableN.xml) 파싱
        let mut tables = Vec::new();
        for rel in sheet_rels.values() {
            if is_rel_type(&rel.rel_type, "/table") {
                let tbl_path = resolve_part_target(&sheet_path, &rel.target);
                if archive.by_name(&tbl_path).is_ok() {
                    if let Ok(tbl) = parse_table_xml(&mut archive, &tbl_path) {
                        tables.push(tbl);
                    }
                }
            }
        }

        let sheet = parse_worksheet_xml(
            &mut archive,
            &sheet_path,
            entry.name,
            entry.sheet_id,
            entry.state,
            &sst_spans,
            &styles,
            &sheet_rels,
            comments,
            tables,
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

/// `xl/tables/tableN.xml`을 파싱하여 표 범위, 머리글/요약 행, 줄무늬, 필터 단추 여부를 읽는다.
fn parse_table_xml<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    table_path: &str,
) -> Result<TableDef, String> {
    let file = archive
        .by_name(table_path)
        .map_err(|e| format!("{} 없음: {}", table_path, e))?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);

    let mut buf = Vec::with_capacity(512);
    let mut name = String::from("Table");
    let mut range = CellRange {
        start_row: 1,
        start_col: 1,
        end_row: 1,
        end_col: 1,
    };
    let mut header_row_count = 1u32;
    let mut totals_row_count = 0u32;
    let mut has_auto_filter = false;
    let mut style_name = String::from("TableStyleMedium2");
    let mut show_first_column = false;
    let mut show_last_column = false;
    let mut show_row_stripes = true;
    let mut show_column_stripes = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"table" => {
                    for attr in e.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"name" | b"displayName" => {
                                name = attr.unescape_value().unwrap_or_default().into_owned();
                            }
                            b"ref" => {
                                if let Some(r) =
                                    parse_range_1based(&attr.unescape_value().unwrap_or_default())
                                {
                                    range = r;
                                }
                            }
                            b"headerRowCount" => {
                                header_row_count = attr
                                    .unescape_value()
                                    .unwrap_or_default()
                                    .parse()
                                    .unwrap_or(1);
                            }
                            b"totalsRowCount" => {
                                totals_row_count = attr
                                    .unescape_value()
                                    .unwrap_or_default()
                                    .parse()
                                    .unwrap_or(0);
                            }
                            _ => {}
                        }
                    }
                }
                b"autoFilter" => {
                    has_auto_filter = true;
                }
                b"tableStyleInfo" => {
                    for attr in e.attributes().flatten() {
                        let v = attr.unescape_value().unwrap_or_default();
                        let flag = v == "1" || v.eq_ignore_ascii_case("true");
                        match attr.key.local_name().as_ref() {
                            b"name" => style_name = v.into_owned(),
                            b"showFirstColumn" => show_first_column = flag,
                            b"showLastColumn" => show_last_column = flag,
                            b"showRowStripes" => show_row_stripes = flag,
                            b"showColumnStripes" => show_column_stripes = flag,
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("table XML 파싱 오류: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(TableDef {
        name,
        range,
        header_row_count,
        totals_row_count,
        has_auto_filter,
        style_name,
        show_first_column,
        show_last_column,
        show_row_stripes,
        show_column_stripes,
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
                b"workbookPr" => extract_workbook_pr(&e, &mut date1904),
                b"calcPr" => extract_calc_pr(&e, &mut full_calc_on_load),
                b"sheet" => sheets.push(extract_sheet_entry(&e)),
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

/// 조건부 서식(`<conditionalFormatting>` 및 `x14:conditionalFormatting`) 파싱용 임시 빌더
#[derive(Default)]
struct CfRuleBuilder {
    rule_type: String,
    operator: String,
    priority: i32,
    stop_if_true: bool,
    dxf_id: Option<u32>,
    bottom: bool,
    percent: bool,
    rank: u32,
    above: bool,
    equal_average: bool,
    std_dev: Option<i32>,
    text_attr: String,
    formulas: Vec<String>,
    cfvos: Vec<Cfvo>,
    colors: Vec<Rgba>,
    databar_min_len: u32,
    databar_max_len: u32,
    databar_show_val: bool,
    databar_axis_pos: Option<DataBarAxisPos>,
    databar_neg_color: Option<Rgba>,
    databar_axis_color: Option<Rgba>,
    icon_set_name: String,
    icon_reverse: bool,
    icon_show_val: bool,
}

impl CfRuleBuilder {
    fn from_attrs(e: &quick_xml::events::BytesStart) -> Self {
        let mut b = Self {
            priority: 9999,
            above: true,
            databar_min_len: 0,
            databar_max_len: 100,
            databar_show_val: true,
            icon_set_name: String::from("3Arrows"),
            icon_show_val: true,
            ..Default::default()
        };
        for attr in e.attributes().flatten() {
            let v = attr.unescape_value().unwrap_or_default();
            let flag = v == "1" || v.eq_ignore_ascii_case("true");
            match attr.key.local_name().as_ref() {
                b"type" => b.rule_type = v.into_owned(),
                b"operator" => b.operator = v.into_owned(),
                b"priority" => b.priority = v.parse().unwrap_or(9999),
                b"stopIfTrue" => b.stop_if_true = flag,
                b"dxfId" => b.dxf_id = v.parse().ok(),
                b"bottom" => b.bottom = flag,
                b"percent" => b.percent = flag,
                b"rank" => b.rank = v.parse().unwrap_or(10),
                b"aboveAverage" => b.above = !(v == "0" || v.eq_ignore_ascii_case("false")),
                b"equalAverage" => b.equal_average = flag,
                b"stdDev" => b.std_dev = v.parse().ok(),
                b"text" => b.text_attr = v.into_owned(),
                _ => {}
            }
        }
        b
    }

    fn finish(self) -> Option<CfRule> {
        let kind = match self.rule_type.as_str() {
            "dataBar" => {
                let min_cfvo = self.cfvos.first().cloned().unwrap_or(Cfvo {
                    cfvo_type: CfvoType::Min,
                    val: None,
                    gte: true,
                });
                let max_cfvo = self.cfvos.get(1).cloned().unwrap_or(Cfvo {
                    cfvo_type: CfvoType::Max,
                    val: None,
                    gte: true,
                });
                let color = self.colors.first().copied().unwrap_or(Rgba {
                    r: 59,
                    g: 130,
                    b: 246,
                    a: 255,
                });
                CfRuleKind::DataBar(DataBarSpec {
                    min_cfvo,
                    max_cfvo,
                    color,
                    negative_color: self.databar_neg_color,
                    axis_color: self.databar_axis_color,
                    axis_pos: self.databar_axis_pos.unwrap_or(DataBarAxisPos::Automatic),
                    min_length: self.databar_min_len,
                    max_length: self.databar_max_len,
                    show_value: self.databar_show_val,
                })
            }
            "colorScale" => CfRuleKind::ColorScale(ColorScaleSpec {
                cfvos: self.cfvos,
                colors: self.colors,
            }),
            "iconSet" => CfRuleKind::IconSet(IconSetSpec {
                kind: IconSetKind::from_str(&self.icon_set_name),
                cfvos: self.cfvos,
                reverse: self.icon_reverse,
                show_value: self.icon_show_val,
            }),
            "cellIs" => CfRuleKind::CellIs {
                operator: self.operator,
                formulas: self.formulas,
            },
            "expression" => CfRuleKind::Expression {
                formula: self.formulas.into_iter().next().unwrap_or_default(),
            },
            "top10" => CfRuleKind::Top10 {
                bottom: self.bottom,
                percent: self.percent,
                rank: self.rank,
            },
            "aboveAverage" => CfRuleKind::AboveAverage {
                above: self.above,
                equal_average: self.equal_average,
                std_dev: self.std_dev,
            },
            "duplicateValues" => CfRuleKind::DuplicateValues,
            "uniqueValues" => CfRuleKind::UniqueValues,
            "containsText" | "notContainsText" | "beginsWith" | "endsWith" => {
                let op = if self.operator.is_empty() {
                    self.rule_type.clone()
                } else {
                    self.operator
                };
                CfRuleKind::ContainsText {
                    text: self.text_attr,
                    operator: op,
                }
            }
            "containsBlanks" => CfRuleKind::ContainsBlanks,
            "notContainsBlanks" => CfRuleKind::NotContainsBlanks,
            "containsErrors" => CfRuleKind::ContainsErrors,
            "notContainsErrors" => CfRuleKind::NotContainsErrors,
            _ => return None,
        };

        Some(CfRule {
            priority: self.priority,
            stop_if_true: self.stop_if_true,
            dxf_id: self.dxf_id,
            dxf_inline: None,
            kind,
        })
    }
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
    sheet_rels: &HashMap<String, Relationship>,
    comments: Vec<CommentEntry>,
    tables: Vec<TableDef>,
    pool: &mut StringPool,
) -> Result<Sheet, String> {
    let file = archive
        .by_name(sheet_path)
        .map_err(|e| format!("워크시트 없음 ({}): {}", sheet_path, e))?;

    let mut reader = Reader::from_reader(BufReader::new(file));
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
    let mut links: HashMap<u64, HyperlinkEntry> = HashMap::new();
    let mut cf_blocks: Vec<CfBlock> = Vec::new();

    let mut custom_col_widths_pt: HashMap<u32, f32> = HashMap::new();
    let mut custom_row_heights_pt: HashMap<u32, f32> = HashMap::new();

    let mut data = Rows::default();
    let mut formulas: HashMap<u64, FormulaEntry> = HashMap::new();
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

    // 조건부 서식 상태 머신
    let mut cur_cf_block: Option<CfBlock> = None;
    let mut cur_cf_rule: Option<CfRuleBuilder> = None;
    let mut in_cf_formula = false;
    let mut cf_formula_buf = String::new();
    let mut in_cfvo_f = false;
    let mut cfvo_f_buf = String::new();
    let mut in_sqref_tag = false;
    let mut sqref_buf = String::new();

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
                    b"hyperlink" => {
                        extract_hyperlink_tag(&e, sheet_rels, pool, &mut links);
                    }
                    // P4: 조건부 서식 블록 (<conditionalFormatting> 및 <x14:conditionalFormatting>)
                    b"conditionalFormatting" => {
                        let mut ranges = Vec::new();
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"sqref" {
                                ranges = parse_sqref_ranges(
                                    &attr.unescape_value().unwrap_or_default(),
                                );
                            }
                        }
                        cur_cf_block = Some(CfBlock {
                            ranges,
                            rules: Vec::new(),
                        });
                    }
                    b"sqref" if cur_cf_block.is_some() => {
                        in_sqref_tag = true;
                        sqref_buf.clear();
                    }
                    b"cfRule" if cur_cf_block.is_some() => {
                        cur_cf_rule = Some(CfRuleBuilder::from_attrs(&e));
                    }
                    b"formula" if cur_cf_rule.is_some() => {
                        in_cf_formula = true;
                        cf_formula_buf.clear();
                    }
                    b"dataBar" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            parse_databar_attrs(&e, rb);
                        }
                    }
                    b"iconSet" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            parse_iconset_attrs(&e, rb);
                        }
                    }
                    b"cfvo" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            rb.cfvos.push(parse_cfvo_attrs(&e));
                        }
                    }
                    b"f" if cur_cf_rule.is_some() && !in_cell => {
                        // x14:cfvo 내부의 <xm:f> 값
                        in_cfvo_f = true;
                        cfvo_f_buf.clear();
                    }
                    b"color" | b"fillColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            if let Some(c) =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed)
                            {
                                rb.colors.push(c);
                            }
                        }
                    }
                    b"negativeFillColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            rb.databar_neg_color =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed);
                        }
                    }
                    b"axisColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            rb.databar_axis_color =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed);
                        }
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
                        has_f_tag = true;
                        extract_f_attrs(&e, &mut f_type, &mut f_ref, &mut f_si);
                    }
                    b"v" if in_cell => {
                        has_v_tag = true;
                    }
                    b"c" => {
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
                    b"hyperlink" => {
                        extract_hyperlink_tag(&e, sheet_rels, pool, &mut links);
                    }
                    b"cfRule" if cur_cf_block.is_some() => {
                        let rb = CfRuleBuilder::from_attrs(&e);
                        if let (Some(block), Some(rule)) = (cur_cf_block.as_mut(), rb.finish()) {
                            block.rules.push(rule);
                        }
                    }
                    b"cfvo" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            rb.cfvos.push(parse_cfvo_attrs(&e));
                        }
                    }
                    b"color" | b"fillColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            if let Some(c) =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed)
                            {
                                rb.colors.push(c);
                            }
                        }
                    }
                    b"negativeFillColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            rb.databar_neg_color =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed);
                        }
                    }
                    b"axisColor" if cur_cf_rule.is_some() => {
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let spec = parse_color_attrs(&e);
                            rb.databar_axis_color =
                                spec.resolve(&styles.theme_colors, &styles.custom_indexed);
                        }
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
                } else if in_cf_formula {
                    let raw = e.unescape().map_err(|err| err.to_string())?;
                    cf_formula_buf.push_str(&raw);
                } else if in_cfvo_f {
                    let raw = e.unescape().map_err(|err| err.to_string())?;
                    cfvo_f_buf.push_str(&raw);
                } else if in_sqref_tag {
                    let raw = e.unescape().map_err(|err| err.to_string())?;
                    sqref_buf.push_str(&raw);
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
                    b"f" if in_cell => in_f = false,
                    b"f" if in_cfvo_f => {
                        in_cfvo_f = false;
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            if let Some(last_cfvo) = rb.cfvos.last_mut() {
                                if last_cfvo.val.is_none() {
                                    last_cfvo.val = Some(cfvo_f_buf.trim().to_string());
                                }
                            }
                        }
                    }
                    b"v" => in_v = false,
                    b"t" if in_is => in_is_t = false,
                    b"rPh" if in_is => in_is_rph = false,
                    b"is" => in_is = false,
                    b"c" => {
                        in_cell = false;
                        let mut flags = 0u8;

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

                            if let Some((target_url, is_int)) =
                                try_extract_hyperlink_formula(&resolved_f)
                            {
                                flags |= CELL_FLAG_HAS_LINK;
                                let span = pool.intern(&target_url);
                                links.insert(
                                    crate::cell_key(current_row, cell_col),
                                    HyperlinkEntry {
                                        target: span,
                                        is_internal: is_int,
                                    },
                                );
                            }

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
                    b"formula" if in_cf_formula => {
                        in_cf_formula = false;
                        if let Some(rb) = cur_cf_rule.as_mut() {
                            let clean = cf_formula_buf
                                .trim()
                                .strip_prefix('=')
                                .unwrap_or(cf_formula_buf.trim());
                            rb.formulas.push(clean.to_string());
                        }
                    }
                    b"sqref" if in_sqref_tag => {
                        in_sqref_tag = false;
                        if let Some(block) = cur_cf_block.as_mut() {
                            block.ranges = parse_sqref_ranges(&sqref_buf);
                        }
                    }
                    b"cfRule" => {
                        if let Some(rb) = cur_cf_rule.take() {
                            if let (Some(block), Some(rule)) = (cur_cf_block.as_mut(), rb.finish())
                            {
                                block.rules.push(rule);
                            }
                        }
                    }
                    b"conditionalFormatting" => {
                        if let Some(block) = cur_cf_block.take() {
                            if !block.ranges.is_empty() && !block.rules.is_empty() {
                                cf_blocks.push(block);
                            }
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

    for cm in &comments {
        if let Some(cell) = data.get_cell_mut(cm.row, cm.col) {
            cell.flags |= CELL_FLAG_HAS_COMMENT;
        }
    }
    for &key in links.keys() {
        let r = (key >> 32) as u32;
        let c = (key & 0xFFFF_FFFF) as u32;
        if let Some(cell) = data.get_cell_mut(r, c) {
            cell.flags |= CELL_FLAG_HAS_LINK;
        }
    }

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
        links,
        cf_blocks,
        tables,
    })
}

fn parse_databar_attrs(e: &quick_xml::events::BytesStart, rb: &mut CfRuleBuilder) {
    for attr in e.attributes().flatten() {
        let v = attr.unescape_value().unwrap_or_default();
        match attr.key.local_name().as_ref() {
            b"minLength" => rb.databar_min_len = v.parse().unwrap_or(0),
            b"maxLength" => rb.databar_max_len = v.parse().unwrap_or(100),
            b"showValue" => rb.databar_show_val = !(v == "0" || v.eq_ignore_ascii_case("false")),
            b"axisPosition" => {
                rb.databar_axis_pos = Some(match v.as_ref() {
                    "middle" | "midpoint" => DataBarAxisPos::Midpoint,
                    "none" => DataBarAxisPos::None,
                    _ => DataBarAxisPos::Automatic,
                });
            }
            _ => {}
        }
    }
}

fn parse_iconset_attrs(e: &quick_xml::events::BytesStart, rb: &mut CfRuleBuilder) {
    for attr in e.attributes().flatten() {
        let v = attr.unescape_value().unwrap_or_default();
        match attr.key.local_name().as_ref() {
            b"iconSet" => rb.icon_set_name = v.into_owned(),
            b"reverse" => rb.icon_reverse = v == "1" || v.eq_ignore_ascii_case("true"),
            b"showValue" => rb.icon_show_val = !(v == "0" || v.eq_ignore_ascii_case("false")),
            _ => {}
        }
    }
}

fn parse_cfvo_attrs(e: &quick_xml::events::BytesStart) -> Cfvo {
    let mut cfvo = Cfvo::default();
    for attr in e.attributes().flatten() {
        let v = attr.unescape_value().unwrap_or_default();
        match attr.key.local_name().as_ref() {
            b"type" => cfvo.cfvo_type = CfvoType::from_str(&v),
            b"val" => cfvo.val = Some(v.into_owned()),
            b"gte" => cfvo.gte = !(v == "0" || v.eq_ignore_ascii_case("false")),
            _ => {}
        }
    }
    cfvo
}

fn extract_hyperlink_tag(
    e: &quick_xml::events::BytesStart,
    sheet_rels: &HashMap<String, Relationship>,
    pool: &mut StringPool,
    links: &mut HashMap<u64, HyperlinkEntry>,
) {
    let mut ref_str = String::new();
    let mut r_id = String::new();
    let mut location = String::new();

    for attr in e.attributes().flatten() {
        match attr.key.local_name().as_ref() {
            b"ref" => ref_str = attr.unescape_value().unwrap_or_default().into_owned(),
            b"id" => r_id = attr.unescape_value().unwrap_or_default().into_owned(),
            b"location" => location = attr.unescape_value().unwrap_or_default().into_owned(),
            _ => {}
        }
    }

    let Some(range) = parse_range_1based(&ref_str) else {
        return;
    };

    let (target_str, is_internal) = if !location.is_empty() {
        (location.trim_start_matches('#').to_string(), true)
    } else if let Some(rel) = sheet_rels.get(&r_id) {
        let is_ext = rel
            .target_mode
            .as_deref()
            .map(|m| m.eq_ignore_ascii_case("External"))
            .unwrap_or(true);
        (rel.target.clone(), !is_ext)
    } else {
        return;
    };

    let span = pool.intern(&target_str);
    // 링크 범위가 열·시트 전체(A1:XFD1048576)면 칸마다 넣다가 메모리가 터진다.
    // 한 링크는 1만 칸, 시트 전체는 20만 칸까지만 넣는다(그 밖은 링크 표시 없이 보인다).
    const MAX_PER_LINK: u64 = 10_000;
    const MAX_LINK_CELLS: usize = 200_000;
    let mut range = range;
    let width = (range.end_col.saturating_sub(range.start_col) as u64) + 1;
    if width > MAX_PER_LINK {
        range.end_col = range.start_col + (MAX_PER_LINK as u32 - 1);
    }
    let width = (range.end_col - range.start_col + 1) as u64;
    let max_rows = (MAX_PER_LINK / width).max(1) as u32;
    if range.end_row.saturating_sub(range.start_row) >= max_rows {
        range.end_row = range.start_row + max_rows - 1;
    }
    if links.len() >= MAX_LINK_CELLS {
        return;
    }
    for r in range.start_row..=range.end_row {
        for c in range.start_col..=range.end_col {
            links.insert(
                crate::cell_key(r, c),
                HyperlinkEntry {
                    target: span,
                    is_internal,
                },
            );
        }
    }
}

fn try_extract_hyperlink_formula(formula: &str) -> Option<(String, bool)> {
    let trimmed = formula.trim();
    if !trimmed.to_ascii_uppercase().starts_with("HYPERLINK(") {
        return None;
    }
    let inside = trimmed.get(10..trimmed.len().saturating_sub(1))?.trim();
    let first_quote = inside.strip_prefix('"')?;
    let end_quote = first_quote.find('"')?;
    let target = &first_quote[..end_quote];
    let is_internal = target.starts_with('#') || target.contains('!');
    Some((target.trim_start_matches('#').to_string(), is_internal))
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