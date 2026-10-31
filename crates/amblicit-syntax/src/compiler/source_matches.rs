//! Exhaustive alternatives with one scrutinee and lexical arm payloads.
use super::{
    compatible, compatible_shape, error, CompilerErrorKind, CompilerStage, CoreExpr, Expr,
    LocalEnvironment, Span, TypeChecker, TypeKind, TypeShape, TypedValue,
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
        let mut lowered = Vec::new();
        for arm in arms {
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
            let body = self.check_expr_with_expected(&arm.body, &scope, expected)?;
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
            lowered.push(CoreMatchArm {
                case: arm.case.clone(),
                binder,
                body: body.expr,
            });
        }
        Some(TypedValue {
            expr: CoreExpr::Match {
                scrutinee: Box::new(value.expr),
                arms: lowered,
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
