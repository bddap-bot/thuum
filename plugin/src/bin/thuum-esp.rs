fn main() {
    let path = std::env::args().nth(1).expect("usage: thuum-esp <output path>");
    std::fs::write(&path, thuum::esp::build()).unwrap_or_else(|e| panic!("{path}: {e}"));
}
