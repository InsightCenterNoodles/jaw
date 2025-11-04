pub mod codegen;
mod module;
mod tokenreader;
mod tokens;

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();

    let Some(x) = args.get(1) else {
        eprintln!(
            "Usage: {} <input.jaw> [output.hpp]",
            args.get(0).map(String::as_str).unwrap_or("jaw")
        );
        return Err(Box::new(std::io::Error::other("Missing input")));
    };

    let path = PathBuf::from(x);

    let module = module::PartialModule::from_file(&path)?;
    let module = module.compile();

    // Emit C++ header
    let header = codegen::emit_cpp_header(module);
    if let Some(out_path) = args.get(2) {
        std::fs::write(out_path, header)?;
    } else {
        println!("{}", header);
    }

    Ok(())
}
