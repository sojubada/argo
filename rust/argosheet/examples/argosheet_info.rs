//! 검수용: 시트별 레이아웃 정보를 찍는다(병합·틀 고정·유효성·메모·너비).
fn main() {
    let path = std::env::args().nth(1).expect("파일 경로");
    let book = argosheet::parse::open_xlsx_workbook(&path).expect("열기");
    let mut m = 0;
    let mut v = 0;
    let mut c = 0;
    for s in &book.sheets {
        println!(
            "{} · 병합 {} · 틀고정 {:?} · 유효성 {} · 메모 {} · 격자선 {} · 너비합 {:.1}pt · 높이합 {:.1}pt",
            s.name,
            s.merges.len(),
            s.freeze.as_ref().map(|f| (f.freeze_rows, f.freeze_cols, f.top_left_row, f.top_left_col)),
            s.validations.len(),
            s.comments.len(),
            s.show_grid_lines,
            s.total_width_pt(),
            s.total_height_pt()
        );
        m += s.merges.len();
        v += s.validations.len();
        c += s.comments.len();
    }
    println!("합계: 병합 {m} · 유효성 {v} · 메모 {c} · 정의된 이름 {}", book.defined_names.len());
}
