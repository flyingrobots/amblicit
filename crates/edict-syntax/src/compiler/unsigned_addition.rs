//! Conservative totality proof for unsigned sums in intent bodies.
use super::{
    compatible, error, expr_span, is_bare_integer_literal, CompareOp, CompilerErrorKind,
    CompilerStage, CoreExpr, CorePredicate, CoreValue, EffectOutputExpectation, Expr,
    LocalEnvironment, Span, TypeChecker, TypeKind, TypeShape, TypedValue,
};

pub(super) fn diagnostic_shape(left: &TypeShape, right: &TypeShape) -> Option<TypeShape> {
    if left == right
        && matches!(&left.kind, TypeKind::Int { width } if width == "U32" || width == "U64")
    {
        Some(left.clone())
    } else {
        super::diagnostic_concat_shape(left, right)
    }
}

impl TypeChecker<'_> {
    pub(super) fn diagnostic_addition_shape(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        env: &LocalEnvironment,
    ) -> Option<TypeShape> {
        let left = self.diagnostic_shape_inner(lhs, env);
        let right = self.diagnostic_shape_inner(rhs, env);
        match (left, right) {
            (Some(left), Some(right)) => diagnostic_shape(&left, &right),
            (Some(known), None) if is_bare_integer_literal(rhs) => unsigned_shape(known),
            (None, Some(known)) if is_bare_integer_literal(lhs) => unsigned_shape(known),
            _ => None,
        }
    }

    pub(super) fn check_addition_or_concat(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        let expected = expectation.value();
        let unsigned = |ty: &TypeShape| matches!(&ty.kind, TypeKind::Int { width } if width == "U32" || width == "U64");
        let infer_unsigned = expected.is_none()
            && !matches!(
                expectation,
                EffectOutputExpectation::String
                    | EffectOutputExpectation::Bytes
                    | EffectOutputExpectation::ConcatenationPeer { .. }
                    | EffectOutputExpectation::Predicate
                    | EffectOutputExpectation::Field { .. }
            );
        if expected.is_some_and(unsigned)
            || infer_unsigned
                && (self
                    .diagnostic_expr_shape(lhs, env)
                    .as_ref()
                    .is_some_and(unsigned)
                    || self
                        .diagnostic_expr_shape(rhs, env)
                        .as_ref()
                        .is_some_and(unsigned))
        {
            self.check_unsigned_addition(lhs, rhs, env, expected, span)
        } else {
            self.check_concat(lhs, rhs, env, expectation, span)
        }
    }

    pub(super) fn check_unsigned_addition(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let (left, right) = if is_bare_integer_literal(lhs) && !is_bare_integer_literal(rhs) {
            let right = self.check_expr_with_effect_expectation(
                rhs,
                env,
                EffectOutputExpectation::Unsigned(expected),
            );
            let left_expected = right.as_ref().map(|value| &value.ty).or(expected);
            let left = left_expected
                .and_then(|expected| self.check_expr_with_expected(lhs, env, Some(expected)));
            (left, right)
        } else {
            let left = self.check_expr_with_effect_expectation(
                lhs,
                env,
                EffectOutputExpectation::Unsigned(expected),
            );
            let right_expected = left.as_ref().map(|value| &value.ty).or(expected);
            let right = if right_expected.is_some() || !is_bare_integer_literal(rhs) {
                self.check_expr_with_effect_expectation(
                    rhs,
                    env,
                    EffectOutputExpectation::Unsigned(right_expected),
                )
            } else {
                None
            };
            (left, right)
        };
        if left.is_none() || right.is_none() {
            for (value, operand) in [(&left, lhs), (&right, rhs)] {
                let Some(value) = value else { continue };
                let unsigned = matches!(&value.ty.kind, TypeKind::Int { width } if width == "U32" || width == "U64");
                if expected.is_some_and(|expected| !compatible(expected, &value.ty)) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "addition operand does not match its expected type",
                        expr_span(operand),
                    ));
                } else if !unsigned {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "addition requires unsigned U32 or U64 operands",
                        expr_span(operand),
                    ));
                }
            }
            return None;
        }
        let left = left?;
        let right = right?;
        if left.ty != right.ty {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "unsigned addition requires identical operand types",
                span,
            ));
            return None;
        }
        let unsigned =
            matches!(&left.ty.kind, TypeKind::Int { width } if width == "U32" || width == "U64");
        if !unsigned
            || !proven_sum(
                &left.expr,
                &right.expr,
                &self.input_proof_constraints,
                if matches!(&left.ty.kind, TypeKind::Int { width } if width == "U32") {
                    u64::from(u32::MAX)
                } else {
                    u64::MAX
                },
            )
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "unsigned addition requires a dominating proof of no overflow",
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.integer.add".to_owned(),
                type_args: vec![left.ty.coord.clone()],
                args: vec![left.expr, right.expr],
            },
            ty: left.ty,
        })
    }
}

fn unsigned_shape(shape: TypeShape) -> Option<TypeShape> {
    matches!(&shape.kind, TypeKind::Int { width } if width == "U32" || width == "U64")
        .then_some(shape)
}

fn constant(expr: &CoreExpr) -> Option<u64> {
    match expr {
        CoreExpr::Const(CoreValue::Int { width, value }) if width == "U32" => {
            value.parse::<u32>().ok().map(u64::from)
        }
        CoreExpr::Const(CoreValue::Int { width, value }) if width == "U64" => value.parse().ok(),
        _ => None,
    }
}

fn proven_sum(left: &CoreExpr, right: &CoreExpr, constraints: &[CorePredicate], max: u64) -> bool {
    let left = upper_bound(left, constraints, max, 0);
    let right = upper_bound(right, constraints, max, 0);
    left.checked_add(right).is_some_and(|sum| sum <= max)
}

fn upper_bound(expr: &CoreExpr, constraints: &[CorePredicate], max: u64, depth: usize) -> u64 {
    if depth > crate::core_ir::MAX_CORE_GRAPH_DEPTH {
        return max;
    }
    let mut bound = constant(expr).unwrap_or_else(|| {
        if let CoreExpr::Call {
            callee,
            type_args,
            args,
        } = expr
        {
            if callee == "core.integer.add" && type_args.len() == 1 {
                if let [left, right] = args.as_slice() {
                    return upper_bound(left, constraints, max, depth + 1)
                        .checked_add(upper_bound(right, constraints, max, depth + 1))
                        .unwrap_or(max)
                        .min(max);
                }
            }
        }
        max
    });
    for predicate in constraints {
        refine_bound(predicate, expr, &mut bound);
    }
    bound
}

fn refine_bound(predicate: &CorePredicate, expr: &CoreExpr, bound: &mut u64) {
    match predicate {
        CorePredicate::All(items) => {
            for item in items {
                refine_bound(item, expr, bound);
            }
        }
        CorePredicate::Compare { op, left, right } => {
            let candidate = match op {
                CompareOp::Le | CompareOp::Lt | CompareOp::Eq if left == expr => constant(right),
                CompareOp::Ge | CompareOp::Gt | CompareOp::Eq if right == expr => constant(left),
                _ => None,
            };
            if let Some(mut candidate) = candidate {
                if matches!(op, CompareOp::Lt | CompareOp::Gt) {
                    candidate = candidate.saturating_sub(1);
                }
                *bound = (*bound).min(candidate);
            }
        }
        _ => {}
    }
}
