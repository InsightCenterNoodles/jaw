use jaw::prelude::*;

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, clap::Parser)]
#[command(version, about)]
struct Arguments {
    /// Input *.jaw file
    input: PathBuf,

    /// Type of code to generate
    #[arg(short, long, value_enum)]
    kind: KnownGenerators,

    /// Output path, based on input
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse();

    let module = PartialModule::from_file(&args.input)?;
    let module = module.compile();

    let output = std::io::BufWriter::new(std::fs::File::create(args.output)?);

    emit_for(args.kind, module, output)?;

    Ok(())
}
