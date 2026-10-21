//! The bounded Bytes single-byte occurrence count prelude.
use super::{
    error, expression_identity, CompilerErrorKind, CompilerStage, CoreExpr,
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
        // The operand is checked whatever else is wrong with the call, so its
        // own diagnostics surface alongside the signature's.
        let value = args.first().and_then(|argument| {
            self.check_expr_with_intrinsic_effect_context(
                argument,
                env,
                EffectOutputExpectation::Bytes,
            )
        });
        let needle = self.check_count_signature(type_args, args, env, span);
        let value = value?;
        let TypeKind::Bytes { max, .. } = &value.ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "the implemented count specialization requires bounded Bytes",
                span,
            ));
            return None;
        };
        let needle = needle?;
        let coordinate = value.ty.value_type_coord();
        self.count_operand_maxima.insert(coordinate.clone(), *max);
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
                type_args: vec![coordinate],
                args: vec![value.expr, needle],
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

    /// Checks arity, type arguments and the needle, returning the needle's
    /// Core value. The needle goes through ordinary expression checking so its
    /// evaluation and storage are charged like any other operand.
    fn check_count_signature(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<CoreExpr> {
        let [_, needle] = args else {
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
        // The counted byte is a literal: a scope decision for this first
        // specialization, so what the scan looks for is fixed in the program.
        let Expr::Bytes { value: literal, .. } = needle else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count requires its needle to be a byte literal",
                span,
            ));
            return None;
        };
        if literal.len() != 1 {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "count requires a byte literal of exactly one decoded byte",
                span,
            ));
            return None;
        }
        self.check_expr_with_expected(needle, env, None)
            .map(|checked| checked.expr)
    }
}
