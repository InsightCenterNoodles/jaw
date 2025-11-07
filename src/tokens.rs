use std::{fmt::Display, path::Path};

use thiserror::Error;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}
impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(line {} column {})", self.line, self.column)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    #[default]
    Builtin,
    Source(Position, Position),
}

impl Span {
    pub fn union(&self, other: &Span) -> Span {
        match (self, other) {
            (Span::Builtin, Span::Builtin) => Span::Builtin,
            (Span::Builtin, x) => *x,
            (x, Span::Builtin) => *x,
            (Span::Source(s1, _), Span::Source(_, e2)) => Self::Source(*s1, *e2),
        }
    }
}

impl Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Span::Builtin => write!(f, "(builtin)"),
            Span::Source(s, e) => write!(f, "(from {}, to {})", s, e),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Pack,
    Enum,
    Bits,
    Variant,
    Seq,
    Alias,
    Use,
    As,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Colon,
    Minus,
    Equals,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Asterisk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Keyword(Keyword),
    Identifier(String),
    Number(u64),
    StringLiteral(String),
    Symbol(Symbol),
    FatArrow,
    Newline,
    EndOfFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Error)]
pub enum LexError {
    #[error("lexer error {0} at {1:?}")]
    LexErrLoc(String, Span),

    #[error("IO error")]
    IO(#[from] std::io::Error),
}

pub fn lex_path(path: &Path) -> Result<Vec<Token>, LexError> {
    let source = std::fs::read_to_string(path)?;
    Lexer::new(&source).collect_tokens().map_err(Into::into)
}

pub fn lex_str(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).collect_tokens()
}

struct Lexer<'a> {
    source: std::borrow::Cow<'a, str>,
    cursor: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source: source.into(),
            cursor: 0,
            line: 1,
            column: 1,
        }
    }

    fn collect_tokens(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        loop {
            self.skip_trivia();

            let Some(ch) = self.peek_char() else {
                let pos = self.position();
                tokens.push(Token {
                    kind: TokenKind::EndOfFile,
                    span: Span::Source(pos, pos),
                });
                break;
            };

            if ch == '\n' {
                let start = self.position();
                self.bump();
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: self.span_from(start),
                });
                continue;
            }

            if ch.is_ascii_digit() {
                tokens.push(self.lex_number()?);
                continue;
            }

            if is_ident_start(ch) {
                tokens.push(self.lex_identifier_or_keyword()?);
                continue;
            }

            if ch == '"' {
                tokens.push(self.lex_string()?);
                continue;
            }

            tokens.push(self.lex_symbol()?);
        }

        Ok(tokens)
    }

    fn skip_trivia(&mut self) {
        loop {
            let mut consumed = false;

            while matches!(self.peek_char(), Some(' ' | '\t' | '\r')) {
                self.bump();
                consumed = true;
            }

            if self.peek_char() == Some('/') && self.peek_next_char() == Some('/') {
                self.bump();
                self.bump();
                while !matches!(self.peek_char(), None | Some('\n')) {
                    self.bump();
                }
                consumed = true;
            }

            if !consumed {
                break;
            }
        }
    }

    fn lex_number(&mut self) -> Result<Token, LexError> {
        let start = self.position();
        let mut literal = String::new();

        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                literal.push(ch);
                self.bump();
            } else {
                break;
            }
        }

        let value: u64 = literal.parse().map_err(|_| {
            LexError::LexErrLoc("invalid integer literal".to_owned(), self.span_from(start))
        })?;

        Ok(Token {
            kind: TokenKind::Number(value),
            span: self.span_from(start),
        })
    }

    fn lex_identifier_or_keyword(&mut self) -> Result<Token, LexError> {
        let start = self.position();
        let mut ident = String::new();

        while let Some(ch) = self.peek_char() {
            if is_ident_continue(ch) {
                ident.push(ch);
                self.bump();
            } else {
                break;
            }
        }

        let kind = match ident.as_str() {
            "pack" => TokenKind::Keyword(Keyword::Pack),
            "enum" => TokenKind::Keyword(Keyword::Enum),
            "bits" => TokenKind::Keyword(Keyword::Bits),
            "variant" => TokenKind::Keyword(Keyword::Variant),
            "seq" => TokenKind::Keyword(Keyword::Seq),
            "alias" => TokenKind::Keyword(Keyword::Alias),
            "use" => TokenKind::Keyword(Keyword::Use),
            "as" => TokenKind::Keyword(Keyword::As),
            _ => TokenKind::Identifier(ident),
        };

        Ok(Token {
            kind,
            span: self.span_from(start),
        })
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        let start = self.position();

        self.bump(); // opening quote
        let mut literal = String::new();

        while let Some(ch) = self.peek_char() {
            match ch {
                '"' => {
                    self.bump();
                    let span = self.span_from(start);
                    return Ok(Token {
                        kind: TokenKind::StringLiteral(literal),
                        span,
                    });
                }
                '\n' => {
                    return Err(LexError::LexErrLoc(
                        "string literal cannot span multiple lines".to_owned(),
                        self.span_from(start),
                    ));
                }
                '\\' => {
                    self.bump();
                    let Some(next) = self.peek_char() else {
                        return Err(LexError::LexErrLoc(
                            "unterminated escape sequence".to_owned(),
                            self.span_from(start),
                        ));
                    };

                    match next {
                        '\\' | '"' => {
                            literal.push(next);
                            self.bump();
                        }
                        'n' => {
                            literal.push('\n');
                            self.bump();
                        }
                        't' => {
                            literal.push('\t');
                            self.bump();
                        }
                        _ => {
                            return Err(LexError::LexErrLoc(
                                "unsupported escape sequence".into(),
                                self.span_from(start),
                            ));
                        }
                    }
                }
                _ => {
                    literal.push(ch);
                    self.bump();
                }
            }
        }

        Err(LexError::LexErrLoc(
            "unterminated string literal".into(),
            self.span_from(start),
        ))
    }

    fn lex_symbol(&mut self) -> Result<Token, LexError> {
        let start = self.position();
        let Some(ch) = self.peek_char() else {
            return Err(LexError::LexErrLoc(
                "unexpected end of input".into(),
                self.span_from(start),
            ));
        };

        if ch == '=' && self.peek_next_char() == Some('>') {
            self.bump();
            self.bump();
            return Ok(Token {
                kind: TokenKind::FatArrow,
                span: self.span_from(start),
            });
        }

        let kind = match ch {
            ':' => TokenKind::Symbol(Symbol::Colon),
            '-' => TokenKind::Symbol(Symbol::Minus),
            '=' => TokenKind::Symbol(Symbol::Equals),
            '(' => TokenKind::Symbol(Symbol::LParen),
            ')' => TokenKind::Symbol(Symbol::RParen),
            '[' => TokenKind::Symbol(Symbol::LBracket),
            ']' => TokenKind::Symbol(Symbol::RBracket),
            '{' => TokenKind::Symbol(Symbol::LBrace),
            '}' => TokenKind::Symbol(Symbol::RBrace),
            '*' => TokenKind::Symbol(Symbol::Asterisk),
            _ => {
                return Err(LexError::LexErrLoc(
                    format!("unexpected character '{}'", ch),
                    self.span_from(start),
                ));
            }
        };

        self.bump();

        Ok(Token {
            kind,
            span: self.span_from(start),
        })
    }

    fn position(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
        }
    }

    fn span_from(&self, start: Position) -> Span {
        Span::Source(start, self.position())
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek_char()?;

        self.cursor += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        Some(ch)
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.cursor..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut iter = self.source[self.cursor..].chars();
        iter.next()?;
        iter.next()
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_example_pack() {
        let source = "
pack MyPOD
- a_thing : u8
- b_thing : u64";
        let tokens = lex_str(source).expect("lexing failed");

        let iter = tokens
            .into_iter()
            .skip_while(|x| matches!(x.kind, TokenKind::Newline))
            .map(|x| x.kind);

        let truth = [
            TokenKind::Keyword(Keyword::Pack),
            TokenKind::Identifier("MyPOD".to_string()),
            TokenKind::Newline,
            TokenKind::Symbol(Symbol::Minus),
            TokenKind::Identifier("a_thing".to_string()),
            TokenKind::Symbol(Symbol::Colon),
            TokenKind::Identifier("u8".to_string()),
            TokenKind::Newline,
            TokenKind::Symbol(Symbol::Minus),
            TokenKind::Identifier("b_thing".to_string()),
            TokenKind::Symbol(Symbol::Colon),
            TokenKind::Identifier("u64".to_string()),
            TokenKind::EndOfFile,
        ];

        itertools::assert_equal(iter, truth.into_iter());
    }
}
