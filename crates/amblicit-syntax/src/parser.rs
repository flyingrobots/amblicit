//! Recursive-descent parser for the minimal-v1 Edict surface grammar.
//!
//! Produces an [`ast::Module`]. Keywords are matched contextually by identifier
//! text so they remain usable as member names after `.`.

use crate::ast::{
    BinOp, Block, BoundRef, BytesRefine, ContinueObstructedArm, Decl, DigestLockedPackageRef,
    ElseClause, EnumDecl, Expr, FieldConstraint, FieldDecl, FunctionDecl, Import, ImportKind,
    IntentClause, IntentDecl, MatchArm, Module, ObstructionArm, ObstructionHandler,
    ObstructionTarget, PackageRef, Param, RecordEntry, RequireElseArm, ScalarRefine, Stmt,
    TypeDecl, TypeExpr, TypeRef, UnOp, VariantCase, YieldBlock,
};
use crate::token::{lex, Span, Token, TokenKind};

/// Stable parser error categories used by deterministic negative tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex,
    ExpectedToken,
    ExpectedKeyword,
    ExpectedIdentifier,
    ExpectedExpression,
    InvalidInteger,
    InvalidDigest,
    InvalidVersion,
    ReservedKeyword,
    UnsupportedSyntax,
    InvalidName,
    EmptyEnum,
    EmptyObstructionMap,
    EmptyMatch,
    MissingRequiredField,
    DuplicateField,
    NonCallEffect,
    NonCallExternalActionOperation,
    ReturnInYieldBlock,
    InvalidTypeCall,
    NestingTooDeep,
}

impl ParseErrorKind {
    /// Stable wire identifier for this category.
    ///
    /// This is the string emitted as the `kind` field of the public CLI
    /// `edict.cli.diagnostic/v1` contract. It is defined by an explicit,
    /// exhaustive match rather than `Debug`, so the wire contract cannot
    /// silently change when a variant is renamed, and adding a variant forces
    /// a compile error here until a stable code is assigned.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ParseErrorKind::Lex => "Lex",
            ParseErrorKind::ExpectedToken => "ExpectedToken",
            ParseErrorKind::ExpectedKeyword => "ExpectedKeyword",
            ParseErrorKind::ExpectedIdentifier => "ExpectedIdentifier",
            ParseErrorKind::ExpectedExpression => "ExpectedExpression",
            ParseErrorKind::InvalidInteger => "InvalidInteger",
            ParseErrorKind::InvalidDigest => "InvalidDigest",
            ParseErrorKind::InvalidVersion => "InvalidVersion",
            ParseErrorKind::ReservedKeyword => "ReservedKeyword",
            ParseErrorKind::UnsupportedSyntax => "UnsupportedSyntax",
            ParseErrorKind::InvalidName => "InvalidName",
            ParseErrorKind::EmptyEnum => "EmptyEnum",
            ParseErrorKind::EmptyObstructionMap => "EmptyObstructionMap",
            ParseErrorKind::EmptyMatch => "EmptyMatch",
            ParseErrorKind::MissingRequiredField => "MissingRequiredField",
            ParseErrorKind::DuplicateField => "DuplicateField",
            ParseErrorKind::NonCallEffect => "NonCallEffect",
            ParseErrorKind::NonCallExternalActionOperation => "NonCallExternalActionOperation",
            ParseErrorKind::ReturnInYieldBlock => "ReturnInYieldBlock",
            ParseErrorKind::InvalidTypeCall => "InvalidTypeCall",
            ParseErrorKind::NestingTooDeep => "NestingTooDeep",
        }
    }
}

/// The deepest source nesting the parser accepts.
///
/// It bounds two things: how deeply the parser recurses (parentheses, unary
/// operators, nested literals, statements and types), and the height of the
/// syntax tree it builds, including operator, field and call chains that it
/// parses with a loop. Past either bound, parsing stops with
/// [`ParseErrorKind::NestingTooDeep`] instead of exhausting the thread stack.
/// The limit sits above the Core graph depth (128), so sources between the two
/// still reach the compiler's more specific `InvalidBound` diagnostic.
pub const MAX_SOURCE_NESTING_DEPTH: usize = 256;

/// A parse failure: a stable kind, message, plus the source span where it was
/// detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Parse Edict source into a [`Module`].
///
/// # Errors
/// Returns a [`ParseError`] on the first lexing or grammar violation.
pub fn parse_module(src: &str) -> Result<Module, ParseError> {
    let tokens = lex(src).map_err(|e| ParseError {
        kind: ParseErrorKind::Lex,
        message: e.message,
        span: e.span,
    })?;
    Parser::new(tokens).module()
}

/// Structural and clause keywords that are **reserved as bare identifiers**
/// (SPEC Edict Language v1 §1510): they may not stand alone as a value or a
/// binder name. They remain legal as member names after `.` (§1511), which is
/// handled separately by [`Parser::ident`].
///
/// Deliberately *excluded* are words that are keywords only in a specific
/// position yet ordinary identifiers elsewhere:
/// - the import **kinds** `shape`/`lawpack`/`target`/`core`/`capability`, which
///   are idiomatically reused as the import alias (`use shape "…" as shape;`);
/// - the prelude value words `none` (also the `basis none` marker), `some`,
///   `len`, `hash` and the built-in type constructors, so `none<T>()` parses.
///   Boolean literal words are included because they do not introduce usable
///   identifiers in value position;
/// - `record`/`map`/`unit`/`migration`/`projection`, whose productions are not
///   yet parsed — they will join this set when their syntax lands.
pub(crate) fn is_keyword(s: &str) -> bool {
    matches!(
        s,
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
            | "request"
            | "return"
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
            | "as"
            | "digest"
            | "fn"
            | "const"
            | "true"
            | "false"
    )
}

fn is_upper_ident(s: &str) -> bool {
    s.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
}

fn is_digest_lit(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_call_expr(e: &Expr) -> bool {
    matches!(e, Expr::Call { .. })
}

fn block_contains_return(block: &Block) -> bool {
    block.stmts.iter().any(stmt_contains_return)
}

fn stmt_contains_return(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_block, els, ..
        } => {
            block_contains_return(then_block)
                || els.as_deref().is_some_and(|clause| match clause {
                    ElseClause::Block(block) => block_contains_return(block),
                    ElseClause::If(stmt) => stmt_contains_return(stmt),
                })
        }
        Stmt::For { body, .. } => block_contains_return(body),
        Stmt::Let { .. }
        | Stmt::Effect { .. }
        | Stmt::ExternalActionRequest { .. }
        | Stmt::Require { .. }
        | Stmt::Guarantee { .. }
        | Stmt::Assert { .. } => false,
    }
}

struct Parser {
    tokens: Vec<Token>,
    idx: usize,
    /// Active recursion through the parser's self-recursive productions.
    depth: usize,
    /// Tallest syntax tree recorded in the current [`Parser::measured`] frame.
    height: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            idx: 0,
            depth: 0,
            height: 0,
        }
    }

    // --- nesting limits (MAX_SOURCE_NESTING_DEPTH) ---

    fn nesting_too_deep<T>(&self) -> Result<T, ParseError> {
        self.err_kind(
            ParseErrorKind::NestingTooDeep,
            format!("source nesting exceeds the limit of {MAX_SOURCE_NESTING_DEPTH} levels"),
        )
    }

    /// Run a self-recursive production one recursion level deeper, refusing
    /// once the parser is already at the limit.
    fn guarded<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if self.depth >= MAX_SOURCE_NESTING_DEPTH {
            return self.nesting_too_deep();
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    /// Parse in a fresh height frame and return the height of the tallest
    /// tree recorded inside it (0 for leaves).
    fn measured<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<(T, usize), ParseError> {
        let outer = std::mem::replace(&mut self.height, 0);
        let result = parse(self);
        let height = std::mem::replace(&mut self.height, outer);
        result.map(|value| (value, height))
    }

    /// Record a tree of `height` in the current frame, refusing past the limit.
    fn record_height(&mut self, height: usize) -> Result<(), ParseError> {
        if height > MAX_SOURCE_NESTING_DEPTH {
            return self.nesting_too_deep();
        }
        self.height = self.height.max(height);
        Ok(())
    }

    /// Parse a node whose children are all parsed by `parse`: one level taller
    /// than its tallest child.
    fn composite<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        let (value, height) = self.measured(parse)?;
        self.record_height(height + 1)?;
        Ok(value)
    }

    // --- cursor helpers ---

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.idx].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.idx].span
    }

    fn next_is(&self, k: &TokenKind) -> bool {
        self.tokens
            .get(self.idx + 1)
            .is_some_and(|token| &token.kind == k)
    }

    fn next_is_kw(&self, kw: &str) -> bool {
        self.tokens
            .get(self.idx + 1)
            .is_some_and(|token| matches!(&token.kind, TokenKind::Ident(s) if s == kw))
    }

    fn prev_end(&self) -> usize {
        if self.idx == 0 {
            0
        } else {
            self.tokens[self.idx - 1].span.end
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, ParseError> {
        self.err_kind(ParseErrorKind::ExpectedToken, message)
    }

    fn err_kind<T>(
        &self,
        kind: ParseErrorKind,
        message: impl Into<String>,
    ) -> Result<T, ParseError> {
        Err(ParseError {
            kind,
            message: message.into(),
            span: self.peek_span(),
        })
    }

    fn err_at<T>(
        kind: ParseErrorKind,
        message: impl Into<String>,
        span: Span,
    ) -> Result<T, ParseError> {
        Err(ParseError {
            kind,
            message: message.into(),
            span,
        })
    }

    /// Match a punctuation token (no payload) and consume it on success.
    fn eat(&mut self, k: &TokenKind) -> bool {
        if self.peek() == k {
            self.idx += 1;
            true
        } else {
            false
        }
    }

    /// Fallibly match and consume one exact token kind.
    ///
    /// This is the parser's graceful token-matching combinator, not
    /// [`Result::expect`]. Mismatches are returned as structured
    /// [`ParseErrorKind::ExpectedToken`] diagnostics and never panic.
    fn expect(&mut self, k: &TokenKind) -> Result<(), ParseError> {
        if self.eat(k) {
            Ok(())
        } else {
            self.err(format!("expected {k:?}, found {:?}", self.peek()))
        }
    }

    fn at_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), TokenKind::Ident(s) if s == kw)
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.at_kw(kw) {
            self.idx += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: &str) -> Result<(), ParseError> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            self.err_kind(
                ParseErrorKind::ExpectedKeyword,
                format!("expected keyword `{kw}`, found {:?}", self.peek()),
            )
        }
    }

    /// Read any bare identifier (keywords are valid here, e.g. member names).
    fn ident(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            TokenKind::Ident(s) => {
                self.idx += 1;
                Ok(s)
            }
            other => self.err_kind(
                ParseErrorKind::ExpectedIdentifier,
                format!("expected identifier, found {other:?}"),
            ),
        }
    }

    fn non_keyword_ident(&mut self) -> Result<String, ParseError> {
        let span = self.peek_span();
        let name = self.ident()?;
        if is_keyword(&name) {
            return Self::err_at(
                ParseErrorKind::ReservedKeyword,
                format!("keyword `{name}` is reserved and cannot be used here"),
                span,
            );
        }
        Ok(name)
    }

    fn upper_ident(&mut self) -> Result<String, ParseError> {
        let span = self.peek_span();
        let name = self.ident()?;
        if !is_upper_ident(&name) {
            return Self::err_at(
                ParseErrorKind::InvalidName,
                format!("expected upper-ident, found `{name}`"),
                span,
            );
        }
        Ok(name)
    }

    /// Read a *binder* identifier — a name being introduced (a `let`/parameter
    /// name). Unlike [`Self::ident`], a reserved keyword is rejected here, since
    /// a binder is a bare identifier (SPEC §1510).
    fn binder(&mut self) -> Result<String, ParseError> {
        self.non_keyword_ident()
    }

    fn string(&mut self) -> Result<String, ParseError> {
        match self.peek().clone() {
            TokenKind::Str(s) => {
                self.idx += 1;
                Ok(s)
            }
            other => self.err(format!("expected string literal, found {other:?}")),
        }
    }

    fn digest_lit(&mut self) -> Result<String, ParseError> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Str(s) => {
                self.idx += 1;
                if is_digest_lit(&s) {
                    Ok(s)
                } else {
                    Self::err_at(
                        ParseErrorKind::InvalidDigest,
                        "expected digest literal `sha256:` plus 64 hex characters",
                        span,
                    )
                }
            }
            other => Self::err_at(
                ParseErrorKind::ExpectedToken,
                format!("expected digest literal string, found {other:?}"),
                span,
            ),
        }
    }

    /// A dotted coordinate: `a.b.c`.
    fn path(&mut self) -> Result<Vec<String>, ParseError> {
        let mut parts = vec![self.non_keyword_ident()?];
        while *self.peek() == TokenKind::Dot {
            self.idx += 1;
            parts.push(self.ident()?);
        }
        Ok(parts)
    }

    // --- module ---

    fn module(&mut self) -> Result<Module, ParseError> {
        let package = self.package_decl()?;
        let mut imports = Vec::new();
        while self.at_kw("use") {
            imports.push(self.import()?);
        }
        let mut decls = Vec::new();
        while !self.at_eof() {
            decls.push(self.decl()?);
        }
        Ok(Module {
            package,
            imports,
            decls,
        })
    }

    fn package_ref(&mut self) -> Result<PackageRef, ParseError> {
        let start = self.peek_span().start;
        let path = self.path()?;
        self.expect(&TokenKind::At)?;
        let version = self.version()?;
        Ok(PackageRef {
            path,
            version,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// A package version: starts with a digit, then a run of digit / `.` / `-` /
    /// identifier segments that are *adjacent* (no intervening whitespace).
    /// Whitespace terminates the version (SPEC grammar `version`).
    fn version(&mut self) -> Result<String, ParseError> {
        if !matches!(self.peek(), TokenKind::Int { .. }) {
            return self.err("expected package version (must start with a digit)");
        }
        let mut s = String::new();
        let mut last_end: Option<usize> = None;
        loop {
            let span = self.peek_span();
            if last_end.is_some_and(|end| span.start != end) {
                break; // whitespace gap ends the version
            }
            match self.peek().clone() {
                TokenKind::Int { raw, suffix, .. } => {
                    s.push_str(&raw);
                    if let Some(suf) = suffix {
                        s.push_str(suf.lexeme());
                    }
                }
                TokenKind::Dot => s.push('.'),
                TokenKind::Minus => s.push('-'),
                TokenKind::Ident(t) => s.push_str(&t),
                _ => break,
            }
            last_end = Some(span.end);
            self.idx += 1;
        }
        Ok(s)
    }

    fn package_decl(&mut self) -> Result<PackageRef, ParseError> {
        self.expect_kw("package")?;
        let pr = self.package_ref()?;
        self.expect(&TokenKind::Semi)?;
        Ok(pr)
    }

    fn import(&mut self) -> Result<Import, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("use")?;
        let kind = if self.eat_kw("shape") {
            ImportKind::Shape
        } else if self.eat_kw("lawpack") {
            ImportKind::Lawpack
        } else if self.eat_kw("target") {
            ImportKind::Target
        } else if self.eat_kw("core") {
            ImportKind::Core
        } else if self.eat_kw("capability") {
            ImportKind::Capability
        } else if self.eat_kw("module") {
            ImportKind::Module
        } else {
            return self.err("expected import kind (shape|lawpack|target|core|capability|module)");
        };

        let (package, shape_path) = if kind == ImportKind::Shape {
            (None, Some(self.string()?))
        } else {
            (Some(self.package_ref()?), None)
        };

        let digest = if kind == ImportKind::Module {
            self.expect_kw("digest")?;
            Some(self.digest_lit()?)
        } else if self.eat_kw("digest") {
            Some(self.digest_lit()?)
        } else {
            None
        };

        self.expect_kw("as")?;
        let alias = self.non_keyword_ident()?;
        self.expect(&TokenKind::Semi)?;
        Ok(Import {
            kind,
            package,
            shape_path,
            digest,
            alias,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn decl(&mut self) -> Result<Decl, ParseError> {
        if self.at_kw("type") {
            Ok(Decl::Type(self.type_decl()?))
        } else if self.at_kw("enum") {
            Ok(Decl::Enum(self.enum_decl()?))
        } else if self.at_kw("intent") {
            Ok(Decl::Intent(self.intent_decl()?))
        } else if self.at_kw("fn") {
            Ok(Decl::Function(self.function_decl()?))
        } else {
            self.err(format!(
                "expected `type`, `enum`, `fn`, or `intent` declaration, found {:?}",
                self.peek()
            ))
        }
    }

    /// `enum Name { CASE, CASE, ... }` — payload-free closed case set.
    fn enum_decl(&mut self) -> Result<EnumDecl, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("enum")?;
        let name = self.upper_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut cases = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            cases.push(self.upper_ident()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        if cases.is_empty() {
            return self.err_kind(
                ParseErrorKind::EmptyEnum,
                "enum declaration must contain at least one case",
            );
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(EnumDecl {
            name,
            cases,
            span: Span::new(start, self.prev_end()),
        })
    }

    // --- types ---

    fn type_decl(&mut self) -> Result<TypeDecl, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("type")?;
        let name = self.upper_ident()?;
        let mut params = Vec::new();
        if self.eat(&TokenKind::Lt) {
            params.push(self.ident()?);
            while self.eat(&TokenKind::Comma) {
                params.push(self.ident()?);
            }
            self.expect(&TokenKind::Gt)?;
        }
        self.expect(&TokenKind::Eq)?;
        let body = if *self.peek() == TokenKind::LBrace {
            TypeExpr::Record(self.record_type()?)
        } else if self.at_kw("variant") {
            TypeExpr::Variant(self.variant_type()?)
        } else {
            TypeExpr::Ref(self.type_ref()?)
        };
        self.expect(&TokenKind::Semi)?;
        Ok(TypeDecl {
            name,
            params,
            body,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn record_type(&mut self) -> Result<Vec<FieldDecl>, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            let start = self.peek_span().start;
            let name = self.ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.type_ref()?;
            let constraints = self.field_constraints()?;
            fields.push(FieldDecl {
                name,
                ty,
                constraints,
                span: Span::new(start, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(fields)
    }

    /// `variant { Case, Case(PayloadType), ... }`.
    fn variant_type(&mut self) -> Result<Vec<VariantCase>, ParseError> {
        self.expect_kw("variant")?;
        self.expect(&TokenKind::LBrace)?;
        let mut cases = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            let start = self.peek_span().start;
            let name = self.upper_ident()?;
            let payload = if self.eat(&TokenKind::LParen) {
                let ty = self.type_ref()?;
                self.expect(&TokenKind::RParen)?;
                Some(ty)
            } else {
                None
            };
            cases.push(VariantCase {
                name,
                payload,
                span: Span::new(start, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(cases)
    }

    fn field_constraints(&mut self) -> Result<Vec<FieldConstraint>, ParseError> {
        let mut out = Vec::new();
        loop {
            if self.eat_kw("max") {
                self.expect(&TokenKind::Eq)?;
                out.push(FieldConstraint::Max(self.bound_ref()?));
            } else if self.eat_kw("min") {
                self.expect(&TokenKind::Eq)?;
                out.push(FieldConstraint::Min(self.bound_ref()?));
            } else if self.eat_kw("pattern") {
                self.expect(&TokenKind::Eq)?;
                out.push(FieldConstraint::Pattern(self.string()?));
            } else if self.eat_kw("canonical") {
                self.expect(&TokenKind::Eq)?;
                out.push(FieldConstraint::Canonical(self.ident()?));
            } else {
                return Ok(out);
            }
        }
    }

    fn bound_ref(&mut self) -> Result<BoundRef, ParseError> {
        match self.peek().clone() {
            TokenKind::Int { value, suffix, .. } => {
                let span = self.peek_span();
                self.idx += 1;
                value
                    .parse::<u64>()
                    .map(|value| BoundRef::Int { value, suffix })
                    .map_err(|_| ParseError {
                        kind: ParseErrorKind::InvalidInteger,
                        message: format!("invalid integer bound `{value}`"),
                        span,
                    })
            }
            TokenKind::Ident(_) => Ok(BoundRef::Coord(self.path()?)),
            other => self.err(format!(
                "expected bound (integer or coordinate), found {other:?}"
            )),
        }
    }

    fn type_ref(&mut self) -> Result<TypeRef, ParseError> {
        self.guarded(|parser| parser.composite(Self::type_ref_unguarded))
    }

    fn type_ref_unguarded(&mut self) -> Result<TypeRef, ParseError> {
        let name = self.ident()?;
        match name.as_str() {
            "String" => Ok(TypeRef::StringTy(self.maybe_scalar_refine()?)),
            "Bytes" => Ok(TypeRef::BytesTy(self.maybe_bytes_refine()?)),
            "Option" => {
                self.expect(&TokenKind::Lt)?;
                let inner = self.type_ref()?;
                self.expect(&TokenKind::Gt)?;
                Ok(TypeRef::Option(Box::new(inner)))
            }
            "CapabilityRef" => {
                self.expect(&TokenKind::Lt)?;
                let inner = self.type_ref()?;
                self.expect(&TokenKind::Gt)?;
                Ok(TypeRef::CapabilityRef(Box::new(inner)))
            }
            "List" => {
                self.expect(&TokenKind::Lt)?;
                let elem = self.type_ref()?;
                self.expect(&TokenKind::Comma)?;
                self.expect_kw("max")?;
                self.expect(&TokenKind::Eq)?;
                let max = self.bound_ref()?;
                self.expect(&TokenKind::Gt)?;
                Ok(TypeRef::List {
                    elem: Box::new(elem),
                    max,
                })
            }
            "Map" => {
                self.expect(&TokenKind::Lt)?;
                let key = self.type_ref()?;
                self.expect(&TokenKind::Comma)?;
                let value = self.type_ref()?;
                self.expect(&TokenKind::Comma)?;
                self.expect_kw("max")?;
                self.expect(&TokenKind::Eq)?;
                let max = self.bound_ref()?;
                self.expect(&TokenKind::Gt)?;
                Ok(TypeRef::Map {
                    key: Box::new(key),
                    value: Box::new(value),
                    max,
                })
            }
            _ => {
                // qualified name with optional generic type-args
                let mut path = vec![name];
                while *self.peek() == TokenKind::Dot {
                    self.idx += 1;
                    path.push(self.ident()?);
                }
                let mut args = Vec::new();
                if self.eat(&TokenKind::Lt) {
                    args.push(self.type_ref()?);
                    while self.eat(&TokenKind::Comma) {
                        args.push(self.type_ref()?);
                    }
                    self.expect(&TokenKind::Gt)?;
                }
                Ok(TypeRef::Named { path, args })
            }
        }
    }

    fn maybe_scalar_refine(&mut self) -> Result<Option<ScalarRefine>, ParseError> {
        if !self.eat(&TokenKind::Lt) {
            return Ok(None);
        }
        self.expect_kw("max")?;
        self.expect(&TokenKind::Eq)?;
        let max = self.bound_ref()?;
        let canonical = if self.eat(&TokenKind::Comma) {
            self.expect_kw("canonical")?;
            self.expect(&TokenKind::Eq)?;
            Some(self.ident()?)
        } else {
            None
        };
        self.expect(&TokenKind::Gt)?;
        Ok(Some(ScalarRefine { max, canonical }))
    }

    fn maybe_bytes_refine(&mut self) -> Result<Option<BytesRefine>, ParseError> {
        if !self.eat(&TokenKind::Lt) {
            return Ok(None);
        }
        let exact = if self.eat_kw("exact") {
            true
        } else {
            self.expect_kw("max")?;
            false
        };
        self.expect(&TokenKind::Eq)?;
        let max = self.bound_ref()?;
        self.expect(&TokenKind::Gt)?;
        Ok(Some(BytesRefine {
            min: exact.then(|| max.clone()),
            max,
        }))
    }

    // --- intents ---

    fn function_decl(&mut self) -> Result<FunctionDecl, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("fn")?;
        let name = self.binder()?;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != TokenKind::RParen {
            let pstart = self.peek_span().start;
            let name = self.binder()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.type_ref()?;
            params.push(Param {
                name,
                ty,
                span: Span::new(pstart, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::Arrow)?;
        let returns = self.type_ref()?;
        let body = self.block()?;
        Ok(FunctionDecl {
            name,
            params,
            returns,
            body,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn intent_decl(&mut self) -> Result<IntentDecl, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("intent")?;
        let name = self.ident()?;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != TokenKind::RParen {
            let pstart = self.peek_span().start;
            let pname = self.binder()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.type_ref()?;
            params.push(Param {
                name: pname,
                ty,
                span: Span::new(pstart, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        self.expect_kw("returns")?;
        let returns = self.type_ref()?;

        let mut clauses = Vec::new();
        loop {
            if self.at_kw("profile") {
                self.idx += 1;
                clauses.push(IntentClause::Profile(self.path()?));
            } else if self.at_kw("implements") {
                self.idx += 1;
                clauses.push(IntentClause::Implements(self.path()?));
            } else if self.at_kw("basis") {
                self.idx += 1;
                if self.eat_kw("none") {
                    clauses.push(IntentClause::Basis(None));
                } else {
                    clauses.push(IntentClause::Basis(Some(self.expr()?)));
                }
            } else if self.at_kw("footprint") {
                self.idx += 1;
                self.expect(&TokenKind::Le)?;
                clauses.push(IntentClause::Footprint(self.path()?));
            } else if self.at_kw("budget") {
                self.idx += 1;
                self.expect(&TokenKind::Le)?;
                clauses.push(IntentClause::Budget(self.path()?));
            } else if self.at_kw("where") {
                self.idx += 1;
                let mut preds = vec![self.expr()?];
                while self.eat(&TokenKind::Comma) {
                    preds.push(self.expr()?);
                }
                clauses.push(IntentClause::Where(preds));
            } else {
                break;
            }
        }

        let body = self.block()?;
        Ok(IntentDecl {
            name,
            params,
            returns,
            clauses,
            body,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn block(&mut self) -> Result<Block, ParseError> {
        let start = self.peek_span().start;
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            stmts.push(self.stmt()?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Block {
            stmts,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn stmt(&mut self) -> Result<Stmt, ParseError> {
        self.guarded(|parser| parser.composite(Self::stmt_unguarded))
    }

    fn stmt_unguarded(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span().start;
        if self.eat_kw("let") {
            let name = self.binder()?;
            let ty = if self.eat(&TokenKind::Colon) {
                Some(self.type_ref()?)
            } else {
                None
            };
            self.expect(&TokenKind::Eq)?;
            let value = self.let_rhs()?;
            let els = self.maybe_else_handler()?;
            if els.is_some() && !is_call_expr(&value) {
                return self.err_kind(
                    ParseErrorKind::NonCallEffect,
                    "`let ... else` is only valid when the right-hand side is a call",
                );
            }
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Let {
                name,
                ty,
                value,
                els,
                span: Span::new(start, self.prev_end()),
            })
        } else if self.eat_kw("request") {
            self.external_action_request_stmt(start)
        } else if self.eat_kw("return") {
            let value = self.expr()?;
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Return {
                value,
                span: Span::new(start, self.prev_end()),
            })
        } else if self.eat_kw("require") {
            let predicate = self.expr()?;
            self.expect_kw("else")?;
            let arm = self.require_else_arm()?;
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Require {
                predicate,
                arm,
                span: Span::new(start, self.prev_end()),
            })
        } else if self.eat_kw("guarantee") {
            let predicate = self.expr()?;
            let obstruction = if self.eat_kw("else") {
                Some(self.obstruction_target()?)
            } else {
                None
            };
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Guarantee {
                predicate,
                obstruction,
                span: Span::new(start, self.prev_end()),
            })
        } else if self.eat_kw("assert") {
            let predicate = self.expr()?;
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Assert {
                predicate,
                span: Span::new(start, self.prev_end()),
            })
        } else if self.at_kw("if") {
            self.if_stmt()
        } else if self.at_kw("for") {
            self.for_stmt()
        } else {
            // effect statement: an imported-effect call with optional `else`.
            let call = self.expr()?;
            if !is_call_expr(&call) {
                return self.err_kind(
                    ParseErrorKind::NonCallEffect,
                    "effect statements must be call expressions",
                );
            }
            let els = self.maybe_else_handler()?;
            self.expect(&TokenKind::Semi)?;
            Ok(Stmt::Effect {
                call,
                els,
                span: Span::new(start, self.prev_end()),
            })
        }
    }

    fn external_action_request_stmt(&mut self, start: usize) -> Result<Stmt, ParseError> {
        let name = self.binder()?;
        self.expect(&TokenKind::Colon)?;
        let request_type = self.type_ref()?;
        self.expect(&TokenKind::Eq)?;
        let operation = self.expr()?;
        if !is_call_expr(&operation) {
            return self.err_kind(
                ParseErrorKind::NonCallExternalActionOperation,
                "external-action request operation must be a call expression",
            );
        }
        self.expect_kw("input")?;
        self.expect_kw("schema")?;
        let input_schema = self.digest_locked_package_ref()?;
        self.expect_kw("settlement")?;
        self.expect_kw("schema")?;
        let settlement_schema = self.digest_locked_package_ref()?;
        self.expect_kw("authority")?;
        let authority_scope = self.expr()?;
        self.expect_kw("basis")?;
        let basis = self.expr()?;
        self.expect_kw("budget")?;
        self.expect_kw("maxSettlementBytes")?;
        let max_settlement_bytes = self.expr()?;
        self.expect_kw("maxAttempts")?;
        let max_attempts = self.expr()?;
        self.expect_kw("reconcile")?;
        let reconciliation_law = self.digest_locked_package_ref()?;
        self.expect(&TokenKind::Semi)?;
        Ok(Stmt::ExternalActionRequest {
            name,
            request_type,
            operation,
            input_schema,
            settlement_schema,
            authority_scope: Box::new(authority_scope),
            basis: Box::new(basis),
            max_settlement_bytes: Box::new(max_settlement_bytes),
            max_attempts: Box::new(max_attempts),
            reconciliation_law,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn digest_locked_package_ref(&mut self) -> Result<DigestLockedPackageRef, ParseError> {
        let start = self.peek_span().start;
        let package = self.package_ref()?;
        self.expect_kw("digest")?;
        let digest = self.digest_lit()?;
        Ok(DigestLockedPackageRef {
            package,
            digest,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// The right-hand side of a `let`: either an ordinary expression, or the
    /// effectful branch-yield form (legal *only* here). Both start with `if`,
    /// so we disambiguate on what follows the predicate: `then` is the pure
    /// ternary; `{` is the branch-yield.
    fn let_rhs(&mut self) -> Result<Expr, ParseError> {
        if !self.at_kw("if") {
            return self.expr();
        }
        self.composite(Self::let_rhs_if)
    }

    fn let_rhs_if(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("if")?;
        let pred = self.expr()?;
        if self.at_kw("then") {
            self.ternary_tail(start, pred)
        } else if *self.peek() == TokenKind::LBrace {
            let then_block = self.yield_block()?;
            self.expect_kw("else")?;
            let else_block = self.yield_block()?;
            Ok(Expr::IfYield {
                pred: Box::new(pred),
                then_block,
                else_block,
                span: Span::new(start, self.prev_end()),
            })
        } else {
            self.err("expected `then` (conditional expression) or `{` (effect branch) after `if` predicate")
        }
    }

    /// An effect-yield block: `{ statement* yield expr; }`.
    fn yield_block(&mut self) -> Result<YieldBlock, ParseError> {
        let start = self.peek_span().start;
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.at_kw("yield") {
            if *self.peek() == TokenKind::RBrace {
                return self.err("effect branch block must end with `yield <expr>;`");
            }
            let stmt = self.stmt()?;
            if stmt_contains_return(&stmt) {
                return self.err_kind(
                    ParseErrorKind::ReturnInYieldBlock,
                    "`return` is not legal inside an effect-yield block",
                );
            }
            stmts.push(stmt);
        }
        self.expect_kw("yield")?;
        let value = self.expr()?;
        self.expect(&TokenKind::Semi)?;
        self.expect(&TokenKind::RBrace)?;
        Ok(YieldBlock {
            stmts,
            value: Box::new(value),
            span: Span::new(start, self.prev_end()),
        })
    }

    /// `if predicate block (else (block | if-stmt))?` control flow.
    fn if_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("if")?;
        let cond = self.expr()?;
        let then_block = self.block()?;
        let els = if self.eat_kw("else") {
            if self.at_kw("if") {
                let nested = self.guarded(|parser| parser.composite(Self::if_stmt))?;
                Some(Box::new(ElseClause::If(Box::new(nested))))
            } else {
                Some(Box::new(ElseClause::Block(self.block()?)))
            }
        } else {
            None
        };
        Ok(Stmt::If {
            cond,
            then_block,
            els,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// `for ident in expr bounded bound-ref block` — a statically bounded loop.
    fn for_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("for")?;
        let var = self.binder()?;
        self.expect_kw("in")?;
        let iter = self.expr()?;
        self.expect_kw("bounded")?;
        let bound = self.bound_ref()?;
        let body = self.block()?;
        Ok(Stmt::For {
            var,
            iter,
            bound,
            body,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// Parse an optional `else <obstruction-handler>` clause.
    fn maybe_else_handler(&mut self) -> Result<Option<ObstructionHandler>, ParseError> {
        if self.eat_kw("else") {
            Ok(Some(self.obstruction_handler()?))
        } else {
            Ok(None)
        }
    }

    fn obstruction_handler(&mut self) -> Result<ObstructionHandler, ParseError> {
        if *self.peek() == TokenKind::LBrace {
            Ok(ObstructionHandler::Map(self.obstruction_map()?))
        } else {
            Ok(ObstructionHandler::Single(self.obstruction_target()?))
        }
    }

    fn require_else_arm(&mut self) -> Result<RequireElseArm, ParseError> {
        if self.at_kw("continue") && self.next_is_kw("obstructed") {
            let start = self.peek_span().start;
            self.expect_kw("continue")?;
            self.expect_kw("obstructed")?;
            let payload_expr = self.record_literal()?;
            let Expr::Record {
                entries,
                span: payload_span,
            } = payload_expr
            else {
                unreachable!("record_literal always returns Expr::Record");
            };
            let reason = Self::require_obstructed_reason(&entries, payload_span)?;
            Ok(RequireElseArm::ContinueObstructed(ContinueObstructedArm {
                reason,
                payload: entries,
                span: Span::new(start, self.prev_end()),
            }))
        } else {
            Ok(RequireElseArm::Terminal(self.obstruction_target()?))
        }
    }

    fn require_obstructed_reason(
        entries: &[RecordEntry],
        payload_span: Span,
    ) -> Result<Expr, ParseError> {
        let mut reason = None;
        for entry in entries {
            let candidate = match entry {
                RecordEntry::Field { name, value } if name == "reason" => {
                    Some((value.clone(), expr_span(value)))
                }
                RecordEntry::Shorthand { name, span } if name == "reason" => Some((
                    Expr::Ident {
                        name: name.clone(),
                        span: *span,
                    },
                    *span,
                )),
                _ => None,
            };
            if let Some((value, span)) = candidate {
                if reason.is_some() {
                    return Self::err_at(
                        ParseErrorKind::DuplicateField,
                        "`continue obstructed` may contain only one `reason` field",
                        span,
                    );
                }
                reason = Some(value);
            }
        }
        reason.ok_or_else(|| ParseError {
            kind: ParseErrorKind::MissingRequiredField,
            message: "`continue obstructed` requires a `reason` field".to_owned(),
            span: payload_span,
        })
    }

    fn obstruction_target(&mut self) -> Result<ObstructionTarget, ParseError> {
        let start = self.peek_span().start;
        let coordinate = self.path()?;
        let payload = if self.eat(&TokenKind::LParen) {
            let e = self.expr()?;
            self.expect(&TokenKind::RParen)?;
            Some(e)
        } else {
            None
        };
        Ok(ObstructionTarget {
            coordinate,
            payload,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn obstruction_map(&mut self) -> Result<Vec<ObstructionArm>, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            let start = self.peek_span().start;
            let failure = self.non_keyword_ident()?;
            let binder = if self.eat(&TokenKind::LParen) {
                let b = self.binder()?;
                self.expect(&TokenKind::RParen)?;
                Some(b)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            let target = self.obstruction_target()?;
            arms.push(ObstructionArm {
                failure,
                binder,
                target,
                span: Span::new(start, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        if arms.is_empty() {
            return self.err_kind(
                ParseErrorKind::EmptyObstructionMap,
                "obstruction map must contain at least one arm",
            );
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(arms)
    }

    // --- expressions (precedence climbing) ---

    fn expr(&mut self) -> Result<Expr, ParseError> {
        // `if-expr` sits at the top of the precedence chain: a leading `if` in
        // expression position is the pure ternary (`if p then a else b`). The
        // effectful branch-yield form is *not* a general expression — it is
        // handled separately in `let_rhs`.
        self.guarded(|parser| {
            if parser.at_kw("if") {
                return parser.composite(Self::if_ternary);
            }
            parser.logic_or()
        })
    }

    /// Pure conditional expression: `if predicate then expr else expr`.
    fn if_ternary(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("if")?;
        let cond = self.expr()?;
        self.ternary_tail(start, cond)
    }

    /// Parse the `then expr else expr` tail of a ternary, given the already
    /// parsed condition and the construct's start offset. Shared by `if_ternary`
    /// and the ternary branch of `let_rhs`.
    fn ternary_tail(&mut self, start: usize, cond: Expr) -> Result<Expr, ParseError> {
        self.expect_kw("then")?;
        let then = self.expr()?;
        self.expect_kw("else")?;
        let els = self.expr()?;
        Ok(Expr::If {
            cond: Box::new(cond),
            then: Box::new(then),
            els: Box::new(els),
            span: Span::new(start, self.prev_end()),
        })
    }

    fn binop_left(
        &mut self,
        next: fn(&mut Self) -> Result<Expr, ParseError>,
        ops: &[(TokenKind, BinOp)],
    ) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        let (mut lhs, mut height) = self.measured(next)?;
        'outer: loop {
            for (tok, op) in ops {
                if self.peek() == tok {
                    self.idx += 1;
                    let (rhs, rhs_height) = self.measured(next)?;
                    // A left-associated chain is built by this loop, not by
                    // recursion, so its height is limited here.
                    height = height.max(rhs_height) + 1;
                    if height > MAX_SOURCE_NESTING_DEPTH {
                        return self.nesting_too_deep();
                    }
                    lhs = Expr::Binary {
                        op: *op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        span: Span::new(start, self.prev_end()),
                    };
                    continue 'outer;
                }
            }
            self.record_height(height)?;
            return Ok(lhs);
        }
    }

    fn logic_or(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(Self::logic_and, &[(TokenKind::PipePipe, BinOp::Or)])
    }

    fn logic_and(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(Self::equality, &[(TokenKind::AmpAmp, BinOp::And)])
    }

    fn equality(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(
            Self::relational,
            &[(TokenKind::EqEq, BinOp::Eq), (TokenKind::Ne, BinOp::Ne)],
        )
    }

    fn relational(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(
            Self::additive,
            &[
                (TokenKind::Lt, BinOp::Lt),
                (TokenKind::Le, BinOp::Le),
                (TokenKind::Gt, BinOp::Gt),
                (TokenKind::Ge, BinOp::Ge),
            ],
        )
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(
            Self::multiplicative,
            &[
                (TokenKind::Plus, BinOp::Add),
                (TokenKind::Minus, BinOp::Sub),
            ],
        )
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        self.binop_left(
            Self::unary,
            &[
                (TokenKind::Star, BinOp::Mul),
                (TokenKind::Slash, BinOp::Div),
                (TokenKind::Percent, BinOp::Rem),
            ],
        )
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        let op = match self.peek() {
            TokenKind::Bang => Some(UnOp::Not),
            TokenKind::Minus => Some(UnOp::Neg),
            _ => None,
        };
        if let Some(op) = op {
            self.idx += 1;
            let operand = self.guarded(|parser| parser.composite(Self::unary))?;
            Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
                span: Span::new(start, self.prev_end()),
            })
        } else {
            self.postfix()
        }
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        let (mut e, mut height) = self.measured(Self::primary)?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.idx += 1;
                    let field = self.ident()?;
                    height += 1;
                    e = Expr::Field {
                        base: Box::new(e),
                        field,
                        span: Span::new(start, self.prev_end()),
                    };
                }
                TokenKind::ColonColon => {
                    // Variant literal: `<qual-ident>::Case` with optional payload.
                    // The preceding expression must be a pure dotted path.
                    let ty_path = Self::expr_to_path(&e).ok_or_else(|| ParseError {
                        kind: ParseErrorKind::ExpectedExpression,
                        message: "`::` must follow a type path (a variant constructor)".into(),
                        span: self.peek_span(),
                    })?;
                    self.idx += 1; // consume `::`
                    let case = self.upper_ident()?;
                    let payload = if self.eat(&TokenKind::LParen) {
                        let (p, payload_height) = self.measured(Self::expr)?;
                        self.expect(&TokenKind::RParen)?;
                        height = height.max(payload_height);
                        Some(Box::new(p))
                    } else {
                        None
                    };
                    height += 1;
                    e = Expr::VariantLit {
                        ty_path,
                        case,
                        payload,
                        span: Span::new(start, self.prev_end()),
                    };
                }
                TokenKind::LParen => {
                    let (args, args_height) = self.measured(Self::call_args)?;
                    height = height.max(args_height) + 1;
                    e = Expr::Call {
                        callee: Box::new(e),
                        type_args: Vec::new(),
                        args,
                        span: Span::new(start, self.prev_end()),
                    };
                }
                TokenKind::Lt => {
                    // `<` is ambiguous (generics vs comparison). Only treat it as
                    // a type-call when `<type-args>` is immediately followed by
                    // `(`; otherwise backtrack and let `relational` handle `<`.
                    let (type_args, type_args_height) =
                        self.measured(|parser| Ok(parser.try_type_call_args()))?;
                    let Some(type_args) = type_args else {
                        break;
                    };
                    let (args, args_height) = self.measured(Self::call_args)?;
                    height = height.max(type_args_height).max(args_height) + 1;
                    e = Expr::Call {
                        callee: Box::new(e),
                        type_args,
                        args,
                        span: Span::new(start, self.prev_end()),
                    };
                }
                _ => break,
            }
            // Like operator chains, postfix chains are built by this loop.
            if height > MAX_SOURCE_NESTING_DEPTH {
                return self.nesting_too_deep();
            }
        }
        self.record_height(height)?;
        Ok(e)
    }

    fn call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        while *self.peek() != TokenKind::RParen {
            args.push(self.expr()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(args)
    }

    /// Flatten an `Ident`/`Field` chain into a dotted path, or `None` if the
    /// expression is anything else (used to validate a variant constructor head).
    fn expr_to_path(e: &Expr) -> Option<Vec<String>> {
        match e {
            Expr::Ident { name, .. } => Some(vec![name.clone()]),
            Expr::Field { base, field, .. } => {
                let mut path = Self::expr_to_path(base)?;
                path.push(field.clone());
                Some(path)
            }
            _ => None,
        }
    }

    /// `match scrutinee { Case (binder)? => expr, ... }` (at least one arm).
    fn match_expr(&mut self) -> Result<Expr, ParseError> {
        self.composite(Self::match_expr_unmeasured)
    }

    fn match_expr_unmeasured(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("match")?;
        let scrutinee = self.expr()?;
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            let astart = self.peek_span().start;
            let case = self.upper_ident()?;
            let binder = if self.eat(&TokenKind::LParen) {
                let b = self.binder()?;
                self.expect(&TokenKind::RParen)?;
                Some(b)
            } else {
                None
            };
            self.expect(&TokenKind::FatArrow)?;
            let body = self.expr()?;
            arms.push(MatchArm {
                case,
                binder,
                body,
                span: Span::new(astart, self.prev_end()),
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        // Check before consuming `}` so the error points at the empty body.
        if arms.is_empty() {
            return self.err_kind(
                ParseErrorKind::EmptyMatch,
                "`match` must have at least one arm",
            );
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Span::new(start, self.prev_end()),
        })
    }

    /// Try to parse `<type-args>` that is immediately followed by `(` (a
    /// type-call). On any mismatch, restore the cursor and return `None` so the
    /// caller can treat `<` as the relational operator.
    fn try_type_call_args(&mut self) -> Option<Vec<TypeRef>> {
        let save = self.idx;
        self.idx += 1; // consume '<'
        let mut args = Vec::new();
        loop {
            let Ok(t) = self.type_ref() else {
                self.idx = save;
                return None;
            };
            args.push(t);
            if self.eat(&TokenKind::Comma) {
                continue;
            }
            break;
        }
        if *self.peek() != TokenKind::Gt {
            self.idx = save;
            return None;
        }
        self.idx += 1; // consume '>'
        if *self.peek() != TokenKind::LParen
            || self.tokens[self.idx - 1].span.end != self.peek_span().start
        {
            self.idx = save;
            return None;
        }
        Some(args)
    }

    fn digest_value_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect_kw("digest")?;
        self.expect(&TokenKind::LParen)?;
        let value = self.digest_lit()?;
        self.expect(&TokenKind::RParen)?;
        Ok(Expr::Digest {
            value,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Int { value, suffix, .. } => {
                self.idx += 1;
                Ok(Expr::Int {
                    value,
                    suffix,
                    span,
                })
            }
            TokenKind::Str(value) => {
                self.idx += 1;
                Ok(Expr::Str { value, span })
            }
            TokenKind::Bytes(value) => {
                self.idx += 1;
                Ok(Expr::Bytes { value, span })
            }
            TokenKind::Ident(name) if name == "true" || name == "false" => {
                self.idx += 1;
                Ok(Expr::Bool {
                    value: name == "true",
                    span,
                })
            }
            TokenKind::Ident(name) if name == "digest" && self.next_is(&TokenKind::LParen) => {
                self.digest_value_lit()
            }
            TokenKind::Ident(kw) if kw == "match" => self.match_expr(),
            TokenKind::Ident(name) if is_keyword(&name) => self.err_kind(
                ParseErrorKind::ReservedKeyword,
                format!("keyword `{name}` is reserved and cannot be used as a bare identifier"),
            ),
            TokenKind::Ident(name) => {
                self.idx += 1;
                Ok(Expr::Ident { name, span })
            }
            TokenKind::LParen => {
                self.idx += 1;
                let e = self.expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::LBrace => self.record_literal(),
            TokenKind::LBracket => self.list_literal(),
            other => self.err(format!("expected expression, found {other:?}")),
        }
    }

    fn list_literal(&mut self) -> Result<Expr, ParseError> {
        self.composite(Self::list_literal_unmeasured)
    }

    fn list_literal_unmeasured(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect(&TokenKind::LBracket)?;
        let mut items = Vec::new();
        while *self.peek() != TokenKind::RBracket {
            items.push(self.expr()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(Expr::List {
            items,
            span: Span::new(start, self.prev_end()),
        })
    }

    fn record_literal(&mut self) -> Result<Expr, ParseError> {
        self.composite(Self::record_literal_unmeasured)
    }

    fn record_literal_unmeasured(&mut self) -> Result<Expr, ParseError> {
        let start = self.peek_span().start;
        self.expect(&TokenKind::LBrace)?;
        let mut entries = Vec::new();
        while *self.peek() != TokenKind::RBrace {
            if self.eat(&TokenKind::Spread) {
                entries.push(RecordEntry::Spread(self.expr()?));
            } else {
                let espan = self.peek_span();
                let name = self.ident()?;
                if self.eat(&TokenKind::Colon) {
                    entries.push(RecordEntry::Field {
                        name,
                        value: self.expr()?,
                    });
                } else {
                    if is_keyword(&name) {
                        return Self::err_at(
                            ParseErrorKind::ReservedKeyword,
                            format!("keyword `{name}` is reserved and cannot be used as shorthand"),
                            espan,
                        );
                    }
                    entries.push(RecordEntry::Shorthand { name, span: espan });
                }
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Record {
            entries,
            span: Span::new(start, self.prev_end()),
        })
    }
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Ident { span, .. }
        | Expr::Int { span, .. }
        | Expr::Str { span, .. }
        | Expr::Bytes { span, .. }
        | Expr::Bool { span, .. }
        | Expr::Digest { span, .. }
        | Expr::Field { span, .. }
        | Expr::Call { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Binary { span, .. }
        | Expr::Record { span, .. }
        | Expr::List { span, .. }
        | Expr::If { span, .. }
        | Expr::IfYield { span, .. }
        | Expr::VariantLit { span, .. }
        | Expr::Match { span, .. } => *span,
    }
}

#[cfg(test)]
mod parse_error_kind_codes {
    use super::ParseErrorKind;

    #[test]
    fn each_variant_has_its_stable_wire_code() {
        let cases = [
            (ParseErrorKind::Lex, "Lex"),
            (ParseErrorKind::ExpectedToken, "ExpectedToken"),
            (ParseErrorKind::ExpectedKeyword, "ExpectedKeyword"),
            (ParseErrorKind::ExpectedIdentifier, "ExpectedIdentifier"),
            (ParseErrorKind::ExpectedExpression, "ExpectedExpression"),
            (ParseErrorKind::InvalidInteger, "InvalidInteger"),
            (ParseErrorKind::InvalidDigest, "InvalidDigest"),
            (ParseErrorKind::InvalidVersion, "InvalidVersion"),
            (ParseErrorKind::ReservedKeyword, "ReservedKeyword"),
            (ParseErrorKind::UnsupportedSyntax, "UnsupportedSyntax"),
            (ParseErrorKind::InvalidName, "InvalidName"),
            (ParseErrorKind::EmptyEnum, "EmptyEnum"),
            (ParseErrorKind::EmptyObstructionMap, "EmptyObstructionMap"),
            (ParseErrorKind::EmptyMatch, "EmptyMatch"),
            (ParseErrorKind::MissingRequiredField, "MissingRequiredField"),
            (ParseErrorKind::DuplicateField, "DuplicateField"),
            (ParseErrorKind::NonCallEffect, "NonCallEffect"),
            (
                ParseErrorKind::NonCallExternalActionOperation,
                "NonCallExternalActionOperation",
            ),
            (ParseErrorKind::ReturnInYieldBlock, "ReturnInYieldBlock"),
            (ParseErrorKind::InvalidTypeCall, "InvalidTypeCall"),
            (ParseErrorKind::NestingTooDeep, "NestingTooDeep"),
        ];
        for (kind, code) in cases {
            assert_eq!(kind.code(), code, "wire code for {kind:?} must be stable");
        }
    }
}
