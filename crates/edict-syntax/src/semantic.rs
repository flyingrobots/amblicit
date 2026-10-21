//! Source/surface semantic validation for checks that do not require import
//! resolution, resolved typing, target/lawpack facts, or Core IR.

use std::collections::BTreeSet;

use crate::ast::{
    Block, Decl, ElseClause, Expr, IntentClause, IntentDecl, Module, ObstructionHandler,
    ObstructionTarget, RecordEntry, RequireElseArm, Stmt, TypeExpr, TypeRef, YieldBlock,
};
use crate::token::Span;

/// Stable semantic validation error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticErrorKind {
    UnboundedScalar,
    MissingOperationMode,
    MissingBudget,
    MissingBasis,
    DuplicateIntentClause,
    DuplicateName,
    ShadowedName,
}

impl SemanticErrorKind {
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
            SemanticErrorKind::UnboundedScalar => "UnboundedScalar",
            SemanticErrorKind::MissingOperationMode => "MissingOperationMode",
            SemanticErrorKind::MissingBudget => "MissingBudget",
            SemanticErrorKind::MissingBasis => "MissingBasis",
            SemanticErrorKind::DuplicateIntentClause => "DuplicateIntentClause",
            SemanticErrorKind::DuplicateName => "DuplicateName",
            SemanticErrorKind::ShadowedName => "ShadowedName",
        }
    }
}

/// A semantic validation failure with a stable kind and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    pub kind: SemanticErrorKind,
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "semantic error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for SemanticError {}

/// Validate the source/surface stage.
///
/// This stage is intentionally context-free over the parsed source AST. It does
/// not resolve imports or names, infer contextual types, prove loop cardinality,
/// inspect target/lawpack failure facts, lower to Core IR, or canonicalize.
///
/// # Errors
/// Returns all semantic errors found by a deterministic source-AST traversal.
/// Exact ordering is not a public contract for this first validation slice.
pub fn validate_surface(module: &Module) -> Result<(), Vec<SemanticError>> {
    let mut errors = Vec::new();
    let module_names = collect_module_names(module, &mut errors);
    let protected_names = protected_names(&module_names);

    for decl in &module.decls {
        match decl {
            Decl::Type(decl) => validate_type_expr(&decl.body, decl.span, &mut errors),
            Decl::Enum(_) => {}
            Decl::Function(function) => {
                if PRELUDE_NAMES.contains(&function.name.as_str()) {
                    errors.push(error(
                        SemanticErrorKind::ShadowedName,
                        "function shadows a prelude name",
                        function.span,
                    ));
                }
                let mut names = NameEnv::new(protected_names.clone());
                names.push_scope();
                for param in &function.params {
                    names.bind(&param.name, param.span, &mut errors);
                    validate_type_ref(&param.ty, param.span, &mut errors);
                }
                validate_type_ref(&function.returns, function.span, &mut errors);
                validate_block(&function.body, &mut names, &mut errors);
            }
            Decl::Intent(intent) => {
                let mut names = NameEnv::new(protected_names.clone());
                validate_intent(intent, &mut names, &mut errors);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Compatibility alias for the current source/surface validation stage.
///
/// New code should call [`validate_surface`] to make the compiler-spine boundary
/// explicit. This alias remains so existing Phase 2 callers keep compiling.
///
/// # Errors
/// Returns the same errors as [`validate_surface`].
pub fn validate_module(module: &Module) -> Result<(), Vec<SemanticError>> {
    validate_surface(module)
}

fn collect_module_names(module: &Module, errors: &mut Vec<SemanticError>) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for import in &module.imports {
        record_module_name(&mut names, &import.alias, import.span, errors);
    }
    for decl in &module.decls {
        match decl {
            Decl::Type(decl) => record_module_name(&mut names, &decl.name, decl.span, errors),
            Decl::Enum(decl) => record_module_name(&mut names, &decl.name, decl.span, errors),
            Decl::Function(decl) => record_module_name(&mut names, &decl.name, decl.span, errors),
            Decl::Intent(decl) => record_module_name(&mut names, &decl.name, decl.span, errors),
        }
    }
    names
}

fn record_module_name(
    names: &mut BTreeSet<String>,
    name: &str,
    span: Span,
    errors: &mut Vec<SemanticError>,
) {
    if !names.insert(name.to_owned()) {
        errors.push(error(
            SemanticErrorKind::DuplicateName,
            format!("module name `{name}` is already bound"),
            span,
        ));
    }
}

fn protected_names(module_names: &BTreeSet<String>) -> BTreeSet<String> {
    let mut names = module_names.clone();
    names.extend(PRELUDE_NAMES.iter().map(|name| (*name).to_owned()));
    names
}

const PRELUDE_NAMES: &[&str] = &[
    "Bool",
    "Bytes",
    "CapabilityRef",
    "I32",
    "I64",
    "List",
    "Map",
    "Option",
    "String",
    "U32",
    "U64",
    "digest",
    "false",
    "hash",
    "count",
    "isUtf8",
    "len",
    "none",
    "slice",
    "some",
    "true",
];

#[derive(Debug, Clone)]
struct NameEnv {
    protected: BTreeSet<String>,
    scopes: Vec<BTreeSet<String>>,
}

impl NameEnv {
    fn new(protected: BTreeSet<String>) -> Self {
        Self {
            protected,
            scopes: Vec::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(BTreeSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: &str, span: Span, errors: &mut Vec<SemanticError>) {
        if self.is_visible(name) {
            errors.push(error(
                SemanticErrorKind::ShadowedName,
                format!("name `{name}` shadows an existing binding"),
                span,
            ));
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_owned());
        }
    }

    fn is_visible(&self, name: &str) -> bool {
        self.protected.contains(name) || self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
}

fn validate_intent(intent: &IntentDecl, names: &mut NameEnv, errors: &mut Vec<SemanticError>) {
    let mut profile = None;
    let mut implements = None;
    let mut basis = None;
    let mut footprint = None;
    let mut budget = None;

    names.push_scope();
    for param in &intent.params {
        names.bind(&param.name, param.span, errors);
    }

    for clause in &intent.clauses {
        match clause {
            IntentClause::Profile(_) => {
                record_singleton("profile", intent.span, &mut profile, errors);
            }
            IntentClause::Implements(_) => {
                record_singleton("implements", intent.span, &mut implements, errors);
            }
            IntentClause::Basis(expr) => {
                record_singleton("basis", intent.span, &mut basis, errors);
                if let Some(expr) = expr {
                    validate_expr(expr, names, errors);
                }
            }
            IntentClause::Footprint(_) => {
                record_singleton("footprint", intent.span, &mut footprint, errors);
            }
            IntentClause::Budget(_) => record_singleton("budget", intent.span, &mut budget, errors),
            IntentClause::Where(predicates) => {
                for predicate in predicates {
                    validate_expr(predicate, names, errors);
                }
            }
        }
    }

    if profile.is_none() && implements.is_none() {
        errors.push(error(
            SemanticErrorKind::MissingOperationMode,
            "intent must declare at least one of `profile` or `implements`",
            intent.span,
        ));
    }
    if budget.is_none() {
        errors.push(error(
            SemanticErrorKind::MissingBudget,
            "intent must declare a `budget` clause",
            intent.span,
        ));
    }
    if basis.is_none() {
        errors.push(error(
            SemanticErrorKind::MissingBasis,
            "intent must declare a `basis` clause",
            intent.span,
        ));
    }

    for param in &intent.params {
        validate_type_ref(&param.ty, param.span, errors);
    }
    validate_type_ref(&intent.returns, intent.span, errors);
    validate_block(&intent.body, names, errors);
    names.pop_scope();
}

fn record_singleton(
    name: &str,
    span: Span,
    slot: &mut Option<()>,
    errors: &mut Vec<SemanticError>,
) {
    if slot.replace(()).is_some() {
        // Intent clauses do not currently preserve their own spans, so duplicate
        // clause diagnostics report at intent granularity.
        errors.push(error(
            SemanticErrorKind::DuplicateIntentClause,
            format!("intent contains duplicate `{name}` clause"),
            span,
        ));
    }
}

fn validate_type_expr(expr: &TypeExpr, span: Span, errors: &mut Vec<SemanticError>) {
    match expr {
        TypeExpr::Record(fields) => {
            for field in fields {
                validate_type_ref(&field.ty, field.span, errors);
            }
        }
        TypeExpr::Variant(cases) => {
            for case in cases {
                if let Some(payload) = &case.payload {
                    validate_type_ref(payload, case.span, errors);
                }
            }
        }
        TypeExpr::Ref(ty) => validate_type_ref(ty, span, errors),
    }
}

fn validate_type_ref(ty: &TypeRef, span: Span, errors: &mut Vec<SemanticError>) {
    match ty {
        TypeRef::Named { args, .. } => {
            for arg in args {
                validate_type_ref(arg, span, errors);
            }
        }
        TypeRef::StringTy(None) => errors.push(error(
            SemanticErrorKind::UnboundedScalar,
            "runtime `String` type must carry a `max=` bound",
            span,
        )),
        TypeRef::BytesTy(None) => errors.push(error(
            SemanticErrorKind::UnboundedScalar,
            "runtime `Bytes` type must carry a `max=` or `exact=` bound",
            span,
        )),
        TypeRef::StringTy(Some(_)) | TypeRef::BytesTy(Some(_)) => {}
        TypeRef::Option(inner) | TypeRef::CapabilityRef(inner) => {
            validate_type_ref(inner, span, errors);
        }
        TypeRef::List { elem, .. } => validate_type_ref(elem, span, errors),
        TypeRef::Map { key, value, .. } => {
            validate_type_ref(key, span, errors);
            validate_type_ref(value, span, errors);
        }
    }
}

fn validate_block(block: &Block, names: &mut NameEnv, errors: &mut Vec<SemanticError>) {
    names.push_scope();
    for stmt in &block.stmts {
        validate_stmt(stmt, names, errors);
    }
    names.pop_scope();
}

fn validate_stmt(stmt: &Stmt, names: &mut NameEnv, errors: &mut Vec<SemanticError>) {
    match stmt {
        Stmt::Let {
            name,
            ty,
            value,
            els,
            span,
        } => {
            names.bind(name, *span, errors);
            if let Some(ty) = ty {
                validate_type_ref(ty, *span, errors);
            }
            validate_expr(value, names, errors);
            if let Some(els) = els {
                validate_obstruction_handler(els, names, errors);
            }
        }
        Stmt::Effect { call, els, .. } => {
            validate_expr(call, names, errors);
            if let Some(els) = els {
                validate_obstruction_handler(els, names, errors);
            }
        }
        Stmt::ExternalActionRequest {
            name,
            request_type,
            operation,
            authority_scope,
            basis,
            max_settlement_bytes,
            max_attempts,
            span,
            ..
        } => {
            names.bind(name, *span, errors);
            validate_type_ref(request_type, *span, errors);
            validate_expr(operation, names, errors);
            validate_expr(authority_scope, names, errors);
            validate_expr(basis, names, errors);
            validate_expr(max_settlement_bytes, names, errors);
            validate_expr(max_attempts, names, errors);
        }
        Stmt::Require { predicate, arm, .. } => {
            validate_expr(predicate, names, errors);
            validate_require_else_arm(arm, names, errors);
        }
        Stmt::Guarantee {
            predicate,
            obstruction,
            ..
        } => {
            validate_expr(predicate, names, errors);
            if let Some(obstruction) = obstruction {
                validate_obstruction_target(obstruction, names, errors);
            }
        }
        Stmt::Assert { predicate, .. }
        | Stmt::Return {
            value: predicate, ..
        } => {
            validate_expr(predicate, names, errors);
        }
        Stmt::If {
            cond,
            then_block,
            els,
            ..
        } => {
            validate_expr(cond, names, errors);
            validate_block(then_block, names, errors);
            if let Some(els) = els.as_deref() {
                match els {
                    ElseClause::Block(block) => validate_block(block, names, errors),
                    ElseClause::If(stmt) => validate_stmt(stmt, names, errors),
                }
            }
        }
        Stmt::For {
            var,
            iter,
            body,
            span,
            ..
        } => {
            validate_expr(iter, names, errors);
            names.push_scope();
            names.bind(var, *span, errors);
            validate_block(body, names, errors);
            names.pop_scope();
        }
    }
}

fn validate_require_else_arm(
    arm: &RequireElseArm,
    names: &mut NameEnv,
    errors: &mut Vec<SemanticError>,
) {
    match arm {
        RequireElseArm::Terminal(target) => validate_obstruction_target(target, names, errors),
        RequireElseArm::ContinueObstructed(obstruction) => {
            validate_record_entries(&obstruction.payload, names, errors);
        }
    }
}

fn validate_obstruction_handler(
    handler: &ObstructionHandler,
    names: &mut NameEnv,
    errors: &mut Vec<SemanticError>,
) {
    match handler {
        ObstructionHandler::Single(target) => validate_obstruction_target(target, names, errors),
        ObstructionHandler::Map(arms) => {
            for arm in arms {
                names.push_scope();
                if let Some(binder) = &arm.binder {
                    names.bind(binder, arm.span, errors);
                }
                validate_obstruction_target(&arm.target, names, errors);
                names.pop_scope();
            }
        }
    }
}

fn validate_obstruction_target(
    target: &ObstructionTarget,
    names: &mut NameEnv,
    errors: &mut Vec<SemanticError>,
) {
    if let Some(payload) = &target.payload {
        validate_expr(payload, names, errors);
    }
}

fn validate_record_entries(
    entries: &[RecordEntry],
    names: &mut NameEnv,
    errors: &mut Vec<SemanticError>,
) {
    for entry in entries {
        match entry {
            RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                validate_expr(value, names, errors);
            }
            RecordEntry::Shorthand { .. } => {}
        }
    }
}

fn validate_expr(expr: &Expr, names: &mut NameEnv, errors: &mut Vec<SemanticError>) {
    match expr {
        Expr::Ident { .. }
        | Expr::Int { .. }
        | Expr::Str { .. }
        | Expr::Bytes { .. }
        | Expr::Bool { .. }
        | Expr::Digest { .. } => {}
        Expr::Field { base, .. } | Expr::Unary { operand: base, .. } => {
            validate_expr(base, names, errors);
        }
        Expr::Call {
            callee,
            type_args,
            args,
            span,
        } => {
            validate_expr(callee, names, errors);
            for ty in type_args {
                validate_type_ref(ty, *span, errors);
            }
            for arg in args {
                validate_expr(arg, names, errors);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            validate_expr(lhs, names, errors);
            validate_expr(rhs, names, errors);
        }
        Expr::Record { entries, .. } => {
            validate_record_entries(entries, names, errors);
        }
        Expr::If {
            cond, then, els, ..
        } => {
            validate_expr(cond, names, errors);
            validate_expr(then, names, errors);
            validate_expr(els, names, errors);
        }
        Expr::IfYield {
            pred,
            then_block,
            else_block,
            ..
        } => {
            validate_expr(pred, names, errors);
            validate_yield_block(then_block, names, errors);
            validate_yield_block(else_block, names, errors);
        }
        Expr::VariantLit { payload, .. } => {
            if let Some(payload) = payload {
                validate_expr(payload, names, errors);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            validate_expr(scrutinee, names, errors);
            for arm in arms {
                names.push_scope();
                if let Some(binder) = &arm.binder {
                    names.bind(binder, arm.span, errors);
                }
                validate_expr(&arm.body, names, errors);
                names.pop_scope();
            }
        }
    }
}

fn validate_yield_block(block: &YieldBlock, names: &mut NameEnv, errors: &mut Vec<SemanticError>) {
    names.push_scope();
    for stmt in &block.stmts {
        validate_stmt(stmt, names, errors);
    }
    validate_expr(&block.value, names, errors);
    names.pop_scope();
}

fn error(kind: SemanticErrorKind, message: impl Into<String>, span: Span) -> SemanticError {
    SemanticError {
        kind,
        message: message.into(),
        span,
    }
}

#[cfg(test)]
mod semantic_error_kind_codes {
    use super::SemanticErrorKind;

    #[test]
    fn each_variant_has_its_stable_wire_code() {
        let cases = [
            (SemanticErrorKind::UnboundedScalar, "UnboundedScalar"),
            (
                SemanticErrorKind::MissingOperationMode,
                "MissingOperationMode",
            ),
            (SemanticErrorKind::MissingBudget, "MissingBudget"),
            (SemanticErrorKind::MissingBasis, "MissingBasis"),
            (
                SemanticErrorKind::DuplicateIntentClause,
                "DuplicateIntentClause",
            ),
            (SemanticErrorKind::DuplicateName, "DuplicateName"),
            (SemanticErrorKind::ShadowedName, "ShadowedName"),
        ];
        for (kind, code) in cases {
            assert_eq!(kind.code(), code, "wire code for {kind:?} must be stable");
        }
    }
}
