use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use jaw::codegen;

#[derive(Debug, Clone, ValueEnum)]
enum GeneratorKind {
    Cpp,
    Python,
    Rust,
}

#[derive(Debug, clap::Parser)]
#[command(version, about)]
struct Arguments {
    /// Input *.jaw file
    input: PathBuf,

    /// Type of code to generate
    #[arg(short, long, value_enum, default_value = "cpp")]
    kind: GeneratorKind,

    /// Output path, based on input
    output: PathBuf,
}

use std::process::ExitCode;

fn process() -> anyhow::Result<()> {
    let args = Arguments::parse();

    let file_stem = args
        .input
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("module")
        .to_string();

    let source = std::fs::read_to_string(args.input)?;

    let module = jaw::intermediate::Module::from_string(file_stem, source)?;

    let world = jaw::compile::compile(module)?;

    match args.kind {
        GeneratorKind::Cpp => codegen::emit_cpp(&world, args.output)?,
        GeneratorKind::Python => codegen::emit_python(&world, args.output)?,
        GeneratorKind::Rust => codegen::emit_rust(&world, args.output)?,
    }

    Ok(())
}

fn main() -> ExitCode {
    match process() {
        Err(x) => {
            println!("Error {x}");

            for (i, cause) in x.chain().enumerate() {
                eprintln!("  {i}: {cause}");
            }

            ExitCode::FAILURE
        }
        _ => ExitCode::SUCCESS,
    }
}
