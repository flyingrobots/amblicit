//! Independent case coverage, lexical payload binding and result judgments.
use super::{
    core_type_fits, expression_type_coordinate, resolved_core_type, CoreExpr, CoreModule, CoreType,
    LocalRef, TargetPureFunctionFact,
};
use crate::core_ir::CoreMatchArm;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn every(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
    mut check: impl FnMut(&CoreExpr, &BTreeMap<&str, &LocalRef>) -> bool,
) -> bool {
    let Some(ty) = expression_type_coordinate(core, functions, scrutinee, available) else {
        return false;
    };
    let Some(CoreType::Variant { cases }) = resolved_core_type(core, &ty) else {
        return false;
    };
    let names = arms
        .iter()
        .map(|arm| arm.case.as_str())
        .collect::<BTreeSet<_>>();
    if arms.is_empty()
        || names.len() != arms.len()
        || names.len() != cases.len()
        || !cases.keys().all(|name| names.contains(name.as_str()))
    {
        return false;
    }
    arms.iter().all(|arm| {
        let mut scope = available.clone();
        if let Some(binder) = &arm.binder {
            let Some(expected) = &cases[&arm.case] else {
                return false;
            };
            if binder.id.is_empty()
                || binder.alpha_name.is_empty()
                || !core_type_fits(core, &binder.ty, expected)
                || !core_type_fits(core, expected, &binder.ty)
                || scope.insert(binder.id.as_str(), binder).is_some()
            {
                return false;
            }
        }
        check(&arm.body, &scope)
    })
}

pub(super) fn shape(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<super::ComparisonOperandShape> {
    joined_shape(core, functions, scrutinee, arms, available, false)
}

pub(super) fn result_shape(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<super::ComparisonOperandShape> {
    joined_shape(core, functions, scrutinee, arms, available, true)
}

fn joined_shape(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
    results: bool,
) -> Option<super::ComparisonOperandShape> {
    let mut result: Option<super::ComparisonOperandShape> = None;
    let valid = every(
        core,
        functions,
        scrutinee,
        arms,
        available,
        |body, scope| {
            let ty = if results {
                result_body_shape(core, functions, body, scope)
            } else {
                super::comparison_operand_shape(core, functions, body, scope)
            };
            let Some(ty) = ty else {
                return false;
            };
            result = match &result {
                None => Some(ty),
                Some(previous) => super::join_conditional_shapes(core, previous, &ty, 0),
            };
            result.is_some()
        },
    );
    if valid {
        result
    } else {
        None
    }
}

fn result_body_shape(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    body: &CoreExpr,
    scope: &BTreeMap<&str, &LocalRef>,
) -> Option<super::ComparisonOperandShape> {
    use super::ComparisonOperandShape as Shape;
    match body {
        CoreExpr::List { values } => {
            let mut item = None;
            for value in values {
                let next = result_body_shape(core, functions, value, scope)?;
                item = Some(match item {
                    None => next,
                    Some(prior) => super::join_conditional_shapes(core, &prior, &next, 0)?,
                });
            }
            Some(item.map_or(Shape::EmptyList, |item| Shape::List {
                item: Box::new(item),
                max: values.len() as u64,
            }))
        }
        CoreExpr::Record { fields } => fields
            .iter()
            .map(|(name, value)| {
                Some((
                    name.clone(),
                    result_body_shape(core, functions, value, scope)?,
                ))
            })
            .collect::<Option<BTreeMap<_, _>>>()
            .map(Shape::Record),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => {
            if !super::predicate_fits_core_types(core, functions, predicate, scope) {
                return None;
            }
            super::join_conditional_shapes(
                core,
                &result_body_shape(core, functions, then_value, scope)?,
                &result_body_shape(core, functions, else_value, scope)?,
                0,
            )
        }
        CoreExpr::Match { scrutinee, arms } => {
            result_shape(core, functions, scrutinee, arms, scope)
        }
        _ => super::comparison_operand_shape(core, functions, body, scope),
    }
}

pub(super) fn coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    shape(core, functions, scrutinee, arms, available)?.type_coordinate()
}
