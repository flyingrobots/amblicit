//! Total single-byte lookup with an exact-operand, strict index proof.
use super::{
    error, integer_shape, CompareOp, CompilerErrorKind, CompilerStage, CoreExpr, CorePredicate,
    CoreValue, EffectOutputExpectation, Expr, LocalEnvironment, Span, TypeChecker, TypeKind,
    TypeRef, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_at(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<TypedValue> {
        let [bytes, index] = args else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "at requires bounded bytes and one U64 index",
                span,
            ));
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "at does not accept authored type arguments",
                span,
            ));
            return None;
        }
        let bytes = self.check_expr_with_intrinsic_effect_context(
            bytes,
            env,
            EffectOutputExpectation::Bytes,
        )?;
        let TypeKind::Bytes { min, .. } = bytes.ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "at requires bounded structural Bytes",
                span,
            ));
            return None;
        };
        let index_type = integer_shape("U64");
        let index = self.check_expr_with_expected(index, env, Some(&index_type))?;
        if index.ty != index_type {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "at index must have type U64",
                span,
            ));
            return None;
        }
        let coordinate = bytes.ty.value_type_coord();
        let length = CoreExpr::Call {
            callee: "core.bytes.length".into(),
            type_args: vec![coordinate.clone()],
            args: vec![bytes.expr.clone()],
        };
        let known_index = match &index.expr {
            CoreExpr::Const(CoreValue::Int { width, value }) if width == "U64" => {
                value.parse::<u64>().ok()
            }
            _ => None,
        };
        let static_bound = known_index.zip(min).is_some_and(|(index, min)| index < min);
        if !static_bound
            && !self
                .input_proof_constraints
                .iter()
                .any(|p| proves_strict(p, true, &index.expr, &length))
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "at requires proof of index < len(bytes) before evaluation",
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.bytes.at".into(),
                type_args: vec![coordinate],
                args: vec![bytes.expr, index.expr],
            },
            ty: integer_shape("U8"),
        })
    }
}

fn proves_strict(
    predicate: &CorePredicate,
    truth: bool,
    index: &CoreExpr,
    length: &CoreExpr,
) -> bool {
    match predicate {
        CorePredicate::Not(inner) => proves_strict(inner, !truth, index, length),
        CorePredicate::All(items) if truth => {
            items.iter().any(|p| proves_strict(p, true, index, length))
        }
        CorePredicate::Any(items) if !truth => {
            items.iter().any(|p| proves_strict(p, false, index, length))
        }
        CorePredicate::Compare { op, left, right } => match (op, truth) {
            (CompareOp::Lt, true) | (CompareOp::Ge, false) => left == index && right == length,
            (CompareOp::Gt, true) | (CompareOp::Le, false) => right == index && left == length,
            _ => false,
        },
        _ => false,
    }
}
