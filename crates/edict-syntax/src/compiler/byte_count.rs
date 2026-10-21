//! The bounded Bytes single-byte occurrence count prelude.
use super::{
    error, expression_identity, CompilerErrorKind, CompilerStage, CoreExpr, CoreValue,
    EffectOutputExpectation, Expr, HelperCost, LocalEnvironment, Span, TypeChecker, TypeKind,
    TypeRef, TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_count(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
        source: &Expr,
    ) -> Option<TypedValue> {
        let [argument, needle] = args else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count requires a bounded byte operand and one byte literal",
                span,
            ));
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count does not accept authored type arguments",
                span,
            ));
            return None;
        }
        // The counted byte is a literal so what the scan looks for never
        // depends on runtime data.
        let Expr::Bytes { value: needle, .. } = needle else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count requires its needle to be a byte literal",
                span,
            ));
            return None;
        };
        let [byte] = needle.as_slice() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count requires a byte literal of exactly one decoded byte",
                span,
            ));
            return None;
        };
        let value = self.check_expr_with_intrinsic_effect_context(
            argument,
            env,
            EffectOutputExpectation::Bytes,
        )?;
        let TypeKind::Bytes { max, .. } = &value.ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "the implemented count specialization requires bounded Bytes",
                span,
            ));
            return None;
        };
        if self.resolved.source_functions.is_empty() {
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
                callee: "core.bytes.count".into(),
                type_args: vec![value.ty.value_type_coord()],
                args: vec![value.expr, CoreExpr::Const(CoreValue::Bytes(vec![*byte]))],
            },
            ty: TypeShape {
                coord: "U64".into(),
                kind: TypeKind::Int {
                    width: "U64".into(),
                },
                preserve_named_identity: false,
            },
        })
    }
}
