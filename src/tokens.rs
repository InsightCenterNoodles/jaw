use std::{fmt::Display, iter::Peekable};

use thiserror::Error;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub offset: usize,
}
impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(offset {})", self.offset)
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

pub fn lex_str(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source.char_indices()).collect_tokens()
}

#[derive(Debug, Default, Clone)]
struct Char(char, Position);

impl Char {
    #[inline]
    fn is_char(&self, c: char) -> bool {
        self.0 == c
    }
}

struct Lexer<T>
where
    T: Iterator<Item = (usize, char)>,
{
    source: Peekable<T>,
    last: Char,
}

impl<T> Lexer<T>
where
    T: Iterator<Item = (usize, char)>,
    T:,
{
    fn new(source: T) -> Self {
        Self {
            source: source.peekable(),
            last: Char::default(),
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

            if ch.is_char('\n') {
                let start = self.position();
                self.bump();
                tokens.push(Token {
                    kind: TokenKind::Newline,
                    span: self.span_from(start),
                });
                continue;
            }

            if ch.0.is_ascii_digit() {
                tokens.push(self.lex_number()?);
                continue;
            }

            if is_ident_start(ch.0) {
                tokens.push(self.lex_identifier_or_keyword()?);
                continue;
            }

            if ch.is_char('"') {
                tokens.push(self.lex_string()?);
                continue;
            }

            tokens.push(self.lex_symbol()?);
        }

        Ok(tokens)
    }

    fn skip_trivia(&mut self) {
        loop {
            let mut consumed = self.skip_while(|x| matches!(x, ' ' | '\t' | '\r'));

            if self.peek_check_char('/') == Some(true) && self.peek_next_char('/') == Some(true) {
                self.bump(); // remove both slash
                self.bump();

                consumed = consumed || self.skip_while(|x| !matches!(x, '\n'));
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
            if ch.0.is_ascii_digit() {
                literal.push(ch.0);
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
            if is_ident_continue(ch.0) {
                ident.push(ch.0);
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
            match ch.0 {
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

                    match next.0 {
                        '\\' | '"' => {
                            literal.push(next.0);
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
                    literal.push(ch.0);
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

        if ch.0 == '=' && self.peek_next_char('>') == Some(true) {
            // '=' was consumed by peek_next_char; consume only the '>'
            self.bump();
            return Ok(Token {
                kind: TokenKind::FatArrow,
                span: self.span_from(start),
            });
        }

        let kind = match ch.0 {
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
                    format!("unexpected character '{}'", ch.0),
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
        self.last.1
    }

    fn span_from(&self, start: Position) -> Span {
        Span::Source(start, self.position())
    }

    fn bump(&mut self) -> Option<Char> {
        let ch = self.source.next()?;

        self.last = Char(ch.1, Position { offset: ch.0 });

        Some(self.last.clone())
    }

    fn peek_char(&mut self) -> Option<Char> {
        self.source
            .peek()
            .map(|x| Char(x.1, Position { offset: x.0 }))
    }

    fn peek_check_char(&mut self, c: char) -> Option<bool> {
        self.source.peek().map(|x| x.1 == c)
    }

    /// Consumes current char and peeks the next to see if it matches the given char
    fn peek_next_char(&mut self, check: char) -> Option<bool> {
        self.bump()?;
        self.peek_char().map(|x| x.is_char(check))
    }

    fn skip_while<F: FnMut(char) -> bool>(&mut self, mut predicate: F) -> bool {
        let mut did_skip = false;

        while let Some(x) = self.peek_char() {
            if predicate(x.0) {
                did_skip = true;
                continue;
            }
            break;
        }

        did_skip
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
