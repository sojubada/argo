//! 시험용: cargo run --release --example doc2pdf -- 입력 출력 [글꼴.ttf]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(font) = args.get(3) {
        let bytes = std::fs::read(font).expect("font");
        minipdf::register_font("Pretendard", bytes.clone());
        minipdf::register_font("Pretendard bold", bytes);
    }
    if let Some(font) = args.get(4) {
        minipdf::register_font("Pretendard bold", std::fs::read(font).expect("font"));
    }
    let input = std::fs::read(&args[1]).expect("input");
    println!("fonts: {:?}", minipdf::doc_font_names(&input));
    let pdf = minipdf::convert_bytes_to_pdf(&input).expect("convert");
    std::fs::write(&args[2], pdf).expect("write");
    println!("stats: {:?}", minipdf::conversion_stats());
    println!("notices: {:?}", minipdf::conversion_notices());
}
