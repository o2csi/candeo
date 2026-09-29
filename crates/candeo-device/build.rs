//! Embeds the unverified definitions (`devices/unverified/*.json`), less the
//! facts their generators read, as `UNVERIFIED` in `definition.rs`: the
//! application recognises the devices they describe without downloading
//! anything (`docs/design/device-sdk.md` §9, #300). Listed here rather than by
//! hand, so a definition the generators write is embedded without a line more.

use std::path::Path;
use std::{env, fs};

fn main() {
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("devices/unverified");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("devices/unverified")
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| {
            name.ends_with(".json") && !name.ends_with(".facts.json") && name != "variants.json"
        })
        .collect();
    files.sort();
    let mut out = String::from("pub const UNVERIFIED: &[(&str, &str)] = &[\n");
    for file in &files {
        let path = dir.join(file).display().to_string();
        out.push_str(&format!("    ({file:?}, include_str!({path:?})),\n"));
    }
    out.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("unverified.rs"),
        out,
    )
    .expect("unverified.rs");
}
