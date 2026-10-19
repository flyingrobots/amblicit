//! The bounded Bytes UTF-8 predicate prelude.
use super::{
    error, BTreeMap, CompilerErrorKind, CompilerStage, CoreExpr, Expr, LocalRef, Span, TypeChecker,
    TypeKind, TypeRef, TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_utf8(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &BTreeMap<String, (LocalRef, TypeShape)>,
        span: Span,
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
        let value = self.check_expr(argument, env)?;
        if !matches!(value.ty.kind, TypeKind::Bytes { .. }) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "the implemented isUtf8 specialization requires bounded Bytes",
                span,
            ));
            return None;
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
