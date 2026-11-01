//! Executable compiler-spine stages from source AST to in-memory Core IR.
//!
//! This module deliberately stops before canonical encoding, hashing, target
//! lowering, and admission. Those are later stages with separate evidence.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    BinOp, Block, BoundRef, BytesRefine, Decl, DigestLockedPackageRef, ElseClause, Expr, FieldDecl,
    FunctionDecl, Import, ImportKind, IntentClause, IntentDecl, Module, ObstructionArm,
    ObstructionHandler, ObstructionTarget, RecordEntry, RequireElseArm, ScalarRefine, Stmt,
    TypeDecl, TypeExpr, TypeRef, UnOp, YieldBlock,
};
use crate::core_ir::{
    classify_core_type_reference, is_lowercase_sha256_review_digest, parse_core_integer,
    render_self_describing_core_type, validate_core_module_type_integrity, CompareOp, CoreBlock,
    CoreBound, CoreBudget, CoreExpr, CoreExternalActionBudget, CoreFunction, CoreImport,
    CoreImportKind, CoreIntent, CoreModule, CoreNode, CoreObstructionArm, CoreObstructionReason,
    CorePredicate, CorePureBinding, CorePureBlock, CoreRequireFailureArm, CoreType,
    CoreTypeReference, CoreValue, InputConstraint, InputConstraintSource, LocalRef, ResourceRef,
    CORE_API_VERSION, CORE_APPLICATION_INPUT_LOCAL_ID, MAX_CORE_TYPE_DEPTH,
};
mod byte_at;
mod byte_concat;
mod byte_count;
mod byte_length;
mod byte_slice;
mod byte_utf8;
mod list_literal;
mod list_operations;
mod return_budget;
mod signature_diagnostic;
mod source_fold;
mod source_functions;
mod source_matches;
mod source_types;
mod source_variants;
mod target_pure_intrinsic;
pub use signature_diagnostic::{
    EffectSignatureMismatch, SignatureMismatchPosition, SignaturePathSegment,
};
mod unsigned_addition;
mod unsigned_subtraction;

use crate::lowerability::WriteClass;
use crate::semantic::validate_surface;
use crate::target_intrinsics::TargetPureIntrinsicFact;
use crate::token::{IntSuffix, Span};

/// Compiler stage that reported an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerStage {
    SurfaceValidation,
    Resolve,
    TypeCheck,
    LowerCore,
}

/// Stable compiler-spine error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerErrorKind {
    SurfaceValidation,
    MissingContextFact,
    UnsupportedSourceShape,
    UnresolvedType,
    UnresolvedFunction,
    EffectWithoutFailureMapping,
    InvalidBound,
    UnknownField,
    TypeMismatch,
    ExpectedPredicate,
    ProfileEffectMismatch,
    UnrequestableExternalOperation,
    DuplicateObstructionFailure,
    DuplicateObstructionPayloadField,
    ReservedTypeIdentity,
    InvalidCoreTypeIntegrity,
}

/// Deterministic signature and canonical identity for one imported pure helper.
///
/// The owning digest-bound lawpack remains the source of executable helper
/// semantics. This fact only makes its reviewed signature and canonical export
/// coordinate available to source resolution and type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PureFunctionFact {
    pub lawpack: ResourceRef,
    pub coordinate: String,
    pub type_parameters: Vec<String>,
    pub parameter_types: Vec<String>,
    pub return_type: String,
    pub cost_template: String,
}

/// Authenticated signature for one semantic effect imported from a lawpack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffectSignatureFact {
    pub(crate) lawpack: ResourceRef,
    pub(crate) coordinate: String,
    pub(crate) type_parameters: Vec<String>,
    pub(crate) input_type: String,
    pub(crate) output_type: String,
    pub(crate) failure_payload_types: BTreeMap<String, String>,
    pub(crate) domain_mappable_failures: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthenticatedEffectExports {
    lawpack: ResourceRef,
    names: BTreeSet<String>,
}

/// Canonical identity and Core definition for one imported lawpack type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeShapeFact {
    pub lawpack: ResourceRef,
    pub coordinate: String,
    pub definition: String,
}

/// Canonical identity plus numeric value for one imported static bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundFact {
    pub lawpack: ResourceRef,
    pub coordinate: String,
    pub value: u64,
}

/// Conservative pure-helper cost owned by one exact imported lawpack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PureHelperCostFact {
    pub lawpack: ResourceRef,
    pub coordinate: String,
    pub budget: CoreBudget,
}

/// A compiler-spine failure with stable stage/kind identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerError {
    pub stage: CompilerStage,
    pub kind: CompilerErrorKind,
    pub message: String,
    pub span: Span,
    /// Optional typed detail for an exported effect signature mismatch.
    pub signature_mismatch: Option<EffectSignatureMismatch>,
}

/// Deterministic facts supplied to the compiler spine by the caller.
///
/// The resolver does not invent target/lawpack facts. Source profile and budget
/// coordinates must be bound here before they can lower into Core.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompilerContext {
    operation_profiles: BTreeMap<String, String>,
    operation_profile_write_classes: BTreeMap<String, BTreeSet<WriteClass>>,
    operation_profile_budgets: BTreeMap<String, String>,
    effect_write_classes: BTreeMap<String, WriteClass>,
    effect_signatures: BTreeMap<String, EffectSignatureFact>,
    authenticated_effect_exports: BTreeMap<String, AuthenticatedEffectExports>,
    pure_functions: BTreeMap<String, PureFunctionFact>,
    pure_helper_costs: BTreeMap<String, PureHelperCostFact>,
    type_shapes: BTreeMap<String, TypeShapeFact>,
    source_type_origins: BTreeMap<String, SourceTypeOrigin>,
    bounds: BTreeMap<String, BoundFact>,
    budgets: BTreeMap<String, CoreBudget>,
    target_pure_intrinsics: Vec<TargetPureIntrinsicFact>,
}

impl CompilerContext {
    pub(crate) fn with_linked_source_types(
        mut self,
        origins: BTreeMap<String, SourceTypeOrigin>,
    ) -> Self {
        self.source_type_origins = origins;
        self
    }

    pub(crate) fn has_imported_type_coordinate(&self, coordinate: &str) -> bool {
        self.type_shapes.contains_key(coordinate)
    }

    pub(crate) fn has_imported_callable_coordinate(&self, coordinate: &str) -> bool {
        self.pure_functions
            .values()
            .any(|fact| fact.coordinate == coordinate)
            || self
                .effect_signatures
                .values()
                .any(|fact| fact.coordinate == coordinate)
            || self
                .target_pure_intrinsics
                .iter()
                .any(|fact| fact.coordinate() == coordinate)
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_operation_profile(
        mut self,
        source_coordinate: impl Into<String>,
        core_profile: impl Into<String>,
    ) -> Self {
        self.operation_profiles
            .insert(source_coordinate.into(), core_profile.into());
        self
    }

    #[must_use]
    pub fn with_operation_profile_write_classes<I>(
        mut self,
        source_coordinate: impl Into<String>,
        write_classes: I,
    ) -> Self
    where
        I: IntoIterator<Item = WriteClass>,
    {
        self.operation_profile_write_classes.insert(
            source_coordinate.into(),
            write_classes.into_iter().collect(),
        );
        self
    }

    #[must_use]
    pub fn with_operation_profile_budget(
        mut self,
        source_profile: impl Into<String>,
        source_budget: impl Into<String>,
    ) -> Self {
        self.operation_profile_budgets
            .insert(source_profile.into(), source_budget.into());
        self
    }

    #[must_use]
    pub fn with_effect_write_class(
        mut self,
        source_effect_coordinate: impl Into<String>,
        write_class: WriteClass,
    ) -> Self {
        self.effect_write_classes
            .insert(source_effect_coordinate.into(), write_class);
        self
    }

    #[must_use]
    pub(crate) fn with_authenticated_effect_exports(
        mut self,
        alias: String,
        lawpack: ResourceRef,
        names: BTreeSet<String>,
    ) -> Self {
        self.authenticated_effect_exports
            .insert(alias, AuthenticatedEffectExports { lawpack, names });
        self
    }

    #[must_use]
    pub(crate) fn with_effect_signature(
        mut self,
        source_effect_coordinate: impl Into<String>,
        fact: EffectSignatureFact,
    ) -> Self {
        self.effect_signatures
            .insert(source_effect_coordinate.into(), fact);
        self
    }

    #[must_use]
    pub fn with_pure_function(
        mut self,
        source_coordinate: impl Into<String>,
        fact: PureFunctionFact,
    ) -> Self {
        self.pure_functions.insert(source_coordinate.into(), fact);
        self
    }

    #[must_use]
    pub fn with_pure_helper_cost(
        mut self,
        source_coordinate: impl Into<String>,
        fact: PureHelperCostFact,
    ) -> Self {
        self.pure_helper_costs
            .insert(source_coordinate.into(), fact);
        self
    }

    #[must_use]
    pub fn with_type_shape(mut self, fact: TypeShapeFact) -> Self {
        self.type_shapes.insert(fact.coordinate.clone(), fact);
        self
    }

    #[must_use]
    pub fn with_bound(mut self, source_coordinate: impl Into<String>, fact: BoundFact) -> Self {
        self.bounds.insert(source_coordinate.into(), fact);
        self
    }

    #[must_use]
    pub fn with_budget(mut self, source_coordinate: impl Into<String>, budget: CoreBudget) -> Self {
        self.budgets.insert(source_coordinate.into(), budget);
        self
    }

    /// Make an admitted target pure intrinsic callable through any source
    /// `use target` import that names exactly its pinned target profile.
    #[must_use]
    pub fn with_target_pure_intrinsic(mut self, fact: TargetPureIntrinsicFact) -> Self {
        self.target_pure_intrinsics.push(fact);
        self
    }
}

/// Module after resolution of names and explicit context facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModule {
    pub coordinate: String,
    pub imports: Vec<CoreImport>,
    pub effect_write_classes: BTreeMap<String, WriteClass>,
    pub(crate) effect_signatures: BTreeMap<String, EffectSignatureFact>,
    authenticated_effect_exports: BTreeMap<String, AuthenticatedEffectExports>,
    pub pure_functions: BTreeMap<String, PureFunctionFact>,
    pub pure_helper_costs: BTreeMap<String, PureHelperCostFact>,
    pub type_shapes: BTreeMap<String, TypeShapeFact>,
    pub(crate) source_type_origins: BTreeMap<String, SourceTypeOrigin>,
    pub bounds: BTreeMap<String, BoundFact>,
    pub budgets: BTreeMap<String, CoreBudget>,
    pub(crate) target_pure_intrinsics: BTreeMap<String, TargetPureIntrinsicFact>,
    /// Coordinates of every admitted target pure intrinsic, imported or not,
    /// which no source function may claim.
    pub(crate) target_intrinsic_coordinates: BTreeSet<String>,
    pub types: Vec<ResolvedTypeDecl>,
    pub source_functions: Vec<FunctionDecl>,
    pub intents: Vec<ResolvedIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceTypeOrigin {
    pub(crate) coordinate: String,
    pub(crate) table_key: String,
}

/// Resolved type declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTypeDecl {
    pub name: String,
    pub source: TypeDecl,
}

/// Resolved intent declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIntent {
    pub name: String,
    pub profile: String,
    pub allowed_write_classes: Option<BTreeSet<WriteClass>>,
    pub budget: CoreBudget,
    pub source: IntentDecl,
}

/// Module after type checking and Core-ready local normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedModule {
    /// Explicit capabilities discovered during source checking.
    pub required_core_capabilities: Vec<String>,
    pub coordinate: String,
    pub imports: Vec<CoreImport>,
    pub types: BTreeMap<String, CoreType>,
    pub functions: BTreeMap<String, CoreFunction>,
    pub intents: Vec<TypedIntent>,
}

/// Typed intent boundary consumed by `lower_core`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedIntent {
    pub name: String,
    pub input: String,
    pub output: String,
    pub profile: String,
    pub budget: CoreBudget,
    pub input_binding: LocalRef,
    pub basis: Option<CoreExpr>,
    pub input_constraints: Vec<InputConstraint>,
    pub body: CoreBlock,
}

/// Run the currently executable compiler spine.
///
/// # Errors
/// Returns structured compiler errors from the first failing stage.
pub fn compile_to_core(
    module: &Module,
    context: &CompilerContext,
) -> Result<CoreModule, Vec<CompilerError>> {
    validate_surface(module).map_err(|errors| {
        errors
            .into_iter()
            .map(|err| CompilerError {
                stage: CompilerStage::SurfaceValidation,
                kind: CompilerErrorKind::SurfaceValidation,
                message: err.message,
                span: err.span,
                signature_mismatch: None,
            })
            .collect::<Vec<_>>()
    })?;
    let resolved = resolve_module(module, context)?;
    let typed = type_check(&resolved)?;
    lower_core(&typed)
}

/// Resolve source names and explicit profile/budget facts.
///
/// # Errors
/// Returns resolver-stage errors when required deterministic context facts are
/// missing or a source construct cannot enter the initial compiler spine.
pub fn resolve_module(
    module: &Module,
    context: &CompilerContext,
) -> Result<ResolvedModule, Vec<CompilerError>> {
    let mut errors = Vec::new();
    for import in &module.imports {
        if import.kind == ImportKind::Module {
            errors.push(error(
                CompilerStage::Resolve,
                CompilerErrorKind::MissingContextFact,
                "source module requires an authenticated source closure before compilation",
                import.span,
            ));
        }
    }
    let coordinate = package_coordinate(&module.package.path, &module.package.version);
    let imports = resolve_imports(&module.imports);
    let target_pure_intrinsics = target_pure_intrinsics_by_source(&imports, context);
    let mut types = Vec::new();
    let mut intents = Vec::new();
    let mut source_functions = Vec::new();

    for decl in &module.decls {
        match decl {
            Decl::Function(decl) => source_functions.push(decl.clone()),
            Decl::Type(decl) => types.push(ResolvedTypeDecl {
                name: decl.name.clone(),
                source: decl.clone(),
            }),
            Decl::Enum(decl) => errors.push(error(
                CompilerStage::Resolve,
                CompilerErrorKind::UnsupportedSourceShape,
                "enum declarations are not in the initial lowerable subset",
                decl.span,
            )),
            Decl::Intent(intent) => {
                if let Some(resolved) = resolve_intent(intent, context, &mut errors) {
                    intents.push(resolved);
                }
            }
        }
    }

    finish(
        errors,
        ResolvedModule {
            coordinate,
            imports,
            effect_write_classes: context.effect_write_classes.clone(),
            effect_signatures: context.effect_signatures.clone(),
            authenticated_effect_exports: context.authenticated_effect_exports.clone(),
            pure_functions: context.pure_functions.clone(),
            pure_helper_costs: context.pure_helper_costs.clone(),
            type_shapes: context.type_shapes.clone(),
            source_type_origins: context.source_type_origins.clone(),
            bounds: context.bounds.clone(),
            budgets: context.budgets.clone(),
            target_intrinsic_coordinates: context
                .target_pure_intrinsics
                .iter()
                .map(|fact| fact.coordinate().to_owned())
                .collect(),
            target_pure_intrinsics,
            types,
            source_functions,
            intents,
        },
    )
}

/// Type-check a resolved module into the Core-ready typed boundary.
///
/// # Errors
/// Returns type-check-stage errors for unresolved types, incompatible values, or
/// source constructs outside the initial lowerable subset.
pub fn type_check(resolved: &ResolvedModule) -> Result<TypedModule, Vec<CompilerError>> {
    TypeChecker::new(resolved).check()
}

/// Lower a typed module into in-memory Core IR.
///
/// # Errors
/// Returns lower-core-stage errors. The current typed boundary is already
/// Core-ready, so this stage is expected to be infallible for valid
/// [`TypedModule`] values.
pub fn lower_core(typed: &TypedModule) -> Result<CoreModule, Vec<CompilerError>> {
    let intents = typed
        .intents
        .iter()
        .map(|intent| {
            (
                intent.name.clone(),
                CoreIntent {
                    input: intent.input.clone(),
                    output: intent.output.clone(),
                    required_operation_profile: intent.profile.clone(),
                    basis: intent.basis.clone(),
                    input_constraints: intent.input_constraints.clone(),
                    core_evaluation_budget: intent.budget.clone(),
                    body: intent.body.clone(),
                },
            )
        })
        .collect();
    let core = CoreModule {
        api_version: CORE_API_VERSION.to_owned(),
        coordinate: typed.coordinate.clone(),
        imports: typed.imports.clone(),
        types: typed.types.clone(),
        functions: typed.functions.clone(),
        intents,
        required_core_capabilities: if typed
            .types
            .values()
            .any(|ty| matches!(ty, CoreType::Variant { .. }))
        {
            let mut capabilities = typed.required_core_capabilities.clone();
            capabilities.push("core.variant/v1".to_owned());
            capabilities
        } else {
            typed.required_core_capabilities.clone()
        },
    };
    match validate_core_module_type_integrity(&core) {
        Ok(_) => Ok(core),
        Err(failure) => Err(vec![CompilerError {
            stage: CompilerStage::LowerCore,
            kind: CompilerErrorKind::InvalidCoreTypeIntegrity,
            message: failure.to_string(),
            span: Span::new(0, 0),
            signature_mismatch: None,
        }]),
    }
}

fn resolve_imports(imports: &[Import]) -> Vec<CoreImport> {
    let mut out = Vec::new();
    for import in imports {
        let Some(kind) = core_import_kind(import.kind) else {
            continue;
        };
        let Some(package) = &import.package else {
            continue;
        };
        out.push(CoreImport {
            kind,
            resource: ResourceRef {
                coordinate: package_coordinate(&package.path, &package.version),
                digest: import.digest.clone(),
            },
            alias: Some(import.alias.clone()),
        });
    }
    out
}

/// Source coordinates (`alias.name`) of target pure intrinsics reachable through
/// a target import whose resource is exactly the intrinsic's pinned target.
fn target_pure_intrinsics_by_source(
    imports: &[CoreImport],
    context: &CompilerContext,
) -> BTreeMap<String, TargetPureIntrinsicFact> {
    let mut out = BTreeMap::new();
    for import in imports {
        let Some(alias) = import.alias.as_deref() else {
            continue;
        };
        if import.kind != CoreImportKind::Target {
            continue;
        }
        for fact in &context.target_pure_intrinsics {
            if fact.target() != &import.resource {
                continue;
            }
            if let Some(name) = fact
                .coordinate()
                .strip_prefix(&import.resource.coordinate)
                .and_then(|suffix| suffix.strip_prefix('.'))
            {
                out.insert(format!("{alias}.{name}"), fact.clone());
            }
        }
    }
    out
}

fn core_import_kind(kind: ImportKind) -> Option<CoreImportKind> {
    match kind {
        ImportKind::Lawpack => Some(CoreImportKind::Lawpack),
        ImportKind::Target => Some(CoreImportKind::Target),
        ImportKind::Core => Some(CoreImportKind::Core),
        ImportKind::Capability => Some(CoreImportKind::Capability),
        ImportKind::Shape | ImportKind::Module => None,
    }
}

fn resolve_intent(
    intent: &IntentDecl,
    context: &CompilerContext,
    errors: &mut Vec<CompilerError>,
) -> Option<ResolvedIntent> {
    let mut profile = None;
    let mut profile_source = None;
    let mut allowed_write_classes = None;
    let mut budget = None;
    let mut budget_source = None;
    for clause in &intent.clauses {
        match clause {
            IntentClause::Profile(path) => {
                let key = path_key(path);
                match context.operation_profiles.get(&key) {
                    Some(value) => {
                        profile = Some(value.clone());
                        profile_source = Some(key.clone());
                        allowed_write_classes =
                            context.operation_profile_write_classes.get(&key).cloned();
                    }
                    None => errors.push(missing_context_fact(
                        format!("operation profile `{key}` has no compiler context fact"),
                        intent.span,
                    )),
                }
            }
            IntentClause::Budget(path) => {
                let key = path_key(path);
                match context.budgets.get(&key) {
                    Some(value) => {
                        budget = Some(value.clone());
                        budget_source = Some(key);
                    }
                    None => errors.push(missing_context_fact(
                        format!("budget `{key}` has no compiler context fact"),
                        intent.span,
                    )),
                }
            }
            IntentClause::Implements(_) => errors.push(error(
                CompilerStage::Resolve,
                CompilerErrorKind::UnsupportedSourceShape,
                "`implements` intents are outside the initial lowerable subset",
                intent.span,
            )),
            IntentClause::Basis(_) | IntentClause::Footprint(_) | IntentClause::Where(_) => {}
        }
    }

    if let (Some(profile_source), Some(budget_source)) = (&profile_source, &budget_source) {
        if let Some(required_budget) = context.operation_profile_budgets.get(profile_source) {
            if required_budget != budget_source {
                errors.push(missing_context_fact(
                    format!(
                        "profile `{profile_source}` requires budget `{required_budget}`, got `{budget_source}`"
                    ),
                    intent.span,
                ));
                return None;
            }
        }
    }

    Some(ResolvedIntent {
        name: intent.name.clone(),
        profile: profile?,
        allowed_write_classes,
        budget: budget?,
        source: intent.clone(),
    })
}

fn missing_context_fact(message: String, span: Span) -> CompilerError {
    error(
        CompilerStage::Resolve,
        CompilerErrorKind::MissingContextFact,
        message,
        span,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeShape {
    coord: String,
    kind: TypeKind,
    preserve_named_identity: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TypeKind {
    Bool,
    Int {
        width: String,
    },
    String {
        max: u64,
        canonical: String,
    },
    Bytes {
        min: Option<u64>,
        max: u64,
    },
    Nominal {
        contract: String,
        representation: Box<TypeShape>,
    },
    List {
        item: Box<TypeShape>,
        max: u64,
    },
    Record(BTreeMap<String, TypeShape>),
    Variant(BTreeMap<String, Option<TypeShape>>),
    ExternalActionRequest {
        settlement: Box<TypeShape>,
    },
}

impl TypeShape {
    fn core_type(&self) -> CoreType {
        match &self.kind {
            TypeKind::Bool => CoreType::Bool,
            TypeKind::Int { width } => CoreType::Int {
                width: width.clone(),
            },
            TypeKind::String { max, canonical } => CoreType::String {
                max: *max,
                canonical: canonical.clone(),
            },
            TypeKind::Bytes { min, max } => CoreType::Bytes {
                min: *min,
                max: *max,
            },
            TypeKind::Nominal {
                contract,
                representation,
            } => CoreType::Nominal {
                contract: contract.clone(),
                representation: representation.value_type_coord(),
            },
            TypeKind::List { item, max } => CoreType::List {
                item: item.value_type_coord(),
                max: *max,
            },
            TypeKind::Record(fields) => CoreType::Record {
                fields: fields
                    .iter()
                    .map(|(name, shape)| (name.clone(), shape.value_type_coord()))
                    .collect(),
            },
            TypeKind::Variant(cases) => CoreType::Variant {
                cases: cases
                    .iter()
                    .map(|(name, payload)| {
                        (
                            name.clone(),
                            payload.as_ref().map(TypeShape::value_type_coord),
                        )
                    })
                    .collect(),
            },
            TypeKind::ExternalActionRequest { settlement } => CoreType::ExternalActionRequest {
                settlement: settlement.value_type_coord(),
            },
        }
    }

    fn value_type_coord(&self) -> String {
        if self.preserve_named_identity {
            self.coord.clone()
        } else {
            render_self_describing_core_type(&self.core_type())
                .unwrap_or_else(|| self.coord.clone())
        }
    }

    fn canonical_structural(kind: TypeKind) -> Option<Self> {
        let mut shape = Self {
            coord: String::new(),
            kind,
            preserve_named_identity: false,
        };
        shape.coord = render_self_describing_core_type(&shape.core_type())?;
        Some(shape)
    }
}

#[derive(Debug, Clone)]
struct TypedValue {
    expr: CoreExpr,
    ty: TypeShape,
}

#[derive(Debug, Clone, Default)]
struct LocalEnvironment {
    values: BTreeMap<String, (LocalRef, TypeShape)>,
    poisoned: BTreeSet<String>,
}

impl LocalEnvironment {
    fn get(&self, name: &str) -> Option<&(LocalRef, TypeShape)> {
        self.values.get(name)
    }

    fn contains_key(&self, name: &str) -> bool {
        self.values.contains_key(name) || self.poisoned.contains(name)
    }

    fn insert(&mut self, name: String, value: (LocalRef, TypeShape)) {
        self.poisoned.remove(&name);
        self.values.insert(name, value);
    }

    fn poison(&mut self, name: &str) {
        self.values.remove(name);
        self.poisoned.insert(name.to_owned());
    }
}

#[derive(Debug, Clone)]
struct BodyState {
    nodes: Vec<CoreNode>,
    result: Option<CoreExpr>,
    saw_return: bool,
    local_index: usize,
    obstruction_index: usize,
    step_factor: u64,
    accumulated_steps: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct HelperCost {
    steps: u64,
    allocated_bytes: u64,
    output_bytes: u64,
}

impl HelperCost {
    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            steps: self.steps.checked_add(other.steps)?,
            allocated_bytes: self.allocated_bytes.checked_add(other.allocated_bytes)?,
            output_bytes: self.output_bytes.checked_add(other.output_bytes)?,
        })
    }

    fn checked_mul(self, factor: u64) -> Option<Self> {
        Some(Self {
            steps: self.steps.checked_mul(factor)?,
            allocated_bytes: self.allocated_bytes.checked_mul(factor)?,
            output_bytes: self.output_bytes.checked_mul(factor)?,
        })
    }

    fn component_max(self, other: Self) -> Self {
        Self {
            steps: self.steps.max(other.steps),
            allocated_bytes: self.allocated_bytes.max(other.allocated_bytes),
            output_bytes: self.output_bytes.max(other.output_bytes),
        }
    }

    fn from_budget(budget: &CoreBudget) -> Self {
        Self {
            steps: budget.max_steps,
            allocated_bytes: budget.max_allocated_bytes,
            output_bytes: budget.max_output_bytes,
        }
    }
}

impl Default for BodyState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            result: None,
            saw_return: false,
            local_index: 0,
            obstruction_index: 0,
            step_factor: 1,
            accumulated_steps: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReasonPayloadMode {
    PreserveReasonField,
    SkipReasonField,
}

#[derive(Debug, Clone, Copy)]
struct LetStatement<'a> {
    name: &'a str,
    ty: Option<&'a TypeRef>,
    value: &'a Expr,
    handler: Option<&'a ObstructionHandler>,
    span: Span,
}

#[derive(Debug, Clone, Copy)]
enum EffectOutputExpectation<'a> {
    Value(Option<&'a TypeShape>),
    Predicate,
    ConcatenationPeer {
        peer: &'a Expr,
        next: &'a Self,
    },
    ConditionalPeer {
        peer: &'a Expr,
        next: &'a Self,
    },
    String,
    Bytes,
    Unsigned(Option<&'a TypeShape>),
    Comparison(&'a TypeShape),
    ComparisonLiteral(&'a Expr),
    Field {
        name: &'a str,
        next: &'a Self,
        span: Span,
    },
}

impl<'a> EffectOutputExpectation<'a> {
    fn value(self) -> Option<&'a TypeShape> {
        match self {
            Self::Value(shape) | Self::Unsigned(shape) => shape,
            Self::ConditionalPeer { next, .. } => next.value(),
            Self::Predicate
            | Self::ConcatenationPeer { .. }
            | Self::String
            | Self::Bytes
            | Self::Comparison(_)
            | Self::ComparisonLiteral(_)
            | Self::Field { .. } => None,
        }
    }

    fn project<'s>(
        self,
        output: &'s TypeShape,
    ) -> Result<(&'s TypeShape, Self), (CompilerErrorKind, Span, &'a str)> {
        if let Self::Field { name, next, span } = self {
            let TypeKind::Record(fields) = &output.kind else {
                return Err((CompilerErrorKind::TypeMismatch, span, name));
            };
            let Some(field) = fields.get(name) else {
                return Err((CompilerErrorKind::UnknownField, span, name));
            };
            next.project(field)
        } else {
            Ok((output, self))
        }
    }

    fn origin(self, fallback: Span) -> Span {
        match self {
            Self::Field { next, span, .. } => next.origin(span),
            _ => fallback,
        }
    }

    fn rejection(self, output: &TypeShape) -> Option<CompilerErrorKind> {
        match self {
            Self::Value(Some(expected)) if !compatible(expected, output) => {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::ConcatenationPeer { .. }
                if !matches!(
                    output.kind,
                    TypeKind::String { .. } | TypeKind::Bytes { .. }
                ) =>
            {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::Unsigned(expected)
                if !matches!(&output.kind, TypeKind::Int { width } if width == "U32" || width == "U64")
                    || expected.is_some_and(|shape| !compatible(shape, output)) =>
            {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::String if !matches!(output.kind, TypeKind::String { .. }) => {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::Bytes if !matches!(output.kind, TypeKind::Bytes { .. }) => {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::Predicate if !matches!(output.kind, TypeKind::Bool) => {
                Some(CompilerErrorKind::ExpectedPredicate)
            }
            Self::Comparison(peer) if !comparable(peer, output) => {
                Some(CompilerErrorKind::TypeMismatch)
            }
            Self::ComparisonLiteral(_) if !matches!(output.kind, TypeKind::Int { .. }) => {
                Some(CompilerErrorKind::TypeMismatch)
            }
            _ => None,
        }
    }
}

enum DiagnosticPeerShape {
    Known(TypeShape),
    Unknown,
    Rejected,
}

#[derive(Debug, Clone)]
struct EffectBindingProfile {
    name: String,
    allowed_write_classes: Option<BTreeSet<WriteClass>>,
}

#[derive(Debug, Clone)]
struct TypeChecker<'a> {
    core_capabilities: BTreeSet<String>,
    list_work_pass: list_operations::WorkPass,
    match_depth: usize,
    match_in_intent: bool,
    resolved: &'a ResolvedModule,
    function_signatures: BTreeMap<String, source_functions::Signature>,
    function_costs: BTreeMap<String, HelperCost>,
    source_expression_costs: BTreeMap<usize, HelperCost>,
    target_pure_intrinsic_costs: BTreeMap<usize, HelperCost>,
    source_predicate_costs: BTreeMap<usize, HelperCost>,
    source_value_bounds: BTreeMap<usize, (u64, u64)>,
    /// Byte maximum of every operand coordinate a `count` call resolved, so the
    /// addition proof can bound named coordinates without a type table.
    count_operand_maxima: BTreeMap<String, u64>,
    /// Proven upper bounds of earlier unsigned `let` bindings in the body being
    /// checked, by local id. Ids are unique and bindings immutable within a body.
    local_bounds: BTreeMap<String, (LocalRef, u64)>,
    input_proof_constraints: Vec<CorePredicate>,
    effect_binding_profile: Option<EffectBindingProfile>,
    errors: Vec<CompilerError>,
    named_types: BTreeMap<String, TypeShape>,
    core_types: BTreeMap<String, CoreType>,
    inferred_yield_shapes: BTreeMap<usize, TypeShape>,
    inferred_yield_envs: BTreeMap<usize, LocalEnvironment>,
    expected_type_unavailable: bool,
    checking_diagnostic_peer: bool,
}

impl<'a> TypeChecker<'a> {
    fn new(resolved: &'a ResolvedModule) -> Self {
        Self {
            resolved,
            core_capabilities: BTreeSet::new(),
            list_work_pass: list_operations::WorkPass::Discover,
            match_depth: 0,
            match_in_intent: false,
            function_signatures: BTreeMap::new(),
            function_costs: BTreeMap::new(),
            source_expression_costs: BTreeMap::new(),
            target_pure_intrinsic_costs: BTreeMap::new(),
            source_predicate_costs: BTreeMap::new(),
            source_value_bounds: BTreeMap::new(),
            count_operand_maxima: BTreeMap::new(),
            local_bounds: BTreeMap::new(),
            input_proof_constraints: Vec::new(),
            effect_binding_profile: None,
            errors: Vec::new(),
            named_types: BTreeMap::new(),
            core_types: BTreeMap::new(),
            inferred_yield_shapes: BTreeMap::new(),
            inferred_yield_envs: BTreeMap::new(),
            expected_type_unavailable: false,
            checking_diagnostic_peer: false,
        }
    }

    fn check(mut self) -> Result<TypedModule, Vec<CompilerError>> {
        self.check_types();
        let functions = self.check_source_functions();
        if !self.resolved.source_functions.is_empty() && !self.errors.is_empty() {
            return Err(self.errors);
        }
        let mut intents = Vec::new();
        for intent in &self.resolved.intents {
            if let Some(typed) = self.check_intent(intent) {
                intents.push(typed);
            }
        }
        // Capability discovery precedes full-module accounting. Recheck once so
        // earlier intents/operands are also charged when a later call uses lists.
        if !self.meters_list_work() && self.core_capabilities.contains("core.list/v1") {
            let mut metered = Self::new(self.resolved);
            metered.list_work_pass = list_operations::WorkPass::Meter;
            return metered.check();
        }
        let module = TypedModule {
            required_core_capabilities: self.core_capabilities.into_iter().collect(),
            coordinate: self.resolved.coordinate.clone(),
            imports: self.resolved.imports.clone(),
            types: self.core_types,
            functions,
            intents,
        };
        finish(self.errors, module)
    }

    fn type_decl_shape(&mut self, decl: &TypeDecl) -> Option<TypeShape> {
        let coord = self
            .resolved
            .source_type_origins
            .get(&decl.name)
            .map_or_else(
                || format!("{}.{}", self.resolved.coordinate, decl.name),
                |origin| origin.coordinate.clone(),
            );
        match &decl.body {
            TypeExpr::Record(fields) => self.record_shape(&coord, fields, decl.span),
            TypeExpr::Ref(ty) => self.type_ref_shape(ty, decl.span),
            TypeExpr::Variant(cases) => self.variant_shape(&coord, cases, decl.span),
        }
    }

    fn record_shape(&mut self, coord: &str, fields: &[FieldDecl], span: Span) -> Option<TypeShape> {
        let mut out = BTreeMap::new();
        for field in fields {
            let Some(shape) = self.type_ref_shape(&field.ty, field.span) else {
                continue;
            };
            out.insert(field.name.clone(), shape);
        }
        if out.len() == fields.len() {
            Some(TypeShape {
                coord: coord.to_owned(),
                kind: TypeKind::Record(out),
                preserve_named_identity: true,
            })
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "record type contains unsupported field types",
                span,
            ));
            None
        }
    }

    fn type_ref_shape(&mut self, ty: &TypeRef, span: Span) -> Option<TypeShape> {
        match ty {
            TypeRef::Named { path, args } if args.is_empty() && path.len() == 1 => {
                let name = &path[0];
                if name == "Bool" {
                    TypeShape::canonical_structural(TypeKind::Bool)
                } else if let Some(width) = builtin_integer_width(name) {
                    Some(integer_shape(width))
                } else if let Some(shape) = self.named_types.get(name) {
                    Some(shape.clone())
                } else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnresolvedType,
                        format!("type `{name}` is not declared in this module"),
                        span,
                    ));
                    None
                }
            }
            TypeRef::Named { path, args } if args.is_empty() && path.len() >= 2 => {
                self.imported_source_type_shape(path, span)
            }
            TypeRef::Named { path, args }
                if path.as_slice() == ["ExternalActionRequest"] && args.len() == 1 =>
            {
                let settlement = self.type_ref_shape(&args[0], span)?;
                self.source_canonical_structural(
                    TypeKind::ExternalActionRequest {
                        settlement: Box::new(settlement),
                    },
                    span,
                )
            }
            TypeRef::StringTy(Some(refine)) => self.string_shape(refine, span),
            TypeRef::BytesTy(Some(refine)) => self.bytes_shape(refine, span),
            TypeRef::List { elem, max } => {
                let BoundRef::Int { value, .. } = max else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "coordinate list bounds require bound proof in a later stage",
                        span,
                    ));
                    return None;
                };
                let item = self.type_ref_shape(elem, span)?;
                let kind = TypeKind::List {
                    item: Box::new(item),
                    max: *value,
                };
                self.source_canonical_structural(kind, span)
            }
            TypeRef::BytesTy(None)
            | TypeRef::Option(_)
            | TypeRef::CapabilityRef(_)
            | TypeRef::Map { .. }
            | TypeRef::Named { .. }
            | TypeRef::StringTy(None) => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "type is outside the initial lowerable subset",
                    span,
                ));
                None
            }
        }
    }

    fn string_shape(&mut self, refine: &ScalarRefine, span: Span) -> Option<TypeShape> {
        let BoundRef::Int { value, .. } = refine.max else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "coordinate string bounds require bound proof in a later stage",
                span,
            ));
            return None;
        };
        let canonical = refine
            .canonical
            .clone()
            .unwrap_or_else(|| "raw-utf8".to_owned());
        if !matches!(canonical.as_str(), "raw-utf8" | "unicode-scalar-nfc") {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "unsupported string canonicalization",
                span,
            ));
            return None;
        }
        let kind = TypeKind::String {
            max: value,
            canonical,
        };
        self.source_canonical_structural(kind, span)
    }

    fn bytes_shape(&mut self, refine: &BytesRefine, span: Span) -> Option<TypeShape> {
        let BoundRef::Int { value: max, .. } = &refine.max else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "coordinate byte bounds require bound proof in a later stage",
                span,
            ));
            return None;
        };
        let min = match &refine.min {
            Some(BoundRef::Int { value, .. }) => Some(*value),
            Some(BoundRef::Coord(_)) => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "coordinate byte bounds require bound proof in a later stage",
                    span,
                ));
                return None;
            }
            None => None,
        };
        if min.is_some_and(|min| min > *max) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "byte minimum exceeds byte maximum",
                span,
            ));
            return None;
        }
        self.source_canonical_structural(TypeKind::Bytes { min, max: *max }, span)
    }

    fn source_canonical_structural(&mut self, kind: TypeKind, span: Span) -> Option<TypeShape> {
        TypeShape::canonical_structural(kind).or_else(|| {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "expanded source structural type depth exceeded",
                span,
            ));
            None
        })
    }

    fn imported_source_type_shape(&mut self, path: &[String], span: Span) -> Option<TypeShape> {
        let source_alias = &path[0];
        let source_suffix = path[1..].join(".");
        let Some(import) = self.resolved.imports.iter().find(|import| {
            import.kind == CoreImportKind::Lawpack
                && import.alias.as_deref() == Some(source_alias.as_str())
        }) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!(
                    "type `{}` has no exact owning lawpack import",
                    path.join(".")
                ),
                span,
            ));
            return None;
        };
        let coordinate = format!("{}.{}", import.resource.coordinate, source_suffix);
        let Some(fact) = self.resolved.type_shapes.get(&coordinate).cloned() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!("type `{}` has no compiler context fact", path.join(".")),
                span,
            ));
            return None;
        };
        if fact.coordinate != coordinate || fact.lawpack != import.resource {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!(
                    "type `{}` is not owned by its exact imported lawpack",
                    path.join(".")
                ),
                span,
            ));
            return None;
        }
        if let Some(shape) = self.shape_from_imported_type(&fact) {
            self.core_types.extend(imported_type_shape_closure(
                &shape,
                &self.resolved.type_shapes,
            ));
            Some(shape)
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!(
                    "type `{}` has an unsupported imported definition",
                    path.join(".")
                ),
                span,
            ));
            None
        }
    }

    fn check_intent(&mut self, intent: &ResolvedIntent) -> Option<TypedIntent> {
        self.match_in_intent = false;
        self.input_proof_constraints.clear();
        self.local_bounds.clear();
        let recorded_costs = self.source_expression_costs.len();
        let recorded_intrinsics = self.target_pure_intrinsic_costs.len();
        let source = &intent.source;
        if source.params.len() != 1 {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "initial Core lowering supports exactly one intent parameter",
                source.span,
            ));
            return None;
        }
        let param = &source.params[0];
        let input_shape = self.type_ref_shape(&param.ty, param.span)?;
        let output_shape = self.type_ref_shape(&source.returns, source.span)?;
        self.check_helper_cost_budget(intent)?;
        let input_binding = LocalRef {
            id: CORE_APPLICATION_INPUT_LOCAL_ID.to_owned(),
            alpha_name: "$arg0".to_owned(),
            ty: input_shape.coord.clone(),
        };
        let mut locals = vec![input_binding.clone()];
        let mut env = LocalEnvironment::default();
        env.insert(param.name.clone(), (input_binding.clone(), input_shape));
        let mut basis_clauses = source.clauses.iter().filter_map(|clause| match clause {
            IntentClause::Basis(basis) => Some(basis),
            _ => None,
        });
        let Some(basis) = basis_clauses.next() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "intent is missing the required basis clause",
                source.span,
            ));
            return None;
        };
        if basis_clauses.next().is_some() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "intent contains duplicate `basis` clause",
                source.span,
            ));
            return None;
        }
        let basis = match basis {
            Some(basis) => Some(self.check_expr(basis, &env)?.expr),
            None => None,
        };
        let input_constraints = self.input_constraints(source, &env);
        self.input_proof_constraints = input_constraints
            .iter()
            .map(|constraint| constraint.predicate.clone())
            .collect();
        self.effect_binding_profile = Some(EffectBindingProfile {
            name: intent.profile.clone(),
            allowed_write_classes: intent.allowed_write_classes.clone(),
        });

        let body_error_count = self.errors.len();

        let body = self.check_body(intent, &output_shape, &mut env, &mut locals);
        self.effect_binding_profile = None;
        self.input_proof_constraints.clear();
        self.local_bounds.clear();
        let body = body?;
        if self.errors.len() != body_error_count {
            return None;
        }
        self.check_return_budget(intent, &output_shape)?;
        if !self.resolved.source_functions.is_empty()
            || self.source_expression_costs.len() > recorded_costs
            || self.target_pure_intrinsic_costs.len() > recorded_intrinsics
        {
            self.check_helper_cost_budget(intent)?;
        }

        Some(TypedIntent {
            name: intent.name.clone(),
            input: input_binding.ty.clone(),
            output: output_shape.coord,
            profile: intent.profile.clone(),
            budget: intent.budget.clone(),
            input_binding,
            basis,
            input_constraints,
            body,
        })
    }

    fn input_constraints(
        &mut self,
        intent: &IntentDecl,
        env: &LocalEnvironment,
    ) -> Vec<InputConstraint> {
        let mut out = Vec::new();
        let mut index = 0usize;
        for clause in &intent.clauses {
            if let IntentClause::Where(predicates) = clause {
                for predicate in predicates {
                    if let Some(predicate) = self.check_predicate(predicate, env) {
                        out.push(InputConstraint {
                            coordinate: format!("where.{index}"),
                            source: InputConstraintSource::Where,
                            predicate,
                        });
                    }
                    index += 1;
                }
            }
        }
        out
    }

    fn check_helper_cost_budget(&mut self, intent: &ResolvedIntent) -> Option<HelperCost> {
        let mut cost = self.helper_cost_for_block(&intent.source.body)?;
        for clause in &intent.source.clauses {
            let clause_cost = match clause {
                IntentClause::Basis(Some(expr)) => self.helper_cost_for_expr(expr)?,
                IntentClause::Where(predicates) => {
                    self.helper_cost_for_exprs_at(predicates, intent.source.span)?
                }
                IntentClause::Profile(_)
                | IntentClause::Implements(_)
                | IntentClause::Basis(None)
                | IntentClause::Footprint(_)
                | IntentClause::Budget(_) => HelperCost::default(),
            };
            cost = self.checked_helper_cost_add(cost, clause_cost, intent.source.span)?;
        }
        if !self.resolved.source_functions.is_empty()
            || self.match_in_intent
            || self.meters_list_work()
        {
            let input =
                self.type_ref_shape(&intent.source.params[0].ty, intent.source.params[0].span)?;
            let output = self.type_ref_shape(&intent.source.returns, intent.source.span)?;
            // Classify unsupported intent results before helper accounting's
            // conservative encoded bound can conflate them with overflow.
            self.check_return_budget(intent, &output)?;
            let frame = self.source_intent_frame_cost(&input, &output, intent.source.span)?;
            cost = self.checked_helper_cost_add(cost, frame, intent.source.span)?;
        }
        if cost.steps > intent.budget.max_steps
            || cost.allocated_bytes > intent.budget.max_allocated_bytes
            || cost.output_bytes > intent.budget.max_output_bytes
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                format!(
                    "pure-helper cost steps={}, allocatedBytes={}, outputBytes={} exceeds operation budget",
                    cost.steps, cost.allocated_bytes, cost.output_bytes
                ),
                intent.source.span,
            ));
            return None;
        }
        Some(cost)
    }

    fn helper_cost_for_block(&mut self, block: &Block) -> Option<HelperCost> {
        let mut cost = HelperCost::default();
        for stmt in &block.stmts {
            let stmt_cost = self.helper_cost_for_stmt(stmt)?;
            cost = self.checked_helper_cost_add(cost, stmt_cost, block.span)?;
        }
        Some(cost)
    }

    fn helper_cost_for_stmt(&mut self, stmt: &Stmt) -> Option<HelperCost> {
        match stmt {
            Stmt::Let {
                value, els, span, ..
            } => self.helper_cost_for_binding(value, els.as_ref(), *span),
            Stmt::Effect { call, els, span } => {
                let call_cost = self.helper_cost_for_expr(call)?;
                let handler_cost = self.helper_cost_for_handler(els.as_ref())?;
                self.checked_helper_cost_add(call_cost, handler_cost, *span)
            }
            Stmt::ExternalActionRequest {
                operation,
                authority_scope,
                basis,
                max_settlement_bytes,
                max_attempts,
                span,
                ..
            } => self.helper_cost_for_exprs_at(
                [
                    operation,
                    authority_scope,
                    basis,
                    max_settlement_bytes,
                    max_attempts,
                ],
                *span,
            ),
            Stmt::Require {
                predicate,
                arm,
                span,
            } => {
                let predicate_cost = self.helper_cost_for_expr(predicate)?;
                let arm_cost = self.helper_cost_for_require_arm(arm)?;
                self.checked_helper_cost_add(predicate_cost, arm_cost, *span)
            }
            Stmt::Guarantee {
                predicate,
                obstruction,
                span,
            } => {
                let predicate_cost = self.helper_cost_for_expr(predicate)?;
                let obstruction_cost = self.helper_cost_for_target(obstruction.as_ref())?;
                self.checked_helper_cost_add(predicate_cost, obstruction_cost, *span)
            }
            Stmt::Assert { predicate, .. }
            | Stmt::Return {
                value: predicate, ..
            } => self.helper_cost_for_expr(predicate),
            Stmt::If {
                cond,
                then_block,
                els,
                span,
            } => {
                let cond_cost = self.helper_cost_for_expr(cond)?;
                let then_cost = self.helper_cost_for_block(then_block)?;
                let else_cost = match els.as_deref() {
                    Some(ElseClause::Block(block)) => self.helper_cost_for_block(block)?,
                    Some(ElseClause::If(stmt)) => self.helper_cost_for_stmt(stmt)?,
                    None => HelperCost::default(),
                };
                self.checked_helper_cost_add(cond_cost, then_cost.component_max(else_cost), *span)
            }
            Stmt::For {
                iter,
                bound,
                body,
                span,
                ..
            } => {
                let iter_cost = self.helper_cost_for_expr(iter)?;
                let body_cost = self.helper_cost_for_block(body)?;
                let Some(factor) = self.helper_cost_loop_bound(bound) else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::MissingContextFact,
                        "bounded-loop helper cost requires a resolvable loop bound",
                        *span,
                    ));
                    return None;
                };
                let body_cost = self.checked_helper_cost_mul(body_cost, factor, *span)?;
                let loop_cost = HelperCost {
                    steps: factor,
                    ..HelperCost::default()
                };
                let body_and_loop = self.checked_helper_cost_add(body_cost, loop_cost, *span)?;
                self.checked_helper_cost_add(iter_cost, body_and_loop, *span)
            }
        }
    }

    fn helper_cost_for_binding(
        &mut self,
        value: &Expr,
        handler: Option<&ObstructionHandler>,
        span: Span,
    ) -> Option<HelperCost> {
        let value_cost = self.helper_cost_for_expr(value)?;
        let handler_cost = self.helper_cost_for_handler(handler)?;
        let combined = self.checked_helper_cost_add(value_cost, handler_cost, span)?;
        let steps = self
            .source_value_bounds
            .get(&expression_identity(value))
            .map_or(0, |bound| bound.0);
        self.checked_helper_cost_add(
            combined,
            HelperCost {
                steps,
                ..HelperCost::default()
            },
            span,
        )
    }

    fn helper_cost_for_expr(&mut self, expr: &Expr) -> Option<HelperCost> {
        if let Expr::Call {
            callee, args, span, ..
        } = expr
        {
            if matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "fold") {
                return self.fold_cost(expr, args, *span);
            }
        }
        let children = match expr {
            Expr::Ident { .. }
            | Expr::Int { .. }
            | Expr::Str { .. }
            | Expr::Bytes { .. }
            | Expr::Bool { .. }
            | Expr::Digest { .. } => Some(HelperCost::default()),
            Expr::Field { base, .. } | Expr::Unary { operand: base, .. } => {
                self.helper_cost_for_expr(base)
            }
            Expr::Call {
                callee, args, span, ..
            } => {
                let nested = self.helper_cost_for_exprs_at(
                    std::iter::once(callee.as_ref()).chain(args.iter()),
                    *span,
                )?;
                let own = self.helper_cost_for_call(callee, *span)?;
                self.checked_helper_cost_add(nested, own, *span)
            }
            Expr::Binary { lhs, rhs, span, .. } => {
                let left = self.helper_cost_for_expr(lhs)?;
                let right = self.helper_cost_for_expr(rhs)?;
                self.checked_helper_cost_add(left, right, *span)
            }
            Expr::List { items, span } => self.helper_cost_for_exprs_at(items.iter(), *span),
            Expr::Record { entries, span } => {
                let mut cost = HelperCost::default();
                for entry in entries {
                    let entry_cost = match entry {
                        RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                            self.helper_cost_for_expr(value)?
                        }
                        RecordEntry::Shorthand { .. } => HelperCost::default(),
                    };
                    cost = self.checked_helper_cost_add(cost, entry_cost, *span)?;
                }
                Some(cost)
            }
            Expr::If {
                cond,
                then,
                els,
                span,
            } => {
                let cond_cost = self.helper_cost_for_expr(cond)?;
                let then_cost = self.helper_cost_for_expr(then)?;
                let else_cost = self.helper_cost_for_expr(els)?;
                self.checked_helper_cost_add(cond_cost, then_cost.component_max(else_cost), *span)
            }
            Expr::IfYield {
                pred,
                then_block,
                else_block,
                span,
            } => {
                let predicate_cost = self.helper_cost_for_expr(pred)?;
                let then_cost = self.helper_cost_for_yield_block(then_block)?;
                let else_cost = self.helper_cost_for_yield_block(else_block)?;
                self.checked_helper_cost_add(
                    predicate_cost,
                    then_cost.component_max(else_cost),
                    *span,
                )
            }
            Expr::VariantLit { payload, .. } => payload
                .as_deref()
                .map_or(Some(HelperCost::default()), |value| {
                    self.helper_cost_for_expr(value)
                }),
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => {
                let scrutinee_cost = self.helper_cost_for_expr(scrutinee)?;
                let mut arm_cost = HelperCost::default();
                for arm in arms {
                    arm_cost = arm_cost.component_max(self.helper_cost_for_expr(&arm.body)?);
                }
                self.checked_helper_cost_add(scrutinee_cost, arm_cost, *span)
            }
        }?;
        let span = expr_span(expr);
        let own = self.recorded_own_cost(expr, span)?;
        self.checked_helper_cost_add(children, own, span)
    }

    /// Work recorded for this expression itself: its value, predicate and any
    /// target pure-intrinsic call.
    fn recorded_own_cost(&mut self, expr: &Expr, span: Span) -> Option<HelperCost> {
        let key = expression_identity(expr);
        let [own, predicate, intrinsic] = [
            &self.source_expression_costs,
            &self.source_predicate_costs,
            &self.target_pure_intrinsic_costs,
        ]
        .map(|costs| costs.get(&key).copied().unwrap_or_default());
        let own = self.checked_helper_cost_add(own, intrinsic, span)?;
        self.checked_helper_cost_add(own, predicate, span)
    }

    fn helper_cost_for_exprs_at<'b, I>(&mut self, exprs: I, span: Span) -> Option<HelperCost>
    where
        I: IntoIterator<Item = &'b Expr>,
    {
        let mut cost = HelperCost::default();
        for expr in exprs {
            let expr_cost = self.helper_cost_for_expr(expr)?;
            cost = self.checked_helper_cost_add(cost, expr_cost, span)?;
        }
        Some(cost)
    }

    fn helper_cost_for_call(&mut self, callee: &Expr, span: Span) -> Option<HelperCost> {
        let Some(source_coordinate) = plain_callee_coordinate(callee) else {
            return Some(HelperCost::default());
        };
        if let Some(cost) = self.function_costs.get(&source_coordinate) {
            return Some(*cost);
        }
        let Some(fact) = self.resolved.pure_functions.get(&source_coordinate) else {
            return Some(HelperCost::default());
        };
        if !self.fact_matches_source_import(&source_coordinate, &fact.coordinate, &fact.lawpack) {
            return Some(HelperCost::default());
        }
        let Some((source_alias, _)) = source_coordinate.split_once('.') else {
            return Some(HelperCost::default());
        };
        let Some(cost_suffix) = fact
            .cost_template
            .strip_prefix(&fact.lawpack.coordinate)
            .and_then(|suffix| suffix.strip_prefix('.'))
        else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!(
                    "pure helper `{source_coordinate}` cost template is outside its owning lawpack"
                ),
                span,
            ));
            return None;
        };
        let source_cost = format!("{source_alias}.{cost_suffix}");
        if !self.fact_matches_source_import(&source_cost, &fact.cost_template, &fact.lawpack) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!("pure helper `{source_coordinate}` cost template is not imported"),
                span,
            ));
            return None;
        }
        let Some(cost_fact) = self.resolved.pure_helper_costs.get(&source_cost) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!("pure helper `{source_coordinate}` cost template is unresolved"),
                span,
            ));
            return None;
        };
        if cost_fact.coordinate != fact.cost_template
            || cost_fact.lawpack != fact.lawpack
            || !self.fact_matches_source_import(
                &source_cost,
                &cost_fact.coordinate,
                &cost_fact.lawpack,
            )
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!(
                    "pure helper `{source_coordinate}` cost template is not owned by its exact imported lawpack"
                ),
                span,
            ));
            return None;
        }
        Some(HelperCost::from_budget(&cost_fact.budget))
    }

    fn helper_cost_for_yield_block(
        &mut self,
        block: &crate::ast::YieldBlock,
    ) -> Option<HelperCost> {
        // Keep the original borrowed expressions: their immutable AST identity
        // keys type/cost summaries, while spans remain diagnostics only.
        let mut statement_cost = HelperCost::default();
        for statement in &block.stmts {
            let cost = self.helper_cost_for_stmt(statement)?;
            statement_cost = self.checked_helper_cost_add(statement_cost, cost, block.span)?;
        }
        let value_cost = self.helper_cost_for_expr(&block.value)?;
        self.checked_helper_cost_add(statement_cost, value_cost, block.span)
    }

    fn helper_cost_for_handler(
        &mut self,
        handler: Option<&ObstructionHandler>,
    ) -> Option<HelperCost> {
        match handler {
            Some(ObstructionHandler::Single(target)) => self.helper_cost_for_target(Some(target)),
            Some(ObstructionHandler::Map(arms)) => {
                let mut cost = HelperCost::default();
                for arm in arms {
                    cost = cost.component_max(self.helper_cost_for_target(Some(&arm.target))?);
                }
                Some(cost)
            }
            None => Some(HelperCost::default()),
        }
    }

    fn helper_cost_for_require_arm(&mut self, arm: &RequireElseArm) -> Option<HelperCost> {
        match arm {
            RequireElseArm::Terminal(target) => self.helper_cost_for_target(Some(target)),
            RequireElseArm::ContinueObstructed(arm) => {
                let reason = self.helper_cost_for_expr(&arm.reason)?;
                let mut payload = HelperCost::default();
                for entry in &arm.payload {
                    let entry_cost = match entry {
                        RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                            self.helper_cost_for_expr(value)?
                        }
                        RecordEntry::Shorthand { .. } => HelperCost::default(),
                    };
                    payload = self.checked_helper_cost_add(payload, entry_cost, arm.span)?;
                }
                self.checked_helper_cost_add(reason, payload, arm.span)
            }
        }
    }

    fn helper_cost_for_target(&mut self, target: Option<&ObstructionTarget>) -> Option<HelperCost> {
        target
            .and_then(|target| target.payload.as_ref())
            .map_or(Some(HelperCost::default()), |payload| {
                self.helper_cost_for_expr(payload)
            })
    }

    fn helper_cost_loop_bound(&self, bound: &BoundRef) -> Option<u64> {
        match bound {
            BoundRef::Int { value, .. } => Some(*value),
            BoundRef::Coord(path) => {
                let source_coordinate = path.join(".");
                let fact = self.resolved.bounds.get(&source_coordinate)?;
                self.fact_matches_source_import(&source_coordinate, &fact.coordinate, &fact.lawpack)
                    .then_some(fact.value)
            }
        }
    }

    fn checked_helper_cost_add(
        &mut self,
        left: HelperCost,
        right: HelperCost,
        span: Span,
    ) -> Option<HelperCost> {
        left.checked_add(right).or_else(|| {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "pure-helper cost accumulation overflows u64",
                span,
            ));
            None
        })
    }

    fn checked_helper_cost_mul(
        &mut self,
        cost: HelperCost,
        factor: u64,
        span: Span,
    ) -> Option<HelperCost> {
        cost.checked_mul(factor).or_else(|| {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "bounded-loop helper cost multiplication overflows u64",
                span,
            ));
            None
        })
    }

    fn check_body(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
    ) -> Option<CoreBlock> {
        let source = &intent.source;
        let mut state = BodyState::default();

        for stmt in &source.body.stmts {
            self.check_body_stmt(intent, output_shape, stmt, env, locals, &mut state);
        }

        if let Some(result) = state.result {
            Some(CoreBlock {
                locals: locals.clone(),
                nodes: state.nodes,
                result,
            })
        } else if state.saw_return {
            None
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "intent body must return a value",
                source.span,
            ));
            None
        }
    }

    #[allow(clippy::too_many_lines)]
    fn check_body_stmt(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        stmt: &Stmt,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        match stmt {
            Stmt::Let {
                name,
                ty,
                value,
                els,
                span,
            } => self.check_let_stmt(
                intent,
                output_shape,
                LetStatement {
                    name,
                    ty: ty.as_ref(),
                    value,
                    handler: els.as_ref(),
                    span: *span,
                },
                env,
                locals,
                state,
            ),
            Stmt::Return { value, .. } => {
                state.saw_return = true;
                let Some(value) = self.check_expr_with_expected(value, env, Some(output_shape))
                else {
                    return;
                };
                if compatible(output_shape, &value.ty) {
                    state.result = Some(value.expr);
                } else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "return value does not match declared output type",
                        intent.source.span,
                    ));
                }
            }
            Stmt::Effect { call, span, .. } => {
                if self.check_effect_profile(intent, call, *span) {
                    self.unsupported_stmt(*span, "effect statement");
                }
            }
            Stmt::ExternalActionRequest {
                name,
                request_type,
                operation,
                input_schema,
                settlement_schema,
                authority_scope,
                basis,
                max_settlement_bytes,
                max_attempts,
                reconciliation_law,
                span,
            } => {
                let previous_id = env.get(name).map(|(local, _)| local.id.clone());
                self.check_external_action_request(
                    name,
                    request_type,
                    operation,
                    input_schema,
                    settlement_schema,
                    authority_scope,
                    basis,
                    max_settlement_bytes,
                    max_attempts,
                    reconciliation_law,
                    *span,
                    env,
                    locals,
                    state,
                );
                let current_id = env.get(name).map(|(local, _)| local.id.clone());
                if current_id.is_none() || current_id == previous_id {
                    env.poison(name);
                }
            }
            Stmt::Require { predicate, arm, .. } => {
                self.check_require_stmt(predicate, arm, env, state);
            }
            Stmt::For {
                var,
                iter,
                bound,
                body,
                span,
            } => self.check_for_stmt(
                intent,
                output_shape,
                var,
                iter,
                bound,
                body,
                *span,
                env,
                state,
            ),
            Stmt::If {
                cond,
                then_block,
                els,
                span,
            } => self.check_if_stmt(
                intent,
                output_shape,
                cond,
                then_block,
                els.as_deref(),
                *span,
                env,
                state,
            ),
            Stmt::Guarantee { span, .. } | Stmt::Assert { span, .. } => {
                self.unsupported_stmt(*span, "statement");
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_if_stmt(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        cond: &Expr,
        then_block: &Block,
        els: Option<&ElseClause>,
        span: Span,
        env: &LocalEnvironment,
        state: &mut BodyState,
    ) {
        let predicate = self.check_predicate(cond, env);
        let baseline_steps = state.accumulated_steps;
        let then_block =
            self.check_isolated_statement_block(intent, output_shape, then_block, env, state);
        let then_steps = state.accumulated_steps;
        state.accumulated_steps = baseline_steps;
        let else_block = match els {
            Some(ElseClause::Block(block)) => {
                self.check_isolated_statement_block(intent, output_shape, block, env, state)
            }
            Some(ElseClause::If(_)) => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "else-if chains require explicit branch-join lowering",
                    span,
                ));
                empty_core_block()
            }
            None => empty_core_block(),
        };
        state.accumulated_steps = then_steps.max(state.accumulated_steps);
        if let Some(predicate) = predicate {
            state.nodes.push(CoreNode::Branch {
                binding: None,
                predicate,
                then_block,
                else_block,
            });
        }
    }

    fn check_isolated_statement_block(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        block: &Block,
        env: &LocalEnvironment,
        state: &mut BodyState,
    ) -> CoreBlock {
        let mut nested_env = env.clone();
        let mut nested_locals = Vec::new();
        let mut nested_state = BodyState {
            local_index: state.local_index,
            obstruction_index: state.obstruction_index,
            step_factor: state.step_factor,
            accumulated_steps: state.accumulated_steps,
            ..BodyState::default()
        };
        for stmt in &block.stmts {
            if let Stmt::Return { span, .. } = stmt {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "return is not legal inside an isolated statement branch",
                    *span,
                ));
                continue;
            }
            self.check_body_stmt(
                intent,
                output_shape,
                stmt,
                &mut nested_env,
                &mut nested_locals,
                &mut nested_state,
            );
        }
        state.local_index = nested_state.local_index;
        state.obstruction_index = nested_state.obstruction_index;
        state.accumulated_steps = nested_state.accumulated_steps;
        CoreBlock {
            locals: nested_locals,
            nodes: nested_state.nodes,
            result: CoreExpr::Const(CoreValue::Null),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_for_stmt(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        var: &str,
        iter: &Expr,
        bound: &BoundRef,
        body: &Block,
        span: Span,
        env: &LocalEnvironment,
        state: &mut BodyState,
    ) {
        let Some(iter_value) = self.check_expr(iter, env) else {
            self.check_failed_for_body(intent, output_shape, var, body, env, state, None);
            return;
        };
        let TypeKind::List { item, max } = &iter_value.ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "bounded for requires a statically bounded list",
                span,
            ));
            self.check_failed_for_body(intent, output_shape, var, body, env, state, None);
            return;
        };
        let Some((value, core_bound)) = self.resolve_loop_bound(bound, span) else {
            self.check_failed_for_body(intent, output_shape, var, body, env, state, Some(item));
            return;
        };
        if value < *max {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                format!("loop bound {value} is below iterable maximum {max}"),
                span,
            ));
            self.check_failed_for_body(intent, output_shape, var, body, env, state, Some(item));
            return;
        }
        let Some((loop_steps, accumulated_steps)) =
            self.charge_loop_work(intent, state, value, span)
        else {
            self.check_failed_for_body(intent, output_shape, var, body, env, state, Some(item));
            return;
        };

        let binder_shape = item.as_ref().clone();
        let binder = next_local(&mut state.local_index, binder_shape.coord.clone());
        let mut nested_env = env.clone();
        nested_env.insert(var.to_owned(), (binder.clone(), binder_shape));
        let mut nested_locals = vec![binder.clone()];
        let mut nested_state = BodyState {
            local_index: state.local_index,
            obstruction_index: state.obstruction_index,
            step_factor: loop_steps,
            accumulated_steps,
            ..BodyState::default()
        };
        self.check_for_body_stmts(
            intent,
            output_shape,
            body,
            &mut nested_env,
            &mut nested_locals,
            &mut nested_state,
        );
        state.local_index = nested_state.local_index;
        state.obstruction_index = nested_state.obstruction_index;
        state.accumulated_steps = nested_state.accumulated_steps;
        state.nodes.push(CoreNode::For {
            binder,
            iter: iter_value.expr,
            bound: core_bound,
            body: CoreBlock {
                locals: nested_locals,
                nodes: nested_state.nodes,
                result: CoreExpr::Const(CoreValue::Null),
            },
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn check_failed_for_body(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        var: &str,
        body: &Block,
        env: &LocalEnvironment,
        state: &BodyState,
        known_item: Option<&TypeShape>,
    ) {
        let mut nested_env = env.clone();
        let mut nested_locals = Vec::new();
        let mut nested_state = BodyState {
            local_index: state.local_index,
            obstruction_index: state.obstruction_index,
            step_factor: state.step_factor,
            accumulated_steps: state.accumulated_steps,
            ..BodyState::default()
        };
        if let Some(item) = known_item {
            let binder = next_local(&mut nested_state.local_index, item.coord.clone());
            nested_env.insert(var.to_owned(), (binder.clone(), item.clone()));
            nested_locals.push(binder);
        } else {
            nested_env.poison(var);
        }
        self.check_for_body_stmts(
            intent,
            output_shape,
            body,
            &mut nested_env,
            &mut nested_locals,
            &mut nested_state,
        );
    }

    fn check_for_body_stmts(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        body: &Block,
        nested_env: &mut LocalEnvironment,
        nested_locals: &mut Vec<LocalRef>,
        nested_state: &mut BodyState,
    ) {
        for stmt in &body.stmts {
            if let Stmt::Return { span, .. } = stmt {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "return is not legal inside a bounded for body",
                    *span,
                ));
                continue;
            }
            self.check_body_stmt(
                intent,
                output_shape,
                stmt,
                nested_env,
                nested_locals,
                nested_state,
            );
        }
    }

    fn resolve_loop_bound(&mut self, bound: &BoundRef, span: Span) -> Option<(u64, CoreBound)> {
        let path = match bound {
            BoundRef::Int { value, .. } => return Some((*value, CoreBound::Literal(*value))),
            BoundRef::Coord(path) => path,
        };
        let source_coordinate = path.join(".");
        let Some(fact) = self.resolved.bounds.get(&source_coordinate) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!("loop bound `{source_coordinate}` has no compiler context fact"),
                span,
            ));
            return None;
        };
        if !self.fact_matches_source_import(&source_coordinate, &fact.coordinate, &fact.lawpack) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!(
                    "loop bound `{source_coordinate}` is not owned by an exact imported lawpack"
                ),
                span,
            ));
            return None;
        }
        Some((fact.value, CoreBound::Coordinate(fact.coordinate.clone())))
    }

    fn charge_loop_work(
        &mut self,
        intent: &ResolvedIntent,
        state: &BodyState,
        bound: u64,
        span: Span,
    ) -> Option<(u64, u64)> {
        let loop_steps = state.step_factor.checked_mul(bound);
        let accumulated_steps =
            loop_steps.and_then(|steps| state.accumulated_steps.checked_add(steps));
        let Some((loop_steps, accumulated_steps)) = loop_steps.zip(accumulated_steps) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "cumulative loop step accounting overflows u64",
                span,
            ));
            return None;
        };
        if accumulated_steps > intent.budget.max_steps {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                format!(
                    "cumulative loop work {accumulated_steps} exceeds operation step budget {}",
                    intent.budget.max_steps
                ),
                span,
            ));
            return None;
        }
        Some((loop_steps, accumulated_steps))
    }

    #[allow(clippy::too_many_arguments)]
    fn check_external_action_request(
        &mut self,
        name: &str,
        request_type: &TypeRef,
        operation: &Expr,
        input_schema: &DigestLockedPackageRef,
        settlement_schema: &DigestLockedPackageRef,
        authority_scope: &Expr,
        basis: &Expr,
        max_settlement_bytes: &Expr,
        max_attempts: &Expr,
        reconciliation_law: &DigestLockedPackageRef,
        span: Span,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        let request_shape = self.type_ref_shape(request_type, span);
        let settlement_type = request_shape.as_ref().and_then(|shape| {
            if let TypeKind::ExternalActionRequest { settlement } = &shape.kind {
                Some(settlement.coord.clone())
            } else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "request binding type must be ExternalActionRequest<Settlement>",
                    span,
                ));
                None
            }
        });
        let operation = self
            .check_external_action_operation(operation, env, span)
            .filter(|(resource, _)| {
                if requestable_operation_family(&resource.coordinate) {
                    true
                } else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnrequestableExternalOperation,
                        format!(
                            "external operation family `{}` is not requestable",
                            resource.coordinate
                        ),
                        span,
                    ));
                    false
                }
            });
        let authority_scope = self.check_expr(authority_scope, env);
        let basis = self.check_expr(basis, env);
        let settlement_budget_shape = integer_shape("U64");
        let max_settlement_bytes = self.check_expr_with_expected(
            max_settlement_bytes,
            env,
            Some(&settlement_budget_shape),
        );
        let attempt_budget_shape = integer_shape("U32");
        let max_attempts =
            self.check_expr_with_expected(max_attempts, env, Some(&attempt_budget_shape));
        let input_schema = self.request_resource(input_schema);
        let settlement_schema = self.request_resource(settlement_schema);
        let reconciliation_law = self.request_resource(reconciliation_law);
        let (
            Some(request_shape),
            Some(settlement_type),
            Some((operation_resource, input)),
            Some(authority_scope),
            Some(basis),
            Some(max_settlement_bytes),
            Some(max_attempts),
            Some(input_schema),
            Some(settlement_schema),
            Some(reconciliation_law),
        ) = (
            request_shape,
            settlement_type,
            operation,
            authority_scope,
            basis,
            max_settlement_bytes,
            max_attempts,
            input_schema,
            settlement_schema,
            reconciliation_law,
        )
        else {
            return;
        };
        let local = next_local(&mut state.local_index, request_shape.coord.clone());
        state.nodes.push(CoreNode::ExternalActionRequest {
            binding: local.clone(),
            operation: operation_resource,
            input_type: input.ty.value_type_coord(),
            settlement_type,
            input_schema,
            settlement_schema,
            input: input.expr,
            authority_scope: Box::new(authority_scope.expr),
            basis: Box::new(basis.expr),
            budget: Box::new(CoreExternalActionBudget {
                max_settlement_bytes: max_settlement_bytes.expr,
                max_attempts: max_attempts.expr,
            }),
            reconciliation_law,
        });
        locals.push(local.clone());
        env.insert(name.to_owned(), (local, request_shape));
    }

    fn check_external_action_operation(
        &mut self,
        operation: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<(ResourceRef, TypedValue)> {
        let Expr::Call {
            callee,
            type_args,
            args,
            ..
        } = operation
        else {
            self.unsupported_stmt(span, "external-action request operation");
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "external-action request operation does not accept type arguments",
                span,
            ));
            return None;
        }
        let Expr::Ident { name: alias, .. } = callee.as_ref() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "external-action request operation must name one capability import alias",
                span,
            ));
            return None;
        };
        let Some(operation_resource) = self
            .resolved
            .imports
            .iter()
            .find(|import| {
                import.kind == CoreImportKind::Capability
                    && import.alias.as_deref() == Some(alias.as_str())
                    && import
                        .resource
                        .digest
                        .as_deref()
                        .is_some_and(is_lowercase_sha256_review_digest)
            })
            .map(|import| import.resource.clone())
        else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                format!(
                    "external-action operation `{alias}` has no digest-locked capability import"
                ),
                span,
            ));
            return None;
        };
        let [input] = args.as_slice() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "external-action request operation accepts exactly one input value",
                span,
            ));
            return None;
        };
        let input = self.check_expr(input, env)?;
        Some((operation_resource, input))
    }

    fn request_resource(&mut self, resource: &DigestLockedPackageRef) -> Option<ResourceRef> {
        if !is_lowercase_sha256_review_digest(&resource.digest) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "external-action resource digest must use lowercase sha256 review rendering",
                resource.span,
            ));
            return None;
        }
        Some(ResourceRef {
            coordinate: package_coordinate(&resource.package.path, &resource.package.version),
            digest: Some(resource.digest.clone()),
        })
    }

    fn check_require_stmt(
        &mut self,
        predicate: &Expr,
        arm: &RequireElseArm,
        env: &LocalEnvironment,
        state: &mut BodyState,
    ) {
        let predicate = self.check_predicate(predicate, env);
        let arm = self.check_require_else_arm(arm, env);
        let (Some(predicate), Some(arm)) = (predicate, arm) else {
            return;
        };
        state.nodes.push(CoreNode::Require { predicate, arm });
    }

    fn check_require_else_arm(
        &mut self,
        arm: &RequireElseArm,
        env: &LocalEnvironment,
    ) -> Option<CoreRequireFailureArm> {
        match arm {
            RequireElseArm::Terminal(target) => {
                let reason = self.check_terminal_require_reason(target, env)?;
                Some(CoreRequireFailureArm::Terminal { reason })
            }
            RequireElseArm::ContinueObstructed(obstruction) => {
                let reason_kind =
                    self.check_reason_kind(&obstruction.reason, env, obstruction.span);
                let payload = self.check_reason_payload(
                    &obstruction.payload,
                    env,
                    ReasonPayloadMode::SkipReasonField,
                );
                Some(CoreRequireFailureArm::ContinueObstructed {
                    reason: CoreObstructionReason {
                        kind: reason_kind?,
                        payload: payload?,
                    },
                })
            }
        }
    }

    fn check_terminal_require_reason(
        &mut self,
        target: &ObstructionTarget,
        env: &LocalEnvironment,
    ) -> Option<CoreObstructionReason> {
        if self.reject_capability_obstruction_root(&target.coordinate, target.span) {
            return None;
        }
        let payload = match &target.payload {
            Some(Expr::Record { entries, .. }) => {
                self.check_reason_payload(entries, env, ReasonPayloadMode::PreserveReasonField)?
            }
            Some(_) => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "terminal require obstruction payloads must be record values",
                    target.span,
                ));
                return None;
            }
            None => BTreeMap::new(),
        };
        Some(CoreObstructionReason {
            kind: path_key(&target.coordinate),
            payload,
        })
    }

    fn check_reason_kind(
        &mut self,
        reason: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<String> {
        let Some(kind) = plain_path_coordinate(reason) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "obstruction reason must be a stable coordinate",
                span,
            ));
            return None;
        };
        if let Some(root) = plain_path_root(reason) {
            if self.capability_import_alias(root) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "capability imports are legal only as request operations",
                    span,
                ));
                return None;
            }
            if env.poisoned.contains(root) {
                return None;
            }
            if env.contains_key(root) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "obstruction reason must be a stable coordinate, not a local expression",
                    span,
                ));
                return None;
            }
        }
        Some(kind)
    }

    fn check_reason_payload(
        &mut self,
        entries: &[RecordEntry],
        env: &LocalEnvironment,
        mode: ReasonPayloadMode,
    ) -> Option<BTreeMap<String, CoreExpr>> {
        let mut fields = BTreeMap::new();
        let mut authored_names = BTreeSet::new();
        let mut accepted = true;
        for entry in entries {
            match entry {
                RecordEntry::Field { name, .. } | RecordEntry::Shorthand { name, .. }
                    if mode == ReasonPayloadMode::SkipReasonField && name == "reason" => {}
                RecordEntry::Field { name, value } => {
                    if !authored_names.insert(name) {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::DuplicateObstructionPayloadField,
                            format!("obstruction reason payload repeats field `{name}`"),
                            expr_span(value),
                        ));
                        accepted = false;
                        // Still diagnose this independently authored value.
                    }
                    if let Some(value) = self.check_expr(value, env) {
                        fields.insert(name.clone(), value.expr);
                    } else {
                        accepted = false;
                    }
                }
                RecordEntry::Shorthand { name, span } => {
                    if !authored_names.insert(name) {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::DuplicateObstructionPayloadField,
                            format!("obstruction reason payload repeats field `{name}`"),
                            *span,
                        ));
                        accepted = false;
                        // A repeated key does not resolve its shorthand name.
                    }
                    let Some((local, _)) = env.get(name) else {
                        if !env.poisoned.contains(name) {
                            self.errors.push(error(
                                CompilerStage::TypeCheck,
                                CompilerErrorKind::UnresolvedType,
                                format!("obstruction reason payload field `{name}` has no binding"),
                                *span,
                            ));
                        }
                        accepted = false;
                        continue;
                    };
                    fields.insert(
                        name.clone(),
                        CoreExpr::Local {
                            reference: local.clone(),
                        },
                    );
                }
                RecordEntry::Spread(value) => {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "obstruction reason payload spreads are outside the Core subset",
                        expr_span(value),
                    ));
                    accepted = false;
                }
            }
        }
        if accepted {
            Some(fields)
        } else {
            None
        }
    }

    fn check_let_stmt(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        stmt: LetStatement<'_>,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        let previous_id = env.get(stmt.name).map(|(local, _)| local.id.clone());
        let name = stmt.name;
        if let Some(handler) = stmt.handler {
            self.check_effectful_let(intent, stmt, handler, env, locals, state);
        } else {
            self.check_pure_let(intent, output_shape, stmt, env, locals, state);
        }
        let current_id = env.get(name).map(|(local, _)| local.id.clone());
        if current_id.is_none() || current_id == previous_id {
            env.poison(name);
        }
    }

    fn check_pure_let(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        stmt: LetStatement<'_>,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        if let Expr::IfYield {
            pred,
            then_block,
            else_block,
            span,
        } = stmt.value
        {
            self.check_branch_yield_let(
                intent,
                output_shape,
                &stmt,
                pred,
                then_block,
                else_block,
                *span,
                env,
                locals,
                state,
            );
            return;
        }
        if !self.check_known_effect_profiles(intent, stmt.value) {
            return;
        }
        let annotation_shape = match stmt.ty {
            Some(annotation) => {
                let Some(shape) = self.type_ref_shape(annotation, stmt.span) else {
                    self.check_expr_with_unavailable_expected_type(stmt.value, env);
                    return;
                };
                Some(shape)
            }
            None => None,
        };
        let Some(value) = self.check_expr_with_expected(stmt.value, env, annotation_shape.as_ref())
        else {
            return;
        };
        let Some(binding_shape) = self.pure_let_binding_shape(&stmt, &value, annotation_shape)
        else {
            return;
        };
        let local = next_local(&mut state.local_index, binding_shape.coord.clone());
        self.record_binding_bound(&local, &value.expr);
        state.nodes.push(CoreNode::Let {
            binding: local.clone(),
            value: value.expr,
        });
        locals.push(local.clone());
        env.insert(stmt.name.to_owned(), (local, binding_shape));
    }

    #[allow(clippy::too_many_arguments)]
    fn check_branch_yield_let(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        stmt: &LetStatement<'_>,
        pred: &Expr,
        then_source: &YieldBlock,
        else_source: &YieldBlock,
        span: Span,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        let predicate = if self.check_known_effect_profiles(intent, pred) {
            self.check_predicate(pred, env)
        } else {
            None
        };
        let annotation_shape = stmt
            .ty
            .and_then(|annotation| self.type_ref_shape(annotation, stmt.span));
        let expected_unavailable = stmt.ty.is_some() && annotation_shape.is_none();
        let baseline_steps = state.accumulated_steps;
        let Some((then_block, then_shape, else_block, else_shape)) = self
            .check_branch_yield_blocks(
                intent,
                output_shape,
                then_source,
                else_source,
                env,
                state,
                baseline_steps,
                annotation_shape.as_ref(),
                expected_unavailable,
            )
        else {
            return;
        };
        let Some(binding_shape) = self.join_branch_shapes(
            annotation_shape.as_ref(),
            &then_shape,
            &else_shape,
            (
                "branch-yield results do not match the annotated type",
                "branch-yield results do not have a compatible bounded type",
            ),
            span,
        ) else {
            return;
        };

        if expected_unavailable {
            return;
        }
        let Some(predicate) = predicate else {
            return;
        };
        let binding = next_local(&mut state.local_index, binding_shape.coord.clone());
        state.nodes.push(CoreNode::Branch {
            binding: Some(binding.clone()),
            predicate,
            then_block,
            else_block,
        });
        locals.push(binding.clone());
        env.insert(stmt.name.to_owned(), (binding, binding_shape));
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn check_branch_yield_blocks(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        then_source: &YieldBlock,
        else_source: &YieldBlock,
        env: &LocalEnvironment,
        state: &mut BodyState,
        baseline_steps: u64,
        annotation_shape: Option<&TypeShape>,
        expected_unavailable: bool,
    ) -> Option<(CoreBlock, TypeShape, CoreBlock, TypeShape)> {
        let joint_shape = if annotation_shape.is_none()
            && contains_contextual_bare_integer(&then_source.value)
            && contains_contextual_bare_integer(&else_source.value)
        {
            self.infer_joint_yield_shape(intent, output_shape, then_source, else_source, env, state)
        } else {
            None
        };
        let shared_expected = annotation_shape.or(joint_shape.as_ref());
        let inferred_else_shape =
            if shared_expected.is_none() && contains_contextual_bare_integer(&then_source.value) {
                self.infer_yield_block_shape(intent, output_shape, else_source, env, state)
            } else {
                None
            };
        let then_result = self.check_yield_block(
            intent,
            output_shape,
            then_source,
            env,
            state,
            shared_expected.or(inferred_else_shape.as_ref()),
            expected_unavailable,
        );
        let then_steps = state.accumulated_steps;
        state.accumulated_steps = baseline_steps;
        let inferred_then_shape =
            if shared_expected.is_none() && contains_contextual_bare_integer(&else_source.value) {
                then_result
                    .as_ref()
                    .map(|(_, shape)| shape.clone())
                    .or_else(|| {
                        self.infer_yield_block_shape(intent, output_shape, then_source, env, state)
                    })
            } else {
                None
            };
        let else_expected = shared_expected.or(inferred_then_shape.as_ref());
        let else_result = self.check_yield_block(
            intent,
            output_shape,
            else_source,
            env,
            state,
            else_expected,
            expected_unavailable,
        );
        state.accumulated_steps = then_steps.max(state.accumulated_steps);
        if then_result.is_none() || else_result.is_none() {
            if let Some(annotation) = annotation_shape {
                for (result, source) in [(&then_result, then_source), (&else_result, else_source)] {
                    if result
                        .as_ref()
                        .is_some_and(|(_, shape)| !compatible(annotation, shape))
                    {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::TypeMismatch,
                            "branch-yield result does not match the annotated type",
                            expr_span(&source.value),
                        ));
                    }
                }
            }
            return None;
        }
        let (then_block, then_shape) = then_result?;
        let (else_block, else_shape) = else_result?;
        Some((then_block, then_shape, else_block, else_shape))
    }

    fn join_branch_shapes(
        &mut self,
        annotation_shape: Option<&TypeShape>,
        then_shape: &TypeShape,
        else_shape: &TypeShape,
        messages: (&'static str, &'static str),
        span: Span,
    ) -> Option<TypeShape> {
        if let Some(annotation_shape) = annotation_shape {
            if !compatible(annotation_shape, then_shape)
                || !compatible(annotation_shape, else_shape)
            {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    messages.0,
                    span,
                ));
                return None;
            }
            Some(annotation_shape.clone())
        } else if let Some(shape) = compatible_shape(then_shape, else_shape) {
            Some(shape)
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                messages.1,
                span,
            ));
            None
        }
    }

    fn infer_yield_block_shape(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        block: &YieldBlock,
        env: &LocalEnvironment,
        state: &BodyState,
    ) -> Option<TypeShape> {
        // `resolved` is a shared borrow for this checker's whole lifetime, so
        // every reachable `YieldBlock` stays immutable and alive. That makes
        // its address a stable per-compilation identity; owned or replaceable
        // AST storage must use a different cache key.
        let cache_key = std::ptr::from_ref(block).addr();
        if let Some(shape) = self.inferred_yield_shapes.get(&cache_key) {
            return Some(shape.clone());
        }
        // Shape probes are observational. Recovered environments retain failed
        // names without values; only the real branch checks publish diagnostics
        // and decide whether a Core block can be constructed.
        let nested_env = self.infer_yield_block_env(intent, output_shape, block, env, state)?;
        let shape = self.infer_expr_shape(&block.value, &nested_env);
        if let Some(shape) = &shape {
            self.inferred_yield_shapes.insert(cache_key, shape.clone());
        }
        shape
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_joint_yield_shape(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        then_block: &YieldBlock,
        else_block: &YieldBlock,
        env: &LocalEnvironment,
        state: &BodyState,
    ) -> Option<TypeShape> {
        let then_env = self.infer_yield_block_env(intent, output_shape, then_block, env, state)?;
        let else_env = self.infer_yield_block_env(intent, output_shape, else_block, env, state)?;
        self.infer_joint_expr_shape(&then_block.value, &then_env, &else_block.value, &else_env)
    }

    fn infer_yield_block_env(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        block: &YieldBlock,
        env: &LocalEnvironment,
        state: &BodyState,
    ) -> Option<LocalEnvironment> {
        // `resolved` is a shared borrow for this checker's whole lifetime, so
        // every reachable `YieldBlock` stays immutable and alive. That makes
        // its address a stable per-compilation identity; owned or replaceable
        // AST storage must use a different cache key.
        let cache_key = std::ptr::from_ref(block).addr();
        if let Some(env) = self.inferred_yield_envs.get(&cache_key) {
            return Some(env.clone());
        }
        let mut checker = self.clone();
        let mut nested_env = env.clone();
        let mut nested_locals = Vec::new();
        let mut nested_state = BodyState {
            local_index: state.local_index,
            obstruction_index: state.obstruction_index,
            step_factor: state.step_factor,
            accumulated_steps: state.accumulated_steps,
            ..BodyState::default()
        };
        for stmt in &block.stmts {
            if matches!(stmt, Stmt::Return { .. }) {
                return None;
            }
            checker.check_body_stmt(
                intent,
                output_shape,
                stmt,
                &mut nested_env,
                &mut nested_locals,
                &mut nested_state,
            );
        }
        self.inferred_yield_shapes
            .append(&mut checker.inferred_yield_shapes);
        self.inferred_yield_envs
            .append(&mut checker.inferred_yield_envs);
        self.inferred_yield_envs
            .insert(cache_key, nested_env.clone());
        Some(nested_env)
    }

    fn infer_joint_expr_shape(
        &self,
        then_expr: &Expr,
        then_env: &LocalEnvironment,
        else_expr: &Expr,
        else_env: &LocalEnvironment,
    ) -> Option<TypeShape> {
        if is_bare_integer_literal(then_expr) {
            return self.infer_expr_shape(else_expr, else_env);
        }
        if is_bare_integer_literal(else_expr) {
            return self.infer_expr_shape(then_expr, then_env);
        }
        if let (
            Expr::Record {
                entries: then_entries,
                ..
            },
            Expr::Record {
                entries: else_entries,
                ..
            },
        ) = (then_expr, else_expr)
        {
            let then_entries = record_entries_by_name(then_entries)?;
            let else_entries = record_entries_by_name(else_entries)?;
            if then_entries.len() != else_entries.len() {
                return None;
            }
            let mut fields = BTreeMap::new();
            for (name, then_entry) in then_entries {
                let else_entry = else_entries.get(name)?;
                let shape = self
                    .infer_joint_record_field_shape(then_entry, then_env, else_entry, else_env)?;
                fields.insert(name.to_owned(), shape);
            }
            return TypeShape::canonical_structural(TypeKind::Record(fields));
        }
        if contains_contextual_bare_integer(then_expr)
            && !contains_contextual_bare_integer(else_expr)
        {
            return self.infer_expr_shape(else_expr, else_env);
        }
        if contains_contextual_bare_integer(else_expr)
            && !contains_contextual_bare_integer(then_expr)
        {
            return self.infer_expr_shape(then_expr, then_env);
        }
        let then_shape = self.infer_expr_shape(then_expr, then_env)?;
        let else_shape = self.infer_expr_shape(else_expr, else_env)?;
        compatible_shape(&then_shape, &else_shape)
    }

    fn infer_joint_record_field_shape(
        &self,
        then_entry: &RecordEntry,
        then_env: &LocalEnvironment,
        else_entry: &RecordEntry,
        else_env: &LocalEnvironment,
    ) -> Option<TypeShape> {
        match (then_entry, else_entry) {
            (
                RecordEntry::Field {
                    value: then_value, ..
                },
                RecordEntry::Field {
                    value: else_value, ..
                },
            ) => self.infer_joint_expr_shape(then_value, then_env, else_value, else_env),
            (RecordEntry::Shorthand { name, .. }, RecordEntry::Field { value, .. }) => {
                let known = &then_env.get(name)?.1;
                if contains_contextual_bare_integer(value) {
                    Some(known.clone())
                } else {
                    compatible_shape(known, &self.infer_expr_shape(value, else_env)?)
                }
            }
            (RecordEntry::Field { value, .. }, RecordEntry::Shorthand { name, .. }) => {
                let known = &else_env.get(name)?.1;
                if contains_contextual_bare_integer(value) {
                    Some(known.clone())
                } else {
                    compatible_shape(&self.infer_expr_shape(value, then_env)?, known)
                }
            }
            (
                RecordEntry::Shorthand {
                    name: then_name, ..
                },
                RecordEntry::Shorthand {
                    name: else_name, ..
                },
            ) => compatible_shape(&then_env.get(then_name)?.1, &else_env.get(else_name)?.1),
            (RecordEntry::Spread(_), _) | (_, RecordEntry::Spread(_)) => None,
        }
    }

    fn infer_expr_shape(&self, expr: &Expr, env: &LocalEnvironment) -> Option<TypeShape> {
        let mut checker = self.clone();
        checker
            .check_expr_with_expected(expr, env, None)
            .map(|value| value.ty)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_yield_block(
        &mut self,
        intent: &ResolvedIntent,
        output_shape: &TypeShape,
        block: &YieldBlock,
        env: &LocalEnvironment,
        state: &mut BodyState,
        expected: Option<&TypeShape>,
        expected_unavailable: bool,
    ) -> Option<(CoreBlock, TypeShape)> {
        let error_count = self.errors.len();
        let mut nested_env = env.clone();
        let mut nested_locals = Vec::new();
        let mut nested_state = BodyState {
            local_index: state.local_index,
            obstruction_index: state.obstruction_index,
            step_factor: state.step_factor,
            accumulated_steps: state.accumulated_steps,
            ..BodyState::default()
        };
        for stmt in &block.stmts {
            if let Stmt::Return { span, .. } = stmt {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "return is not legal inside a branch-yield block",
                    *span,
                ));
                continue;
            }
            self.check_body_stmt(
                intent,
                output_shape,
                stmt,
                &mut nested_env,
                &mut nested_locals,
                &mut nested_state,
            );
        }
        if !self.check_known_effect_profiles(intent, &block.value) {
            return None;
        }
        let value = if expected_unavailable && expected.is_none() {
            self.check_expr_with_unavailable_expected_type(&block.value, &nested_env)
        } else {
            self.check_expr_with_expected(&block.value, &nested_env, expected)
        }?;
        state.local_index = nested_state.local_index;
        state.obstruction_index = nested_state.obstruction_index;
        state.accumulated_steps = nested_state.accumulated_steps;
        if self.errors.len() != error_count {
            return None;
        }
        Some((
            CoreBlock {
                locals: nested_locals,
                nodes: nested_state.nodes,
                result: value.expr,
            },
            value.ty,
        ))
    }

    fn pure_let_binding_shape(
        &mut self,
        stmt: &LetStatement<'_>,
        value: &TypedValue,
        annotation_shape: Option<TypeShape>,
    ) -> Option<TypeShape> {
        let Some(annotation_shape) = annotation_shape else {
            return Some(value.ty.clone());
        };
        if compatible(&annotation_shape, &value.ty) {
            Some(annotation_shape)
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("`{}` initializer does not match annotation", stmt.name),
                stmt.span,
            ));
            None
        }
    }

    fn check_effectful_let(
        &mut self,
        intent: &ResolvedIntent,
        stmt: LetStatement<'_>,
        handler: &ObstructionHandler,
        env: &mut LocalEnvironment,
        locals: &mut Vec<LocalRef>,
        state: &mut BodyState,
    ) {
        let profile_matches = self.check_effect_profile(intent, stmt.value, stmt.span);
        let Some(binding_shape) = self.effect_binding_shape(stmt.ty, stmt.span) else {
            self.check_effect_input(stmt.value, env, stmt.span);
            self.check_failed_effect_handler(&stmt, handler, state);
            return;
        };
        let Some((effect, input)) =
            self.check_effect_call(stmt.value, env, &binding_shape, stmt.span)
        else {
            self.check_failed_effect_handler(&stmt, handler, state);
            return;
        };
        if !profile_matches {
            self.check_failed_effect_handler(&stmt, handler, state);
            return;
        }
        let local = next_local(&mut state.local_index, binding_shape.coord.clone());
        locals.push(local.clone());
        let Some(obstruction_map) = self.check_obstruction_handler(
            handler,
            &effect,
            &mut state.obstruction_index,
            locals,
            stmt.span,
        ) else {
            return;
        };
        state.nodes.push(CoreNode::Effect {
            binding: local.clone(),
            effect,
            input,
            obstruction_map,
        });
        env.insert(stmt.name.to_owned(), (local, binding_shape));
    }

    fn check_failed_effect_handler(
        &mut self,
        stmt: &LetStatement<'_>,
        handler: &ObstructionHandler,
        state: &BodyState,
    ) {
        let Some(effect) = effect_coordinate(stmt.value) else {
            return;
        };
        if let Some(signature) = self.resolved.effect_signatures.get(&effect) {
            if !self.fact_matches_source_import(&effect, &signature.coordinate, &signature.lawpack)
                || !signature.type_parameters.is_empty()
            {
                return;
            }
        }
        let mut obstruction_index = state.obstruction_index;
        let mut diagnostic_locals = Vec::new();
        self.check_obstruction_handler(
            handler,
            &effect,
            &mut obstruction_index,
            &mut diagnostic_locals,
            stmt.span,
        );
    }

    fn check_effect_profile(&mut self, intent: &ResolvedIntent, call: &Expr, span: Span) -> bool {
        if let Some(alias) = self.capability_import_alias_for_call(call) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                format!(
                    "external-action capability `{alias}` is request data, not a semantic effect"
                ),
                span,
            ));
            return false;
        }
        let Some(effect) = effect_coordinate(call) else {
            self.unsupported_stmt(span, "effect call");
            return false;
        };
        let Some(write_class) = self.resolved.effect_write_classes.get(&effect) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                self.missing_effect_context_message(&effect),
                span,
            ));
            return false;
        };
        if let Some(failure) = effect_profile_error(
            &effect,
            write_class,
            &intent.profile,
            intent.allowed_write_classes.as_ref(),
            span,
        ) {
            self.errors.push(failure);
            false
        } else {
            true
        }
    }

    fn capability_import_alias_for_call(&self, call: &Expr) -> Option<String> {
        let Expr::Call { callee, .. } = call else {
            return None;
        };
        let alias = plain_path_root(callee)?;
        self.resolved
            .imports
            .iter()
            .any(|import| {
                import.kind == CoreImportKind::Capability && import.alias.as_deref() == Some(alias)
            })
            .then(|| alias.to_owned())
    }

    fn effect_binding_shape(&mut self, ty: Option<&TypeRef>, span: Span) -> Option<TypeShape> {
        let Some(ty) = ty else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "effectful `let ... else` requires an explicit result type annotation",
                span,
            ));
            return None;
        };
        self.type_ref_shape(ty, span)
    }

    fn check_effect_call(
        &mut self,
        call: &Expr,
        env: &LocalEnvironment,
        binding_shape: &TypeShape,
        span: Span,
    ) -> Option<(String, CoreExpr)> {
        let (effect, input, output) = self.check_effect_input(call, env, span)?;
        if let Some(output) = output {
            if let Some(failure) = signature_diagnostic::mismatch(
                &effect,
                SignatureMismatchPosition::Receipt,
                binding_shape,
                &output,
                expr_span(call),
            ) {
                self.errors.push(failure);
                return None;
            }
        }
        Some((effect, input.expr))
    }

    fn check_effect_input(
        &mut self,
        call: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<(String, TypedValue, Option<TypeShape>)> {
        let effect = effect_coordinate(call)?;
        let Expr::Call {
            args, type_args, ..
        } = call
        else {
            self.unsupported_stmt(span, "effect call");
            return None;
        };
        if !type_args.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "effectful `let ... else` does not support effect type arguments",
                span,
            ));
            return None;
        }
        let [arg] = args.as_slice() else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "effectful `let ... else` supports exactly one effect argument",
                span,
            ));
            return None;
        };
        let mut output_shape = None;
        let (input, signature) = if let Some(signature) =
            self.resolved.effect_signatures.get(&effect).cloned()
        {
            if !self.fact_matches_source_import(&effect, &signature.coordinate, &signature.lawpack)
            {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::MissingContextFact,
                    format!("effect `{effect}` has no exact non-generic lawpack signature"),
                    span,
                ));
                return None;
            }
            if !signature.type_parameters.is_empty() {
                self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        format!("generic semantic effects such as `{effect}` are not supported by source compilation"),
                        span,
                    ));
                return None;
            }
            let expected = self.effect_input_shape(&effect, &signature, span)?;
            let input = self.check_expr_with_expected(arg, env, Some(&expected))?;
            (input, Some((signature, expected)))
        } else {
            (self.check_expr(arg, env)?, None)
        };
        if let Some((signature, expected_input)) = signature {
            let effect_output = self.effect_output_shape(&effect, &signature, span)?;
            if let Some(failure) = signature_diagnostic::mismatch(
                &effect,
                SignatureMismatchPosition::Input,
                &expected_input,
                &input.ty,
                expr_span(call),
            ) {
                self.errors.push(failure);
                return None;
            }
            output_shape = Some(effect_output);
        }
        Some((effect, input, output_shape))
    }

    fn check_obstruction_handler(
        &mut self,
        handler: &ObstructionHandler,
        effect: &str,
        obstruction_index: &mut usize,
        locals: &mut Vec<LocalRef>,
        span: Span,
    ) -> Option<BTreeMap<String, CoreObstructionArm>> {
        let ObstructionHandler::Map(arms) = handler else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "effectful `let ... else` supports obstruction maps only",
                span,
            ));
            return None;
        };
        let mut arms_by_failure = BTreeMap::new();
        let mut accepted = true;
        for arm in arms {
            if arms_by_failure.insert(arm.failure.as_str(), arm).is_some() {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::DuplicateObstructionFailure,
                    format!(
                        "obstruction map repeats failure key `{}` in effect `{effect}`",
                        arm.failure
                    ),
                    arm.span,
                ));
                accepted = false;
            }
        }
        if !accepted {
            // Duplicate keys invalidate the entire map, but each source arm can
            // still contain an independent error. Inspect all arms (including
            // overwritten duplicates) without allocating locals or publishing
            // a recovered obstruction map.
            for arm in arms {
                let _ = self.check_obstruction_arm(effect, arm, *obstruction_index);
            }
            return None;
        }

        let mut out = BTreeMap::new();
        for arm in arms_by_failure.values() {
            let Some((failure, obstruction_arm)) =
                self.check_obstruction_arm(effect, arm, *obstruction_index)
            else {
                continue;
            };
            *obstruction_index += 1;
            locals.push(obstruction_arm.binder.clone());
            out.insert(failure, obstruction_arm);
        }
        if out.len() == arms.len() {
            Some(out)
        } else {
            None
        }
    }

    fn check_obstruction_arm(
        &mut self,
        effect: &str,
        arm: &ObstructionArm,
        obstruction_index: usize,
    ) -> Option<(String, CoreObstructionArm)> {
        if arm.binder.is_none() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "effect obstruction map arms require binders in the first lowerable subset",
                arm.span,
            ));
            return None;
        }
        let binder_type = if let Some(signature) =
            self.resolved.effect_signatures.get(effect).cloned()
        {
            let Some(payload_type) = signature.failure_payload_types.get(&arm.failure).cloned()
            else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::MissingContextFact,
                    format!(
                        "effect `{effect}` has no authenticated payload type for failure `{}`",
                        arm.failure
                    ),
                    arm.span,
                ));
                return None;
            };
            let Some(payload_shape) =
                self.shape_for_helper_coordinate(&payload_type, &signature.lawpack)
            else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnresolvedType,
                    format!(
                        "effect `{effect}` failure `{}` payload type `{payload_type}` is outside its exact exported type closure",
                        arm.failure
                    ),
                    arm.span,
                ));
                return None;
            };
            payload_shape.value_type_coord()
        } else {
            // Legacy facts carry no failure-payload authority and this source
            // subset cannot read the binder. Preserve that erasure as Unit.
            "Unit".to_owned()
        };
        let binder = LocalRef {
            id: format!("obstruction.{obstruction_index}"),
            alpha_name: format!("$obstruction{obstruction_index}"),
            ty: binder_type,
        };
        let value = self.check_obstruction_target(&arm.target)?;
        Some((arm.failure.clone(), CoreObstructionArm { binder, value }))
    }

    fn check_obstruction_target(&mut self, target: &ObstructionTarget) -> Option<CoreExpr> {
        if self.reject_capability_obstruction_root(&target.coordinate, target.span) {
            return None;
        }
        if target.payload.is_some() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "obstruction payload expressions are outside the first effectful Core subset",
                target.span,
            ));
            return None;
        }
        Some(CoreExpr::Call {
            callee: path_key(&target.coordinate),
            type_args: Vec::new(),
            args: Vec::new(),
        })
    }

    fn reject_capability_obstruction_root(&mut self, coordinate: &[String], span: Span) -> bool {
        if coordinate
            .first()
            .is_some_and(|root| self.capability_import_alias(root))
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "capability imports are legal only as request operations",
                span,
            ));
            true
        } else {
            false
        }
    }

    fn capability_import_alias(&self, alias: &str) -> bool {
        self.resolved.imports.iter().any(|import| {
            import.kind == CoreImportKind::Capability && import.alias.as_deref() == Some(alias)
        })
    }

    fn check_known_effect_profiles(&mut self, intent: &ResolvedIntent, expr: &Expr) -> bool {
        let mut accepted = true;
        if let Some(effect) = effect_coordinate(expr) {
            if self.resolved.effect_write_classes.contains_key(&effect) {
                accepted &= self.check_effect_profile(intent, expr, expr_span(expr));
            }
        }
        match expr {
            Expr::Ident { .. }
            | Expr::Int { .. }
            | Expr::Str { .. }
            | Expr::Bytes { .. }
            | Expr::Bool { .. }
            | Expr::Digest { .. } => {}
            Expr::Field { base, .. } => {
                accepted &= self.check_known_effect_profiles(intent, base);
            }
            Expr::Call { callee, args, .. } => {
                accepted &= self.check_known_effect_profiles(intent, callee);
                for arg in args {
                    accepted &= self.check_known_effect_profiles(intent, arg);
                }
            }
            Expr::Unary { operand, .. } => {
                accepted &= self.check_known_effect_profiles(intent, operand);
            }
            Expr::Binary { lhs, rhs, .. } => {
                accepted &= self.check_known_effect_profiles(intent, lhs);
                accepted &= self.check_known_effect_profiles(intent, rhs);
            }
            Expr::Record { entries, .. } => {
                for entry in entries {
                    accepted &= self.check_known_effect_profiles_in_record_entry(intent, entry);
                }
            }
            Expr::List { items, .. } => {
                for item in items {
                    accepted &= self.check_known_effect_profiles(intent, item);
                }
            }
            Expr::If {
                cond, then, els, ..
            } => {
                accepted &= self.check_known_effect_profiles(intent, cond);
                accepted &= self.check_known_effect_profiles(intent, then);
                accepted &= self.check_known_effect_profiles(intent, els);
            }
            Expr::IfYield {
                pred,
                then_block,
                else_block,
                ..
            } => {
                accepted &= self.check_known_effect_profiles(intent, pred);
                accepted &= self.check_known_effect_profiles_in_yield_block(intent, then_block);
                accepted &= self.check_known_effect_profiles_in_yield_block(intent, else_block);
            }
            Expr::VariantLit { payload, .. } => {
                if let Some(payload) = payload {
                    accepted &= self.check_known_effect_profiles(intent, payload);
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                accepted &= self.check_known_effect_profiles(intent, scrutinee);
                for arm in arms {
                    accepted &= self.check_known_effect_profiles(intent, &arm.body);
                }
            }
        }
        accepted
    }

    fn check_known_effect_profiles_in_record_entry(
        &mut self,
        intent: &ResolvedIntent,
        entry: &RecordEntry,
    ) -> bool {
        match entry {
            RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                self.check_known_effect_profiles(intent, value)
            }
            RecordEntry::Shorthand { .. } => true,
        }
    }

    fn check_known_effect_profiles_in_yield_block(
        &mut self,
        intent: &ResolvedIntent,
        block: &YieldBlock,
    ) -> bool {
        let mut accepted = true;
        for stmt in &block.stmts {
            accepted &= self.check_known_effect_profiles_in_stmt(intent, stmt);
        }
        accepted &= self.check_known_effect_profiles(intent, &block.value);
        accepted
    }

    fn check_known_effect_profiles_in_block(
        &mut self,
        intent: &ResolvedIntent,
        block: &Block,
    ) -> bool {
        let mut accepted = true;
        for stmt in &block.stmts {
            accepted &= self.check_known_effect_profiles_in_stmt(intent, stmt);
        }
        accepted
    }

    fn check_known_effect_profiles_in_stmt(
        &mut self,
        intent: &ResolvedIntent,
        stmt: &Stmt,
    ) -> bool {
        match stmt {
            Stmt::Let {
                value, els, span, ..
            } => {
                if els.is_some() {
                    self.check_effect_profile(intent, value, *span)
                } else {
                    self.check_known_effect_profiles(intent, value)
                }
            }
            Stmt::Effect { call, span, .. } => self.check_effect_profile(intent, call, *span),
            Stmt::ExternalActionRequest {
                operation,
                authority_scope,
                basis,
                max_settlement_bytes,
                max_attempts,
                ..
            } => {
                let mut accepted = self.check_known_effect_profiles(intent, operation);
                accepted &= self.check_known_effect_profiles(intent, authority_scope);
                accepted &= self.check_known_effect_profiles(intent, basis);
                accepted &= self.check_known_effect_profiles(intent, max_settlement_bytes);
                accepted &= self.check_known_effect_profiles(intent, max_attempts);
                accepted
            }
            Stmt::Require { predicate, .. }
            | Stmt::Guarantee { predicate, .. }
            | Stmt::Assert { predicate, .. } => self.check_known_effect_profiles(intent, predicate),
            Stmt::If {
                cond,
                then_block,
                els,
                ..
            } => {
                let mut accepted = self.check_known_effect_profiles(intent, cond);
                accepted &= self.check_known_effect_profiles_in_block(intent, then_block);
                if let Some(els) = els {
                    accepted &= self.check_known_effect_profiles_in_else(intent, els);
                }
                accepted
            }
            Stmt::For { iter, body, .. } => {
                let mut accepted = self.check_known_effect_profiles(intent, iter);
                accepted &= self.check_known_effect_profiles_in_block(intent, body);
                accepted
            }
            Stmt::Return { value, .. } => self.check_known_effect_profiles(intent, value),
        }
    }

    fn check_known_effect_profiles_in_else(
        &mut self,
        intent: &ResolvedIntent,
        els: &ElseClause,
    ) -> bool {
        match els {
            ElseClause::Block(block) => self.check_known_effect_profiles_in_block(intent, block),
            ElseClause::If(stmt) => self.check_known_effect_profiles_in_stmt(intent, stmt),
        }
    }

    fn unsupported_stmt(&mut self, span: Span, what: &str) {
        self.errors.push(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::UnsupportedSourceShape,
            format!("{what} is outside the initial lowerable subset"),
            span,
        ));
    }

    fn check_predicate(&mut self, expr: &Expr, env: &LocalEnvironment) -> Option<CorePredicate> {
        let previous = std::mem::replace(&mut self.expected_type_unavailable, false);
        let checked = self.check_predicate_inner(expr, env);
        self.expected_type_unavailable = previous;
        checked
    }

    fn check_predicate_inner(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
    ) -> Option<CorePredicate> {
        let checked = match expr {
            Expr::Bool { value, .. } => Some(if *value {
                CorePredicate::True
            } else {
                CorePredicate::False
            }),
            Expr::Unary {
                op: crate::ast::UnOp::Not,
                operand,
                ..
            } => self
                .check_predicate(operand, env)
                .map(|value| CorePredicate::Not(Box::new(value))),
            Expr::Binary {
                op: BinOp::And,
                lhs,
                rhs,
                ..
            } => {
                let left = self.check_predicate(lhs, env);
                let prefix = self.input_proof_constraints.len();
                if let Some(left) = &left {
                    self.input_proof_constraints.push(left.clone());
                }
                let right = self.check_predicate(rhs, env);
                self.input_proof_constraints.truncate(prefix);
                Some(CorePredicate::All(vec![left?, right?]))
            }
            Expr::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
                ..
            } => {
                let left = self.check_predicate(lhs, env);
                let prefix = self.input_proof_constraints.len();
                if let Some(left) = &left {
                    self.input_proof_constraints
                        .push(CorePredicate::Not(Box::new(left.clone())));
                }
                let right = self.check_predicate(rhs, env);
                self.input_proof_constraints.truncate(prefix);
                Some(CorePredicate::Any(vec![left?, right?]))
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                if let Some(op) = compare_op(*op) {
                    self.check_compare_predicate(op, lhs, rhs, env, expr_span(expr))
                } else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::ExpectedPredicate,
                        "where expression is not a predicate",
                        expr_span(expr),
                    ));
                    None
                }
            }
            Expr::Ident { .. }
            | Expr::Field { .. }
            | Expr::Call { .. }
            | Expr::If { .. }
            | Expr::Match { .. } => {
                let expected = TypeShape::canonical_structural(TypeKind::Bool)?;
                // Check the operand on its own terms before applying the
                // predicate requirement; nested typing failures stay distinct.
                let value = self.check_expr_with_effect_expectation(
                    expr,
                    env,
                    EffectOutputExpectation::Predicate,
                )?;
                if !compatible(&expected, &value.ty) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::ExpectedPredicate,
                        "predicate value must have type Bool",
                        expr_span(expr),
                    ));
                    return None;
                }
                Some(CorePredicate::Compare {
                    op: CompareOp::Eq,
                    left: value.expr,
                    right: CoreExpr::Const(CoreValue::Bool(true)),
                })
            }
            _ => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::ExpectedPredicate,
                    "where expression is not a predicate",
                    expr_span(expr),
                ));
                None
            }
        }?;
        if self.records_source_cost(expr) {
            self.record_source_predicate_cost(expr)?;
        }
        Some(checked)
    }

    fn check_compare_predicate(
        &mut self,
        op: CompareOp,
        lhs: &Expr,
        rhs: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<CorePredicate> {
        // A bare effect always rejects in this value position. Validate its
        // own guards and exported numeric output before inferring its peer.
        if self.is_authenticated_effect_call(lhs) && is_bare_integer_literal(rhs) {
            self.check_expr_with_effect_expectation(
                lhs,
                env,
                EffectOutputExpectation::ComparisonLiteral(rhs),
            )?;
            return None;
        }
        if self.is_authenticated_effect_call(rhs) && is_bare_integer_literal(lhs) {
            self.check_expr_with_effect_expectation(
                rhs,
                env,
                EffectOutputExpectation::ComparisonLiteral(lhs),
            )?;
            return None;
        }
        let (left, right) = match (is_bare_integer_literal(lhs), is_bare_integer_literal(rhs)) {
            (true, false) => {
                let right = self.check_expr(rhs, env)?;
                let left = self.check_expr_with_expected(lhs, env, Some(&right.ty))?;
                (left, right)
            }
            (false, true) => {
                let left = self.check_expr(lhs, env)?;
                let right = self.check_expr_with_expected(rhs, env, Some(&left.ty))?;
                (left, right)
            }
            _ if self.is_authenticated_effect_call(lhs) => {
                let right = self.check_expr(rhs, env);
                let left = self.check_expr_with_effect_expectation(
                    lhs,
                    env,
                    right
                        .as_ref()
                        .map_or(EffectOutputExpectation::Value(None), |right| {
                            EffectOutputExpectation::Comparison(&right.ty)
                        }),
                );
                (left?, right?)
            }
            _ => {
                let left = self.check_expr(lhs, env);
                let right = self.check_expr_with_effect_expectation(
                    rhs,
                    env,
                    left.as_ref()
                        .map_or(EffectOutputExpectation::Value(None), |left| {
                            EffectOutputExpectation::Comparison(&left.ty)
                        }),
                );
                (left?, right?)
            }
        };
        if comparable(&left.ty, &right.ty) {
            Some(CorePredicate::Compare {
                op,
                left: left.expr,
                right: right.expr,
            })
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "comparison operands are not compatible",
                span,
            ));
            None
        }
    }

    fn is_authenticated_effect_call(&self, expr: &Expr) -> bool {
        if let Expr::Field { base, .. } = expr {
            return self.is_authenticated_effect_call(base);
        }
        let Expr::Call { callee, .. } = expr else {
            return false;
        };
        let Some(coordinate) = plain_callee_coordinate(callee) else {
            return false;
        };
        self.resolved
            .effect_signatures
            .get(&coordinate)
            .is_some_and(|signature| {
                self.fact_matches_source_import(
                    &coordinate,
                    &signature.coordinate,
                    &signature.lawpack,
                )
            })
    }

    fn check_expr(&mut self, expr: &Expr, env: &LocalEnvironment) -> Option<TypedValue> {
        self.check_expr_with_expected(expr, env, None)
    }

    fn check_negated_integer(
        &mut self,
        operand: &Expr,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let Expr::Int { value, suffix, .. } = operand else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "negated expression is outside the initial lowerable subset",
                span,
            ));
            return None;
        };
        self.check_integer_literal(&format!("-{value}"), *suffix, expected, span)
    }

    fn check_expr_with_expected(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
    ) -> Option<TypedValue> {
        self.check_expr_with_effect_expectation(expr, env, EffectOutputExpectation::Value(expected))
    }

    fn check_expr_with_effect_expectation(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
    ) -> Option<TypedValue> {
        let expected = expectation.value();
        let checked = match expr {
            Expr::Ident { name, span } => self.check_ident(name, *span, env),
            Expr::Str { value, .. } => string_value(value),
            Expr::Bytes { value, .. } => bytes_value(value),
            Expr::Bool { value, .. } => Some(TypedValue {
                expr: CoreExpr::Const(CoreValue::Bool(*value)),
                ty: TypeShape::canonical_structural(TypeKind::Bool)?,
            }),
            Expr::Int {
                value,
                suffix,
                span,
            } => self.check_integer_literal(value, *suffix, expected, *span),
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                span,
            } => self.check_negated_integer(operand, expected, *span),
            Expr::Field { base, field, span } => {
                self.check_field(base, field, *span, env, expectation)
            }
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                span,
            } => self.check_addition_or_concat(lhs, rhs, env, expectation, *span),
            Expr::Binary {
                op: BinOp::Sub,
                lhs,
                rhs,
                span,
            } => self.check_unsigned_subtraction(lhs, rhs, env, expected, *span),
            Expr::Record { entries, span } => self.check_record(entries, env, expected, *span),
            Expr::List { items, span } => self.check_list_literal(items, env, expected, *span),
            Expr::Call { .. } => self.check_call_expression(expr, env, expectation),
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.check_variant_match(scrutinee, arms, env, expected, *span),
            Expr::VariantLit {
                ty_path,
                case,
                payload,
                span,
            } => self.check_variant_constructor(
                ty_path,
                case,
                payload.as_deref(),
                env,
                expected,
                *span,
            ),
            Expr::If {
                cond,
                then,
                els,
                span,
            } => self.check_pure_conditional(cond, then, els, env, expectation, *span),
            Expr::Binary {
                op:
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or,
                ..
            }
            | Expr::Unary { op: UnOp::Not, .. } => self.check_predicate_value(expr, env),
            Expr::Binary { .. } | Expr::Digest { .. } | Expr::IfYield { .. } => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnsupportedSourceShape,
                    "expression is outside the initial lowerable subset",
                    expr_span(expr),
                ));
                None
            }
        };
        if self.records_source_cost(expr) {
            if let Some(value) = &checked {
                self.record_source_expression_cost(expr, &value.ty)?;
            }
        }
        checked
    }

    fn meters_list_work(&self) -> bool {
        self.list_work_pass == list_operations::WorkPass::Meter
    }

    fn records_source_cost(&self, expr: &Expr) -> bool {
        self.meters_list_work()
            || !self.resolved.source_functions.is_empty()
            || self.match_depth > 0
            || matches!(expr, Expr::Match { .. })
    }

    fn check_call_expression(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
    ) -> Option<TypedValue> {
        let Expr::Call {
            callee,
            type_args,
            args,
            span,
        } = expr
        else {
            return None;
        };
        match callee.as_ref() {
            Expr::Ident { name, .. }
                if matches!(
                    name.as_str(),
                    "listLen" | "listAt" | "listAppend" | "listConcat"
                ) =>
            {
                self.check_list_operation(name, type_args, args, env, *span)
            }
            Expr::Ident { name, .. } if name == "fold" => {
                self.check_fold(type_args, args, env, *span)
            }
            Expr::Ident { name, .. } if name == "len" => {
                self.check_byte_length(type_args, args, env, *span)
            }
            Expr::Ident { name, .. } if name == "isUtf8" => {
                self.check_byte_utf8(type_args, args, env, *span, expr)
            }
            Expr::Ident { name, .. } if name == "at" => {
                self.check_byte_at(type_args, args, env, *span)
            }
            Expr::Ident { name, .. } if name == "count" => {
                self.check_byte_count(type_args, args, env, *span, expr)
            }
            Expr::Ident { name, .. } if name == "slice" => {
                self.check_byte_slice(type_args, args, env, *span)
            }
            _ if self.target_pure_intrinsic(callee).is_some() => {
                self.check_target_pure_intrinsic(expr, env, expectation)
            }
            _ => self.check_pure_call(callee, type_args, args, env, expectation, *span),
        }
    }

    fn check_predicate_value(&mut self, expr: &Expr, env: &LocalEnvironment) -> Option<TypedValue> {
        let predicate = self.check_predicate(expr, env)?;
        Some(TypedValue {
            expr: CoreExpr::If {
                predicate: Box::new(predicate),
                then_value: Box::new(CoreExpr::Const(CoreValue::Bool(true))),
                else_value: Box::new(CoreExpr::Const(CoreValue::Bool(false))),
            },
            ty: TypeShape::canonical_structural(TypeKind::Bool)?,
        })
    }

    fn check_pure_call(
        &mut self,
        callee: &Expr,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        if plain_path_root(callee).is_some_and(|root| env.poisoned.contains(root)) {
            for arg in args {
                self.check_expr_with_unavailable_expected_type(arg, env);
            }
            return None;
        }
        if let Expr::Ident { name, .. } = callee {
            if let Some(signature) = self.function_signatures.get(name).cloned() {
                return self.check_source_call(
                    name,
                    &signature,
                    type_args,
                    args,
                    env,
                    expectation.value(),
                    span,
                );
            }
        }
        self.check_imported_pure_call(callee, type_args, args, env, expectation, span)
    }

    fn check_imported_pure_call(
        &mut self,
        callee: &Expr,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        let Some((source_coordinate, fact)) =
            self.resolve_pure_function(callee, type_args, args, env, expectation, span)
        else {
            for arg in args {
                self.check_expr_with_unavailable_expected_type(arg, env);
            }
            return None;
        };
        let expected = expectation.value();
        if self
            .resolved
            .effect_write_classes
            .contains_key(&source_coordinate)
        {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                format!(
                    "coordinate `{source_coordinate}` is classified as a semantic effect and cannot lower as a pure helper"
                ),
                span,
            ));
            return None;
        }
        if !type_args.is_empty() || !fact.type_parameters.is_empty() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                "generic pure-helper calls are outside the initial lowerable subset",
                span,
            ));
            return None;
        }
        if args.len() != fact.parameter_types.len() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!(
                    "pure helper `{source_coordinate}` expects {} arguments, got {}",
                    fact.parameter_types.len(),
                    args.len()
                ),
                span,
            ));
        }

        let core_args = self.check_imported_arguments(args, env, &fact, &source_coordinate, span);

        let Some(return_shape) = self.shape_for_helper_coordinate(&fact.return_type, &fact.lawpack)
        else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!(
                    "pure helper `{source_coordinate}` return type `{}` is not available in the module type closure",
                    fact.return_type
                ),
                span,
            ));
            return None;
        };
        if expected.is_some_and(|expected| !compatible(expected, &return_shape)) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("pure helper `{source_coordinate}` return type does not match its use"),
                span,
            ));
            return None;
        }

        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: fact.coordinate,
                type_args: Vec::new(),
                args: core_args?,
            },
            ty: return_shape,
        })
    }

    fn check_imported_arguments(
        &mut self,
        args: &[Expr],
        env: &LocalEnvironment,
        fact: &PureFunctionFact,
        source_coordinate: &str,
        span: Span,
    ) -> Option<Vec<CoreExpr>> {
        let mut core_args = Vec::with_capacity(args.len());
        let mut accepted = args.len() == fact.parameter_types.len();
        for (arg, parameter_type) in args.iter().zip(&fact.parameter_types) {
            let Some(parameter_shape) =
                self.shape_for_helper_coordinate(parameter_type, &fact.lawpack)
            else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnresolvedType,
                    format!(
                        "pure helper `{source_coordinate}` parameter type `{parameter_type}` is not available in the module type closure"
                    ),
                    span,
                ));
                self.check_expr_with_unavailable_expected_type(arg, env);
                accepted = false;
                continue;
            };
            let Some(value) = self.check_expr_with_expected(arg, env, Some(&parameter_shape))
            else {
                accepted = false;
                continue;
            };
            if !compatible(&parameter_shape, &value.ty) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    format!("argument does not match pure helper `{source_coordinate}` signature"),
                    expr_span(arg),
                ));
                accepted = false;
                continue;
            }
            core_args.push(value.expr);
        }

        for arg in args.iter().skip(fact.parameter_types.len()) {
            self.check_expr_with_unavailable_expected_type(arg, env);
        }

        accepted.then_some(core_args)
    }

    fn fact_matches_source_import(
        &self,
        source_coordinate: &str,
        fact_coordinate: &str,
        lawpack: &ResourceRef,
    ) -> bool {
        let Some((source_alias, source_suffix)) = source_coordinate.split_once('.') else {
            return false;
        };
        let Some(fact_suffix) = fact_coordinate
            .strip_prefix(&lawpack.coordinate)
            .and_then(|suffix| suffix.strip_prefix('.'))
        else {
            return false;
        };
        !source_alias.is_empty()
            && !source_suffix.is_empty()
            && source_suffix == fact_suffix
            && self.resolved.imports.iter().any(|import| {
                import.kind == CoreImportKind::Lawpack
                    && &import.resource == lawpack
                    && import.alias.as_deref() == Some(source_alias)
            })
    }

    fn missing_effect_context_message(&self, effect: &str) -> String {
        let Some((alias, _)) = effect.split_once('.') else {
            return format!("effect `{effect}` has no compiler context fact");
        };
        let Some(import) = self.resolved.imports.iter().find(|import| {
            import.kind == CoreImportKind::Lawpack && import.alias.as_deref() == Some(alias)
        }) else {
            return format!("effect `{effect}` has no compiler context fact");
        };
        let available = self
            .resolved
            .effect_signatures
            .iter()
            .filter(|(source, fact)| {
                source
                    .split_once('.')
                    .is_some_and(|(source_alias, _)| source_alias == alias)
                    && fact.lawpack == import.resource
                    && self.fact_matches_source_import(source, &fact.coordinate, &fact.lawpack)
            })
            .map(|(source, _)| source.as_str())
            .collect::<Vec<_>>();
        if available.is_empty() {
            if let Some(surface) = self
                .resolved
                .authenticated_effect_exports
                .get(alias)
                .filter(|surface| surface.lawpack == import.resource)
            {
                if surface.names.is_empty() {
                    return format!("effect `{effect}` has no compiler context fact; imported lawpack `{}` has no authenticated effect exports", import.resource.coordinate);
                }
                let names = surface.names.iter().cloned().collect::<Vec<_>>().join(", ");
                return format!("effect `{effect}` has no compiler context fact; authenticated effect exports from imported lawpack `{}` lack compiler signatures in this context: {names}", import.resource.coordinate);
            }
            return format!(
                "effect `{effect}` has no compiler context fact; missing authenticated effect export information for imported lawpack `{}`",
                import.resource.coordinate
            );
        }
        format!(
            "effect `{effect}` has no compiler context fact; available authenticated effect exports in this compiler context for imported lawpack `{}`: {}",
            import.resource.coordinate,
            available.join(", ")
        )
    }

    fn bare_effect_profile_error(&self, effect: &str, span: Span) -> Option<CompilerError> {
        let Some(profile) = self.effect_binding_profile.as_ref() else {
            return Some(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnsupportedSourceShape,
                format!("semantic effect `{effect}` is unavailable in this pure context"),
                span,
            ));
        };
        let Some(write_class) = self.resolved.effect_write_classes.get(effect) else {
            return Some(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::MissingContextFact,
                self.missing_effect_context_message(effect),
                span,
            ));
        };
        effect_profile_error(
            effect,
            write_class,
            &profile.name,
            profile.allowed_write_classes.as_ref(),
            span,
        )
    }

    fn effect_input_shape(
        &mut self,
        effect: &str,
        signature: &EffectSignatureFact,
        span: Span,
    ) -> Option<TypeShape> {
        self.shape_for_helper_coordinate(&signature.input_type, &signature.lawpack)
            .or_else(|| {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnresolvedType,
                    format!(
                        "effect `{effect}` input type `{}` is outside its exact exported type closure",
                        signature.input_type
                    ),
                    span,
                ));
                None
            })
    }

    fn effect_output_shape(
        &mut self,
        effect: &str,
        signature: &EffectSignatureFact,
        span: Span,
    ) -> Option<TypeShape> {
        self.shape_for_helper_coordinate(&signature.output_type, &signature.lawpack)
            .or_else(|| {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::UnresolvedType,
                    format!("effect `{effect}` output type `{}` is outside its exact exported type closure", signature.output_type),
                    span,
                ));
                None
            })
    }

    fn check_bare_effect_input(
        &mut self,
        effect: &str,
        signature: &EffectSignatureFact,
        arg: &Expr,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<()> {
        let expected = self.effect_input_shape(effect, signature, span)?;
        let input = self.check_expr_with_expected(arg, env, Some(&expected))?;
        if compatible(&expected, &input.ty) {
            Some(())
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("effect `{effect}` call does not match its exported signature"),
                span,
            ));
            None
        }
    }

    fn resolve_pure_function(
        &mut self,
        callee: &Expr,
        type_args: &[TypeRef],
        args: &[Expr],
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<(String, PureFunctionFact)> {
        let Some(source_coordinate) = plain_callee_coordinate(callee) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedFunction,
                "pure-helper callee is not a stable imported coordinate",
                span,
            ));
            return None;
        };
        let resolved = self.resolved;
        if let Some(signature) = resolved.effect_signatures.get(&source_coordinate) {
            if self.fact_matches_source_import(
                &source_coordinate,
                &signature.coordinate,
                &signature.lawpack,
            ) {
                if !type_args.is_empty() {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "semantic effect calls do not support effect type arguments",
                        span,
                    ));
                    return None;
                }
                if args.len() != 1 {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "semantic effect calls support exactly one effect argument",
                        span,
                    ));
                    return None;
                }
                if !signature.type_parameters.is_empty() {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        format!("generic semantic effects such as `{source_coordinate}` are not supported by source compilation"),
                        span,
                    ));
                    return None;
                }
                if let Some(failure) = self.bare_effect_profile_error(&source_coordinate, span) {
                    self.errors.push(failure);
                    return None;
                }
                self.check_bare_effect_input(&source_coordinate, signature, &args[0], env, span)?;
                self.check_bare_effect_output(
                    &source_coordinate,
                    signature,
                    expectation,
                    env,
                    span,
                )?;
                self.errors
                    .push(effect_mapping_error(&source_coordinate, signature, span));
                return None;
            }
        }
        let Some(fact) = self
            .resolved
            .pure_functions
            .get(&source_coordinate)
            .cloned()
        else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedFunction,
                format!("pure helper `{source_coordinate}` has no compiler context fact"),
                span,
            ));
            return None;
        };
        if !self.fact_matches_source_import(&source_coordinate, &fact.coordinate, &fact.lawpack) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedFunction,
                format!(
                    "pure helper `{source_coordinate}` is not owned by an exact imported lawpack"
                ),
                span,
            ));
            return None;
        }
        Some((source_coordinate, fact))
    }

    fn check_bare_effect_output(
        &mut self,
        effect: &str,
        signature: &EffectSignatureFact,
        expectation: EffectOutputExpectation<'_>,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<()> {
        let output = self.effect_output_shape(effect, signature, span)?;
        self.check_effect_shape(effect, &output, expectation, env, span)
    }

    fn check_effect_shape(
        &mut self,
        effect: &str,
        output: &TypeShape,
        expectation: EffectOutputExpectation<'_>,
        env: &LocalEnvironment,
        span: Span,
    ) -> Option<()> {
        let (projected, terminal) = match expectation.project(output) {
            Ok(projection) => projection,
            Err((kind, span, name)) => {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    kind,
                    format!("effect `{effect}` result cannot select field `{name}`"),
                    span,
                ));
                return None;
            }
        };
        if let Some(kind) = terminal.rejection(projected) {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                kind,
                format!("effect `{effect}` output does not match the expected type"),
                expectation.origin(span),
            ));
            return None;
        }
        if let EffectOutputExpectation::ComparisonLiteral(peer) = terminal {
            self.check_expr_with_expected(peer, env, Some(projected))?;
        }
        match terminal {
            EffectOutputExpectation::ConcatenationPeer { peer, next } => {
                self.check_effect_concat_peer(projected, peer, env)?;
                if let Some(peer_shape) = self.diagnostic_expr_shape(peer, env) {
                    let Some(combined) = diagnostic_concat_shape(projected, &peer_shape) else {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::InvalidBound,
                            "concatenation maximum exceeds its bounded domain",
                            span,
                        ));
                        return None;
                    };
                    self.check_effect_shape(effect, &combined, *next, env, span)?;
                }
            }
            EffectOutputExpectation::ConditionalPeer { peer, next } => {
                // A peer probe already traverses both branches. Recursively
                // launching peer probes here would multiply that work.
                if self.checking_diagnostic_peer {
                    return self.check_effect_shape(effect, projected, *next, env, span);
                }
                let expected = next
                    .value()
                    .or_else(|| contains_contextual_bare_integer(peer).then_some(projected));
                let peer_shape = match self.check_effect_conditional_peer(peer, env, expected) {
                    DiagnosticPeerShape::Known(shape) => shape,
                    DiagnosticPeerShape::Unknown => return Some(()),
                    DiagnosticPeerShape::Rejected => return None,
                };
                let Some(joined) = compatible_shape(projected, &peer_shape) else {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "conditional branches do not have compatible bounded types",
                        span,
                    ));
                    return None;
                };
                self.check_effect_shape(effect, &joined, *next, env, span)?;
            }
            _ => {}
        }
        Some(())
    }

    fn check_effect_conditional_peer(
        &mut self,
        peer: &Expr,
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
    ) -> DiagnosticPeerShape {
        let mut probe = self.clone();
        probe.errors.clear();
        probe.checking_diagnostic_peer = true;
        let shape = probe
            .check_expr_with_expected(peer, env, expected)
            .map(|value| value.ty);
        probe
            .errors
            .retain(|error| error.kind != CompilerErrorKind::EffectWithoutFailureMapping);
        if !probe.errors.is_empty() {
            self.errors.extend(probe.errors);
            return DiagnosticPeerShape::Rejected;
        }
        shape
            .or_else(|| self.diagnostic_expr_shape(peer, env))
            .map_or(DiagnosticPeerShape::Unknown, DiagnosticPeerShape::Known)
    }

    fn diagnostic_expr_shape(&self, expr: &Expr, env: &LocalEnvironment) -> Option<TypeShape> {
        // This fork reads authenticated signatures only. It cannot authorize
        // effects or fabricate a Core expression for an unbound effect.
        let mut probe = self.clone();
        probe.diagnostic_shape_inner(expr, env)
    }

    fn diagnostic_shape_inner(&mut self, expr: &Expr, env: &LocalEnvironment) -> Option<TypeShape> {
        match expr {
            Expr::Field { base, field, .. } => {
                let base = self.diagnostic_shape_inner(base, env)?;
                let TypeKind::Record(fields) = base.kind else {
                    return None;
                };
                fields.get(field).cloned()
            }
            Expr::Call { callee, span, .. } => {
                let coordinate = plain_callee_coordinate(callee)?;
                if let Some(signature) = self.resolved.effect_signatures.get(&coordinate).cloned() {
                    if self.fact_matches_source_import(
                        &coordinate,
                        &signature.coordinate,
                        &signature.lawpack,
                    ) {
                        return self.effect_output_shape(&coordinate, &signature, *span);
                    }
                }
                self.infer_expr_shape(expr, env)
            }
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => self.diagnostic_addition_shape(lhs, rhs, env),
            Expr::Binary {
                op: BinOp::Sub,
                lhs,
                rhs,
                ..
            } => self
                .diagnostic_shape_inner(lhs, env)
                .and_then(|shape| match shape.kind {
                    TypeKind::Int { .. } => Some(shape),
                    _ => None,
                })
                .or_else(|| self.diagnostic_shape_inner(rhs, env)),
            Expr::If { then, els, .. } => compatible_shape(
                &self.diagnostic_shape_inner(then, env)?,
                &self.diagnostic_shape_inner(els, env)?,
            ),
            Expr::Record { entries, .. } => {
                let fields = entries
                    .iter()
                    .map(|entry| match entry {
                        RecordEntry::Field { name, value } => {
                            Some((name.clone(), self.diagnostic_shape_inner(value, env)?))
                        }
                        RecordEntry::Shorthand { name, .. } => {
                            Some((name.clone(), env.get(name)?.1.clone()))
                        }
                        RecordEntry::Spread(_) => None,
                    })
                    .collect::<Option<BTreeMap<_, _>>>()?;
                TypeShape::canonical_structural(TypeKind::Record(fields))
            }
            _ => self.infer_expr_shape(expr, env),
        }
    }

    fn check_effect_concat_peer(
        &mut self,
        output: &TypeShape,
        peer: &Expr,
        env: &LocalEnvironment,
    ) -> Option<()> {
        let family = if matches!(output.kind, TypeKind::Bytes { .. }) {
            EffectOutputExpectation::Bytes
        } else {
            EffectOutputExpectation::String
        };
        let mut probe = self.clone();
        probe.errors.clear();
        if let Some(value) = probe.check_expr_with_effect_expectation(peer, env, family) {
            if let Some(kind) = family.rejection(&value.ty) {
                probe.errors.push(error(
                    CompilerStage::TypeCheck,
                    kind,
                    "concatenation requires matching string or byte operand families",
                    expr_span(peer),
                ));
            }
        }
        probe
            .errors
            .retain(|error| error.kind != CompilerErrorKind::EffectWithoutFailureMapping);
        if probe.errors.is_empty() {
            Some(())
        } else {
            self.errors.extend(probe.errors);
            None
        }
    }

    fn shape_for_helper_coordinate(
        &mut self,
        coordinate: &str,
        lawpack: &ResourceRef,
    ) -> Option<TypeShape> {
        let shape = if let Some(fact) = self
            .resolved
            .type_shapes
            .get(coordinate)
            .filter(|fact| {
                &fact.lawpack == lawpack
                    && coordinate_is_below_lawpack(&fact.coordinate, &fact.lawpack)
            })
            .cloned()
        {
            self.shape_from_imported_type(&fact)?
        } else {
            imported_type_definition_shape(
                coordinate,
                &self.resolved.type_shapes,
                lawpack,
                &mut BTreeSet::new(),
                None,
                1,
            )?
        };
        self.core_types.extend(imported_type_shape_closure(
            &shape,
            &self.resolved.type_shapes,
        ));
        Some(shape)
    }

    fn shape_from_imported_type(&self, fact: &TypeShapeFact) -> Option<TypeShape> {
        let mut resolving = BTreeSet::from([fact.coordinate.clone()]);
        let mut shape = imported_type_definition_shape(
            &fact.definition,
            &self.resolved.type_shapes,
            &fact.lawpack,
            &mut resolving,
            Some(&fact.coordinate),
            1,
        )?;
        shape.coord.clone_from(&fact.coordinate);
        shape.preserve_named_identity = true;
        Some(shape)
    }

    fn check_pure_conditional(
        &mut self,
        cond: &Expr,
        then: &Expr,
        els: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        let expected = expectation.value();
        let predicate = self.check_predicate(cond, env);
        let then_expectation = EffectOutputExpectation::ConditionalPeer {
            peer: els,
            next: &expectation,
        };
        let else_expectation = EffectOutputExpectation::ConditionalPeer {
            peer: then,
            next: &expectation,
        };
        let (then_value, else_value) = match expected {
            None if is_bare_integer_literal(then) => {
                let else_value = self.check_conditional_arm(
                    els,
                    env,
                    else_expectation,
                    predicate.as_ref(),
                    false,
                );
                let then_value = else_value
                    .as_ref()
                    .and_then(|value| self.check_expr_with_expected(then, env, Some(&value.ty)));
                (then_value, else_value)
            }
            None if is_bare_integer_literal(els) => {
                let then_value = self.check_conditional_arm(
                    then,
                    env,
                    then_expectation,
                    predicate.as_ref(),
                    true,
                );
                let else_value = then_value
                    .as_ref()
                    .and_then(|value| self.check_expr_with_expected(els, env, Some(&value.ty)));
                (then_value, else_value)
            }
            Some(_) | None => (
                self.check_conditional_arm(then, env, then_expectation, predicate.as_ref(), true),
                self.check_conditional_arm(els, env, else_expectation, predicate.as_ref(), false),
            ),
        };

        if then_value.is_none() || else_value.is_none() {
            if let Some(expected) = expected {
                for (value, source) in [(&then_value, then), (&else_value, els)] {
                    if value
                        .as_ref()
                        .is_some_and(|value| !compatible(expected, &value.ty))
                    {
                        self.errors.push(error(
                            CompilerStage::TypeCheck,
                            CompilerErrorKind::TypeMismatch,
                            "conditional arm does not match its expected type",
                            expr_span(source),
                        ));
                    }
                }
            }
            return None;
        }
        let then_value = then_value?;
        let else_value = else_value?;
        let ty = self.join_branch_shapes(
            expected,
            &then_value.ty,
            &else_value.ty,
            (
                "conditional branches do not match the expected type",
                "conditional branches do not have a compatible bounded type",
            ),
            span,
        )?;

        Some(TypedValue {
            expr: CoreExpr::If {
                predicate: Box::new(predicate?),
                then_value: Box::new(then_value.expr),
                else_value: Box::new(else_value.expr),
            },
            ty,
        })
    }

    fn check_conditional_arm(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        predicate: Option<&CorePredicate>,
        truth: bool,
    ) -> Option<TypedValue> {
        let before = self.input_proof_constraints.len();
        if let Some(predicate) = predicate {
            self.input_proof_constraints.push(if truth {
                predicate.clone()
            } else {
                CorePredicate::Not(Box::new(predicate.clone()))
            });
        }
        let checked = self.check_expr_with_effect_expectation(expr, env, expectation);
        self.input_proof_constraints.truncate(before);
        checked
    }

    fn check_expr_with_intrinsic_effect_context(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
    ) -> Option<TypedValue> {
        let previous = std::mem::replace(&mut self.expected_type_unavailable, false);
        let value = self.check_expr_with_effect_expectation(expr, env, expectation);
        self.expected_type_unavailable = previous;
        value
    }

    fn check_expr_with_unavailable_expected_type(
        &mut self,
        expr: &Expr,
        env: &LocalEnvironment,
    ) -> Option<TypedValue> {
        let previous = self.expected_type_unavailable;
        self.expected_type_unavailable = true;
        let value = self.check_expr(expr, env);
        self.expected_type_unavailable = previous;
        value
    }

    fn check_integer_literal(
        &mut self,
        value: &str,
        suffix: Option<IntSuffix>,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        if suffix.is_none() && expected.is_none() && self.expected_type_unavailable {
            return None;
        }
        let width = if let Some(suffix) = suffix {
            integer_suffix_width(suffix)
        } else {
            let Some(TypeShape {
                kind: TypeKind::Int { width },
                ..
            }) = expected
            else {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    "bare integer literal has no unambiguous expected width",
                    span,
                ));
                return None;
            };
            width
        };
        if parse_core_integer(width, value).is_none() {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                format!("integer literal is outside the exact `{width}` domain"),
                span,
            ));
            return None;
        }
        Some(TypedValue {
            expr: CoreExpr::Const(CoreValue::Int {
                width: width.to_owned(),
                value: value.to_owned(),
            }),
            ty: integer_shape(width),
        })
    }

    fn check_ident(
        &mut self,
        name: &str,
        span: Span,
        env: &LocalEnvironment,
    ) -> Option<TypedValue> {
        if env.poisoned.contains(name) {
            return None;
        }
        if let Some((local, ty)) = env.get(name) {
            Some(TypedValue {
                expr: CoreExpr::Local {
                    reference: local.clone(),
                },
                ty: ty.clone(),
            })
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnresolvedType,
                format!("identifier `{name}` has no typed binding"),
                span,
            ));
            None
        }
    }

    fn check_field(
        &mut self,
        base: &Expr,
        field: &str,
        span: Span,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
    ) -> Option<TypedValue> {
        let base = self.check_expr_with_effect_expectation(
            base,
            env,
            EffectOutputExpectation::Field {
                name: field,
                next: &expectation,
                span,
            },
        )?;
        let TypeKind::Record(fields) = &base.ty.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "field access requires a record base",
                span,
            ));
            return None;
        };
        if let Some(field_ty) = fields.get(field) {
            Some(TypedValue {
                expr: CoreExpr::Field {
                    base: Box::new(base.expr),
                    field: field.to_owned(),
                },
                ty: field_ty.clone(),
            })
        } else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::UnknownField,
                format!("record type has no field `{field}`"),
                span,
            ));
            None
        }
    }

    fn check_concat(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        env: &LocalEnvironment,
        expectation: EffectOutputExpectation<'_>,
        span: Span,
    ) -> Option<TypedValue> {
        let left = self.check_expr_with_intrinsic_effect_context(
            lhs,
            env,
            EffectOutputExpectation::ConcatenationPeer {
                peer: rhs,
                next: &expectation,
            },
        );
        let right = self.check_expr_with_intrinsic_effect_context(
            rhs,
            env,
            EffectOutputExpectation::ConcatenationPeer {
                peer: lhs,
                next: &expectation,
            },
        );
        if left.is_none() || right.is_none() {
            for (value, expression) in [(&left, lhs), (&right, rhs)] {
                if value.as_ref().is_some_and(|value| {
                    !matches!(
                        value.ty.kind,
                        TypeKind::String { .. } | TypeKind::Bytes { .. }
                    )
                }) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        "concatenation operand must be a bounded string or bounded bytes",
                        expr_span(expression),
                    ));
                }
            }
            return None;
        }
        let (left, right) = (left?, right?);
        if matches!(
            (&left.ty.kind, &right.ty.kind),
            (TypeKind::Bytes { .. }, TypeKind::Bytes { .. })
        ) {
            return self.check_byte_concat(left, right, span);
        }
        let (TypeKind::String { max: lmax, .. }, TypeKind::String { max: rmax, .. }) =
            (&left.ty.kind, &right.ty.kind)
        else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "concatenation requires two bounded strings or two bounded bytes",
                span,
            ));
            return None;
        };
        let Some(max) = lmax.checked_add(*rmax) else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::InvalidBound,
                "string concatenation maximum exceeds U64",
                span,
            ));
            return None;
        };
        let canonical = "raw-utf8".to_owned();
        let ty = TypeShape::canonical_structural(TypeKind::String { max, canonical })?;
        Some(TypedValue {
            expr: CoreExpr::Call {
                callee: "core.string.concat".to_owned(),
                type_args: Vec::new(),
                args: vec![left.expr, right.expr],
            },
            ty,
        })
    }

    fn check_record(
        &mut self,
        entries: &[RecordEntry],
        env: &LocalEnvironment,
        expected: Option<&TypeShape>,
        span: Span,
    ) -> Option<TypedValue> {
        let expected_fields = expected.and_then(|shape| match &shape.kind {
            TypeKind::Record(fields) => Some(fields),
            _ => None,
        });
        let mut fields = BTreeMap::new();
        let mut field_types = BTreeMap::new();
        let mut field_spans = BTreeMap::new();
        let mut accepted = true;
        for entry in entries {
            match entry {
                RecordEntry::Field { name, value } => {
                    let field_expected = expected_fields.and_then(|fields| fields.get(name));
                    if let Some(checked) = self.check_expr_with_expected(value, env, field_expected)
                    {
                        field_spans.insert(name.clone(), expr_span(value));
                        fields.insert(name.clone(), checked.expr);
                        field_types.insert(name.clone(), checked.ty);
                    } else {
                        accepted = false;
                    }
                }
                RecordEntry::Shorthand { name, span } => {
                    let Some((local, ty)) = env.get(name) else {
                        accepted = false;
                        if !env.poisoned.contains(name) {
                            self.errors.push(error(
                                CompilerStage::TypeCheck,
                                CompilerErrorKind::UnresolvedType,
                                format!("record shorthand `{name}` has no typed binding"),
                                *span,
                            ));
                        }
                        continue;
                    };
                    field_spans.insert(name.clone(), *span);
                    fields.insert(
                        name.clone(),
                        CoreExpr::Local {
                            reference: local.clone(),
                        },
                    );
                    field_types.insert(name.clone(), ty.clone());
                }
                RecordEntry::Spread(_) => {
                    accepted = false;
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::UnsupportedSourceShape,
                        "record spreads are outside the initial lowerable subset",
                        span,
                    ));
                }
            }
        }
        if !accepted {
            self.report_unavailable_record_mismatches(
                entries,
                expected,
                &field_types,
                &field_spans,
                span,
            );
            return None;
        }
        let ty = TypeShape::canonical_structural(TypeKind::Record(field_types))?;
        Some(TypedValue {
            expr: CoreExpr::Record { fields },
            ty,
        })
    }
    fn report_unavailable_record_mismatches(
        &mut self,
        entries: &[RecordEntry],
        expected: Option<&TypeShape>,
        field_types: &BTreeMap<String, TypeShape>,
        field_spans: &BTreeMap<String, Span>,
        span: Span,
    ) {
        let Some(expected) = expected else { return };
        let TypeKind::Record(expected_fields) = &expected.kind else {
            self.errors.push(error(
                CompilerStage::TypeCheck,
                CompilerErrorKind::TypeMismatch,
                "record value does not match its non-record expected type",
                span,
            ));
            return;
        };
        for (name, actual) in field_types {
            if expected_fields
                .get(name)
                .is_some_and(|expected| !compatible(expected, actual))
            {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    format!("record field `{name}` does not match its expected type"),
                    field_spans[name],
                ));
            }
        }
        let mut authored_keys = BTreeSet::new();
        let mut has_spread = false;
        for entry in entries {
            let (name, field_span) = match entry {
                RecordEntry::Field { name, value } => (name, expr_span(value)),
                RecordEntry::Shorthand { name, span } => (name, *span),
                RecordEntry::Spread(_) => {
                    has_spread = true;
                    continue;
                }
            };
            authored_keys.insert(name.as_str());
            if !expected_fields.contains_key(name) {
                self.errors.push(error(
                    CompilerStage::TypeCheck,
                    CompilerErrorKind::TypeMismatch,
                    format!("record has unexpected field `{name}`"),
                    field_span,
                ));
            }
        }
        if !has_spread {
            for name in expected_fields.keys() {
                if !authored_keys.contains(name.as_str()) {
                    self.errors.push(error(
                        CompilerStage::TypeCheck,
                        CompilerErrorKind::TypeMismatch,
                        format!("record is missing expected field `{name}`"),
                        span,
                    ));
                }
            }
        }
    }
}

fn is_bare_integer_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Int { suffix: None, .. } => true,
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => matches!(operand.as_ref(), Expr::Int { suffix: None, .. }),
        _ => false,
    }
}

fn contains_contextual_bare_integer(expr: &Expr) -> bool {
    if is_bare_integer_literal(expr) {
        return true;
    }
    match expr {
        Expr::List { items, .. } => items.iter().any(contains_contextual_bare_integer),
        Expr::Record { entries, .. } => entries.iter().any(|entry| match entry {
            RecordEntry::Field { value, .. } | RecordEntry::Spread(value) => {
                contains_contextual_bare_integer(value)
            }
            RecordEntry::Shorthand { .. } => false,
        }),
        Expr::If { then, els, .. } => {
            contains_contextual_bare_integer(then) || contains_contextual_bare_integer(els)
        }
        Expr::Ident { .. }
        | Expr::Int { .. }
        | Expr::Str { .. }
        | Expr::Bytes { .. }
        | Expr::Bool { .. }
        | Expr::Digest { .. }
        | Expr::Field { .. }
        | Expr::Call { .. }
        | Expr::Unary { .. }
        | Expr::Binary { .. }
        | Expr::IfYield { .. }
        | Expr::VariantLit { .. }
        | Expr::Match { .. } => false,
    }
}

fn record_entries_by_name(entries: &[RecordEntry]) -> Option<BTreeMap<&str, &RecordEntry>> {
    let mut named = BTreeMap::new();
    for entry in entries {
        let name = match entry {
            RecordEntry::Field { name, .. } | RecordEntry::Shorthand { name, .. } => name.as_str(),
            RecordEntry::Spread(_) => return None,
        };
        if named.insert(name, entry).is_some() {
            return None;
        }
    }
    Some(named)
}

fn compatible_shape(left: &TypeShape, right: &TypeShape) -> Option<TypeShape> {
    if let (
        TypeKind::Bytes {
            min: left_min,
            max: left_max,
        },
        TypeKind::Bytes {
            min: right_min,
            max: right_max,
        },
    ) = (&left.kind, &right.kind)
    {
        let joined_min = match (left_min, right_min) {
            (Some(left), Some(right)) => Some((*left).min(*right)),
            (None, _) | (_, None) => None,
        };
        let joined_max = (*left_max).max(*right_max);
        let joined_kind = TypeKind::Bytes {
            min: joined_min,
            max: joined_max,
        };
        return joined_shape(left, right, joined_kind);
    }
    if let (
        TypeKind::List {
            item: left_item,
            max: left_max,
        },
        TypeKind::List {
            item: right_item,
            max: right_max,
        },
    ) = (&left.kind, &right.kind)
    {
        let joined_item = compatible_shape(left_item, right_item)?;
        let joined_max = (*left_max).max(*right_max);
        return joined_shape(
            left,
            right,
            TypeKind::List {
                item: Box::new(joined_item),
                max: joined_max,
            },
        );
    }
    if let (TypeKind::Record(left_fields), TypeKind::Record(right_fields)) =
        (&left.kind, &right.kind)
    {
        if left_fields.len() != right_fields.len() {
            return None;
        }
        let mut joined_fields = BTreeMap::new();
        for (name, left_field) in left_fields {
            let right_field = right_fields.get(name)?;
            joined_fields.insert(name.clone(), compatible_shape(left_field, right_field)?);
        }
        return joined_shape(left, right, TypeKind::Record(joined_fields));
    }
    let selected = if compatible(left, right) {
        left
    } else if compatible(right, left) {
        right
    } else {
        return None;
    };
    if left.coord == right.coord {
        Some(selected.clone())
    } else {
        TypeShape::canonical_structural(selected.kind.clone())
    }
}

fn joined_shape(left: &TypeShape, right: &TypeShape, kind: TypeKind) -> Option<TypeShape> {
    if left.coord == right.coord && left.kind == kind && right.kind == kind {
        Some(TypeShape {
            coord: left.coord.clone(),
            kind,
            preserve_named_identity: left.preserve_named_identity && right.preserve_named_identity,
        })
    } else {
        TypeShape::canonical_structural(kind)
    }
}

fn string_value(value: &str) -> Option<TypedValue> {
    let max = value.len() as u64;
    let canonical = "raw-utf8".to_owned();
    let kind = TypeKind::String { max, canonical };
    Some(TypedValue {
        expr: CoreExpr::Const(CoreValue::String(value.to_owned())),
        ty: TypeShape::canonical_structural(kind)?,
    })
}

fn bytes_value(value: &[u8]) -> Option<TypedValue> {
    let length = u64::try_from(value.len()).ok()?;
    Some(TypedValue {
        expr: CoreExpr::Const(CoreValue::Bytes(value.to_vec())),
        ty: TypeShape::canonical_structural(TypeKind::Bytes {
            min: Some(length),
            max: length,
        })?,
    })
}

fn next_local(index: &mut usize, ty: String) -> LocalRef {
    let current = *index;
    *index += 1;
    LocalRef {
        id: format!("local.{current}"),
        alpha_name: format!("$local{current}"),
        ty,
    }
}

fn compatible(expected: &TypeShape, actual: &TypeShape) -> bool {
    if let (TypeKind::Record(expected), TypeKind::Record(actual)) = (&expected.kind, &actual.kind) {
        return expected.len() == actual.len()
            && expected.iter().all(|(name, expected_ty)| {
                actual
                    .get(name)
                    .is_some_and(|actual_ty| compatible(expected_ty, actual_ty))
            });
    }
    if expected.coord == actual.coord {
        return true;
    }
    if matches!(expected.kind, TypeKind::Nominal { .. })
        || matches!(actual.kind, TypeKind::Nominal { .. })
    {
        return false;
    }
    match (&expected.kind, &actual.kind) {
        (
            TypeKind::String {
                max: expected_max,
                canonical: expected_canonical,
            },
            TypeKind::String {
                max: actual_max,
                canonical: actual_canonical,
            },
        ) => actual_max <= expected_max && actual_canonical == expected_canonical,
        (TypeKind::Bool, TypeKind::Bool) => true,
        (TypeKind::Int { width: expected }, TypeKind::Int { width: actual }) => expected == actual,
        (
            TypeKind::Bytes {
                min: expected_min,
                max: expected_max,
            },
            TypeKind::Bytes {
                min: actual_min,
                max: actual_max,
            },
        ) => actual_min.unwrap_or(0) >= expected_min.unwrap_or(0) && actual_max <= expected_max,
        (
            TypeKind::List {
                item: expected_item,
                max: expected_max,
            },
            TypeKind::List {
                item: actual_item,
                max: actual_max,
            },
        ) => actual_max <= expected_max && compatible(expected_item, actual_item),
        (
            TypeKind::ExternalActionRequest {
                settlement: expected,
            },
            TypeKind::ExternalActionRequest { settlement: actual },
        ) => compatible(expected, actual),
        _ => false,
    }
}

fn requestable_operation_family(coordinate: &str) -> bool {
    coordinate
        .split(['.', '@'])
        .next()
        .is_some_and(|root| root.eq_ignore_ascii_case("workspace"))
}

fn coordinate_is_below_lawpack(coordinate: &str, lawpack: &ResourceRef) -> bool {
    let prefix = format!("{}.", lawpack.coordinate);
    coordinate
        .strip_prefix(&prefix)
        .is_some_and(|suffix| !suffix.is_empty())
}

fn comparable(left: &TypeShape, right: &TypeShape) -> bool {
    compatible(left, right) || compatible(right, left)
}

fn compare_op(op: BinOp) -> Option<CompareOp> {
    match op {
        BinOp::Eq => Some(CompareOp::Eq),
        BinOp::Ne => Some(CompareOp::Ne),
        BinOp::Lt => Some(CompareOp::Lt),
        BinOp::Le => Some(CompareOp::Le),
        BinOp::Gt => Some(CompareOp::Gt),
        BinOp::Ge => Some(CompareOp::Ge),
        BinOp::Or | BinOp::And | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            None
        }
    }
}

fn imported_type_definition_shape(
    definition: &str,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
    lawpack: &ResourceRef,
    resolving: &mut BTreeSet<String>,
    definition_coordinate: Option<&str>,
    depth: usize,
) -> Option<TypeShape> {
    if depth > MAX_CORE_TYPE_DEPTH {
        return None;
    }
    if definition.starts_with("Nominal<") {
        return imported_nominal_type_definition_shape(
            definition,
            type_shapes,
            lawpack,
            resolving,
            definition_coordinate?,
            depth,
        );
    }
    match classify_core_type_reference(definition)? {
        CoreTypeReference::Intrinsic(core_type) | CoreTypeReference::Structural(core_type) => {
            imported_core_type_shape(
                definition,
                core_type,
                type_shapes,
                lawpack,
                resolving,
                depth,
            )
        }
        CoreTypeReference::Named => {
            let fact = type_shapes.get(definition).filter(|fact| {
                &fact.lawpack == lawpack && coordinate_is_below_lawpack(&fact.coordinate, lawpack)
            })?;
            if !resolving.insert(definition.to_owned()) {
                return None;
            }
            let mut shape = imported_type_definition_shape(
                &fact.definition,
                type_shapes,
                lawpack,
                resolving,
                Some(&fact.coordinate),
                depth + 1,
            )?;
            resolving.remove(definition);
            shape.coord.clone_from(&fact.coordinate);
            shape.preserve_named_identity = true;
            Some(shape)
        }
    }
}

fn imported_core_type_shape(
    definition: &str,
    core_type: CoreType,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
    lawpack: &ResourceRef,
    resolving: &mut BTreeSet<String>,
    depth: usize,
) -> Option<TypeShape> {
    let kind = match core_type {
        CoreType::Bool => TypeKind::Bool,
        CoreType::Int { width } => TypeKind::Int { width },
        CoreType::String { max, canonical } => TypeKind::String { max, canonical },
        CoreType::Bytes { min, max } => TypeKind::Bytes { min, max },
        CoreType::Record { fields } => TypeKind::Record(
            fields
                .into_iter()
                .map(|(name, definition)| {
                    Some((
                        name,
                        imported_type_definition_shape(
                            &definition,
                            type_shapes,
                            lawpack,
                            resolving,
                            None,
                            depth + 1,
                        )?,
                    ))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        ),
        CoreType::List { item, max } => TypeKind::List {
            item: Box::new(imported_type_definition_shape(
                &item,
                type_shapes,
                lawpack,
                resolving,
                None,
                depth + 1,
            )?),
            max,
        },
        CoreType::ExternalActionRequest { settlement } => TypeKind::ExternalActionRequest {
            settlement: Box::new(imported_type_definition_shape(
                &settlement,
                type_shapes,
                lawpack,
                resolving,
                None,
                depth + 1,
            )?),
        },
        CoreType::Unit
        | CoreType::Nominal { .. }
        | CoreType::Variant { .. }
        | CoreType::Option { .. }
        | CoreType::Map { .. }
        | CoreType::CapabilityRef { .. } => return None,
    };
    Some(TypeShape {
        coord: definition.to_owned(),
        kind,
        preserve_named_identity: false,
    })
}

fn imported_type_shape_closure(
    shape: &TypeShape,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
) -> BTreeMap<String, CoreType> {
    let mut closure = BTreeMap::new();
    collect_imported_type_shape_closure(shape, type_shapes, &mut closure);
    closure
}

fn collect_imported_type_shape_closure(
    shape: &TypeShape,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
    closure: &mut BTreeMap<String, CoreType>,
) {
    if type_shapes.contains_key(&shape.coord) {
        closure.insert(shape.coord.clone(), shape.core_type());
    }
    match &shape.kind {
        TypeKind::Record(fields) => {
            for field in fields.values() {
                collect_imported_type_shape_closure(field, type_shapes, closure);
            }
        }
        TypeKind::Variant(cases) => {
            for payload in cases.values().flatten() {
                collect_imported_type_shape_closure(payload, type_shapes, closure);
            }
        }
        TypeKind::Nominal { representation, .. }
        | TypeKind::List {
            item: representation,
            ..
        }
        | TypeKind::ExternalActionRequest {
            settlement: representation,
        } => collect_imported_type_shape_closure(representation, type_shapes, closure),
        TypeKind::Bool
        | TypeKind::Int { .. }
        | TypeKind::String { .. }
        | TypeKind::Bytes { .. } => {}
    }
}

pub(crate) fn imported_type_core_closure(
    coordinates: &[String],
    type_shapes: &BTreeMap<String, TypeShapeFact>,
    lawpack: &ResourceRef,
) -> Option<BTreeMap<String, CoreType>> {
    let mut closure = BTreeMap::new();
    for coordinate in coordinates {
        let shape = if let Some(fact) = type_shapes.get(coordinate).filter(|fact| {
            &fact.lawpack == lawpack && coordinate_is_below_lawpack(&fact.coordinate, lawpack)
        }) {
            let mut resolving = BTreeSet::from([fact.coordinate.clone()]);
            let mut shape = imported_type_definition_shape(
                &fact.definition,
                type_shapes,
                lawpack,
                &mut resolving,
                Some(&fact.coordinate),
                1,
            )?;
            shape.coord.clone_from(&fact.coordinate);
            shape.preserve_named_identity = true;
            shape
        } else {
            imported_type_definition_shape(
                coordinate,
                type_shapes,
                lawpack,
                &mut BTreeSet::new(),
                None,
                1,
            )?
        };
        closure.extend(imported_type_shape_closure(&shape, type_shapes));
    }
    Some(closure)
}

fn imported_nominal_type_definition_shape(
    definition: &str,
    type_shapes: &BTreeMap<String, TypeShapeFact>,
    lawpack: &ResourceRef,
    resolving: &mut BTreeSet<String>,
    contract: &str,
    depth: usize,
) -> Option<TypeShape> {
    let inner = definition.strip_prefix("Nominal<")?.strip_suffix('>')?;
    let representation =
        imported_type_definition_shape(inner, type_shapes, lawpack, resolving, None, depth + 1)?;
    Some(TypeShape {
        coord: contract.to_owned(),
        kind: TypeKind::Nominal {
            contract: contract.to_owned(),
            representation: Box::new(representation),
        },
        preserve_named_identity: true,
    })
}

fn empty_core_block() -> CoreBlock {
    CoreBlock {
        locals: Vec::new(),
        nodes: Vec::new(),
        result: CoreExpr::Const(CoreValue::Null),
    }
}

fn builtin_integer_width(name: &str) -> Option<&'static str> {
    match name {
        "I32" => Some("I32"),
        "I64" => Some("I64"),
        "U8" => Some("U8"),
        "U32" => Some("U32"),
        "U64" => Some("U64"),
        _ => None,
    }
}

const fn integer_suffix_width(suffix: IntSuffix) -> &'static str {
    match suffix {
        IntSuffix::I32 => "I32",
        IntSuffix::I64 => "I64",
        IntSuffix::U8 => "U8",
        IntSuffix::U32 => "U32",
        IntSuffix::U64 => "U64",
    }
}

fn integer_shape(width: &str) -> TypeShape {
    TypeShape {
        coord: width.to_owned(),
        kind: TypeKind::Int {
            width: width.to_owned(),
        },
        preserve_named_identity: false,
    }
}

fn path_key(path: &[String]) -> String {
    path.join(".")
}

fn effect_coordinate(call: &Expr) -> Option<String> {
    if let Expr::Call { callee, .. } = call {
        plain_callee_coordinate(callee)
    } else {
        None
    }
}

fn plain_callee_coordinate(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Field { base, field, .. } => {
            Some(format!("{}.{}", plain_callee_coordinate(base)?, field))
        }
        Expr::Call { .. }
        | Expr::Int { .. }
        | Expr::Str { .. }
        | Expr::Bytes { .. }
        | Expr::Bool { .. }
        | Expr::Digest { .. }
        | Expr::Unary { .. }
        | Expr::List { .. }
        | Expr::Binary { .. }
        | Expr::Record { .. }
        | Expr::If { .. }
        | Expr::IfYield { .. }
        | Expr::VariantLit { .. }
        | Expr::Match { .. } => None,
    }
}

fn plain_path_coordinate(expr: &Expr) -> Option<String> {
    plain_callee_coordinate(expr)
}

fn plain_path_root(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Ident { name, .. } => Some(name),
        Expr::Field { base, .. } => plain_path_root(base),
        Expr::Call { .. }
        | Expr::Int { .. }
        | Expr::Str { .. }
        | Expr::Bytes { .. }
        | Expr::Bool { .. }
        | Expr::Digest { .. }
        | Expr::Unary { .. }
        | Expr::List { .. }
        | Expr::Binary { .. }
        | Expr::Record { .. }
        | Expr::If { .. }
        | Expr::IfYield { .. }
        | Expr::VariantLit { .. }
        | Expr::Match { .. } => None,
    }
}

fn package_coordinate(path: &[String], version: &str) -> String {
    format!("{}@{version}", path.join("."))
}

// The resolved AST remains immutably borrowed for the entire check. Addresses
// are only local memoization keys: they never enter Core, digests or diagnostics.
fn expression_identity(expr: &Expr) -> usize {
    std::ptr::from_ref(expr).addr()
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Ident { span, .. }
        | Expr::Int { span, .. }
        | Expr::Str { span, .. }
        | Expr::Bytes { span, .. }
        | Expr::Bool { span, .. }
        | Expr::Digest { span, .. }
        | Expr::Field { span, .. }
        | Expr::Call { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Binary { span, .. }
        | Expr::List { span, .. }
        | Expr::Record { span, .. }
        | Expr::If { span, .. }
        | Expr::IfYield { span, .. }
        | Expr::VariantLit { span, .. }
        | Expr::Match { span, .. } => *span,
    }
}

fn effect_mapping_error(
    effect: &str,
    signature: &EffectSignatureFact,
    span: Span,
) -> CompilerError {
    let guidance = if signature.failure_payload_types.is_empty() {
        "no declared failures; effect bindings without a failure map are not supported".to_owned()
    } else if signature.domain_mappable_failures.is_empty() {
        "no domain-mappable failures; effect bindings without an authored failure map are not supported".to_owned()
    } else {
        let failures = signature
            .domain_mappable_failures
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        format!("use an annotated effect binding with `else`; domain-mappable failures: {failures}")
    };
    error(
        CompilerStage::TypeCheck,
        CompilerErrorKind::EffectWithoutFailureMapping,
        format!("effect `{effect}` is not a pure helper; {guidance}"),
        span,
    )
}

fn effect_profile_error(
    effect: &str,
    write_class: &WriteClass,
    profile: &str,
    allowed_write_classes: Option<&BTreeSet<WriteClass>>,
    span: Span,
) -> Option<CompilerError> {
    let Some(allowed) = allowed_write_classes else {
        return Some(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::MissingContextFact,
            format!("operation profile `{profile}` has no write-class compiler context fact"),
            span,
        ));
    };
    if allowed.contains(write_class) {
        None
    } else {
        Some(error(
            CompilerStage::TypeCheck,
            CompilerErrorKind::ProfileEffectMismatch,
            format!(
                "effect `{effect}` requires write class {write_class:?}, which profile `{profile}` does not allow"
            ),
            span,
        ))
    }
}

fn error(
    stage: CompilerStage,
    kind: CompilerErrorKind,
    message: impl Into<String>,
    span: Span,
) -> CompilerError {
    CompilerError {
        stage,
        kind,
        message: message.into(),
        span,
        signature_mismatch: None,
    }
}

fn finish<T>(errors: Vec<CompilerError>, value: T) -> Result<T, Vec<CompilerError>> {
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}

fn diagnostic_concat_shape(left: &TypeShape, right: &TypeShape) -> Option<TypeShape> {
    let kind = match (&left.kind, &right.kind) {
        (TypeKind::Bytes { max: left, .. }, TypeKind::Bytes { max: right, .. }) => {
            TypeKind::Bytes {
                min: None,
                max: left.checked_add(*right)?,
            }
        }
        (TypeKind::String { max: left, .. }, TypeKind::String { max: right, .. }) => {
            TypeKind::String {
                max: left.checked_add(*right)?,
                canonical: "raw-utf8".into(),
            }
        }
        _ => return None,
    };
    TypeShape::canonical_structural(kind)
}
