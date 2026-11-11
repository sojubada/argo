fn main() {
    let args: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&args[1]).expect("emf");
    match argoconv::slides::debug_emf_to_svg(&data) {
        Some(svg) => {
            if args.len() > 2 {
                std::fs::write(&args[2], &svg).unwrap();
            }
            println!("ok {} bytes", svg.len());
        }
        None => println!("None (예전 변환기로 넘어감)"),
    }
}
