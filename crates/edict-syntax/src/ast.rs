//! Edict abstract syntax tree (minimal-v1 subset).
//!
//! This is source AST, not Core IR: it keeps source order and surface spelling.
//! Canonicalization, alpha-normalization, and coordinate resolution happen
//! later when lowering to Core IR (Phase 3).

use crate::token::{IntSuffix, Span};

/// A package coordinate such as `examples.hello@1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    pub path: Vec<String>,
    pub version: String,
    pub span: Span,
}

/// A source module: one package, then imports, then declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub package: PackageRef,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
}

/// What kind of artifact an import binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Shape,
    Lawpack,
    Target,
    Core,
    /// Digest-bound external operation family available only to `request`.
    Capability,
    /// Digest-bound source compilation unit, resolved before Core lowering.
    Module,
}

/// An `use <kind> ... [digest "..."] as <alias>;` import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub kind: ImportKind,
    /// Package ref for lawpack/target/core/capability imports.
    pub package: Option<PackageRef>,
    /// Quoted locator for `use shape "path" ...`.
    pub shape_path: Option<String>,
    /// Optional `digest "..."` clause (required for a locked bundle).
    pub digest: Option<String>,
    pub alias: String,
    pub span: Span,
}

/// A digest-locked package resource carried by source syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestLockedPackageRef {
    pub package: PackageRef,
    pub digest: String,
    pub span: Span,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    Type(TypeDecl),
    Enum(EnumDecl),
    Intent(IntentDecl),
    Function(FunctionDecl),
}

/// A nongeneric source-owned pure function. Statement purity is checked by the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub returns: TypeRef,
    pub body: Block,
    pub span: Span,
}

/// `enum Name { CASE, CASE, ... }` — a closed set of payload-free cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
    pub name: String,
    /// Case names, in source order. Enum cases carry no payload.
    pub cases: Vec<String>,
    pub span: Span,
}

/// `type Name = <type-expr>;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: TypeExpr,
    pub span: Span,
}

/// The right-hand side of a `type` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    Record(Vec<FieldDecl>),
    /// `variant { Case, Case(Payload), ... }` — a tagged union.
    Variant(Vec<VariantCase>),
    Ref(TypeRef),
}

/// One case of a `variant` type: a tag with an optional payload type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantCase {
    pub name: String,
    pub payload: Option<TypeRef>,
    pub span: Span,
}

/// A record field with optional field-level constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeRef,
    pub constraints: Vec<FieldConstraint>,
    pub span: Span,
}

/// A `max=`, `min=`, `pattern=`, or `canonical=` field constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldConstraint {
    Max(BoundRef),
    Min(BoundRef),
    Pattern(String),
    Canonical(String),
}

/// A literal-or-digest-locked bound (`max=128` or `max=rope.maxLeaves`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundRef {
    Int {
        value: u64,
        suffix: Option<IntSuffix>,
    },
    Coord(Vec<String>),
}

/// A type reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// `qual.ident` with optional `<type-args>`.
    Named {
        path: Vec<String>,
        args: Vec<TypeRef>,
    },
    /// `String` / `String<max=N[, canonical=x]>`.
    StringTy(Option<ScalarRefine>),
    /// `Bytes` / `Bytes<max=N>` / `Bytes<exact=N>` (no canonicalization).
    BytesTy(Option<BytesRefine>),
    Option(Box<TypeRef>),
    CapabilityRef(Box<TypeRef>),
    List {
        elem: Box<TypeRef>,
        max: BoundRef,
    },
    Map {
        key: Box<TypeRef>,
        value: Box<TypeRef>,
        max: BoundRef,
    },
}

/// A refined `String` bound: `max=` plus optional `canonical=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarRefine {
    pub max: BoundRef,
    pub canonical: Option<String>,
}

/// A refined `Bytes` interval.
///
/// `Bytes<max=N>` has no minimum. `Bytes<exact=N>` is represented as the
/// closed interval `min=N,max=N` so every later stage can preserve the exact
/// structural contract without application vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytesRefine {
    pub min: Option<BoundRef>,
    pub max: BoundRef,
}

/// `intent name(params) returns Ty <clauses> { body }`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub returns: TypeRef,
    pub clauses: Vec<IntentClause>,
    pub body: Block,
    pub span: Span,
}

/// A `name: Type` parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

/// An intent clause (order-independent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentClause {
    Profile(Vec<String>),
    Implements(Vec<String>),
    /// `basis none` or `basis <expr>`.
    Basis(Option<Expr>),
    Footprint(Vec<String>),
    Budget(Vec<String>),
    Where(Vec<Expr>),
}

/// A `{ ... }` statement block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

/// A statement inside an intent body (minimal-v1 subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<TypeRef>,
        value: Expr,
        /// Optional effect-failure mapping when the rhs is an imported effect.
        els: Option<ObstructionHandler>,
        span: Span,
    },
    /// A bare imported-effect call statement, e.g. `ref.replace(x) else Obs;`.
    Effect {
        call: Expr,
        els: Option<ObstructionHandler>,
        span: Span,
    },
    /// Construct one typed external-action request value without performing it.
    ExternalActionRequest {
        name: String,
        request_type: TypeRef,
        operation: Expr,
        input_schema: DigestLockedPackageRef,
        settlement_schema: DigestLockedPackageRef,
        authority_scope: Box<Expr>,
        basis: Box<Expr>,
        max_settlement_bytes: Box<Expr>,
        max_attempts: Box<Expr>,
        reconciliation_law: DigestLockedPackageRef,
        span: Span,
    },
    /// `require predicate else <arm>;` (always carries `else`).
    Require {
        predicate: Expr,
        arm: RequireElseArm,
        span: Span,
    },
    /// `guarantee predicate [else Obstruction];` (`else` for precommit checks).
    Guarantee {
        predicate: Expr,
        obstruction: Option<ObstructionTarget>,
        span: Span,
    },
    /// `assert predicate;` (proof-only, never carries `else`).
    Assert {
        predicate: Expr,
        span: Span,
    },
    /// `if cond { ... } [else { ... } | else if ...]` control flow.
    If {
        cond: Expr,
        then_block: Block,
        els: Option<Box<ElseClause>>,
        span: Span,
    },
    /// `for var in iter bounded <bound> { ... }` — a statically bounded loop.
    For {
        var: String,
        iter: Expr,
        /// The mandatory cardinality bound (`EDICT-LANG-BOUNDS`): every loop
        /// must carry a provable maximum iteration count.
        bound: BoundRef,
        body: Block,
        span: Span,
    },
    Return {
        value: Expr,
        span: Span,
    },
}

/// The `else` arm of an `if` statement: a block, or a chained `else if`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElseClause {
    Block(Block),
    /// Always a [`Stmt::If`].
    If(Box<Stmt>),
}

/// The `else` arm of a `require` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequireElseArm {
    /// Terminal obstruction: `else rope.StaleBase`.
    Terminal(ObstructionTarget),
    /// First-class source syntax reserved for obstruction-strand preservation:
    /// `else continue obstructed { reason: ..., ... }`.
    ContinueObstructed(ContinueObstructedArm),
}

/// Source-only obstruction-strand continuation arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinueObstructedArm {
    pub reason: Expr,
    pub payload: Vec<RecordEntry>,
    pub span: Span,
}

/// How an effect's failures map to typed domain obstructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObstructionHandler {
    /// Single-obstruction shorthand: `else rope.Missing` / `else rope.X({...})`.
    Single(ObstructionTarget),
    /// Full mapping: `else { mismatch(f) => rope.X({...}), ... }`.
    Map(Vec<ObstructionArm>),
}

/// One arm of an obstruction map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObstructionArm {
    /// The low-level failure coordinate (map key), e.g. `mismatch`.
    pub failure: String,
    /// Optional binder for the low-level failure value: `mismatch(f) => ...`.
    pub binder: Option<String>,
    pub target: ObstructionTarget,
    pub span: Span,
}

/// A domain obstruction constructor: a coordinate plus optional payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObstructionTarget {
    pub coordinate: Vec<String>,
    /// `rope.X({ expected: ..., observed: ... })` payload expression, if any.
    pub payload: Option<Expr>,
    pub span: Span,
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// A unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

/// An expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A single bare identifier (local/parameter).
    Ident {
        name: String,
        span: Span,
    },
    Int {
        value: String,
        suffix: Option<IntSuffix>,
        span: Span,
    },
    Str {
        value: String,
        span: Span,
    },
    Bytes {
        value: Vec<u8>,
        span: Span,
    },
    Bool {
        value: bool,
        span: Span,
    },
    /// `digest("sha256:...")` value literal.
    Digest {
        value: String,
        span: Span,
    },
    /// `expr.field` member access.
    Field {
        base: Box<Expr>,
        field: String,
        span: Span,
    },
    /// A call: `callee(args)` or `callee<TypeArgs>(args)`.
    Call {
        callee: Box<Expr>,
        type_args: Vec<TypeRef>,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    /// A record literal `{ a: x, b, ...spread }`.
    Record {
        entries: Vec<RecordEntry>,
        span: Span,
    },
    /// Pure ternary: `if cond then a else b`.
    If {
        cond: Box<Expr>,
        then: Box<Expr>,
        els: Box<Expr>,
        span: Span,
    },
    /// Branch-yield conditional effect (legal only as a `let` rhs):
    /// `if pred { ...; yield a; } else { ...; yield b; }`.
    IfYield {
        pred: Box<Expr>,
        then_block: YieldBlock,
        else_block: YieldBlock,
        span: Span,
    },
    /// A variant constructor: `qual.Type::Case` or `qual.Type::Case(payload)`.
    VariantLit {
        ty_path: Vec<String>,
        case: String,
        payload: Option<Box<Expr>>,
        span: Span,
    },
    /// `match scrutinee { Case (binder)? => expr, ... }`.
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
}

/// One arm of a `match` expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The bare case name (enum case or variant tag).
    pub case: String,
    /// Optional payload binder for variant cases: `Case(binder) => ...`.
    pub binder: Option<String>,
    pub body: Expr,
    pub span: Span,
}

/// A `{ stmt* yield expr; }` block: statements followed by a single `yield`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YieldBlock {
    pub stmts: Vec<Stmt>,
    pub value: Box<Expr>,
    pub span: Span,
}

/// One entry in a record literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordEntry {
    /// `name: value`
    Field { name: String, value: Expr },
    /// `name` (shorthand for `name: name`).
    Shorthand { name: String, span: Span },
    /// `...expr`
    Spread(Expr),
}
