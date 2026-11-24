//! Contextual syntax for typed function failures.
//!
//! `failure`, `fails`, `fail` and `expect` are keywords only in the positions
//! below and only when a non-keyword identifier follows them, so programs that
//! use these words as names keep parsing.
use super::{is_keyword, ParseError, Parser};
use crate::ast::{Expr, FailureDecl, FailureRef};
use crate::token::{Span, TokenKind};

impl Parser {
    /// The current token is `word` and the next is a non-keyword identifier.
    pub(super) fn at_contextual(&self, word: &str) -> bool {
        self.at_kw(word)
            && self.tokens.get(self.idx + 1).is_some_and(
                |token| matches!(&token.kind, TokenKind::Ident(name) if !is_keyword(name)),
            )
    }

    /// `failure Name ;`
    pub(super) fn failure_decl(&mut self) -> Result<FailureDecl, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("failure")?;
        let name = self.upper_ident()?;
        self.expect(&TokenKind::Semi)?;
        Ok(FailureDecl {
            name,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// An optional `fails A, alias.B` clause after a function's return type.
    pub(super) fn fails_clause(&mut self) -> Result<Vec<FailureRef>, ParseError> {
        let mut failures = Vec::new();
        if !self.at_contextual("fails") {
            return Ok(failures);
        }
        self.idx += 1;
        loop {
            let start = self.peek_span().start;
            let path = self.path()?;
            failures.push(FailureRef {
                path,
                span: Span::new(start, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                return Ok(failures);
            }
        }
    }

    /// `fail Name` or `expect postfix`, when the current token starts one.
    pub(super) fn failure_expr(&mut self) -> Result<Option<Expr>, ParseError> {
        let start = self.peek_span().start;
        if self.at_contextual("fail") {
            self.idx += 1;
            let failure = self.path()?;
            return Ok(Some(Expr::Fail {
                failure,
                span: Span::new(start, self.prev_end()),
            }));
        }
        if self.at_contextual("expect") {
            self.idx += 1;
            let call = self.postfix()?;
            return Ok(Some(Expr::Expect {
                call: Box::new(call),
                span: Span::new(start, self.prev_end()),
            }));
        }
        Ok(None)
    }
}
