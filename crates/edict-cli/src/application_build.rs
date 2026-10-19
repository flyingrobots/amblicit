mod source_inputs;
use source_inputs::{LoadedSources, SourceArtifact};

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use edict_provider_host_wasmtime::{
    ProviderComponentHost, ProviderHostLimits, ResolvedProviderComponent,
};
use edict_provider_schema::{ProviderArtifactSchemaRegistry, ResolvedProviderSchemaArtifact};
use edict_syntax::{
    bind_target_provider_manifest, compile_to_core, decode_canonical_cbor, decode_lawpack_adapter,
    decode_lawpack_bundle, digest_canonical_artifact, encode_core_module,
    encode_target_ir_artifact, lower_to_target_ir, prepare_lawpack_compilation,
    select_provider_component, validate_lawpack_dependency_graph,
    validate_provider_lowering_request, validate_provider_verification_request,
    verify_result_projection, CanonicalValue, ProviderArtifact, ProviderArtifactBinding,
    ProviderArtifactKind, ProviderArtifactRef, ProviderBoundArtifact, ProviderDigest,
    ProviderDigestAlgorithm, ProviderInvocationKind, ProviderLoweringInvocationContract,
    ProviderLoweringOutputKind, ProviderLoweringOutputRequest, ProviderLoweringRequest,
    ProviderResourceRef, ProviderResponseLimits, ProviderSemanticInput,
    ProviderSemanticInputBinding, ProviderSemanticInputKind,
    ProviderVerificationInvocationContract, ProviderVerificationOutputKind,
    ProviderVerificationOutputRequest, ProviderVerificationRequest, ResultProjectionArtifact,
    TargetIrArtifact, TargetLoweringStatus, TargetProviderManifest, ValidatedLawpackBundle,
    ValidatedTargetProviderManifest, CORE_MODULE_DIGEST_DOMAIN,
    EXTERNAL_ACTION_RESOURCE_API_VERSION, EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN,
    PROVIDER_LAWPACK_ARTIFACT_DOMAIN, RESULT_PROJECTION_DIGEST_DOMAIN,
    TARGET_IR_ARTIFACT_DIGEST_DOMAIN, TARGET_PROFILE_API_VERSION, TARGET_PROVIDER_PROTOCOL_VERSION,
};
use serde::Deserialize;

const APPLICATION_SCHEMA: &str = "edict.application/v1";
const SOURCE_DOMAIN: &str = "edict.source/v1";
const EXPORTS_DOMAIN: &str = "edict.lawpack-exports/v1";
const ADAPTER_DOMAIN: &str = "edict.lawpack-adapter/v1";
const PACKAGE_ROLE: &str = "executable-operation-package.echo";
const PACKAGE_DOMAIN: &str = "echo.operation-package/v1";
const VERIFICATION_REPORT_ROLE: &str = "verifier-report.echo-operation";
const VERIFICATION_REPORT_DOMAIN: &str = "echo.operation-package-verifier-report/v1";
const RESULT_PROJECTION_ROLE: &str = "07-result-projection";
const MAX_APPLICATION_ARTIFACT_BYTES: u64 = 1024 * 1024;
const MAX_EXTERNAL_ACTION_RESOURCES: usize = 192;

#[derive(Debug)]
pub(crate) struct ApplicationBuildFailure {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
    pub(crate) source_diagnostics: Option<Vec<ApplicationSourceDiagnostics>>,
}

#[derive(Debug)]
pub(crate) struct ApplicationSourceDiagnostics {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) errors: Vec<ApplicationSourceError>,
}

#[derive(Debug)]
pub(crate) enum ApplicationSourceError {
    Parse(edict_syntax::ParseError),
    Compiler(edict_syntax::CompilerError),
    Linker(edict_syntax::source_linker::SourceLinkError),
}

impl ApplicationSourceError {
    pub(crate) fn span(&self) -> edict_syntax::Span {
        match self {
            Self::Parse(error) => error.span,
            Self::Compiler(error) => error.span,
            Self::Linker(error) => error.span,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum ApplicationBuildKind {
    #[default]
    ExecutableOperation,
    ExternalAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplicationManifest {
    schema: String,
    #[serde(default)]
    build_kind: ApplicationBuildKind,
    coordinate: String,
    sources: Vec<PathBuf>,
    lawpacks: Vec<ApplicationLawpack>,
    #[serde(default)]
    external_action_resources: Vec<ApplicationExternalActionResource>,
    target: ApplicationTarget,
    output_directory: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplicationExternalActionResource {
    artifact: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplicationLawpack {
    manifest: PathBuf,
    exports: PathBuf,
    adapter: PathBuf,
    target_configuration: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplicationTarget {
    profile: String,
    provider_package: PathBuf,
}

struct LoadedLawpack {
    manifest_bytes: Vec<u8>,
    exports_bytes: Vec<u8>,
    adapter_bytes: Vec<u8>,
    configuration_bytes: Vec<u8>,
    bundle: ValidatedLawpackBundle,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExternalActionResourceKind {
    InputSchema,
    SettlementSchema,
    ReconciliationLaw,
}

impl ExternalActionResourceKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InputSchema => "inputSchema",
            Self::SettlementSchema => "settlementSchema",
            Self::ReconciliationLaw => "reconciliationLaw",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "inputSchema" => Some(Self::InputSchema),
            "settlementSchema" => Some(Self::SettlementSchema),
            "reconciliationLaw" => Some(Self::ReconciliationLaw),
            _ => None,
        }
    }
}

struct LoadedExternalActionResource {
    coordinate: String,
    kind: ExternalActionResourceKind,
    digest: String,
}

struct ProviderInvocationContext<'a> {
    coordinate: &'a str,
    core_bytes: &'a [u8],
    target_profile: &'a ProviderBoundArtifact,
    loaded: &'a LoadedLawpack,
    adapter: &'a edict_syntax::ValidatedLawpackAdapter,
    configuration: &'a edict_syntax::LawpackResourceRef,
    source: &'a SourceArtifact,
    target_ir_bytes: &'a [u8],
    result_projection: &'a ResultProjectionArtifact,
}

#[allow(
    clippy::too_many_lines,
    reason = "the application build keeps its ordered authority-boundary crossing explicit"
)]
pub(crate) fn build_application(config_path: &Path) -> Result<(), ApplicationBuildFailure> {
    let config_bytes = read(
        config_path,
        "ApplicationConfigReadFailed",
        "application config",
    )?;
    let config = serde_json::from_slice::<ApplicationManifest>(&config_bytes).map_err(|error| {
        failure(
            "InvalidApplicationConfig",
            format!(
                "application config `{}` is not valid edict.application/v1 JSON: {error}",
                config_path.display()
            ),
        )
    })?;
    validate_application_manifest(&config)?;
    let root = canonical_application_root(config_path)?;
    let external_action_resources =
        load_external_action_resources(&root, &config.external_action_resources)?;

    let sources = LoadedSources::load(&root, &config.sources)?;
    let linked = sources
        .closure
        .as_ref()
        .map(edict_syntax::source_linker::link_source_closure)
        .transpose()
        .map_err(|error| sources.link_failure(error))?;
    let module = linked
        .as_ref()
        .map_or(&sources.root_module, |linked| linked.module());
    let source_path = &sources.root_path;
    let source = sources.root_source.as_str();

    let mut loaded_lawpacks = Vec::with_capacity(config.lawpacks.len());
    for lawpack in &config.lawpacks {
        loaded_lawpacks.push(load_lawpack(&root, lawpack)?);
    }
    validate_application_lawpack_closure(&loaded_lawpacks)?;
    let loaded = loaded_lawpacks.first().ok_or_else(|| {
        failure(
            "InvalidApplicationConfig",
            "the application build requires a root lawpack",
        )
    })?;

    let provider_root = confined_existing_path(
        &root,
        &config.target.provider_package,
        "target.providerPackage",
        "ProviderPackageReadFailed",
        "provider package",
    )?;
    let manifest_path = find_provider_manifest(&provider_root)?;
    let provider_manifest_bytes = read(
        &manifest_path,
        "ProviderPackageReadFailed",
        "provider manifest",
    )?;
    let provider_manifest = serde_json::from_slice::<TargetProviderManifest>(
        &provider_manifest_bytes,
    )
    .map_err(|error| {
        failure(
            "InvalidProviderPackage",
            format!(
                "provider manifest `{}` is not valid typed JSON: {error}",
                manifest_path.display()
            ),
        )
    })?;
    let provider_proof = bind_target_provider_manifest(&provider_manifest).map_err(|report| {
        failure(
            "InvalidProviderPackage",
            format!("provider manifest validation failed: {report:?}"),
        )
    })?;
    let target_profile_artifact = unique_artifact(
        &provider_manifest,
        ProviderArtifactKind::TargetProfile,
        Some(&config.target.profile),
    )?;
    let target_profile_bytes = read_provider_artifact(
        &provider_root,
        target_profile_artifact,
        ProviderArtifactKind::TargetProfile,
    )?;
    let target_profile = bound_artifact(
        &target_profile_artifact.resource.coordinate,
        TARGET_PROFILE_API_VERSION,
        &target_profile_bytes,
    )?;
    require_manifest_identity(target_profile_artifact, &target_profile)?;

    let adapter = decode_lawpack_adapter(
        &loaded.bundle,
        &config.target.profile,
        &loaded.adapter_bytes,
    )
    .map_err(|failures| {
        failure(
            "InvalidLawpackAdapter",
            format!("lawpack target adapter is invalid: {failures:?}"),
        )
    })?;
    if adapter.target_profile().digest_review_string()
        != rendered_digest(&target_profile.reference.digest)
    {
        return Err(failure(
            "TargetProfileMismatch",
            "lawpack adapter target profile digest does not match the selected provider profile",
        ));
    }
    let preparation =
        prepare_lawpack_compilation(module, &loaded.bundle, &adapter).map_err(|failures| {
            failure(
                "InvalidApplicationClosure",
                format!("source and lawpack closure do not corroborate: {failures:?}"),
            )
        })?;
    let core = if let Some(linked) = &linked {
        linked
            .compile(preparation.compiler_context())
            .map_err(|errors| sources.compiler_failure(errors))?
    } else {
        compile_to_core(module, preparation.compiler_context()).map_err(|errors| {
            source_failure(
                "ApplicationCompilationFailed",
                "Edict application did not compile to Core",
                source_path,
                source,
                errors
                    .into_iter()
                    .map(ApplicationSourceError::Compiler)
                    .collect(),
            )
        })?
    };
    if core.coordinate != config.coordinate {
        return Err(failure(
            "ApplicationCoordinateMismatch",
            format!(
                "application config coordinate `{}` does not match source coordinate `{}`",
                config.coordinate, core.coordinate
            ),
        ));
    }
    let configuration =
        single_configuration_for_core(adapter.operation_profiles(), adapter.effects(), &core)?;
    validate_target_configuration_binding(configuration, &loaded.configuration_bytes)?;
    let target_ir_report = lower_to_target_ir(&core, preparation.target_ir_facts());
    if target_ir_report.status != TargetLoweringStatus::Lowered {
        return Err(failure(
            "TargetLoweringFailed",
            format!(
                "application did not lower through the selected target adapter: {:?}",
                target_ir_report.failures
            ),
        ));
    }
    let target_ir = target_ir_report.artifact.ok_or_else(|| {
        failure(
            "TargetLoweringFailed",
            "target lowering reported success without an artifact",
        )
    })?;

    let core_bytes = encode_core_module(&core).map_err(|error| {
        failure(
            "ApplicationEncodingFailed",
            format!("Core canonical encoding failed: {error}"),
        )
    })?;
    let target_ir_bytes = encode_target_ir_artifact(&target_ir).map_err(|error| {
        failure(
            "ApplicationEncodingFailed",
            format!("Target IR canonical encoding failed: {error}"),
        )
    })?;

    if config.build_kind == ApplicationBuildKind::ExternalAction {
        validate_external_action_artifacts(
            &target_ir,
            &loaded_lawpacks,
            &external_action_resources,
        )?;
        let output_directory = prepare_output_directory(&root, &config.output_directory)?;
        return write_external_action_outputs(&output_directory, &core_bytes, &target_ir_bytes);
    }

    let (result_intent, result_projection) = single_result_projection(
        &target_ir_report.result_projections,
        &target_ir_report.result_projection_failures,
    )?;
    verify_result_projection(
        &core,
        &target_ir,
        result_intent,
        result_projection.canonical_bytes(),
        result_projection.digest(),
    )
    .map_err(|error| {
        failure(
            "ResultProjectionVerificationFailed",
            format!("compiler result projection failed independent verification: {error}"),
        )
    })?;

    let schema_artifacts = provider_schema_artifacts(&provider_manifest)?;
    let resolved_schema_artifacts = schema_artifacts
        .into_iter()
        .map(|schema_artifact| {
            Ok(ResolvedProviderSchemaArtifact {
                role: schema_artifact.role.clone(),
                bytes: Arc::<[u8]>::from(read_provider_artifact(
                    &provider_root,
                    schema_artifact,
                    ProviderArtifactKind::ArtifactSchema,
                )?),
            })
        })
        .collect::<Result<Vec<_>, ApplicationBuildFailure>>()?;
    let required_domains = provider_manifest
        .schema_bindings
        .iter()
        .map(|binding| binding.domain.as_str());
    let registry = ProviderArtifactSchemaRegistry::from_manifest(
        &provider_proof,
        resolved_schema_artifacts,
        required_domains,
    )
    .map_err(|error| {
        failure(
            "InvalidProviderPackage",
            format!("provider artifact-schema registry failed: {error}"),
        )
    })?;

    let source_artifact = sources.artifact()?;

    let lowerer_artifact =
        unique_artifact(&provider_manifest, ProviderArtifactKind::Lowerer, None)?;
    let verifier_artifact =
        unique_artifact(&provider_manifest, ProviderArtifactKind::Verifier, None)?;
    let lowerer_bytes = read_provider_artifact(
        &provider_root,
        lowerer_artifact,
        ProviderArtifactKind::Lowerer,
    )?;
    let verifier_bytes = read_provider_artifact(
        &provider_root,
        verifier_artifact,
        ProviderArtifactKind::Verifier,
    )?;

    let host = ProviderComponentHost::new().map_err(|error| {
        failure(
            "ProviderHostFailed",
            format!("provider host configuration failed: {error}"),
        )
    })?;
    let invocation = ProviderInvocationContext {
        coordinate: &core.coordinate,
        core_bytes: &core_bytes,
        target_profile: &target_profile,
        loaded,
        adapter: &adapter,
        configuration,
        source: &source_artifact,
        target_ir_bytes: &target_ir_bytes,
        result_projection,
    };
    let package_bytes = invoke_lowerer(
        &host,
        provider_proof,
        &registry,
        lowerer_artifact,
        lowerer_bytes,
        &invocation,
    )?;
    let report_bytes = invoke_verifier(
        &host,
        provider_proof,
        &registry,
        verifier_artifact,
        verifier_bytes,
        &invocation,
        &package_bytes,
    )?;
    require_accepted_report(&report_bytes)?;

    let output_directory = prepare_output_directory(&root, &config.output_directory)?;
    write_outputs(&output_directory, &package_bytes, &report_bytes)
}

fn load_external_action_resources(
    root: &Path,
    configured: &[ApplicationExternalActionResource],
) -> Result<Vec<LoadedExternalActionResource>, ApplicationBuildFailure> {
    let mut loaded = Vec::with_capacity(configured.len());
    let mut coordinates = BTreeSet::new();
    for resource in configured {
        let resource = load_external_action_resource(root, resource)?;
        if !coordinates.insert(resource.coordinate.clone()) {
            return Err(failure(
                "ExternalActionResourceClosureMismatch",
                format!(
                    "external-action resource coordinate `{}` is configured more than once",
                    resource.coordinate
                ),
            ));
        }
        loaded.push(resource);
    }
    Ok(loaded)
}

fn load_external_action_resource(
    root: &Path,
    configured: &ApplicationExternalActionResource,
) -> Result<LoadedExternalActionResource, ApplicationBuildFailure> {
    let path = confined_existing_path(
        root,
        &configured.artifact,
        "externalActionResources.artifact",
        "ExternalActionResourceReadFailed",
        "external-action resource",
    )?;
    let bytes = read(
        &path,
        "ExternalActionResourceReadFailed",
        "external-action resource",
    )?;
    let value = decode_canonical_cbor(&bytes).map_err(|error| {
        invalid_external_action_resource(&path, format!("is not canonical CBOR: {error}"))
    })?;
    let fields = external_action_resource_object(
        &value,
        &["apiVersion", "coordinate", "definition", "kind"],
        &path,
        "artifact",
    )?;
    require_external_action_resource_text(
        &fields,
        "apiVersion",
        Some(EXTERNAL_ACTION_RESOURCE_API_VERSION),
        &path,
    )?;
    let coordinate =
        require_external_action_resource_text(&fields, "coordinate", None, &path)?.to_owned();
    let kind_text = require_external_action_resource_text(&fields, "kind", None, &path)?;
    let kind = ExternalActionResourceKind::parse(kind_text).ok_or_else(|| {
        invalid_external_action_resource(&path, format!("kind `{kind_text}` is unsupported"))
    })?;
    let definition = fields
        .get("definition")
        .ok_or_else(|| invalid_external_action_resource(&path, "is missing its `definition`"))?;
    validate_external_action_resource_definition(kind, definition, &path)?;
    let digest = digest_canonical_artifact(EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, &bytes)
        .map_err(|error| {
            invalid_external_action_resource(&path, format!("cannot be identified: {error}"))
        })?
        .to_review_string();
    Ok(LoadedExternalActionResource {
        coordinate,
        kind,
        digest,
    })
}

fn validate_external_action_resource_definition(
    kind: ExternalActionResourceKind,
    definition: &CanonicalValue,
    path: &Path,
) -> Result<(), ApplicationBuildFailure> {
    match kind {
        ExternalActionResourceKind::InputSchema | ExternalActionResourceKind::SettlementSchema => {
            let fields = external_action_resource_object(
                definition,
                &["closed", "encoding", "fields", "root"],
                path,
                "schema definition",
            )?;
            require_external_action_resource_text(
                &fields,
                "encoding",
                Some("canonical-cbor"),
                path,
            )?;
            require_external_action_resource_text(&fields, "root", None, path)?;
            if !matches!(fields.get("closed"), Some(CanonicalValue::Bool(true))) {
                return Err(invalid_external_action_resource(
                    path,
                    "schema definition field `closed` must be true",
                ));
            }
            let Some(CanonicalValue::Array(schema_fields)) = fields.get("fields") else {
                return Err(invalid_external_action_resource(
                    path,
                    "schema definition field `fields` must be a non-empty array",
                ));
            };
            if schema_fields.is_empty() {
                return Err(invalid_external_action_resource(
                    path,
                    "schema definition field `fields` must be a non-empty array",
                ));
            }
            let mut names = BTreeSet::new();
            for field in schema_fields {
                let field = external_action_resource_object(
                    field,
                    &["authority", "name", "required", "type"],
                    path,
                    "schema field",
                )?;
                let name = require_external_action_resource_text(&field, "name", None, path)?;
                require_external_action_resource_text(&field, "type", None, path)?;
                require_external_action_resource_text(&field, "authority", None, path)?;
                if !matches!(field.get("required"), Some(CanonicalValue::Bool(_))) {
                    return Err(invalid_external_action_resource(
                        path,
                        "schema field `required` must be boolean",
                    ));
                }
                if !names.insert(name) {
                    return Err(invalid_external_action_resource(
                        path,
                        "schema field names must be unique",
                    ));
                }
            }
        }
        ExternalActionResourceKind::ReconciliationLaw => {
            let fields = external_action_resource_object(
                definition,
                &[
                    "replayRule",
                    "requestKind",
                    "requiredBindings",
                    "settlementKind",
                    "terminalPostures",
                ],
                path,
                "reconciliation definition",
            )?;
            require_external_action_resource_text(&fields, "requestKind", None, path)?;
            require_external_action_resource_text(&fields, "settlementKind", None, path)?;
            require_external_action_resource_text(&fields, "replayRule", None, path)?;
            let bindings =
                require_external_action_resource_text_array(&fields, "requiredBindings", path)?;
            if bindings.is_empty()
                || bindings.iter().copied().collect::<BTreeSet<_>>().len() != bindings.len()
            {
                return Err(invalid_external_action_resource(
                    path,
                    "reconciliation requiredBindings must be non-empty and unique",
                ));
            }
            let postures =
                require_external_action_resource_text_array(&fields, "terminalPostures", path)?;
            if postures != ["succeeded", "obstructed", "outcomeUnknown"] {
                return Err(invalid_external_action_resource(
                    path,
                    "reconciliation terminalPostures must be succeeded, obstructed, outcomeUnknown",
                ));
            }
        }
    }
    Ok(())
}

fn external_action_resource_object<'a>(
    value: &'a CanonicalValue,
    expected: &[&str],
    path: &Path,
    label: &str,
) -> Result<BTreeMap<&'a str, &'a CanonicalValue>, ApplicationBuildFailure> {
    let CanonicalValue::Map(entries) = value else {
        return Err(invalid_external_action_resource(
            path,
            format!("{label} must be a canonical map"),
        ));
    };
    let mut fields = BTreeMap::new();
    for (key, value) in entries {
        let CanonicalValue::Text(key) = key else {
            return Err(invalid_external_action_resource(
                path,
                format!("{label} has a non-text field name"),
            ));
        };
        if fields.insert(key.as_str(), value).is_some() {
            return Err(invalid_external_action_resource(
                path,
                format!("{label} repeats field `{key}`"),
            ));
        }
    }
    if fields.keys().copied().collect::<BTreeSet<_>>()
        != expected.iter().copied().collect::<BTreeSet<_>>()
    {
        return Err(invalid_external_action_resource(
            path,
            format!("{label} has an unsupported field set"),
        ));
    }
    Ok(fields)
}

fn require_external_action_resource_text<'a>(
    fields: &BTreeMap<&str, &'a CanonicalValue>,
    field: &str,
    exact: Option<&str>,
    path: &Path,
) -> Result<&'a str, ApplicationBuildFailure> {
    let Some(CanonicalValue::Text(value)) = fields.get(field) else {
        return Err(invalid_external_action_resource(
            path,
            format!("resource field `{field}` must be text"),
        ));
    };
    if value.is_empty() || exact.is_some_and(|expected| value != expected) {
        return Err(invalid_external_action_resource(
            path,
            format!("resource field `{field}` has an invalid value"),
        ));
    }
    Ok(value)
}

fn require_external_action_resource_text_array<'a>(
    fields: &BTreeMap<&str, &'a CanonicalValue>,
    field: &str,
    path: &Path,
) -> Result<Vec<&'a str>, ApplicationBuildFailure> {
    let Some(CanonicalValue::Array(values)) = fields.get(field) else {
        return Err(invalid_external_action_resource(
            path,
            format!("resource field `{field}` must be an array"),
        ));
    };
    values
        .iter()
        .map(|value| match value {
            CanonicalValue::Text(value) if !value.is_empty() => Ok(value.as_str()),
            _ => Err(invalid_external_action_resource(
                path,
                format!("resource field `{field}` must contain non-empty text"),
            )),
        })
        .collect()
}

fn invalid_external_action_resource(
    path: &Path,
    message: impl std::fmt::Display,
) -> ApplicationBuildFailure {
    failure(
        "InvalidExternalActionResource",
        format!("external-action resource `{}` {message}", path.display()),
    )
}

fn validate_external_action_artifacts(
    target_ir: &TargetIrArtifact,
    loaded_lawpacks: &[LoadedLawpack],
    loaded_resources: &[LoadedExternalActionResource],
) -> Result<(), ApplicationBuildFailure> {
    validate_external_action_execution_class(target_ir)?;
    validate_external_action_capability_closure(target_ir, loaded_lawpacks)?;
    let required_resources = required_external_action_resources(target_ir)?;
    validate_external_action_resource_closure(&required_resources, loaded_resources)
}

fn validate_external_action_execution_class(
    target_ir: &TargetIrArtifact,
) -> Result<(), ApplicationBuildFailure> {
    let request_count = target_ir
        .intents
        .values()
        .map(|intent| intent.external_action_requests.len())
        .sum::<usize>();
    if request_count == 0 {
        return Err(failure(
            "ExternalActionRequestUnavailable",
            "external-action application build requires at least one typed request",
        ));
    }
    if target_ir
        .intents
        .values()
        .any(|intent| !intent.steps.is_empty())
    {
        return Err(failure(
            "MixedApplicationExecution",
            "external-action application build cannot mix requests with callable target steps",
        ));
    }
    Ok(())
}

fn validate_external_action_capability_closure(
    target_ir: &TargetIrArtifact,
    loaded_lawpacks: &[LoadedLawpack],
) -> Result<(), ApplicationBuildFailure> {
    let capability_manifest_digests = loaded_lawpacks
        .iter()
        .map(|loaded| loaded.bundle.manifest_digest_review_string())
        .collect::<BTreeSet<_>>();
    let substituted = target_ir.intents.values().find_map(|intent| {
        intent
            .external_action_requests
            .iter()
            .map(|request| &request.operation)
            .find(|operation| {
                !operation
                    .digest
                    .as_ref()
                    .is_some_and(|digest| capability_manifest_digests.contains(digest))
            })
    });
    if let Some(operation) = substituted {
        return Err(failure(
            "ExternalActionCapabilityClosureMismatch",
            format!(
                "request operation `{}` is not bound to one exact root-reachable application lawpack manifest digest",
                operation.coordinate
            ),
        ));
    }
    Ok(())
}

fn required_external_action_resources(
    target_ir: &TargetIrArtifact,
) -> Result<BTreeMap<(String, ExternalActionResourceKind), String>, ApplicationBuildFailure> {
    let mut required_resources = BTreeMap::new();
    for request in target_ir
        .intents
        .values()
        .flat_map(|intent| &intent.external_action_requests)
    {
        for (kind, resource) in [
            (
                ExternalActionResourceKind::InputSchema,
                &request.input_schema,
            ),
            (
                ExternalActionResourceKind::SettlementSchema,
                &request.settlement_schema,
            ),
            (
                ExternalActionResourceKind::ReconciliationLaw,
                &request.reconciliation_law,
            ),
        ] {
            let Some(digest) = resource.digest.as_deref() else {
                return Err(failure(
                    "ExternalActionResourceClosureMismatch",
                    format!(
                        "request resource `{}` is not digest locked",
                        resource.coordinate
                    ),
                ));
            };
            let key = (resource.coordinate.clone(), kind);
            if required_resources
                .insert(key, digest.to_owned())
                .is_some_and(|existing| existing != digest)
            {
                return Err(failure(
                    "ExternalActionResourceClosureMismatch",
                    format!(
                        "request resource `{}` has conflicting digests",
                        resource.coordinate
                    ),
                ));
            }
        }
    }
    Ok(required_resources)
}

fn validate_external_action_resource_closure(
    required_resources: &BTreeMap<(String, ExternalActionResourceKind), String>,
    loaded_resources: &[LoadedExternalActionResource],
) -> Result<(), ApplicationBuildFailure> {
    let supplied = loaded_resources
        .iter()
        .map(|resource| (resource.coordinate.as_str(), resource))
        .collect::<BTreeMap<_, _>>();
    for ((coordinate, kind), digest) in required_resources {
        let Some(resource) = supplied.get(coordinate.as_str()) else {
            return Err(failure(
                "ExternalActionResourceClosureMismatch",
                format!(
                    "request {kind} `{coordinate}` has no configured canonical artifact",
                    kind = kind.as_str()
                ),
            ));
        };
        if resource.kind != *kind || resource.digest != *digest {
            return Err(failure(
                "ExternalActionResourceClosureMismatch",
                format!(
                    "request {kind} `{coordinate}` does not match its configured canonical artifact",
                    kind = kind.as_str()
                ),
            ));
        }
    }
    let required_coordinates = required_resources
        .keys()
        .map(|(coordinate, _)| coordinate.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(disconnected) = loaded_resources
        .iter()
        .find(|resource| !required_coordinates.contains(resource.coordinate.as_str()))
    {
        return Err(failure(
            "ExternalActionResourceClosureMismatch",
            format!(
                "configured external-action resource `{}` is not referenced by the Target request closure",
                disconnected.coordinate
            ),
        ));
    }
    Ok(())
}

fn validate_application_lawpack_closure(
    loaded_lawpacks: &[LoadedLawpack],
) -> Result<(), ApplicationBuildFailure> {
    let bundles = loaded_lawpacks
        .iter()
        .map(|loaded| loaded.bundle.clone())
        .collect::<Vec<_>>();
    validate_lawpack_dependency_graph(&bundles).map_err(|failures| {
        failure(
            "InvalidLawpackClosure",
            format!("application lawpack dependency closure is invalid: {failures:?}"),
        )
    })?;
    let root = loaded_lawpacks.first().ok_or_else(|| {
        failure(
            "InvalidLawpackClosure",
            "application lawpack dependency closure has no root",
        )
    })?;
    let by_identity = loaded_lawpacks
        .iter()
        .map(|loaded| {
            (
                (
                    loaded.bundle.manifest().id.clone(),
                    loaded.bundle.manifest().version.clone(),
                ),
                &loaded.bundle,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut pending = vec![(
        root.bundle.manifest().id.clone(),
        root.bundle.manifest().version.clone(),
    )];
    let mut reachable = BTreeSet::new();
    while let Some(identity) = pending.pop() {
        if !reachable.insert(identity.clone()) {
            continue;
        }
        let Some(bundle) = by_identity.get(&identity) else {
            return Err(failure(
                "InvalidLawpackClosure",
                format!(
                    "application root dependency closure omitted lawpack `{}@{}`",
                    identity.0, identity.1
                ),
            ));
        };
        pending.extend(
            bundle
                .manifest()
                .dependencies
                .iter()
                .map(|dependency| (dependency.id.clone(), dependency.version.clone())),
        );
    }
    if let Some(unreachable) = by_identity
        .keys()
        .find(|identity| !reachable.contains(*identity))
    {
        return Err(failure(
            "InvalidLawpackClosure",
            format!(
                "supplied lawpack `{}@{}` is unreachable from root `{}@{}`",
                unreachable.0,
                unreachable.1,
                root.bundle.manifest().id,
                root.bundle.manifest().version
            ),
        ));
    }
    Ok(())
}

fn validate_application_manifest(
    config: &ApplicationManifest,
) -> Result<(), ApplicationBuildFailure> {
    if config.schema != APPLICATION_SCHEMA {
        return Err(failure(
            "InvalidApplicationConfig",
            format!(
                "application schema must be `{APPLICATION_SCHEMA}`, got `{}`",
                config.schema
            ),
        ));
    }
    if config.coordinate.is_empty()
        || config.target.profile.is_empty()
        || config.target.provider_package.as_os_str().is_empty()
        || config.output_directory.as_os_str().is_empty()
    {
        return Err(failure(
            "InvalidApplicationConfig",
            "application coordinate, target profile, provider package, and output directory must be non-empty",
        ));
    }
    if config.sources.is_empty()
        || config.sources.len()
            > edict_syntax::source_closure::SourceClosureLimits::default().max_modules
    {
        return Err(failure(
            "InvalidApplicationConfig",
            "the application build requires a root source and at most 63 explicit dependencies",
        ));
    }
    if config.lawpacks.is_empty() {
        return Err(failure(
            "InvalidApplicationConfig",
            "the application build requires one root lawpack followed by its complete dependency closure",
        ));
    }
    if config.external_action_resources.len() > MAX_EXTERNAL_ACTION_RESOURCES {
        return Err(failure(
            "InvalidApplicationConfig",
            format!(
                "externalActionResources exceeds the bounded maximum of {MAX_EXTERNAL_ACTION_RESOURCES}"
            ),
        ));
    }
    if config.build_kind == ApplicationBuildKind::ExecutableOperation
        && !config.external_action_resources.is_empty()
    {
        return Err(failure(
            "InvalidApplicationConfig",
            "executable-operation builds cannot declare external-action resources",
        ));
    }
    let paths = config
        .sources
        .iter()
        .map(|path| ("sources", path.as_path()))
        .chain(config.lawpacks.iter().flat_map(|lawpack| {
            [
                ("lawpacks.manifest", lawpack.manifest.as_path()),
                ("lawpacks.exports", lawpack.exports.as_path()),
                ("lawpacks.adapter", lawpack.adapter.as_path()),
                (
                    "lawpacks.targetConfiguration",
                    lawpack.target_configuration.as_path(),
                ),
            ]
        }))
        .chain(config.external_action_resources.iter().map(|resource| {
            (
                "externalActionResources.artifact",
                resource.artifact.as_path(),
            )
        }))
        .chain([
            (
                "target.providerPackage",
                config.target.provider_package.as_path(),
            ),
            ("outputDirectory", config.output_directory.as_path()),
        ]);
    for (field, path) in paths {
        validate_application_path(field, path)?;
    }
    Ok(())
}

fn validate_application_path(field: &str, path: &Path) -> Result<(), ApplicationBuildFailure> {
    if path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(failure(
            "InvalidApplicationConfig",
            format!(
                "application field `{field}` must be a non-empty relative path without parent traversal"
            ),
        ));
    }
    Ok(())
}

fn load_lawpack(
    root: &Path,
    config: &ApplicationLawpack,
) -> Result<LoadedLawpack, ApplicationBuildFailure> {
    let manifest_path = confined_existing_path(
        root,
        &config.manifest,
        "lawpacks.manifest",
        "LawpackReadFailed",
        "lawpack manifest",
    )?;
    let manifest_bytes = read(&manifest_path, "LawpackReadFailed", "lawpack manifest")?;
    let exports_path = confined_existing_path(
        root,
        &config.exports,
        "lawpacks.exports",
        "LawpackReadFailed",
        "lawpack exports",
    )?;
    let exports_bytes = read(&exports_path, "LawpackReadFailed", "lawpack exports")?;
    let adapter_path = confined_existing_path(
        root,
        &config.adapter,
        "lawpacks.adapter",
        "LawpackReadFailed",
        "lawpack target adapter",
    )?;
    let adapter_bytes = read(&adapter_path, "LawpackReadFailed", "lawpack target adapter")?;
    let configuration_path = confined_existing_path(
        root,
        &config.target_configuration,
        "lawpacks.targetConfiguration",
        "LawpackReadFailed",
        "target configuration",
    )?;
    let configuration_bytes = read(
        &configuration_path,
        "LawpackReadFailed",
        "target configuration",
    )?;
    let bundle = decode_lawpack_bundle(&manifest_bytes, &exports_bytes).map_err(|failures| {
        failure(
            "InvalidLawpackClosure",
            format!("lawpack manifest and exports are invalid: {failures:?}"),
        )
    })?;
    Ok(LoadedLawpack {
        manifest_bytes,
        exports_bytes,
        adapter_bytes,
        configuration_bytes,
        bundle,
    })
}

fn find_provider_manifest(root: &Path) -> Result<PathBuf, ApplicationBuildFailure> {
    let entries = fs::read_dir(root).map_err(|error| {
        failure(
            "ProviderPackageReadFailed",
            format!(
                "failed to read provider package `{}`: {error}",
                root.display()
            ),
        )
    })?;
    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            failure(
                "ProviderPackageReadFailed",
                format!(
                    "failed to enumerate provider package `{}`: {error}",
                    root.display()
                ),
            )
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("provider-manifest.") && name.ends_with(".json") {
            matches.push(entry.path());
        }
    }
    matches.sort();
    match matches.as_slice() {
        [path] => confined_existing_path(
            root,
            path.strip_prefix(root).map_err(|error| {
                failure(
                    "ProviderPackageReadFailed",
                    format!(
                        "provider manifest `{}` is not beneath package root `{}`: {error}",
                        path.display(),
                        root.display()
                    ),
                )
            })?,
            "target.providerPackage.manifest",
            "ProviderPackageReadFailed",
            "provider manifest",
        ),
        _ => Err(failure(
            "InvalidProviderPackage",
            format!(
                "provider package `{}` must contain exactly one provider-manifest.*.json",
                root.display()
            ),
        )),
    }
}

fn unique_artifact<'a>(
    manifest: &'a TargetProviderManifest,
    kind: ProviderArtifactKind,
    coordinate: Option<&str>,
) -> Result<&'a ProviderArtifactRef, ApplicationBuildFailure> {
    let matches = manifest
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.artifact_kind == kind
                && coordinate.is_none_or(|expected| artifact.resource.coordinate == expected)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [artifact] => Ok(*artifact),
        _ => Err(failure(
            "InvalidProviderPackage",
            format!(
                "provider manifest must expose exactly one {kind:?} artifact{}",
                coordinate.map_or(String::new(), |value| format!(" for `{value}`"))
            ),
        )),
    }
}

fn provider_schema_artifacts(
    manifest: &TargetProviderManifest,
) -> Result<Vec<&ProviderArtifactRef>, ApplicationBuildFailure> {
    let bound_roles = manifest
        .schema_bindings
        .iter()
        .map(|binding| binding.schema_role.as_str())
        .collect::<BTreeSet<_>>();
    let mut artifacts = manifest
        .artifacts
        .iter()
        .filter(|artifact| {
            artifact.artifact_kind == ProviderArtifactKind::ArtifactSchema
                && bound_roles.contains(artifact.role.as_str())
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.role.cmp(&right.role));
    if artifacts.len() != bound_roles.len() || artifacts.is_empty() {
        return Err(failure(
            "InvalidProviderPackage",
            "provider manifest must expose every artifact schema named by its schema bindings",
        ));
    }
    Ok(artifacts)
}

fn read_provider_artifact(
    root: &Path,
    artifact: &ProviderArtifactRef,
    kind: ProviderArtifactKind,
) -> Result<Vec<u8>, ApplicationBuildFailure> {
    let relative = match kind {
        ProviderArtifactKind::Lowerer | ProviderArtifactKind::Verifier => {
            PathBuf::from("components").join(format!("{}.component.wasm", artifact.role))
        }
        ProviderArtifactKind::ArtifactSchema => PathBuf::from("generated")
            .join("primary")
            .join(format!("{}.cddl", artifact.role)),
        ProviderArtifactKind::TargetProfile => PathBuf::from("generated")
            .join("primary")
            .join(format!("{}.cbor", artifact.role)),
        _ => {
            return Err(failure(
                "InvalidProviderPackage",
                format!("unsupported provider artifact path kind {kind:?}"),
            ));
        }
    };
    let path = confined_existing_path(
        root,
        &relative,
        "target.providerPackage.artifact",
        "ProviderPackageReadFailed",
        "provider artifact",
    )?;
    read(&path, "ProviderPackageReadFailed", "provider artifact")
}

fn validate_target_configuration_binding(
    reference: &edict_syntax::LawpackResourceRef,
    bytes: &[u8],
) -> Result<(), ApplicationBuildFailure> {
    let digest = provider_digest(&reference.id, bytes)?;
    if reference.digest_review_string() != rendered_digest(&digest) {
        return Err(failure(
            "TargetConfigurationMismatch",
            "target configuration bytes do not reproduce the adapter's digest-locked reference",
        ));
    }
    Ok(())
}

fn invoke_lowerer(
    host: &ProviderComponentHost,
    provider_proof: ValidatedTargetProviderManifest<'_>,
    registry: &ProviderArtifactSchemaRegistry,
    component: &ProviderArtifactRef,
    component_bytes: Vec<u8>,
    invocation: &ProviderInvocationContext<'_>,
) -> Result<Vec<u8>, ApplicationBuildFailure> {
    let selected = select_provider_component(
        &provider_proof,
        &component.role,
        ProviderInvocationKind::Lowering,
    )
    .map_err(|error| {
        failure(
            "InvalidProviderPackage",
            format!("provider lowerer selection failed: {error}"),
        )
    })?;
    let resolved = ResolvedProviderComponent::new(selected, Arc::<[u8]>::from(component_bytes));
    let prepared = host.prepare(&resolved).map_err(|error| {
        failure(
            "ProviderLowererFailed",
            format!("provider lowerer preflight failed: {error}"),
        )
    })?;

    let core = bound_artifact(
        invocation.coordinate,
        CORE_MODULE_DIGEST_DOMAIN,
        invocation.core_bytes,
    )?;
    let semantic_inputs = lowering_inputs(
        invocation.loaded,
        invocation.adapter,
        invocation.configuration,
        invocation.coordinate,
        invocation.source,
        invocation.target_ir_bytes,
        invocation.result_projection,
    )?;
    let contract = lowering_contract(&core, invocation.target_profile, &semantic_inputs);
    let request = ProviderLoweringRequest {
        protocol_version: TARGET_PROVIDER_PROTOCOL_VERSION,
        core,
        target_profile: invocation.target_profile.clone(),
        semantic_inputs,
        requested_outputs: vec![ProviderLoweringOutputRequest {
            role: PACKAGE_ROLE.to_owned(),
            kind: ProviderLoweringOutputKind::GeneratedArtifact,
            domain: PACKAGE_DOMAIN.to_owned(),
        }],
        limits: response_limits(),
    };
    let request =
        validate_provider_lowering_request(registry, &contract, &request).map_err(|report| {
            failure(
                "InvalidProviderInvocation",
                format!("provider lowering request validation failed: {report:?}"),
            )
        })?;
    let outcome = host
        .invoke_lowerer(&prepared, &request, registry, host_limits())
        .map_err(|error| {
            failure(
                "ProviderLowererFailed",
                format!("provider lowerer invocation failed: {error}"),
            )
        })?;
    if let Some(refusal) = outcome.refusal() {
        return Err(failure(
            "ProviderLowererRefused",
            format!("provider lowerer refused application semantics: {refusal:?}"),
        ));
    }
    let response = outcome.response().ok_or_else(|| {
        failure(
            "ProviderLowererFailed",
            "provider lowerer returned neither a response nor a refusal",
        )
    })?;
    if !response.diagnostics.is_empty() {
        return Err(failure(
            "ProviderLowererFailed",
            format!(
                "provider lowerer returned diagnostics: {:?}",
                response.diagnostics
            ),
        ));
    }
    let [output] = response.outputs.as_slice() else {
        return Err(failure(
            "ProviderLowererFailed",
            "provider lowerer did not emit exactly one package",
        ));
    };
    Ok(output.artifact.bytes.clone())
}

fn invoke_verifier(
    host: &ProviderComponentHost,
    provider_proof: ValidatedTargetProviderManifest<'_>,
    registry: &ProviderArtifactSchemaRegistry,
    component: &ProviderArtifactRef,
    component_bytes: Vec<u8>,
    invocation: &ProviderInvocationContext<'_>,
    package_bytes: &[u8],
) -> Result<Vec<u8>, ApplicationBuildFailure> {
    let selected = select_provider_component(
        &provider_proof,
        &component.role,
        ProviderInvocationKind::Verification,
    )
    .map_err(|error| {
        failure(
            "InvalidProviderPackage",
            format!("provider verifier selection failed: {error}"),
        )
    })?;
    let resolved = ResolvedProviderComponent::new(selected, Arc::<[u8]>::from(component_bytes));
    let prepared = host.prepare(&resolved).map_err(|error| {
        failure(
            "ProviderVerifierFailed",
            format!("provider verifier preflight failed: {error}"),
        )
    })?;

    let core = bound_artifact(
        invocation.coordinate,
        CORE_MODULE_DIGEST_DOMAIN,
        invocation.core_bytes,
    )?;
    let target_ir = bound_artifact(
        invocation.adapter.target_ir().id.as_str(),
        TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
        invocation.target_ir_bytes,
    )?;
    let semantic_inputs = verification_inputs(
        invocation.loaded,
        invocation.adapter,
        invocation.configuration,
        invocation.coordinate,
        invocation.source,
        package_bytes,
        invocation.result_projection,
    )?;
    let contract = verification_contract(
        &core,
        invocation.target_profile,
        &target_ir,
        &semantic_inputs,
    );
    let request = ProviderVerificationRequest {
        protocol_version: TARGET_PROVIDER_PROTOCOL_VERSION,
        core,
        target_profile: invocation.target_profile.clone(),
        target_ir,
        semantic_inputs,
        requested_outputs: vec![ProviderVerificationOutputRequest {
            role: VERIFICATION_REPORT_ROLE.to_owned(),
            kind: ProviderVerificationOutputKind::VerifierReport,
            domain: VERIFICATION_REPORT_DOMAIN.to_owned(),
        }],
        limits: response_limits(),
    };
    let request = validate_provider_verification_request(registry, &contract, &request).map_err(
        |report| {
            failure(
                "InvalidProviderInvocation",
                format!("provider verification request validation failed: {report:?}"),
            )
        },
    )?;
    let outcome = host
        .invoke_verifier(&prepared, &request, registry, host_limits())
        .map_err(|error| {
            failure(
                "ProviderVerifierFailed",
                format!("provider verifier invocation failed: {error}"),
            )
        })?;
    if let Some(refusal) = outcome.refusal() {
        return Err(failure(
            "ProviderVerifierRefused",
            format!("provider verifier refused application semantics: {refusal:?}"),
        ));
    }
    let response = outcome.response().ok_or_else(|| {
        failure(
            "ProviderVerifierFailed",
            "provider verifier returned neither a response nor a refusal",
        )
    })?;
    if !response.diagnostics.is_empty() {
        return Err(failure(
            "ProviderVerificationRejected",
            format!(
                "provider verifier rejected the package: {:?}",
                response.diagnostics
            ),
        ));
    }
    let [output] = response.outputs.as_slice() else {
        return Err(failure(
            "ProviderVerifierFailed",
            "provider verifier did not emit exactly one report",
        ));
    };
    Ok(output.artifact.bytes.clone())
}

fn lowering_inputs(
    loaded: &LoadedLawpack,
    adapter: &edict_syntax::ValidatedLawpackAdapter,
    configuration: &edict_syntax::LawpackResourceRef,
    coordinate: &str,
    source: &SourceArtifact,
    target_ir_bytes: &[u8],
    result_projection: &ResultProjectionArtifact,
) -> Result<Vec<ProviderSemanticInput>, ApplicationBuildFailure> {
    let adapter_reference = selected_adapter_reference(
        &loaded.bundle.manifest().target_adapters,
        adapter.target_profile(),
    )?;
    with_result_projection_input(
        vec![
            semantic_input(
                "01-lawpack-adapter",
                ProviderSemanticInputKind::Auxiliary("lawpack-adapter".to_owned()),
                &adapter_reference.id,
                ADAPTER_DOMAIN,
                &loaded.adapter_bytes,
            )?,
            semantic_input(
                "02-lawpack-exports",
                ProviderSemanticInputKind::Auxiliary("lawpack-exports".to_owned()),
                &loaded.bundle.manifest().exports.id,
                EXPORTS_DOMAIN,
                &loaded.exports_bytes,
            )?,
            semantic_input(
                "03-lawpack",
                ProviderSemanticInputKind::Lawpack,
                &format!(
                    "{}@{}",
                    loaded.bundle.manifest().id,
                    loaded.bundle.manifest().version
                ),
                PROVIDER_LAWPACK_ARTIFACT_DOMAIN,
                &loaded.manifest_bytes,
            )?,
            semantic_input(
                "04-source",
                ProviderSemanticInputKind::Auxiliary(source.kind.to_owned()),
                coordinate,
                source.domain,
                &source.bytes,
            )?,
            semantic_input(
                "05-target-configuration",
                ProviderSemanticInputKind::Auxiliary("target-configuration".to_owned()),
                &configuration.id,
                &configuration.id,
                &loaded.configuration_bytes,
            )?,
            semantic_input(
                "06-target-ir",
                ProviderSemanticInputKind::Auxiliary("target-ir".to_owned()),
                &adapter.target_ir().id,
                TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
                target_ir_bytes,
            )?,
        ],
        result_projection,
    )
}

fn verification_inputs(
    loaded: &LoadedLawpack,
    adapter: &edict_syntax::ValidatedLawpackAdapter,
    configuration: &edict_syntax::LawpackResourceRef,
    coordinate: &str,
    source: &SourceArtifact,
    package_bytes: &[u8],
    result_projection: &ResultProjectionArtifact,
) -> Result<Vec<ProviderSemanticInput>, ApplicationBuildFailure> {
    let adapter_reference = selected_adapter_reference(
        &loaded.bundle.manifest().target_adapters,
        adapter.target_profile(),
    )?;
    with_result_projection_input(
        vec![
            semantic_input(
                "01-lawpack-adapter",
                ProviderSemanticInputKind::Auxiliary("lawpack-adapter".to_owned()),
                &adapter_reference.id,
                ADAPTER_DOMAIN,
                &loaded.adapter_bytes,
            )?,
            semantic_input(
                "02-executable-operation-package",
                ProviderSemanticInputKind::Auxiliary("executable-operation-package".to_owned()),
                PACKAGE_ROLE,
                PACKAGE_DOMAIN,
                package_bytes,
            )?,
            semantic_input(
                "03-lawpack-exports",
                ProviderSemanticInputKind::Auxiliary("lawpack-exports".to_owned()),
                &loaded.bundle.manifest().exports.id,
                EXPORTS_DOMAIN,
                &loaded.exports_bytes,
            )?,
            semantic_input(
                "04-lawpack",
                ProviderSemanticInputKind::Lawpack,
                &format!(
                    "{}@{}",
                    loaded.bundle.manifest().id,
                    loaded.bundle.manifest().version
                ),
                PROVIDER_LAWPACK_ARTIFACT_DOMAIN,
                &loaded.manifest_bytes,
            )?,
            semantic_input(
                "05-source",
                ProviderSemanticInputKind::Auxiliary(source.kind.to_owned()),
                coordinate,
                source.domain,
                &source.bytes,
            )?,
            semantic_input(
                "06-target-configuration",
                ProviderSemanticInputKind::Auxiliary("target-configuration".to_owned()),
                &configuration.id,
                &configuration.id,
                &loaded.configuration_bytes,
            )?,
        ],
        result_projection,
    )
}

fn single_configuration_for_core<'a>(
    operation_profiles: &'a BTreeMap<String, edict_syntax::LawpackAdapterOperationProfile>,
    effects: &'a BTreeMap<String, edict_syntax::LawpackAdapterEffect>,
    core: &edict_syntax::CoreModule,
) -> Result<&'a edict_syntax::LawpackResourceRef, ApplicationBuildFailure> {
    for (intent_name, intent) in &core.intents {
        let mut matching_profiles = operation_profiles
            .iter()
            .filter(|(_, profile)| profile.core == intent.required_operation_profile);
        let Some((profile_coordinate, profile)) = matching_profiles.next() else {
            return Err(failure(
                "InvalidLawpackAdapter",
                format!(
                    "compiled Core intent `{intent_name}` requires unknown operation profile `{}`",
                    intent.required_operation_profile
                ),
            ));
        };
        if matching_profiles.next().is_some() {
            return Err(failure(
                "InvalidLawpackAdapter",
                format!(
                    "compiled Core intent `{intent_name}` requires an ambiguous operation profile `{}`",
                    intent.required_operation_profile
                ),
            ));
        }
        let required_effects = semantic_effects_required_by_block(core, &intent.body);
        if let Some(unadvertised_effect) = required_effects
            .iter()
            .find(|effect| !profile.semantic_effects.contains(effect))
        {
            return Err(failure(
                "InvalidLawpackAdapter",
                format!(
                    "compiled Core intent `{intent_name}` invokes effect `{unadvertised_effect}` outside operation profile `{profile_coordinate}`"
                ),
            ));
        }
    }
    let required_core_profiles = core
        .intents
        .values()
        .map(|intent| intent.required_operation_profile.as_str())
        .collect::<BTreeSet<_>>();
    let required_semantic_effects = semantic_effects_required_by_core(core);
    single_configuration_for_required_core_profiles(
        operation_profiles,
        effects,
        &required_core_profiles,
        &required_semantic_effects,
    )
}

fn semantic_effects_required_by_core(core: &edict_syntax::CoreModule) -> BTreeSet<String> {
    core.intents
        .values()
        .flat_map(|intent| semantic_effects_required_by_block(core, &intent.body))
        .collect()
}

fn semantic_effects_required_by_block(
    core: &edict_syntax::CoreModule,
    root: &edict_syntax::CoreBlock,
) -> BTreeSet<String> {
    let mut source_effects = BTreeSet::new();
    let mut blocks = vec![root];
    while let Some(block) = blocks.pop() {
        for node in &block.nodes {
            match node {
                edict_syntax::CoreNode::Effect { effect, .. } => {
                    source_effects.insert(effect.as_str());
                }
                edict_syntax::CoreNode::For { body, .. } => blocks.push(body),
                edict_syntax::CoreNode::Branch {
                    then_block,
                    else_block,
                    ..
                } => {
                    blocks.push(else_block);
                    blocks.push(then_block);
                }
                edict_syntax::CoreNode::Let { .. }
                | edict_syntax::CoreNode::Require { .. }
                | edict_syntax::CoreNode::ExternalActionRequest { .. } => {}
            }
        }
    }
    source_effects
        .into_iter()
        .map(|effect| {
            let Some((alias, suffix)) = effect.split_once('.') else {
                return effect.to_owned();
            };
            core.imports
                .iter()
                .find(|import| {
                    import.kind == edict_syntax::CoreImportKind::Lawpack
                        && import.alias.as_deref() == Some(alias)
                })
                .map_or_else(
                    || effect.to_owned(),
                    |import| format!("{}.{}", import.resource.coordinate, suffix),
                )
        })
        .collect()
}

fn single_configuration_for_required_core_profiles<'a>(
    operation_profiles: &'a BTreeMap<String, edict_syntax::LawpackAdapterOperationProfile>,
    effects: &'a BTreeMap<String, edict_syntax::LawpackAdapterEffect>,
    required_core_profiles: &BTreeSet<&str>,
    required_semantic_effects: &BTreeSet<String>,
) -> Result<&'a edict_syntax::LawpackResourceRef, ApplicationBuildFailure> {
    let mut configurations = Vec::new();
    for required_core_profile in required_core_profiles {
        let mut matched = false;
        for (coordinate, profile) in operation_profiles
            .iter()
            .filter(|(_, profile)| profile.core == *required_core_profile)
        {
            matched = true;
            if profile.semantic_effects.is_empty() {
                let configuration = profile.target_configuration.as_ref().ok_or_else(|| {
                    failure(
                        "InvalidLawpackAdapter",
                        format!("operation profile `{coordinate}` has no target configuration"),
                    )
                })?;
                configurations.push(configuration);
            } else {
                let invoked_effects = profile
                    .semantic_effects
                    .iter()
                    .filter(|effect| required_semantic_effects.contains(effect.as_str()))
                    .collect::<Vec<_>>();
                let configuration_effects = if invoked_effects.is_empty() {
                    profile.semantic_effects.iter().collect::<Vec<_>>()
                } else {
                    invoked_effects
                };
                for effect_coordinate in configuration_effects {
                    let effect = effects.get(effect_coordinate).ok_or_else(|| {
                        failure(
                            "InvalidLawpackAdapter",
                            format!(
                                "operation profile `{coordinate}` selects unknown effect `{effect_coordinate}`"
                            ),
                        )
                    })?;
                    configurations.push(&effect.target_configuration);
                }
            }
        }
        if !matched {
            return Err(failure(
                "InvalidLawpackAdapter",
                format!(
                    "compiled Core requires unknown operation profile `{required_core_profile}`"
                ),
            ));
        }
    }
    single_unique_configuration(configurations)
}

fn single_result_projection<'a>(
    projections: &'a BTreeMap<String, ResultProjectionArtifact>,
    failures: &BTreeMap<String, edict_syntax::ResultProjectionFailure>,
) -> Result<(&'a str, &'a ResultProjectionArtifact), ApplicationBuildFailure> {
    if !failures.is_empty() {
        return Err(failure(
            "ResultProjectionUnavailable",
            format!("application result projection failed closed: {failures:?}"),
        ));
    }
    let mut projections = projections.iter();
    match (projections.next(), projections.next()) {
        (Some((intent, projection)), None) => Ok((intent.as_str(), projection)),
        _ => Err(failure(
            "ResultProjectionUnavailable",
            "the executable-operation build requires exactly one compiler result projection",
        )),
    }
}

fn single_unique_configuration<'a>(
    references: impl IntoIterator<Item = &'a edict_syntax::LawpackResourceRef>,
) -> Result<&'a edict_syntax::LawpackResourceRef, ApplicationBuildFailure> {
    let references = references.into_iter().collect::<BTreeSet<_>>();
    let mut references = references.into_iter();
    match (references.next(), references.next()) {
        (Some(reference), None) => Ok(reference),
        _ => Err(failure(
            "InvalidLawpackAdapter",
            "the application adapter requires exactly one target configuration",
        )),
    }
}

fn selected_adapter_reference<'a>(
    adapters: &'a [edict_syntax::LawpackTargetAdapter],
    selected_target_profile: &edict_syntax::LawpackResourceRef,
) -> Result<&'a edict_syntax::LawpackResourceRef, ApplicationBuildFailure> {
    let mut matches = adapters
        .iter()
        .filter(|descriptor| descriptor.accepted_target_profile == *selected_target_profile);
    match (matches.next(), matches.next()) {
        (Some(descriptor), None) => Ok(&descriptor.adapter),
        _ => Err(failure(
            "InvalidLawpackAdapter",
            "the selected target profile must identify exactly one lawpack adapter",
        )),
    }
}

fn semantic_input(
    role: &str,
    kind: ProviderSemanticInputKind,
    coordinate: &str,
    domain: &str,
    bytes: &[u8],
) -> Result<ProviderSemanticInput, ApplicationBuildFailure> {
    Ok(ProviderSemanticInput {
        role: role.to_owned(),
        kind,
        artifact: bound_artifact(coordinate, domain, bytes)?,
    })
}

fn with_result_projection_input(
    mut inputs: Vec<ProviderSemanticInput>,
    projection: &ResultProjectionArtifact,
) -> Result<Vec<ProviderSemanticInput>, ApplicationBuildFailure> {
    let input = semantic_input(
        RESULT_PROJECTION_ROLE,
        ProviderSemanticInputKind::Auxiliary("result-projection".to_owned()),
        &projection.projection().operation_coordinate,
        RESULT_PROJECTION_DIGEST_DOMAIN,
        projection.canonical_bytes(),
    )?;
    if input.artifact.reference.digest.bytes != projection.digest().bytes() {
        return Err(failure(
            "ResultProjectionDigestMismatch",
            "provider input does not preserve the compiler result projection identity",
        ));
    }
    inputs.push(input);
    Ok(inputs)
}

fn lowering_contract(
    core: &ProviderBoundArtifact,
    target_profile: &ProviderBoundArtifact,
    inputs: &[ProviderSemanticInput],
) -> ProviderLoweringInvocationContract {
    ProviderLoweringInvocationContract {
        core: artifact_binding(core),
        target_profile: artifact_binding(target_profile),
        semantic_inputs: input_bindings(inputs),
    }
}

fn verification_contract(
    core: &ProviderBoundArtifact,
    target_profile: &ProviderBoundArtifact,
    target_ir: &ProviderBoundArtifact,
    inputs: &[ProviderSemanticInput],
) -> ProviderVerificationInvocationContract {
    ProviderVerificationInvocationContract {
        core: artifact_binding(core),
        target_profile: artifact_binding(target_profile),
        target_ir: artifact_binding(target_ir),
        semantic_inputs: input_bindings(inputs),
    }
}

fn input_bindings(inputs: &[ProviderSemanticInput]) -> Vec<ProviderSemanticInputBinding> {
    inputs
        .iter()
        .map(|input| ProviderSemanticInputBinding {
            role: input.role.clone(),
            kind: input.kind.clone(),
            artifact: artifact_binding(&input.artifact),
        })
        .collect()
}

fn artifact_binding(bound: &ProviderBoundArtifact) -> ProviderArtifactBinding {
    ProviderArtifactBinding {
        reference: bound.reference.clone(),
        domain: bound.artifact.domain.clone(),
    }
}

fn bound_artifact(
    coordinate: &str,
    domain: &str,
    bytes: &[u8],
) -> Result<ProviderBoundArtifact, ApplicationBuildFailure> {
    Ok(ProviderBoundArtifact {
        reference: ProviderResourceRef {
            coordinate: coordinate.to_owned(),
            digest: provider_digest(domain, bytes)?,
        },
        artifact: ProviderArtifact {
            domain: domain.to_owned(),
            bytes: bytes.to_vec(),
        },
    })
}

fn provider_digest(domain: &str, bytes: &[u8]) -> Result<ProviderDigest, ApplicationBuildFailure> {
    let digest = digest_canonical_artifact(domain, bytes).map_err(|error| {
        failure(
            "NonCanonicalApplicationArtifact",
            format!("artifact under `{domain}` is not canonical CBOR: {error}"),
        )
    })?;
    Ok(ProviderDigest {
        algorithm: ProviderDigestAlgorithm::Sha256,
        bytes: digest.bytes().to_vec(),
    })
}

fn require_manifest_identity(
    manifest_artifact: &ProviderArtifactRef,
    bound: &ProviderBoundArtifact,
) -> Result<(), ApplicationBuildFailure> {
    if manifest_artifact.resource.digest.as_deref()
        != Some(&rendered_digest(&bound.reference.digest))
    {
        return Err(failure(
            "ProviderArtifactDigestMismatch",
            format!(
                "provider artifact `{}` bytes do not reproduce the manifest digest",
                manifest_artifact.role
            ),
        ));
    }
    Ok(())
}

fn rendered_digest(digest: &ProviderDigest) -> String {
    let mut value = String::from("sha256:");
    for byte in &digest.bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

fn require_accepted_report(bytes: &[u8]) -> Result<(), ApplicationBuildFailure> {
    let value = decode_canonical_cbor(bytes).map_err(|error| {
        failure(
            "ProviderVerificationRejected",
            format!("verification report is not canonical CBOR: {error}"),
        )
    })?;
    let CanonicalValue::Map(entries) = value else {
        return Err(failure(
            "ProviderVerificationRejected",
            "verification report is not a canonical map",
        ));
    };
    let mut outcomes = entries
        .iter()
        .filter(|(key, _)| matches!(key, CanonicalValue::Text(key) if key == "outcome"));
    let accepted = matches!(
        (outcomes.next(), outcomes.next()),
        (Some((_, CanonicalValue::Text(outcome))), None) if outcome == "accepted"
    );
    if !accepted {
        return Err(failure(
            "ProviderVerificationRejected",
            "independent verifier report did not accept the emitted package",
        ));
    }
    Ok(())
}

fn write_outputs(
    directory: &Path,
    package: &[u8],
    report: &[u8],
) -> Result<(), ApplicationBuildFailure> {
    write_application_output_pair(
        directory,
        &[
            ApplicationOutput {
                file_name: "executable-operation-package.cbor",
                transaction_name: "new-package.cbor",
                backup_name: "previous-package.cbor",
                bytes: package,
            },
            ApplicationOutput {
                file_name: "verification-report.cbor",
                transaction_name: "new-report.cbor",
                backup_name: "previous-report.cbor",
                bytes: report,
            },
        ],
        &[
            ObsoleteApplicationOutput {
                file_name: "core.cbor",
                backup_name: "previous-core.cbor",
            },
            ObsoleteApplicationOutput {
                file_name: "target-ir.cbor",
                backup_name: "previous-target-ir.cbor",
            },
        ],
    )
}

fn write_external_action_outputs(
    directory: &Path,
    core: &[u8],
    target_ir: &[u8],
) -> Result<(), ApplicationBuildFailure> {
    write_application_output_pair(
        directory,
        &[
            ApplicationOutput {
                file_name: "core.cbor",
                transaction_name: "new-core.cbor",
                backup_name: "previous-core.cbor",
                bytes: core,
            },
            ApplicationOutput {
                file_name: "target-ir.cbor",
                transaction_name: "new-target-ir.cbor",
                backup_name: "previous-target-ir.cbor",
                bytes: target_ir,
            },
        ],
        &[
            ObsoleteApplicationOutput {
                file_name: "executable-operation-package.cbor",
                backup_name: "previous-package.cbor",
            },
            ObsoleteApplicationOutput {
                file_name: "verification-report.cbor",
                backup_name: "previous-report.cbor",
            },
        ],
    )
}

struct ApplicationOutput<'a> {
    file_name: &'static str,
    transaction_name: &'static str,
    backup_name: &'static str,
    bytes: &'a [u8],
}

struct ObsoleteApplicationOutput {
    file_name: &'static str,
    backup_name: &'static str,
}

#[allow(
    clippy::too_many_lines,
    reason = "paired output publication keeps every rollback transition explicit"
)]
fn write_application_output_pair(
    directory: &Path,
    outputs: &[ApplicationOutput<'_>; 2],
    obsolete: &[ObsoleteApplicationOutput; 2],
) -> Result<(), ApplicationBuildFailure> {
    fs::create_dir_all(directory).map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "failed to create application output directory `{}`: {error}",
                directory.display()
            ),
        )
    })?;

    let lock_path = output_lock_path(directory);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| {
            failure(
                "ApplicationOutputWriteFailed",
                format!(
                    "failed to open application output lock `{}`: {error}",
                    lock_path.display()
                ),
            )
        })?;
    lock.try_lock().map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "another application build owns output directory `{}`: {error}",
                directory.display()
            ),
        )
    })?;

    let mut destinations = Vec::with_capacity(4);
    for output in outputs {
        let destination = directory.join(output.file_name);
        let existed = validate_output_target(&destination)?;
        destinations.push((destination, output.backup_name, existed));
    }
    for output in obsolete {
        let destination = directory.join(output.file_name);
        let existed = validate_output_target(&destination)?;
        destinations.push((destination, output.backup_name, existed));
    }

    let transaction = create_output_transaction(directory)?;
    let mut recoveries = destinations
        .into_iter()
        .map(|(destination, backup_name, existed)| OutputRecovery {
            destination,
            backup: transaction.join(backup_name),
            existed,
            published: false,
        })
        .collect::<Vec<_>>();

    let staged = outputs
        .iter()
        .map(|output| transaction.join(output.transaction_name))
        .collect::<Vec<_>>();
    for (output, staged_path) in outputs.iter().zip(&staged) {
        if let Err(error) = write_synced(staged_path, output.bytes) {
            let _ = fs::remove_dir_all(&transaction);
            return Err(failure(
                "ApplicationOutputWriteFailed",
                format!(
                    "failed to stage `{}`: {error}",
                    directory.join(output.file_name).display()
                ),
            ));
        }
    }

    for index in 0..recoveries.len() {
        if !recoveries[index].existed {
            continue;
        }
        if let Err(error) = fs::rename(&recoveries[index].destination, &recoveries[index].backup) {
            let rollback = restore_previous_outputs(&recoveries[..index]);
            if rollback.is_ok() {
                let _ = fs::remove_dir_all(&transaction);
            }
            return Err(output_publication_failure(
                &recoveries[index].destination,
                &error,
                rollback.err(),
            ));
        }
    }

    for index in [1_usize, 0] {
        if let Err(error) = fs::rename(&staged[index], &recoveries[index].destination) {
            let rollback = restore_previous_outputs(&recoveries);
            if rollback.is_ok() {
                let _ = fs::remove_dir_all(&transaction);
            }
            return Err(output_publication_failure(
                &recoveries[index].destination,
                &error,
                rollback.err(),
            ));
        }
        recoveries[index].published = true;
    }

    fs::remove_dir_all(&transaction).map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "published application outputs but failed to remove transaction `{}`: {error}",
                transaction.display()
            ),
        )
    })
}

fn output_lock_path(directory: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(
        directory
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("root")),
    );
    name.push(".edict-application-build.lock");
    directory
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name)
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn validate_output_target(path: &Path) -> Result<bool, ApplicationBuildFailure> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(failure(
            "ApplicationOutputWriteFailed",
            format!(
                "application output target `{}` must be absent or a regular file",
                path.display()
            ),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(failure(
            "ApplicationOutputWriteFailed",
            format!(
                "failed to inspect application output target `{}`: {error}",
                path.display()
            ),
        )),
    }
}

fn create_output_transaction(directory: &Path) -> Result<PathBuf, ApplicationBuildFailure> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            failure(
                "ApplicationOutputWriteFailed",
                format!("system clock cannot name an output transaction: {error}"),
            )
        })?
        .as_nanos();
    for attempt in 0..16 {
        let path = directory.join(format!(
            ".edict-application-build-{}-{timestamp}-{attempt}.transaction",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(failure(
                    "ApplicationOutputWriteFailed",
                    format!(
                        "failed to create output transaction `{}`: {error}",
                        path.display()
                    ),
                ));
            }
        }
    }
    Err(failure(
        "ApplicationOutputWriteFailed",
        format!(
            "failed to allocate a unique output transaction in `{}`",
            directory.display()
        ),
    ))
}

struct OutputRecovery {
    destination: PathBuf,
    backup: PathBuf,
    existed: bool,
    published: bool,
}

fn restore_previous_outputs(outputs: &[OutputRecovery]) -> Result<(), String> {
    let mut failures = Vec::new();
    for output in outputs.iter().filter(|output| output.published) {
        if let Err(error) = fs::remove_file(&output.destination) {
            failures.push(format!(
                "failed to remove partial output `{}`: {error}",
                output.destination.display()
            ));
        }
    }
    for output in outputs {
        if let Err(error) = restore_output(&output.backup, &output.destination, output.existed) {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn restore_output(backup: &Path, destination: &Path, existed: bool) -> Result<(), String> {
    if !existed {
        return Ok(());
    }
    fs::rename(backup, destination).map_err(|error| {
        format!(
            "failed to restore previous output `{}`: {error}",
            destination.display()
        )
    })
}

fn output_publication_failure(
    path: &Path,
    error: &std::io::Error,
    rollback: Option<String>,
) -> ApplicationBuildFailure {
    let rollback = rollback.map_or(String::new(), |message| {
        format!("; rollback was incomplete: {message}")
    });
    failure(
        "ApplicationOutputWriteFailed",
        format!("failed to publish `{}`: {error}{rollback}", path.display()),
    )
}

const fn response_limits() -> ProviderResponseLimits {
    ProviderResponseLimits {
        max_output_count: 1,
        max_diagnostic_count: 8,
        max_total_response_bytes: 64 * 1024,
    }
}

const fn host_limits() -> ProviderHostLimits {
    ProviderHostLimits {
        max_input_bytes: 1024 * 1024,
        max_output_bytes: 3 * 1024 * 1024,
        max_diagnostic_bytes: 3 * 1024 * 1024,
        max_wasm_memory_bytes: 16 * 1024 * 1024,
        max_table_elements: 10_000,
        max_instances: 100,
        max_memories: 8,
        max_tables: 8,
        max_wasm_fuel: 50_000_000,
        max_hostcall_bytes: 4 * 1024 * 1024,
        max_host_diagnostic_bytes: 512,
    }
}

fn read(
    path: &Path,
    kind: &'static str,
    subject: &str,
) -> Result<Vec<u8>, ApplicationBuildFailure> {
    let mut file = OpenOptions::new().read(true).open(path).map_err(|error| {
        failure(
            kind,
            format!("failed to read {subject} `{}`: {error}", path.display()),
        )
    })?;
    let length = file
        .metadata()
        .map_err(|error| {
            failure(
                kind,
                format!("failed to inspect {subject} `{}`: {error}", path.display()),
            )
        })?
        .len();
    if length > MAX_APPLICATION_ARTIFACT_BYTES {
        return Err(artifact_too_large(path, subject));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_APPLICATION_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            failure(
                kind,
                format!("failed to read {subject} `{}`: {error}", path.display()),
            )
        })?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_APPLICATION_ARTIFACT_BYTES) {
        return Err(artifact_too_large(path, subject));
    }
    Ok(bytes)
}

fn artifact_too_large(path: &Path, subject: &str) -> ApplicationBuildFailure {
    failure(
        "ApplicationArtifactTooLarge",
        format!(
            "{subject} `{}` exceeds the {} byte application-artifact limit",
            path.display(),
            MAX_APPLICATION_ARTIFACT_BYTES
        ),
    )
}

fn canonical_application_root(config_path: &Path) -> Result<PathBuf, ApplicationBuildFailure> {
    let root = config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::canonicalize(root).map_err(|error| {
        failure(
            "ApplicationConfigReadFailed",
            format!(
                "failed to resolve application root `{}`: {error}",
                root.display()
            ),
        )
    })
}

fn confined_existing_path(
    root: &Path,
    relative: &Path,
    field: &str,
    read_failure_kind: &'static str,
    subject: &str,
) -> Result<PathBuf, ApplicationBuildFailure> {
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        failure(
            read_failure_kind,
            format!(
                "failed to resolve {subject} `{}`: {error}",
                candidate.display()
            ),
        )
    })?;
    require_path_beneath_root(root, &canonical, field)?;
    Ok(canonical)
}

fn prepare_output_directory(
    root: &Path,
    relative: &Path,
) -> Result<PathBuf, ApplicationBuildFailure> {
    let candidate = root.join(relative);
    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor.parent().ok_or_else(|| {
            failure(
                "ApplicationOutputWriteFailed",
                format!(
                    "application output `{}` has no existing ancestor",
                    candidate.display()
                ),
            )
        })?;
    }
    let canonical_ancestor = fs::canonicalize(existing_ancestor).map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "failed to resolve output ancestor `{}`: {error}",
                existing_ancestor.display()
            ),
        )
    })?;
    require_path_beneath_root(root, &canonical_ancestor, "outputDirectory")?;
    fs::create_dir_all(&candidate).map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "failed to create application output directory `{}`: {error}",
                candidate.display()
            ),
        )
    })?;
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        failure(
            "ApplicationOutputWriteFailed",
            format!(
                "failed to resolve application output directory `{}`: {error}",
                candidate.display()
            ),
        )
    })?;
    require_path_beneath_root(root, &canonical, "outputDirectory")?;
    Ok(canonical)
}

fn require_path_beneath_root(
    root: &Path,
    canonical: &Path,
    field: &str,
) -> Result<(), ApplicationBuildFailure> {
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err(failure(
            "ApplicationPathEscape",
            format!(
                "application field `{field}` resolves outside manifest root `{}`",
                root.display()
            ),
        ))
    }
}

fn failure(kind: &'static str, message: impl Into<String>) -> ApplicationBuildFailure {
    ApplicationBuildFailure {
        kind,
        message: message.into(),
        source_diagnostics: None,
    }
}

fn source_failure(
    kind: &'static str,
    message: &'static str,
    path: &Path,
    source: &str,
    errors: Vec<ApplicationSourceError>,
) -> ApplicationBuildFailure {
    ApplicationBuildFailure {
        kind,
        message: message.to_owned(),
        source_diagnostics: Some(vec![ApplicationSourceDiagnostics {
            path: path.to_path_buf(),
            source: source.to_owned(),
            errors,
        }]),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use edict_syntax::{
        author_lawpack, compile_to_core, decode_canonical_cbor, decode_lawpack_adapter,
        decode_lawpack_bundle, decode_result_projection, digest_canonical_artifact,
        encode_canonical_cbor, lower_to_target_ir, parse_module, prepare_lawpack_compilation,
        CanonicalValue, LawpackAdapterOperationProfile, LawpackArtifactKind,
        LawpackAuthoringDefinition, LawpackResourceRef, LawpackTargetAdapter, ProviderArtifactKind,
        ProviderArtifactRef, ProviderArtifactSource, ProviderSchemaBinding, ProviderSchemaFormat,
        ResourceRef, ResultProjectionArtifact, TargetIrArtifact, TargetLoweringReport,
        TargetProviderManifest, RESULT_PROJECTION_DIGEST_DOMAIN, TARGET_PROVIDER_ABI,
        TARGET_PROVIDER_MANIFEST_API_VERSION,
    };

    use super::{
        build_application, canonical_application_root, lowering_inputs, output_lock_path,
        provider_schema_artifacts, read, selected_adapter_reference,
        semantic_effects_required_by_core, single_configuration_for_core,
        single_configuration_for_required_core_profiles, single_result_projection,
        single_unique_configuration, validate_application_manifest,
        validate_external_action_artifacts, with_result_projection_input,
        write_external_action_outputs, write_outputs, ApplicationBuildKind,
        ApplicationExternalActionResource, ApplicationLawpack, ApplicationManifest,
        ApplicationSourceError, ApplicationTarget, ExternalActionResourceKind,
        LoadedExternalActionResource, LoadedLawpack, LoadedSources,
        EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, MAX_EXTERNAL_ACTION_RESOURCES,
        RESULT_PROJECTION_ROLE,
    };

    const STRESS_SEED: u64 = 0x5eed_1a77_c105_0a11;
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn application_manifest_accepts_one_root_and_its_complete_dependency_closure() {
        let config = application_manifest(64);

        test_ok(
            validate_application_manifest(&config),
            "one root plus a bounded dependency closure must be accepted",
        );
    }

    #[test]
    fn application_manifest_accepts_an_explicit_external_action_build_kind() {
        let manifest = serde_json::json!({
            "schema": "edict.application/v1",
            "buildKind": "externalAction",
            "coordinate": "examples.workspace_observer@1",
            "sources": ["src/workspace_observer.edict"],
            "lawpacks": [{
                "manifest": "vendor/workspace/manifest.cbor",
                "exports": "vendor/workspace/exports.cbor",
                "adapter": "vendor/workspace/adapter.cbor",
                "targetConfiguration": "vendor/workspace/target-configuration.cbor"
            }],
            "target": {
                "profile": "echo.dpo@1",
                "providerPackage": ".build/echo-provider"
            },
            "outputDirectory": ".build/application"
        });

        let decoded = test_ok(
            serde_json::from_value::<ApplicationManifest>(manifest),
            "decode external-action application manifest",
        );

        assert_eq!(decoded.build_kind, ApplicationBuildKind::ExternalAction);
    }

    #[test]
    fn external_action_build_accepts_request_only_target_ir() {
        let closure = [external_action_loaded_lawpack()];
        let resources = external_action_loaded_resources();
        test_ok(
            validate_external_action_artifacts(&external_action_target_ir(), &closure, &resources),
            "request-only Target IR is publishable",
        );
    }

    #[test]
    fn external_action_build_binds_operation_authority_by_manifest_digest() {
        let closure = [external_action_loaded_lawpack()];
        let resources = external_action_loaded_resources();
        let mut external = external_action_target_ir();
        let Some(observe) = external.intents.get_mut("observe") else {
            panic!("workspace observer intent exists");
        };
        observe.external_action_requests[0].operation.coordinate =
            "workspace.snapshot.observe@2".to_owned();

        test_ok(
            validate_external_action_artifacts(&external, &closure, &resources),
            "operation identity is independent of its authority manifest version",
        );
    }

    #[test]
    fn external_action_build_requires_a_typed_request() {
        let failure = test_err(
            validate_external_action_artifacts(&hello_echo_target_ir(), &[], &[]),
            "callable-only Target IR must not publish as external-action artifacts",
        );

        assert_eq!(failure.kind, "ExternalActionRequestUnavailable");
    }

    #[test]
    fn external_action_build_rejects_mixed_callable_execution() {
        let closure = [external_action_loaded_lawpack()];
        let resources = external_action_loaded_resources();
        let mut external = external_action_target_ir();
        let Some(callable) = hello_echo_target_ir()
            .intents
            .get("createGreeting")
            .and_then(|intent| intent.steps.first())
            .cloned()
        else {
            panic!("Hello Echo Target IR has one callable step");
        };
        let Some(observe) = external.intents.get_mut("observe") else {
            panic!("workspace observer intent exists");
        };
        observe.steps.push(callable);

        let failure = test_err(
            validate_external_action_artifacts(&external, &closure, &resources),
            "mixed callable/request execution must reject",
        );

        assert_eq!(failure.kind, "MixedApplicationExecution");
    }

    #[test]
    fn external_action_build_rejects_a_substituted_capability_manifest() {
        let closure = [external_action_loaded_lawpack()];
        let resources = external_action_loaded_resources();
        let mut external = external_action_target_ir();
        let Some(observe) = external.intents.get_mut("observe") else {
            panic!("workspace observer intent exists");
        };
        let operation = &mut observe.external_action_requests[0].operation;
        operation.digest = Some(format!("sha256:{}", "0".repeat(64)));

        let failure = test_err(
            validate_external_action_artifacts(&external, &closure, &resources),
            "substituted capability manifest must reject",
        );

        assert_eq!(failure.kind, "ExternalActionCapabilityClosureMismatch");
    }

    #[test]
    fn public_external_action_build_emits_exact_compiler_artifacts() {
        let root = temp_tree("public-external-action");
        let config_path = write_external_action_application(&root);
        let output = root.join(".build/application");
        test_ok(fs::create_dir_all(&output), "create stale output directory");
        test_ok(
            fs::write(
                output.join("executable-operation-package.cbor"),
                b"stale-package",
            ),
            "write stale package",
        );
        test_ok(
            fs::write(output.join("verification-report.cbor"), b"stale-report"),
            "write stale report",
        );

        test_ok(
            build_application(&config_path),
            "build public external-action application",
        );
        let first_core = test_ok(fs::read(output.join("core.cbor")), "read first Core");
        let first_target = test_ok(
            fs::read(output.join("target-ir.cbor")),
            "read first Target IR",
        );

        assert_eq!(
            first_core,
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/observe-workspace.core.cbor"
            )
        );
        assert_eq!(
            first_target,
            include_bytes!(
                "../../../fixtures/lawpack/workspace-snapshot/observe-workspace.target-ir.cbor"
            )
        );
        assert!(!output.join("executable-operation-package.cbor").exists());
        assert!(!output.join("verification-report.cbor").exists());

        test_ok(
            build_application(&config_path),
            "rerun public external-action application build",
        );
        assert_eq!(
            test_ok(fs::read(output.join("core.cbor")), "read rerun Core"),
            first_core
        );
        assert_eq!(
            test_ok(
                fs::read(output.join("target-ir.cbor")),
                "read rerun Target IR",
            ),
            first_target
        );
        test_ok(fs::remove_dir_all(root), "remove public build tree");
    }

    fn write_source_module_fixture(root: &std::path::Path) -> (String, String, Vec<PathBuf>) {
        let library = "package library.values@1; type Value = U64;".to_owned();
        let pin = test_ok(
            edict_syntax::source_closure::source_module_digest(&library),
            "source module digest",
        );
        let source = format!("package application.root@1; use module library.values@1 digest \"{pin}\" as values; type Value = values.Value;");
        test_ok(fs::create_dir_all(root.join("src")), "source directory");
        test_ok(
            fs::write(root.join("src/root.edict"), &source),
            "root source",
        );
        test_ok(
            fs::write(root.join("src/values.edict"), &library),
            "dependency source",
        );
        (
            source,
            library,
            vec!["src/root.edict".into(), "src/values.edict".into()],
        )
    }

    #[test]
    fn application_sources_preserve_exact_legacy_and_authenticated_artifacts() {
        let root = test_ok(
            fs::canonicalize(temp_tree("source-artifacts")),
            "canonical root",
        );
        let (source, library, paths) = write_source_module_fixture(&root);
        let loaded = test_ok(
            LoadedSources::load(&root, &paths),
            "load explicit source closure",
        );
        let artifact = test_ok(loaded.artifact(), "module source artifact");
        assert_eq!(artifact.kind, "edict-source-closure");
        assert_eq!(artifact.domain, "edict.source-closure/v1");
        let closure = test_ok(
            edict_syntax::source_closure::resolve_source_closure(
                &source,
                &[&library],
                edict_syntax::source_closure::SourceClosureLimits::default(),
            ),
            "independent closure witness",
        );
        assert_eq!(artifact.bytes, closure.canonical_bytes());
        let legacy = "package application.root@1; // exact UTF-8 🦀\r\n type Value = U64;";
        test_ok(fs::write(root.join(&paths[0]), legacy), "legacy source");
        let loaded = test_ok(
            LoadedSources::load(&root, &paths[..1]),
            "load legacy source",
        );
        let artifact = test_ok(loaded.artifact(), "legacy artifact");
        assert_eq!(artifact.kind, "edict-source");
        assert_eq!(artifact.domain, "edict.source/v1");
        assert_eq!(
            test_ok(
                decode_canonical_cbor(&artifact.bytes),
                "decode legacy bytes"
            ),
            CanonicalValue::Bytes(legacy.as_bytes().to_vec())
        );
        test_ok(fs::remove_dir_all(root), "remove source artifact tree");
    }

    #[test]
    fn application_sources_refuse_missing_substituted_disconnected_and_duplicate_units() {
        for case in [
            "missing",
            "substituted",
            "disconnected",
            "duplicate-path",
            "duplicate-coordinate",
        ] {
            let root = test_ok(fs::canonicalize(temp_tree(case)), "canonical root");
            let (_, library, mut paths) = write_source_module_fixture(&root);
            let expected = match case {
                "missing" => {
                    paths.pop();
                    "InvalidApplicationSourceClosure"
                }
                "substituted" => {
                    test_ok(
                        fs::write(root.join(&paths[1]), format!("{library} // substituted")),
                        "substituted source",
                    );
                    "InvalidApplicationSourceClosure"
                }
                "disconnected" => {
                    test_ok(
                        fs::write(
                            root.join("src/unused.edict"),
                            "package library.unused@1; type Value = U64;",
                        ),
                        "disconnected source",
                    );
                    paths.push("src/unused.edict".into());
                    "InvalidApplicationSourceClosure"
                }
                "duplicate-path" => {
                    paths.push(paths[1].clone());
                    "DuplicateApplicationSource"
                }
                "duplicate-coordinate" => {
                    test_ok(
                        fs::write(root.join("src/alias.edict"), library),
                        "duplicate package",
                    );
                    paths.push("src/alias.edict".into());
                    "InvalidApplicationSourceClosure"
                }
                _ => unreachable!("declared case"),
            };
            let Err(error) = LoadedSources::load(&root, &paths) else {
                panic!("must reject {case}")
            };
            assert_eq!(error.kind, expected, "{case}");
            assert!(!root.join(".build").exists());
            test_ok(fs::remove_dir_all(root), "remove negative source tree");
        }
    }

    #[test]
    fn application_sources_enforce_aggregate_byte_budget_before_parse() {
        let root = test_ok(
            fs::canonicalize(temp_tree("source-aggregate-budget")),
            "canonical root",
        );
        let (_, _, paths) = write_source_module_fixture(&root);
        let limit = edict_syntax::source_closure::SourceClosureLimits::default().max_source_bytes;
        // Each source fits the per-artifact read bound; their sum does not.
        // Invalid syntax would produce a parse failure if parsing ran first.
        test_ok(
            fs::write(root.join(&paths[0]), vec![b'?'; limit / 2 + 1]),
            "large root",
        );
        test_ok(
            fs::write(root.join(&paths[1]), vec![b'?'; limit / 2 + 1]),
            "large dependency",
        );
        let Err(error) = LoadedSources::load(&root, &paths) else {
            panic!("must reject aggregate source budget")
        };
        assert_eq!(error.kind, "ApplicationSourceBudgetExceeded");
        test_ok(fs::remove_dir_all(root), "remove bounded source tree");
    }

    #[cfg(unix)]
    #[test]
    fn application_sources_refuse_dependency_symlink_escape() {
        let root = test_ok(
            fs::canonicalize(temp_tree("source-confined")),
            "canonical root",
        );
        let outside = temp_tree("source-outside");
        let (_, library, paths) = write_source_module_fixture(&root);
        let external_path = outside.join("values.edict");
        test_ok(fs::write(&external_path, library), "outside source");
        test_ok(
            fs::remove_file(root.join(&paths[1])),
            "remove owned dependency",
        );
        test_ok(
            std::os::unix::fs::symlink(&external_path, root.join(&paths[1])),
            "escaping source symlink",
        );
        let Err(error) = LoadedSources::load(&root, &paths) else {
            panic!("must refuse escaped dependency")
        };
        assert_eq!(error.kind, "ApplicationPathEscape");
        assert!(!root.join(".build").exists());
        test_ok(fs::remove_dir_all(root), "remove confined tree");
        test_ok(fs::remove_dir_all(outside), "remove outside fixture");
    }

    #[test]
    fn public_application_build_reports_library_compiler_source() {
        let root = temp_tree("public-source-module-diagnostics");
        let config = write_external_action_application(&root);
        let library = "package library.payloads@1;\nfn invalid() -> U64 { return true; }";
        let pin = test_ok(
            edict_syntax::source_closure::source_module_digest(library),
            "digest library",
        );
        let source_path = root.join("src/observe-workspace.edict");
        let original = test_ok(fs::read_to_string(&source_path), "read root source");
        let source = original.replacen("package examples.workspace_observer@1;", &format!("package examples.workspace_observer@1; use module library.payloads@1 digest \"{pin}\" as payloads;"), 1);
        test_ok(fs::write(&source_path, source), "write importing source");
        let library_path = root.join("src/payloads.edict");
        test_ok(
            fs::write(&library_path, library),
            "write explicit dependency",
        );
        let mut manifest: serde_json::Value = test_ok(
            serde_json::from_slice(&test_ok(fs::read(&config), "read manifest")),
            "decode manifest",
        );
        manifest["sources"] =
            serde_json::json!(["src/observe-workspace.edict", "src/payloads.edict"]);
        test_ok(
            fs::write(
                &config,
                test_ok(serde_json::to_vec(&manifest), "encode manifest"),
            ),
            "write module manifest",
        );
        let Err(failure) = build_application(&config) else {
            panic!("invalid library must fail")
        };
        assert_eq!(failure.kind, "ApplicationCompilationFailed");
        assert!(
            failure.source_diagnostics.is_some(),
            "library failures must retain original-file diagnostics"
        );
        let groups = failure
            .source_diagnostics
            .as_ref()
            .unwrap_or_else(|| panic!("source groups"));
        let group = groups
            .iter()
            .find(|group| {
                group.path == test_ok(fs::canonicalize(&library_path), "canonical library")
            })
            .unwrap_or_else(|| panic!("original library path"));
        assert_eq!(group.source, library);
        assert!(group.errors.iter().any(|error| matches!(error, ApplicationSourceError::Compiler(error) if error.kind == edict_syntax::CompilerErrorKind::TypeMismatch && error.span.start == library.find("return true").unwrap_or_else(|| panic!("return anchor")))));
        assert!(!root.join(".build/application").exists());
        test_ok(fs::remove_dir_all(root), "remove module diagnostic tree");
    }

    #[test]
    fn public_external_action_build_rejects_sentinel_resource_identities() {
        let root = temp_tree("public-external-action-sentinel-resources");
        let config_path = write_external_action_application(&root);
        let source_path = root.join("src/observe-workspace.edict");
        let mut source = test_ok(
            fs::read_to_string(&source_path),
            "read resource-bound source",
        );
        for (fixture_digest, sentinel) in [
            (
                include_str!("../../../fixtures/lawpack/workspace-snapshot/input-schema.sha256")
                    .trim(),
                '9',
            ),
            (
                include_str!(
                    "../../../fixtures/lawpack/workspace-snapshot/settlement-schema.sha256"
                )
                .trim(),
                '8',
            ),
            (
                include_str!(
                    "../../../fixtures/lawpack/workspace-snapshot/reconciliation-law.sha256"
                )
                .trim(),
                '7',
            ),
        ] {
            let substituted = source.replacen(
                fixture_digest,
                &format!("sha256:{}", sentinel.to_string().repeat(64)),
                1,
            );
            assert_ne!(substituted, source, "resource fixture digest must mutate");
            source = substituted;
        }
        test_ok(
            fs::write(&source_path, source),
            "write sentinel-bound application source",
        );

        let failure = test_err(
            build_application(&config_path),
            "syntactically valid placeholder resource digests must not publish",
        );

        assert_eq!(failure.kind, "ExternalActionResourceClosureMismatch");
        assert!(!root.join(".build/application/core.cbor").exists());
        assert!(!root.join(".build/application/target-ir.cbor").exists());
        test_ok(fs::remove_dir_all(root), "remove sentinel resource tree");
    }

    #[test]
    fn public_external_action_build_rejects_opaque_resource_definitions() {
        let root = temp_tree("public-external-action-opaque-resource");
        let config_path = write_external_action_application(&root);
        let artifact_path = root.join("vendor/workspace-snapshot/input-schema.cbor");
        let source_path = root.join("src/observe-workspace.edict");
        let artifact_bytes = test_ok(fs::read(&artifact_path), "read input schema artifact");
        let CanonicalValue::Map(mut artifact) = test_ok(
            decode_canonical_cbor(&artifact_bytes),
            "decode input schema artifact",
        ) else {
            panic!("input schema artifact is a canonical map");
        };
        let Some((_, definition)) = artifact
            .iter_mut()
            .find(|(key, _)| key == &CanonicalValue::Text("definition".to_owned()))
        else {
            panic!("input schema artifact carries a definition");
        };
        *definition = CanonicalValue::Map(vec![(
            CanonicalValue::Text("opaque".to_owned()),
            CanonicalValue::Text("not-a-schema".to_owned()),
        )]);
        let substituted_bytes = test_ok(
            encode_canonical_cbor(&CanonicalValue::Map(artifact)),
            "encode opaque input schema artifact",
        );
        let substituted_digest = test_ok(
            digest_canonical_artifact(EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, &substituted_bytes),
            "digest opaque input schema artifact",
        )
        .to_review_string();
        test_ok(
            fs::write(&artifact_path, substituted_bytes),
            "write opaque input schema artifact",
        );
        let source = test_ok(
            fs::read_to_string(&source_path),
            "read resource-bound source",
        );
        let original_digest =
            include_str!("../../../fixtures/lawpack/workspace-snapshot/input-schema.sha256").trim();
        let substituted = source.replacen(original_digest, &substituted_digest, 1);
        assert_ne!(substituted, source, "input schema digest must mutate");
        test_ok(
            fs::write(&source_path, substituted),
            "pin opaque input schema artifact",
        );

        let failure = test_err(
            build_application(&config_path),
            "opaque resource definition must not publish",
        );

        assert_eq!(failure.kind, "InvalidExternalActionResource");
        assert!(!root.join(".build/application/core.cbor").exists());
        assert!(!root.join(".build/application/target-ir.cbor").exists());
        test_ok(fs::remove_dir_all(root), "remove opaque resource tree");
    }

    #[test]
    fn public_external_action_build_rejects_invalid_request_resource_closure() {
        for (case, expected_kind) in [
            ("missing", "ExternalActionResourceClosureMismatch"),
            ("duplicate", "ExternalActionResourceClosureMismatch"),
            ("disconnected", "ExternalActionResourceClosureMismatch"),
            ("noncanonical", "InvalidExternalActionResource"),
        ] {
            let root = temp_tree(&format!("public-external-action-resource-{case}"));
            let config_path = write_external_action_application(&root);
            let mut config = test_ok(
                serde_json::from_slice::<serde_json::Value>(&test_ok(
                    fs::read(&config_path),
                    "read application config",
                )),
                "decode application config",
            );
            let Some(resources) = config
                .get_mut("externalActionResources")
                .and_then(serde_json::Value::as_array_mut)
            else {
                panic!("application config has an externalActionResources array");
            };
            match case {
                "missing" => {
                    let removed = resources.pop();
                    assert!(removed.is_some(), "resource fixture must be removable");
                }
                "duplicate" => {
                    let Some(first) = resources.first().cloned() else {
                        panic!("resource fixture is non-empty");
                    };
                    resources.push(first);
                }
                "disconnected" => {
                    let disconnected =
                        root.join("vendor/workspace-snapshot/disconnected-resource.cbor");
                    test_ok(
                        fs::write(
                            &disconnected,
                            include_bytes!(
                                "../../../fixtures/lawpack/workspace-patch/input-schema.cbor"
                            ),
                        ),
                        "write disconnected resource",
                    );
                    resources.push(serde_json::json!({
                        "artifact": "vendor/workspace-snapshot/disconnected-resource.cbor"
                    }));
                }
                "noncanonical" => {
                    test_ok(
                        fs::write(
                            root.join("vendor/workspace-snapshot/input-schema.cbor"),
                            [0x18, 0x00],
                        ),
                        "write non-canonical resource",
                    );
                }
                _ => unreachable!("the case table is closed"),
            }
            test_ok(
                fs::write(
                    &config_path,
                    test_ok(
                        serde_json::to_vec_pretty(&config),
                        "encode application config",
                    ),
                ),
                "write mutated application config",
            );

            let failure = test_err(
                build_application(&config_path),
                "invalid request resource closure must reject",
            );

            assert_eq!(
                failure.kind, expected_kind,
                "case {case} returned unexpected failure kind"
            );
            assert!(!root.join(".build/application/core.cbor").exists());
            assert!(!root.join(".build/application/target-ir.cbor").exists());
            test_ok(fs::remove_dir_all(root), "remove invalid closure tree");
        }
    }

    fn canonical_map_field_mut<'a>(
        fields: &'a mut [(CanonicalValue, CanonicalValue)],
        key: &str,
    ) -> &'a mut CanonicalValue {
        fields
            .iter_mut()
            .find_map(|(candidate, value)| {
                matches!(candidate, CanonicalValue::Text(text) if text == key).then_some(value)
            })
            .unwrap_or_else(|| panic!("canonical map contains `{key}`"))
    }

    fn mutate_external_action_resource(
        artifact: &mut [(CanonicalValue, CanonicalValue)],
        ordinal: usize,
        seed: u64,
    ) {
        match ordinal {
            0 => {
                *canonical_map_field_mut(artifact, "apiVersion") =
                    CanonicalValue::Text(format!("edict.external-action-resource/v1-{seed:016x}"));
            }
            1 => {
                *canonical_map_field_mut(artifact, "coordinate") =
                    CanonicalValue::Text(format!("workspace.snapshot.input.mutated-{seed:016x}@1"));
            }
            2 => {
                *canonical_map_field_mut(artifact, "kind") =
                    CanonicalValue::Text("settlementSchema".to_owned());
            }
            _ => {
                let CanonicalValue::Map(definition) =
                    canonical_map_field_mut(artifact, "definition")
                else {
                    panic!("property resource has a definition map");
                };
                match ordinal {
                    3 => {
                        *canonical_map_field_mut(definition, "encoding") =
                            CanonicalValue::Text("noncanonical-cbor".to_owned());
                    }
                    4 => {
                        *canonical_map_field_mut(definition, "root") = CanonicalValue::Text(
                            format!("boundedWorkspaceObservationInput-{seed:016x}"),
                        );
                    }
                    5 => {
                        *canonical_map_field_mut(definition, "closed") =
                            CanonicalValue::Bool(false);
                    }
                    6 => {
                        *canonical_map_field_mut(definition, "fields") =
                            CanonicalValue::Array(Vec::new());
                    }
                    7..=14 => {
                        let CanonicalValue::Array(fields) =
                            canonical_map_field_mut(definition, "fields")
                        else {
                            panic!("property schema has a fields array");
                        };
                        let field_ordinal = (ordinal - 7) / 4;
                        let Some(CanonicalValue::Map(field)) = fields.get_mut(field_ordinal) else {
                            panic!("property schema has field {field_ordinal}");
                        };
                        let field_key =
                            ["name", "type", "required", "authority"][(ordinal - 7) % 4];
                        match canonical_map_field_mut(field, field_key) {
                            CanonicalValue::Text(text) => {
                                test_ok(
                                    std::fmt::Write::write_fmt(
                                        text,
                                        format_args!("-{seed:016x}-{ordinal:02x}"),
                                    ),
                                    "mutate property schema text",
                                );
                            }
                            CanonicalValue::Bool(required) => *required = !*required,
                            _ => panic!("property schema field `{field_key}` is scalar"),
                        }
                    }
                    15 => {
                        let CanonicalValue::Array(fields) =
                            canonical_map_field_mut(definition, "fields")
                        else {
                            panic!("property schema has a fields array");
                        };
                        fields.reverse();
                    }
                    _ => unreachable!("the mutation table is closed"),
                }
            }
        }
    }

    #[test]
    fn fixed_seed_request_resource_mutations_fail_closed() {
        const RESOURCE_MUTATION_SEED: u64 = 0x4558_5452_4551_0180;

        let mutations = [
            ("api-version", "InvalidExternalActionResource"),
            ("coordinate", "ExternalActionResourceClosureMismatch"),
            ("kind", "ExternalActionResourceClosureMismatch"),
            ("definition-encoding", "InvalidExternalActionResource"),
            ("definition-root", "ExternalActionResourceClosureMismatch"),
            ("definition-closed", "InvalidExternalActionResource"),
            ("definition-fields-empty", "InvalidExternalActionResource"),
            ("field-zero-name", "ExternalActionResourceClosureMismatch"),
            ("field-zero-type", "ExternalActionResourceClosureMismatch"),
            (
                "field-zero-required",
                "ExternalActionResourceClosureMismatch",
            ),
            (
                "field-zero-authority",
                "ExternalActionResourceClosureMismatch",
            ),
            ("field-one-name", "ExternalActionResourceClosureMismatch"),
            ("field-one-type", "ExternalActionResourceClosureMismatch"),
            (
                "field-one-required",
                "ExternalActionResourceClosureMismatch",
            ),
            (
                "field-one-authority",
                "ExternalActionResourceClosureMismatch",
            ),
            ("field-order", "ExternalActionResourceClosureMismatch"),
        ];

        for (ordinal, (case, expected_kind)) in mutations.into_iter().enumerate() {
            let root = temp_tree(&format!("external-resource-property-{case}"));
            let config_path = write_external_action_application(&root);
            let artifact_path = root.join("vendor/workspace-snapshot/input-schema.cbor");
            let artifact_bytes = test_ok(fs::read(&artifact_path), "read property resource");
            let CanonicalValue::Map(mut artifact) = test_ok(
                decode_canonical_cbor(&artifact_bytes),
                "decode property resource",
            ) else {
                panic!("property resource is a canonical map");
            };

            mutate_external_action_resource(&mut artifact, ordinal, RESOURCE_MUTATION_SEED);
            let mutated = test_ok(
                encode_canonical_cbor(&CanonicalValue::Map(artifact)),
                "encode property resource mutation",
            );
            test_ok(
                fs::write(&artifact_path, mutated),
                "write property resource mutation",
            );

            let failure = test_err(
                build_application(&config_path),
                "stale request digest must reject a canonical resource mutation",
            );
            assert_eq!(
                failure.kind, expected_kind,
                "property case {case} ({ordinal}) returned the wrong failure kind"
            );
            assert!(!root.join(".build/application/core.cbor").exists());
            assert!(!root.join(".build/application/target-ir.cbor").exists());
            test_ok(fs::remove_dir_all(root), "remove property resource tree");
        }
    }

    #[test]
    fn sixty_four_request_resources_resolve_as_one_bounded_closure() {
        let closure = [external_action_loaded_lawpack()];
        let mut external = external_action_target_ir();
        let Some(intent) = external.intents.get_mut("observe") else {
            panic!("workspace observer intent exists");
        };
        let Some(template) = intent.external_action_requests.first().cloned() else {
            panic!("workspace observer request exists");
        };
        intent.external_action_requests.clear();
        let mut resources = external_action_loaded_resources()
            .into_iter()
            .filter(|resource| resource.kind != ExternalActionResourceKind::InputSchema)
            .collect::<Vec<_>>();
        for ordinal in 0_u8..64 {
            let coordinate = format!("workspace.snapshot.input-{ordinal:02}@1");
            let digest = format!("sha256:{}", format!("{:02x}", ordinal + 1).repeat(32));
            let mut request = template.clone();
            request.id = format!("observe-{ordinal:02}");
            request.input_schema.coordinate = coordinate.clone();
            request.input_schema.digest = Some(digest.clone());
            intent.external_action_requests.push(request);
            resources.push(LoadedExternalActionResource {
                coordinate,
                kind: ExternalActionResourceKind::InputSchema,
                digest,
            });
        }
        assert_eq!(intent.external_action_requests.len(), 64);
        assert_eq!(resources.len(), 66);

        test_ok(
            validate_external_action_artifacts(&external, &closure, &resources),
            "64 request resource identities form one bounded exact closure",
        );
    }

    #[test]
    fn external_action_resource_configuration_has_a_fixed_boundary() {
        let mut config = application_manifest(1);
        config.build_kind = ApplicationBuildKind::ExternalAction;
        config.external_action_resources = (0..MAX_EXTERNAL_ACTION_RESOURCES)
            .map(|index| ApplicationExternalActionResource {
                artifact: PathBuf::from(format!("resources/{index:03}.cbor")),
            })
            .collect();
        test_ok(
            validate_application_manifest(&config),
            "the exact resource configuration boundary is accepted",
        );
        config
            .external_action_resources
            .push(ApplicationExternalActionResource {
                artifact: PathBuf::from("resources/overflow.cbor"),
            });
        let failure = test_err(
            validate_application_manifest(&config),
            "one resource beyond the boundary must reject",
        );
        assert_eq!(failure.kind, "InvalidApplicationConfig");
    }

    #[test]
    fn public_external_action_build_rejects_capability_substitution() {
        let root = temp_tree("public-external-action-substitution");
        let config_path = write_external_action_application(&root);
        let source_path = root.join("src/observe-workspace.edict");
        let source = test_ok(fs::read_to_string(&source_path), "read application source");
        let manifest_digest =
            include_str!("../../../fixtures/lawpack/workspace-snapshot/manifest.sha256").trim();
        let capability_import =
            format!("use capability workspace.snapshot.observe@1 digest \"{manifest_digest}\"");
        let substituted = source.replacen(
            &capability_import,
            &format!(
                "use capability workspace.snapshot.observe@1 digest \"sha256:{}\"",
                "0".repeat(64)
            ),
            1,
        );
        assert_ne!(substituted, source, "capability import fixture must mutate");
        test_ok(
            fs::write(&source_path, substituted),
            "write substituted application source",
        );

        let failure = test_err(
            build_application(&config_path),
            "substituted capability closure must reject",
        );

        assert_eq!(failure.kind, "ExternalActionCapabilityClosureMismatch");
        assert!(!root.join(".build/application/core.cbor").exists());
        assert!(!root.join(".build/application/target-ir.cbor").exists());
        test_ok(fs::remove_dir_all(root), "remove substituted build tree");
    }

    #[test]
    fn public_external_action_build_rejects_a_disconnected_lawpack() {
        let root = temp_tree("public-external-action-disconnected-lawpack");
        let config_path = write_external_action_application(&root);
        let lawpack_directory = root.join("vendor/hello-echo");
        test_ok(
            fs::create_dir_all(&lawpack_directory),
            "create disconnected lawpack directory",
        );
        for (name, bytes) in [
            (
                "manifest.cbor",
                include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice(),
            ),
            (
                "exports.cbor",
                include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice(),
            ),
            (
                "adapter.cbor",
                include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice(),
            ),
            (
                "target-configuration.cbor",
                include_bytes!(
                    "../../../fixtures/lawpack/hello-echo/echo-operation-configuration.cbor"
                )
                .as_slice(),
            ),
        ] {
            test_ok(
                fs::write(lawpack_directory.join(name), bytes),
                "write disconnected lawpack fixture",
            );
        }
        let mut application = test_ok(
            serde_json::from_slice::<serde_json::Value>(&test_ok(
                fs::read(&config_path),
                "read application manifest",
            )),
            "decode application manifest",
        );
        let Some(lawpacks) = application
            .get_mut("lawpacks")
            .and_then(serde_json::Value::as_array_mut)
        else {
            panic!("application manifest lawpacks");
        };
        lawpacks.push(serde_json::json!({
            "manifest": "vendor/hello-echo/manifest.cbor",
            "exports": "vendor/hello-echo/exports.cbor",
            "adapter": "vendor/hello-echo/adapter.cbor",
            "targetConfiguration": "vendor/hello-echo/target-configuration.cbor"
        }));
        test_ok(
            fs::write(
                &config_path,
                test_ok(
                    serde_json::to_vec_pretty(&application),
                    "encode application manifest",
                ),
            ),
            "write application manifest",
        );

        let failure = test_err(
            build_application(&config_path),
            "a disconnected supplied lawpack must reject",
        );

        assert_eq!(failure.kind, "InvalidLawpackClosure");
        assert!(!root.join(".build/application/core.cbor").exists());
        assert!(!root.join(".build/application/target-ir.cbor").exists());
        test_ok(fs::remove_dir_all(root), "remove disconnected build tree");
    }

    #[test]
    fn relative_application_config_uses_the_current_directory_as_root() {
        let actual = test_ok(
            canonical_application_root(PathBuf::from("edict.application.json").as_path()),
            "resolve relative application root",
        );
        let expected = test_ok(fs::canonicalize("."), "resolve current directory");

        assert_eq!(actual, expected);
    }

    #[test]
    fn application_manifest_rejects_absolute_and_parent_traversal_paths() {
        let mut source_escape = application_manifest(1);
        source_escape.sources = vec![PathBuf::from("../outside.edict")];
        let mut lawpack_escape = application_manifest(1);
        lawpack_escape.lawpacks[0].manifest = PathBuf::from("/outside/manifest.cbor");
        let mut provider_escape = application_manifest(1);
        provider_escape.target.provider_package = PathBuf::from("../provider");
        let mut output_escape = application_manifest(1);
        output_escape.output_directory = PathBuf::from("/outside/output");

        for config in [
            source_escape,
            lawpack_escape,
            provider_escape,
            output_escape,
        ] {
            assert!(
                validate_application_manifest(&config).is_err(),
                "application-owned paths must remain beneath the manifest root"
            );
        }
    }

    #[test]
    fn selected_adapter_reference_follows_the_selected_target_profile() {
        let first_profile = lawpack_ref("target.first@1", 0x11);
        let selected_profile = lawpack_ref("target.selected@1", 0x22);
        let first = adapter_descriptor(first_profile, "adapter.first@1", 0x31);
        let selected = adapter_descriptor(selected_profile.clone(), "adapter.selected@1", 0x32);
        let adapters = [first, selected];

        let actual = test_ok(
            selected_adapter_reference(&adapters, &selected_profile),
            "selected adapter",
        );

        assert_eq!(actual.id, "adapter.selected@1");
    }

    #[test]
    fn repeated_effect_references_to_one_configuration_are_deduplicated() {
        let configuration = lawpack_ref("target.configuration@1", 0x44);

        let actual = test_ok(
            single_unique_configuration([&configuration, &configuration]),
            "identical effect references name one configuration",
        );

        assert_eq!(actual, &configuration);
    }

    #[test]
    fn operation_profile_configuration_is_selected_when_adapter_has_no_effects() {
        let loaded = external_action_loaded_lawpack();
        let adapter = test_ok(
            decode_lawpack_adapter(&loaded.bundle, "echo.dpo@1", &loaded.adapter_bytes),
            "decode request-only adapter",
        );
        assert!(
            adapter.effects().is_empty(),
            "request-only fixture must remain effect-free"
        );

        let required_core_profiles = BTreeSet::from(["continuum.profile.read-only/v1"]);
        let configuration = test_ok(
            single_configuration_for_required_core_profiles(
                adapter.operation_profiles(),
                adapter.effects(),
                &required_core_profiles,
                &BTreeSet::new(),
            ),
            "profile-owned target configuration",
        );

        assert_eq!(configuration.id, "workspace.snapshot.request-profile/v1");

        let source_bytes = test_ok(
            encode_canonical_cbor(&CanonicalValue::Bytes(
                include_str!(
                    "../../../fixtures/lawpack/workspace-snapshot/observe-workspace.edict"
                )
                .as_bytes()
                .to_vec(),
            )),
            "encode the invocation source artifact",
        );
        let target_ir_bytes = include_bytes!(
            "../../../fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor"
        );

        let inputs = test_ok(
            lowering_inputs(
                &loaded,
                &adapter,
                configuration,
                "examples.workspace_observer@1",
                &super::SourceArtifact {
                    domain: super::SOURCE_DOMAIN,
                    kind: "edict-source",
                    bytes: source_bytes,
                },
                target_ir_bytes,
                &result_projection_artifact(),
            ),
            "construct the lowerer invocation inputs",
        );
        let Some(configuration_input) = inputs
            .iter()
            .find(|input| input.role == "05-target-configuration")
        else {
            panic!("lowerer invocation carries the selected target configuration");
        };
        assert_eq!(configuration_input.role, "05-target-configuration");
        assert_eq!(
            configuration_input.kind,
            edict_syntax::ProviderSemanticInputKind::Auxiliary("target-configuration".to_owned())
        );
        assert_eq!(
            configuration_input.artifact.reference.coordinate,
            configuration.id
        );
        assert_eq!(
            configuration_input.artifact.reference.digest.algorithm,
            edict_syntax::ProviderDigestAlgorithm::Sha256
        );
        assert_eq!(
            configuration_input.artifact.reference.digest.bytes,
            configuration.digest
        );
        assert_eq!(
            configuration_input.artifact.artifact.domain,
            configuration.id
        );
        assert_eq!(
            configuration_input.artifact.artifact.bytes,
            loaded.configuration_bytes
        );
    }

    #[test]
    fn unused_operation_profile_configuration_does_not_enter_application_selection() {
        let selected_configuration = lawpack_ref("target.selected-configuration@1", 0x45);
        let unused_configuration = lawpack_ref("target.unused-configuration@1", 0x46);
        let operation_profiles = BTreeMap::from([
            (
                "profile.selected@1".to_owned(),
                LawpackAdapterOperationProfile {
                    core: "continuum.profile.read-only/v1".to_owned(),
                    semantic_effects: Vec::new(),
                    budget_obligation: Some("budget.selected@1".to_owned()),
                    target_configuration: Some(selected_configuration.clone()),
                },
            ),
            (
                "profile.unused@1".to_owned(),
                LawpackAdapterOperationProfile {
                    core: "continuum.profile.write-only/v1".to_owned(),
                    semantic_effects: Vec::new(),
                    budget_obligation: Some("budget.unused@1".to_owned()),
                    target_configuration: Some(unused_configuration),
                },
            ),
        ]);
        let required_core_profiles = BTreeSet::from(["continuum.profile.read-only/v1"]);
        let effects = BTreeMap::new();

        let actual = test_ok(
            single_configuration_for_required_core_profiles(
                &operation_profiles,
                &effects,
                &required_core_profiles,
                &BTreeSet::new(),
            ),
            "configuration from the Core-selected operation profile",
        );

        assert_eq!(actual, &selected_configuration);
    }

    #[test]
    fn unused_effect_configuration_does_not_enter_application_selection() {
        let manifest =
            include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice();
        let exports =
            include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice();
        let adapter_bytes =
            include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice();
        let source = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
        let bundle = test_ok(
            decode_lawpack_bundle(manifest, exports),
            "decode Hello Echo lawpack",
        );
        let adapter = test_ok(
            decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter_bytes),
            "decode Hello Echo adapter",
        );
        let module = test_ok(parse_module(source), "parse Hello Echo source");
        let preparation = test_ok(
            prepare_lawpack_compilation(&module, &bundle, &adapter),
            "prepare Hello Echo compilation",
        );
        let core = test_ok(
            compile_to_core(&module, preparation.compiler_context()),
            "compile Hello Echo Core",
        );

        let mut operation_profiles = adapter.operation_profiles().clone();
        let mut effects = adapter.effects().clone();
        assert_eq!(operation_profiles.len(), 1, "fixture selects one profile");
        assert_eq!(effects.len(), 1, "fixture invokes one effect");
        let Some(used_effect_coordinate) = effects.keys().next().cloned() else {
            panic!("fixture effect coordinate");
        };
        let Some(used_effect) = effects.get(&used_effect_coordinate) else {
            panic!("fixture effect");
        };
        let selected_configuration = used_effect.target_configuration.clone();
        assert_eq!(
            semantic_effects_required_by_core(&core),
            BTreeSet::from([used_effect_coordinate.clone()]),
            "configuration selection follows the exact effects present in Core",
        );
        let mut unused_effect = used_effect.clone();
        unused_effect.target_configuration =
            lawpack_ref("target.unused-effect-configuration@1", 0x47);
        let unused_effect_coordinate = "hello.echo@1.unusedEffect".to_owned();
        effects.insert(unused_effect_coordinate.clone(), unused_effect);
        let Some(operation_profile) = operation_profiles.values_mut().next() else {
            panic!("fixture operation profile");
        };
        operation_profile
            .semantic_effects
            .push(unused_effect_coordinate);

        let actual = test_ok(
            single_configuration_for_core(&operation_profiles, &effects, &core),
            "configuration from the effect actually invoked by Core",
        );

        assert_eq!(actual, &selected_configuration);
    }

    #[test]
    fn application_configuration_rejects_effect_outside_selected_profile() {
        let manifest =
            include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice();
        let exports =
            include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice();
        let adapter_bytes =
            include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice();
        let source = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
        let bundle = test_ok(
            decode_lawpack_bundle(manifest, exports),
            "decode Hello Echo lawpack",
        );
        let adapter = test_ok(
            decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter_bytes),
            "decode Hello Echo adapter",
        );
        let module = test_ok(parse_module(source), "parse Hello Echo source");
        let preparation = test_ok(
            prepare_lawpack_compilation(&module, &bundle, &adapter),
            "prepare Hello Echo compilation",
        );
        let core = test_ok(
            compile_to_core(&module, preparation.compiler_context()),
            "compile Hello Echo Core",
        );

        let mut operation_profiles = adapter.operation_profiles().clone();
        let mut effects = adapter.effects().clone();
        let Some((invoked_coordinate, invoked_effect)) = effects
            .first_key_value()
            .map(|(coordinate, effect)| (coordinate.clone(), effect.clone()))
        else {
            panic!("Hello Echo invokes one effect");
        };
        assert_eq!(
            semantic_effects_required_by_core(&core),
            BTreeSet::from([invoked_coordinate.clone()]),
        );
        let selected_only_coordinate = "hello.echo@1.selectedOnly".to_owned();
        let mut selected_only_effect = invoked_effect.clone();
        selected_only_effect.target_configuration =
            lawpack_ref("target.selected-profile-configuration@1", 0x49);
        effects.insert(selected_only_coordinate.clone(), selected_only_effect);

        let Some((selected_coordinate, selected_profile)) = operation_profiles
            .first_key_value()
            .map(|(coordinate, profile)| (coordinate.clone(), profile.clone()))
        else {
            panic!("Hello Echo selects one profile");
        };
        let Some(selected_entry) = operation_profiles.get_mut(&selected_coordinate) else {
            panic!("selected profile remains present");
        };
        selected_entry.semantic_effects = vec![selected_only_coordinate];
        operation_profiles.insert(
            "hello.echo@1.otherProfile".to_owned(),
            LawpackAdapterOperationProfile {
                core: "continuum.profile.other/v1".to_owned(),
                semantic_effects: vec![invoked_coordinate],
                budget_obligation: selected_profile.budget_obligation,
                target_configuration: selected_profile.target_configuration,
            },
        );

        let failure = test_err(
            single_configuration_for_core(&operation_profiles, &effects, &core),
            "an invoked effect outside the selected profile must reject",
        );

        assert_eq!(failure.kind, "InvalidLawpackAdapter");
    }

    #[test]
    fn selected_effectful_profile_without_invocations_uses_unique_advertised_configuration() {
        let manifest =
            include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice();
        let exports =
            include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice();
        let adapter_bytes =
            include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice();
        let bundle = test_ok(
            decode_lawpack_bundle(manifest, exports),
            "decode Hello Echo lawpack",
        );
        let adapter = test_ok(
            decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter_bytes),
            "decode Hello Echo adapter",
        );
        let mut operation_profiles = adapter.operation_profiles().clone();
        let mut effects = adapter.effects().clone();
        let Some((advertised_coordinate, advertised_effect)) = effects
            .first_key_value()
            .map(|(coordinate, effect)| (coordinate.clone(), effect.clone()))
        else {
            panic!("Hello Echo advertises one semantic effect");
        };
        let duplicate_coordinate = "hello.echo@1.sameConfiguration".to_owned();
        effects.insert(duplicate_coordinate.clone(), advertised_effect.clone());
        let Some(profile) = operation_profiles.values_mut().next() else {
            panic!("Hello Echo advertises one operation profile");
        };
        profile.semantic_effects = vec![advertised_coordinate, duplicate_coordinate.clone()];
        let required_core_profile = profile.core.clone();
        let required_core_profiles = BTreeSet::from([required_core_profile.as_str()]);

        let selected = test_ok(
            single_configuration_for_required_core_profiles(
                &operation_profiles,
                &effects,
                &required_core_profiles,
                &BTreeSet::new(),
            ),
            "unique configuration advertised by an uninvoked effectful profile",
        );
        assert_eq!(selected, &advertised_effect.target_configuration);

        let Some(duplicate_effect) = effects.get_mut(&duplicate_coordinate) else {
            panic!("duplicate advertised effect");
        };
        duplicate_effect.target_configuration =
            lawpack_ref("target.ambiguous-configuration@1", 0x48);
        let failure = test_err(
            single_configuration_for_required_core_profiles(
                &operation_profiles,
                &effects,
                &required_core_profiles,
                &BTreeSet::new(),
            ),
            "ambiguous advertised configurations must reject",
        );
        assert_eq!(failure.kind, "InvalidLawpackAdapter");
    }

    #[test]
    fn public_application_selects_and_binds_configuration_from_compiled_core_profiles() {
        let root = temp_tree("public-selected-core-profile");
        let config_path = write_external_action_application(&root);
        install_two_profile_workspace_lawpack(&root);

        test_ok(
            build_application(&config_path),
            "build with a differently configured unused operation profile",
        );

        assert!(root.join(".build/application/core.cbor").is_file());
        assert!(root.join(".build/application/target-ir.cbor").is_file());
        test_ok(fs::remove_dir_all(root), "remove two-profile build tree");

        let root = temp_tree("public-mismatched-core-profile");
        let config_path = write_external_action_application(&root);
        install_two_profile_workspace_lawpack(&root);
        let mut config = test_ok(
            serde_json::from_slice::<serde_json::Value>(&test_ok(
                fs::read(&config_path),
                "read application manifest",
            )),
            "decode application manifest",
        );
        config["lawpacks"][0]["targetConfiguration"] =
            serde_json::json!("vendor/workspace-snapshot/unused-profile-configuration.cbor");
        test_ok(
            fs::write(
                &config_path,
                test_ok(
                    serde_json::to_vec_pretty(&config),
                    "encode mismatched application manifest",
                ),
            ),
            "write mismatched application manifest",
        );

        let failure = test_err(
            build_application(&config_path),
            "the unused configuration cannot impersonate the selected profile",
        );

        assert_eq!(failure.kind, "TargetConfigurationMismatch");
        assert!(
            !root.join(".build/application").exists(),
            "a configuration mismatch must reject before publication"
        );
        test_ok(fs::remove_dir_all(root), "remove mismatched build tree");
    }

    #[test]
    fn compiler_result_projection_is_bound_into_the_provider_closure() {
        let projection = result_projection_artifact();
        let bytes = projection.canonical_bytes().to_vec();
        let input = test_ok(
            with_result_projection_input(Vec::new(), &projection),
            "bind compiler result projection",
        );

        assert_eq!(input.len(), 1);
        assert_eq!(input[0].role, RESULT_PROJECTION_ROLE);
        assert_eq!(
            input[0].kind,
            edict_syntax::ProviderSemanticInputKind::Auxiliary("result-projection".to_owned())
        );
        assert_eq!(
            input[0].artifact.reference.coordinate,
            "examples.hello_echo@1.createGreeting"
        );
        assert_eq!(
            input[0].artifact.artifact.domain,
            RESULT_PROJECTION_DIGEST_DOMAIN
        );
        assert_eq!(input[0].artifact.artifact.bytes, bytes);
    }

    #[test]
    fn application_build_requires_one_projection_and_no_projection_failures() {
        let projection = result_projection_artifact();
        let empty = BTreeMap::new();
        let no_failures = BTreeMap::new();
        assert_eq!(
            test_err(
                single_result_projection(&empty, &no_failures),
                "an application build without a projection must reject",
            )
            .kind,
            "ResultProjectionUnavailable"
        );

        let multiple = BTreeMap::from([
            ("first".to_owned(), projection.clone()),
            ("second".to_owned(), projection.clone()),
        ]);
        assert_eq!(
            test_err(
                single_result_projection(&multiple, &no_failures),
                "an application build with multiple projections must reject",
            )
            .kind,
            "ResultProjectionUnavailable"
        );

        let admitted = BTreeMap::from([("createGreeting".to_owned(), projection)]);
        let projection_failure = test_err(
            decode_result_projection(&[]),
            "empty projection bytes must reject",
        );
        let failures = BTreeMap::from([("createGreeting".to_owned(), projection_failure)]);
        assert_eq!(
            test_err(
                single_result_projection(&admitted, &failures),
                "a recorded projection failure must reject the application build",
            )
            .kind,
            "ResultProjectionUnavailable"
        );
    }

    #[test]
    fn provider_schema_selection_loads_every_bound_role_under_bounded_stress() {
        let mut manifest = provider_manifest_with_schema_count(32);
        let mut state = STRESS_SEED;
        for index in (1..manifest.artifacts.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bound = test_ok(u64::try_from(index + 1), "stress bound fits u64");
            let swap_with = test_ok(
                usize::try_from(state % bound),
                "stress permutation index fits usize",
            );
            manifest.artifacts.swap(index, swap_with);
        }

        let actual = test_ok(
            provider_schema_artifacts(&manifest),
            "every schema role bound by the provider must be loaded",
        );
        let roles = actual
            .iter()
            .map(|artifact| artifact.role.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(actual.len(), 32);
        assert_eq!(roles.len(), 32);
    }

    fn result_projection_artifact() -> ResultProjectionArtifact {
        let mut report = hello_echo_target_ir_report();
        match report.result_projections.remove("createGreeting") {
            Some(projection) => projection,
            None => panic!("Hello Echo lowering emits its result projection"),
        }
    }

    fn hello_echo_target_ir() -> TargetIrArtifact {
        match hello_echo_target_ir_report().artifact {
            Some(artifact) => artifact,
            None => panic!("Hello Echo lowering emits Target IR"),
        }
    }

    fn hello_echo_target_ir_report() -> TargetLoweringReport {
        let manifest =
            include_bytes!("../../../fixtures/lawpack/hello-echo/manifest.cbor").as_slice();
        let exports =
            include_bytes!("../../../fixtures/lawpack/hello-echo/exports.cbor").as_slice();
        let adapter =
            include_bytes!("../../../fixtures/lawpack/hello-echo/adapter.cbor").as_slice();
        let source = include_str!("../../../fixtures/lawpack/hello-echo/create-greeting.edict");
        let bundle = test_ok(
            decode_lawpack_bundle(manifest, exports),
            "decode Hello Echo lawpack",
        );
        let adapter = test_ok(
            decode_lawpack_adapter(&bundle, "echo.dpo@1", adapter),
            "decode Hello Echo adapter",
        );
        let module = test_ok(parse_module(source), "parse Hello Echo source");
        let preparation = test_ok(
            prepare_lawpack_compilation(&module, &bundle, &adapter),
            "prepare Hello Echo compilation",
        );
        let core = test_ok(
            compile_to_core(&module, preparation.compiler_context()),
            "compile Hello Echo Core",
        );
        lower_to_target_ir(&core, preparation.target_ir_facts())
    }

    fn external_action_target_ir() -> TargetIrArtifact {
        let loaded = external_action_loaded_lawpack();
        let source =
            include_str!("../../../fixtures/lawpack/workspace-snapshot/observe-workspace.edict");
        let module = test_ok(parse_module(source), "parse workspace observation source");
        let adapter = test_ok(
            decode_lawpack_adapter(&loaded.bundle, "echo.dpo@1", &loaded.adapter_bytes),
            "decode workspace observation adapter",
        );
        let preparation = test_ok(
            prepare_lawpack_compilation(&module, &loaded.bundle, &adapter),
            "prepare workspace observation compilation",
        );
        let core = test_ok(
            compile_to_core(&module, preparation.compiler_context()),
            "compile workspace observation Core",
        );
        match lower_to_target_ir(&core, preparation.target_ir_facts()).artifact {
            Some(artifact) => artifact,
            None => panic!("workspace observation lowering emits Target IR"),
        }
    }

    fn external_action_loaded_lawpack() -> LoadedLawpack {
        let manifest_bytes =
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/manifest.cbor").to_vec();
        let exports_bytes =
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/exports.cbor").to_vec();
        let adapter_bytes =
            include_bytes!("../../../fixtures/lawpack/workspace-snapshot/adapter.cbor").to_vec();
        let configuration_bytes = include_bytes!(
            "../../../fixtures/lawpack/workspace-snapshot/request-profile-configuration.cbor"
        )
        .to_vec();
        let bundle = test_ok(
            decode_lawpack_bundle(&manifest_bytes, &exports_bytes),
            "decode workspace observation lawpack",
        );
        LoadedLawpack {
            manifest_bytes,
            exports_bytes,
            adapter_bytes,
            configuration_bytes,
            bundle,
        }
    }

    fn external_action_loaded_resources() -> Vec<LoadedExternalActionResource> {
        [
            (
                "workspace.snapshot.input@1",
                ExternalActionResourceKind::InputSchema,
                include_bytes!("../../../fixtures/lawpack/workspace-snapshot/input-schema.cbor")
                    .as_slice(),
            ),
            (
                "workspace.snapshot.settlement@1",
                ExternalActionResourceKind::SettlementSchema,
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/settlement-schema.cbor"
                )
                .as_slice(),
            ),
            (
                "workspace.snapshot.reconcile@1",
                ExternalActionResourceKind::ReconciliationLaw,
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/reconciliation-law.cbor"
                )
                .as_slice(),
            ),
        ]
        .into_iter()
        .map(|(coordinate, kind, bytes)| LoadedExternalActionResource {
            coordinate: coordinate.to_owned(),
            kind,
            digest: test_ok(
                digest_canonical_artifact(EXTERNAL_ACTION_RESOURCE_DIGEST_DOMAIN, bytes),
                "digest external-action resource fixture",
            )
            .to_review_string(),
        })
        .collect()
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the test lawpack keeps both profile configurations reviewable"
    )]
    fn install_two_profile_workspace_lawpack(root: &std::path::Path) {
        let definition = test_ok(
            serde_json::from_value::<LawpackAuthoringDefinition>(serde_json::json!({
                "schema": "edict.lawpack-authoring/v1",
                "id": "workspace.snapshot",
                "version": "1",
                "acceptedCoreAbi": ["edict.core/v1"],
                "dependencies": [],
                "exportsCoordinate": "workspace.snapshot.exports/v1",
                "exports": {
                    "types": [],
                    "constants": [],
                    "pureFunctions": [],
                    "effects": [],
                    "obstructions": [],
                    "operationProfiles": {
                        "workspace.snapshot@1.observeRequest": {
                            "opticTemplate": {
                                "opticKind": "revelation",
                                "boundaryKind": "projection",
                                "supportPolicy": "workspace.snapshot@1.requestOnly",
                                "lossDisposition": "workspace.snapshot@1.lossless",
                                "basisTemplate": null,
                                "apertureRequirement": {
                                    "kind": "abstractFootprintObligation",
                                    "reference": "workspace.snapshot@1.authorityScope"
                                }
                            },
                            "effectPredicate": "workspace.snapshot@1.externalObservation"
                        },
                        "workspace.snapshot@1.unusedRequest": {
                            "opticTemplate": {
                                "opticKind": "revelation",
                                "boundaryKind": "projection",
                                "supportPolicy": "workspace.snapshot@1.requestOnly",
                                "lossDisposition": "workspace.snapshot@1.lossless",
                                "basisTemplate": null,
                                "apertureRequirement": {
                                    "kind": "abstractFootprintObligation",
                                    "reference": "workspace.snapshot@1.authorityScope"
                                }
                            },
                            "effectPredicate": "workspace.snapshot@1.externalObservation"
                        }
                    }
                },
                "targetAdapters": [{
                    "coordinate": "workspace.snapshot.echo-adapter/v1",
                    "output": "adapter.cbor",
                    "acceptedTargetProfile": {
                        "id": "echo.dpo@1",
                        "digest": "sha256:2e2494121aecf5e6a2d920f5fb85408825d394765fad41484c416397c920fb04"
                    },
                    "acceptedTargetIr": {
                        "id": "echo.span-ir/v1",
                        "digest": "sha256:0057167e68f50c99dcce087b3e1cd677d17c5d1dc238bdb52d89469e1472fc2f"
                    },
                    "operationProfiles": {
                        "workspace.snapshot@1.observeRequest": {
                            "core": "continuum.profile.read-only/v1",
                            "semanticEffects": [],
                            "budgetObligation": "workspace.snapshot@1.tinyObservationBudget",
                            "targetConfiguration": {"local": "selected-configuration"}
                        },
                        "workspace.snapshot@1.unusedRequest": {
                            "core": "continuum.profile.unused/v1",
                            "semanticEffects": [],
                            "budgetObligation": "workspace.snapshot@1.tinyObservationBudget",
                            "targetConfiguration": {"local": "unused-configuration"}
                        }
                    },
                    "effectImplementations": {},
                    "budgets": {
                        "workspace.snapshot@1.tinyObservationBudget": {
                            "maxSteps": 512,
                            "maxAllocatedBytes": 262_144,
                            "maxOutputBytes": 131_072
                        }
                    }
                }],
                "helperComponent": null,
                "verifier": {
                    "class": "declarative",
                    "ruleset": {
                        "id": "workspace.snapshot.verifier-rules/v1",
                        "digest": format!("sha256:{}", "84".repeat(32))
                    }
                },
                "compatibility": {
                    "id": "workspace.snapshot.compatibility/v1",
                    "digest": format!("sha256:{}", "85".repeat(32))
                },
                "conformanceFixtureCorpus": {
                    "id": "workspace.snapshot.fixtures/v1",
                    "digest": format!("sha256:{}", "86".repeat(32))
                },
                "localResources": [{
                    "name": "selected-configuration",
                    "coordinate": "workspace.snapshot.request-profile/v1",
                    "output": "request-profile-configuration.cbor",
                    "value": {
                        "operation": "workspace.snapshot.observe@1",
                        "apiVersion": "workspace.snapshot.request-profile/v1",
                        "basisClass": "workspace-root",
                        "authorityClass": "scoped"
                    }
                }, {
                    "name": "unused-configuration",
                    "coordinate": "workspace.snapshot.unused-profile/v1",
                    "output": "unused-profile-configuration.cbor",
                    "value": {
                        "apiVersion": "workspace.snapshot.unused-profile/v1",
                        "sentinel": "must-not-select"
                    }
                }]
            })),
            "decode two-profile lawpack definition",
        );
        let artifacts = test_ok(
            author_lawpack(&definition, &[]),
            "author two-profile lawpack",
        );
        let lawpack_directory = root.join("vendor/workspace-snapshot");
        for artifact in artifacts.artifacts() {
            let path = lawpack_directory.join(artifact.path());
            if let Some(parent) = path.parent() {
                test_ok(
                    fs::create_dir_all(parent),
                    "create authored artifact parent",
                );
            }
            test_ok(
                fs::write(path, artifact.bytes()),
                "write authored lawpack artifact",
            );
        }

        let manifest = test_ok(
            artifacts
                .artifact(LawpackArtifactKind::Manifest)
                .ok_or("authored lawpack has no manifest"),
            "locate authored lawpack manifest",
        );
        let source_path = root.join("src/observe-workspace.edict");
        let source = test_ok(fs::read_to_string(&source_path), "read workspace source");
        let original_digest =
            include_str!("../../../fixtures/lawpack/workspace-snapshot/manifest.sha256").trim();
        let repinned = source.replace(original_digest, manifest.digest());
        assert_ne!(repinned, source, "source lawpack digest must move");
        test_ok(fs::write(source_path, repinned), "repin workspace source");

        let selected = test_ok(
            fs::read(lawpack_directory.join("request-profile-configuration.cbor")),
            "read selected configuration",
        );
        let unused = test_ok(
            fs::read(lawpack_directory.join("unused-profile-configuration.cbor")),
            "read unused configuration",
        );
        assert_ne!(
            selected, unused,
            "the unused configuration is materially distinct"
        );
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the public-build fixture keeps its complete file closure visible"
    )]
    fn write_external_action_application(root: &std::path::Path) -> PathBuf {
        let source_directory = root.join("src");
        let lawpack_directory = root.join("vendor/workspace-snapshot");
        let provider_directory = root.join("provider");
        let provider_generated = provider_directory.join("generated/primary");
        for directory in [&source_directory, &lawpack_directory, &provider_generated] {
            test_ok(
                fs::create_dir_all(directory),
                "create application fixture tree",
            );
        }

        for (path, bytes) in [
            (
                source_directory.join("observe-workspace.edict"),
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/observe-workspace.edict"
                )
                .as_slice(),
            ),
            (
                lawpack_directory.join("manifest.cbor"),
                include_bytes!("../../../fixtures/lawpack/workspace-snapshot/manifest.cbor")
                    .as_slice(),
            ),
            (
                lawpack_directory.join("exports.cbor"),
                include_bytes!("../../../fixtures/lawpack/workspace-snapshot/exports.cbor")
                    .as_slice(),
            ),
            (
                lawpack_directory.join("adapter.cbor"),
                include_bytes!("../../../fixtures/lawpack/workspace-snapshot/adapter.cbor")
                    .as_slice(),
            ),
            (
                lawpack_directory.join("request-profile-configuration.cbor"),
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/request-profile-configuration.cbor"
                )
                .as_slice(),
            ),
            (
                lawpack_directory.join("input-schema.cbor"),
                include_bytes!("../../../fixtures/lawpack/workspace-snapshot/input-schema.cbor")
                    .as_slice(),
            ),
            (
                lawpack_directory.join("settlement-schema.cbor"),
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/settlement-schema.cbor"
                )
                .as_slice(),
            ),
            (
                lawpack_directory.join("reconciliation-law.cbor"),
                include_bytes!(
                    "../../../fixtures/lawpack/workspace-snapshot/reconciliation-law.cbor"
                )
                .as_slice(),
            ),
            (
                provider_generated.join("target-profile.echo-dpo.cbor"),
                include_bytes!(
                    "../../../fixtures/providers/echo-target-profile/generated/primary/target-profile.echo-dpo.cbor"
                )
                .as_slice(),
            ),
        ] {
            test_ok(fs::write(path, bytes), "write application fixture");
        }

        let provider_manifest = serde_json::json!({
            "apiVersion": TARGET_PROVIDER_MANIFEST_API_VERSION,
            "providerAbi": TARGET_PROVIDER_ABI,
            "provider": resource_json("echo.edict-provider@1", '1'),
            "artifacts": [
                {
                    "role": "target-profile.echo-dpo",
                    "artifactKind": "targetProfile",
                    "resource": {
                        "coordinate": "echo.dpo@1",
                        "digest": "sha256:2e2494121aecf5e6a2d920f5fb85408825d394765fad41484c416397c920fb04"
                    },
                    "source": {
                        "kind": "generated",
                        "semanticSource": resource_json("echo.semantic-schema@1", '2'),
                        "generator": resource_json("echo-wesley-gen.provider-artifact-generator@1", '3')
                    }
                },
                {
                    "role": "schema.echo-provider-artifacts",
                    "artifactKind": "artifactSchema",
                    "resource": resource_json("echo.provider-artifacts.cddl@1", '4'),
                    "source": {
                        "kind": "generated",
                        "semanticSource": resource_json("echo.semantic-schema@1", '2'),
                        "generator": resource_json("echo-wesley-gen.provider-artifact-generator@1", '3')
                    }
                }
            ],
            "schemaBindings": [{
                "domain": "echo.generated-artifact/v1",
                "schemaRole": "schema.echo-provider-artifacts",
                "format": "selfContainedCddlV1",
                "rootRule": "generated-artifact"
            }]
        });
        test_ok(
            fs::write(
                provider_directory.join("provider-manifest.echo.json"),
                test_ok(
                    serde_json::to_vec_pretty(&provider_manifest),
                    "encode provider manifest",
                ),
            ),
            "write provider manifest",
        );

        let application = serde_json::json!({
            "schema": "edict.application/v1",
            "buildKind": "externalAction",
            "coordinate": "examples.workspace_observer@1",
            "sources": ["src/observe-workspace.edict"],
            "lawpacks": [{
                "manifest": "vendor/workspace-snapshot/manifest.cbor",
                "exports": "vendor/workspace-snapshot/exports.cbor",
                "adapter": "vendor/workspace-snapshot/adapter.cbor",
                "targetConfiguration": "vendor/workspace-snapshot/request-profile-configuration.cbor"
            }],
            "externalActionResources": [
                {"artifact": "vendor/workspace-snapshot/input-schema.cbor"},
                {"artifact": "vendor/workspace-snapshot/settlement-schema.cbor"},
                {"artifact": "vendor/workspace-snapshot/reconciliation-law.cbor"}
            ],
            "target": {
                "profile": "echo.dpo@1",
                "providerPackage": "provider"
            },
            "outputDirectory": ".build/application"
        });
        let config_path = root.join("edict.application.json");
        test_ok(
            fs::write(
                &config_path,
                test_ok(
                    serde_json::to_vec_pretty(&application),
                    "encode application manifest",
                ),
            ),
            "write application manifest",
        );
        config_path
    }

    fn resource_json(coordinate: &str, digest: char) -> serde_json::Value {
        serde_json::json!({
            "coordinate": coordinate,
            "digest": format!("sha256:{}", digest.to_string().repeat(64))
        })
    }

    #[test]
    fn failed_pair_publication_preserves_the_previous_package() {
        let root = temp_tree("pair-publication");
        let package_path = root.join("executable-operation-package.cbor");
        let report_path = root.join("verification-report.cbor");
        test_ok(
            fs::write(&package_path, b"previous-package"),
            "write prior package",
        );
        test_ok(
            fs::create_dir(&report_path),
            "create conflicting report directory",
        );

        let failure = test_err(
            write_outputs(&root, b"new-package", b"new-report"),
            "a non-file report target must reject the pair",
        );

        assert_eq!(failure.kind, "ApplicationOutputWriteFailed");
        assert_eq!(
            test_ok(fs::read(&package_path), "read preserved package"),
            b"previous-package"
        );
        test_ok(fs::remove_dir_all(root), "remove test-owned temp tree");
    }

    #[test]
    fn executable_publication_removes_stale_external_action_outputs() {
        let root = temp_tree("executable-stale");
        test_ok(
            fs::write(root.join("core.cbor"), b"stale-core"),
            "write stale Core",
        );
        test_ok(
            fs::write(root.join("target-ir.cbor"), b"stale-target-ir"),
            "write stale Target IR",
        );

        test_ok(
            write_outputs(&root, b"package", b"report"),
            "publish executable pair over stale external-action outputs",
        );

        assert!(!root.join("core.cbor").exists());
        assert!(!root.join("target-ir.cbor").exists());
        assert_eq!(
            test_ok(
                fs::read(root.join("executable-operation-package.cbor")),
                "read package",
            ),
            b"package"
        );
        assert_eq!(
            test_ok(
                fs::read(root.join("verification-report.cbor")),
                "read report",
            ),
            b"report"
        );
        test_ok(fs::remove_dir_all(root), "remove executable output tree");
    }

    #[test]
    fn external_action_publication_writes_the_exact_compiler_pair() {
        let root = temp_tree("external-action-pair");
        let core = include_bytes!("../../../fixtures/core/canonical/workspace-snapshot.core.cbor");
        let target_ir = include_bytes!(
            "../../../fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor"
        );

        test_ok(
            write_external_action_outputs(&root, core, target_ir),
            "publish compiler-owned external-action artifacts",
        );

        assert_eq!(
            test_ok(fs::read(root.join("core.cbor")), "read published Core"),
            core
        );
        assert_eq!(
            test_ok(
                fs::read(root.join("target-ir.cbor")),
                "read published Target IR",
            ),
            target_ir
        );
        assert!(!root.join("executable-operation-package.cbor").exists());
        assert!(!root.join("verification-report.cbor").exists());
        test_ok(fs::remove_dir_all(root), "remove external-action pair");
    }

    #[test]
    fn external_action_publication_removes_stale_executable_outputs() {
        let root = temp_tree("external-action-stale");
        test_ok(
            fs::write(
                root.join("executable-operation-package.cbor"),
                b"stale-package",
            ),
            "write stale package",
        );
        test_ok(
            fs::write(root.join("verification-report.cbor"), b"stale-report"),
            "write stale report",
        );

        test_ok(
            write_external_action_outputs(&root, b"core", b"target"),
            "publish external-action pair over stale executable outputs",
        );

        assert!(!root.join("executable-operation-package.cbor").exists());
        assert!(!root.join("verification-report.cbor").exists());
        assert_eq!(
            test_ok(fs::read(root.join("core.cbor")), "read Core"),
            b"core"
        );
        assert_eq!(
            test_ok(fs::read(root.join("target-ir.cbor")), "read Target IR"),
            b"target"
        );
        test_ok(fs::remove_dir_all(root), "remove stale-output tree");
    }

    #[test]
    fn failed_external_action_pair_publication_preserves_previous_core() {
        let root = temp_tree("external-action-rollback");
        let core_path = root.join("core.cbor");
        let target_path = root.join("target-ir.cbor");
        test_ok(fs::write(&core_path, b"previous-core"), "write prior Core");
        test_ok(
            fs::create_dir(&target_path),
            "create conflicting Target IR directory",
        );

        let failure = test_err(
            write_external_action_outputs(&root, b"new-core", b"new-target"),
            "a non-file target must reject the pair",
        );

        assert_eq!(failure.kind, "ApplicationOutputWriteFailed");
        assert_eq!(
            test_ok(fs::read(&core_path), "read preserved Core"),
            b"previous-core"
        );
        test_ok(fs::remove_dir_all(root), "remove rollback tree");
    }

    #[test]
    fn external_action_pair_publication_is_deterministic_for_a_fixed_seed_corpus() {
        let root = temp_tree("external-action-property");
        let mut state = STRESS_SEED;
        for ordinal in 0_u8..16 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let core = state.to_le_bytes();
            let target = [ordinal; 8];

            test_ok(
                write_external_action_outputs(&root, &core, &target),
                "publish fixed-seed pair",
            );
            assert_eq!(
                test_ok(fs::read(root.join("core.cbor")), "read property Core"),
                core
            );
            assert_eq!(
                test_ok(
                    fs::read(root.join("target-ir.cbor")),
                    "read property Target IR",
                ),
                target
            );
        }
        test_ok(fs::remove_dir_all(root), "remove property tree");
    }

    #[test]
    fn external_action_pair_publication_remains_bounded_under_stress() {
        let root = temp_tree("external-action-stress");
        for ordinal in 0_u8..64 {
            let core = vec![ordinal; 1024];
            let target = vec![ordinal.wrapping_add(1); 1024];
            test_ok(
                write_external_action_outputs(&root, &core, &target),
                "publish bounded stress pair",
            );
        }

        assert_eq!(
            test_ok(fs::metadata(root.join("core.cbor")), "measure stress Core",).len(),
            1024
        );
        assert_eq!(
            test_ok(
                fs::metadata(root.join("target-ir.cbor")),
                "measure stress Target IR",
            )
            .len(),
            1024
        );
        test_ok(fs::remove_dir_all(root), "remove stress tree");
    }

    #[test]
    fn application_artifact_reads_are_bounded_before_allocation() {
        let root = temp_tree("bounded-read");
        let path = root.join("oversized.cbor");
        let file = test_ok(fs::File::create(&path), "create oversized artifact");
        test_ok(
            file.set_len(1024 * 1024 + 1),
            "size oversized artifact without allocating it",
        );

        let failure = test_err(
            read(&path, "ArtifactReadFailed", "test artifact"),
            "oversized artifact must reject",
        );

        assert_eq!(failure.kind, "ApplicationArtifactTooLarge");
        test_ok(fs::remove_dir_all(root), "remove bounded read temp tree");
    }

    #[cfg(unix)]
    #[test]
    fn application_build_rejects_source_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = temp_tree("source-symlink-root");
        let outside = temp_tree("source-symlink-outside");
        let outside_source = outside.join("operation.edict");
        test_ok(
            fs::write(&outside_source, "package examples.test@1;"),
            "write external source",
        );
        test_ok(
            symlink(&outside_source, root.join("operation.edict")),
            "link source outside application root",
        );
        let config_path = root.join("edict.application.json");
        let config = serde_json::json!({
            "schema": "edict.application/v1",
            "coordinate": "examples.test@1.operation",
            "sources": ["operation.edict"],
            "lawpacks": [{
                "manifest": "lawpack/manifest.cbor",
                "exports": "lawpack/exports.cbor",
                "adapter": "lawpack/adapter.cbor",
                "targetConfiguration": "lawpack/configuration.cbor"
            }],
            "target": {
                "profile": "target.test@1",
                "providerPackage": "provider"
            },
            "outputDirectory": ".build/application"
        });
        let config_bytes = test_ok(serde_json::to_vec(&config), "encode application config");
        test_ok(
            fs::write(&config_path, config_bytes),
            "write application config",
        );

        let failure = test_err(
            build_application(&config_path),
            "source symlink outside the manifest root must reject",
        );

        assert_eq!(failure.kind, "ApplicationPathEscape");
        test_ok(fs::remove_dir_all(root), "remove symlink root");
        test_ok(fs::remove_dir_all(outside), "remove symlink target");
    }

    #[test]
    fn successful_publication_leaves_no_lock_in_output_directory() {
        let root = temp_tree("output-lock-location");
        let lock_path = output_lock_path(&root);

        test_ok(
            write_outputs(&root, b"package", b"report"),
            "publish application outputs",
        );

        assert!(!root.join(".edict-application-build.lock").exists());
        test_ok(fs::remove_file(lock_path), "remove test output lock");
        test_ok(fs::remove_dir_all(root), "remove output lock temp tree");
    }

    fn application_manifest(lawpack_count: usize) -> ApplicationManifest {
        ApplicationManifest {
            schema: "edict.application/v1".to_owned(),
            build_kind: ApplicationBuildKind::ExecutableOperation,
            coordinate: "examples.test@1.operation".to_owned(),
            sources: vec![PathBuf::from("operation.edict")],
            lawpacks: (0..lawpack_count)
                .map(|index| ApplicationLawpack {
                    manifest: PathBuf::from(format!("lawpacks/{index}/manifest.cbor")),
                    exports: PathBuf::from(format!("lawpacks/{index}/exports.cbor")),
                    adapter: PathBuf::from(format!("lawpacks/{index}/adapter.cbor")),
                    target_configuration: PathBuf::from(format!(
                        "lawpacks/{index}/configuration.cbor"
                    )),
                })
                .collect(),
            external_action_resources: Vec::new(),
            target: ApplicationTarget {
                profile: "target.test@1".to_owned(),
                provider_package: PathBuf::from("provider"),
            },
            output_directory: PathBuf::from(".build/application"),
        }
    }

    fn lawpack_ref(id: &str, digest_byte: u8) -> LawpackResourceRef {
        LawpackResourceRef {
            id: id.to_owned(),
            digest: [digest_byte; 32],
        }
    }

    fn adapter_descriptor(
        accepted_target_profile: LawpackResourceRef,
        adapter_id: &str,
        digest_byte: u8,
    ) -> LawpackTargetAdapter {
        LawpackTargetAdapter {
            accepted_target_profile,
            accepted_target_ir: lawpack_ref("target.ir/v1", digest_byte.wrapping_add(1)),
            adapter: lawpack_ref(adapter_id, digest_byte),
        }
    }

    fn provider_manifest_with_schema_count(count: usize) -> TargetProviderManifest {
        let artifacts = (0..count).map(schema_artifact).collect::<Vec<_>>();
        let schema_bindings = (0..count)
            .map(|index| ProviderSchemaBinding {
                domain: format!("example.schema-{index:02}/v1"),
                schema_role: format!("schema.example-{index:02}"),
                format: ProviderSchemaFormat::SelfContainedCddlV1,
                root_rule: format!("example-{index:02}"),
            })
            .collect();
        TargetProviderManifest {
            api_version: TARGET_PROVIDER_MANIFEST_API_VERSION.to_owned(),
            provider_abi: TARGET_PROVIDER_ABI.to_owned(),
            provider: resource("provider.example@1", 0x51),
            artifacts,
            schema_bindings,
        }
    }

    fn schema_artifact(index: usize) -> ProviderArtifactRef {
        ProviderArtifactRef {
            role: format!("schema.example-{index:02}"),
            artifact_kind: ProviderArtifactKind::ArtifactSchema,
            resource: resource(
                &format!("schema.example-{index:02}@1"),
                test_ok(u8::try_from(index), "schema fixture index fits u8"),
            ),
            source: ProviderArtifactSource::Generated {
                semantic_source: resource("source.example@1", 0x61),
                generator: resource("generator.example@1", 0x62),
            },
        }
    }

    fn resource(coordinate: &str, digest_byte: u8) -> ResourceRef {
        ResourceRef {
            coordinate: coordinate.to_owned(),
            digest: Some(format!(
                "sha256:{}",
                format!("{digest_byte:02x}").repeat(32)
            )),
        }
    }

    fn temp_tree(name: &str) -> PathBuf {
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "edict-application-build-{name}-{}-{unique}",
            std::process::id()
        ));
        test_ok(fs::create_dir_all(&path), "create test temp tree");
        path
    }

    fn test_ok<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("{context}: {error:?}"),
        }
    }

    fn test_err<T: std::fmt::Debug, E>(result: Result<T, E>, context: &str) -> E {
        match result {
            Ok(value) => panic!("{context}: got {value:?}"),
            Err(error) => error,
        }
    }
}
