use thiserror::Error;

use super::source::SourceLocation;

#[derive(Debug, Error)]
pub enum IntermediateError {
    #[error("unsupported declaration type `{decl_type}` at {line}")]
    UnsupportedDeclaration {
        line: SourceLocation,
        decl_type: String,
    },
    #[error("{kind} declaration requires additional detail at {line}")]
    MissingDeclarationDetail {
        line: SourceLocation,
        kind: &'static str,
    },
    #[error("malformed member at {position}: {reason}")]
    MalformedMember {
        position: SourceLocation,
        reason: String,
    },
    #[error("invalid number `{value}` at {position}: {source}")]
    InvalidNumber {
        position: SourceLocation,
        value: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("duplicate default member at line {line}")]
    DuplicateDefault { line: SourceLocation },
    #[error("invalid array specification at line {line}")]
    InvalidArraySpec { line: SourceLocation },
    #[error("import declarations must appear before type definitions at {line}")]
    ImportAfterDeclaration { line: SourceLocation },
    #[error("invalid import at {line}: {reason}")]
    InvalidImport {
        line: SourceLocation,
        reason: String,
    },
    #[error("type name contains illegal character {reason}")]
    IllegalChar { line: SourceLocation, reason: char },
}
