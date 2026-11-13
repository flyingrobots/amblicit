//! The closed Option prelude and the lexical `isSome` presence refinement.
//!
//! SPEC Edict Language v1, Standard Functions and Option Refinement
//! (`EDICT-LANG-OPTION-REFINE-001`). Each call lowers to a typed Core call
//! `core.option.<operation><T>(...)`, where `T` is the item coordinate, under
//! the required capability `core.option/v1`.
use super::{
    compatible, error, CompareOp, CompilerErrorKind, CompilerStage, CoreExpr, CorePredicate,
    CoreValue, Expr, LocalEnvironment, Span, TypeChecker, TypeKind, TypeRef, TypeShape, TypedValue,
};

/// Source prelude names owned by this module.
pub(super) const NAMES: &[&str] = &["some", "none", "isSome", "default", "unwrap"];
/// Core capability required by every `core.option.*` call.
pub(super) const CAPABILITY: &str = "core.option/v1";
const IS_SOME: &str = "core.option.isSome";

impl TypeChecker<'_> {
    pub(super) fn check_option_operation(
        &mut self,
        name: &str,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let (arity, type_arity) = match name {
            "some" => (1, 1),
            "none" => (0, 1),
            "default" => (2, 1),
            "isSome" | "unwrap" => (1, 0),
            _ => return None,
        };
        if args.len() != arity || type_args.len() > type_arity {
            return self.option_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("{name} takes {arity} value argument(s) and at most {type_arity} type argument(s)"),
                span,
            );
        }
        let declared = match type_args.first() {
            Some(reference) => Some(self.type_ref_shape(reference, span)?),
            None => None,
        };
        let expected_item = expected.and_then(option_item).cloned();
        let (operation, item, operands, result) = match name {
            "some" => {
                let target = declared.clone().or(expected_item);
                let value = self.check_expr_with_expected(&args[0], env, target.as_ref())?;
                let item = match target {
                    Some(target) if compatible(&target, &value.ty) => target,
                    Some(_) => {
                        return self.option_error(
                            CompilerErrorKind::TypeMismatch,
                            "some value does not fit the Option item type",
                            span,
                        )
                    }
                    None => value.ty,
                };
                let result = option_shape(&item)?;
                ("some", item, vec![value.expr], result)
            }
            "none" => {
                let Some(item) = declared.or(expected_item) else {
                    return self.option_error(
                        CompilerErrorKind::UnsupportedSourceShape,
                        "none requires a type argument or an expected Option type",
                        span,
                    );
                };
                let result = option_shape(&item)?;
                ("none", item, Vec::new(), result)
            }
            "isSome" => {
                let (value, item) = self.option_operand(&args[0], env, None, span)?;
                let result = TypeShape::canonical_structural(TypeKind::Bool)?;
                ("isSome", item, vec![value.expr], result)
            }
            "default" => {
                // `default<T>(Option<T>, T) -> T`: an expected type is the
                // result `T` itself, and only guides an untyped operand. The
                // Core item is the operand's own item, never a widening, so
                // native Target's exact Option signature check agrees.
                let hint = declared.clone().or_else(|| expected.cloned());
                let (option, item) = self.option_operand(&args[0], env, hint.as_ref(), span)?;
                if declared.as_ref().is_some_and(|declared| {
                    !(compatible(declared, &item) && compatible(&item, declared))
                }) {
                    return self.option_error(
                        CompilerErrorKind::TypeMismatch,
                        "default Option item does not match its type argument",
                        span,
                    );
                }
                let fallback = self.check_expr_with_expected(&args[1], env, Some(&item))?;
                if !compatible(&item, &fallback.ty) {
                    return self.option_error(
                        CompilerErrorKind::TypeMismatch,
                        "default fallback does not fit the Option item type",
                        span,
                    );
                }
                (
                    "default",
                    item.clone(),
                    vec![option.expr, fallback.expr],
                    item,
                )
            }
            "unwrap" => {
                let (value, item) = self.refined_operand(&args[0], env, span)?;
                ("unwrap", item.clone(), vec![value.expr], item)
            }
            _ => return None,
        };
        self.core_capabilities.insert(CAPABILITY.to_owned());
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: format!("core.option.{operation}"),
                type_args: vec![item.value_type_coord()],
                args: operands,
            },
            ty: result,
        })
    }

    /// Check an Option-typed operand and return its item shape.
    fn option_operand(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        wanted_item: Option<&TypeShape>,
        span: Span,
    ) -> Option<(TypedValue, TypeShape)> {
        let expected = wanted_item.and_then(option_shape);
        let value = self.check_expr_with_expected(expr, env, expected.as_ref())?;
        let Some(item) = option_item(&value.ty).cloned() else {
            return self.option_error(
                CompilerErrorKind::TypeMismatch,
                "operand must have an Option type",
                span,
            );
        };
        Some((value, item))
    }

    /// `unwrap(x)` needs a local `x` positively refined by `isSome(x)`.
    fn refined_operand(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<(TypedValue, TypeShape)> {
        let Expr::Ident { name, .. } = expr else {
            return self.option_error(
                CompilerErrorKind::UnsupportedSourceShape,
                "unwrap requires a local variable refined by isSome",
                span,
            );
        };
        let (value, item) = self.option_operand(expr, env, None, span)?;
        let refined = matches!(
            &value.expr,
            CoreExpr::Local { reference } if env.get(name).is_some_and(|(local, _)| local == reference)
                && self.option_presence.contains(&reference.id)
        );
        if !refined {
            return self.option_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("unwrap({name}) requires a dominating isSome({name}) refinement"),
                span,
            );
        }
        Some((value, item))
    }

    /// Enter a region where `predicate` holds; returns the mark to restore.
    pub(super) fn enter_presence(&mut self, predicate: Option<&CorePredicate>) -> usize {
        let mark = self.option_presence.len();
        if let Some(predicate) = predicate {
            self.option_presence.extend(present_locals(predicate));
        }
        mark
    }

    pub(super) fn leave_presence(&mut self, mark: usize) {
        self.option_presence.truncate(mark);
    }

    fn option_error<T>(&mut self, kind: CompilerErrorKind, message: &str, span: Span) -> Option<T> {
        self.errors
            .push(error(CompilerStage::TypeCheck, kind, message, span));
        None
    }
}

fn option_item(shape: &TypeShape) -> Option<&TypeShape> {
    match &shape.kind {
        TypeKind::Option(item) => Some(item),
        _ => None,
    }
}

fn option_shape(item: &TypeShape) -> Option<TypeShape> {
    TypeShape::canonical_structural(TypeKind::Option(Box::new(item.clone())))
}

/// Local ids that `predicate` proves present when it holds: `isSome(x)`
/// directly or as a conjunct. Negations and disjunctions prove nothing.
pub(super) fn present_locals(predicate: &CorePredicate) -> Vec<String> {
    match predicate {
        CorePredicate::All(items) => items.iter().flat_map(present_locals).collect(),
        CorePredicate::Compare {
            op: CompareOp::Eq,
            left: CoreExpr::Call { callee, args, .. },
            right: CoreExpr::Const(CoreValue::Bool(true)),
        } if callee == IS_SOME => match args.as_slice() {
            [CoreExpr::Local { reference }] => vec![reference.id.clone()],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Whether `expr` is an untyped `none()` whose item type must come from context.
pub(super) fn is_untyped_none(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call { callee, type_args, args, .. }
            if type_args.is_empty()
                && args.is_empty()
                && matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "none")
    )
}
