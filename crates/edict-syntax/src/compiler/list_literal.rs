//! Bounded list literals typed against their expected `List<T, max=N>`.
use super::{
    compatible, error, expr_span, CompilerErrorKind, CompilerStage, CoreExpr, Expr,
    LocalEnvironment, Span, TypeChecker, TypeKind, TypeShape, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_list_literal(
        &mut self,
        items: &[Expr],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let Some(expected) = expected else {
            if self.expected_type_unavailable {
                // An enclosing failure already explains the missing type; only
                // the elements can still carry independent errors.
                for element in items {
                    self.check_expr(element, env);
                }
            } else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "a list literal requires an expected List<T, max=N> type",
                    span,
                ));
            }
            return None;
        };
        let TypeKind::List { item, max } = &expected.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "a list literal can only produce a List value",
                span,
            ));
            return None;
        };
        if u64::try_from(items.len()).map_or(true, |count| count > *max) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                format!("list literal has more than its declared maximum of {max} elements"),
                span,
            ));
            return None;
        }
        // Check every element so independent element errors all surface.
        let mut values = Vec::with_capacity(items.len());
        let mut accepted = true;
        for element in items {
            match self.check_expr_with_expected(element, env, Some(item)) {
                Some(checked) if compatible(item, &checked.ty) => values.push(checked.expr),
                Some(_) => {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "list element does not match the declared item type",
                        expr_span(element),
                    ));
                    accepted = false;
                }
                None => accepted = false,
            }
        }
        accepted.then(|| TypedValue {
            expr: CoreExpr::List { values },
            ty: expected.clone(),
        })
    }
}
