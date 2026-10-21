//! Calls to pinned target-catalog pure intrinsics.
//!
//! Each argument accepts any operand that fits the contract's ceiling type, so
//! a narrower byte string or a shorter list is accepted. The call is charged
//! from the operand bounds this checker established: a literal's actual size,
//! otherwise the operand's checked type. A caller never states a cost.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    compatible, error, expr_span, expression_identity, imported_type_definition_shape,
    imported_type_shape_closure, plain_callee_coordinate, CompilerErrorKind, CompilerStage,
    CoreExpr, CoreValue, EffectOutputExpectation, Expr, HelperCost, LocalEnvironment, Span,
    TypeChecker, TypeKind, TypeShape, TypeShapeFact, TypedValue,
};
use crate::target_intrinsics::TargetPureIntrinsicFact;

impl TypeChecker<'_> {
    pub(super) fn target_pure_intrinsic(&self, callee: &Expr) -> Option<&TargetPureIntrinsicFact> {
        self.resolved
            .target_pure_intrinsics
            .get(&plain_callee_coordinate(callee)?)
    }

    /// Check `source`, a call whose callee resolves to a target pure intrinsic.
    pub(super) fn check_target_pure_intrinsic(
        &mut self,
        source: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
    ) -> Option<TypedValue> {
        let Expr::Call {
            callee,
            type_args,
            args,
            span,
        } = source
        else {
            return None;
        };
        let span = *span;
        let fact = self.target_pure_intrinsic(callee)?.clone();
        let name = fact.coordinate().to_owned();
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                format!("target pure intrinsic `{name}` does not accept type arguments"),
                span,
            ));
            return None;
        }
        if args.len() != fact.parameter_types().len() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!(
                    "target pure intrinsic `{name}` expects {} arguments, got {}",
                    fact.parameter_types().len(),
                    args.len()
                ),
                span,
            ));
            return None;
        }
        let provider = provider_type_shapes(&fact);
        let (core_args, maxima) =
            self.check_intrinsic_arguments(&fact, &provider, args, env, span)?;
        let Some(steps) = fact.steps_for(&maxima) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                format!("target pure intrinsic `{name}` cost overflows"),
                span,
            ));
            return None;
        };
        self.target_pure_intrinsic_costs.insert(
            expression_identity(source),
            HelperCost {
                steps,
                ..HelperCost::default()
            },
        );
        let Some(result) = self.intrinsic_type_shape(fact.return_type(), &fact, &provider) else {
            self.unresolved_intrinsic_type(&name, fact.return_type(), span);
            return None;
        };
        if expectation
            .value()
            .is_some_and(|expected| !compatible(expected, &result))
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("target pure intrinsic `{name}` return type does not match its use"),
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: name,
                type_args: Vec::new(),
                args: core_args,
            },
            ty: result,
        })
    }

    /// Check every argument against its ceiling type, returning the Core
    /// arguments and the size bound established for each.
    fn check_intrinsic_arguments(
        &mut self,
        fact: &TargetPureIntrinsicFact,
        provider: &BTreeMap<String, TypeShapeFact>,
        args: &[Expr],
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<(Vec<CoreExpr>, Vec<u64>)> {
        let name = fact.coordinate();
        let mut core_args = Vec::with_capacity(args.len());
        let mut maxima = Vec::with_capacity(args.len());
        let mut accepted = true;
        for (arg, parameter) in args.iter().zip(fact.parameter_types()) {
            let Some(shape) = self.intrinsic_type_shape(parameter, fact, provider) else {
                self.unresolved_intrinsic_type(name, parameter, span);
                accepted = false;
                continue;
            };
            let Some(value) = self.check_expr_with_expected(arg, env, Some(&shape)) else {
                accepted = false;
                continue;
            };
            if !compatible(&shape, &value.ty) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    format!("argument does not fit target pure intrinsic `{name}`"),
                    expr_span(arg),
                ));
                accepted = false;
                continue;
            }
            maxima.push(established_max(&value));
            core_args.push(value.expr);
        }
        accepted.then_some((core_args, maxima))
    }

    fn intrinsic_type_shape(
        &mut self,
        coordinate: &str,
        fact: &TargetPureIntrinsicFact,
        provider: &BTreeMap<String, TypeShapeFact>,
    ) -> Option<TypeShape> {
        let shape = imported_type_definition_shape(
            coordinate,
            provider,
            fact.target(),
            &mut BTreeSet::new(),
            None,
            1,
        )?;
        self.core_types
            .extend(imported_type_shape_closure(&shape, provider));
        Some(shape)
    }

    fn unresolved_intrinsic_type(&mut self, name: &str, coordinate: &str, span: Span) {
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnresolvedType,
            format!("target pure intrinsic `{name}` type `{coordinate}` is not in its contract"),
            span,
        ));
    }
}

fn provider_type_shapes(fact: &TargetPureIntrinsicFact) -> BTreeMap<String, TypeShapeFact> {
    fact.provider_definitions()
        .iter()
        .map(|(coordinate, definition)| {
            (
                coordinate.clone(),
                TypeShapeFact {
                    lawpack: fact.target().clone(),
                    coordinate: coordinate.clone(),
                    definition: definition.clone(),
                },
            )
        })
        .collect()
}

/// The size bound this checker established for one argument.
fn established_max(value: &TypedValue) -> u64 {
    match &value.expr {
        CoreExpr::List { values } => u64::try_from(values.len()).unwrap_or(u64::MAX),
        CoreExpr::Const(CoreValue::Bytes(bytes)) => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        _ => match &value.ty.kind {
            TypeKind::Bytes { max, .. } | TypeKind::List { max, .. } => *max,
            _ => 0,
        },
    }
}
