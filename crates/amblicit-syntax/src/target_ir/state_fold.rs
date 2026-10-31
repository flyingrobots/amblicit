//! Independent first-order fold authority and invariant-state judgment.
use super::{
    core_type_fits, expression_fits_declared_type, resolved_core_type, CoreExpr, CoreModule,
    CoreType, LocalRef, TargetPureFunctionFact,
};
use std::collections::BTreeMap;

pub(super) fn state<'a>(
    core: &'a CoreModule,
    imported: &[TargetPureFunctionFact],
    expression: &'a CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<&'a str> {
    let CoreExpr::Fold {
        initial,
        step,
        state_type,
        ..
    } = expression
    else {
        return None;
    };
    if !core
        .required_core_capabilities
        .iter()
        .any(|c| c == "core.fold/v1")
        || imported.iter().any(|f| f.coordinate == *step)
    {
        return None;
    }
    let name = step
        .strip_prefix(core.coordinate.as_str())?
        .strip_prefix('.')?;
    let function = core.functions.get(name)?;
    if function.params.len() != 1
        || function.params[0].ty != *state_type
        || function.return_type != *state_type
        || !core_type_fits(core, state_type, &function.params[0].ty)
        || !expression_fits_declared_type(core, imported, initial, state_type, available)
    {
        return None;
    }
    Some(state_type)
}

pub(super) fn string_shape(
    core: &CoreModule,
    imported: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    available: &BTreeMap<&str, &LocalRef>,
) -> Option<(u64, String)> {
    let ty = state(core, imported, expression, available)?;
    match resolved_core_type(core, ty)? {
        CoreType::String { max, canonical } => Some((max, canonical)),
        _ => None,
    }
}
