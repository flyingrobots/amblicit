//! Hygienic source traversal. Record labels and runtime member names never rename.
use super::{
    failure, DefinitionKind, ImportBinding, SourceLinkError, SourceLinkErrorKind, UnitNames,
};
use crate::ast::{
    Block, BoundRef, Decl, DigestLockedPackageRef, ElseClause, Expr, FieldConstraint, IntentClause,
    ObstructionHandler, ObstructionTarget, Param, RecordEntry, RequireElseArm, Stmt, TypeExpr,
    TypeRef, YieldBlock,
};
use crate::token::Span;
use std::collections::{BTreeMap, BTreeSet};

type Env = BTreeMap<String, String>;
type Result<T = ()> = std::result::Result<T, SourceLinkError>;
const MAX_LINK_DEPTH: usize = 128;

pub(super) struct Rewrite<'a> {
    names: &'a UnitNames,
    units: &'a BTreeMap<String, UnitNames>,
    offset: usize,
    depth: usize,
    declaration: usize,
    local: usize,
    type_parameters: BTreeSet<String>,
}
impl<'a> Rewrite<'a> {
    pub(super) fn new(
        names: &'a UnitNames,
        units: &'a BTreeMap<String, UnitNames>,
        offset: usize,
    ) -> Self {
        Self {
            names,
            units,
            offset,
            depth: 0,
            declaration: 0,
            local: 0,
            type_parameters: BTreeSet::new(),
        }
    }
    fn error(&self, kind: SourceLinkErrorKind, span: Span, name: &str) -> SourceLinkError {
        failure(kind, &self.names.coordinate, span, name)
    }
    pub(super) fn shift(&self, span: &mut Span) -> Result {
        let original = *span;
        span.start = span.start.checked_add(self.offset).ok_or_else(|| {
            self.error(SourceLinkErrorKind::SpanOverflow, original, "source span")
        })?;
        span.end = span.end.checked_add(self.offset).ok_or_else(|| {
            self.error(SourceLinkErrorKind::SpanOverflow, original, "source span")
        })?;
        Ok(())
    }
    fn enter(&mut self, span: Span) -> Result {
        if self.depth >= MAX_LINK_DEPTH {
            return Err(self.error(SourceLinkErrorKind::DepthExceeded, span, "source structure"));
        }
        self.depth += 1;
        Ok(())
    }
    fn bind(&mut self, name: &mut String, env: &mut Env) {
        let linked = format!("{}L_{}_{}", self.names.prefix, self.declaration, self.local);
        self.local += 1;
        env.insert(name.clone(), linked.clone());
        *name = linked;
    }
    fn params(&mut self, params: &mut [Param], env: &mut Env) -> Result {
        for param in params {
            self.ty(&mut param.ty, param.span)?;
            self.bind(&mut param.name, env);
            self.shift(&mut param.span)?;
        }
        Ok(())
    }
    pub(super) fn declaration(&mut self, declaration: &mut Decl, index: usize) -> Result {
        self.declaration = index;
        self.local = 0;
        self.type_parameters.clear();
        match declaration {
            Decl::Type(definition) => {
                self.type_parameters
                    .extend(definition.params.iter().cloned());
                definition
                    .name
                    .clone_from(&self.names.definitions[&definition.name].linked);
                match &mut definition.body {
                    TypeExpr::Ref(ty) => self.ty(ty, definition.span)?,
                    TypeExpr::Record(fields) => {
                        for field in fields {
                            self.ty(&mut field.ty, field.span)?;
                            for constraint in &mut field.constraints {
                                match constraint {
                                    FieldConstraint::Max(bound) | FieldConstraint::Min(bound) => {
                                        self.bound(bound, field.span)?
                                    }
                                    FieldConstraint::Pattern(_) | FieldConstraint::Canonical(_) => {
                                    }
                                }
                            }
                            self.shift(&mut field.span)?;
                        }
                    }
                    TypeExpr::Variant(cases) => {
                        for case in cases {
                            if let Some(ty) = &mut case.payload {
                                self.ty(ty, case.span)?;
                            }
                            self.shift(&mut case.span)?;
                        }
                    }
                }
                self.shift(&mut definition.span)?;
            }
            Decl::Enum(definition) => {
                definition
                    .name
                    .clone_from(&self.names.definitions[&definition.name].linked);
                self.shift(&mut definition.span)?;
            }
            Decl::Function(definition) => {
                definition
                    .name
                    .clone_from(&self.names.definitions[&definition.name].linked);
                let mut env = Env::new();
                self.params(&mut definition.params, &mut env)?;
                self.ty(&mut definition.returns, definition.span)?;
                self.block(&mut definition.body, &mut env)?;
                self.shift(&mut definition.span)?;
            }
            Decl::Intent(definition) => {
                let mut env = Env::new();
                self.params(&mut definition.params, &mut env)?;
                self.ty(&mut definition.returns, definition.span)?;
                for clause in &mut definition.clauses {
                    match clause {
                        IntentClause::Profile(path)
                        | IntentClause::Implements(path)
                        | IntentClause::Footprint(path)
                        | IntentClause::Budget(path) => {
                            self.authority_path(path, definition.span)?
                        }
                        IntentClause::Basis(expr) => {
                            if let Some(expr) = expr {
                                self.expr(expr, &env)?;
                            }
                        }
                        IntentClause::Where(values) => {
                            for value in values {
                                self.expr(value, &env)?;
                            }
                        }
                    }
                }
                self.block(&mut definition.body, &mut env)?;
                self.shift(&mut definition.span)?;
            }
        }
        Ok(())
    }
    fn authority_path(&self, path: &mut [String], span: Span) -> Result {
        let Some(first) = path.first_mut() else {
            return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, "empty path"));
        };
        match self.names.imports.get(first) {
            Some(ImportBinding::Source(_)) => {
                Err(self.error(SourceLinkErrorKind::UnknownExport, span, first))
            }
            Some(ImportBinding::External(alias)) => {
                first.clone_from(alias);
                Ok(())
            }
            None => Ok(()),
        }
    }
    fn type_path(&self, path: &mut Vec<String>, span: Span) -> Result {
        if path.len() == 1 {
            let name = &path[0];
            if self.type_parameters.contains(name)
                || matches!(
                    name.as_str(),
                    "Bool" | "I32" | "I64" | "U32" | "U64" | "Unit"
                )
            {
                return Ok(());
            }
            if let Some(definition) = self
                .names
                .definitions
                .get(name)
                .filter(|definition| definition.kind == DefinitionKind::Type)
            {
                *path = vec![definition.linked.clone()];
                return Ok(());
            }
            return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, name));
        }
        let Some(first) = path.first() else {
            return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, "empty type"));
        };
        match self.names.imports.get(first) {
            Some(ImportBinding::Source(coordinate)) => {
                if path.len() != 2 {
                    return Err(self.error(
                        SourceLinkErrorKind::UnknownExport,
                        span,
                        &path.join("."),
                    ));
                }
                let definition = self.units[coordinate]
                    .definitions
                    .get(&path[1])
                    .filter(|definition| definition.kind == DefinitionKind::Type)
                    .ok_or_else(|| {
                        self.error(SourceLinkErrorKind::UnknownExport, span, &path.join("."))
                    })?;
                *path = vec![definition.linked.clone()];
                Ok(())
            }
            Some(ImportBinding::External(alias)) => {
                path[0].clone_from(alias);
                Ok(())
            }
            None => Err(self.error(SourceLinkErrorKind::UnresolvedName, span, first)),
        }
    }
    fn bound(&self, bound: &mut BoundRef, span: Span) -> Result {
        if let BoundRef::Coord(path) = bound {
            self.authority_path(path, span)?;
        }
        Ok(())
    }
    fn ty(&mut self, ty: &mut TypeRef, span: Span) -> Result {
        self.enter(span)?;
        let result = self.ty_inner(ty, span);
        self.depth -= 1;
        result
    }
    fn ty_inner(&mut self, ty: &mut TypeRef, span: Span) -> Result {
        match ty {
            TypeRef::Named { path, args } => {
                self.type_path(path, span)?;
                for arg in args {
                    self.ty(arg, span)?;
                }
            }
            TypeRef::StringTy(Some(refine)) => self.bound(&mut refine.max, span)?,
            TypeRef::BytesTy(Some(refine)) => {
                self.bound(&mut refine.max, span)?;
                if let Some(min) = &mut refine.min {
                    self.bound(min, span)?;
                }
            }
            TypeRef::StringTy(None) | TypeRef::BytesTy(None) => {}
            TypeRef::Option(inner) | TypeRef::CapabilityRef(inner) => self.ty(inner, span)?,
            TypeRef::List { elem, max } => {
                self.ty(elem, span)?;
                self.bound(max, span)?;
            }
            TypeRef::Map { key, value, max } => {
                self.ty(key, span)?;
                self.ty(value, span)?;
                self.bound(max, span)?;
            }
        }
        Ok(())
    }
    fn block(&mut self, block: &mut Block, env: &mut Env) -> Result {
        self.enter(block.span)?;
        let result = self.statements(&mut block.stmts, env);
        self.depth -= 1;
        result?;
        self.shift(&mut block.span)
    }
    fn statements(&mut self, statements: &mut [Stmt], env: &mut Env) -> Result {
        for statement in statements {
            self.statement(statement, env)?;
        }
        Ok(())
    }
    fn statement(&mut self, statement: &mut Stmt, env: &mut Env) -> Result {
        let span = *statement_span(statement);
        self.enter(span)?;
        let result = self.statement_inner(statement, env, span);
        self.depth -= 1;
        result?;
        self.shift(statement_span(statement))
    }
    fn statement_inner(&mut self, statement: &mut Stmt, env: &mut Env, span: Span) -> Result {
        match statement {
            Stmt::Let {
                name,
                ty,
                value,
                els,
                ..
            } => {
                if let Some(ty) = ty {
                    self.ty(ty, span)?;
                }
                self.expr(value, env)?;
                if let Some(handler) = els {
                    self.handler(handler, env)?;
                }
                self.bind(name, env);
            }
            Stmt::Effect { call, els, .. } => {
                self.expr(call, env)?;
                if let Some(handler) = els {
                    self.handler(handler, env)?;
                }
            }
            Stmt::ExternalActionRequest {
                name,
                request_type,
                operation,
                input_schema,
                settlement_schema,
                authority_scope,
                basis,
                max_settlement_bytes,
                max_attempts,
                reconciliation_law,
                ..
            } => {
                self.ty(request_type, span)?;
                self.expr(operation, env)?;
                self.resource(input_schema)?;
                self.resource(settlement_schema)?;
                for value in [authority_scope, basis, max_settlement_bytes, max_attempts] {
                    self.expr(value, env)?;
                }
                self.resource(reconciliation_law)?;
                self.bind(name, env);
            }
            Stmt::Require { predicate, arm, .. } => {
                self.expr(predicate, env)?;
                match arm {
                    RequireElseArm::Terminal(target) => self.target(target, env)?,
                    RequireElseArm::ContinueObstructed(arm) => {
                        self.expr(&mut arm.reason, env)?;
                        self.entries(&mut arm.payload, env)?;
                        self.shift(&mut arm.span)?;
                    }
                }
            }
            Stmt::Guarantee {
                predicate,
                obstruction,
                ..
            } => {
                self.expr(predicate, env)?;
                if let Some(target) = obstruction {
                    self.target(target, env)?;
                }
            }
            Stmt::Assert { predicate, .. } => self.expr(predicate, env)?,
            Stmt::Return { value, .. } => self.expr(value, env)?,
            Stmt::If {
                cond,
                then_block,
                els,
                ..
            } => {
                self.expr(cond, env)?;
                self.block(then_block, &mut env.clone())?;
                if let Some(els) = els {
                    self.else_clause(els, env)?;
                }
            }
            Stmt::For {
                var,
                iter,
                bound,
                body,
                ..
            } => {
                self.expr(iter, env)?;
                self.bound(bound, span)?;
                let mut inner = env.clone();
                self.bind(var, &mut inner);
                self.block(body, &mut inner)?;
            }
        }
        Ok(())
    }
    fn else_clause(&mut self, clause: &mut ElseClause, env: &Env) -> Result {
        match clause {
            ElseClause::Block(block) => self.block(block, &mut env.clone()),
            ElseClause::If(statement) => self.statement(statement, &mut env.clone()),
        }
    }
    fn resource(&self, resource: &mut DigestLockedPackageRef) -> Result {
        self.shift(&mut resource.span)?;
        self.shift(&mut resource.package.span)
    }
    fn target(&mut self, target: &mut ObstructionTarget, env: &Env) -> Result {
        self.authority_path(&mut target.coordinate, target.span)?;
        if let Some(payload) = &mut target.payload {
            self.expr(payload, env)?;
        }
        self.shift(&mut target.span)
    }
    fn handler(&mut self, handler: &mut ObstructionHandler, env: &Env) -> Result {
        match handler {
            ObstructionHandler::Single(target) => self.target(target, env)?,
            ObstructionHandler::Map(arms) => {
                for arm in arms {
                    let mut inner = env.clone();
                    if let Some(binder) = &mut arm.binder {
                        self.bind(binder, &mut inner);
                    }
                    self.target(&mut arm.target, &inner)?;
                    self.shift(&mut arm.span)?;
                }
            }
        }
        Ok(())
    }
    fn entries(&mut self, entries: &mut [RecordEntry], env: &Env) -> Result {
        for entry in entries {
            match entry {
                RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                    self.expr(value, env)?
                }
                RecordEntry::Shorthand { name, span } => {
                    let label = name.clone();
                    let mut value = Expr::Ident {
                        name: label.clone(),
                        span: *span,
                    };
                    self.expr(&mut value, env)?;
                    *entry = RecordEntry::Field { name: label, value };
                }
            }
        }
        Ok(())
    }
    fn yield_block(&mut self, block: &mut YieldBlock, env: &Env) -> Result {
        self.enter(block.span)?;
        let mut inner = env.clone();
        let result = self
            .statements(&mut block.stmts, &mut inner)
            .and_then(|()| self.expr(&mut block.value, &inner));
        self.depth -= 1;
        result?;
        self.shift(&mut block.span)
    }
    fn expr(&mut self, expr: &mut Expr, env: &Env) -> Result {
        let span = *expr_span(expr);
        self.enter(span)?;
        let result = match self.path(expr, span)? {
            Some(path) => self.value_path(expr, &path, env, false, span),
            None => self.expr_inner(expr, env, span),
        };
        self.depth -= 1;
        result
    }
    fn callee(&mut self, expr: &mut Expr, env: &Env) -> Result {
        let span = *expr_span(expr);
        match self.path(expr, span)? {
            Some(path) => self.value_path(expr, &path, env, true, span),
            None => self.expr(expr, env),
        }
    }
    fn expr_inner(&mut self, expr: &mut Expr, env: &Env, span: Span) -> Result {
        match expr {
            Expr::Ident { .. } => unreachable!("identifiers have a static path"),
            Expr::Field { base, .. } | Expr::Unary { operand: base, .. } => self.expr(base, env)?,
            Expr::Int { .. }
            | Expr::Str { .. }
            | Expr::Bytes { .. }
            | Expr::Bool { .. }
            | Expr::Digest { .. } => {}
            Expr::Call {
                callee,
                type_args,
                args,
                ..
            } => {
                self.callee(callee, env)?;
                for ty in type_args {
                    self.ty(ty, span)?;
                }
                for arg in args {
                    self.expr(arg, env)?;
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs, env)?;
                self.expr(rhs, env)?;
            }
            Expr::Record { entries, .. } => self.entries(entries, env)?,
            Expr::If {
                cond, then, els, ..
            } => {
                self.expr(cond, env)?;
                self.expr(then, env)?;
                self.expr(els, env)?;
            }
            Expr::IfYield {
                pred,
                then_block,
                else_block,
                ..
            } => {
                self.expr(pred, env)?;
                self.yield_block(then_block, env)?;
                self.yield_block(else_block, env)?;
            }
            Expr::VariantLit {
                ty_path, payload, ..
            } => {
                self.type_path(ty_path, span)?;
                if let Some(payload) = payload {
                    self.expr(payload, env)?;
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr(scrutinee, env)?;
                for arm in arms {
                    let mut inner = env.clone();
                    if let Some(binder) = &mut arm.binder {
                        self.bind(binder, &mut inner);
                    }
                    self.expr(&mut arm.body, &inner)?;
                    self.shift(&mut arm.span)?;
                }
            }
        }
        self.shift(expr_span(expr))
    }
    fn path(&self, expr: &Expr, span: Span) -> Result<Option<Vec<String>>> {
        let mut members = Vec::new();
        let mut cursor = expr;
        loop {
            if members.len() >= MAX_LINK_DEPTH {
                return Err(self.error(SourceLinkErrorKind::DepthExceeded, span, "member path"));
            }
            match cursor {
                Expr::Field { base, field, .. } => {
                    members.push(field.clone());
                    cursor = base;
                }
                Expr::Ident { name, .. } => {
                    members.push(name.clone());
                    members.reverse();
                    return Ok(Some(members));
                }
                _ => return Ok(None),
            }
        }
    }
    fn value_path(
        &self,
        expr: &mut Expr,
        path: &[String],
        env: &Env,
        function: bool,
        span: Span,
    ) -> Result {
        let head = &path[0];
        let rename = if let Some(local) = env.get(head) {
            Some(local.clone())
        } else if let Some(binding) = self.names.imports.get(head) {
            match binding {
                ImportBinding::Source(coordinate) => {
                    if !function || path.len() != 2 {
                        return Err(self.error(
                            SourceLinkErrorKind::UnknownExport,
                            span,
                            &path.join("."),
                        ));
                    }
                    let definition = self.units[coordinate]
                        .definitions
                        .get(&path[1])
                        .filter(|definition| definition.kind == DefinitionKind::Function)
                        .ok_or_else(|| {
                            self.error(SourceLinkErrorKind::UnknownExport, span, &path.join("."))
                        })?;
                    *expr = Expr::Ident {
                        name: definition.linked.clone(),
                        span,
                    };
                    return self.shift(expr_span(expr));
                }
                ImportBinding::External(alias) if path.len() > 1 => Some(alias.clone()),
                ImportBinding::External(_) => {
                    return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, head))
                }
            }
        } else if function && path.len() == 1 {
            if let Some(definition) = self
                .names
                .definitions
                .get(head)
                .filter(|definition| definition.kind == DefinitionKind::Function)
            {
                Some(definition.linked.clone())
            } else if matches!(
                head.as_str(),
                "len" | "slice" | "hash" | "digest" | "some" | "none"
            ) {
                None
            } else {
                return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, head));
            }
        } else if function && head == "core" && path.len() > 1 {
            None
        } else {
            return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, head));
        };
        let mut cursor = expr;
        loop {
            match cursor {
                Expr::Field { base, span, .. } => {
                    self.shift(span)?;
                    cursor = base;
                }
                Expr::Ident { name, span } => {
                    if let Some(rename) = rename {
                        *name = rename;
                    }
                    self.shift(span)?;
                    return Ok(());
                }
                _ => return Err(self.error(SourceLinkErrorKind::UnresolvedName, span, head)),
            }
        }
    }
}
fn statement_span(statement: &mut Stmt) -> &mut Span {
    match statement {
        Stmt::Let { span, .. }
        | Stmt::Effect { span, .. }
        | Stmt::ExternalActionRequest { span, .. }
        | Stmt::Require { span, .. }
        | Stmt::Guarantee { span, .. }
        | Stmt::Assert { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. } => span,
    }
}
fn expr_span(expr: &mut Expr) -> &mut Span {
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
        | Expr::If { span, .. }
        | Expr::IfYield { span, .. }
        | Expr::VariantLit { span, .. }
        | Expr::Match { span, .. } => span,
    }
}
