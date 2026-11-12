//! Declared failure sets on source functions, `fail`, `expect`, and
//! intent-level failure maps (the language spec's Typed Function Failures).
//!
//! A function's declared set must equal what its body produces. A failing
//! call is legal only as the operand of `expect` inside a function that
//! declares its failures, or as the whole value of a mapped intent `let`.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    error, CompilerErrorKind, CompilerStage, CoreExpr, EffectOutputExpectation, Expr, FunctionDecl,
    LocalEnvironment, Span, TypeChecker, TypeShape, TypedValue,
};

/// The capability a Core module requires once any function declares failures.
const CORE_FAILURE_CAPABILITY: &str = crate::core_ir::CORE_FAILURE_CAPABILITY;

/// Failure bookkeeping for one type-checking pass.
#[derive(Debug, Clone, Default)]
pub(super) struct FailureState {
    /// A declared failure as named after linking, to its coordinate.
    coordinates: BTreeMap<String, String>,
    /// A failure coordinate to the bare name that map arms use.
    pub(super) names: BTreeMap<String, String>,
    /// A source function to its sorted declared failure coordinates.
    functions: BTreeMap<String, Vec<String>>,
    /// The declared and produced sets of the function body being checked.
    scope: Option<FunctionScope>,
    /// The span of the single failing call that `expect` or a mapped `let`
    /// handles. Every other failing call is unhandled.
    permitted_call: Option<Span>,
}

#[derive(Debug, Clone)]
struct FunctionScope {
    declared: BTreeSet<String>,
    produced: BTreeSet<String>,
}

impl TypeChecker<'_> {
    /// Record every `failure` declaration under its module-owned coordinate.
    pub(super) fn declare_source_failures(&mut self) {
        for declaration in &self.resolved.failures {
            let (coordinate, name) = self
                .resolved
                .source_failure_origins
                .get(&declaration.name)
                .map_or_else(
                    || {
                        (
                            format!("{}.{}", self.resolved.coordinate, declaration.name),
                            declaration.name.clone(),
                        )
                    },
                    |origin| (origin.coordinate.clone(), origin.name.clone()),
                );
            self.failures
                .coordinates
                .insert(declaration.name.clone(), coordinate.clone());
            self.failures.names.insert(coordinate, name);
        }
    }

    /// Resolve a function's `fails` clause into its sorted coordinate set.
    pub(super) fn declare_function_failures(&mut self, definition: &FunctionDecl) {
        let mut coordinates = BTreeSet::new();
        let mut names = BTreeMap::new();
        for reference in &definition.fails {
            let Some(coordinate) = self
                .failures
                .coordinates
                .get(&reference.path.join("."))
                .cloned()
            else {
                self.errors.push(error(
                    CompilerStage::Resolve,
                    CompilerErrorKind::UnknownFailure,
                    format!(
                        "`{}` is not a declared failure in scope",
                        reference.path.join(".")
                    ),
                    reference.span,
                ));
                continue;
            };
            let name = self.failures.names[&coordinate].clone();
            if !coordinates.insert(coordinate.clone()) {
                self.failure_shape::<()>(
                    format!("`fails` lists `{coordinate}` more than once"),
                    definition.span,
                );
            } else if let Some(other) = names.insert(name.clone(), coordinate.clone()) {
                self.failure_shape::<()>(
                    format!(
                        "`fails` lists `{other}` and `{coordinate}`, which callers cannot tell apart by the name `{name}`"
                    ),
                    definition.span,
                );
            }
        }
        if !coordinates.is_empty() {
            self.core_capabilities
                .insert(CORE_FAILURE_CAPABILITY.to_owned());
        }
        self.failures
            .functions
            .insert(definition.name.clone(), coordinates.into_iter().collect());
    }

    /// The sorted declared failure set of a source function; empty if total.
    pub(super) fn function_failures(&self, name: &str) -> &[String] {
        self.failures.functions.get(name).map_or(&[], Vec::as_slice)
    }

    pub(super) fn begin_function_failures(&mut self, name: &str) {
        self.failures.scope = Some(FunctionScope {
            declared: self.function_failures(name).iter().cloned().collect(),
            produced: BTreeSet::new(),
        });
    }

    /// Close the function's scope. With a clean body, every declared failure
    /// must have been produced somewhere in it.
    pub(super) fn end_function_failures(&mut self, definition: &FunctionDecl, clean: bool) {
        let Some(scope) = self.failures.scope.take() else {
            return;
        };
        let unraised: Vec<_> = scope
            .declared
            .difference(&scope.produced)
            .cloned()
            .collect();
        if clean && !unraised.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnraisedFailure,
                format!(
                    "function `{}` declares {} but its body never produces {}",
                    definition.name,
                    unraised.join(", "),
                    if unraised.len() == 1 { "it" } else { "them" }
                ),
                definition.span,
            ));
        }
    }

    /// `fail Name`: legal only inside a function that declares `Name`, and
    /// typed by its context.
    pub(super) fn check_fail_expr(
        &mut self,
        failure: &[String],
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let written = failure.join(".");
        let Some(coordinate) = self.failures.coordinates.get(&written).cloned() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnknownFailure,
                format!("`{written}` is not a declared failure in scope"),
                span,
            ));
            return None;
        };
        let declared = self
            .failures
            .scope
            .as_ref()
            .is_some_and(|scope| scope.declared.contains(&coordinate));
        if !declared {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UndeclaredFailure,
                format!(
                    "`fail {written}` is legal only in a function whose `fails` clause lists it"
                ),
                span,
            ));
            return None;
        }
        if let Some(scope) = &mut self.failures.scope {
            scope.produced.insert(coordinate.clone());
        }
        let Some(expected) = expected else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("`fail {written}` needs a type from its context"),
                span,
            ));
            return None;
        };
        Some(TypedValue {
            expr: CoreExpr::Fail {
                failure: coordinate,
            },
            ty: expected.clone(),
        })
    }

    /// `expect call`: propagate the callee's whole declared set.
    pub(super) fn check_expect_expr(
        &mut self,
        call: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        let Some(name) = self.source_call_name(call).map(str::to_owned) else {
            return self.failure_shape("`expect` needs a direct call to a source function", span);
        };
        let failures = self.function_failures(&name).to_vec();
        if failures.is_empty() {
            return self.failure_shape(
                format!("`expect` needs a call that declares failures; `{name}` declares none"),
                span,
            );
        }
        let Some(scope) = &mut self.failures.scope else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnhandledFailure,
                format!(
                    "an intent cannot propagate {}; bind the call with `let ... else {{ ... }}`",
                    failures.join(", ")
                ),
                span,
            ));
            return None;
        };
        let undeclared: Vec<_> = failures
            .iter()
            .filter(|failure| !scope.declared.contains(*failure))
            .cloned()
            .collect();
        if !undeclared.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UndeclaredFailure,
                format!(
                    "`expect` propagates {} that the enclosing function does not declare",
                    undeclared.join(", ")
                ),
                span,
            ));
            return None;
        }
        scope.produced.extend(failures);
        self.with_permitted_call(call, |this| {
            this.check_expr_with_effect_expectation(call, env, expectation)
        })
    }

    /// Refuse a failing source call that neither `expect` nor a mapped `let`
    /// handles.
    pub(super) fn failing_call_is_handled(&mut self, name: &str, span: Span) -> bool {
        let failures = self.function_failures(name);
        if failures.is_empty() || self.failures.permitted_call == Some(span) {
            return true;
        }
        let guidance = if self.failures.scope.is_some() {
            "propagate them with `expect` and declare them in `fails`"
        } else {
            "bind the call with `let ... else { ... }` and map each one"
        };
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnhandledFailure,
            format!("`{name}` can fail with {}; {guidance}", failures.join(", ")),
            span,
        ));
        false
    }

    /// The source function named by a direct call, if `value` is one.
    pub(super) fn source_call_name<'e>(&self, value: &'e Expr) -> Option<&'e str> {
        let Expr::Call { callee, .. } = value else {
            return None;
        };
        let Expr::Ident { name, .. } = callee.as_ref() else {
            return None;
        };
        self.function_signatures
            .contains_key(name)
            .then_some(name.as_str())
    }

    pub(super) fn with_permitted_call<T>(
        &mut self,
        call: &Expr,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.failures.permitted_call.take();
        if let Expr::Call { span, .. } = call {
            self.failures.permitted_call = Some(*span);
        }
        let result = check(self);
        self.failures.permitted_call = previous;
        result
    }

    pub(super) fn failure_shape<T>(&mut self, message: impl Into<String>, span: Span) -> Option<T> {
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnsupportedSourceShape,
            message,
            span,
        ));
        None
    }
}
