use argosheet::calc::{compare_with_golden, evaluate_workbook, CalcErr, Value};
use argosheet::parse::{format_cell_address, open_xlsx_workbook};
use argosheet::CELL_FLAG_HAS_FORMULA;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!(
            "Usage: argosheet_dump <file.xlsx> [--cells <sheet>] [--check-golden <golden_values.json>]"
        );
        process::exit(2);
    }

    let mut file_path: Option<String> = None;
    let mut target_sheet: Option<String> = None;
    let mut golden_path: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cells" if i + 1 < args.len() => {
                target_sheet = Some(args[i + 1].clone());
                i += 2;
            }
            "--check-golden" if i + 1 < args.len() => {
                golden_path = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                if file_path.is_none() {
                    file_path = Some(args[i].clone());
                }
                i += 1;
            }
        }
    }

    let Some(path) = file_path else {
        eprintln!("엑셀 파일 경로가 필요합니다.");
        process::exit(2);
    };

    let wb = match open_xlsx_workbook(&path) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };

    if let Some(gp) = golden_path {
        let json_str = fs::read_to_string(&gp).expect("golden_values.json 읽기 실패");
        let golden = parse_golden_json(&json_str);
        let calc_res = evaluate_workbook(&wb);
        let (matched, total, reports) = compare_with_golden(&wb, &calc_res, &golden);

        for rep in &reports {
            println!("{} : {} / {}", rep.sheet_name, rep.matched, rep.total);
            for (addr, exp, act) in &rep.diffs {
                println!("  DIFF {}\texpected={}\tactual={}", addr, exp, act);
            }
        }
        let ratio = if total > 0 {
            (matched as f64 / total as f64) * 100.0
        } else {
            100.0
        };
        println!("TOTAL : {} / {} ({:.2}%)", matched, total, ratio);
        if total > 0 && matched * 100 < total * 99 {
            process::exit(1);
        }
        return;
    }

    if let Some(sheet_query) = target_sheet {
        let calc_res = evaluate_workbook(&wb);
        let Some((s_idx, sheet)) = wb.sheets.iter().enumerate().find(|(idx, s)| {
            s.name == sheet_query
                || s.name.starts_with(&sheet_query)
                || idx.to_string() == sheet_query
        }) else {
            eprintln!("시트를 찾을 수 없습니다: {}", sheet_query);
            process::exit(1);
        };

        for (r_pos, &row_1based) in sheet.data.row_ids.iter().enumerate() {
            let start = sheet.data.row_start[r_pos] as usize;
            let end = sheet.data.row_start[r_pos + 1] as usize;
            for cell in &sheet.data.cells[start..end] {
                let addr = format_cell_address(row_1based, cell.col);
                let has_f = (cell.flags & CELL_FLAG_HAS_FORMULA) != 0;
                let kind_str = cell.kind_enum().as_tag_str(has_f);
                let raw_val = if has_f {
                    calc_res
                        .values
                        .get(&argosheet::calc::CellAddr {
                            sheet: s_idx,
                            row: row_1based,
                            col: cell.col,
                        })
                        .and_then(|v| v.coerce_str().ok())
                        .unwrap_or_default()
                } else {
                    sheet.raw_value_str(&wb.pool, cell)
                };
                let formula = sheet.formula_str(&wb.pool, row_1based, cell.col);
                println!("{}\t{}\t{}\t{}", addr, kind_str, raw_val, formula);
            }
        }
    } else {
        for sheet in &wb.sheets {
            println!(
                "{} · {} · {} · {} · {}",
                sheet.name,
                sheet.max_row,
                sheet.max_col,
                sheet.cell_count,
                sheet.formulas.len()
            );
        }
    }
}

/// 외부 crate(serde_json) 추가 없이 golden_values.json(`{"시트명": {"A1": 값, ...}}`)을 읽는 경량 파서
fn parse_golden_json(input: &str) -> HashMap<String, HashMap<String, Value>> {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out: HashMap<String, HashMap<String, Value>> = HashMap::new();

    fn skip_ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }

    fn read_str(b: &[u8], i: &mut usize) -> Option<String> {
        skip_ws(b, i);
        if *i >= b.len() || b[*i] != b'"' {
            return None;
        }
        *i += 1;
        let mut s = String::new();
        while *i < b.len() {
            if b[*i] == b'\\' && *i + 1 < b.len() {
                *i += 1;
                match b[*i] {
                    b'n' => s.push('\n'),
                    b'r' => s.push('\r'),
                    b't' => s.push('\t'),
                    c => s.push(c as char),
                }
                *i += 1;
            } else if b[*i] == b'"' {
                *i += 1;
                return Some(s);
            } else {
                let ch = std::str::from_utf8(&b[*i..]).ok()?.chars().next()?;
                s.push(ch);
                *i += ch.len_utf8();
            }
        }
        None
    }

    skip_ws(bytes, &mut i);
    if i < bytes.len() && bytes[i] == b'{' {
        i += 1;
    }

    while i < bytes.len() {
        skip_ws(bytes, &mut i);
        if i >= bytes.len() || bytes[i] == b'}' {
            break;
        }
        let Some(sheet_name) = read_str(bytes, &mut i) else {
            break;
        };
        skip_ws(bytes, &mut i);
        if i < bytes.len() && bytes[i] == b':' {
            i += 1;
        }
        skip_ws(bytes, &mut i);
        if i < bytes.len() && bytes[i] == b'{' {
            i += 1;
        }

        let mut sheet_map = HashMap::new();
        while i < bytes.len() {
            skip_ws(bytes, &mut i);
            if i < bytes.len() && bytes[i] == b'}' {
                i += 1;
                break;
            }
            let Some(cell_addr) = read_str(bytes, &mut i) else {
                break;
            };
            skip_ws(bytes, &mut i);
            if i < bytes.len() && bytes[i] == b':' {
                i += 1;
            }
            skip_ws(bytes, &mut i);

            let val = if i < bytes.len() && bytes[i] == b'"' {
                let s = read_str(bytes, &mut i).unwrap_or_default();
                if let Some(err) = CalcErr::from_str(&s) {
                    Value::Err(err)
                } else {
                    Value::Str(s)
                }
            } else {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
                {
                    i += 1;
                }
                let tok = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                if tok == "true" {
                    Value::Bool(true)
                } else if tok == "false" {
                    Value::Bool(false)
                } else if tok == "null" {
                    Value::Blank
                } else if let Ok(n) = tok.parse::<f64>() {
                    Value::Num(n)
                } else {
                    Value::Str(tok.to_string())
                }
            };

            sheet_map.insert(cell_addr, val);
            skip_ws(bytes, &mut i);
            if i < bytes.len() && bytes[i] == b',' {
                i += 1;
            }
        }
        out.insert(sheet_name, sheet_map);
        skip_ws(bytes, &mut i);
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
        }
    }

    out
}

