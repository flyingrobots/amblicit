//! The bounded Bytes UTF-8 predicate prelude.
use super::{
    error, expression_identity, CompilerErrorKind, CompilerStage, CoreExpr,
    EffectOutputExpectation, Expr, HelperCost, LocalEnvironment, Span, TypeChecker, TypeKind,
    TypeRef, TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_utf8(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
        source: &Expr,
    ) -> Option<TypedValue> {
        let [argument] = args else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "isUtf8 requires exactly one bounded byte operand",
                span,
            ));
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "isUtf8 does not accept authored type arguments",
                span,
            ));
            return None;
        }
        let value = self.check_expr_with_intrinsic_effect_context(
            argument,
            env,
            EffectOutputExpectation::Bytes,
        )?;
        if !matches!(value.ty.kind, TypeKind::Bytes { .. }) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "the implemented isUtf8 specialization requires bounded Bytes",
                span,
            ));
            return None;
        }
        if self.resolved.source_functions.is_empty() {
            let TypeKind::Bytes { max, .. } = &value.ty.kind else {
                return None;
            };
            // Helper-bearing modules already record this scan in their complete
            // expression costs. The legacy intent path still needs its scan bound.
            self.source_expression_costs.insert(
                expression_identity(source),
                HelperCost {
                    steps: *max,
                    ..HelperCost::default()
                },
            );
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.bytes.isUtf8".into(),
                type_args: vec![value.ty.value_type_coord()],
                args: vec![value.expr],
            },
            ty: TypeShape::canonical_structural(TypeKind::Bool)?,
        })
    }
}
