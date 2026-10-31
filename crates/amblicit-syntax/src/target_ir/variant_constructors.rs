//! Target rechecks exact variant identity and payload typing from untrusted Core.
use super::{
    expression_fits_declared_type, CoreExpr, CoreModule, LocalRef, TargetPureFunctionFact,
};
use crate::core_ir::{core_variant_case, core_variant_identity_matches};
use std::collections::BTreeMap;

pub(super) fn fits(
    core: &CoreModule,
    pure_functions: &[TargetPureFunctionFact],
    expression: &CoreExpr,
    expected: &str,
    available: &BTreeMap<&str, &LocalRef>,
) -> bool {
    let CoreExpr::Variant { ty, case, payload } = expression else {
        return false;
    };
    if !core_variant_identity_matches(core, ty, expected) {
        return false;
    }
    match (core_variant_case(core, ty, case), payload.as_deref()) {
        (Some(None), None) => true,
        (Some(Some(expected)), Some(payload)) => {
            expression_fits_declared_type(core, pure_functions, payload, expected, available)
        }
        _ => false,
    }
}
