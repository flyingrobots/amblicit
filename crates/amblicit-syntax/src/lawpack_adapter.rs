//! Canonical direct lawpack-adapter loading and compiler preparation.
//!
//! A direct adapter is declarative, selected by one exact digest-locked target
//! profile entry in a validated lawpack manifest, and complete over that
//! lawpack's runtime effects. It supplies compiler and Target IR facts; it does
//! not execute a runtime, emit an Echo package, or confer admission authority.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{ImportKind, Module};
use crate::canonical::{
    decode_canonical_cbor, digest_canonical_value, sha256_review_string, CanonicalValue,
};
use crate::compiler::{
    imported_type_core_closure, BoundFact, CompilerContext, EffectSignatureFact, PureFunctionFact,
    PureHelperCostFact, TypeShapeFact,
};
use crate::core_ir::{CoreBudget, ResourceRef};
use crate::lawpack::{
    LawpackAuthorityClass, LawpackExecutionClass, LawpackResourceRef, LawpackSemanticEffect,
    LawpackTargetAdapter, ValidatedLawpackBundle, LAWPACK_API_VERSION,
};
use crate::lowerability::WriteClass;
use crate::target_ir::{
    TargetEffectLowering, TargetEffectSignatureFact, TargetEffectSignatureShape,
    TargetIrLoweringFacts, TargetPureFunctionFact,
};

/// Canonical direct lawpack-adapter ABI supported by this crate.
pub const LAWPACK_ADAPTER_API_VERSION: &str = "edict.lawpack-adapter/v1";

const ADAPTER_PATH: &str = "<lawpack-adapter-cbor>";

/// Stable direct-adapter failure classifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LawpackAdapterFailureKind {
    InvalidCanonicalCbor,
    InvalidShape,
    MissingField,
    UnexpectedField,
    InvalidApiVersion,
    UnsupportedClass,
    MissingTargetAdapter,
    AmbiguousTargetAdapter,
    AdapterDigestMismatch,
    MissingOperationProfile,
    UnknownOperationProfile,
    MissingEffectImplementation,
    UnknownEffectImplementation,
    DuplicateReference,
    ObligationMismatch,
    FailureMappingMismatch,
    MissingBudget,
    UnknownBudget,
    InvalidWriteClass,
    InvalidTargetIntrinsic,
    InvalidTargetConfiguration,
    SourceImportMismatch,
}

/// One failed direct-adapter validation or preparation obligation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAdapterFailure {
    pub kind: LawpackAdapterFailureKind,
    pub path: String,
    pub obligation: String,
}

/// One operation profile discharged by a direct adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAdapterOperationProfile {
    pub core: String,
    pub semantic_effects: Vec<String>,
    /// Exact source budget required when `semantic_effects` is empty.
    pub budget_obligation: Option<String>,
    /// Opaque target configuration required when `semantic_effects` is empty.
    pub target_configuration: Option<LawpackResourceRef>,
}

/// One semantic effect discharged by a direct adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawpackAdapterEffect {
    pub target_intrinsic: String,
    pub target_configuration: LawpackResourceRef,
    pub write_class: WriteClass,
    pub footprint_obligation: String,
    pub cost_obligation: String,
    pub failure_mappings: BTreeMap<String, String>,
}

/// A canonical direct adapter corroborated against its owning lawpack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedLawpackAdapter {
    descriptor: LawpackTargetAdapter,
    operation_profiles: BTreeMap<String, LawpackAdapterOperationProfile>,
    effects: BTreeMap<String, LawpackAdapterEffect>,
    budgets: BTreeMap<String, CoreBudget>,
    digest: [u8; 32],
}

impl ValidatedLawpackAdapter {
    /// Exact target profile selected by the owning lawpack manifest.
    #[must_use]
    pub fn target_profile(&self) -> &LawpackResourceRef {
        &self.descriptor.accepted_target_profile
    }

    /// Exact Target IR selected by the owning lawpack manifest.
    #[must_use]
    pub fn target_ir(&self) -> &LawpackResourceRef {
        &self.descriptor.accepted_target_ir
    }

    /// Canonical operation-profile discharges.
    #[must_use]
    pub fn operation_profiles(&self) -> &BTreeMap<String, LawpackAdapterOperationProfile> {
        &self.operation_profiles
    }

    /// Canonical semantic-effect discharges.
    #[must_use]
    pub fn effects(&self) -> &BTreeMap<String, LawpackAdapterEffect> {
        &self.effects
    }

    /// Canonical budget discharges.
    #[must_use]
    pub fn budgets(&self) -> &BTreeMap<String, CoreBudget> {
        &self.budgets
    }

    /// Domain-framed digest of the exact canonical adapter bytes.
    #[must_use]
    pub fn digest_review_string(&self) -> String {
        sha256_review_string(&self.digest)
    }
}

/// Compiler and Target IR facts derived from an exact module/lawpack/adapter
/// closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedLawpackCompilation {
    compiler_context: CompilerContext,
    target_ir_facts: TargetIrLoweringFacts,
}

struct ProjectedEffects {
    compiler_context: CompilerContext,
    obstruction_coordinates: BTreeSet<String>,
    effect_lowerings: Vec<TargetEffectLowering>,
    effect_signatures: Vec<TargetEffectSignatureFact>,
}

impl PreparedLawpackCompilation {
    /// Compiler facts projected through the module-local lawpack alias.
    #[must_use]
    pub fn compiler_context(&self) -> &CompilerContext {
        &self.compiler_context
    }

    /// Target IR facts projected through the same module-local alias.
    #[must_use]
    pub fn target_ir_facts(&self) -> &TargetIrLoweringFacts {
        &self.target_ir_facts
    }
}

/// Decode one canonical direct adapter selected by a validated lawpack.
///
/// # Errors
///
/// Returns stable failures for canonical-shape errors, target selection
/// ambiguity, digest substitution, incomplete coverage, or mismatched
/// footprint, cost, and failure obligations.
pub fn decode_lawpack_adapter(
    bundle: &ValidatedLawpackBundle,
    target_profile: &str,
    bytes: &[u8],
) -> Result<ValidatedLawpackAdapter, Vec<LawpackAdapterFailure>> {
    let descriptor = select_descriptor(bundle, target_profile)?;
    let value = decode_canonical_cbor(bytes).map_err(|_| {
        one(failure(
            LawpackAdapterFailureKind::InvalidCanonicalCbor,
            ADAPTER_PATH,
            "exact canonical CBOR",
        ))
    })?;
    let digest = digest_value(&descriptor.adapter.id, &value)?;
    if digest != descriptor.adapter.digest {
        return Err(one(failure(
            LawpackAdapterFailureKind::AdapterDigestMismatch,
            "manifest.targetAdapters.adapter.digest",
            "digest of the exact canonical adapter bytes",
        )));
    }

    let (operation_profiles, effects, budgets) = parse_adapter(&value)?;
    validate_adapter_closure(bundle, &descriptor, &operation_profiles, &effects, &budgets)?;

    Ok(ValidatedLawpackAdapter {
        descriptor,
        operation_profiles,
        effects,
        budgets,
        digest,
    })
}

/// Derive compiler and target facts for one module from its exact imported
/// lawpack and selected direct adapter.
///
/// # Errors
///
/// Returns `SourceImportMismatch` unless the module contains at least one
/// matching lawpack import and every such import has the exact manifest digest
/// and a distinct alias. All aliases receive the same authenticated facts.
pub fn prepare_lawpack_compilation(
    module: &Module,
    bundle: &ValidatedLawpackBundle,
    adapter: &ValidatedLawpackAdapter,
) -> Result<PreparedLawpackCompilation, Vec<LawpackAdapterFailure>> {
    let aliases = matching_import_aliases(module, bundle)?;
    let prefix = format!("{}@{}.", bundle.manifest().id, bundle.manifest().version);
    let lawpack = ResourceRef {
        coordinate: format!("{}@{}", bundle.manifest().id, bundle.manifest().version),
        digest: Some(bundle.manifest_digest_review_string()),
    };
    let mut compiler_context = CompilerContext::new();
    let mut operation_profiles = BTreeSet::new();
    let type_shapes = exported_type_shapes(bundle, &lawpack);
    let mut all_obstructions = BTreeSet::new();
    let mut all_effect_lowerings = Vec::new();
    let mut all_effect_signatures = Vec::new();
    let mut canonical_pure_functions = Vec::new();

    for alias in aliases {
        for type_shape in type_shapes.values() {
            local_coordinate(&alias, &prefix, &type_shape.coordinate)?;
            compiler_context = compiler_context.with_type_shape(type_shape.clone());
        }

        for (coordinate, profile) in &adapter.operation_profiles {
            let local_profile = local_coordinate(&alias, &prefix, coordinate)?;
            let mut write_classes = BTreeSet::new();
            for effect_coordinate in &profile.semantic_effects {
                let effect = adapter.effects.get(effect_coordinate).ok_or_else(|| {
                    one(failure(
                        LawpackAdapterFailureKind::MissingEffectImplementation,
                        format!("adapter.operationProfiles.{coordinate}.semanticEffects"),
                        effect_coordinate,
                    ))
                })?;
                write_classes.insert(effect.write_class.clone());
            }
            compiler_context = compiler_context
                .with_operation_profile(local_profile.clone(), profile.core.clone())
                .with_operation_profile_write_classes(local_profile.clone(), write_classes);
            if let Some(budget) = &profile.budget_obligation {
                let local_budget = local_coordinate(&alias, &prefix, budget)?;
                compiler_context =
                    compiler_context.with_operation_profile_budget(local_profile, local_budget);
            }
            operation_profiles.insert(profile.core.clone());
        }

        let ProjectedEffects {
            compiler_context: projected_context,
            obstruction_coordinates,
            effect_lowerings,
            effect_signatures,
        } = project_effects(
            compiler_context,
            bundle,
            adapter,
            &lawpack,
            &alias,
            &prefix,
            &type_shapes,
        )?;
        compiler_context = projected_context;

        let (projected_context, pure_functions) = project_pure_functions(
            compiler_context,
            bundle,
            adapter,
            &lawpack,
            &alias,
            &prefix,
            &type_shapes,
        )?;
        compiler_context = projected_context;

        compiler_context =
            project_bound_constants(compiler_context, bundle, &lawpack, &alias, &prefix)?;

        compiler_context = project_budgets(compiler_context, adapter, &lawpack, &alias, &prefix)?;

        all_obstructions.extend(obstruction_coordinates);
        all_effect_lowerings.extend(effect_lowerings);
        all_effect_signatures.extend(effect_signatures);
        // Pure facts use canonical export coordinates rather than local aliases.
        // Each alias projects the same exact bundle, so retain one canonical set.
        canonical_pure_functions = pure_functions;
    }

    Ok(PreparedLawpackCompilation {
        compiler_context,
        target_ir_facts: TargetIrLoweringFacts {
            target_profile: ResourceRef {
                coordinate: adapter.target_profile().id.clone(),
                digest: Some(adapter.target_profile().digest_review_string()),
            },
            target_ir_domain: adapter.target_ir().id.clone(),
            operation_profiles: operation_profiles.into_iter().collect(),
            obstruction_coordinates: all_obstructions.into_iter().collect(),
            effect_lowerings: all_effect_lowerings,
            effect_signatures: all_effect_signatures,
            pure_functions: canonical_pure_functions,
        },
    })
}

fn project_budgets(
    mut context: CompilerContext,
    adapter: &ValidatedLawpackAdapter,
    lawpack: &ResourceRef,
    alias: &str,
    prefix: &str,
) -> Result<CompilerContext, Vec<LawpackAdapterFailure>> {
    for (coordinate, budget) in &adapter.budgets {
        let local_budget = local_coordinate(alias, prefix, coordinate)?;
        context = context
            .with_budget(local_budget.clone(), budget.clone())
            .with_pure_helper_cost(
                local_budget,
                PureHelperCostFact {
                    lawpack: lawpack.clone(),
                    coordinate: coordinate.clone(),
                    budget: budget.clone(),
                },
            );
    }
    Ok(context)
}

fn exported_type_shapes(
    bundle: &ValidatedLawpackBundle,
    lawpack: &ResourceRef,
) -> BTreeMap<String, TypeShapeFact> {
    bundle
        .exports()
        .types
        .iter()
        .map(|exported_type| {
            (
                exported_type.coordinate.clone(),
                TypeShapeFact {
                    lawpack: lawpack.clone(),
                    coordinate: exported_type.coordinate.clone(),
                    definition: exported_type.definition.clone(),
                },
            )
        })
        .collect()
}

fn exported_effect_names(
    bundle: &ValidatedLawpackBundle,
    alias: &str,
    prefix: &str,
) -> Result<BTreeSet<String>, Vec<LawpackAdapterFailure>> {
    bundle
        .exports()
        .effects
        .iter()
        .map(|effect| local_coordinate(alias, prefix, &effect.coordinate))
        .collect()
}

fn project_effects(
    mut compiler_context: CompilerContext,
    bundle: &ValidatedLawpackBundle,
    adapter: &ValidatedLawpackAdapter,
    lawpack: &ResourceRef,
    alias: &str,
    prefix: &str,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
) -> Result<ProjectedEffects, Vec<LawpackAdapterFailure>> {
    compiler_context = compiler_context.with_authenticated_effect_exports(
        alias.to_owned(),
        lawpack.clone(),
        exported_effect_names(bundle, alias, prefix)?,
    );
    let mut obstruction_coordinates = BTreeSet::new();
    let mut effect_lowerings = Vec::new();
    let mut effect_signatures = Vec::new();
    for (coordinate, effect) in &adapter.effects {
        let local_effect = local_coordinate(alias, prefix, coordinate)?;
        let exported = bundle
            .exports()
            .effects
            .iter()
            .find(|exported| exported.coordinate == *coordinate)
            .ok_or_else(|| {
                one(failure(
                    LawpackAdapterFailureKind::MissingEffectImplementation,
                    format!("exports.effects.{coordinate}"),
                    "matching exported semantic effect",
                ))
            })?;
        let mut signature_types = vec![exported.input_type.clone(), exported.output_type.clone()];
        signature_types.extend(
            exported
                .effect_failures
                .values()
                .map(|failure| failure.payload_type.clone()),
        );
        let type_closure = imported_type_core_closure(&signature_types, type_shapes, lawpack)
            .ok_or_else(|| {
                one(failure(
                    LawpackAdapterFailureKind::InvalidShape,
                    format!("exports.effects.{coordinate}"),
                    "input and output types in the exact exported bounded type closure",
                ))
            })?;
        let failure_payload_types = exported
            .effect_failures
            .iter()
            .map(|(failure, definition)| (failure.clone(), definition.payload_type.clone()))
            .collect::<BTreeMap<_, _>>();
        compiler_context = compiler_context
            .with_effect_write_class(local_effect.clone(), effect.write_class.clone())
            .with_effect_signature(
                local_effect.clone(),
                EffectSignatureFact {
                    lawpack: lawpack.clone(),
                    coordinate: exported.coordinate.clone(),
                    type_parameters: exported.type_parameters.clone(),
                    input_type: exported.input_type.clone(),
                    output_type: exported.output_type.clone(),
                    failure_payload_types: failure_payload_types.clone(),
                    domain_mappable_failures: exported
                        .effect_failures
                        .iter()
                        .filter(|(_, failure)| {
                            failure.authority_class == LawpackAuthorityClass::DomainMappable
                        })
                        .map(|(name, _)| name.clone())
                        .collect(),
                },
            );
        effect_signatures.push(TargetEffectSignatureFact::from_validated_lawpack_export(
            lawpack.clone(),
            local_effect.clone(),
            TargetEffectSignatureShape {
                coordinate: exported.coordinate.clone(),
                type_parameters: exported.type_parameters.clone(),
                input_type: exported.input_type.clone(),
                output_type: exported.output_type.clone(),
                failure_payload_types,
            },
            type_closure,
        ));
        obstruction_coordinates.extend(effect.failure_mappings.values().cloned());
        effect_lowerings.push(TargetEffectLowering {
            effect: local_effect,
            target_intrinsic: effect.target_intrinsic.clone(),
            failure_mappings: effect.failure_mappings.clone(),
        });
    }

    Ok(ProjectedEffects {
        compiler_context,
        obstruction_coordinates,
        effect_lowerings,
        effect_signatures,
    })
}

fn project_pure_functions(
    mut context: CompilerContext,
    bundle: &ValidatedLawpackBundle,
    adapter: &ValidatedLawpackAdapter,
    lawpack: &ResourceRef,
    alias: &str,
    prefix: &str,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
) -> Result<(CompilerContext, Vec<TargetPureFunctionFact>), Vec<LawpackAdapterFailure>> {
    let mut target_facts = Vec::new();
    for function in &bundle.exports().pure_functions {
        let local_function = local_coordinate(alias, prefix, &function.coordinate)?;
        let mut signature_types = function.parameter_types.clone();
        signature_types.push(function.return_type.clone());
        let type_closure = imported_type_core_closure(&signature_types, type_shapes, lawpack)
            .ok_or_else(|| {
                one(failure(
                    LawpackAdapterFailureKind::InvalidShape,
                    format!("exports.pureFunctions.{}", function.coordinate),
                    "parameter and return types in the exact exported bounded type closure",
                ))
            })?;
        target_facts.push(TargetPureFunctionFact::from_validated_lawpack_export(
            lawpack.clone(),
            function.coordinate.clone(),
            function.type_parameters.clone(),
            function.parameter_types.clone(),
            function.return_type.clone(),
            type_closure,
            adapter
                .budgets
                .get(&function.cost_template)
                .cloned()
                .ok_or_else(|| {
                    one(failure(
                        LawpackAdapterFailureKind::InvalidShape,
                        format!("exports.pureFunctions.{}", function.coordinate),
                        "validated exported helper budget",
                    ))
                })?,
        ));
        context = context.with_pure_function(
            local_function,
            PureFunctionFact {
                lawpack: lawpack.clone(),
                coordinate: function.coordinate.clone(),
                type_parameters: function.type_parameters.clone(),
                parameter_types: function.parameter_types.clone(),
                return_type: function.return_type.clone(),
                cost_template: function.cost_template.clone(),
            },
        );
    }
    Ok((context, target_facts))
}

fn project_bound_constants(
    mut context: CompilerContext,
    bundle: &ValidatedLawpackBundle,
    lawpack: &ResourceRef,
    alias: &str,
    prefix: &str,
) -> Result<CompilerContext, Vec<LawpackAdapterFailure>> {
    for constant in &bundle.exports().constants {
        if !matches!(constant.ty.as_str(), "U32" | "U64") {
            continue;
        }
        let CanonicalValue::Integer(value) = &constant.value else {
            return Err(invalid_numeric_constant(constant, "canonical integer"));
        };
        let Ok(value) = u64::try_from(*value) else {
            return Err(invalid_numeric_constant(constant, "value in type domain"));
        };
        if constant.ty == "U32" && value > u64::from(u32::MAX) {
            return Err(invalid_numeric_constant(constant, "value in U32 domain"));
        }
        context = context.with_bound(
            local_coordinate(alias, prefix, &constant.coordinate)?,
            BoundFact {
                lawpack: lawpack.clone(),
                coordinate: constant.coordinate.clone(),
                value,
            },
        );
    }
    Ok(context)
}

fn invalid_numeric_constant(
    constant: &crate::lawpack::LawpackExportedConstant,
    obligation: &str,
) -> Vec<LawpackAdapterFailure> {
    one(failure(
        LawpackAdapterFailureKind::InvalidShape,
        format!("exports.constants.{}.value", constant.coordinate),
        format!("{obligation} for {}", constant.ty),
    ))
}

type AdapterParts = (
    BTreeMap<String, LawpackAdapterOperationProfile>,
    BTreeMap<String, LawpackAdapterEffect>,
    BTreeMap<String, CoreBudget>,
);

fn parse_adapter(value: &CanonicalValue) -> Result<AdapterParts, Vec<LawpackAdapterFailure>> {
    let fields = closed_map(
        value,
        "adapter",
        &[
            "apiVersion",
            "class",
            "operationProfiles",
            "effectImplementations",
            "budgets",
        ],
    )?;
    let api_version = required_text(&fields, "apiVersion", "adapter")?;
    if api_version != LAWPACK_ADAPTER_API_VERSION {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidApiVersion,
            "adapter.apiVersion",
            LAWPACK_ADAPTER_API_VERSION,
        )));
    }
    if required_text(&fields, "class", "adapter")? != "declarative" {
        return Err(one(failure(
            LawpackAdapterFailureKind::UnsupportedClass,
            "adapter.class",
            "declarative",
        )));
    }
    Ok((
        parse_operation_profiles(required(&fields, "operationProfiles", "adapter")?)?,
        parse_effects(required(&fields, "effectImplementations", "adapter")?)?,
        parse_budgets(required(&fields, "budgets", "adapter")?)?,
    ))
}

fn parse_operation_profiles(
    value: &CanonicalValue,
) -> Result<BTreeMap<String, LawpackAdapterOperationProfile>, Vec<LawpackAdapterFailure>> {
    let values = text_map(value, "adapter.operationProfiles")?;
    let mut profiles = BTreeMap::new();
    for (coordinate, value) in values {
        let path = format!("adapter.operationProfiles.{coordinate}");
        let fields = closed_map(
            value,
            &path,
            &[
                "core",
                "semanticEffects",
                "budgetObligation",
                "targetConfiguration",
            ],
        )?;
        let core = required_nonempty_text(&fields, "core", &path)?;
        let semantic_effects = text_array(
            required(&fields, "semanticEffects", &path)?,
            &format!("{path}.semanticEffects"),
            false,
        )?;
        let budget_obligation = fields
            .get("budgetObligation")
            .map(|value| nonempty_text(value, &format!("{path}.budgetObligation")))
            .transpose()?;
        let target_configuration = fields
            .get("targetConfiguration")
            .map(|value| parse_resource_ref(value, &format!("{path}.targetConfiguration")))
            .transpose()?;
        profiles.insert(
            coordinate,
            LawpackAdapterOperationProfile {
                core,
                semantic_effects,
                budget_obligation,
                target_configuration,
            },
        );
    }
    Ok(profiles)
}

fn parse_effects(
    value: &CanonicalValue,
) -> Result<BTreeMap<String, LawpackAdapterEffect>, Vec<LawpackAdapterFailure>> {
    let values = text_map(value, "adapter.effectImplementations")?;
    let mut effects = BTreeMap::new();
    for (coordinate, value) in values {
        let path = format!("adapter.effectImplementations.{coordinate}");
        let fields = closed_map(
            value,
            &path,
            &[
                "targetIntrinsic",
                "targetConfiguration",
                "writeClass",
                "footprintObligation",
                "costObligation",
                "failureMappings",
            ],
        )?;
        let failure_values = text_map(
            required(&fields, "failureMappings", &path)?,
            &format!("{path}.failureMappings"),
        )?;
        let mut failure_mappings = BTreeMap::new();
        for (failure_coordinate, target_obstruction) in failure_values {
            failure_mappings.insert(
                failure_coordinate,
                nonempty_text(target_obstruction, &format!("{path}.failureMappings"))?,
            );
        }
        effects.insert(
            coordinate,
            LawpackAdapterEffect {
                target_intrinsic: required_nonempty_text(&fields, "targetIntrinsic", &path)?,
                target_configuration: parse_resource_ref(
                    required(&fields, "targetConfiguration", &path)?,
                    &format!("{path}.targetConfiguration"),
                )?,
                write_class: parse_write_class(
                    &required_text(&fields, "writeClass", &path)?,
                    &format!("{path}.writeClass"),
                )?,
                footprint_obligation: required_nonempty_text(
                    &fields,
                    "footprintObligation",
                    &path,
                )?,
                cost_obligation: required_nonempty_text(&fields, "costObligation", &path)?,
                failure_mappings,
            },
        );
    }
    Ok(effects)
}

fn parse_budgets(
    value: &CanonicalValue,
) -> Result<BTreeMap<String, CoreBudget>, Vec<LawpackAdapterFailure>> {
    let values = text_map(value, "adapter.budgets")?;
    let mut budgets = BTreeMap::new();
    for (coordinate, value) in values {
        let path = format!("adapter.budgets.{coordinate}");
        let fields = closed_map(
            value,
            &path,
            &["maxSteps", "maxAllocatedBytes", "maxOutputBytes"],
        )?;
        budgets.insert(
            coordinate,
            CoreBudget {
                max_steps: required_u64(&fields, "maxSteps", &path)?,
                max_allocated_bytes: required_u64(&fields, "maxAllocatedBytes", &path)?,
                max_output_bytes: required_u64(&fields, "maxOutputBytes", &path)?,
            },
        );
    }
    Ok(budgets)
}

fn validate_adapter_closure(
    bundle: &ValidatedLawpackBundle,
    descriptor: &LawpackTargetAdapter,
    operation_profiles: &BTreeMap<String, LawpackAdapterOperationProfile>,
    effects: &BTreeMap<String, LawpackAdapterEffect>,
    budgets: &BTreeMap<String, CoreBudget>,
) -> Result<(), Vec<LawpackAdapterFailure>> {
    let exported_profiles = &bundle.exports().operation_profiles;
    exact_keys(
        exported_profiles.keys().map(String::as_str),
        operation_profiles.keys().map(String::as_str),
        LawpackAdapterFailureKind::MissingOperationProfile,
        LawpackAdapterFailureKind::UnknownOperationProfile,
        "adapter.operationProfiles",
    )?;

    let runtime_effects = bundle
        .exports()
        .effects
        .iter()
        .filter(|effect| effect.execution_class == LawpackExecutionClass::Runtime)
        .map(|effect| (effect.coordinate.as_str(), effect))
        .collect::<BTreeMap<_, _>>();
    exact_keys(
        runtime_effects.keys().copied(),
        effects.keys().map(String::as_str),
        LawpackAdapterFailureKind::MissingEffectImplementation,
        LawpackAdapterFailureKind::UnknownEffectImplementation,
        "adapter.effectImplementations",
    )?;

    let intrinsic_prefix = format!("{}.", descriptor.accepted_target_profile.id);
    let mut required_budgets = bundle
        .exports()
        .pure_functions
        .iter()
        .map(|function| function.cost_template.as_str())
        .collect::<BTreeSet<_>>();
    for (coordinate, effect) in effects {
        let exported = runtime_effects.get(coordinate.as_str()).ok_or_else(|| {
            one(failure(
                LawpackAdapterFailureKind::UnknownEffectImplementation,
                format!("adapter.effectImplementations.{coordinate}"),
                "exported runtime semantic effect",
            ))
        })?;
        validate_effect(coordinate, exported, effect, &intrinsic_prefix)?;
        required_budgets.insert(effect.cost_obligation.as_str());
    }

    let mut core_profile_owners = BTreeMap::new();
    for (coordinate, profile) in operation_profiles {
        if let Some(budget) = &profile.budget_obligation {
            required_budgets.insert(budget);
        } else if profile.semantic_effects.is_empty() {
            return Err(one(failure(
                LawpackAdapterFailureKind::MissingBudget,
                format!("adapter.operationProfiles.{coordinate}.budgetObligation"),
                "request-only profile budget obligation",
            )));
        }
        if profile.semantic_effects.is_empty() && profile.target_configuration.is_none() {
            return Err(one(failure(
                LawpackAdapterFailureKind::InvalidTargetConfiguration,
                format!("adapter.operationProfiles.{coordinate}.targetConfiguration"),
                "request-only profile target configuration",
            )));
        }
        let mut seen = BTreeSet::new();
        for effect in &profile.semantic_effects {
            if !seen.insert(effect) {
                return Err(one(failure(
                    LawpackAdapterFailureKind::DuplicateReference,
                    format!("adapter.operationProfiles.{coordinate}.semanticEffects"),
                    "unique semantic effect references",
                )));
            }
            if !effects.contains_key(effect) {
                return Err(one(failure(
                    LawpackAdapterFailureKind::MissingEffectImplementation,
                    format!("adapter.operationProfiles.{coordinate}.semanticEffects"),
                    effect,
                )));
            }
        }
        if let Some(first_owner) = core_profile_owners.insert(&profile.core, coordinate) {
            return Err(one(failure(
                LawpackAdapterFailureKind::DuplicateReference,
                format!("adapter.operationProfiles.{coordinate}.core"),
                format!("unique Core profile mapping; first mapped by `{first_owner}`"),
            )));
        }
    }
    exact_keys(
        required_budgets,
        budgets.keys().map(String::as_str),
        LawpackAdapterFailureKind::MissingBudget,
        LawpackAdapterFailureKind::UnknownBudget,
        "adapter.budgets",
    )?;
    Ok(())
}

fn validate_effect(
    coordinate: &str,
    exported: &LawpackSemanticEffect,
    effect: &LawpackAdapterEffect,
    intrinsic_prefix: &str,
) -> Result<(), Vec<LawpackAdapterFailure>> {
    let path = format!("adapter.effectImplementations.{coordinate}");
    if !effect.target_intrinsic.starts_with(intrinsic_prefix)
        || effect.target_intrinsic.len() == intrinsic_prefix.len()
    {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidTargetIntrinsic,
            format!("{path}.targetIntrinsic"),
            format!(
                "intrinsic below `{}`",
                intrinsic_prefix.trim_end_matches('.')
            ),
        )));
    }
    if effect.footprint_obligation != exported.footprint_obligation {
        return Err(one(failure(
            LawpackAdapterFailureKind::ObligationMismatch,
            format!("{path}.footprintObligation"),
            &exported.footprint_obligation,
        )));
    }
    if effect.cost_obligation != exported.cost_obligation {
        return Err(one(failure(
            LawpackAdapterFailureKind::ObligationMismatch,
            format!("{path}.costObligation"),
            &exported.cost_obligation,
        )));
    }
    let exported_failures = exported
        .effect_failures
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mapped_failures = effect
        .failure_mappings
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if exported_failures != mapped_failures {
        return Err(one(failure(
            LawpackAdapterFailureKind::FailureMappingMismatch,
            format!("{path}.failureMappings"),
            "exact exported named-failure set",
        )));
    }
    let target_failures = effect
        .failure_mappings
        .values()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if target_failures.len() != effect.failure_mappings.len() {
        return Err(one(failure(
            LawpackAdapterFailureKind::FailureMappingMismatch,
            format!("{path}.failureMappings"),
            "unique target obstruction coordinates",
        )));
    }
    Ok(())
}

fn select_descriptor(
    bundle: &ValidatedLawpackBundle,
    target_profile: &str,
) -> Result<LawpackTargetAdapter, Vec<LawpackAdapterFailure>> {
    let matches = bundle
        .manifest()
        .target_adapters
        .iter()
        .filter(|adapter| adapter.accepted_target_profile.id == target_profile)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [descriptor] => Ok((*descriptor).clone()),
        [] => Err(one(failure(
            LawpackAdapterFailureKind::MissingTargetAdapter,
            "manifest.targetAdapters",
            format!("one adapter for `{target_profile}`"),
        ))),
        _ => Err(one(failure(
            LawpackAdapterFailureKind::AmbiguousTargetAdapter,
            "manifest.targetAdapters",
            format!("one adapter for `{target_profile}`"),
        ))),
    }
}

fn matching_import_aliases(
    module: &Module,
    bundle: &ValidatedLawpackBundle,
) -> Result<Vec<String>, Vec<LawpackAdapterFailure>> {
    let manifest_coordinate = format!("{}@{}", bundle.manifest().id, bundle.manifest().version);
    let manifest_digest = bundle.manifest_digest_review_string();
    let matches = module
        .imports
        .iter()
        .filter(|import| {
            import.kind == ImportKind::Lawpack
                && import.package.as_ref().is_some_and(|package| {
                    format!("{}@{}", package.path.join("."), package.version) == manifest_coordinate
                })
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Err(one(failure(
            LawpackAdapterFailureKind::SourceImportMismatch,
            "module.imports",
            format!("at least one import of `{manifest_coordinate}`"),
        )));
    }
    let mut aliases = BTreeSet::new();
    for import in matches {
        if !aliases.insert(import.alias.clone())
            || import.digest.as_deref() != Some(manifest_digest.as_str())
        {
            return Err(one(failure(
            LawpackAdapterFailureKind::SourceImportMismatch,
            format!("module.imports.{}.digest", import.alias),
            format!(
                "{LAWPACK_API_VERSION} domain-framed manifest digest `{manifest_digest}` (not the raw manifest.cbor file hash)"
            ),
        )));
        }
    }
    Ok(aliases.into_iter().collect())
}

fn local_coordinate(
    alias: &str,
    canonical_prefix: &str,
    coordinate: &str,
) -> Result<String, Vec<LawpackAdapterFailure>> {
    let Some(suffix) = coordinate.strip_prefix(canonical_prefix) else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            coordinate,
            format!(
                "coordinate below `{}`",
                canonical_prefix.trim_end_matches('.')
            ),
        )));
    };
    if suffix.is_empty() {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            coordinate,
            "non-empty exported coordinate suffix",
        )));
    }
    Ok(format!("{alias}.{suffix}"))
}

fn exact_keys<'a>(
    expected: impl IntoIterator<Item = &'a str>,
    actual: impl IntoIterator<Item = &'a str>,
    missing_kind: LawpackAdapterFailureKind,
    unknown_kind: LawpackAdapterFailureKind,
    path: &str,
) -> Result<(), Vec<LawpackAdapterFailure>> {
    let expected = expected.into_iter().collect::<BTreeSet<_>>();
    let actual = actual.into_iter().collect::<BTreeSet<_>>();
    if let Some(missing) = expected.difference(&actual).next() {
        return Err(one(failure(
            missing_kind,
            path,
            format!("include `{missing}`"),
        )));
    }
    if let Some(unknown) = actual.difference(&expected).next() {
        return Err(one(failure(
            unknown_kind,
            path,
            format!("exclude unknown `{unknown}`"),
        )));
    }
    Ok(())
}

fn parse_write_class(value: &str, path: &str) -> Result<WriteClass, Vec<LawpackAdapterFailure>> {
    match value {
        "none" => Ok(WriteClass::None),
        "read" => Ok(WriteClass::Read),
        "create" => Ok(WriteClass::Create),
        "ensure" => Ok(WriteClass::Ensure),
        "append" => Ok(WriteClass::Append),
        "replace" => Ok(WriteClass::Replace),
        "delete" => Ok(WriteClass::Delete),
        "custom" => Ok(WriteClass::Custom("custom".to_owned())),
        _ => Err(one(failure(
            LawpackAdapterFailureKind::InvalidWriteClass,
            path,
            "v1 authority write class",
        ))),
    }
}

fn parse_resource_ref(
    value: &CanonicalValue,
    path: &str,
) -> Result<LawpackResourceRef, Vec<LawpackAdapterFailure>> {
    let fields = closed_map(value, path, &["id", "digest"])?;
    Ok(LawpackResourceRef {
        id: required_nonempty_text(&fields, "id", path)?,
        digest: parse_digest(
            required(&fields, "digest", path)?,
            &format!("{path}.digest"),
        )?,
    })
}

fn parse_digest(
    value: &CanonicalValue,
    path: &str,
) -> Result<[u8; 32], Vec<LawpackAdapterFailure>> {
    let CanonicalValue::Array(parts) = value else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidTargetConfiguration,
            path,
            "['sha256', 32-byte bstr]",
        )));
    };
    let [CanonicalValue::Text(algorithm), CanonicalValue::Bytes(bytes)] = parts.as_slice() else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidTargetConfiguration,
            path,
            "['sha256', 32-byte bstr]",
        )));
    };
    let Ok(digest) = <[u8; 32]>::try_from(bytes.as_slice()) else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidTargetConfiguration,
            path,
            "['sha256', 32-byte bstr]",
        )));
    };
    if algorithm != "sha256" {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidTargetConfiguration,
            path,
            "['sha256', 32-byte bstr]",
        )));
    }
    Ok(digest)
}

fn digest_value(
    domain: &str,
    value: &CanonicalValue,
) -> Result<[u8; 32], Vec<LawpackAdapterFailure>> {
    digest_canonical_value(domain, value).map_err(|_| {
        one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            ADAPTER_PATH,
            "canonically encodable adapter digest frame",
        ))
    })
}

fn closed_map<'a>(
    value: &'a CanonicalValue,
    path: &str,
    allowed: &[&str],
) -> Result<BTreeMap<&'a str, &'a CanonicalValue>, Vec<LawpackAdapterFailure>> {
    let fields = text_map_ref(value, path)?;
    if let Some(unexpected) = fields.keys().find(|key| !allowed.contains(key)) {
        return Err(one(failure(
            LawpackAdapterFailureKind::UnexpectedField,
            format!("{path}.{unexpected}"),
            "closed map",
        )));
    }
    Ok(fields)
}

fn text_map<'a>(
    value: &'a CanonicalValue,
    path: &str,
) -> Result<BTreeMap<String, &'a CanonicalValue>, Vec<LawpackAdapterFailure>> {
    Ok(text_map_ref(value, path)?
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect())
}

fn text_map_ref<'a>(
    value: &'a CanonicalValue,
    path: &str,
) -> Result<BTreeMap<&'a str, &'a CanonicalValue>, Vec<LawpackAdapterFailure>> {
    let CanonicalValue::Map(entries) = value else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            path,
            "map",
        )));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in entries {
        let CanonicalValue::Text(key) = key else {
            return Err(one(failure(
                LawpackAdapterFailureKind::InvalidShape,
                path,
                "text-keyed map",
            )));
        };
        fields.insert(key.as_str(), value);
    }
    Ok(fields)
}

fn required<'a>(
    fields: &BTreeMap<&str, &'a CanonicalValue>,
    field: &str,
    path: &str,
) -> Result<&'a CanonicalValue, Vec<LawpackAdapterFailure>> {
    fields.get(field).copied().ok_or_else(|| {
        one(failure(
            LawpackAdapterFailureKind::MissingField,
            format!("{path}.{field}"),
            "required field",
        ))
    })
}

fn required_text(
    fields: &BTreeMap<&str, &CanonicalValue>,
    field: &str,
    path: &str,
) -> Result<String, Vec<LawpackAdapterFailure>> {
    text(required(fields, field, path)?, &format!("{path}.{field}"))
}

fn required_nonempty_text(
    fields: &BTreeMap<&str, &CanonicalValue>,
    field: &str,
    path: &str,
) -> Result<String, Vec<LawpackAdapterFailure>> {
    nonempty_text(required(fields, field, path)?, &format!("{path}.{field}"))
}

fn required_u64(
    fields: &BTreeMap<&str, &CanonicalValue>,
    field: &str,
    path: &str,
) -> Result<u64, Vec<LawpackAdapterFailure>> {
    let value = required(fields, field, path)?;
    let CanonicalValue::Integer(value) = value else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            format!("{path}.{field}"),
            "unsigned integer",
        )));
    };
    u64::try_from(*value).map_err(|_| {
        one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            format!("{path}.{field}"),
            "u64 value",
        ))
    })
}

fn text(value: &CanonicalValue, path: &str) -> Result<String, Vec<LawpackAdapterFailure>> {
    let CanonicalValue::Text(value) = value else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            path,
            "text",
        )));
    };
    Ok(value.clone())
}

fn nonempty_text(value: &CanonicalValue, path: &str) -> Result<String, Vec<LawpackAdapterFailure>> {
    let value = text(value, path)?;
    if value.is_empty() {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            path,
            "non-empty text",
        )));
    }
    Ok(value)
}

fn text_array(
    value: &CanonicalValue,
    path: &str,
    nonempty: bool,
) -> Result<Vec<String>, Vec<LawpackAdapterFailure>> {
    let CanonicalValue::Array(values) = value else {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            path,
            "array",
        )));
    };
    if nonempty && values.is_empty() {
        return Err(one(failure(
            LawpackAdapterFailureKind::InvalidShape,
            path,
            "non-empty array",
        )));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| nonempty_text(value, &format!("{path}[{index}]")))
        .collect()
}

fn failure(
    kind: LawpackAdapterFailureKind,
    path: impl Into<String>,
    obligation: impl Into<String>,
) -> LawpackAdapterFailure {
    LawpackAdapterFailure {
        kind,
        path: path.into(),
        obligation: obligation.into(),
    }
}

fn one(failure: LawpackAdapterFailure) -> Vec<LawpackAdapterFailure> {
    vec![failure]
}
