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
    let mut result: Option<super::ComparisonOperandShape> = None;
    let valid = every(
        core,
        functions,
        scrutinee,
        arms,
        available,
        |body, scope| {
            let Some(ty) = super::comparison_operand_shape(core, functions, body, scope) else {
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

pub(super) fn coordinate(
    core: &CoreModule,
    functions: &[TargetPureFunctionFact],
    scrutinee: &CoreExpr,
    arms: &[CoreMatchArm],
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<String> {
    shape(core, functions, scrutinee, arms, available)?.type_coordinate()
}
