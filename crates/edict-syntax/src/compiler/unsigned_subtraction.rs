//! Conservative totality proof for unsigned differences in intent bodies.
use super::{
    error, is_bare_integer_literal, BTreeMap, CompareOp, CompilerErrorKind, CompilerStage,
    CoreExpr, CorePredicate, CoreValue, Expr, LocalRef, Span, TypeChecker, TypeKind, TypeShape,
    TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_unsigned_subtraction(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        env: &BTreeMap<String, (LocalRef, TypeShape)>,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let (left, right) = if is_bare_integer_literal(lhs) && !is_bare_integer_literal(rhs) {
            let right = self.check_expr_with_expected(rhs, env, expected)?;
            (
                self.check_expr_with_expected(lhs, env, Some(&right.ty))?,
                right,
            )
        } else {
            let left = self.check_expr_with_expected(lhs, env, expected)?;
            let right = self.check_expr_with_expected(rhs, env, Some(&left.ty))?;
            (left, right)
        };
        if left.ty != right.ty {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "unsigned subtraction requires identical operand types",
                span,
            ));
            return None;
        }
        let unsigned =
            matches!(&left.ty.kind, TypeKind::Int { width } if width == "U32" || width == "U64");
        if !unsigned || !proven_order(&left.expr, &right.expr, &self.input_proof_constraints) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "unsigned subtraction requires a dominating proof of no underflow",
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.integer.subtract".to_owned(),
                type_args: vec![left.ty.coord.clone()],
                args: vec![left.expr, right.expr],
            },
            ty: left.ty,
        })
    }
}

fn integer(expr: &CoreExpr) -> Option<u64> {
    match expr {
        CoreExpr::Const(CoreValue::Int { width, value }) if width == "U32" || width == "U64" => {
            value.parse().ok()
        }
        _ => None,
    }
}

fn unconditional_order(left: &CoreExpr, right: &CoreExpr) -> bool {
    left == right
        || integer(right) == Some(0)
        || matches!((integer(left), integer(right)), (Some(a), Some(b)) if a >= b)
}

pub(super) fn proven_order(
    left: &CoreExpr,
    right: &CoreExpr,
    constraints: &[CorePredicate],
) -> bool {
    unconditional_order(left, right)
        || constraints
            .iter()
            .any(|predicate| supports_order(predicate, left, right))
}

fn supports_order(predicate: &CorePredicate, left: &CoreExpr, right: &CoreExpr) -> bool {
    match predicate {
        CorePredicate::All(items) => items.iter().any(|item| supports_order(item, left, right)),
        CorePredicate::Compare {
            op,
            left: a,
            right: b,
        } => match op {
            CompareOp::Ge | CompareOp::Gt => a == left && b == right,
            CompareOp::Le | CompareOp::Lt => a == right && b == left,
            CompareOp::Eq => (a == left && b == right) || (a == right && b == left),
            CompareOp::Ne => false,
        },
        CorePredicate::True
        | CorePredicate::False
        | CorePredicate::Not(_)
        | CorePredicate::Any(_) => false,
    }
}
