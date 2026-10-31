//! Exhaustive alternatives with one scrutinee and lexical arm payloads.
use super::{
    compatible, compatible_shape, error, is_bare_integer_literal, CompilerErrorKind, CompilerStage,
    CoreExpr, Expr, LocalEnvironment, Span, TypeChecker, TypeKind, TypeShape, TypedValue,
};
use crate::{
    ast::MatchArm,
    core_ir::{CoreMatchArm, LocalRef},
};
use std::collections::BTreeSet;

impl TypeChecker<'_> {
    pub(super) fn check_variant_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        self.match_depth += 1;
        self.match_in_intent = true;
        let result = self.check_variant_match_inner(scrutinee, arms, env, expected, span);
        self.match_depth -= 1;
        result
    }

    fn check_variant_match_inner(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let value = self.check_expr(scrutinee, env)?;
        let TypeKind::Variant(cases) = &value.ty.kind else {
            return self.invalid_match("match requires a variant value", span);
        };
        let names = arms
            .iter()
            .map(|arm| arm.case.as_str())
            .collect::<BTreeSet<_>>();
        if names.len() != arms.len()
            || names.len() != cases.len()
            || !cases.keys().all(|case| names.contains(case.as_str()))
        {
            return self.invalid_match("match must cover every case exactly once", span);
        }
        let mut result_type = expected.cloned();
        let mut lowered = vec![None; arms.len()];
        // Infer typed peers before bare literals, while retaining source arm order
        // in Core. Like conditionals, no width is invented for all-bare arms.
        let mut order = (0..arms.len()).collect::<Vec<_>>();
        if expected.is_none() {
            order.sort_by_key(|index| is_bare_integer_literal(&arms[*index].body));
        }
        for index in order {
            let arm = &arms[index];
            let declared = &cases[&arm.case];
            let mut scope = env.clone();
            let binder = match (&arm.binder, declared) {
                (Some(name), Some(payload)) => {
                    let mut index = 0;
                    while scope
                        .values
                        .values()
                        .any(|(reference, _)| reference.id == format!("match.{index}"))
                    {
                        index += 1;
                    }
                    let local = LocalRef {
                        id: format!("match.{index}"),
                        alpha_name: format!("$match{index}"),
                        ty: payload.value_type_coord(),
                    };
                    scope.insert(name.clone(), (local.clone(), payload.clone()));
                    Some(local)
                }
                (Some(_), None) => {
                    return self.invalid_match("payloadless case cannot bind a payload", arm.span)
                }
                (None, _) => None,
            };
            let arm_expected = expected.or_else(|| {
                is_bare_integer_literal(&arm.body)
                    .then_some(result_type.as_ref())
                    .flatten()
            });
            let body = self.check_expr_with_expected(&arm.body, &scope, arm_expected)?;
            result_type = match result_type {
                None => Some(body.ty.clone()),
                Some(prior) if expected.is_some() => {
                    if !compatible(&prior, &body.ty) {
                        return self.invalid_match("match result type mismatch", arm.span);
                    }
                    Some(prior)
                }
                Some(prior) => Some(compatible_shape(&prior, &body.ty).or_else(|| {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "match arm result types disagree",
                        arm.span,
                    ));
                    None
                })?),
            };
            lowered[index] = Some(CoreMatchArm {
                case: arm.case.clone(),
                binder,
                body: body.expr,
            });
        }
        Some(TypedValue {
            expr: CoreExpr::Match {
                scrutinee: Box::new(value.expr),
                arms: lowered.into_iter().collect::<Option<Vec<_>>>()?,
            },
            ty: result_type?,
        })
    }
    fn invalid_match(&mut self, message: &str, span: Span) -> Option<TypedValue> {
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::TypeMismatch,
            message,
            span,
        ));
        None
    }
}
