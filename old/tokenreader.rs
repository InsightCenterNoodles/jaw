//! Small helper around a `Peekable` token iterator used by the parser.
//!
//! This provides ergonomic methods to demand specific token kinds and
//! produce consistent `ModuleBuildError`s with spans when expectations are not met.

use crate::module::ModuleBuildError;
use crate::tokens::{Keyword, Span, Symbol, Token, TokenKind};

/// Thin wrapper over a token stream with convenience methods for parsing.
pub struct TokenReader {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token>>,
}

impl TokenReader {
    /// Construct a reader from a peekable token iterator.
    pub fn new(tokens: std::iter::Peekable<std::vec::IntoIter<Token>>) -> Self {
        Self { tokens }
    }

    /// Current span based on the next token in the stream.
    pub fn current_span(&mut self) -> Span {
        self.tokens.peek().map(|x| x.span).unwrap_or_default()
    }

    /// Helper to construct a standardized "unexpected token" error.
    pub fn make_unexpected(&self, wanted: &str, found: Token) -> ModuleBuildError {
        ModuleBuildError::UnexpectedToken {
            expected: wanted.to_string(),
            found,
        }
    }

    /// Advance to the next keyword token, skipping newlines. Returns `None` if
    /// a non-newline, non-keyword token is encountered first or the stream ends.
    pub fn scan_to_next_kw(&mut self) -> Option<(Keyword, Span)> {
        loop {
            let next = self.tokens.next()?;
            match next.kind {
                TokenKind::Keyword(kw) => {
                    return Some((kw, next.span));
                }
                TokenKind::Newline => {
                    continue;
                }
                _ => return None,
            }
        }
    }

    /// Take the next token or return `UnexpectedEOF`.
    pub fn demand_next(&mut self) -> Result<Token, ModuleBuildError> {
        self.tokens.next().ok_or(ModuleBuildError::UnexpectedEOF)
    }

    /// Parse an optional `-` followed by a number token and return the signed value
    /// and the covering span. Fails if the next token is not a number.
    pub fn demand_number(&mut self) -> Result<(i64, Span), ModuleBuildError> {
        // Optionally consume a leading minus and then a number token.
        let mut is_neg = false;
        let mut start_span: Option<Span> = None;

        if let Some(tok) = self.tokens.peek() {
            if let TokenKind::Symbol(Symbol::Minus) = tok.kind {
                // consume '-'
                let t = self.tokens.next().expect("peeked token disappeared");
                start_span = Some(t.span);
                is_neg = true;
            }
        }

        let token = self.demand_next()?;
        let TokenKind::Number(x) = token.kind else {
            return Err(self.make_unexpected("number", token));
        };

        let x: i64 = x.try_into().map_err(|_| {
            ModuleBuildError::Internal("failed to parse integer literal".to_string())
        })?;

        let span = if let Some(s) = start_span {
            s.union(&token.span)
        } else {
            token.span
        };
        Ok((if is_neg { -x } else { x }, span))
    }

    /// Take and return an identifier token.
    pub fn demand_identifier(&mut self) -> Result<(String, Span), ModuleBuildError> {
        let token = self.demand_next()?;
        let TokenKind::Identifier(x) = token.kind else {
            return Err(self.make_unexpected("identifier", token));
        };

        Ok((x, token.span))
    }

    /// Demand a newline or EOF token.
    pub fn demand_newline(&mut self) -> Result<Span, ModuleBuildError> {
        let token = self.demand_next()?;
        match token.kind {
            TokenKind::Newline | TokenKind::EndOfFile => {}
            _ => return Err(self.make_unexpected("newline", token)),
        }

        Ok(token.span)
    }

    /// Demand the special `=>` fat arrow token.
    pub fn demand_fat_arrow(&mut self) -> Result<(), ModuleBuildError> {
        let token = self.demand_next()?;
        match token.kind {
            TokenKind::FatArrow => Ok(()),
            _ => Err(self.make_unexpected("'=>'", token)),
        }
    }

    /// Demand a specific symbol token.
    pub fn demand_symbol(&mut self, sym: Symbol) -> Result<(), ModuleBuildError> {
        let token = self.demand_next()?;
        let TokenKind::Symbol(x) = token.kind else {
            return Err(self.make_unexpected("symbol", token));
        };

        if x != sym {
            return Err(self.make_unexpected(&format!("symbol '{sym:?}'"), token));
        }

        Ok(())
    }

    // pub fn demand_symbols(&mut self, syms: &[Symbol]) -> Result<Symbol, ModuleBuildError> {
    //     let token = self.demand_next()?;
    //     let TokenKind::Symbol(x) = token.kind else {
    //         return Err(self.make_unexpected("symbol", token));
    //     };

    //     if !syms.contains(&x) {
    //         return Err(self.make_unexpected(&format!("one of '{syms:?}'"), token));
    //     }

    //     Ok(x)
    // }

    /// Peek and request one of a set of allowed symbols without consuming it.
    pub fn request_symbols(&mut self, syms: &[Symbol]) -> Option<Symbol> {
        let token = self.tokens.peek()?;

        let TokenKind::Symbol(x) = token.kind else {
            return None;
        };

        if !syms.contains(&x) {
            return None;
        }

        Some(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module::ModuleBuildError;
    use crate::tokens::{self, Keyword, Symbol};

    fn toks(src: &str) -> Vec<crate::tokens::Token> {
        tokens::lex_str(src).expect("lexing failed")
    }

    #[test]
    fn scan_to_next_kw_skips_newlines_and_stops_on_other_tokens() {
        let t = toks("\n\npack P\n- a : u8\n");
        let mut r = TokenReader::new(t.into_iter().peekable());

        let first = r.scan_to_next_kw();
        assert!(matches!(first, Some((Keyword::Pack, _))));

        // Next non-newline token is an identifier; should stop scanning keywords
        let second = r.scan_to_next_kw();
        assert!(second.is_none());
    }

    #[test]
    fn demand_number_handles_positive_and_negative_with_span_union() {
        // Negative number: span should cover '-' and number
        let t = toks("-123\n");
        let expect_span = t[0].span.union(&t[1].span);
        let mut r = TokenReader::new(t.into_iter().peekable());
        let (val, sp) = r.demand_number().expect("expected number");
        assert_eq!(val, -123);
        assert_eq!(sp, expect_span);

        // Positive number: span is the number token's span
        let t2 = toks("456\n");
        let expect_span2 = t2[0].span;
        let mut r2 = TokenReader::new(t2.into_iter().peekable());
        let (val2, sp2) = r2.demand_number().expect("expected number");
        assert_eq!(val2, 456);
        assert_eq!(sp2, expect_span2);
    }

    #[test]
    fn demand_number_errors_on_non_number() {
        let t = toks("foo\n");
        let mut r = TokenReader::new(t.into_iter().peekable());
        let err = r.demand_number().unwrap_err();
        assert!(matches!(
            err,
            ModuleBuildError::UnexpectedToken { expected, .. } if expected == "number"
        ));
    }

    #[test]
    fn demand_identifier_and_newline() {
        let t = toks("name\n");
        let mut r = TokenReader::new(t.into_iter().peekable());
        let (name, _sp) = r.demand_identifier().expect("identifier");
        assert_eq!(name, "name");
        r.demand_newline().expect("newline");
    }

    #[test]
    fn demand_newline_accepts_eof() {
        let t = toks("");
        let eof_span = t[0].span;
        let mut r = TokenReader::new(t.into_iter().peekable());
        let sp = r.demand_newline().expect("newline or eof");
        assert_eq!(sp, eof_span);
    }

    #[test]
    fn demand_fat_arrow_and_symbol_mismatch() {
        let t = toks("=>\n");
        let mut r = TokenReader::new(t.into_iter().peekable());
        r.demand_fat_arrow().expect("fat arrow");

        let t2 = toks(":\n");
        let mut r2 = TokenReader::new(t2.into_iter().peekable());
        let err = r2.demand_symbol(Symbol::Minus).unwrap_err();
        assert!(matches!(
            err,
            ModuleBuildError::UnexpectedToken { expected, .. } if expected.contains("Minus")
        ));
    }

    #[test]
    fn request_symbols_peeks_without_consuming() {
        let t = toks("- =>\n");
        let mut r = TokenReader::new(t.into_iter().peekable());
        assert_eq!(r.request_symbols(&[Symbol::Minus]), Some(Symbol::Minus));
        // After consuming '-', next is identifier or fat arrow depending on spacing
        r.demand_symbol(Symbol::Minus).unwrap();
        // Next token isn't a Symbol::Minus anymore
        assert_eq!(r.request_symbols(&[Symbol::Minus]), None);
        // And fat arrow is not a Symbol token
        assert_eq!(r.request_symbols(&[Symbol::LParen, Symbol::RParen]), None);
    }
}
