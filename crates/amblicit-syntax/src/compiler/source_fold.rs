//! First-order bounded state iteration; no callback values or lexical captures.
use super::{
    compatible, error, CompilerErrorKind, CompilerStage, CoreExpr, Expr, HelperCost, IntSuffix,
    LocalEnvironment, Span, TypeChecker, TypeRef, TypedValue,
};

impl TypeChecker<'_> {
    pub(super) fn check_fold(
        &mut self,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<TypedValue> {
        if !type_args.is_empty() || args.len() != 3 {
            return self.fold_failure(
                CompilerErrorKind::TypeMismatch,
                "fold requires initial state, a source step and a static bound",
                span,
            );
        }
        let Expr::Ident { name, .. } = &args[1] else {
            return self.fold_failure(
                CompilerErrorKind::UnsupportedSourceShape,
                "fold step must name a source helper",
                span,
            );
        };
        if env.contains_key(name) {
            return self.fold_failure(
                CompilerErrorKind::UnsupportedSourceShape,
                "fold cannot use a callback value",
                span,
            );
        }
        let Some(signature) = self.function_signatures.get(name).cloned() else {
            return self.fold_failure(
                CompilerErrorKind::UnresolvedFunction,
                "fold step must be a source helper",
                span,
            );
        };
        if signature.params.len() != 1 || signature.params[0].coord != signature.result.coord {
            return self.fold_failure(
                CompilerErrorKind::TypeMismatch,
                "fold step must preserve its single state type",
                span,
            );
        }
        let bound = self.fold_bound(&args[2], span)?;
        let initial = self.check_expr_with_expected(&args[0], env, Some(&signature.result))?;
        if !compatible(&signature.result, &initial.ty)
            || !compatible(&initial.ty, &signature.result)
        {
            return self.fold_failure(
                CompilerErrorKind::TypeMismatch,
                "fold initial value must have the step state type",
                span,
            );
        }
        self.uses_fold = true;
        Some(TypedValue {
            expr: CoreExpr::Fold {
                initial: Box::new(initial.expr),
                step: format!("{}.{}", self.resolved.coordinate, name),
                bound,
                state_type: signature.result.coord.clone(),
            },
            ty: signature.result,
        })
    }

    fn fold_bound(&mut self, expr: &Expr, span: Span) -> Option<u64> {
        if let Expr::Int {
            value,
            suffix: Some(IntSuffix::U64),
            ..
        } = expr
        {
            if let Ok(bound) = value.parse::<u64>() {
                return Some(bound);
            }
        }
        self.fold_failure(
            CompilerErrorKind::InvalidBound,
            "fold bound must be a statically known U64 literal",
            span,
        )
    }

    pub(super) fn fold_cost(
        &mut self,
        expr: &Expr,
        args: &[Expr],
        span: Span,
    ) -> Option<HelperCost> {
        let [initial, Expr::Ident { name, .. }, bound] = args else {
            return None;
        };
        let count = self.fold_bound(bound, span)?;
        let initial_cost = self.helper_cost_for_expr(initial)?;
        let step = *self.function_costs.get(name)?;
        let step = self.checked_helper_cost_add(
            step,
            HelperCost {
                steps: 1,
                ..HelperCost::default()
            },
            span,
        )?;
        let repeated = self.checked_helper_cost_mul(step, count, span)?;
        let nested = self.checked_helper_cost_add(initial_cost, repeated, span)?;
        let own = self.recorded_own_cost(expr, span)?;
        self.checked_helper_cost_add(nested, own, span)
    }

    fn fold_failure<T>(&mut self, kind: CompilerErrorKind, message: &str, span: Span) -> Option<T> {
        self.errors
            .push(error(CompilerStage::TypeCheck, kind, message, span));
        None
    }
}
