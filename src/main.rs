mod module;
mod tokens;

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();

    let Some(x) = args.get(1) else {
        return Err(Box::new(std::io::Error::other("Missing input")));
    };

    let path = PathBuf::from(x);

    let module = module::Module::from_file(&path)?;

    dbg!(module);

    Ok(())
}
