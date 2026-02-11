use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use jaw::{GlobalOptions, codegen};

#[derive(Debug, Clone, ValueEnum)]
enum GeneratorKind {
    Cpp,
    Python,
    Rust,
}

#[derive(Debug, clap::Parser)]
#[command(version, about)]
struct Arguments {
    /// Output path, based on input
    output: PathBuf,

    /// Type of code to generate
    #[arg(short, long, value_enum, default_value = "cpp")]
    kind: GeneratorKind,

    #[command(flatten)]
    options: GlobalOptions,

    /// Input *.jaw files. Multiple inputs are merged into a single output module.
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
}

use std::process::ExitCode;

/// Parse CLI args, compile the input module, and emit code for the selected generator.
fn process() -> anyhow::Result<()> {
    let args = Arguments::parse();

    for input in args.inputs {
        let file_stem = input
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("module")
            .to_string();

        let module = jaw::intermediate::load_module_with_imports(&args.input)
            .with_context(|| format!("while loading module {file_stem}"))?;

        let world = jaw::compile::compile(module)?;
    }

    match args.kind {
        GeneratorKind::Cpp => codegen::emit_cpp(&world, &args.options, args.output)?,
        GeneratorKind::Python => codegen::emit_python(&world, &args.options, args.output)?,
        GeneratorKind::Rust => codegen::emit_rust(&world, &args.options, args.output)?,
    }

    Ok(())
}

/// CLI entrypoint; prints errors to stderr and returns a non-zero exit code on failure.
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
