use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use jaw::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    // Repo root is one level up from generated/rust
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .ok_or("no parent dir")?
        .to_path_buf();
    let jaw_path = repo_root.join("assets").join("basic.jaw");

    println!("cargo:rerun-if-changed={}", jaw_path.display());

    let src = fs::read_to_string(&jaw_path)?;
    let stem = jaw_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    let pm = PartialModule::from_string(stem, &src)?.compile();

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_file = out_dir.join("basic.rs");
    let f = fs::File::create(&out_file)?;
    let mut w = std::io::BufWriter::new(f);
    emit_for(KnownGenerators::RUST, pm, &mut w)?;
    w.flush()?;

    Ok(())
}
