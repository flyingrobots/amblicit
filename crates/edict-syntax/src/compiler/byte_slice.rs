//! Proven half-open slicing over bounded raw bytes.
use super::{
    error, expr_span, integer_shape, unsigned_subtraction::proven_order, CompilerErrorKind,
    CompilerStage, CoreExpr, Expr, LocalEnvironment, Span, TypeChecker, TypeKind, TypeRef,
    TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_slice(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<TypedValue> {
        let [bytes, start, end] = args else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "slice requires bounded bytes and two U64 endpoints",
                span,
            ));
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "slice does not accept authored type arguments",
                span,
            ));
            return None;
        }
        let bytes = self.check_expr_with_intrinsic_context(bytes, env);
        let max = bytes.as_ref().and_then(|value| {
            if let TypeKind::Bytes { max, .. } = value.ty.kind {
                Some(max)
            } else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "slice requires bounded structural Bytes",
                    expr_span(&args[0]),
                ));
                None
            }
        });
        let index_type = integer_shape("U64");
        let start = self.check_expr_with_expected(start, env, Some(&index_type));
        let end = self.check_expr_with_expected(end, env, Some(&index_type));
        let mut accepted = true;
        for (value, operand) in [(&start, &args[1]), (&end, &args[2])] {
            if value.as_ref().is_some_and(|value| value.ty != index_type) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "slice endpoint must have type U64",
                    expr_span(operand),
                ));
                accepted = false;
            }
        }
        if !accepted {
            return None;
        }
        let max = max?;
        let bytes = bytes?;
        let start = start?;
        let end = end?;
        let coordinate = bytes.ty.value_type_coord();
        let length = CoreExpr::Call {
            callee: "core.bytes.length".into(),
            type_args: vec![coordinate.clone()],
            args: vec![bytes.expr.clone()],
        };
        if !proven_order(&end.expr, &start.expr, &self.input_proof_constraints)
            || !proven_order(&length, &end.expr, &self.input_proof_constraints)
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "slice requires proof of start <= end <= len(bytes) before evaluation",
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.bytes.slice".into(),
                type_args: vec![coordinate],
                args: vec![bytes.expr, start.expr, end.expr],
            },
            ty: TypeShape::canonical_structural(TypeKind::Bytes { min: None, max })?,
        })
    }
}
