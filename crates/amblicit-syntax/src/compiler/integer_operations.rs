//! The checked integer prelude (`checkedAdd` .. `checkedRem`) and total `min`/`max`.
//!
//! SPEC Edict Language v1, Integer Safety (`EDICT-LANG-INT-SAFETY-001`).
//! A checked call lowers to `core.integer.<operation><T>(a, b)` returning
//! `Option<T>` under `core.option/v1`; it never rejects on operand values,
//! because overflow, underflow and a zero divisor are its `none` case. `min`
//! and `max` lower to `core.integer.min<T>` and `core.integer.max<T>`, return
//! `T`, and carry the operand bounds into later addition proofs.
use super::{
    error, is_bare_integer_literal, CompilerErrorKind, CompilerStage, CoreExpr, Expr,
    LocalEnvironment, Span, TypeChecker, TypeKind, TypeShape, TypedValue,
};

/// Checked operations; each returns `Option<T>`.
const CHECKED: &[&str] = &[
    "checkedAdd",
    "checkedSub",
    "checkedMul",
    "checkedDiv",
    "checkedRem",
];
/// Source prelude names owned by this module.
pub(super) const NAMES: &[&str] = &[
    "checkedAdd",
    "checkedSub",
    "checkedMul",
    "checkedDiv",
    "checkedRem",
    "min",
    "max",
];

fn is_supported_width(shape: &TypeShape) -> bool {
    matches!(&shape.kind, TypeKind::Int { width } if matches!(width.as_str(), "U8" | "U32" | "U64"))
}

impl TypeChecker<'_> {
    pub(super) fn check_integer_operation(
        &mut self,
        name: &str,
        type_args_len: usize,
        args: &[Expr],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let checked = CHECKED.contains(&name);
        let [lhs, rhs] = args else {
            return self.integer_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("{name} takes exactly two operands"),
                span,
            );
        };
        if type_args_len != 0 {
            return self.integer_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("{name} takes no type arguments"),
                span,
            );
        }
        // An expected `Option<T>` (checked) or `T` (min/max) supplies the
        // width of bare literal operands; a typed operand overrides it.
        let hint = match expected.map(|shape| &shape.kind) {
            Some(TypeKind::Option(item)) if checked => Some(item.as_ref()),
            Some(TypeKind::Int { .. }) if !checked => expected,
            _ => None,
        };
        let (first, second) = if is_bare_integer_literal(lhs) {
            (rhs, lhs)
        } else {
            (lhs, rhs)
        };
        if hint.is_none() && is_bare_integer_literal(first) {
            return self.integer_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("{name} needs a typed operand or an expected integer type"),
                span,
            );
        }
        let first = self.check_expr_with_expected(first, env, hint)?;
        let second = self.check_expr_with_expected(second, env, Some(&first.ty))?;
        let (left, right) = if is_bare_integer_literal(lhs) {
            (second, first)
        } else {
            (first, second)
        };
        if !is_supported_width(&left.ty) || !is_supported_width(&right.ty) {
            return self.integer_error(
                CompilerErrorKind::UnsupportedSourceShape,
                &format!("{name} requires U8, U32 or U64 operands"),
                span,
            );
        }
        if left.ty != right.ty {
            return self.integer_error(
                CompilerErrorKind::TypeMismatch,
                &format!("{name} requires identical operand types"),
                span,
            );
        }
        let ty = if checked {
            self.core_capabilities
                .insert(super::source_options::CAPABILITY.to_owned());
            TypeShape::canonical_structural(TypeKind::Option(Box::new(left.ty.clone())))?
        } else {
            left.ty.clone()
        };
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: format!("core.integer.{name}"),
                type_args: vec![left.ty.coord.clone()],
                args: vec![left.expr, right.expr],
            },
            ty,
        })
    }

    fn integer_error<T>(
        &mut self,
        kind: CompilerErrorKind,
        message: &str,
        span: Span,
    ) -> Option<T> {
        self.errors
            .push(error(CompilerStage::TypeCheck, kind, message, span));
        None
    }
}
