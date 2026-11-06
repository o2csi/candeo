//! The same program as `size-oxc` without the transpiler: the difference in
//! size between the two release binaries is what Oxc adds.

fn main() {
    let path = std::env::args().nth(1).expect("a file");
    let source = std::fs::read_to_string(path).expect("readable");
    print!("{source}");
}
