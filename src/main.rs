use jaw::prelude::*;

use std::path::PathBuf;

use clap::Parser;
use ariadne::{ColorGenerator, Label, Report, ReportKind, Source};

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

fn show_io_error(error: std::io::Error) {
    eprintln!("IO error: {}", error);
}

fn make_report_span(name: &str, span: Span) -> (String, std::ops::Range<usize>) {
    match span {
        Span::Builtin => ("Builtin".to_owned(), 0..0),
        Span::Source(s, e) => (name.to_owned(), s.offset..e.offset),
    }
}

fn eprint_report(
    name: &str,
    source: &str,
    head: Span,
    message: impl AsRef<str>,
    labels: &[(Span, String)],
) {
    let mut colors = ColorGenerator::new();
    let mut builder = Report::build(ReportKind::Error, make_report_span(name, head))
        .with_message(message.as_ref().to_string());

    for (i, (span, msg)) in labels.iter().enumerate() {
        // Use distinct colors for each label for readability
        let color = match i {
            0 => colors.next(),
            1 => colors.next(),
            _ => colors.next(),
        };
        builder = builder.with_label(
            Label::new(make_report_span(name, *span))
                .with_message(msg.clone())
                .with_color(color),
        );
    }

    builder
        .finish()
        .eprint((name.to_owned(), Source::from(source)))
        .unwrap();
}

fn show_error(name: &str, source: &str, error: ModuleBuildError) {
    let error_string = error.to_string();

    match error {
        ModuleBuildError::IO(error) => show_io_error(error),
        ModuleBuildError::Lex(lex_error) => match lex_error {
            LexError::LexErrLoc(note, span) => {
                eprint_report(name, source, span, note, &[]);
            }
            LexError::IO(error) => show_io_error(error),
        },
        ModuleBuildError::UnknownType { ty: _, error_at } => {
            eprint_report(
                name,
                source,
                error_at,
                &error_string,
                &[(error_at, "unknown type here".into())],
            );
        }
        ModuleBuildError::BadLookup {
            type_name,
            first_seen,
            looking_up_from,
        } => {
            eprint_report(
                name,
                source,
                looking_up_from,
                format!("incomplete type '{}': not defined", type_name),
                &[
                    (looking_up_from, "used here".into()),
                    (first_seen, "first seen here".into()),
                ],
            );
        }
        ModuleBuildError::NonPODInPack(_, span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "non-POD member".into())],
            );
        }
        ModuleBuildError::NonIntEnum(_, span, _) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "enum must be integer-based".into())],
            );
        }
        ModuleBuildError::EnumValueOutOfRange { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "value out of range".into())],
            );
        }
        ModuleBuildError::NonIntBitfld(_, span, _) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "bitfield base must be integer".into())],
            );
        }
        ModuleBuildError::BitfldVOutOfRange { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "bit range out of bounds".into())],
            );
        }
        ModuleBuildError::NonIntVariant(_, span, _) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "variant base must be integer".into())],
            );
        }
        ModuleBuildError::VariantValueOutOfRange { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "discriminant out of range".into())],
            );
        }
        ModuleBuildError::FixedArrayRequiresPOD(span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "[I * M] requires M to be POD".into())],
            );
        }
        ModuleBuildError::ArraySizeTypeNotInteger { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "size type must be primitive integer".into())],
            );
        }
        ModuleBuildError::ArrayCountNotPositive { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "I must be positive".into())],
            );
        }
        ModuleBuildError::InvalidTypeName { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "invalid type name".into())],
            );
        }
        ModuleBuildError::DuplicateTypeDefinition { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "duplicate type definition".into())],
            );
        }
        ModuleBuildError::BitfieldMemberTypeInvalid { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "bitfield member must be primitive or enum".into())],
            );
        }
        ModuleBuildError::UnexpectedDefault { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "unexpected default here".into())],
            );
        }
        ModuleBuildError::InvalidBitRange { at, .. } => {
            eprint_report(
                name,
                source,
                at,
                &error_string,
                &[(at, "invalid bit range".into())],
            );
        }
        ModuleBuildError::EnumValueNotInteger(span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "enum value must be integer".into())],
            );
        }
        ModuleBuildError::VariantValueNegative(span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "variant value must be non-negative".into())],
            );
        }
        ModuleBuildError::NonPrimitiveEnumBase(_, _, span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "enum base must be primitive".into())],
            );
        }
        ModuleBuildError::NonPrimitiveVariantBase(_, _, span) => {
            eprint_report(
                name,
                source,
                span,
                &error_string,
                &[(span, "variant base must be primitive".into())],
            );
        }
        ModuleBuildError::UnexpectedToken { expected, found } => {
            let span = found.span;
            eprint_report(
                name,
                source,
                span,
                format!("unexpected token: expected {}", expected),
                &[(span, format!("found {:?}", found.kind))],
            );
        }
        ModuleBuildError::UnexpectedEOF => {
            eprintln!("{}", error_string);
        }
        ModuleBuildError::Internal(_) => {
            eprintln!("{}", error_string);
        }
    }
}

fn show_gen_error(error: GeneratorError) {
    match error {
        GeneratorError::IO(error) => {
            show_io_error(error);
        }
    }
}

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = Arguments::parse();

    let file_stem = args
        .input
        .file_stem()
        .and_then(|x| x.to_str())
        .unwrap_or("module")
        .to_string();

    let source = match std::fs::read_to_string(args.input) {
        Ok(x) => x,
        Err(e) => {
            show_io_error(e);
            return ExitCode::FAILURE;
        }
    };

    let module = match PartialModule::from_string(&file_stem, &source) {
        Ok(x) => x,
        Err(e) => {
            show_error(&file_stem, &source, e);
            return ExitCode::FAILURE;
        }
    };
    let module = module.compile();

    let output = match std::fs::File::create(args.output) {
        Ok(x) => x,
        Err(e) => {
            show_io_error(e);
            return ExitCode::FAILURE;
        }
    };

    let output = std::io::BufWriter::new(output);

    match emit_for(args.kind, module, output) {
        Ok(x) => x,
        Err(e) => {
            show_gen_error(e);
            return ExitCode::FAILURE;
        }
    };

    ExitCode::SUCCESS
}
