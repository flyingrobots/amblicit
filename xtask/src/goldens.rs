use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use edict_syntax::{
    assemble_contract_bundle, canonical_target_profile_contract_resources, compile_to_core,
    digest_authority_facts_document, digest_core_module, digest_target_ir_artifact,
    encode_authority_facts_cbor, encode_core_module, encode_target_ir_artifact,
    load_authority_facts_file, lower_to_target_ir, parse_module, CompilerContext,
    ContractBundleAssemblyInput, ContractBundleSourceArtifact, CoreBudget, DigestLockedResource,
    ResourceRef, SuppliedTargetIrResource, TargetEffectLowering, TargetIrArtifact,
    TargetIrLoweringFacts, WriteClass, ECHO_DPO_TARGET_PROFILE, ECHO_SPAN_IR_DOMAIN,
    GITWARP_COMMIT_REDUCER_IR_DOMAIN, GITWARP_REF_CRDT_TARGET_PROFILE,
};

use crate::util::{dirs, read_to_string, run_cmd};

const CLI_MAX_STDIN_BYTES_ENV: &str = "EDICT_CLI_MAX_STDIN_BYTES";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthorityFactsGoldenMode {
    Check,
    Write,
}

#[derive(Debug)]
struct AuthorityFactsGoldenCase {
    review: &'static str,
    bytes: &'static str,
    digest: &'static str,
}

const AUTHORITY_FACTS_GOLDEN_CASES: &[AuthorityFactsGoldenCase] = &[AuthorityFactsGoldenCase {
    review: "fixtures/authority-facts/canonical/example-effectful.authority-facts.json",
    bytes: "fixtures/authority-facts/canonical/example-effectful.authority-facts.cbor",
    digest: "fixtures/authority-facts/canonical/example-effectful.authority-facts.sha256",
}];

pub(crate) fn authority_facts_goldens(
    root: &Path,
    mode: AuthorityFactsGoldenMode,
) -> Result<(), String> {
    for case in AUTHORITY_FACTS_GOLDEN_CASES {
        let document = load_authority_facts_file(root.join(case.review))
            .map_err(|failures| format!("load {}: {failures:?}", case.review))?;
        let bytes = encode_authority_facts_cbor(&document)
            .map_err(|failures| format!("encode {}: {failures:?}", case.review))?;
        let digest = digest_authority_facts_document(&document)
            .map_err(|failures| format!("digest {}: {failures:?}", case.review))?;
        let digest = format!("{digest}\n");

        match mode {
            AuthorityFactsGoldenMode::Check => {
                check_golden_file(root, case.bytes, &bytes)?;
                check_golden_file(root, case.digest, digest.as_bytes())?;
            }
            AuthorityFactsGoldenMode::Write => {
                write_golden_file(&root.join(case.bytes), &bytes)?;
                write_golden_file(&root.join(case.digest), digest.as_bytes())?;
            }
        }
    }

    println!(
        "authority-facts-goldens: {} case(s) {}",
        AUTHORITY_FACTS_GOLDEN_CASES.len(),
        match mode {
            AuthorityFactsGoldenMode::Check => "checked",
            AuthorityFactsGoldenMode::Write => "written",
        }
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetProfileResourceGoldenMode {
    Check,
    Write,
}

pub(crate) fn target_profile_resource_goldens(
    root: &Path,
    mode: TargetProfileResourceGoldenMode,
) -> Result<(), String> {
    let resources = canonical_target_profile_contract_resources();
    for resource in &resources {
        let bytes_path = resource.provenance.source_path.as_str();
        let digest_path = bytes_path
            .strip_suffix(".cbor")
            .map(|stem| format!("{stem}.sha256"))
            .ok_or_else(|| format!("contract resource path is not canonical CBOR: {bytes_path}"))?;
        let digest = format!("{}\n", resource.digest);

        match mode {
            TargetProfileResourceGoldenMode::Check => {
                check_golden_file(root, bytes_path, &resource.canonical_bytes)?;
                check_golden_file(root, &digest_path, digest.as_bytes())?;
            }
            TargetProfileResourceGoldenMode::Write => {
                write_golden_file(&root.join(bytes_path), &resource.canonical_bytes)?;
                write_golden_file(&root.join(digest_path), digest.as_bytes())?;
            }
        }
    }

    println!(
        "target-profile-resource-goldens: {} resource(s) {}",
        resources.len(),
        match mode {
            TargetProfileResourceGoldenMode::Check => "checked",
            TargetProfileResourceGoldenMode::Write => "written",
        }
    );
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreGoldenMode {
    Check,
    Write,
}

#[derive(Debug)]
struct CoreGoldenCase {
    source: &'static str,
    bytes: &'static str,
    digest: &'static str,
}

const CORE_GOLDEN_CASES: &[CoreGoldenCase] = &[
    CoreGoldenCase {
        source: "fixtures/lang/bounds/bounded-hello.edict",
        bytes: "fixtures/core/canonical/bounded-hello.core.cbor",
        digest: "fixtures/core/canonical/bounded-hello.core.sha256",
    },
    CoreGoldenCase {
        source: "fixtures/lang/external-actions/workspace-snapshot.edict",
        bytes: "fixtures/core/canonical/workspace-snapshot.core.cbor",
        digest: "fixtures/core/canonical/workspace-snapshot.core.sha256",
    },
];

pub(crate) fn core_goldens(root: &Path, mode: CoreGoldenMode) -> Result<(), String> {
    for case in CORE_GOLDEN_CASES {
        check_or_write_core_golden(root, case, mode)?;
    }
    println!(
        "core-goldens: {} case(s) {}",
        CORE_GOLDEN_CASES.len(),
        match mode {
            CoreGoldenMode::Check => "checked",
            CoreGoldenMode::Write => "written",
        }
    );
    Ok(())
}

fn check_or_write_core_golden(
    root: &Path,
    case: &CoreGoldenCase,
    mode: CoreGoldenMode,
) -> Result<(), String> {
    let source = read_to_string(&root.join(case.source))?;
    let module = parse_module(&source).map_err(|err| format!("parse {}: {err}", case.source))?;
    let core = compile_to_core(&module, &core_golden_context())
        .map_err(|err| format!("compile {} to Core: {err:?}", case.source))?;
    let bytes = encode_core_module(&core)
        .map_err(|err| format!("encode {} as canonical Core: {err}", case.source))?;
    let digest = digest_core_module(&core)
        .map_err(|err| format!("digest {} as canonical Core: {err}", case.source))?;
    let digest_text = format!("{digest}\n");

    match mode {
        CoreGoldenMode::Check => {
            check_golden_file(root, case.bytes, &bytes)?;
            check_golden_file(root, case.digest, digest_text.as_bytes())?;
        }
        CoreGoldenMode::Write => {
            write_golden_file(&root.join(case.bytes), &bytes)?;
            write_golden_file(&root.join(case.digest), digest_text.as_bytes())?;
        }
    }
    Ok(())
}

fn core_golden_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("hello.readOnly", "continuum.profile.read-only/v1")
        .with_operation_profile("workspace.read", "continuum.profile.read-only/v1")
        .with_budget(
            "hello.tinyBudget",
            CoreBudget {
                max_steps: 64,
                max_allocated_bytes: 4096,
                max_output_bytes: 4096,
            },
        )
        .with_budget(
            "workspace.tiny",
            CoreBudget {
                max_steps: 512,
                max_allocated_bytes: 256 * 1024,
                max_output_bytes: 128 * 1024,
            },
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetIrGoldenMode {
    Check,
    Write,
}

#[derive(Debug, Clone, Copy)]
enum TargetIrGoldenKind {
    Echo,
    Gitwarp,
    ExternalRequest,
}

#[derive(Debug)]
struct TargetIrGoldenCase {
    kind: TargetIrGoldenKind,
    bytes: &'static str,
    digest: &'static str,
}

const TARGET_IR_GOLDEN_CASES: &[TargetIrGoldenCase] = &[
    TargetIrGoldenCase {
        kind: TargetIrGoldenKind::Echo,
        bytes: "fixtures/target-ir/canonical/echo-effectful.target-ir.cbor",
        digest: "fixtures/target-ir/canonical/echo-effectful.target-ir.sha256",
    },
    TargetIrGoldenCase {
        kind: TargetIrGoldenKind::Gitwarp,
        bytes: "fixtures/target-ir/canonical/gitwarp-append.target-ir.cbor",
        digest: "fixtures/target-ir/canonical/gitwarp-append.target-ir.sha256",
    },
    TargetIrGoldenCase {
        kind: TargetIrGoldenKind::ExternalRequest,
        bytes: "fixtures/target-ir/canonical/workspace-snapshot.target-ir.cbor",
        digest: "fixtures/target-ir/canonical/workspace-snapshot.target-ir.sha256",
    },
];

const TARGET_IR_ECHO_SOURCE: &str = "package a.b@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.effectful\n\
      basis none\n\
      budget <= p.tiny {\n\
      let receipt: Receipt = target.replace(input.id)\n\
        else { rejected(reason) => domain.WriteRejected };\n\
      return { id: input.id };\n\
    }";

const TARGET_IR_GITWARP_SOURCE: &str = "package a.git@1;\n\
    type Input = { id: String<max=16>, };\n\
    type Receipt = { id: String<max=16>, };\n\
    type Output = { id: String<max=16>, };\n\
    intent t(input: Input) returns Output\n\
      profile p.gitwarp\n\
      basis none\n\
      budget <= p.tiny\n\
      where input.id != \"\" {\n\
      let receipt: Receipt = gitwarp.appendEvent(input.id)\n\
        else { conflict(reason) => domain.MergeConflict };\n\
      return { id: receipt.id };\n\
    }";

const TARGET_IR_EXTERNAL_REQUEST_SOURCE: &str =
    include_str!("../../fixtures/lang/external-actions/workspace-snapshot.edict");

pub(crate) fn target_ir_goldens(root: &Path, mode: TargetIrGoldenMode) -> Result<(), String> {
    for case in TARGET_IR_GOLDEN_CASES {
        check_or_write_target_ir_golden(root, case, mode)?;
    }
    println!(
        "target-ir-goldens: {} case(s) {}",
        TARGET_IR_GOLDEN_CASES.len(),
        match mode {
            TargetIrGoldenMode::Check => "checked",
            TargetIrGoldenMode::Write => "written",
        }
    );
    Ok(())
}

fn check_or_write_target_ir_golden(
    root: &Path,
    case: &TargetIrGoldenCase,
    mode: TargetIrGoldenMode,
) -> Result<(), String> {
    let artifact = target_ir_golden_artifact(case)?;
    let bytes = encode_target_ir_artifact(&artifact)
        .map_err(|err| format!("encode {:?} as canonical Target IR: {err}", case.kind))?;
    let digest = digest_target_ir_artifact(&artifact)
        .map_err(|err| format!("digest {:?} as canonical Target IR: {err}", case.kind))?;
    let digest_text = format!("{digest}\n");

    match mode {
        TargetIrGoldenMode::Check => {
            check_golden_file(root, case.bytes, &bytes)?;
            check_golden_file(root, case.digest, digest_text.as_bytes())?;
        }
        TargetIrGoldenMode::Write => {
            write_golden_file(&root.join(case.bytes), &bytes)?;
            write_golden_file(&root.join(case.digest), digest_text.as_bytes())?;
        }
    }
    Ok(())
}

fn target_ir_golden_artifact(case: &TargetIrGoldenCase) -> Result<TargetIrArtifact, String> {
    let (source, context, facts) = match case.kind {
        TargetIrGoldenKind::Echo => (
            TARGET_IR_ECHO_SOURCE,
            target_ir_echo_context(),
            target_ir_echo_facts(),
        ),
        TargetIrGoldenKind::Gitwarp => (
            TARGET_IR_GITWARP_SOURCE,
            target_ir_gitwarp_context(),
            target_ir_gitwarp_facts(),
        ),
        TargetIrGoldenKind::ExternalRequest => (
            TARGET_IR_EXTERNAL_REQUEST_SOURCE,
            target_ir_external_request_context(),
            target_ir_external_request_facts(),
        ),
    };
    let module = parse_module(source)
        .map_err(|err| format!("parse {:?} Target IR golden source: {err}", case.kind))?;
    let core = compile_to_core(&module, &context)
        .map_err(|err| format!("compile {:?} Target IR golden source: {err:?}", case.kind))?;
    let report = lower_to_target_ir(&core, &facts);
    report.artifact.ok_or_else(|| {
        format!(
            "lower {:?} Target IR golden source failed: {:?}",
            case.kind, report.failures
        )
    })
}

fn target_ir_echo_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.effectful", "continuum.profile.write/v1")
        .with_operation_profile_write_classes("p.effectful", [WriteClass::Replace])
        .with_effect_write_class("target.replace", WriteClass::Replace)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 8,
                max_allocated_bytes: 1024,
                max_output_bytes: 256,
            },
        )
}

fn target_ir_gitwarp_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("p.gitwarp", "continuum.profile.append/v1")
        .with_operation_profile_write_classes("p.gitwarp", [WriteClass::Append])
        .with_effect_write_class("gitwarp.appendEvent", WriteClass::Append)
        .with_budget(
            "p.tiny",
            CoreBudget {
                max_steps: 13,
                max_allocated_bytes: 2048,
                max_output_bytes: 512,
            },
        )
}

fn target_ir_external_request_context() -> CompilerContext {
    CompilerContext::new()
        .with_operation_profile("workspace.read", "continuum.profile.read-only/v1")
        .with_budget(
            "workspace.tiny",
            CoreBudget {
                max_steps: 512,
                max_allocated_bytes: 256 * 1024,
                max_output_bytes: 128 * 1024,
            },
        )
}

fn target_ir_echo_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(digest_text('1')),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.write/v1".to_owned()],
        obstruction_coordinates: vec!["rejected".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "target.replace".to_owned(),
            target_intrinsic: "echo.dpo@1.replace".to_owned(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn target_ir_gitwarp_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: GITWARP_REF_CRDT_TARGET_PROFILE.to_owned(),
            digest: Some(digest_text('2')),
        },
        target_ir_domain: GITWARP_COMMIT_REDUCER_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.append/v1".to_owned()],
        obstruction_coordinates: vec!["conflict".to_owned()],
        effect_lowerings: vec![TargetEffectLowering {
            effect: "gitwarp.appendEvent".to_owned(),
            target_intrinsic: "gitwarp.ref_crdt@1.appendEvent".to_owned(),
            failure_mappings: std::collections::BTreeMap::new(),
        }],
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn target_ir_external_request_facts() -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: ECHO_DPO_TARGET_PROFILE.to_owned(),
            digest: Some(digest_text('3')),
        },
        target_ir_domain: ECHO_SPAN_IR_DOMAIN.to_owned(),
        operation_profiles: vec!["continuum.profile.read-only/v1".to_owned()],
        obstruction_coordinates: Vec::new(),
        effect_lowerings: Vec::new(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BundleGoldenMode {
    Check,
    Write,
}

#[derive(Debug)]
struct BundleGoldenCase {
    source: &'static str,
    output: &'static str,
}

const BUNDLE_GOLDEN_CASES: &[BundleGoldenCase] = &[BundleGoldenCase {
    source: "fixtures/lang/bounds/bounded-hello.edict",
    output: "fixtures/bundle/assembly/bounded-hello.bundle-digests.txt",
}];

const BUNDLE_CORE_IR_COORDINATE: &str = "edict.core.bounded-hello/v1";
const BUNDLE_SOURCE_LOGICAL_PATH: &str = "contracts/hello.edict";
const BUNDLE_SOURCE_COORDINATE: &str = "source.contracts.hello";
const BUNDLE_SOURCE_DIGEST: char = 'e';
const BUNDLE_SOURCE_PROFILE_COORDINATE: &str = "source-profile.hello/v1";
const BUNDLE_SOURCE_PROFILE_DIGEST: char = 'f';
const BUNDLE_TARGET_PROFILE_COORDINATE: &str = "echo.dpo@1";
const BUNDLE_TARGET_PROFILE_DIGEST: char = 'c';
const BUNDLE_TARGET_IR_COORDINATE: &str = "echo.span-ir/v1";
const BUNDLE_TARGET_IR_DIGEST: char = 'd';
const BUNDLE_LAWPACK_COORDINATE: &str = "hello.optics@1";
const BUNDLE_LAWPACK_DIGEST: char = '2';
const BUNDLE_GENERATED_COORDINATE: &str = "echo.dpo.registration/v1";
const BUNDLE_GENERATED_DIGEST: char = '3';
const BUNDLE_COMPILER_COORDINATE: &str = "edict.compiler/v1";
const BUNDLE_COMPILER_DIGEST: char = '4';
const BUNDLE_LOWERER_COORDINATE: &str = "echo.dpo.lowerer/v1";
const BUNDLE_LOWERER_DIGEST: char = '5';
const BUNDLE_VERIFIER_COORDINATE: &str = "echo.dpo.verifier/v1";
const BUNDLE_VERIFIER_DIGEST: char = '6';
const BUNDLE_SEMANTIC_OPTIONS_COORDINATE: &str = "edict.compile-options.semantic/v1";
const BUNDLE_SEMANTIC_OPTIONS_DIGEST: char = '7';
const BUNDLE_NONSEMANTIC_OPTIONS_COORDINATE: &str = "edict.compile-options.nonsemantic/v1";
const BUNDLE_NONSEMANTIC_OPTIONS_DIGEST: char = '8';
const BUNDLE_BUILD_PROVENANCE_COORDINATE: &str = "edict.build-provenance/v1";
const BUNDLE_BUILD_PROVENANCE_DIGEST: char = '9';
const BUNDLE_CANONICALIZATION_COORDINATE: &str = "edict.canonical-cbor/v1";
const BUNDLE_CANONICALIZATION_DIGEST: char = '8';
const BUNDLE_CONFORMANCE_COORDINATE: &str = "echo.dpo.fixtures/v1";
const BUNDLE_CONFORMANCE_DIGEST: char = '9';
const BUNDLE_VERIFIER_REPORT_COORDINATE: &str = "echo.dpo.verifier-report/v1";
const BUNDLE_VERIFIER_REPORT_DIGEST: char = 'a';
const BUNDLE_COMPILE_EXPLANATION_COORDINATE: &str = "watson.compile-explanation/v1";
const BUNDLE_COMPILE_EXPLANATION_DIGEST: char = 'b';

pub(crate) fn bundle_goldens(root: &Path, mode: BundleGoldenMode) -> Result<(), String> {
    for case in BUNDLE_GOLDEN_CASES {
        check_or_write_bundle_golden(root, case, mode)?;
    }
    println!(
        "bundle-goldens: {} case(s) {}",
        BUNDLE_GOLDEN_CASES.len(),
        match mode {
            BundleGoldenMode::Check => "checked",
            BundleGoldenMode::Write => "written",
        }
    );
    Ok(())
}

fn check_or_write_bundle_golden(
    root: &Path,
    case: &BundleGoldenCase,
    mode: BundleGoldenMode,
) -> Result<(), String> {
    let golden = render_bundle_golden(root, case)?;
    match mode {
        BundleGoldenMode::Check => check_golden_file(root, case.output, golden.as_bytes()),
        BundleGoldenMode::Write => write_golden_file(&root.join(case.output), golden.as_bytes()),
    }
}

fn render_bundle_golden(root: &Path, case: &BundleGoldenCase) -> Result<String, String> {
    let input = bundle_golden_input(root, case)?;
    let manifest = assemble_contract_bundle(input)
        .map_err(|err| format!("assemble {} contract bundle: {err}", case.source))?;
    Ok(format!(
        "\
# Edict contract bundle v0.11 digest golden
# Generated by `cargo xtask bundle-goldens --write`.
# Scope: semantic/release bundle digest preimage shape and digest values only.
# Non-scope: canonical Target IR bytes and ContractBundleManifest bytes.

source_fixture = {source}
core_ir_coordinate = {core_ir_coordinate}
source_artifact = {source_logical_path} {source_coordinate} {source_digest}
source_profile_semantic_facts = {source_profile_coordinate} {source_profile_digest}
target_profile = {target_profile_coordinate} {target_profile_digest}
target_ir = {target_ir_coordinate} {target_ir_digest}
lawpack = {lawpack_coordinate} {lawpack_digest}
generated_artifact = {generated_coordinate} {generated_digest}
compiler = {compiler_coordinate} {compiler_digest}
lowerer = {lowerer_coordinate} {lowerer_digest}
verifier = {verifier_coordinate} {verifier_digest}
semantic_compile_options = {semantic_options_coordinate} {semantic_options_digest}
non_semantic_compile_options = {nonsemantic_options_coordinate} {nonsemantic_options_digest}
build_provenance = {build_provenance_coordinate} {build_provenance_digest}
canonicalization_profile = {canonicalization_coordinate} {canonicalization_digest}
conformance_fixture_corpus = {conformance_coordinate} {conformance_digest}
verifier_report = {verifier_report_coordinate} {verifier_report_digest}
compile_explanation = {compile_explanation_coordinate} {compile_explanation_digest}

semantic_bundle_digest = {semantic_bundle_digest}
release_bundle_digest = {release_bundle_digest}
",
        source = case.source,
        core_ir_coordinate = BUNDLE_CORE_IR_COORDINATE,
        source_logical_path = BUNDLE_SOURCE_LOGICAL_PATH,
        source_coordinate = BUNDLE_SOURCE_COORDINATE,
        source_digest = digest_text(BUNDLE_SOURCE_DIGEST),
        source_profile_coordinate = BUNDLE_SOURCE_PROFILE_COORDINATE,
        source_profile_digest = digest_text(BUNDLE_SOURCE_PROFILE_DIGEST),
        target_profile_coordinate = BUNDLE_TARGET_PROFILE_COORDINATE,
        target_profile_digest = digest_text(BUNDLE_TARGET_PROFILE_DIGEST),
        target_ir_coordinate = BUNDLE_TARGET_IR_COORDINATE,
        target_ir_digest = digest_text(BUNDLE_TARGET_IR_DIGEST),
        lawpack_coordinate = BUNDLE_LAWPACK_COORDINATE,
        lawpack_digest = digest_text(BUNDLE_LAWPACK_DIGEST),
        generated_coordinate = BUNDLE_GENERATED_COORDINATE,
        generated_digest = digest_text(BUNDLE_GENERATED_DIGEST),
        compiler_coordinate = BUNDLE_COMPILER_COORDINATE,
        compiler_digest = digest_text(BUNDLE_COMPILER_DIGEST),
        lowerer_coordinate = BUNDLE_LOWERER_COORDINATE,
        lowerer_digest = digest_text(BUNDLE_LOWERER_DIGEST),
        verifier_coordinate = BUNDLE_VERIFIER_COORDINATE,
        verifier_digest = digest_text(BUNDLE_VERIFIER_DIGEST),
        semantic_options_coordinate = BUNDLE_SEMANTIC_OPTIONS_COORDINATE,
        semantic_options_digest = digest_text(BUNDLE_SEMANTIC_OPTIONS_DIGEST),
        nonsemantic_options_coordinate = BUNDLE_NONSEMANTIC_OPTIONS_COORDINATE,
        nonsemantic_options_digest = digest_text(BUNDLE_NONSEMANTIC_OPTIONS_DIGEST),
        build_provenance_coordinate = BUNDLE_BUILD_PROVENANCE_COORDINATE,
        build_provenance_digest = digest_text(BUNDLE_BUILD_PROVENANCE_DIGEST),
        canonicalization_coordinate = BUNDLE_CANONICALIZATION_COORDINATE,
        canonicalization_digest = digest_text(BUNDLE_CANONICALIZATION_DIGEST),
        conformance_coordinate = BUNDLE_CONFORMANCE_COORDINATE,
        conformance_digest = digest_text(BUNDLE_CONFORMANCE_DIGEST),
        verifier_report_coordinate = BUNDLE_VERIFIER_REPORT_COORDINATE,
        verifier_report_digest = digest_text(BUNDLE_VERIFIER_REPORT_DIGEST),
        compile_explanation_coordinate = BUNDLE_COMPILE_EXPLANATION_COORDINATE,
        compile_explanation_digest = digest_text(BUNDLE_COMPILE_EXPLANATION_DIGEST),
        semantic_bundle_digest = manifest.semantic_bundle_digest,
        release_bundle_digest = manifest.release_bundle_digest,
    ))
}

fn bundle_golden_input(
    root: &Path,
    case: &BundleGoldenCase,
) -> Result<ContractBundleAssemblyInput, String> {
    let source = read_to_string(&root.join(case.source))?;
    let module = parse_module(&source).map_err(|err| format!("parse {}: {err}", case.source))?;
    let core = compile_to_core(&module, &core_golden_context())
        .map_err(|err| format!("compile {} to Core: {err:?}", case.source))?;
    Ok(ContractBundleAssemblyInput {
        core_module: core,
        core_ir_coordinate: BUNDLE_CORE_IR_COORDINATE.to_owned(),
        source_artifacts: vec![bundle_source_artifact(
            BUNDLE_SOURCE_LOGICAL_PATH,
            BUNDLE_SOURCE_COORDINATE,
            BUNDLE_SOURCE_DIGEST,
        )?],
        source_profile_semantic_facts: bundle_resource(
            BUNDLE_SOURCE_PROFILE_COORDINATE,
            BUNDLE_SOURCE_PROFILE_DIGEST,
        )?,
        target_profile: bundle_resource(
            BUNDLE_TARGET_PROFILE_COORDINATE,
            BUNDLE_TARGET_PROFILE_DIGEST,
        )?,
        target_ir: bundle_target_ir(BUNDLE_TARGET_IR_COORDINATE, BUNDLE_TARGET_IR_DIGEST)?,
        lawpacks: vec![bundle_resource(
            BUNDLE_LAWPACK_COORDINATE,
            BUNDLE_LAWPACK_DIGEST,
        )?],
        generated_artifacts: vec![bundle_resource(
            BUNDLE_GENERATED_COORDINATE,
            BUNDLE_GENERATED_DIGEST,
        )?],
        compiler: bundle_resource(BUNDLE_COMPILER_COORDINATE, BUNDLE_COMPILER_DIGEST)?,
        lowerer: bundle_resource(BUNDLE_LOWERER_COORDINATE, BUNDLE_LOWERER_DIGEST)?,
        verifier: bundle_resource(BUNDLE_VERIFIER_COORDINATE, BUNDLE_VERIFIER_DIGEST)?,
        semantic_compile_options: bundle_resource(
            BUNDLE_SEMANTIC_OPTIONS_COORDINATE,
            BUNDLE_SEMANTIC_OPTIONS_DIGEST,
        )?,
        non_semantic_compile_options: bundle_resource(
            BUNDLE_NONSEMANTIC_OPTIONS_COORDINATE,
            BUNDLE_NONSEMANTIC_OPTIONS_DIGEST,
        )?,
        build_provenance: bundle_resource(
            BUNDLE_BUILD_PROVENANCE_COORDINATE,
            BUNDLE_BUILD_PROVENANCE_DIGEST,
        )?,
        canonicalization_profile: bundle_resource(
            BUNDLE_CANONICALIZATION_COORDINATE,
            BUNDLE_CANONICALIZATION_DIGEST,
        )?,
        conformance_fixture_corpora: vec![bundle_resource(
            BUNDLE_CONFORMANCE_COORDINATE,
            BUNDLE_CONFORMANCE_DIGEST,
        )?],
        verifier_report: bundle_resource(
            BUNDLE_VERIFIER_REPORT_COORDINATE,
            BUNDLE_VERIFIER_REPORT_DIGEST,
        )?,
        compile_explanation: bundle_resource(
            BUNDLE_COMPILE_EXPLANATION_COORDINATE,
            BUNDLE_COMPILE_EXPLANATION_DIGEST,
        )?,
        assurance_evidence: Vec::new(),
    })
}

fn bundle_resource(coordinate: &str, hex: char) -> Result<DigestLockedResource, String> {
    DigestLockedResource::new(coordinate, digest_text(hex)).map_err(|err| err.to_string())
}

fn bundle_target_ir(coordinate: &str, hex: char) -> Result<SuppliedTargetIrResource, String> {
    SuppliedTargetIrResource::new(coordinate, digest_text(hex)).map_err(|err| err.to_string())
}

fn bundle_source_artifact(
    logical_path: &str,
    coordinate: &str,
    hex: char,
) -> Result<ContractBundleSourceArtifact, String> {
    ContractBundleSourceArtifact::new(logical_path, coordinate, digest_text(hex))
        .map_err(|err| err.to_string())
}

fn digest_text(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

pub(crate) fn check_golden_file(
    root: &Path,
    relative: &str,
    expected: &[u8],
) -> Result<(), String> {
    check_golden_file_with_instruction(
        root,
        relative,
        expected,
        "run the matching `cargo xtask *-goldens --write` command",
    )
}

pub(crate) fn check_golden_file_with_command(
    root: &Path,
    relative: &str,
    expected: &[u8],
    write_command: &str,
) -> Result<(), String> {
    check_golden_file_with_instruction(root, relative, expected, &format!("run `{write_command}`"))
}

fn check_golden_file_with_instruction(
    root: &Path,
    relative: &str,
    expected: &[u8],
    instruction: &str,
) -> Result<(), String> {
    let path = root.join(relative);
    let actual = fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{} does not match generated golden; {instruction}",
            path.display(),
        ))
    }
}

pub(crate) fn write_golden_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|err| format!("write {}: {err}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CliGoldenMode {
    Check,
    Write,
}

pub(crate) fn cli_goldens(root: &Path, mode: CliGoldenMode) -> Result<(), String> {
    run_cmd(root, "cargo", ["build", "-p", "edict-cli"])?;
    let cases = dirs(&root.join("fixtures/cli"))?;
    if cases.is_empty() {
        return Err("fixtures/cli contains no golden cases".into());
    }
    let binary = cli_binary_path(root)?;
    for case in &cases {
        check_or_write_cli_golden(&binary, case, mode)?;
    }
    println!(
        "cli-goldens: {} case(s) {}",
        cases.len(),
        match mode {
            CliGoldenMode::Check => "checked",
            CliGoldenMode::Write => "written",
        }
    );
    Ok(())
}

fn cli_binary_path(root: &Path) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .map_err(|err| format!("run cargo metadata: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    cli_binary_path_from_cargo_metadata(&output.stdout)
}

pub(crate) fn cli_binary_path_from_cargo_metadata(metadata: &[u8]) -> Result<PathBuf, String> {
    let metadata: serde_json::Value =
        serde_json::from_slice(metadata).map_err(|err| format!("parse cargo metadata: {err}"))?;
    let target_directory = metadata
        .get("target_directory")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "cargo metadata missing string field `target_directory`".to_owned())?;
    Ok(PathBuf::from(target_directory)
        .join("debug")
        .join(format!("edict{}", std::env::consts::EXE_SUFFIX)))
}

fn check_or_write_cli_golden(
    binary: &Path,
    case: &Path,
    mode: CliGoldenMode,
) -> Result<(), String> {
    let name = case
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("CLI golden case has no name: {}", case.display()))?;
    let request_path = case.join("request.jsonl");
    let request = fs::read(&request_path)
        .map_err(|err| format!("[{name}] read {}: {err}", request_path.display()))?;
    let output = run_cli_golden_case(binary, case, &request)?;
    let exit_code = output
        .status
        .code()
        .ok_or_else(|| format!("[{name}] edict terminated without an exit code"))?;

    match mode {
        CliGoldenMode::Check => {
            check_cli_golden_bytes(case, "expected.stdout.jsonl", &output.stdout)?;
            check_cli_golden_bytes(case, "expected.stderr.jsonl", &output.stderr)?;
            let expected_exit = read_optional_cli_exit(case)?;
            if expected_exit != exit_code {
                return Err(format!(
                    "[{name}] exit code mismatch: expected {expected_exit}, got {exit_code}; run `cargo xtask cli-goldens --write`"
                ));
            }
        }
        CliGoldenMode::Write => {
            write_optional_cli_golden(&case.join("expected.stdout.jsonl"), &output.stdout)?;
            write_optional_cli_golden(&case.join("expected.stderr.jsonl"), &output.stderr)?;
            write_optional_cli_exit(&case.join("exit"), exit_code)?;
        }
    }
    Ok(())
}

fn run_cli_golden_case(
    binary: &Path,
    case: &Path,
    request: &[u8],
) -> Result<std::process::Output, String> {
    let mut command = Command::new(binary);
    command
        .current_dir(case)
        .env_remove(CLI_MAX_STDIN_BYTES_ENV)
        .envs(read_cli_env_overrides(case)?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| format!("[{}] spawn {}: {err}", case.display(), binary.display()))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| format!("[{}] missing stdin pipe", case.display()))?
        .write_all(request)
        .map_err(|err| format!("[{}] write stdin: {err}", case.display()))?;
    child
        .wait_with_output()
        .map_err(|err| format!("[{}] collect output: {err}", case.display()))
}

fn check_cli_golden_bytes(case: &Path, file: &str, actual: &[u8]) -> Result<(), String> {
    let expected = read_optional_bytes(&case.join(file))?;
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "[{}] {file} does not match generated output; run `cargo xtask cli-goldens --write`",
        case.file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("<unnamed>")
    ))
}

fn read_optional_cli_exit(case: &Path) -> Result<i32, String> {
    let path = case.join("exit");
    match fs::read_to_string(&path) {
        Ok(text) => text
            .trim()
            .parse::<i32>()
            .map_err(|err| format!("parse {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(format!("read {}: {err}", path.display())),
    }
}

fn write_optional_cli_golden(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        remove_if_exists(path)
    } else {
        write_golden_file(path, bytes)
    }
}

fn write_optional_cli_exit(path: &Path, exit_code: i32) -> Result<(), String> {
    if exit_code == 0 {
        return remove_if_exists(path);
    }
    write_golden_file(path, format!("{exit_code}\n").as_bytes())
}

fn read_cli_env_overrides(case: &Path) -> Result<Vec<(String, String)>, String> {
    let path = case.join("env.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("read {}: {err}", path.display())),
    };
    let value = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|err| format!("parse {}: {err}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{} must be a JSON object", path.display()))?;
    let mut env = Vec::new();
    for (key, value) in object {
        let value = value
            .as_str()
            .ok_or_else(|| format!("{} entry `{key}` must be a string", path.display()))?;
        env.push((key.clone(), value.to_owned()));
    }
    env.sort();
    Ok(env)
}

fn read_optional_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("read {}: {err}", path.display())),
    }
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove {}: {err}", path.display())),
    }
}
