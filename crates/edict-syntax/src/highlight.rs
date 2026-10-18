//! Editor-facing lexical highlighting for Edict source.
//!
//! Highlighting is a tooling contract, not a semantic stage. It preserves
//! comments for editors while relying on the lexer for all non-trivia tokens.

use crate::token::{lex, LexError, Span, TokenKind};

/// Stable editor-facing token roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightRole {
    Comment,
    Identifier,
    Keyword,
    Number,
    Operator,
    Punctuation,
    String,
    TypeIdentifier,
}

/// A highlighted source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightToken {
    pub role: HighlightRole,
    pub span: Span,
}

impl HighlightToken {
    /// Return the source lexeme covered by this highlighted token.
    #[must_use]
    pub fn lexeme(self, src: &str) -> &str {
        &src[self.span.start..self.span.end]
    }
}

/// Classify lexically meaningful Edict source spans for editor adapters.
///
/// Comments are emitted even though the parser discards them as trivia.
///
/// # Errors
/// Returns the underlying lexer error for invalid Edict source.
pub fn highlight_source(src: &str) -> Result<Vec<HighlightToken>, LexError> {
    let mut highlights = comment_tokens(src);

    for token in lex(src)? {
        if token.kind == TokenKind::Eof {
            continue;
        }
        highlights.push(HighlightToken {
            role: role_for(&token.kind),
            span: token.span,
        });
    }

    highlights.sort_by_key(|token| (token.span.start, token.span.end));
    Ok(highlights)
}

fn role_for(kind: &TokenKind) -> HighlightRole {
    match kind {
        TokenKind::Ident(text) if is_highlight_keyword(text) => HighlightRole::Keyword,
        TokenKind::Ident(text) if starts_with_uppercase_ascii(text) => {
            HighlightRole::TypeIdentifier
        }
        TokenKind::Ident(_) => HighlightRole::Identifier,
        TokenKind::Int { .. } => HighlightRole::Number,
        TokenKind::Str(_) | TokenKind::Bytes(_) => HighlightRole::String,
        TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::Lt
        | TokenKind::Gt
        | TokenKind::Le
        | TokenKind::Ge
        | TokenKind::EqEq
        | TokenKind::Ne
        | TokenKind::Eq
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Bang
        | TokenKind::AmpAmp
        | TokenKind::PipePipe => HighlightRole::Operator,
        TokenKind::Semi
        | TokenKind::Colon
        | TokenKind::ColonColon
        | TokenKind::Comma
        | TokenKind::Dot
        | TokenKind::Spread
        | TokenKind::At
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::Eof => HighlightRole::Punctuation,
    }
}

fn is_highlight_keyword(text: &str) -> bool {
    matches!(
        text,
        "package"
            | "use"
            | "type"
            | "enum"
            | "variant"
            | "intent"
            | "returns"
            | "profile"
            | "implements"
            | "basis"
            | "footprint"
            | "budget"
            | "where"
            | "let"
            | "return"
            | "request"
            | "require"
            | "guarantee"
            | "assert"
            | "if"
            | "then"
            | "else"
            | "for"
            | "in"
            | "bounded"
            | "yield"
            | "match"
            | "shape"
            | "lawpack"
            | "target"
            | "core"
            | "capability"
            | "as"
            | "digest"
            | "fn"
            | "const"
            | "true"
            | "false"
    )
}

fn starts_with_uppercase_ascii(text: &str) -> bool {
    text.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
}

fn comment_tokens(src: &str) -> Vec<HighlightToken> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut pos = 0;

    while pos < bytes.len() {
        match bytes[pos] {
            b'"' => skip_string(bytes, &mut pos),
            b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                let start = pos;
                pos += 2;
                while pos < bytes.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
                tokens.push(comment(start, pos));
            }
            b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                let start = pos;
                pos += 2;
                while pos + 1 < bytes.len() && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                    pos += 1;
                }
                if pos + 1 < bytes.len() {
                    pos += 2;
                } else {
                    pos = bytes.len();
                }
                tokens.push(comment(start, pos));
            }
            _ => pos += 1,
        }
    }

    tokens
}

fn skip_string(bytes: &[u8], pos: &mut usize) {
    *pos += 1;
    while *pos < bytes.len() {
        match bytes[*pos] {
            b'\\' => {
                *pos += 1;
                if *pos < bytes.len() {
                    *pos += 1;
                }
            }
            b'"' => {
                *pos += 1;
                break;
            }
            _ => *pos += 1,
        }
    }
}

fn comment(start: usize, end: usize) -> HighlightToken {
    HighlightToken {
        role: HighlightRole::Comment,
        span: Span::new(start, end),
    }
}
