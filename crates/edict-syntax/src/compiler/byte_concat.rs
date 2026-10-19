//! Conservative maximum-bound composition for ordered raw byte concatenation.
use super::{
    error, CompilerErrorKind, CompilerStage, CoreExpr, Span, TypeChecker, TypeKind, TypeShape,
    TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_byte_concat(
        &mut self,
        left: TypedValue,
        right: TypedValue,
        span: Span,
    ) -> Option<TypedValue> {
        let (TypeKind::Bytes { max: left_max, .. }, TypeKind::Bytes { max: right_max, .. }) =
            (&left.ty.kind, &right.ty.kind)
        else {
            return None;
        };
        let Some(max) = left_max.checked_add(*right_max) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "byte concatenation maximum exceeds U64",
                span,
            ));
            return None;
        };
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.bytes.concat".into(),
                type_args: vec![left.ty.value_type_coord(), right.ty.value_type_coord()],
                args: vec![left.expr, right.expr],
            },
            ty: TypeShape::canonical_structural(TypeKind::Bytes { min: None, max })?,
        })
    }
}
