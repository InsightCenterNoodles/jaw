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

fn show_io_error(error: std::io::Error) {
    eprintln!("IO error: {}", error);
}

fn show_error(name: &str, source: &str, error: ModuleBuildError) {
    use ariadne::{ColorGenerator, Label, Report, ReportKind, Source};

    let mut colors = ColorGenerator::new();

    // Generate & choose some colours for each of our elements
    let a = colors.next();
    let b = colors.next();
    //let out = Color::Fixed(81);

    let make_report_span = |span: Span| -> (String, std::ops::Range<usize>) {
        match span {
            Span::Builtin => ("Builtin".to_owned(), 0..0),
            Span::Source(position, position1) => {
                (name.to_owned(), position.offset..position1.offset)
            }
        }
    };

    let error_string = error.to_string();

    match error {
        ModuleBuildError::IO(error) => show_io_error(error),
        ModuleBuildError::Lex(lex_error) => match lex_error {
            LexError::LexErrLoc(note, span) => {
                Report::build(ReportKind::Error, make_report_span(span))
                    .with_message(note)
                    .finish()
                    .eprint((name.to_owned(), Source::from(source)))
                    .unwrap();
            }
            LexError::IO(error) => {
                show_io_error(error);
            }
        },
        ModuleBuildError::UnknownType { ty: _, error_at } => {
            Report::build(ReportKind::Error, make_report_span(error_at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(error_at))
                        .with_message("unknown type here")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::BadLookup {
            type_name,
            first_seen,
            looking_up_from,
        } => {
            Report::build(ReportKind::Error, make_report_span(looking_up_from))
                .with_message(format!("incomplete type '{}': not defined", type_name))
                .with_label(
                    Label::new(make_report_span(looking_up_from))
                        .with_message("used here")
                        .with_color(a),
                )
                .with_label(
                    Label::new(make_report_span(first_seen))
                        .with_message("first seen here")
                        .with_color(b),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonPODInPack(_, span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("non-POD member")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonIntEnum(_, span, _) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("enum must be integer-based")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::EnumValueOutOfRange { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("value out of range")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonIntBitfld(_, span, _) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("bitfield base must be integer")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::BitfldVOutOfRange { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("bit range out of bounds")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonIntVariant(_, span, _) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("variant base must be integer")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::VariantValueOutOfRange { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("discriminant out of range")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::FixedArrayRequiresPOD(span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("[I * M] requires M to be POD")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::ArraySizeTypeNotInteger { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("size type must be primitive integer")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::ArrayCountNotPositive { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("I must be positive")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::InvalidTypeName { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("invalid type name")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::DuplicateTypeDefinition { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("duplicate type definition")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::BitfieldMemberTypeInvalid { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("bitfield member must be primitive or enum")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::UnexpectedDefault { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("unexpected default here")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::InvalidBitRange { at, .. } => {
            Report::build(ReportKind::Error, make_report_span(at))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(at))
                        .with_message("invalid bit range")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::EnumValueNotInteger(span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("enum value must be integer")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::VariantValueNegative(span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("variant value must be non-negative")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonPrimitiveEnumBase(_, _, span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("enum base must be primitive")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::NonPrimitiveVariantBase(_, _, span) => {
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(error_string)
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message("variant base must be primitive")
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
        }
        ModuleBuildError::UnexpectedToken { expected, found } => {
            let span = found.span;
            Report::build(ReportKind::Error, make_report_span(span))
                .with_message(format!("unexpected token: expected {}", expected))
                .with_label(
                    Label::new(make_report_span(span))
                        .with_message(format!("found {:?}", found.kind))
                        .with_color(a),
                )
                .finish()
                .eprint((name.to_owned(), Source::from(source)))
                .unwrap();
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
