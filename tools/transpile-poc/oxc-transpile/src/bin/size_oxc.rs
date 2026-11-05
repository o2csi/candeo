//! Transpiles one file and prints the JavaScript, or the errors: `size-baseline`
//! plus Oxc, for the binary size Oxc adds.

use oxc_transpile::{transpile, Options};

fn main() {
    let path = std::env::args().nth(1).expect("a file");
    let source = std::fs::read_to_string(path).expect("readable");
    match transpile(&source, &Options::default()) {
        Ok(output) => print!("{}", output.js),
        Err(errors) => {
            for e in errors {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    }
}
