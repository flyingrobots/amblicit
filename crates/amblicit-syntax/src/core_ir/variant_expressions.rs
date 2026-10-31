//! Constructor metadata and payload traversal at the Core reference boundary.
use super::{
    resolved_named_core_type_table_key, validate_core_expression_types,
    validate_core_type_reference, CoreExpr, CoreModule, CoreType, CoreTypeIntegrityFailure,
    CoreTypeIntegrityFailureKind, CoreTypeIntegrityState,
};

pub(crate) fn core_variant_case<'a>(
    core: &'a CoreModule,
    ty: &str,
    case: &str,
) -> Option<&'a Option<String>> {
    let key = resolved_named_core_type_table_key(core, ty)?;
    let CoreType::Variant { cases } = &core.types[key] else {
        return None;
    };
    cases.get(case)
}
pub(crate) fn core_variant_identity_matches(core: &CoreModule, left: &str, right: &str) -> bool {
    let Some(left) = resolved_named_core_type_table_key(core, left) else {
        return false;
    };
    resolved_named_core_type_table_key(core, right) == Some(left)
        && matches!(core.types[left], CoreType::Variant { .. })
}
pub(super) fn validate(
    core: &CoreModule,
    state: &mut CoreTypeIntegrityState,
    ty: &str,
    case: &str,
    payload: Option<&CoreExpr>,
    path: &str,
    depth: usize,
) -> Result<(), CoreTypeIntegrityFailure> {
    validate_core_type_reference(core, state, ty, &format!("{path}.variant.type"), 0)?;
    let key = resolved_named_core_type_table_key(core, ty).ok_or_else(|| {
        CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidReference,
            format!("{path}.variant.type"),
        )
    })?;
    let CoreType::Variant { cases } = &core.types[key] else {
        return Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidReference,
            format!("{path}.variant.type"),
        ));
    };
    let declared = cases.get(case).ok_or_else(|| {
        CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.variant.case"),
        )
    })?;
    if declared.is_some() != payload.is_some() {
        return Err(CoreTypeIntegrityFailure::new(
            CoreTypeIntegrityFailureKind::InvalidDefinition,
            format!("{path}.variant.payload"),
        ));
    }
    if let Some(payload) = payload {
        validate_core_expression_types(
            core,
            state,
            payload,
            &format!("{path}.variant.payload"),
            depth + 1,
        )?;
    }
    Ok(())
}
