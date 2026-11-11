use argosheet::parse::{format_cell_address, open_xlsx_workbook};
use argosheet::CELL_FLAG_HAS_FORMULA;
use std::env;
use std::process;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("Usage: argosheet_dump <file.xlsx> [--cells <sheet_name_or_index>]");
        process::exit(2);
    }

    let mut file_path: Option<String> = None;
    let mut target_sheet: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--cells" {
            if i + 1 < args.len() {
                target_sheet = Some(args[i + 1].clone());
                i += 2;
                continue;
            } else {
                eprintln!("--cells 옵션 뒤에 시트 이름이나 번호가 필요합니다.");
                process::exit(2);
            }
        } else if file_path.is_none() {
            file_path = Some(args[i].clone());
        }
        i += 1;
    }

    let Some(path) = file_path else {
        eprintln!("엑셀 파일 경로가 지정되지 않았습니다.");
        process::exit(2);
    };

    let wb = match open_xlsx_workbook(&path) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("ERROR: {}", e);
            process::exit(1);
        }
    };

    if let Some(sheet_query) = target_sheet {
        let found = wb
            .sheets
            .iter()
            .enumerate()
            .find(|(idx, s)| {
                s.name == sheet_query
                    || s.name.starts_with(&sheet_query)
                    || idx.to_string() == sheet_query
            })
            .map(|(_, s)| s);

        let Some(sheet) = found else {
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
                let raw_val = sheet
                    .raw_value_str(&wb.pool, cell)
                    .replace('\r', "\\r")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t");
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

