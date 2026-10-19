//! The bounded Bytes specialization of the language's length prelude.
use super::{
    error, CompilerErrorKind, CompilerStage, CoreExpr, Expr, LocalEnvironment, Span, TypeChecker,
    TypeKind, TypeRef, TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_length(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<TypedValue> {
        let [argument] = args else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "len requires exactly one bounded byte operand",
                span,
            ));
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "len does not accept authored type arguments",
                span,
            ));
            return None;
        }
        let value = self.check_expr_with_intrinsic_context(argument, env)?;
        if !matches!(value.ty.kind, TypeKind::Bytes { .. }) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "the implemented len specialization requires bounded Bytes",
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.bytes.length".into(),
                type_args: vec![value.ty.value_type_coord()],
                args: vec![value.expr],
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
