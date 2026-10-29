//! 시험용: 망가진 .doc 들에서 패닉이 나는지.
fn main() {
    let mut panics = 0;
    for entry in std::fs::read_dir(std::env::args().nth(1).unwrap()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        if std::panic::catch_unwind(|| { let _ = minipdf::doc_to_docx(&bytes); }).is_err() {
            panics += 1;
        }
    }
    println!("panics: {panics}");
}
