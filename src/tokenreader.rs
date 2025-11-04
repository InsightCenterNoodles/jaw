use crate::tokens::{Keyword, Span, Symbol, Token, TokenKind};

pub struct TokenReader {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token>>,
}

impl TokenReader {
    pub fn new(tokens: std::iter::Peekable<std::vec::IntoIter<Token>>) -> Self {
        Self { tokens }
    }

    pub fn current_span(&mut self) -> Span {
        self.tokens.peek().map(|x| x.span).unwrap_or_default()
    }

    pub fn make_unexpected(&self, wanted: &str, found: Token) -> std::io::Error {
        std::io::Error::other(format!("expected {wanted}, found {found:?}"))
    }

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

    pub fn demand_next(&mut self) -> std::io::Result<Token> {
        self.tokens
            .next()
            .ok_or_else(|| std::io::Error::other("expected token, found end of file"))
    }

    pub fn demand_number(&mut self) -> std::io::Result<(i64, Span)> {
        let token = self.demand_next()?;

        let is_neg = self.request_symbols(&[Symbol::Minus]).is_some();

        let TokenKind::Number(x) = token.kind else {
            return Err(self.make_unexpected("number", token));
        };

        let x: i64 = x
            .try_into()
            .map_err(|_| std::io::Error::other("internal error parsing integer"))?;

        Ok((if is_neg { -x } else { x }, token.span))
    }

    pub fn demand_identifier(&mut self) -> std::io::Result<(String, Span)> {
        let token = self.demand_next()?;
        let TokenKind::Identifier(x) = token.kind else {
            return Err(self.make_unexpected("identifier", token));
        };

        Ok((x, token.span))
    }

    pub fn demand_newline(&mut self) -> std::io::Result<Span> {
        let token = self.demand_next()?;
        match token.kind {
            TokenKind::Newline | TokenKind::EndOfFile => {}
            _ => return Err(self.make_unexpected("newline", token)),
        }

        Ok(token.span)
    }

    pub fn demand_fat_arrow(&mut self) -> std::io::Result<()> {
        let token = self.demand_next()?;
        match token.kind {
            TokenKind::FatArrow => Ok(()),
            _ => Err(self.make_unexpected("'=>", token)),
        }
    }

    pub fn demand_symbol(&mut self, sym: Symbol) -> std::io::Result<()> {
        let token = self.demand_next()?;
        let TokenKind::Symbol(x) = token.kind else {
            return Err(self.make_unexpected("symbol", token));
        };

        if x != sym {
            return Err(self.make_unexpected(&format!("symbol '{sym:?}'"), token));
        }

        Ok(())
    }

    pub fn demand_symbols(&mut self, syms: &[Symbol]) -> std::io::Result<Symbol> {
        let token = self.demand_next()?;
        let TokenKind::Symbol(x) = token.kind else {
            return Err(self.make_unexpected("symbol", token));
        };

        if !syms.contains(&x) {
            return Err(self.make_unexpected(&format!("one of '{syms:?}'"), token));
        }

        Ok(x)
    }

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
