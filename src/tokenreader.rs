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
