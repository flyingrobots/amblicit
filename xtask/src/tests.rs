use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use amblicit_provider_schema::{
    PROVIDER_CONTRACT_PACK_API_VERSION, PROVIDER_CONTRACT_PACK_COORDINATE,
    PROVIDER_CONTRACT_PACK_LICENSE,
};
use amblicit_syntax::{
    canonical_target_profile_contract_resources, parse_module, AUTHORITY_FACTS_API_VERSION,
    CORE_MODULE_DIGEST_DOMAIN, PROVIDER_LAWPACK_ARTIFACT_DOMAIN, TARGET_IR_ARTIFACT_DIGEST_DOMAIN,
    TARGET_PROFILE_API_VERSION,
};
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use wit_parser::{
    Function, FunctionKind, Interface, Package, Record, Resolve, Type, TypeDefKind, TypeId, World,
    WorldItem, WorldKey,
};

use super::contract_check::{check_links, check_topic, contract_check};
use super::goldens::{
    authority_facts_goldens, bundle_goldens, cli_binary_path_from_cargo_metadata, core_goldens,
    target_ir_goldens, target_profile_resource_goldens, AuthorityFactsGoldenMode, BundleGoldenMode,
    CoreGoldenMode, TargetIrGoldenMode, TargetProfileResourceGoldenMode,
};
use super::lawpack_goldens::{lawpack_goldens, LawpackGoldenMode};
use super::provider_contract_pack::{
    provider_contract_pack, ProviderContractPackMode, CONTRACT_PACK_CDDL, CONTRACT_PACK_MANIFEST,
};
use super::provider_dependencies::provider_runtime_dependencies;
use super::release_dates::{ReleaseDateFindingKind, Surface};
use super::release_prep::release_prep;
use super::util::{choose_diff_check_base, repo_root};

fn toml_section(document: &str, header: &str) -> String {
    let start = document.find(header).expect("section header");
    let section = &document[start..];
    let end = section
        .lines()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| line.starts_with('[').then_some(index))
        .unwrap_or_else(|| section.lines().count());
    section.lines().take(end).collect::<Vec<_>>().join("\n")
}

#[test]
fn contract_graph_is_valid() {
    contract_check(&repo_root().expect("repo root")).expect("contract graph is valid");
}

#[test]
fn bundle_digest_goldens_match_assembly() {
    bundle_goldens(&repo_root().expect("repo root"), BundleGoldenMode::Check)
        .expect("bundle digest goldens match assembly output");
}

#[test]
fn core_goldens_match_executable_encoder() {
    core_goldens(&repo_root().expect("repo root"), CoreGoldenMode::Check)
        .expect("Core goldens match executable encoder output");
}

#[test]
fn target_ir_goldens_match_executable_encoder() {
    target_ir_goldens(&repo_root().expect("repo root"), TargetIrGoldenMode::Check)
        .expect("Target IR goldens match executable encoder output");
}

#[test]
fn authority_facts_goldens_match_executable_codec() {
    authority_facts_goldens(
        &repo_root().expect("repo root"),
        AuthorityFactsGoldenMode::Check,
    )
    .expect("authority-facts goldens match executable codec output");
}

#[test]
fn lawpack_goldens_match_executable_codec() {
    lawpack_goldens(&repo_root().expect("repo root"), LawpackGoldenMode::Check)
        .expect("lawpack goldens match executable codec output");
}

#[test]
fn target_profile_resource_goldens_match_executable_contract() {
    target_profile_resource_goldens(
        &repo_root().expect("repo root"),
        TargetProfileResourceGoldenMode::Check,
    )
    .expect("target-profile resource goldens match executable contract");
}

#[test]
fn provider_contract_pack_goldens_match_executable_contract() {
    let root = repo_root().expect("repo root");
    provider_contract_pack(&root, ProviderContractPackMode::Check)
        .expect("provider contract-pack goldens match executable contract");

    let cddl = fs::read(root.join(CONTRACT_PACK_CDDL)).expect("generated CDDL");
    let manifest_bytes = fs::read(root.join(CONTRACT_PACK_MANIFEST)).expect("generated manifest");
    let manifest: Value =
        serde_json::from_slice(&manifest_bytes).expect("manifest is independently valid JSON");

    assert_provider_contract_manifest_metadata(&manifest);
    assert_provider_contract_schema(&manifest, &cddl);
    assert_provider_contract_resources(&manifest);
}

fn assert_provider_contract_manifest_metadata(manifest: &Value) {
    assert_eq!(
        manifest["apiVersion"].as_str(),
        Some(PROVIDER_CONTRACT_PACK_API_VERSION)
    );
    assert_eq!(
        manifest["coordinate"].as_str(),
        Some(PROVIDER_CONTRACT_PACK_COORDINATE)
    );
    assert_eq!(
        manifest["license"].as_str(),
        Some(PROVIDER_CONTRACT_PACK_LICENSE)
    );

    let contracts = manifest["contracts"]
        .as_array()
        .expect("manifest contracts are an array")
        .iter()
        .map(|binding| {
            (
                binding["contract"]
                    .as_str()
                    .expect("contract name is a string"),
                binding["rootRule"]
                    .as_str()
                    .expect("contract root is a string"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        contracts,
        [
            ("authority-facts", "authority-facts"),
            ("core-module", "core-module"),
            ("lawpack-adapter", "lawpack-adapter"),
            ("lawpack-exports", "lawpack-exports"),
            ("lawpack-manifest", "lawpack-manifest"),
            ("lowering-requirements", "lowering-requirements"),
            ("result-projection", "result-projection"),
            ("source-closure", "source-closure"),
            ("target-ir-artifact", "target-ir-artifact"),
            ("target-profile-intrinsics", "intrinsics-document"),
            ("target-profile-manifest", "target-profile-manifest"),
            (
                "target-profile-operation-profiles",
                "operation-profiles-document"
            ),
        ]
    );

    let domains = manifest["domains"]
        .as_array()
        .expect("manifest domains are an array")
        .iter()
        .map(|binding| {
            (
                binding["domain"]
                    .as_str()
                    .expect("artifact domain is a string"),
                binding["rootRule"]
                    .as_str()
                    .expect("domain root is a string"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        domains,
        [
            (AUTHORITY_FACTS_API_VERSION, "authority-facts"),
            (CORE_MODULE_DIGEST_DOMAIN, "core-module"),
            (PROVIDER_LAWPACK_ARTIFACT_DOMAIN, "lawpack-manifest"),
            ("edict.lowering-requirements/v1", "lowering-requirements"),
            (
                amblicit_syntax::RESULT_PROJECTION_DIGEST_DOMAIN,
                "result-projection"
            ),
            ("edict.source-closure/v1", "source-closure"),
            (TARGET_IR_ARTIFACT_DIGEST_DOMAIN, "target-ir-artifact"),
            (TARGET_PROFILE_API_VERSION, "target-profile-manifest"),
        ]
    );
}

fn assert_provider_contract_schema(manifest: &Value, cddl: &[u8]) {
    let schema = json_object(manifest, "schema");
    assert_eq!(
        decode_lower_hex(
            schema["bytesHex"]
                .as_str()
                .expect("schema bytesHex is a string")
        ),
        cddl,
        "manifest embeds the exact generated CDDL bytes"
    );
    assert_eq!(
        schema["rawSha256"]
            .as_str()
            .expect("schema rawSha256 is a string"),
        raw_sha256_hex(cddl)
    );
}

fn assert_provider_contract_resources(manifest: &Value) {
    let resources = manifest["resources"]
        .as_array()
        .expect("manifest resources are an array");
    let expected = canonical_target_profile_contract_resources();
    assert_eq!(resources.len(), expected.len());
    for (resource, expected) in resources.iter().zip(expected) {
        let resource = resource.as_object().expect("resource is an object");
        assert_eq!(
            resource["coordinate"].as_str(),
            Some(expected.coordinate.as_str())
        );
        assert_eq!(
            decode_lower_hex(
                resource["canonicalBytesHex"]
                    .as_str()
                    .expect("resource canonicalBytesHex is a string")
            ),
            expected.canonical_bytes
        );
        assert_eq!(
            resource["rawSha256"]
                .as_str()
                .expect("resource rawSha256 is a string"),
            raw_sha256_hex(&expected.canonical_bytes)
        );
        assert_eq!(
            resource["domainFramedDigest"].as_str(),
            Some(expected.digest.as_str())
        );
        let provenance = resource["provenance"]
            .as_object()
            .expect("resource provenance is an object");
        assert_eq!(
            provenance["repository"].as_str(),
            Some(expected.provenance.repository.as_str())
        );
        assert_eq!(
            provenance["sourcePath"].as_str(),
            Some(expected.provenance.source_path.as_str())
        );
    }
}

fn decode_lower_hex(value: &str) -> Vec<u8> {
    assert!(value.len().is_multiple_of(2), "hex has complete bytes");
    assert!(
        value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "hex is lowercase ASCII"
    );
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("hex pair is UTF-8");
            u8::from_str_radix(pair, 16).expect("hex pair decodes")
        })
        .collect()
}

fn raw_sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn provider_contract_pack_check_rejects_drift_without_rewriting() {
    let source_root = repo_root().expect("repo root");
    let root = temp_root("provider-contract-pack-drift");
    for relative in [
        "docs/abi/edict-common.cddl",
        "docs/abi/edict-core.cddl",
        "docs/abi/edict-lawpack.cddl",
        "docs/abi/edict-lawpack-adapter.cddl",
        "docs/abi/edict-target-profile.cddl",
        "docs/abi/edict-authority-facts.cddl",
        "docs/abi/edict-result-projection.cddl",
        "docs/abi/edict-target-ir.cddl",
        "docs/abi/edict-source-closure.cddl",
    ] {
        let destination = root.join(relative);
        fs::create_dir_all(destination.parent().expect("contract source parent"))
            .expect("create contract source parent");
        fs::copy(source_root.join(relative), &destination).expect("copy contract source");
    }

    provider_contract_pack(&root, ProviderContractPackMode::Write)
        .expect("write isolated provider contract pack");
    let cddl = fs::read(root.join(CONTRACT_PACK_CDDL)).expect("generated CDDL");
    let manifest = fs::read(root.join(CONTRACT_PACK_MANIFEST)).expect("generated manifest");

    let mut drifted_cddl = cddl.clone();
    drifted_cddl.extend_from_slice(b"\n");
    fs::write(root.join(CONTRACT_PACK_CDDL), &drifted_cddl).expect("introduce CDDL drift");

    let error = provider_contract_pack(&root, ProviderContractPackMode::Check)
        .expect_err("check mode rejects CDDL drift");
    assert!(
        error.contains("does not match generated golden"),
        "unexpected CDDL drift error: {error}"
    );
    assert!(
        error.contains("cargo xtask provider-contract-pack --write"),
        "drift error must identify the exact regeneration command: {error}"
    );
    assert_eq!(
        fs::read(root.join(CONTRACT_PACK_CDDL)).expect("CDDL remains readable"),
        drifted_cddl,
        "check mode must not rewrite the drifted CDDL fixture"
    );
    assert_eq!(
        fs::read(root.join(CONTRACT_PACK_MANIFEST)).expect("manifest remains readable"),
        manifest,
        "check mode must not rewrite the matching manifest fixture"
    );

    fs::write(root.join(CONTRACT_PACK_CDDL), &cddl).expect("restore generated CDDL");
    let mut drifted_manifest = manifest;
    drifted_manifest.extend_from_slice(b"\n");
    fs::write(root.join(CONTRACT_PACK_MANIFEST), &drifted_manifest)
        .expect("introduce manifest drift");

    let error = provider_contract_pack(&root, ProviderContractPackMode::Check)
        .expect_err("check mode rejects manifest drift");
    assert!(
        error.contains("does not match generated golden"),
        "unexpected drift error: {error}"
    );
    assert_eq!(
        fs::read(root.join(CONTRACT_PACK_CDDL)).expect("CDDL remains readable"),
        cddl,
        "check mode must not rewrite the matching CDDL fixture"
    );
    assert_eq!(
        fs::read(root.join(CONTRACT_PACK_MANIFEST)).expect("manifest remains readable"),
        drifted_manifest,
        "check mode must not rewrite the drifted manifest"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn contract_graph_rejects_unknown_registry_source_ids() {
    let root = temp_root("unknown-registry-source");
    let topic = root.join("docs/topics/example");
    fs::create_dir_all(&topic).expect("topic directory");
    fs::write(topic.join("README.md"), "# Example\n\n[EXAMPLE-REQ-001]\n").expect("chapter");
    fs::write(
            topic.join("test-plan.md"),
            "# Example Test Plan\n\n\
             | ID | Status | Requirement | Source |\n\
             | --- | --- | --- | --- |\n\
             | EXAMPLE-REQ-001 | implemented | Example requirement. | EDICT-NOT-A-REAL-ID |\n\n\
             | ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n\
             | --- | --- | --- | --- | --- | --- | --- | --- |\n\
             | EXAMPLE-TP-001 | implemented | Golden path | EXAMPLE-REQ-001 | exact state | evidence_test | - | fixture-free |\n",
        )
        .expect("test plan");

    let tests = BTreeSet::from(["evidence_test".to_owned()]);
    let err = check_topic(&root, &topic, &tests, "EDICT-KNOWN-ID")
        .expect_err("unknown registry Source ID must fail");
    assert!(
        err.contains("unknown registry source ID `EDICT-NOT-A-REAL-ID`"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn contract_graph_rejects_truncated_registry_bracket_ids() {
    let root = temp_root("truncated-registry-bracket-id");
    let topic = root.join("docs/topics/example");
    fs::create_dir_all(&topic).expect("topic directory");
    fs::write(
        topic.join("README.md"),
        "# Example\n\n[EDICT-LANG-RECORD]\n",
    )
    .expect("chapter");
    fs::write(
            topic.join("test-plan.md"),
            "# Example Test Plan\n\n\
             | ID | Status | Requirement | Source |\n\
             | --- | --- | --- | --- |\n\
             | EXAMPLE-REQ-001 | implemented | Example requirement. | docs/REQUIREMENTS.md |\n\n\
             | ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n\
             | --- | --- | --- | --- | --- | --- | --- | --- |\n\
             | EXAMPLE-TP-001 | implemented | Golden path | EXAMPLE-REQ-001 | exact state | evidence_test | - | fixture-free |\n",
        )
        .expect("test plan");
    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::write(
        root.join("docs/REQUIREMENTS.md"),
        "| ID | Requirement |\n\
             | --- | --- |\n\
             | EDICT-LANG-RECORD-SHORTHAND-001 | exact longer ID |\n",
    )
    .expect("requirements");

    let tests = BTreeSet::from(["evidence_test".to_owned()]);
    let err = check_topic(&root, &topic, &tests, "EDICT-LANG-RECORD-SHORTHAND-001")
        .expect_err("truncated registry ID must fail");
    assert!(
        err.contains("unknown registry ID `EDICT-LANG-RECORD`"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn contract_graph_requires_implemented_evidence_for_implemented_requirements() {
    let root = temp_root("implemented-requirement-without-evidence");
    let topic = root.join("docs/topics/example");
    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::create_dir_all(&topic).expect("topic directory");
    fs::write(root.join("docs/REQUIREMENTS.md"), "# Requirements\n").expect("requirements");
    fs::write(topic.join("README.md"), "# Example\n\n[EXAMPLE-REQ-001]\n").expect("chapter");
    fs::write(
            topic.join("test-plan.md"),
            "# Example Test Plan\n\n\
             | ID | Status | Requirement | Source |\n\
             | --- | --- | --- | --- |\n\
             | EXAMPLE-REQ-001 | implemented | Example requirement. | docs/REQUIREMENTS.md |\n\n\
             | ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n\
             | --- | --- | --- | --- | --- | --- | --- | --- |\n\
             | EXAMPLE-TP-001 | planned | Golden path | EXAMPLE-REQ-001 | exact state | - | - | planned only |\n",
        )
        .expect("test plan");

    let tests = BTreeSet::new();
    let err = check_topic(&root, &topic, &tests, "EDICT-KNOWN-ID")
        .expect_err("implemented requirement without implemented case must fail");
    assert!(
        err.contains("implemented requirement EXAMPLE-REQ-001 has no implemented case"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn contract_graph_accepts_policy_rows_without_rust_evidence() {
    let root = temp_root("policy-rows-without-rust-evidence");
    let topic = root.join("docs/topics/example");
    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::create_dir_all(&topic).expect("topic directory");
    fs::write(root.join("docs/REQUIREMENTS.md"), "# Requirements\n").expect("requirements");
    fs::write(topic.join("README.md"), "# Example\n\n[EXAMPLE-REQ-001]\n").expect("chapter");
    fs::write(
            topic.join("test-plan.md"),
            "# Example Test Plan\n\n\
             | ID | Status | Requirement | Source |\n\
             | --- | --- | --- | --- |\n\
             | EXAMPLE-REQ-001 | policy | Example policy. | docs/REQUIREMENTS.md |\n\n\
             | ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n\
             | --- | --- | --- | --- | --- | --- | --- | --- |\n\
             | EXAMPLE-TP-001 | policy | Review policy | EXAMPLE-REQ-001 | human review records the policy decision | - | - | no executable software behavior |\n",
        )
        .expect("test plan");

    let tests = BTreeSet::new();
    check_topic(&root, &topic, &tests, "").expect("policy rows require no Rust evidence");

    fs::remove_dir_all(root).ok();
}

#[test]
fn obstruction_taxonomy_boundary_uses_layer_authority_terms() {
    let root = repo_root().expect("repo root");
    let design = fs::read_to_string(root.join("docs/design/obstruction-strands-v0.md"))
        .expect("obstruction strand design note");
    let test_plan = fs::read_to_string(root.join("docs/topics/obstruction-strands/test-plan.md"))
        .expect("obstruction strand test plan");

    assert!(
        design.contains(
            "| Not-admitted scheduler counterfactual | Candidate was considered or available but left unselected before execution. | Scheduler/runtime authority |",
        ),
        "not-admitted taxonomy must describe an unselected candidate, not a selected-but-unrun candidate"
    );
    assert!(
        design.contains(
            "| Admitted obstructed strand | Accepted/evaluated attempt continued into obstruction outcome. | Runtime receipt authority, grounded in Edict-produced artifact semantics |",
        ),
        "admitted obstruction taxonomy must remain owned by runtime receipt authority"
    );
    assert!(
        design.contains(
            "| Hard rejection | Artifact, input, profile, validation, or runtime refused before admitted execution. | Validator or target/runtime acceptor |",
        ),
        "hard rejection taxonomy must remain distinct from scheduler counterfactuals"
    );
    assert!(
        test_plan.contains("| OBSTRAND-REQ-009 | implemented |"),
        "taxonomy boundary requirement must use implemented contract evidence"
    );
    assert!(
        !test_plan.contains("| OBSTRAND-REQ-009 | policy |")
            && !test_plan.contains("| OBSTRAND-TP-012 | policy |"),
        "taxonomy boundary rows must not use policy status"
    );
}

#[test]
fn contract_graph_rejects_unknown_requirement_status() {
    let root = temp_root("unknown-requirement-status");
    let topic = root.join("docs/topics/example");
    fs::create_dir_all(root.join("docs")).expect("docs directory");
    fs::create_dir_all(&topic).expect("topic directory");
    fs::write(root.join("docs/REQUIREMENTS.md"), "# Requirements\n").expect("requirements");
    fs::write(topic.join("README.md"), "# Example\n\n[EXAMPLE-REQ-001]\n").expect("chapter");
    fs::write(
            topic.join("test-plan.md"),
            "# Example Test Plan\n\n\
             | ID | Status | Requirement | Source |\n\
             | --- | --- | --- | --- |\n\
             | EXAMPLE-REQ-001 | maybe | Example requirement. | docs/REQUIREMENTS.md |\n\n\
             | ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n\
             | --- | --- | --- | --- | --- | --- | --- | --- |\n\
             | EXAMPLE-TP-001 | planned | Planned path | EXAMPLE-REQ-001 | exact state | - | - | planned only |\n",
        )
        .expect("test plan");

    let tests = BTreeSet::new();
    let err =
        check_topic(&root, &topic, &tests, "").expect_err("unknown requirement status must reject");
    assert!(
        err.contains("EXAMPLE-REQ-001 has invalid status `maybe`"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(root).ok();
}

#[test]
fn link_check_rejects_local_links_outside_repo() {
    let root = temp_root("repo-escaping-link");
    let outside = temp_root("repo-escaping-link-outside").join("outside.md");
    fs::write(&outside, "# Outside\n").expect("outside file");
    let doc = root.join("docs/topics/example/README.md");
    fs::create_dir_all(doc.parent().expect("doc parent")).expect("doc directory");
    fs::write(&doc, format!("# Example\n\n[bad]({})\n", outside.display())).expect("chapter");

    let err = check_links(&root, &doc).expect_err("absolute local links must reject");
    assert!(
        err.contains("absolute local link"),
        "unexpected error: {err}"
    );

    fs::remove_dir_all(root).ok();
    if let Some(parent) = outside.parent() {
        fs::remove_dir_all(parent).ok();
    }
}

#[test]
fn diff_check_base_selection_falls_back_without_origin_main() {
    let refs = BTreeSet::from(["HEAD^"]);
    let base = choose_diff_check_base(|candidate| Ok(refs.contains(candidate)))
        .expect("fallback diff base");
    assert_eq!(base, "HEAD^");
}

#[test]
fn diff_check_base_selection_prefers_origin_main() {
    let refs = BTreeSet::from(["origin/main", "main", "HEAD^"]);
    let base = choose_diff_check_base(|candidate| Ok(refs.contains(candidate)))
        .expect("preferred diff base");
    assert_eq!(base, "origin/main");
}

fn workflow_block<'a>(workflow: &'a str, start: &str, end: &str) -> &'a str {
    workflow
        .split(start)
        .nth(1)
        .and_then(|tail| tail.split(end).next())
        .unwrap_or_else(|| panic!("workflow block missing `{start}`"))
}

fn milestone_lookup_consumes_complete_stream(block: &str) -> bool {
    block.contains(
        "gh api --paginate \"repos/${GITHUB_REPOSITORY}/milestones?state=all&per_page=100\"",
    ) && block.contains("jq -s '.[0] // empty'")
        && !block.contains("head -n 1")
}

fn tree_sitter_rule_declared(grammar: &str, rule: &str) -> bool {
    grammar.contains(&format!("{rule}: $ =>"))
}

fn tree_sitter_literal_declared(grammar: &str, literal: &str) -> bool {
    grammar.contains(&format!("'{literal}'")) || grammar.contains(&format!("\"{literal}\""))
}

fn quoted_query_atoms(line: &str) -> Vec<String> {
    let mut atoms = Vec::new();
    let mut chars = line.char_indices();
    while let Some((start, ch)) = chars.next() {
        if ch != '"' {
            continue;
        }
        let atom_start = start + ch.len_utf8();
        for (end, end_ch) in chars.by_ref() {
            if end_ch == '"' {
                atoms.push(line[atom_start..end].to_owned());
                break;
            }
        }
    }
    atoms
}

fn tree_sitter_query_atoms_for_capture(query: &str, capture: &str) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    let mut list_atoms = Vec::new();
    let mut in_list = false;

    for line in query.lines() {
        let trimmed = line.trim();
        if trimmed == "[" {
            in_list = true;
            list_atoms.clear();
            continue;
        }

        if in_list {
            if trimmed.starts_with(']') {
                if trimmed.contains(capture) {
                    atoms.extend(list_atoms.drain(..));
                }
                in_list = false;
                list_atoms.clear();
            } else {
                list_atoms.extend(quoted_query_atoms(trimmed));
            }
            continue;
        }

        if trimmed.contains("#any-of?") && trimmed.contains(capture) {
            atoms.extend(quoted_query_atoms(trimmed));
        }
    }

    atoms
}

fn textmate_grammar() -> Value {
    let root = repo_root().expect("repo root");
    let grammar = fs::read_to_string(root.join("grammars/textmate/edict.tmLanguage.json"))
        .expect("TextMate grammar");
    serde_json::from_str(&grammar).expect("TextMate grammar must be valid JSON")
}

fn json_str<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("JSON object missing string field `{key}`"))
}

fn textmate_repository_match<'a>(grammar: &'a Value, entry: &str) -> &'a str {
    grammar
        .get("repository")
        .and_then(|repository| repository.get(entry))
        .and_then(|entry| entry.get("match"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("TextMate repository entry `{entry}` missing match regex"))
}

fn textmate_repository_patterns<'a>(grammar: &'a Value, entry: &str) -> &'a Vec<Value> {
    grammar
        .get("repository")
        .and_then(|repository| repository.get(entry))
        .and_then(|entry| entry.get("patterns"))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("TextMate repository entry `{entry}` missing patterns"))
}

fn textmate_includes(grammar: &Value) -> BTreeSet<String> {
    grammar
        .get("patterns")
        .and_then(Value::as_array)
        .expect("TextMate grammar missing top-level patterns")
        .iter()
        .map(|pattern| json_str(pattern, "include").to_owned())
        .collect()
}

fn textmate_regex_matches_literal(pattern: &str, literal: &str) -> bool {
    let anchored = format!("^(?:{pattern})$");
    Regex::new(&anchored)
        .unwrap_or_else(|err| panic!("TextMate regex `{pattern}` must compile: {err}"))
        .is_match(literal)
}

fn textmate_regex_spans(pattern: &str, source: &str) -> Vec<(usize, usize)> {
    Regex::new(pattern)
        .unwrap_or_else(|err| panic!("TextMate regex `{pattern}` must compile: {err}"))
        .find_iter(source)
        .map(|match_| (match_.start(), match_.end()))
        .collect()
}

fn vscode_extension_manifest() -> Value {
    let root = repo_root().expect("repo root");
    let manifest = fs::read_to_string(root.join("editors/vscode/package.json"))
        .expect("VS Code extension package manifest");
    serde_json::from_str(&manifest).expect("VS Code extension manifest must be valid JSON")
}

fn read_json_file(path: &Path) -> Value {
    let file = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("JSON file `{}` must be readable: {err}", path.display()));
    serde_json::from_str(&file)
        .unwrap_or_else(|err| panic!("JSON file `{}` must parse: {err}", path.display()))
}

fn json_array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("JSON object missing array field `{key}`"))
}

fn json_object<'a>(value: &'a Value, key: &str) -> &'a serde_json::Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("JSON object missing object field `{key}`"))
}

fn json_string_array_contains(value: &Value, key: &str, expected: &str) -> bool {
    json_array(value, key).iter().any(|item| item == expected)
}

#[derive(Debug)]
struct TreeSitterCorpusExample {
    title: String,
    source: String,
    expected_tree: String,
}

fn parse_tree_sitter_corpus(corpus: &str) -> Vec<TreeSitterCorpusExample> {
    let lines = corpus.lines().collect::<Vec<_>>();
    let mut examples = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        while index < lines.len() && !is_tree_sitter_delimiter(lines[index]) {
            index += 1;
        }
        if index == lines.len() {
            break;
        }

        index += 1;
        assert!(index < lines.len(), "Tree-sitter corpus missing title");
        let title = lines[index].trim().to_owned();
        index += 1;
        assert!(
            index < lines.len() && is_tree_sitter_delimiter(lines[index]),
            "Tree-sitter corpus example `{title}` missing title delimiter"
        );
        index += 1;
        if index < lines.len() && lines[index].trim().is_empty() {
            index += 1;
        }

        let mut source_lines = Vec::new();
        while index < lines.len() && lines[index].trim() != "---" {
            source_lines.push(lines[index]);
            index += 1;
        }
        assert!(
            index < lines.len(),
            "Tree-sitter corpus example `{title}` missing expected-tree delimiter"
        );
        index += 1;

        let mut expected_tree_lines = Vec::new();
        while index < lines.len() && !is_tree_sitter_delimiter(lines[index]) {
            expected_tree_lines.push(lines[index]);
            index += 1;
        }

        examples.push(TreeSitterCorpusExample {
            title,
            source: source_lines.join("\n").trim().to_owned(),
            expected_tree: expected_tree_lines.join("\n").trim().to_owned(),
        });
    }

    examples
}

fn is_tree_sitter_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '=')
}

#[test]
fn tree_sitter_grammar_declares_current_editor_contract() {
    let root = repo_root().expect("repo root");
    let grammar = fs::read_to_string(root.join("grammars/tree-sitter-edict/grammar.js"))
        .expect("Tree-sitter grammar source");
    let highlights =
        fs::read_to_string(root.join("grammars/tree-sitter-edict/queries/highlights.scm"))
            .expect("Tree-sitter highlight query");

    for rule in [
        "source_file",
        "package_declaration",
        "use_declaration",
        "type_declaration",
        "enum_declaration",
        "variant_type",
        "intent_declaration",
        "intent_clause",
        "block",
        "let_statement",
        "return_statement",
        "require_statement",
        "guarantee_statement",
        "assert_statement",
        "if_statement",
        "for_statement",
        "yield_block",
        "match_expression",
        "call_expression",
        "call_type_arguments",
        "call_argument_list",
        "record_literal",
        "type_reference",
        "comment",
        "string",
        "number",
    ] {
        assert!(
            tree_sitter_rule_declared(&grammar, rule),
            "Tree-sitter grammar must declare `{rule}`"
        );
    }

    for keyword in [
        "package",
        "use",
        "type",
        "enum",
        "variant",
        "intent",
        "returns",
        "profile",
        "implements",
        "basis",
        "footprint",
        "budget",
        "where",
        "let",
        "return",
        "require",
        "guarantee",
        "assert",
        "if",
        "then",
        "else",
        "for",
        "in",
        "bounded",
        "yield",
        "match",
        "shape",
        "lawpack",
        "target",
        "core",
        "as",
        "digest",
        "true",
        "false",
    ] {
        assert!(
            tree_sitter_literal_declared(&grammar, keyword),
            "Tree-sitter grammar must recognize keyword `{keyword}`"
        );
    }

    for capture in [
        "@comment",
        "@keyword",
        "@string",
        "@number",
        "@operator",
        "@punctuation",
        "@type",
        "@function",
    ] {
        assert!(
            highlights.contains(capture),
            "Tree-sitter highlight query must emit `{capture}`"
        );
    }
}

#[test]
fn tree_sitter_query_covers_public_keyword_roles() {
    let root = repo_root().expect("repo root");
    let highlights =
        fs::read_to_string(root.join("grammars/tree-sitter-edict/queries/highlights.scm"))
            .expect("Tree-sitter highlight query");
    let source = r"
package examples.keywords@1;
use capability caps.auth@1 as caps;
const demo = 1;
fn run() {}
";
    let public_keywords = amblicit_syntax::highlight_source(source)
        .expect("source lexes for highlighting")
        .into_iter()
        .filter(|token| token.role == amblicit_syntax::HighlightRole::Keyword)
        .map(|token| token.lexeme(source).to_owned())
        .collect::<BTreeSet<_>>();
    let query_keywords = tree_sitter_query_atoms_for_capture(&highlights, "@keyword");

    for keyword in public_keywords {
        assert!(
            query_keywords.contains(&keyword),
            "Tree-sitter highlight query must capture public keyword `{keyword}`"
        );
    }
}

#[test]
fn tree_sitter_query_operator_and_punctuation_roles_match_public_highlighter() {
    let root = repo_root().expect("repo root");
    let highlights =
        fs::read_to_string(root.join("grammars/tree-sitter-edict/queries/highlights.scm"))
            .expect("Tree-sitter highlight query");
    let source = "= == != < <= > >= + - * / % ! && || => :: ... ; : , . @ ( ) { }";
    let public_roles = amblicit_syntax::highlight_source(source)
        .expect("operator and punctuation source lexes")
        .into_iter()
        .map(|token| (token.lexeme(source).to_owned(), token.role))
        .collect::<std::collections::BTreeMap<_, _>>();
    let query_operators = tree_sitter_query_atoms_for_capture(&highlights, "@operator");
    let query_punctuation = tree_sitter_query_atoms_for_capture(&highlights, "@punctuation");

    if let Some(duplicate) = query_operators.intersection(&query_punctuation).next() {
        panic!("Tree-sitter query assigns `{duplicate}` to both operator and punctuation");
    }

    for operator in query_operators {
        if let Some(role) = public_roles.get(&operator) {
            assert_eq!(
                    *role,
                    amblicit_syntax::HighlightRole::Operator,
                    "Tree-sitter query marks `{operator}` as operator but public highlighter emits {role:?}"
                );
        }
    }

    for punctuation in query_punctuation {
        if let Some(role) = public_roles.get(&punctuation) {
            assert_eq!(
                    *role,
                    amblicit_syntax::HighlightRole::Punctuation,
                    "Tree-sitter query marks `{punctuation}` as punctuation but public highlighter emits {role:?}"
                );
        }
    }
}

#[test]
fn textmate_grammar_declares_current_editor_contract() {
    let grammar = textmate_grammar();
    assert_eq!(json_str(&grammar, "name"), "Edict");
    assert_eq!(json_str(&grammar, "scopeName"), "source.edict");
    let file_types = grammar
        .get("fileTypes")
        .and_then(Value::as_array)
        .expect("TextMate grammar missing file types");
    assert!(
        file_types.iter().any(|file_type| file_type == "edict"),
        "TextMate grammar must register .edict files"
    );

    let includes = textmate_includes(&grammar);
    for include in [
        "#comments",
        "#strings",
        "#numbers",
        "#keywords",
        "#types",
        "#operators",
        "#punctuation",
        "#identifiers",
    ] {
        assert!(
            includes.contains(include),
            "TextMate grammar must include repository pattern `{include}`"
        );
    }

    let comments = textmate_repository_patterns(&grammar, "comments");
    assert!(
        comments
            .iter()
            .any(|pattern| json_str(pattern, "name") == "comment.line.double-slash.edict"),
        "TextMate grammar must scope line comments"
    );
    assert!(
        comments
            .iter()
            .any(|pattern| json_str(pattern, "name") == "comment.block.edict"),
        "TextMate grammar must scope block comments"
    );
}

#[test]
fn textmate_grammar_covers_public_highlight_roles() {
    let grammar = textmate_grammar();
    let keyword_regex = textmate_repository_match(&grammar, "keywords");
    let operator_regex = textmate_repository_match(&grammar, "operators");
    let punctuation_regex = textmate_repository_match(&grammar, "punctuation");
    let type_regex = textmate_repository_match(&grammar, "types");
    let identifier_regex = textmate_repository_match(&grammar, "identifiers");
    let source = "package use type enum variant intent returns profile implements basis \
            footprint budget where let return request require guarantee assert if then else for in \
            bounded yield match shape lawpack target core capability as digest fn const true \
            false HelloInput input = == != < <= > >= + - * / % ! && || => -> :: ... ; : , . @ \
            ( ) { } [ ] \"text\" 123";
    let highlights = amblicit_syntax::highlight_source(source).expect("highlight role source");

    for token in highlights {
        let lexeme = token.lexeme(source).trim();
        if lexeme.is_empty() {
            continue;
        }
        match token.role {
            amblicit_syntax::HighlightRole::Keyword => assert!(
                textmate_regex_matches_literal(keyword_regex, lexeme),
                "TextMate keyword regex must cover `{lexeme}`"
            ),
            amblicit_syntax::HighlightRole::Operator => assert!(
                textmate_regex_matches_literal(operator_regex, lexeme),
                "TextMate operator regex must cover `{lexeme}`"
            ),
            amblicit_syntax::HighlightRole::Punctuation => assert!(
                textmate_regex_matches_literal(punctuation_regex, lexeme),
                "TextMate punctuation regex must cover `{lexeme}`"
            ),
            amblicit_syntax::HighlightRole::TypeIdentifier => assert!(
                textmate_regex_matches_literal(type_regex, lexeme),
                "TextMate type regex must cover `{lexeme}`"
            ),
            amblicit_syntax::HighlightRole::Identifier => assert!(
                textmate_regex_matches_literal(identifier_regex, lexeme),
                "TextMate identifier regex must cover `{lexeme}`"
            ),
            amblicit_syntax::HighlightRole::Comment
            | amblicit_syntax::HighlightRole::Number
            | amblicit_syntax::HighlightRole::String => {}
        }
    }
}

#[test]
fn textmate_grammar_scopes_public_number_spans_in_version_labels() {
    let grammar = textmate_grammar();
    let number_regex = textmate_repository_match(&grammar, "numbers");
    let source = "package examples.tooling@1_beta;";
    let public_number = amblicit_syntax::highlight_source(source)
        .expect("version label source lexes")
        .into_iter()
        .find(|token| {
            token.role == amblicit_syntax::HighlightRole::Number && token.lexeme(source) == "1_"
        })
        .expect("public highlighter emits leading numeric version-label span");
    let number_spans = textmate_regex_spans(number_regex, source);

    assert!(
        number_spans.iter().any(|(start, end)| {
            *start == public_number.span.start && *end == public_number.span.end
        }),
        "TextMate number regex must exactly scope public version-label number span `{}`",
        public_number.lexeme(source)
    );
}

#[test]
fn vscode_extension_declares_textmate_language_contract() {
    let root = repo_root().expect("repo root");
    let manifest = vscode_extension_manifest();
    assert_eq!(json_str(&manifest, "name"), "edict-vscode");
    assert_eq!(json_str(&manifest, "publisher"), "flyingrobots");

    let engines = json_object(&manifest, "engines");
    assert_eq!(
        engines.get("vscode").and_then(Value::as_str),
        Some("^1.85.0")
    );
    assert!(
        manifest.get("main").is_none()
            && manifest.get("browser").is_none()
            && manifest.get("activationEvents").is_none(),
        "thin grammar extension must not declare a runtime activation surface"
    );

    let contributes = manifest
        .get("contributes")
        .expect("VS Code extension manifest must contribute editor behavior");
    let languages = json_array(contributes, "languages");
    let language = languages
        .iter()
        .find(|language| json_str(language, "id") == "edict")
        .expect("VS Code extension must register the Edict language");
    assert!(
        json_string_array_contains(language, "extensions", ".edict"),
        "VS Code extension must register .edict files"
    );
    assert!(
        json_string_array_contains(language, "aliases", "Edict"),
        "VS Code extension must expose the Edict language alias"
    );
    assert_eq!(
        json_str(language, "configuration"),
        "./language-configuration.json"
    );

    let grammars = json_array(contributes, "grammars");
    let grammar = grammars
        .iter()
        .find(|grammar| json_str(grammar, "language") == "edict")
        .expect("VS Code extension must map the Edict language to a grammar");
    assert_eq!(json_str(grammar, "scopeName"), "source.edict");
    assert_eq!(
        json_str(grammar, "path"),
        "./syntaxes/edict.tmLanguage.json"
    );

    let extension_grammar = read_json_file(
        &root
            .join("editors/vscode")
            .join(json_str(grammar, "path").trim_start_matches("./")),
    );
    assert_eq!(
        extension_grammar,
        textmate_grammar(),
        "VS Code extension grammar must match the canonical TextMate grammar"
    );
}

#[test]
fn vscode_language_configuration_matches_lexer_boundaries() {
    let root = repo_root().expect("repo root");
    let manifest = vscode_extension_manifest();
    let contributes = manifest
        .get("contributes")
        .expect("VS Code extension manifest must contribute editor behavior");
    let language = json_array(contributes, "languages")
        .iter()
        .find(|language| json_str(language, "id") == "edict")
        .expect("VS Code extension must register the Edict language");
    let config = read_json_file(
        &root
            .join("editors/vscode")
            .join(json_str(language, "configuration").trim_start_matches("./")),
    );

    let comments = config
        .get("comments")
        .expect("language configuration must declare comments");
    assert_eq!(json_str(comments, "lineComment"), "//");
    assert_eq!(
        json_array(comments, "blockComment")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>(),
        vec!["/*", "*/"]
    );

    let brackets = json_array(&config, "brackets")
        .iter()
        .map(|pair| {
            pair.as_array()
                .expect("bracket pair must be an array")
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for pair in [vec!["{", "}"], vec!["[", "]"], vec!["(", ")"]] {
        assert!(
            brackets.iter().any(|bracket| bracket == &pair),
            "language configuration must expose bracket pair {pair:?}"
        );
    }
}

#[test]
fn tree_sitter_corpus_examples_match_reference_parser() {
    let root = repo_root().expect("repo root");
    let corpus =
        fs::read_to_string(root.join("grammars/tree-sitter-edict/test/corpus/current-subset.txt"))
            .expect("Tree-sitter corpus");
    let examples = parse_tree_sitter_corpus(&corpus);
    assert!(
        !examples.is_empty(),
        "Tree-sitter corpus must include accepted Edict examples"
    );

    let mut titles = BTreeSet::new();
    for example in &examples {
        assert!(
            titles.insert(example.title.as_str()),
            "duplicate Tree-sitter corpus example `{}`",
            example.title
        );
        assert!(
            !example.expected_tree.trim().is_empty(),
            "Tree-sitter corpus example `{}` must include an expected tree",
            example.title
        );
        parse_module(&example.source).unwrap_or_else(|err| {
            panic!(
                "Tree-sitter corpus example `{}` must parse with the reference parser: {err}",
                example.title
            );
        });
    }

    for title in [
        "bounded hello",
        "conditional blob",
        "spaced type call",
        "read greeting",
        "uppercase bare identifiers",
        "color match",
    ] {
        assert!(
            titles.contains(title),
            "Tree-sitter corpus must cover `{title}`"
        );
    }
}

#[test]
fn release_workflow_publishes_only_main_reachable_tags() {
    let root = repo_root().expect("repo root");
    let workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).expect("workflow");
    for needle in [
        "tags:",
        "\"v*\"",
        "workflow_dispatch:",
        "contents: write",
        "git merge-base --is-ancestor",
        "origin/main",
        "docs/releases/${TAG}.md",
        "release create",
        "--verify-tag",
        "--prerelease",
        "prerelease=true",
    ] {
        assert!(
            workflow.contains(needle),
            "release workflow missing expected guard/action: {needle}"
        );
    }
    assert!(
        !workflow.contains("cargo publish"),
        "release workflow must not publish crates"
    );
    assert!(
        !workflow.contains("git fetch --force"),
        "release workflow must not force-fetch refs"
    );
}

#[test]
fn release_workflow_supports_dispatch_and_milestone_closure() {
    let root = repo_root().expect("repo root");
    let workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).expect("workflow");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for needle in [
        "workflow_dispatch:",
        "RELEASE_TAG:",
        "inputs.tag",
        "issues: write",
        "milestones?state=all",
        "OPEN_ISSUES",
        "gh release view",
        "Close release milestone",
        "--method PATCH",
        "-f state=closed",
    ] {
        assert!(
            workflow.contains(needle),
            "release workflow missing dispatch/milestone guard: {needle}"
        );
    }
    assert!(
        policy.contains("publication_before_milestone_closure = true"),
        "release policy must require publication before milestone closure"
    );
    let publish_step = workflow
        .find("name: Publish GitHub release")
        .expect("release workflow must publish the GitHub Release");
    let close_step = workflow
        .find("name: Close release milestone")
        .expect("release workflow must close the milestone");
    assert!(
        publish_step < close_step,
        "release workflow must close the milestone only after publishing the release"
    );
    assert!(
        workflow.contains("MILESTONE_NUMBER=\"${{ steps.release.outputs.milestone_number }}\"")
            && workflow
                .contains("MILESTONE_STATE=\"${{ steps.release.outputs.milestone_state }}\"",),
        "milestone closure must be driven by release verification outputs"
    );
}

#[test]
fn release_workflow_paginates_milestone_lookup() {
    let root = repo_root().expect("repo root");
    let workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).expect("workflow");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    assert!(
        policy.contains("milestone_lookup = \"paginated_all_states\""),
        "release automation policy must require paginated all-state milestone lookup"
    );
    let verification_step = workflow
        .split("name: Verify tag target, release notes, and milestone")
        .nth(1)
        .and_then(|tail| tail.split("name: Publish GitHub release").next())
        .expect("release verification step");
    assert!(
        verification_step.contains(
            "gh api --paginate \"repos/${GITHUB_REPOSITORY}/milestones?state=all&per_page=100\""
        ),
        "release workflow must paginate all-state milestone lookup"
    );
}

#[test]
fn release_milestone_lookup_consumes_complete_paginated_stream() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    let release_workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).expect("workflow");
    let auto_workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    assert!(
        policy.contains("milestone_selection = \"complete_stream_first_match\""),
        "release automation policy must require full-stream milestone selection"
    );
    let release_verification = workflow_block(
        &release_workflow,
        "name: Verify tag target, release notes, and milestone",
        "name: Publish GitHub release",
    );
    assert!(
        milestone_lookup_consumes_complete_stream(release_verification),
        "release workflow must consume the full paginated milestone stream"
    );
    let auto_tag_job = workflow_block(
        &auto_workflow,
        "name: Create immutable release tag",
        "dispatch-release-publication:",
    );
    assert!(
        milestone_lookup_consumes_complete_stream(auto_tag_job),
        "auto-release workflow must consume the full paginated milestone stream"
    );
}

#[test]
fn milestone_lookup_contract_rejects_head_truncation() {
    let lookup = r#"MILESTONE_JSON="$(gh api --paginate "repos/${GITHUB_REPOSITORY}/milestones?state=all&per_page=100" \
            --jq ".[] | select(.title == \"${TAG}\")" | head -n 1)""#;
    assert!(
        !milestone_lookup_consumes_complete_stream(lookup),
        "milestone lookup contract must reject head-based truncation"
    );
}

#[test]
fn release_workflow_checks_out_release_tag_for_dispatch() {
    let root = repo_root().expect("repo root");
    let workflow =
        fs::read_to_string(root.join(".github/workflows/release.yml")).expect("workflow");
    let checkout_step = workflow
        .split("actions/checkout@")
        .nth(1)
        .and_then(|tail| tail.split("- name: Verify tag target").next())
        .expect("release workflow checkout block");
    let release_ref =
        "${{ github.event_name == 'workflow_dispatch' && inputs.tag || github.ref_name }}";
    assert!(
        checkout_step.contains(&format!("ref: {release_ref}")),
        "release workflow must read release notes from the tag being published"
    );
}

#[test]
fn auto_release_tag_workflow_is_guarded() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for needle in [
        "workflow_run:",
        "workflow_dispatch:",
        "workflows: [\"CI\"]",
        "branches: [main]",
        "github.event.workflow_run.conclusion == 'success'",
        "github.event.workflow_run.event == 'push'",
        "inputs.tag",
        "inputs.sha",
        "/commits/${SHA}/pulls",
        "^release/v[0-9]+",
        "docs/releases/${TAG}.md",
        "git tag -a",
        "refusing to move",
        "gh workflow run release.yml",
        "actions: write",
    ] {
        assert!(
            workflow.contains(needle),
            "auto release workflow missing guard/action: {needle}"
        );
    }
    assert!(
        !workflow.contains("--force"),
        "auto release workflow must not force any git operation"
    );
    assert!(
        policy.contains("source_event = \"push\""),
        "release automation policy must restrict auto-tagging to push CI runs"
    );
    assert!(
        policy.contains("manual_recovery_trigger = \"workflow_dispatch\""),
        "release automation policy must allow manual recovery dispatch"
    );
    assert!(
        policy.contains("manual_recovery_requires = ["),
        "release automation policy must structure manual recovery requirements"
    );
    for required in [
        "main_ci_success",
        "merged_release_pr",
        "manual_tag_matches_release_prep_pr",
    ] {
        assert!(
            policy.contains(required),
            "release automation policy missing manual recovery requirement: {required}"
        );
    }
}

#[test]
fn auto_release_tag_manual_dispatch_checks_verified_main_sha() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let identify_job = workflow_block(&workflow, "identify-release-pr:", "create-release-tag:");
    let manual_recovery = workflow_block(
        identify_job,
        "if [[ \"${EVENT_NAME}\" == \"workflow_dispatch\" ]]; then",
        "exit 0",
    );
    for required in [
        "workflow_dispatch:",
        "tag:",
        "sha:",
        "github.event_name == 'workflow_dispatch'",
        "INPUT_TAG:",
        "INPUT_SHA:",
        "git fetch origin main:refs/remotes/origin/main --tags",
        "git merge-base --is-ancestor \"${SHA}\" origin/main",
        "actions/workflows/ci.yml/runs",
        "head_sha=${SHA}",
        "status=success",
        "Manual recovery SHA must have a successful main CI run",
        "/commits/${SHA}/pulls",
        "Manual recovery SHA must resolve to exactly one merged release-prep PR.",
        "if [[ \"${TAG}\" != \"${INPUT_TAG}\" ]]",
        "release=true",
        "tag=${TAG}",
        "sha=${SHA}",
    ] {
        assert!(
            manual_recovery.contains(required) || workflow.contains(required),
            "auto-release manual recovery missing verified-sha contract: {required}"
        );
    }
}

#[test]
fn auto_release_tag_workflow_scopes_job_permissions() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    assert!(
        policy.contains("job_permissions = \"least_privilege\""),
        "release automation policy must require least-privilege job permissions"
    );
    let top_level_permissions = workflow
        .split("permissions:")
        .nth(1)
        .and_then(|tail| tail.split("\njobs:").next())
        .expect("top-level permissions block");
    assert!(
        top_level_permissions.contains("contents: read")
            && !top_level_permissions.contains("write"),
        "top-level auto-release permissions must be read-only"
    );
    let identify_job = workflow
        .split("identify-release-pr:")
        .nth(1)
        .and_then(|tail| tail.split("create-release-tag:").next())
        .expect("identify-release-pr job block");
    assert!(
        identify_job.contains(
            "permissions:\n      actions: read\n      contents: read\n      pull-requests: read",
        ) && !identify_job.contains("write"),
        "identify-release-pr must only read actions, contents, and pull requests"
    );
    let tag_job = workflow
        .split("create-release-tag:")
        .nth(1)
        .and_then(|tail| tail.split("dispatch-release-publication:").next())
        .expect("create-release-tag job block");
    assert!(
        tag_job.contains("permissions:\n      contents: write\n      issues: read")
            && !tag_job.contains("actions: write")
            && !tag_job.contains("pull-requests: write"),
        "create-release-tag must only write contents and read issues"
    );
    let dispatch_job = workflow
        .split("dispatch-release-publication:")
        .nth(1)
        .expect("dispatch-release-publication job block");
    assert!(
        dispatch_job.contains("permissions:\n      actions: write")
            && !dispatch_job.contains("contents: write")
            && !dispatch_job.contains("issues: write")
            && !dispatch_job.contains("pull-requests: write"),
        "dispatch-release-publication must only write actions"
    );
}

#[test]
fn auto_release_tag_uses_ephemeral_push_credentials() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let tag_job = workflow
        .split("create-release-tag:")
        .nth(1)
        .and_then(|tail| tail.split("dispatch-release-publication:").next())
        .expect("create-release-tag job block");
    for required in [
            "persist-credentials: false",
            "AUTHENTICATED_ORIGIN=\"https://x-access-token:${GH_TOKEN}@github.com/${GITHUB_REPOSITORY}.git\"",
            "git ls-remote --exit-code --tags \"${AUTHENTICATED_ORIGIN}\"",
            "git fetch \"${AUTHENTICATED_ORIGIN}\"",
            "git push \"${AUTHENTICATED_ORIGIN}\"",
        ] {
            assert!(
                tag_job.contains(required),
                "create-release-tag job missing ephemeral credential guard: {required}"
            );
        }
}

#[test]
fn auto_release_tag_checks_milestone_before_tag_push() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let milestone_check = workflow
        .find("OPEN_ISSUES")
        .expect("auto release workflow must check milestone open issue count");
    let tag_creation = workflow
        .find("git tag -a")
        .expect("auto release workflow must create an annotated tag");
    assert!(
        milestone_check < tag_creation,
        "auto release workflow must check milestone readiness before creating an immutable tag"
    );
}

#[test]
fn auto_release_tag_dispatches_release_from_tag_ref() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    assert!(
        workflow.contains(r#"gh workflow run release.yml --ref "${TAG}" -f tag="${TAG}""#),
        "auto release workflow must dispatch release.yml from the created tag ref"
    );
}

#[test]
fn auto_release_tag_dispatches_with_explicit_repo() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/auto-release-tag.yml"))
        .expect("auto release workflow");
    let dispatch_job = workflow
        .split("dispatch-release-publication:")
        .nth(1)
        .expect("dispatch-release-publication job block");
    assert!(
        dispatch_job.contains("GH_REPO: ${{ github.repository }}")
            || dispatch_job.contains("--repo \"${GITHUB_REPOSITORY}\""),
        "release dispatch must name the repository explicitly"
    );
}

#[test]
fn release_tag_recovery_policy_is_structured() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for needle in [
        "[release_tags]",
        "mutation = \"forbidden\"",
        "recovery = \"publish_existing_valid_tag\"",
        "requires_main_reachable_target = true",
        "normal_creation = \"auto_after_release_pr_main_ci\"",
        "manual_creation = \"operator_fallback\"",
    ] {
        assert!(
            policy.contains(needle),
            "release policy missing structured field: {needle}"
        );
    }
}

#[test]
fn release_runbook_policy_is_structured() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for required in [
        "[release_runbook]",
        "prepare_branch",
        "write_release_thesis",
        "diff_previous_tag",
        "reconcile_signpost_docs",
        "merge_gate",
        "auto_tag_publish",
        "watch_workflow",
        "capture_evidence",
        "capture_release_report",
        "manual_fallback_target = \"verified_main_merge_commit\"",
        "post_release_milestone_lookup = \"all_states_paginated\"",
        "cargo xtask verify",
        "cargo test -p xtask release_",
        "git diff --stat <previous-tag>..HEAD",
        "git diff --name-status <previous-tag>..HEAD",
        "git log --oneline <previous-tag>..HEAD",
        "gh pr checks",
        "gh release view",
        "release_thesis",
        "release_issue",
        "previous_tag_diff_stat",
        "previous_tag_diff_name_status",
        "previous_tag_log",
        "milestone_zero_open_at_tag_time",
        "no_crates_io_publication",
        "release_report",
        "plan_versus_actual",
        "fallout_issues",
        "next_release_thesis",
        "scaffold_command = \"cargo xtask release-prep <version>\"",
        "workspace_package_versions",
        "lockfile_package_versions",
        "dated_changelog_section",
        "release_policy_boundary_block",
        "release_notes_stub",
        "release_process_test_plan_rows",
    ] {
        assert!(
            policy.contains(required),
            "release runbook policy missing structured field: {required}"
        );
    }
}

#[test]
fn release_prep_scaffolds_version_policy_changelog_and_notes() {
    let root = temp_root("release-prep-scaffold");
    write_release_prep_scaffold_fixture(&root);
    let xtask_source_before =
        fs::read_to_string(root.join("xtask/src/tests.rs")).expect("xtask tests before");
    release_prep(
        &root,
        "v0.12.0-alpha.1",
        Some("2026-08-04"),
        std::time::UNIX_EPOCH,
    )
    .expect("release prep scaffold");

    let package_version = "version = \"0.12.0-alpha.1\"";
    assert!(
        fs::read_to_string(root.join("crates/amblicit-cli/Cargo.toml"))
            .expect("amblicit-cli manifest")
            .contains(package_version),
        "release prep must bump amblicit-cli"
    );
    assert!(
        fs::read_to_string(root.join("crates/amblicit-syntax/Cargo.toml"))
            .expect("amblicit-syntax manifest")
            .contains(package_version),
        "release prep must bump amblicit-syntax"
    );
    let lockfile = fs::read_to_string(root.join("Cargo.lock")).expect("lockfile");
    assert!(
        lockfile.contains("name = \"amblicit-cli\"\nversion = \"0.12.0-alpha.1\"")
            && lockfile.contains("name = \"amblicit-syntax\"\nversion = \"0.12.0-alpha.1\""),
        "release prep must update workspace package lockfile entries"
    );
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    assert!(
            changelog.contains(
                "## [Unreleased]\n\n## [v0.12.0-alpha.1] - 2026-08-04\n\n### Changed\n\n- Pending release note."
            ),
            "release prep must move pending changelog content under the dated release section"
        );
    let policy =
        fs::read_to_string(root.join("docs/topics/release-process/policy.toml")).expect("policy");
    for required in [
        "[release_notes.v0_12_0_alpha_1]",
        "tag = \"v0.12.0-alpha.1\"",
        "target_date = \"2026-08-04\"",
        "status = \"prep\"",
        "TODO_release_scope",
        "TODO_release_non_goal",
    ] {
        assert!(
            policy.contains(required),
            "release prep must scaffold policy field: {required}"
        );
    }
    assert!(
        fs::read_to_string(root.join("docs/releases/v0.12.0-alpha.1.md"))
            .expect("release notes")
            .contains("# v0.12.0-alpha.1"),
        "release prep must create release notes"
    );
    let test_plan = fs::read_to_string(root.join("docs/topics/release-process/test-plan.md"))
        .expect("test plan");
    assert!(
        test_plan.contains("RELEASE-REQ-025") && test_plan.contains("RELEASE-TP-020"),
        "release prep must add paired requirement and test-plan rows"
    );
    // Release prep no longer writes Rust test stubs. Per-release policy content
    // is reviewed, block structure is covered by one data-driven test, and
    // dates are reconciled against git tags by `cargo xtask release-dates`.
    // Comparing the whole file against the seeded fixture is what makes this
    // assertion able to fail: searching for a stub name the fixture never
    // contained would pass no matter what release-prep wrote.
    let xtask_source = fs::read_to_string(root.join("xtask/src/tests.rs")).expect("xtask tests");
    assert_eq!(
        xtask_source, xtask_source_before,
        "release prep must not modify xtask/src/tests.rs"
    );
}

#[test]
fn release_prep_keeps_facade_exact_dependency_resolvable() {
    let root = temp_root("release-prep-facade");
    write_release_prep_scaffold_fixture(&root);
    release_prep(
        &root,
        "v0.12.0-alpha.1",
        Some("2026-08-04"),
        std::time::UNIX_EPOCH,
    )
    .expect("release prep scaffold");

    let lockfile_before = fs::read(root.join("Cargo.lock")).expect("prepared lockfile");
    let output = Command::new("cargo")
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .current_dir(&root)
        .output()
        .expect("cargo metadata for prepared workspace");
    assert!(
        output.status.success(),
        "prepared workspace must resolve: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("Cargo metadata");
    let packages = metadata["packages"].as_array().expect("packages");
    assert_eq!(packages.len(), 3);
    for package in packages {
        assert_eq!(package["version"], "0.12.0-alpha.1", "{}", package["name"]);
    }
    let facade = packages
        .iter()
        .find(|package| package["name"] == "flyingrobots-amblicit")
        .expect("facade package");
    let syntax = facade["dependencies"]
        .as_array()
        .expect("facade dependencies")
        .iter()
        .find(|dependency| dependency["name"] == "amblicit-syntax")
        .expect("facade implementation dependency");
    assert_eq!(syntax["req"], "=0.12.0-alpha.1");
    assert_eq!(
        fs::read(root.join("Cargo.lock")).expect("checked lockfile"),
        lockfile_before,
        "metadata must not repair the prepared lockfile"
    );
}

#[test]
fn release_prep_rejects_existing_release_notes_before_writing() {
    let root = temp_root("release-prep-existing-notes");
    write_release_prep_scaffold_fixture(&root);
    fs::write(
        root.join("docs/releases/v0.12.0-alpha.1.md"),
        "# existing notes\n",
    )
    .expect("existing release notes");
    let tracked = [
        "crates/amblicit-cli/Cargo.toml",
        "crates/amblicit-syntax/Cargo.toml",
        "crates/amblicit/Cargo.toml",
        "Cargo.lock",
        "CHANGELOG.md",
        "docs/topics/release-process/policy.toml",
        "docs/topics/release-process/test-plan.md",
        "xtask/src/main.rs",
        "xtask/src/tests.rs",
    ];
    let before = tracked
        .iter()
        .map(|path| {
            (
                *path,
                fs::read_to_string(root.join(path)).expect("read fixture before"),
            )
        })
        .collect::<Vec<_>>();

    let error = release_prep(
        &root,
        "v0.12.0-alpha.1",
        Some("2026-08-04"),
        std::time::UNIX_EPOCH,
    )
    .expect_err("existing notes reject");
    assert!(
        error.contains("release notes already exist"),
        "duplicate release notes must be rejected before writes: {error}"
    );
    for (path, original) in before {
        assert_eq!(
            fs::read_to_string(root.join(path)).expect("read fixture after"),
            original,
            "{path} must not change after preflight rejection"
        );
    }
}

fn write_release_prep_scaffold_fixture(root: &Path) {
    fs::create_dir_all(root).expect("workspace dir");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/amblicit-cli\", \"crates/amblicit-syntax\", \"crates/amblicit\"]\nresolver = \"3\"\n",
    )
    .expect("workspace manifest");
    for package in ["amblicit-cli", "amblicit-syntax", "amblicit"] {
        let source = root.join("crates").join(package).join("src");
        fs::create_dir_all(&source).expect("package source dir");
        fs::write(source.join("lib.rs"), "").expect("library target");
    }
    fs::write(
        root.join("crates/amblicit/Cargo.toml"),
        "[package]\nname = \"flyingrobots-amblicit\"\nversion = \"0.11.0-alpha.1\"\n[dependencies]\namblicit-syntax = { path = \"../amblicit-syntax\", version = \"=0.11.0-alpha.1\" }\n",
    )
    .expect("facade manifest");
    fs::create_dir_all(root.join("crates/amblicit-cli")).expect("amblicit-cli dir");
    fs::create_dir_all(root.join("crates/amblicit-syntax")).expect("amblicit-syntax dir");
    fs::create_dir_all(root.join("docs/releases")).expect("release notes dir");
    fs::create_dir_all(root.join("docs/topics/release-process")).expect("release topic dir");
    fs::create_dir_all(root.join("xtask/src")).expect("xtask source dir");
    fs::write(
        root.join("crates/amblicit-cli/Cargo.toml"),
        "[package]\nname = \"amblicit-cli\"\nversion = \"0.11.0-alpha.1\"\n",
    )
    .expect("amblicit-cli manifest");
    fs::write(
        root.join("crates/amblicit-syntax/Cargo.toml"),
        "[package]\nname = \"amblicit-syntax\"\nversion = \"0.11.0-alpha.1\"\n",
    )
    .expect("amblicit-syntax manifest");
    fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"amblicit-cli\"\nversion = \"0.11.0-alpha.1\"\n\n[[package]]\nname = \"amblicit-syntax\"\nversion = \"0.11.0-alpha.1\"\n\n[[package]]\nname = \"flyingrobots-amblicit\"\nversion = \"0.11.0-alpha.1\"\ndependencies = [\n \"amblicit-syntax\",\n]\n",
        )
        .expect("lockfile");
    fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\n## [Unreleased]\n\n### Changed\n\n- Pending release note.\n\n## [v0.11.0-alpha.1] - 2026-11-04\n",
        )
        .expect("changelog");
    write_release_prep_policy_fixture(root);
    write_release_prep_test_plan_fixture(root);
    fs::write(root.join("xtask/src/main.rs"), "fn main() {}\n").expect("xtask dispatch source");
    fs::write(
        root.join("xtask/src/tests.rs"),
        "fn release_policy_tracks_v0_11_boundary() {}\n\nfn alpha_changelog_dates_match_release_policy() {\n    for (tag, target) in [\n        (\"v0.11.0-alpha.1\", \"2026-11-04\"),\n    ] {\n        let _ = (tag, target);\n    }\n}\n",
    )
    .expect("xtask test source");
}

fn write_release_prep_policy_fixture(root: &Path) {
    fs::write(
            root.join("docs/topics/release-process/policy.toml"),
            "[release_notes.v0_11_0_alpha_1]\ntag = \"v0.11.0-alpha.1\"\ntarget_date = \"2026-11-04\"\nstatus = \"published\"\nscope = [\n  \"previous_scope\",\n]\nnon_goals = [\n  \"previous_non_goal\",\n]\n",
        )
        .expect("policy");
}

fn write_release_prep_test_plan_fixture(root: &Path) {
    fs::write(
            root.join("docs/topics/release-process/test-plan.md"),
            "# Release Process Test Plan\n\n## Requirements\n\n| ID | Status | Requirement | Source |\n| --- | --- | --- | --- |\n| RELEASE-REQ-023 | implemented | Existing release boundary. | docs/topics/release-process/policy.toml |\n| RELEASE-REQ-024 | implemented | Release prep scaffolding exists. | xtask/src/main.rs |\n\n## Fixtures\n\n| Fixture | Purpose | Oracle |\n| --- | --- | --- |\n| docs/topics/release-process/policy.toml | Structured release policy. | Checked by xtask. |\n\n## Test Cases\n\n| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n| RELEASE-TP-018 | implemented | Boundary guard | RELEASE-REQ-023 | Existing release boundary is pinned. | release_policy_tracks_v0_11_boundary | docs/topics/release-process/policy.toml | Existing row. |\n| RELEASE-TP-019 | implemented | Scaffolding guard | RELEASE-REQ-024 | Release prep scaffolding is pinned. | release_prep_scaffolds_version_policy_changelog_and_notes | xtask/src/main.rs | Existing row. |\n\n## Determinism Obligations\n",
        )
        .expect("test plan");
}

#[test]
fn rust_workspace_lints_define_safety_baseline() {
    let root = repo_root().expect("repo root");
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    for required in [
        "[workspace.lints.rust]",
        "unsafe_code = \"forbid\"",
        "missing_debug_implementations = \"deny\"",
        "[workspace.lints.clippy]",
        "all = { level = \"deny\", priority = -1 }",
        "pedantic = { level = \"deny\", priority = -1 }",
    ] {
        assert!(
            manifest.contains(required),
            "workspace lint baseline missing structured field: {required}"
        );
    }
}

#[test]
fn workspace_msrv_matches_the_ci_toolchain() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("CI workflow");
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(&root)
        .output()
        .expect("cargo metadata runs");
    assert!(output.status.success(), "cargo metadata must succeed");
    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON");
    let workspace_members: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .expect("workspace members")
        .iter()
        .map(|member| member.as_str().expect("workspace member is a string"))
        .collect();
    let packages = metadata["packages"].as_array().expect("metadata packages");
    let mut checked_members = 0;

    for package in packages {
        let id = package["id"].as_str().expect("package id");
        if workspace_members.contains(id) {
            checked_members += 1;
            assert_eq!(
                package["rust_version"].as_str(),
                Some("1.96"),
                "workspace package {} must inherit Rust 1.96",
                package["name"].as_str().expect("package name")
            );
        }
    }
    assert_eq!(checked_members, workspace_members.len());
    assert!(
        workflow.contains("rust-version: \"1.96.0\"")
            && workflow.contains("rust-label: \"msrv 1.96.0\""),
        "CI must exercise the exact Rust 1.96.0 MSRV"
    );
}

#[test]
fn provider_fixture_inventory_is_an_explicit_ci_gate() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("CI workflow");

    assert!(
        workflow.contains("Provider component fixture inventory")
            && workflow.contains(
                "cargo +\"${{ matrix.rust-version }}\" xtask provider-component-fixtures --check"
            ),
        "CI must explicitly verify the checked provider component inventory"
    );
}

#[test]
fn cargo_deny_supply_chain_gate_covers_root_and_fixture_guest() {
    let root = repo_root().expect("repo root");
    let workflow = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("CI workflow");
    let policy = fs::read_to_string(root.join("deny.toml")).expect("cargo-deny policy");

    for required in [
        "name: supply-chain (cargo-deny)",
        "cargo +stable install cargo-deny --locked --version 0.18.9",
        "cargo +stable deny --locked check",
        "--manifest-path fixtures/providers/components/guests/Cargo.toml check",
    ] {
        assert!(
            workflow.contains(required),
            "cargo-deny workflow contract missing `{required}`"
        );
    }
    for required in [
        "[advisories]",
        "yanked = \"deny\"",
        "[licenses]",
        "[bans]",
        "[sources]",
        "unknown-registry = \"deny\"",
        "unknown-git = \"deny\"",
    ] {
        assert!(
            policy.contains(required),
            "cargo-deny policy missing `{required}`"
        );
    }
}

#[test]
fn provider_runtime_dependency_boundary_is_narrow() {
    provider_runtime_dependencies(&repo_root().expect("repo root"))
        .expect("provider runtime dependency boundary remains narrow");
}

fn assert_compiler_settings_projection_properties(properties: &serde_json::Map<String, Value>) {
    for field in ["emit", "compilerContext", "target"] {
        assert!(
            properties.contains_key(field),
            "compiler settings schema missing projection field `{field}`"
        );
    }
    let emit = properties
        .get("emit")
        .unwrap_or_else(|| panic!("compiler settings schema missing `emit` property"));
    assert_eq!(
        emit.get("minItems").and_then(Value::as_u64),
        Some(1),
        "compiler settings projection emit list must reject empty arrays"
    );
    let expected_emit_values = ["syntax", "diagnostics", "core", "targetIr", "digests"]
        .into_iter()
        .map(Value::from)
        .collect::<Vec<_>>();
    assert_eq!(
        emit.pointer("/items/enum").and_then(Value::as_array),
        Some(&expected_emit_values),
        "compiler settings projection emit enum is an exact ordered contract"
    );
}

#[test]
fn compiler_settings_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = read_json_file(&root.join("docs/schemas/edict.compiler-settings.v1.schema.json"));

    assert_eq!(
        json_str(&schema, "$schema"),
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        json_str(&schema, "$id"),
        "https://flyingrobots.dev/schemas/edict/compiler-settings/v1"
    );
    assert_eq!(json_str(&schema, "type"), "object");
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false)
    );
    for required in ["schema", "type", "operation"] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "compiler settings schema must require `{required}`"
        );
    }

    let properties = json_object(&schema, "properties");
    let record_schema = properties
        .get("schema")
        .unwrap_or_else(|| panic!("compiler settings schema missing `schema` property"));
    assert_eq!(
        record_schema.get("const").and_then(Value::as_str),
        Some("edict.compiler.settings/v1")
    );
    let record_type = properties
        .get("type")
        .unwrap_or_else(|| panic!("compiler settings schema missing `type` property"));
    assert_eq!(
        record_type.get("const").and_then(Value::as_str),
        Some("compilerSettings"),
        "compiler settings schema must expose one record type"
    );
    let operation = properties
        .get("operation")
        .unwrap_or_else(|| panic!("compiler settings schema missing `operation` property"));
    assert_eq!(
        json_array(operation, "enum"),
        &vec![
            Value::String("build".to_owned()),
            Value::String("check".to_owned()),
            Value::String("project".to_owned()),
        ],
        "compiler settings schema must expose exactly the supported operations"
    );
    let application = properties
        .get("application")
        .unwrap_or_else(|| panic!("compiler settings schema missing `application` property"));
    assert_eq!(
        application.get("type").and_then(Value::as_str),
        Some("string"),
        "application path must be a string"
    );
    assert_eq!(
        application.get("minLength").and_then(Value::as_u64),
        Some(1),
        "application path must be non-empty"
    );
    assert_compiler_settings_projection_properties(properties);
    assert_eq!(
        schema
            .pointer("/properties/target/properties/profileDigest/pattern")
            .and_then(Value::as_str),
        Some("^sha256:[0-9a-f]{64}$"),
        "project target profile digest must be lowercase strict"
    );
    assert!(
        properties.contains_key("directoryExtensions"),
        "compiler settings schema missing deterministic directory extension field"
    );
    assert!(
        properties.contains_key("followSymlinks"),
        "compiler settings schema missing symlink traversal field"
    );
    assert!(
        properties.contains_key("inputRoot"),
        "compiler settings schema missing optional root-confinement field"
    );
}

#[test]
fn cli_goldens_command_is_wired_into_verify() {
    let root = repo_root().expect("repo root");
    let source = fs::read_to_string(root.join("xtask/src/main.rs")).expect("read xtask source");

    assert!(
        source.contains("Some(\"cli-goldens\")"),
        "xtask dispatch must expose the cli-goldens command"
    );
    assert!(
        source.contains("cli_goldens(root, CliGoldenMode::Check)?;"),
        "cargo xtask verify must run cli-goldens in check mode"
    );
}

#[test]
fn verify_rust_commands_runs_one_default_workspace_test_pass() {
    let root = Path::new("/repo");
    let mut commands = Vec::new();

    super::verify_rust_commands_with(root, |actual_root, program, args| {
        commands.push((
            actual_root.to_owned(),
            program.to_owned(),
            args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
        ));
        Ok(())
    })
    .expect("record Rust verification commands");

    assert!(
        commands
            .iter()
            .all(|(actual_root, _, _)| actual_root == root),
        "every Rust verification command must run from the repository root"
    );
    let workspace_test_commands = commands
        .iter()
        .filter(|(_, program, args)| {
            program == "cargo" && args.first().is_some_and(|arg| arg == "test")
        })
        .map(|(_, _, args)| args.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        workspace_test_commands,
        vec![vec![
            "test".to_owned(),
            "--workspace".to_owned(),
            "--all-features".to_owned(),
        ]],
        "the default workspace test pass already includes doctests"
    );
}

#[test]
fn cli_golden_binary_path_uses_cargo_metadata_target_directory() {
    let target_directory = std::env::temp_dir().join("edict-custom-target");
    let target_directory_json =
        serde_json::to_string(target_directory.to_str().expect("utf-8 temp path"))
            .expect("quote target directory");
    let metadata = format!(r#"{{"target_directory":{target_directory_json}}}"#);
    let binary = cli_binary_path_from_cargo_metadata(metadata.as_bytes())
        .expect("metadata target directory");

    assert_eq!(
        binary,
        target_directory
            .join("debug")
            .join(format!("amblicit{}", std::env::consts::EXE_SUFFIX))
    );
}

fn cli_record_schema(root: &Path, file: &str, id: &str, record_schema: &str) -> Value {
    let schema = read_json_file(&root.join("docs/schemas").join(file));
    assert_eq!(
        json_str(&schema, "$schema"),
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(json_str(&schema, "$id"), id);
    assert_eq!(json_str(&schema, "type"), "object");
    assert_eq!(
        schema.get("additionalProperties").and_then(Value::as_bool),
        Some(false),
        "{file} must forbid additional properties"
    );
    let properties = json_object(&schema, "properties");
    assert_eq!(
        property_const(properties, "schema"),
        Some(record_schema),
        "{file} `schema` field must pin `{record_schema}`"
    );
    schema
}

fn property_const<'a>(
    properties: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Option<&'a str> {
    properties.get(key)?.get("const")?.as_str()
}

fn property_enum_has_int(
    properties: &serde_json::Map<String, Value>,
    key: &str,
    expected: i64,
) -> bool {
    properties
        .get(key)
        .map(|property| json_array(property, "enum"))
        .is_some_and(|values| values.iter().any(|value| value.as_i64() == Some(expected)))
}

fn one_of_branch<'a>(schema: &'a Value, kind: &str) -> &'a Value {
    json_array(schema, "oneOf")
        .iter()
        .find(|branch| {
            branch
                .pointer("/properties/kind/const")
                .and_then(Value::as_str)
                == Some(kind)
        })
        .unwrap_or_else(|| panic!("schema missing `oneOf` branch for kind `{kind}`"))
}

#[test]
fn compiler_input_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = cli_record_schema(
        &root,
        "edict.compiler-input.v1.schema.json",
        "https://flyingrobots.dev/schemas/edict/compiler-input/v1",
        "edict.compiler.input/v1",
    );
    for required in ["schema", "type", "kind"] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "compiler input schema must require `{required}`"
        );
    }
    let properties = json_object(&schema, "properties");
    assert_eq!(property_const(properties, "type"), Some("compilerInput"));
    let kind = properties
        .get("kind")
        .unwrap_or_else(|| panic!("compiler input schema missing `kind` property"));
    for variant in ["source", "path", "pathList", "directory", "glob"] {
        assert!(
            json_string_array_contains(kind, "enum", variant),
            "compiler input schema must declare the `{variant}` input kind"
        );
    }
    for field in ["name", "source", "path", "paths", "pattern"] {
        assert!(
            properties.contains_key(field),
            "compiler input schema missing variant field `{field}`"
        );
    }

    // Input kinds must be mutually exclusive: one `oneOf` branch per kind,
    // each forbidding the other kinds' variant fields so hybrid records
    // (e.g. a `source` record that also carries `path`) cannot validate.
    let branches = json_array(&schema, "oneOf");
    assert_eq!(
        branches.len(),
        5,
        "compiler input schema must declare one `oneOf` branch per input kind"
    );
    let exclusivity = [
        ("source", ["path", "paths", "pattern"].as_slice()),
        ("path", ["name", "source", "paths", "pattern"].as_slice()),
        ("pathList", ["name", "source", "path", "pattern"].as_slice()),
        (
            "directory",
            ["name", "source", "paths", "pattern"].as_slice(),
        ),
        ("glob", ["name", "source", "path", "paths"].as_slice()),
    ];
    for (kind, forbidden) in exclusivity {
        let branch = one_of_branch(&schema, kind);
        for field in forbidden {
            assert_eq!(
                branch.pointer(&format!("/properties/{field}")),
                Some(&Value::Bool(false)),
                "`{kind}` input branch must forbid `{field}` for mutual exclusivity"
            );
        }
    }
}

#[test]
fn check_result_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = cli_record_schema(
        &root,
        "edict.cli-check-result.v1.schema.json",
        "https://flyingrobots.dev/schemas/edict/cli-check-result/v1",
        "edict.cli.check-result/v1",
    );
    for required in ["schema", "type", "command", "input", "status"] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "check result schema must require `{required}`"
        );
    }
    let properties = json_object(&schema, "properties");
    assert_eq!(property_const(properties, "type"), Some("checkResult"));
    assert_eq!(property_const(properties, "command"), Some("check"));
    assert_eq!(property_const(properties, "status"), Some("ok"));
    assert!(
        properties.contains_key("input"),
        "check result schema missing `input` descriptor field"
    );
}

#[test]
fn projection_record_schemas_declare_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let cases = [
        (
            "edict.projection-syntax.v1.schema.json",
            "https://flyingrobots.dev/schemas/edict/projection-syntax/v1",
            "edict.projection.syntax/v1",
            "syntax",
        ),
        (
            "edict.projection-diagnostics.v1.schema.json",
            "https://flyingrobots.dev/schemas/edict/projection-diagnostics/v1",
            "edict.projection.diagnostics/v1",
            "diagnostics",
        ),
        (
            "edict.projection-core.v1.schema.json",
            "https://flyingrobots.dev/schemas/edict/projection-core/v1",
            "edict.projection.core/v1",
            "core",
        ),
        (
            "edict.projection-target-ir.v1.schema.json",
            "https://flyingrobots.dev/schemas/edict/projection-target-ir/v1",
            "edict.projection.target-ir/v1",
            "targetIr",
        ),
    ];

    for (file, id, schema_id, record_type) in cases {
        let schema = cli_record_schema(&root, file, id, schema_id);
        for required in ["schema", "type", "command", "input"] {
            assert!(
                json_string_array_contains(&schema, "required", required),
                "{file} must require `{required}`"
            );
        }
        let properties = json_object(&schema, "properties");
        assert_eq!(property_const(properties, "type"), Some(record_type));
        assert_eq!(property_const(properties, "command"), Some("project"));
        assert!(
            properties.contains_key("input"),
            "{file} missing input descriptor"
        );
    }

    let core = read_json_file(
        &root
            .join("docs/schemas")
            .join("edict.projection-core.v1.schema.json"),
    );
    assert!(
        json_string_array_contains(
            json_object(&core, "properties").get("state").unwrap(),
            "enum",
            "available"
        ) && json_string_array_contains(
            json_object(&core, "properties").get("state").unwrap(),
            "enum",
            "blocked"
        ),
        "Core projection schema must declare available and blocked states"
    );

    let target_ir = read_json_file(
        &root
            .join("docs/schemas")
            .join("edict.projection-target-ir.v1.schema.json"),
    );
    let target_state = json_object(&target_ir, "properties")
        .get("state")
        .unwrap_or_else(|| panic!("Target IR projection schema missing state"));
    for value in ["available", "blocked", "failed"] {
        assert!(
            json_string_array_contains(target_state, "enum", value),
            "Target IR projection schema must declare `{value}` state"
        );
    }
}

#[test]
fn diagnostic_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = cli_record_schema(
        &root,
        "edict.cli-diagnostic.v1.schema.json",
        "https://flyingrobots.dev/schemas/edict/cli-diagnostic/v1",
        "edict.cli.diagnostic/v1",
    );
    for required in [
        "schema", "type", "command", "severity", "stage", "kind", "input",
    ] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "diagnostic schema must require `{required}`"
        );
    }
    let properties = json_object(&schema, "properties");
    assert_eq!(property_const(properties, "type"), Some("diagnostic"));
    let command = properties
        .get("command")
        .unwrap_or_else(|| panic!("diagnostic schema missing `command` property"));
    for value in ["check", "project"] {
        assert!(
            json_string_array_contains(command, "enum", value),
            "diagnostic schema must declare `{value}` command"
        );
    }
    let stage = properties
        .get("stage")
        .unwrap_or_else(|| panic!("diagnostic schema missing `stage` property"));
    for value in ["parse", "semantic", "cli"] {
        assert!(
            json_string_array_contains(stage, "enum", value),
            "diagnostic schema must declare the `{value}` stage"
        );
    }
    for optional in ["span", "line", "message"] {
        assert!(
            properties.contains_key(optional),
            "diagnostic schema missing optional field `{optional}`"
        );
    }
}

#[test]
fn event_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = cli_record_schema(
        &root,
        "edict.cli-event.v1.schema.json",
        "https://flyingrobots.dev/schemas/edict/cli-event/v1",
        "edict.cli.event/v1",
    );
    for required in [
        "schema", "type", "command", "status", "checked", "errors", "exitCode",
    ] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "event schema must require `{required}`"
        );
    }
    let properties = json_object(&schema, "properties");
    assert_eq!(property_const(properties, "type"), Some("status"));
    let command = properties
        .get("command")
        .unwrap_or_else(|| panic!("event schema missing `command` property"));
    for value in ["check", "project"] {
        assert!(
            json_string_array_contains(command, "enum", value),
            "event schema must declare `{value}` command"
        );
    }
    let status = properties
        .get("status")
        .unwrap_or_else(|| panic!("event schema missing `status` property"));
    for value in ["ok", "error"] {
        assert!(
            json_string_array_contains(status, "enum", value),
            "event schema must declare the `{value}` terminal status"
        );
    }
    for code in [0, 1, 2] {
        assert!(
            property_enum_has_int(properties, "exitCode", code),
            "event schema must declare exit code `{code}`"
        );
    }

    // `status` and `exitCode` must be coupled so the schema cannot accept
    // contradictory terminal records (e.g. `status: ok` with `exitCode: 1`).
    let coupling = json_array(&schema, "allOf");
    let rule = coupling
        .first()
        .unwrap_or_else(|| panic!("event schema must couple `status` to `exitCode`"));
    assert_eq!(
        rule.pointer("/then/properties/exitCode/const")
            .and_then(Value::as_i64),
        Some(0),
        "event schema must require exit code 0 when status is `ok`"
    );
    let error_codes = rule
        .pointer("/else/properties/exitCode/enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("event schema must constrain exit code when status is `error`"));
    for code in [1, 2] {
        assert!(
            error_codes.iter().any(|value| value.as_i64() == Some(code)),
            "event schema must allow exit code `{code}` when status is `error`"
        );
    }
}

#[test]
fn info_schema_declares_jsonl_contract() {
    let root = repo_root().expect("repo root");
    let schema = cli_record_schema(
        &root,
        "edict.cli-info.v1.schema.json",
        "https://flyingrobots.dev/schemas/edict/cli-info/v1",
        "edict.cli.info/v1",
    );
    for required in ["schema", "type", "topic", "version"] {
        assert!(
            json_string_array_contains(&schema, "required", required),
            "info schema must require `{required}`"
        );
    }
    let properties = json_object(&schema, "properties");
    assert_eq!(property_const(properties, "type"), Some("info"));
    let topic = properties
        .get("topic")
        .unwrap_or_else(|| panic!("info schema missing `topic` property"));
    for value in ["help", "version"] {
        assert!(
            json_string_array_contains(topic, "enum", value),
            "info schema must declare the `{value}` topic"
        );
    }
    for field in ["usage", "requestSchemas", "exitCodes", "docs"] {
        assert!(
            properties.contains_key(field),
            "info schema missing help field `{field}`"
        );
    }

    // `requestSchemas` is pinned to the exact accepted request identifiers.
    assert_eq!(
        schema
            .pointer("/properties/requestSchemas/const")
            .and_then(Value::as_array),
        Some(&vec![
            Value::from("edict.compiler.settings/v1"),
            Value::from("edict.compiler.input/v1"),
        ]),
        "info schema must pin the exact accepted request schemas"
    );
    // `exitCodes` is an ordered 0/1/2 triple, not any subset/order.
    let exit_prefix = schema
        .pointer("/properties/exitCodes/prefixItems")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("info schema must pin exitCodes as an ordered triple"));
    let codes: Vec<i64> = exit_prefix
        .iter()
        .filter_map(|item| {
            item.pointer("/properties/code/const")
                .and_then(Value::as_i64)
        })
        .collect();
    assert_eq!(
        codes,
        [0, 1, 2],
        "info schema exitCodes must be the 0/1/2 triple in order"
    );
    assert_eq!(
        schema.pointer("/properties/exitCodes/items"),
        Some(&Value::Bool(false)),
        "info schema must forbid extra exit codes beyond the triple"
    );

    // The conditional must key on `topic == "help"`, require the help-only
    // fields, and forbid them on the `version` topic.
    let rule = json_array(&schema, "allOf")
        .first()
        .unwrap_or_else(|| panic!("info schema must conditionally constrain by topic"));
    assert_eq!(
        rule.pointer("/if/properties/topic/const")
            .and_then(Value::as_str),
        Some("help"),
        "info schema conditional must key on the `help` topic"
    );
    for field in ["usage", "requestSchemas", "exitCodes", "docs"] {
        assert!(
            rule.pointer("/then/required")
                .and_then(Value::as_array)
                .is_some_and(|req| req.iter().any(|value| value == field)),
            "info schema `help` topic must require `{field}`"
        );
        assert_eq!(
            rule.pointer(&format!("/else/properties/{field}")),
            Some(&Value::Bool(false)),
            "info schema `version` topic must forbid help field `{field}`"
        );
    }
}

#[test]
fn review_bot_fallback_policy_is_structured() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/review-process/policy.toml"))
        .expect("review policy");
    let bots = toml_section(&policy, "[review_bots]");
    assert_eq!(toml_string_value(&bots, "primary_bot"), "CodeRabbit");
    assert_eq!(
        toml_string_value(&bots, "primary_review_required_when"),
        "actively_reviewing"
    );
    assert_eq!(
        toml_array_values(&bots, "primary_unavailable_states"),
        [
            "rate_limited".to_owned(),
            "insufficient_usage_credits".to_owned(),
            "out_of_credits".to_owned(),
        ]
    );
    assert_eq!(
        toml_string_value(&bots, "fallback_request"),
        "@codex review please"
    );
    assert!(toml_bool_value(
        &bots,
        "fallback_required_when_primary_unavailable"
    ));
    assert!(toml_bool_value(
        &bots,
        "fallback_response_required_before_merge"
    ));
    assert_eq!(
        toml_string_value(&bots, "goal"),
        "at_least_one_automated_or_human_review"
    );
    assert_eq!(toml_string_value(&bots, "merge_without_review"), "blocked");
}

#[test]
fn release_topic_audit_policy_sets_minimums() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for required in [
        "[release_topic_audit]",
        "scope = \"docs/topics\"",
        "timing = \"release_prep_before_pr\"",
        "evidence_location = \"release_issue_or_pr_before_merge\"",
        "release_blocking = true",
        "coverage_definition = \"audited_topic_shelves / total_topic_shelves\"",
        "accuracy_definition = \"accurate_audited_topic_shelves / audited_topic_shelves\"",
        "stale_current_truth = \"correct_or_remove_before_counting_accurate\"",
        "topic_shelf_total",
        "topic_shelf_audited",
        "accurate_audited_topic_shelves",
        "coverage_percent",
        "accuracy_percent",
        "findings_fixed_or_release_blocking",
    ] {
        assert!(
            policy.contains(required),
            "release topic audit policy missing structured field: {required}"
        );
    }

    assert!(
        toml_integer_value(&policy, "coverage_floor_percent") >= 90,
        "release topic audit coverage floor must be at least 90%"
    );
    assert!(
        toml_integer_value(&policy, "accuracy_floor_percent") >= 90,
        "release topic audit accuracy floor must be at least 90%"
    );
}

fn toml_integer_value(policy: &str, key: &str) -> u16 {
    policy
        .lines()
        .find_map(|line| {
            let (raw_key, raw_value) = line.trim().split_once('=')?;
            if raw_key.trim() == key {
                raw_value.trim().parse::<u16>().ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("release policy missing integer field `{key}`"))
}

fn toml_string_value(policy: &str, key: &str) -> String {
    policy
        .lines()
        .find_map(|line| {
            let (raw_key, raw_value) = line.trim().split_once('=')?;
            (raw_key.trim() == key).then(|| raw_value.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| panic!("policy missing string field `{key}`"))
}

fn toml_bool_value(policy: &str, key: &str) -> bool {
    policy
        .lines()
        .find_map(|line| {
            let (raw_key, raw_value) = line.trim().split_once('=')?;
            (raw_key.trim() == key).then(|| raw_value.trim() == "true")
        })
        .unwrap_or_else(|| panic!("policy missing boolean field `{key}`"))
}

fn toml_array_values(policy: &str, key: &str) -> Vec<String> {
    let mut lines = policy.lines().map(str::trim);
    while let Some(line) = lines.next() {
        if !line.starts_with(key) {
            continue;
        }
        let mut values = Vec::new();
        for value_line in lines.by_ref() {
            if value_line == "]" {
                return values;
            }
            values.push(
                value_line
                    .trim_end_matches(',')
                    .trim_matches('"')
                    .to_owned(),
            );
        }
    }
    panic!("policy missing array field `{key}`")
}

#[test]
fn release_automation_policy_is_structured() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    for required in [
        "[release_automation]",
        "trigger = \"successful_main_ci_after_release_prep_merge\"",
        "source_branch_pattern = \"release/vX.Y.Z-alpha.N-prep\"",
        "tag_derivation = \"strip_release_prefix_and_prep_suffix\"",
        "publication_trigger = \"workflow_dispatch\"",
        "close_milestone = \"after_github_release_when_zero_open_issues\"",
        "idempotency = \"existing_same_target_tag_ok\"",
        "existing_different_target_tag = \"fail_without_mutation\"",
        "merged_release_pr",
        "tag_absent_or_same_target",
        "zero_open_milestone_issues",
    ] {
        assert!(
            policy.contains(required),
            "release automation policy missing structured field: {required}"
        );
    }
}

#[test]
fn release_policy_blocks_are_structurally_complete() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    assert_release_policy_structure(&policy);
}

fn assert_release_policy_structure(policy: &str) {
    let blocks = crate::release_dates::parse_release_policy_blocks(policy);

    // Preserve historical identities while allowing future prep blocks.
    // Git tag reconciliation separately covers every actual tagged release.
    assert_eq!(missing_historical_policy_tags(&blocks), BTreeSet::new());

    let mut seen_sections = BTreeSet::new();
    let mut seen_tags = BTreeSet::new();
    for block in &blocks {
        let section = &block.section;
        assert!(
            seen_sections.insert(section.clone()),
            "duplicate release policy section [release_notes.{section}]"
        );
        let Some(tag) = block.tag.as_deref() else {
            panic!("[release_notes.{section}] has no tag");
        };
        assert!(
            seen_tags.insert(tag.to_owned()),
            "duplicate release policy tag {tag} in [release_notes.{section}]"
        );
        assert_eq!(
            *section,
            crate::release_dates::policy_section_key(tag),
            "[release_notes.{section}] declares tag {tag}, which belongs in a differently keyed section"
        );
        let Some(date) = block.target_date.as_deref() else {
            panic!("[release_notes.{section}] has no target_date");
        };
        assert!(
            is_iso_date(date),
            "[release_notes.{section}] target_date `{date}` is not an ISO date"
        );
        let Some(status) = block.status.as_deref() else {
            panic!("[release_notes.{section}] has no status");
        };
        assert!(
            matches!(status, "published" | "planned" | "prep"),
            "[release_notes.{section}] has unexpected status `{status}`"
        );
        for (name, values) in [("scope", &block.scope), ("non_goals", &block.non_goals)] {
            let values = values.as_ref().unwrap_or_else(|| {
                panic!("[release_notes.{section}] is missing a string-list `{name}`")
            });
            // Auto-release-tag publishes from a merged prep branch, so these
            // placeholders must reject for every status before merging.
            for value in values {
                for placeholder in ["TODO_release_scope", "TODO_release_non_goal"] {
                    assert!(
                        !value.contains(placeholder),
                        "[release_notes.{section}] still has scaffold placeholder `{placeholder}`"
                    );
                }
            }
        }
    }
}

fn missing_historical_policy_tags(
    blocks: &[crate::release_dates::ReleasePolicyBlock],
) -> BTreeSet<String> {
    let present: BTreeSet<_> = blocks
        .iter()
        .filter_map(|block| block.tag.as_deref())
        .collect();
    // v0.1 predates the structured policy. v0.2 through v0.11 form the
    // historical baseline this guard replaces the old per-release checks for.
    (2..=11)
        .map(|minor| format!("v0.{minor}.0-alpha.1"))
        .filter(|tag| !present.contains(tag.as_str()))
        .collect()
}

#[test]
fn release_policy_history_cannot_be_replaced_by_a_new_release() {
    let root = repo_root().expect("repo root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    let mut blocks = crate::release_dates::parse_release_policy_blocks(&policy);
    assert_eq!(missing_historical_policy_tags(&blocks), BTreeSet::new());
    let original_count = blocks.len();
    blocks.retain(|block| block.tag.as_deref() != Some("v0.2.0-alpha.1"));
    blocks.extend(crate::release_dates::parse_release_policy_blocks(
        COMPLETED_NEXT_RELEASE_POLICY,
    ));
    assert_eq!(blocks.len(), original_count);
    assert_eq!(
        missing_historical_policy_tags(&blocks),
        BTreeSet::from(["v0.2.0-alpha.1".to_owned()])
    );
}

#[test]
fn release_policy_structure_accepts_additional_completed_prep() {
    let root = repo_root().expect("repo root");
    let mut policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("release policy");
    policy.push_str(COMPLETED_NEXT_RELEASE_POLICY);
    assert_release_policy_structure(&policy);
}

const COMPLETED_NEXT_RELEASE_POLICY: &str = "
[release_notes.v0_12_0_alpha_1]
tag = \"v0.12.0-alpha.1\"
target_date = \"2026-09-07\"
status = \"prep\"
scope = [
  \"next_release_scope\",
]
non_goals = [
  \"next_release_non_goal\",
]
";

/// Regression guard for the substring-matching flaw this test file used to
/// have. The previous per-release guards asserted
/// `policy.contains("target_date = \"...\"")` against the whole file, so a
/// release carrying a wrong date still passed whenever any other release
/// happened to carry the expected string. Releases tagged on the same day
/// share a date, so that is the normal case, not a corner case.
#[test]
fn release_policy_block_parsing_scopes_fields_to_their_own_release() {
    let policy = concat!(
        "[release_notes.v0_3_0_alpha_1]\n",
        "tag = \"v0.3.0-alpha.1\"\n",
        "target_date = \"1999-01-01\"\n",
        "status = \"published\"\n",
        "scope = [\n  \"first_scope\",\n]\n",
        "non_goals = [\n  \"first_non_goal\",\n]\n",
        "\n",
        "[release_notes.v0_4_0_alpha_1]\n",
        "tag = \"v0.4.0-alpha.1\"\n",
        "target_date = \"2026-06-24\"\n",
        "status = \"published\"\n",
        "scope = [\n  \"second_scope\",\n]\n",
        "non_goals = [\n  \"second_non_goal\",\n]\n",
    );
    let blocks = crate::release_dates::parse_release_policy_blocks(policy);
    assert_eq!(blocks.len(), 2, "both release blocks must parse");

    let first = blocks
        .iter()
        .find(|block| block.tag.as_deref() == Some("v0.3.0-alpha.1"))
        .expect("v0.3 block");

    // A whole-file substring search finds the correct-looking date here...
    assert!(policy.contains("target_date = \"2026-06-24\""));
    // ...but it belongs to v0.4, and block-scoped parsing reports v0.3's own.
    assert_eq!(first.target_date.as_deref(), Some("1999-01-01"));
    assert_eq!(first.status.as_deref(), Some("published"));
    assert_eq!(
        first.scope.as_deref(),
        Some(["first_scope".to_owned()].as_slice())
    );
}

/// The reconciliation compares recorded dates against git tag dates, which is
/// the check the previous guard could not perform: it compared `CHANGELOG.md`
/// against `policy.toml`, and release-prep writes both from one field, so the
/// two agreed even while both disagreed with the tags.
#[test]
fn release_date_reconciliation_reports_internally_consistent_wrong_dates() {
    let policy = concat!(
        "[release_notes.v0_9_0_alpha_1]\n",
        "tag = \"v0.9.0-alpha.1\"\n",
        "target_date = \"2026-10-07\"\n",
        "status = \"published\"\n",
        "scope = [\n  \"only_scope\",\n]\n",
        "non_goals = [\n  \"only_non_goal\",\n]\n",
    );
    let changelog =
        "# Changelog\n\n## [v0.9.0-alpha.1] - 2026-10-07\n\n## [v0.8.0-alpha.1] - 2026-06-28\n";
    let tags = BTreeMap::from([
        ("v0.9.0-alpha.1".to_owned(), annotated_tag("2026-06-28")),
        ("v0.8.0-alpha.1".to_owned(), annotated_tag("2026-06-28")),
    ]);
    let notes = BTreeMap::from([("v0.9.0-alpha.1".to_owned(), Some("2026-10-07".to_owned()))]);

    let report = crate::release_dates::reconcile_release_dates(&tags, policy, changelog, &notes);

    let findings: Vec<_> = report
        .drift
        .iter()
        .map(|finding| {
            (
                finding.kind,
                finding.surface,
                finding.tag.as_str(),
                finding.expected.as_deref(),
                finding.actual.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        findings,
        vec![
            (
                ReleaseDateFindingKind::MissingSurface,
                Surface::PolicyBlock,
                "v0.8.0-alpha.1",
                None,
                None
            ),
            (
                ReleaseDateFindingKind::MissingSurface,
                Surface::ReleaseNotesFile,
                "v0.8.0-alpha.1",
                None,
                None
            ),
            (
                ReleaseDateFindingKind::DateMismatch,
                Surface::PolicyTargetDate,
                "v0.9.0-alpha.1",
                Some("2026-06-28"),
                Some("2026-10-07")
            ),
            (
                ReleaseDateFindingKind::DateMismatch,
                Surface::ChangelogSection,
                "v0.9.0-alpha.1",
                Some("2026-06-28"),
                Some("2026-10-07")
            ),
            (
                ReleaseDateFindingKind::DateMismatch,
                Surface::ReleaseNotesDate,
                "v0.9.0-alpha.1",
                Some("2026-06-28"),
                Some("2026-10-07")
            ),
        ]
    );
    assert_eq!(
        report.gaps,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
}

#[test]
fn release_date_reconciliation_accepts_dates_matching_their_tags() {
    let policy = concat!(
        "[release_notes.v0_8_0_alpha_1]\n",
        "tag = \"v0.8.0-alpha.1\"\n",
        "target_date = \"2026-06-28\"\n",
        "status = \"published\"\n",
        "scope = [\n  \"only_scope\",\n]\n",
        "non_goals = [\n  \"only_non_goal\",\n]\n",
    );
    let changelog = "# Changelog\n\n## [v0.8.0-alpha.1] - 2026-06-28\n";
    let tags = BTreeMap::from([("v0.8.0-alpha.1".to_owned(), annotated_tag("2026-06-28"))]);
    let notes = BTreeMap::from([("v0.8.0-alpha.1".to_owned(), Some("2026-06-28".to_owned()))]);

    let report = crate::release_dates::reconcile_release_dates(&tags, policy, changelog, &notes);
    assert!(
        report.drift.is_empty(),
        "unexpected drift: {:?}",
        report.drift
    );
    assert!(report.gaps.is_empty(), "unexpected gaps: {:?}", report.gaps);
}

/// A lightweight tag has no tagger, so `creatordate` would silently fall back
/// to the tagged commit's committer date. Tagging an older commit would then
/// report a date that never corresponded to a release, so the tag is rejected
/// rather than trusted.
#[test]
fn release_date_reconciliation_rejects_lightweight_release_tags() {
    let policy = concat!(
        "[release_notes.v0_8_0_alpha_1]\n",
        "tag = \"v0.8.0-alpha.1\"\n",
        "target_date = \"2026-06-28\"\n",
        "status = \"published\"\n",
        "scope = [\n  \"only_scope\",\n]\n",
        "non_goals = [\n  \"only_non_goal\",\n]\n",
    );
    let changelog = "# Changelog\n\n## [v0.8.0-alpha.1] - 2026-06-28\n";
    let tags = BTreeMap::from([(
        "v0.8.0-alpha.1".to_owned(),
        crate::release_dates::TagRecord {
            date: Some("2026-06-28".to_owned()),
            annotated: false,
        },
    )]);
    let notes = BTreeMap::from([("v0.8.0-alpha.1".to_owned(), Some("2026-06-28".to_owned()))]);

    let report = crate::release_dates::reconcile_release_dates(&tags, policy, changelog, &notes);
    assert_eq!(report.drift.len(), 1);
    let finding = &report.drift[0];
    assert_eq!(finding.kind, ReleaseDateFindingKind::LightweightTag);
    assert_eq!(finding.surface, Surface::Tag);
    assert_eq!(finding.tag, "v0.8.0-alpha.1");
    assert_eq!(finding.expected.as_deref(), Some("annotated"));
    assert_eq!(finding.actual.as_deref(), Some("lightweight"));
    assert_eq!(
        report.gaps,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
}

/// A tag exists before the post-publication change flips its block from `prep`
/// to `published`. That window is a lagging surface, not a contradiction;
/// failing it would make `verify` red on `main` for unrelated branches.
#[test]
fn release_date_reconciliation_tolerates_prep_status_for_a_fresh_tag() {
    let policy = concat!(
        "[release_notes.v0_12_0_alpha_1]\n",
        "tag = \"v0.12.0-alpha.1\"\n",
        "target_date = \"2026-08-04\"\n",
        "status = \"prep\"\n",
        "scope = [\n  \"only_scope\",\n]\n",
        "non_goals = [\n  \"only_non_goal\",\n]\n",
    );
    let changelog = "# Changelog\n\n## [v0.12.0-alpha.1] - 2026-08-04\n";
    let tags = BTreeMap::from([("v0.12.0-alpha.1".to_owned(), annotated_tag("2026-08-04"))]);
    let notes = BTreeMap::from([("v0.12.0-alpha.1".to_owned(), Some("2026-08-04".to_owned()))]);

    let report = crate::release_dates::reconcile_release_dates(&tags, policy, changelog, &notes);
    assert_eq!(
        report.drift,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
    assert_eq!(report.gaps.len(), 1);
    let finding = &report.gaps[0];
    assert_eq!(finding.kind, ReleaseDateFindingKind::AwaitingPublication);
    assert_eq!(finding.surface, Surface::PolicyStatus);
    assert_eq!(finding.tag, "v0.12.0-alpha.1");
    assert_eq!(finding.expected.as_deref(), Some("published"));
    assert_eq!(finding.actual.as_deref(), Some("prep"));
}

/// Deleting a date-bearing surface must fail rather than downgrade to an
/// advisory gap, otherwise removing the evidence makes the gate pass.
#[test]
fn release_date_reconciliation_fails_when_a_covered_surface_disappears() {
    let tags = BTreeMap::from([("v0.8.0-alpha.1".to_owned(), annotated_tag("2026-06-28"))]);
    let report =
        crate::release_dates::reconcile_release_dates(&tags, "", "# Changelog\n", &BTreeMap::new());
    let surfaces: Vec<_> = report
        .drift
        .iter()
        .map(|finding| {
            assert_eq!(finding.kind, ReleaseDateFindingKind::MissingSurface);
            assert_eq!(finding.tag, "v0.8.0-alpha.1");
            assert_eq!(finding.expected, None);
            assert_eq!(finding.actual, None);
            finding.surface
        })
        .collect();
    assert_eq!(
        surfaces,
        vec![
            Surface::PolicyBlock,
            Surface::ChangelogSection,
            Surface::ReleaseNotesFile
        ]
    );
    assert_eq!(
        report.gaps,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
}

/// The one release that predates the structured policy stays advisory.
#[test]
fn release_date_reconciliation_allowlists_the_prepolicy_release() {
    let tags = BTreeMap::from([("v0.1.0-alpha.1".to_owned(), annotated_tag("2026-06-21"))]);
    let notes = BTreeMap::from([("v0.1.0-alpha.1".to_owned(), Some("2026-06-21".to_owned()))]);
    let report = crate::release_dates::reconcile_release_dates(
        &tags,
        "",
        "# Changelog\n\n## [v0.1.0-alpha.1] - 2026-06-21\n",
        &notes,
    );
    assert_eq!(
        report.drift,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
    assert_eq!(report.gaps.len(), 1);
    let finding = &report.gaps[0];
    assert_eq!(finding.kind, ReleaseDateFindingKind::MissingSurface);
    assert_eq!(finding.surface, Surface::PolicyBlock);
    assert_eq!(finding.tag, "v0.1.0-alpha.1");
    assert_eq!(finding.expected, None);
    assert_eq!(finding.actual, None);
}

#[test]
fn release_date_findings_identify_policy_errors_without_prose() {
    let tags = BTreeMap::from([("v0.8.0-alpha.1".to_owned(), annotated_tag("2026-06-28"))]);
    let report = crate::release_dates::reconcile_release_dates(
        &tags,
        "[release_notes.wrong]\ntag = \"v0.8.0-alpha.1\"\nstatus = \"planned\"\n",
        "## [v0.8.0-alpha.1] - 2026-06-28\n",
        &BTreeMap::from([("v0.8.0-alpha.1".to_owned(), Some("2026-06-28".to_owned()))]),
    );
    let fields: Vec<_> = report
        .drift
        .iter()
        .map(|finding| {
            assert_eq!(finding.tag, "v0.8.0-alpha.1");
            (
                finding.kind,
                finding.surface,
                finding.expected.as_deref(),
                finding.actual.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        vec![
            (
                ReleaseDateFindingKind::SectionMismatch,
                Surface::PolicySection,
                Some("v0_8_0_alpha_1"),
                Some("wrong")
            ),
            (
                ReleaseDateFindingKind::MissingSurface,
                Surface::PolicyTargetDate,
                None,
                None
            ),
            (
                ReleaseDateFindingKind::InvalidStatus,
                Surface::PolicyStatus,
                Some("published or prep"),
                Some("planned")
            ),
            (
                ReleaseDateFindingKind::MissingSurface,
                Surface::PolicyScope,
                None,
                None
            ),
            (
                ReleaseDateFindingKind::MissingSurface,
                Surface::PolicyNonGoals,
                None,
                None
            ),
        ]
    );
    assert_eq!(
        report.gaps,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
}

#[test]
fn release_date_findings_identify_missing_tagger_date() {
    use crate::release_dates::TagRecord;
    let tags = BTreeMap::from([(
        "v0.8.0-alpha.1".to_owned(),
        TagRecord {
            date: None,
            annotated: true,
        },
    )]);
    let report = crate::release_dates::reconcile_release_dates(&tags, "", "", &BTreeMap::new());
    assert_eq!(report.drift.len(), 1);
    let finding = &report.drift[0];
    assert_eq!(finding.kind, ReleaseDateFindingKind::MissingTaggerDate);
    assert_eq!(finding.surface, Surface::Tag);
    assert_eq!(finding.tag, "v0.8.0-alpha.1");
    assert_eq!(finding.expected, None);
    assert_eq!(finding.actual, None);
    assert_eq!(
        report.gaps,
        Vec::<crate::release_dates::ReleaseDateFinding>::new()
    );
}

fn annotated_tag(date: &str) -> crate::release_dates::TagRecord {
    crate::release_dates::TagRecord {
        date: Some(date.to_owned()),
        annotated: true,
    }
}

#[test]
fn release_policy_dates_require_real_calendar_days() {
    for (value, expected) in [
        ("2026-02-30", false),
        ("2026-02-29", false),
        ("1900-02-29", false),
        ("2026-04-31", false),
        ("2026-00-01", false),
        ("2026-13-01", false),
        ("2026-01-00", false),
        ("2026-1-001", false),
        ("2026-02-28", true),
        ("2000-02-29", true),
        ("2024-02-29", true),
    ] {
        assert_eq!(is_iso_date(value), expected, "{value}");
    }
}

fn is_iso_date(value: &str) -> bool {
    crate::release_prep::validate_iso_date(value).is_ok()
}

fn wit_named_type(interface: &Interface, name: &str) -> TypeId {
    *interface
        .types
        .get(name)
        .unwrap_or_else(|| panic!("WIT interface missing named type `{name}`"))
}

fn wit_record(resolve: &Resolve, type_id: TypeId) -> &Record {
    match &resolve.types[type_id].kind {
        TypeDefKind::Record(record) => record,
        other => panic!("WIT type must be a record, found {}", other.as_str()),
    }
}

fn wit_world_function<'a>(world: &'a World, name: &str) -> &'a Function {
    match world
        .exports
        .get(&WorldKey::Name(name.to_owned()))
        .unwrap_or_else(|| panic!("WIT world missing export `{name}`"))
    {
        WorldItem::Function(function) => function,
        _ => panic!("WIT world export `{name}` must be a function"),
    }
}

fn wit_unaliased_type(resolve: &Resolve, mut ty: Type) -> Type {
    while let Type::Id(type_id) = ty {
        match &resolve.types[type_id].kind {
            TypeDefKind::Type(next) => ty = *next,
            _ => return ty,
        }
    }
    ty
}

fn assert_wit_record_fields(resolve: &Resolve, type_id: TypeId, expected: &[&str]) {
    let actual = wit_record(resolve, type_id)
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_wit_named_field_type(
    resolve: &Resolve,
    record_id: TypeId,
    field_name: &str,
    expected_type: TypeId,
) {
    let field = wit_record(resolve, record_id)
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("WIT record missing field `{field_name}`"));
    assert_eq!(field.ty, Type::Id(expected_type));
}

fn assert_wit_field_type(
    resolve: &Resolve,
    record_id: TypeId,
    field_name: &str,
    expected_type: Type,
) {
    let field = wit_record(resolve, record_id)
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("WIT record missing field `{field_name}`"));
    assert_eq!(field.ty, expected_type);
}

fn assert_wit_list_field_type(
    resolve: &Resolve,
    record_id: TypeId,
    field_name: &str,
    expected_element_type: Type,
) {
    let field = wit_record(resolve, record_id)
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("WIT record missing field `{field_name}`"));
    let Type::Id(list_id) = field.ty else {
        panic!("WIT field `{field_name}` must be a named list type");
    };
    assert_eq!(
        resolve.types[list_id].kind,
        TypeDefKind::List(expected_element_type)
    );
}

fn assert_wit_option_field_type(
    resolve: &Resolve,
    record_id: TypeId,
    field_name: &str,
    expected_element_type: Type,
) {
    let field = wit_record(resolve, record_id)
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .unwrap_or_else(|| panic!("WIT record missing field `{field_name}`"));
    let Type::Id(option_id) = field.ty else {
        panic!("WIT field `{field_name}` must be a named option type");
    };
    assert_eq!(
        resolve.types[option_id].kind,
        TypeDefKind::Option(expected_element_type)
    );
}

fn assert_wit_enum_cases(resolve: &Resolve, type_id: TypeId, expected: &[&str]) {
    let TypeDefKind::Enum(enumeration) = &resolve.types[type_id].kind else {
        panic!("WIT type must be an enum");
    };
    let actual = enumeration
        .cases
        .iter()
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_wit_variant_cases(resolve: &Resolve, type_id: TypeId, expected: &[&str]) {
    let TypeDefKind::Variant(variant) = &resolve.types[type_id].kind else {
        panic!("WIT type must be a variant");
    };
    let actual = variant
        .cases
        .iter()
        .map(|case| case.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn assert_wit_variant_case_type(
    resolve: &Resolve,
    type_id: TypeId,
    case_name: &str,
    expected_type: Option<Type>,
) {
    let TypeDefKind::Variant(variant) = &resolve.types[type_id].kind else {
        panic!("WIT type must be a variant");
    };
    let case = variant
        .cases
        .iter()
        .find(|case| case.name == case_name)
        .unwrap_or_else(|| panic!("WIT variant missing case `{case_name}`"));
    assert_eq!(case.ty, expected_type);
}

fn assert_wit_result(
    resolve: &Resolve,
    result_id: TypeId,
    expected_ok: TypeId,
    expected_err: TypeId,
) {
    let TypeDefKind::Result(result) = &resolve.types[result_id].kind else {
        panic!("WIT type must be a result");
    };
    assert_eq!(result.ok, Some(Type::Id(expected_ok)));
    assert_eq!(result.err, Some(Type::Id(expected_err)));
}

#[derive(Debug, Clone, Copy)]
struct ProviderWitTypes {
    protocol_version: TypeId,
    digest_algorithm: TypeId,
    digest: TypeId,
    resource_ref: TypeId,
    artifact: TypeId,
    bound_artifact: TypeId,
    semantic_input_kind: TypeId,
    semantic_input: TypeId,
    lowering_output_kind: TypeId,
    lowering_output_request: TypeId,
    lowering_output_artifact: TypeId,
    verification_output_kind: TypeId,
    verification_output_request: TypeId,
    verification_output_artifact: TypeId,
    response_limits: TypeId,
    diagnostic_severity: TypeId,
    diagnostic: TypeId,
    refusal_kind: TypeId,
    refusal: TypeId,
    lowering_request: TypeId,
    verification_request: TypeId,
    lowering_success: TypeId,
    verification_success: TypeId,
    lowering_result: TypeId,
    verification_result: TypeId,
}

impl ProviderWitTypes {
    fn resolve(protocol: &Interface) -> Self {
        Self {
            protocol_version: wit_named_type(protocol, "protocol-version-v1"),
            digest_algorithm: wit_named_type(protocol, "digest-algorithm"),
            digest: wit_named_type(protocol, "digest"),
            resource_ref: wit_named_type(protocol, "resource-ref"),
            artifact: wit_named_type(protocol, "artifact"),
            bound_artifact: wit_named_type(protocol, "bound-artifact"),
            semantic_input_kind: wit_named_type(protocol, "semantic-input-kind"),
            semantic_input: wit_named_type(protocol, "semantic-input"),
            lowering_output_kind: wit_named_type(protocol, "lowering-output-kind"),
            lowering_output_request: wit_named_type(protocol, "lowering-output-request"),
            lowering_output_artifact: wit_named_type(protocol, "lowering-output-artifact"),
            verification_output_kind: wit_named_type(protocol, "verification-output-kind"),
            verification_output_request: wit_named_type(protocol, "verification-output-request"),
            verification_output_artifact: wit_named_type(protocol, "verification-output-artifact"),
            response_limits: wit_named_type(protocol, "response-limits-v1"),
            diagnostic_severity: wit_named_type(protocol, "diagnostic-severity"),
            diagnostic: wit_named_type(protocol, "diagnostic"),
            refusal_kind: wit_named_type(protocol, "provider-refusal-kind"),
            refusal: wit_named_type(protocol, "provider-refusal-v1"),
            lowering_request: wit_named_type(protocol, "lowering-request-v1"),
            verification_request: wit_named_type(protocol, "verification-request-v1"),
            lowering_success: wit_named_type(protocol, "lowering-success-v1"),
            verification_success: wit_named_type(protocol, "verification-success-v1"),
            lowering_result: wit_named_type(protocol, "lowering-result-v1"),
            verification_result: wit_named_type(protocol, "verification-result-v1"),
        }
    }
}

fn assert_provider_wit_package_shape(resolve: &Resolve, package: &Package, protocol: &Interface) {
    assert_eq!(resolve.packages.len(), 1);
    assert_eq!(
        package
            .interfaces
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["protocol"])
    );
    assert_eq!(
        package
            .worlds
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["lowerer", "verifier"])
    );
    assert!(protocol.functions.is_empty());
    assert_eq!(
        protocol
            .types
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "protocol-version-v1",
            "digest-algorithm",
            "digest",
            "resource-ref",
            "artifact",
            "bound-artifact",
            "semantic-input-kind",
            "semantic-input",
            "lowering-output-kind",
            "lowering-output-request",
            "lowering-output-artifact",
            "verification-output-kind",
            "verification-output-request",
            "verification-output-artifact",
            "response-limits-v1",
            "diagnostic-severity",
            "diagnostic",
            "provider-refusal-kind",
            "provider-refusal-v1",
            "lowering-request-v1",
            "verification-request-v1",
            "lowering-success-v1",
            "verification-success-v1",
            "lowering-result-v1",
            "verification-result-v1",
        ])
    );
}

fn assert_provider_wit_artifact_shapes(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_record_fields(
        resolve,
        types.protocol_version,
        &["major", "minor", "patch"],
    );
    assert_wit_enum_cases(resolve, types.digest_algorithm, &["sha256"]);
    assert_wit_record_fields(resolve, types.digest, &["algorithm", "bytes"]);
    assert_wit_record_fields(resolve, types.resource_ref, &["coordinate", "digest"]);
    assert_wit_record_fields(resolve, types.artifact, &["domain", "bytes"]);
    assert_wit_record_fields(resolve, types.bound_artifact, &["reference", "artifact"]);
    assert_wit_variant_cases(
        resolve,
        types.semantic_input_kind,
        &[
            "lawpack",
            "authority-facts",
            "lowerability-facts",
            "auxiliary",
        ],
    );
    assert_wit_record_fields(resolve, types.semantic_input, &["role", "kind", "artifact"]);
    assert_wit_enum_cases(
        resolve,
        types.lowering_output_kind,
        &["target-ir", "generated-artifact", "review-payload"],
    );
    assert_wit_record_fields(
        resolve,
        types.lowering_output_request,
        &["role", "kind", "domain"],
    );
    assert_wit_record_fields(
        resolve,
        types.lowering_output_artifact,
        &["role", "kind", "artifact", "logical-path"],
    );
    assert_wit_enum_cases(
        resolve,
        types.verification_output_kind,
        &["verifier-report"],
    );
    assert_wit_record_fields(
        resolve,
        types.verification_output_request,
        &["role", "kind", "domain"],
    );
    assert_wit_record_fields(
        resolve,
        types.verification_output_artifact,
        &["role", "kind", "artifact", "logical-path"],
    );
}

fn assert_provider_wit_invocation_shapes(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_record_fields(
        resolve,
        types.response_limits,
        &[
            "max-output-count",
            "max-diagnostic-count",
            "max-total-response-bytes",
        ],
    );
    assert_wit_enum_cases(
        resolve,
        types.diagnostic_severity,
        &["error", "warning", "info"],
    );
    assert_wit_record_fields(
        resolve,
        types.diagnostic,
        &["code", "severity", "message", "repair"],
    );
    assert_wit_enum_cases(
        resolve,
        types.refusal_kind,
        &[
            "unsupported-core-abi",
            "unsupported-target-profile",
            "unsupported-semantics",
            "unsupported-output-role",
            "invalid-semantic-artifact",
        ],
    );
    assert_wit_record_fields(resolve, types.refusal, &["kind", "subject", "diagnostics"]);
    assert_wit_record_fields(
        resolve,
        types.lowering_request,
        &[
            "protocol-version",
            "core",
            "target-profile",
            "semantic-inputs",
            "requested-outputs",
            "limits",
        ],
    );
    assert_wit_record_fields(
        resolve,
        types.verification_request,
        &[
            "protocol-version",
            "core",
            "target-profile",
            "target-ir",
            "semantic-inputs",
            "requested-outputs",
            "limits",
        ],
    );
    assert_wit_record_fields(resolve, types.lowering_success, &["outputs", "diagnostics"]);
    assert_wit_record_fields(
        resolve,
        types.verification_success,
        &["outputs", "diagnostics"],
    );
}

fn assert_provider_wit_resource_bindings(resolve: &Resolve, types: &ProviderWitTypes) {
    for field_name in ["major", "minor", "patch"] {
        assert_wit_field_type(resolve, types.protocol_version, field_name, Type::U32);
    }
    assert_wit_named_field_type(resolve, types.digest, "algorithm", types.digest_algorithm);
    assert_wit_list_field_type(resolve, types.digest, "bytes", Type::U8);
    assert_wit_field_type(resolve, types.resource_ref, "coordinate", Type::String);
    assert_wit_named_field_type(resolve, types.resource_ref, "digest", types.digest);
    assert_wit_field_type(resolve, types.artifact, "domain", Type::String);
    assert_wit_list_field_type(resolve, types.artifact, "bytes", Type::U8);
    assert_wit_named_field_type(
        resolve,
        types.bound_artifact,
        "reference",
        types.resource_ref,
    );
    assert_wit_named_field_type(resolve, types.bound_artifact, "artifact", types.artifact);
    for case_name in ["lawpack", "authority-facts", "lowerability-facts"] {
        assert_wit_variant_case_type(resolve, types.semantic_input_kind, case_name, None);
    }
    assert_wit_variant_case_type(
        resolve,
        types.semantic_input_kind,
        "auxiliary",
        Some(Type::String),
    );
    assert_wit_field_type(resolve, types.semantic_input, "role", Type::String);
    assert_wit_named_field_type(
        resolve,
        types.semantic_input,
        "kind",
        types.semantic_input_kind,
    );
    assert_wit_named_field_type(
        resolve,
        types.semantic_input,
        "artifact",
        types.bound_artifact,
    );
}

fn assert_provider_wit_output_bindings(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_field_type(resolve, types.lowering_output_request, "role", Type::String);
    assert_wit_named_field_type(
        resolve,
        types.lowering_output_request,
        "kind",
        types.lowering_output_kind,
    );
    assert_wit_field_type(
        resolve,
        types.lowering_output_request,
        "domain",
        Type::String,
    );
    assert_wit_field_type(
        resolve,
        types.lowering_output_artifact,
        "role",
        Type::String,
    );
    assert_wit_named_field_type(
        resolve,
        types.lowering_output_artifact,
        "kind",
        types.lowering_output_kind,
    );
    assert_wit_named_field_type(
        resolve,
        types.lowering_output_artifact,
        "artifact",
        types.artifact,
    );
    assert_wit_option_field_type(
        resolve,
        types.lowering_output_artifact,
        "logical-path",
        Type::String,
    );
    assert_wit_field_type(
        resolve,
        types.verification_output_request,
        "role",
        Type::String,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_output_request,
        "kind",
        types.verification_output_kind,
    );
    assert_wit_field_type(
        resolve,
        types.verification_output_request,
        "domain",
        Type::String,
    );
    assert_wit_field_type(
        resolve,
        types.verification_output_artifact,
        "role",
        Type::String,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_output_artifact,
        "kind",
        types.verification_output_kind,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_output_artifact,
        "artifact",
        types.artifact,
    );
    assert_wit_option_field_type(
        resolve,
        types.verification_output_artifact,
        "logical-path",
        Type::String,
    );
}

fn assert_provider_wit_diagnostic_bindings(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_field_type(resolve, types.diagnostic, "code", Type::String);
    assert_wit_named_field_type(
        resolve,
        types.diagnostic,
        "severity",
        types.diagnostic_severity,
    );
    assert_wit_field_type(resolve, types.diagnostic, "message", Type::String);
    assert_wit_option_field_type(resolve, types.diagnostic, "repair", Type::String);
    assert_wit_named_field_type(resolve, types.refusal, "kind", types.refusal_kind);
    assert_wit_option_field_type(resolve, types.refusal, "subject", Type::String);
    assert_wit_list_field_type(
        resolve,
        types.refusal,
        "diagnostics",
        Type::Id(types.diagnostic),
    );
    assert_wit_field_type(
        resolve,
        types.response_limits,
        "max-output-count",
        Type::U32,
    );
    assert_wit_field_type(
        resolve,
        types.response_limits,
        "max-diagnostic-count",
        Type::U32,
    );
    assert_wit_field_type(
        resolve,
        types.response_limits,
        "max-total-response-bytes",
        Type::U64,
    );
}

fn assert_provider_wit_lowering_bindings(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_named_field_type(
        resolve,
        types.lowering_request,
        "protocol-version",
        types.protocol_version,
    );
    assert_wit_named_field_type(
        resolve,
        types.lowering_request,
        "core",
        types.bound_artifact,
    );
    assert_wit_named_field_type(
        resolve,
        types.lowering_request,
        "target-profile",
        types.bound_artifact,
    );
    assert_wit_list_field_type(
        resolve,
        types.lowering_request,
        "semantic-inputs",
        Type::Id(types.semantic_input),
    );
    assert_wit_list_field_type(
        resolve,
        types.lowering_request,
        "requested-outputs",
        Type::Id(types.lowering_output_request),
    );
    assert_wit_named_field_type(
        resolve,
        types.lowering_request,
        "limits",
        types.response_limits,
    );
    assert_wit_list_field_type(
        resolve,
        types.lowering_success,
        "outputs",
        Type::Id(types.lowering_output_artifact),
    );
    assert_wit_list_field_type(
        resolve,
        types.lowering_success,
        "diagnostics",
        Type::Id(types.diagnostic),
    );
    assert_wit_result(
        resolve,
        types.lowering_result,
        types.lowering_success,
        types.refusal,
    );
}

fn assert_provider_wit_verification_bindings(resolve: &Resolve, types: &ProviderWitTypes) {
    assert_wit_named_field_type(
        resolve,
        types.verification_request,
        "protocol-version",
        types.protocol_version,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_request,
        "core",
        types.bound_artifact,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_request,
        "target-profile",
        types.bound_artifact,
    );
    assert_wit_list_field_type(
        resolve,
        types.verification_request,
        "semantic-inputs",
        Type::Id(types.semantic_input),
    );
    assert_wit_list_field_type(
        resolve,
        types.verification_request,
        "requested-outputs",
        Type::Id(types.verification_output_request),
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_request,
        "target-ir",
        types.bound_artifact,
    );
    assert_wit_named_field_type(
        resolve,
        types.verification_request,
        "limits",
        types.response_limits,
    );
    assert_wit_list_field_type(
        resolve,
        types.verification_success,
        "outputs",
        Type::Id(types.verification_output_artifact),
    );
    assert_wit_list_field_type(
        resolve,
        types.verification_success,
        "diagnostics",
        Type::Id(types.diagnostic),
    );
    assert_wit_result(
        resolve,
        types.verification_result,
        types.verification_success,
        types.refusal,
    );
}

fn assert_provider_wit_world_imports(
    resolve: &Resolve,
    world: &World,
    protocol_id: wit_parser::InterfaceId,
    expected: &[(&str, TypeId)],
) {
    assert_eq!(
        world.imports.len(),
        expected.len() + 1,
        "unexpected WIT world imports: {:#?}",
        world.imports
    );
    let protocol_import = world
        .imports
        .get(&WorldKey::Interface(protocol_id))
        .expect("WIT world missing protocol interface import");
    let WorldItem::Interface { id, .. } = protocol_import else {
        panic!("WIT protocol import must be an interface");
    };
    assert_eq!(*id, protocol_id);
    assert!(resolve.interfaces[*id].functions.is_empty());
    for (name, expected_type) in expected {
        let item = world
            .imports
            .get(&WorldKey::Name((*name).to_owned()))
            .unwrap_or_else(|| panic!("WIT world missing type import `{name}`"));
        let WorldItem::Type { id, .. } = item else {
            panic!("WIT world import `{name}` must be a type");
        };
        assert_eq!(
            wit_unaliased_type(resolve, Type::Id(*id)),
            Type::Id(*expected_type)
        );
    }
}

fn assert_provider_wit_worlds(resolve: &Resolve, package: &Package, types: &ProviderWitTypes) {
    let protocol_id = *package
        .interfaces
        .get("protocol")
        .expect("provider WIT protocol interface");
    for (world_name, function_name, request_name, request_id, result_name, result_id) in [
        (
            "lowerer",
            "lower",
            "lowering-request-v1",
            types.lowering_request,
            "lowering-result-v1",
            types.lowering_result,
        ),
        (
            "verifier",
            "verify",
            "verification-request-v1",
            types.verification_request,
            "verification-result-v1",
            types.verification_result,
        ),
    ] {
        let world_id = *package
            .worlds
            .get(world_name)
            .unwrap_or_else(|| panic!("provider WIT missing `{world_name}` world"));
        let world = &resolve.worlds[world_id];
        assert_provider_wit_world_imports(
            resolve,
            world,
            protocol_id,
            &[(request_name, request_id), (result_name, result_id)],
        );
        assert!(
            world.includes.is_empty(),
            "`{world_name}` includes another world"
        );
        assert_eq!(world.exports.len(), 1, "unexpected `{world_name}` exports");
        let function = wit_world_function(world, function_name);
        assert_eq!(function.kind, FunctionKind::Freestanding);
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.params[0].name, "request");
        assert_eq!(
            wit_unaliased_type(resolve, function.params[0].ty),
            Type::Id(request_id)
        );
        assert_eq!(
            function
                .result
                .map(|result| wit_unaliased_type(resolve, result)),
            Some(Type::Id(result_id))
        );
    }
}

#[test]
fn provider_wit_declares_explicit_lowering_envelope() {
    let root = repo_root().expect("repo root");
    let mut resolve = Resolve::default();
    let (package_id, _) = resolve
        .push_path(root.join("docs/abi/edict-target-provider.wit"))
        .expect("provider WIT must parse and resolve");
    let package = &resolve.packages[package_id];

    assert_eq!(package.name.to_string(), "edict:target-provider@1.0.0");
    let protocol_id = *package
        .interfaces
        .get("protocol")
        .expect("provider WIT protocol interface");
    let protocol = &resolve.interfaces[protocol_id];
    let types = ProviderWitTypes::resolve(protocol);

    assert_provider_wit_package_shape(&resolve, package, protocol);
    assert_provider_wit_artifact_shapes(&resolve, &types);
    assert_provider_wit_invocation_shapes(&resolve, &types);
    assert_provider_wit_resource_bindings(&resolve, &types);
    assert_provider_wit_output_bindings(&resolve, &types);
    assert_provider_wit_diagnostic_bindings(&resolve, &types);
    assert_provider_wit_lowering_bindings(&resolve, &types);
    assert_provider_wit_verification_bindings(&resolve, &types);
    assert_provider_wit_worlds(&resolve, package, &types);
}

#[test]
fn legacy_target_profile_wit_is_not_provider_envelope() {
    let root = repo_root().expect("repo root");
    let mut resolve = Resolve::default();
    let (package_id, _) = resolve
        .push_path(root.join("fixtures/providers/wit/legacy-target-profile.wit"))
        .expect("legacy target-profile WIT must parse and resolve");
    let package = &resolve.packages[package_id];

    assert_eq!(resolve.packages.len(), 1);
    assert_eq!(package.name.to_string(), "edict:target-profile@1.0.0");
    assert_eq!(
        package
            .interfaces
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["artifacts"])
    );
    assert!(!package.interfaces.contains_key("protocol"));
    assert_eq!(
        package
            .worlds
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["lowerer", "verifier"])
    );

    let lowerer = &resolve.worlds[package.worlds["lowerer"]];
    assert_eq!(
        lowerer
            .exports
            .keys()
            .filter_map(|key| match key {
                WorldKey::Name(name) => Some(name.as_str()),
                WorldKey::Interface(_) => None,
            })
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "effect-signature",
            "footprint-compare",
            "cost-compare",
            "attach-guard",
            "lower",
        ])
    );
    let verifier = &resolve.worlds[package.worlds["verifier"]];
    assert!(verifier
        .exports
        .contains_key(&WorldKey::Name("verify".to_owned())));
}

#[test]
fn core_cddl_declares_v1_semantic_model() {
    let root = repo_root().expect("repo root");
    let cddl =
        fs::read_to_string(root.join("docs/abi/edict-core.cddl")).expect("Core schema exists");
    for needle in [
        "core-module =",
        "apiVersion: \"edict.core/v1\"",
        "core-type =",
        "core-intent =",
        "core-block =",
        "core-node =",
        "core-expr =",
        "core-predicate =",
        "core-fn-body =",
        "input-constraint =",
        "local-ref =",
        "alphaName:",
        "requiredOperationProfile:",
    ] {
        assert!(cddl.contains(needle), "Core CDDL missing {needle}");
    }
}

#[test]
fn core_cddl_has_no_digest_freeze_fields() {
    let root = repo_root().expect("repo root");
    let cddl =
        fs::read_to_string(root.join("docs/abi/edict-core.cddl")).expect("Core schema exists");
    for forbidden in [
        "coreDigest",
        "canonicalCoreDigest",
        "canonicalBytes",
        "goldenBytes",
        "exactDigest",
    ] {
        assert!(
            !cddl.contains(forbidden),
            "v0.2 Core schema must not freeze byte/hash field `{forbidden}`"
        );
    }
}

#[test]
fn core_schema_shape_fixtures_match_cddl() {
    let root = repo_root().expect("repo root");
    let cddl =
        fs::read_to_string(root.join("docs/abi/edict-core.cddl")).expect("Core schema exists");
    let fixture_root = root.join("fixtures/core/schema");
    let mut checked = 0usize;

    for expectation in [FixtureExpectation::Accept, FixtureExpectation::Reject] {
        let dir = fixture_root.join(expectation.dir_name());
        for entry in fs::read_dir(&dir).unwrap_or_else(|err| {
            panic!(
                "read Core schema fixture directory {}: {err}",
                dir.display()
            )
        }) {
            let path = entry.expect("fixture entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("fields") {
                continue;
            }
            let fixture = parse_core_schema_fixture(&path);
            assert_eq!(
                fixture.expectation,
                expectation,
                "{} expectation disagrees with directory name",
                path.display()
            );
            let shape = core_shape_fields(&cddl, &fixture.shape);
            let missing = shape
                .required
                .difference(&fixture.fields)
                .cloned()
                .collect::<BTreeSet<_>>();
            let unknown = fixture
                .fields
                .difference(&shape.allowed)
                .cloned()
                .collect::<BTreeSet<_>>();
            let valid = missing.is_empty() && unknown.is_empty();
            match expectation {
                FixtureExpectation::Accept => assert!(
                    valid,
                    "{} should be accepted; missing {:?}, unknown {:?}",
                    path.display(),
                    missing,
                    unknown
                ),
                FixtureExpectation::Reject => assert!(
                    !valid,
                    "{} should be rejected by schema shape validation",
                    path.display()
                ),
            }
            checked += 1;
        }
    }

    assert!(checked >= 5, "expected at least 5 Core schema fixtures");
}

#[derive(Debug)]
struct CoreShapeFields {
    allowed: BTreeSet<String>,
    required: BTreeSet<String>,
}

#[derive(Debug)]
struct CoreSchemaFixture {
    expectation: FixtureExpectation,
    fields: BTreeSet<String>,
    shape: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum FixtureExpectation {
    Accept,
    Reject,
}

impl FixtureExpectation {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Accept => "accepted",
            Self::Reject => "rejected",
        }
    }
}

fn core_shape_fields(cddl: &str, shape: &str) -> CoreShapeFields {
    let start = format!("{shape} = {{");
    let mut in_shape = false;
    let mut fields = CoreShapeFields {
        allowed: BTreeSet::new(),
        required: BTreeSet::new(),
    };
    for raw in cddl.lines() {
        let line = raw.split_once(';').map_or(raw, |(before, _)| before).trim();
        if !in_shape {
            if line == start {
                in_shape = true;
            }
            continue;
        }
        if line.starts_with('}') {
            break;
        }
        let Some((field, _)) = line
            .strip_suffix(',')
            .unwrap_or(line)
            .strip_prefix("? ")
            .unwrap_or(line)
            .split_once(':')
        else {
            continue;
        };
        let field = field.trim();
        if field.is_empty() || field.contains(char::is_whitespace) {
            continue;
        }
        fields.allowed.insert(field.to_owned());
        if !line.starts_with("? ") {
            fields.required.insert(field.to_owned());
        }
    }
    assert!(
        !fields.allowed.is_empty(),
        "no fields parsed for Core shape {shape}"
    );
    fields
}

fn parse_core_schema_fixture(path: &Path) -> CoreSchemaFixture {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read Core schema fixture {}: {err}", path.display()));
    let mut expectation = None;
    let mut shape = None;
    let mut fields = BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(' ') else {
            panic!("{} has malformed line `{line}`", path.display());
        };
        match key {
            "expect" => {
                expectation = Some(match value {
                    "accept" => FixtureExpectation::Accept,
                    "reject" => FixtureExpectation::Reject,
                    other => panic!("{} has invalid expectation `{other}`", path.display()),
                });
            }
            "field" => {
                fields.insert(value.to_owned());
            }
            "shape" => {
                shape = Some(value.to_owned());
            }
            other => panic!("{} has unknown directive `{other}`", path.display()),
        }
    }
    CoreSchemaFixture {
        expectation: expectation
            .unwrap_or_else(|| panic!("{} missing expectation", path.display())),
        fields,
        shape: shape.unwrap_or_else(|| panic!("{} missing shape", path.display())),
    }
}

fn temp_root(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("edict-xtask-{name}-{}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).expect("temp root");
    dir
}

#[test]
fn release_date_reconciliation_rejects_missing_published_tags() {
    let root = repo_root().expect("repo root");
    let policy =
        fs::read_to_string(root.join("docs/topics/release-process/policy.toml")).expect("policy");
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("changelog");
    let blocks = crate::release_dates::parse_release_policy_blocks(&policy);
    let tags: BTreeMap<_, _> = blocks
        .iter()
        .filter(|block| block.status.as_deref() == Some("published"))
        .map(|block| {
            (
                block.tag.clone().expect("tag"),
                annotated_tag(block.target_date.as_deref().expect("date")),
            )
        })
        .collect();
    let notes: BTreeMap<_, _> = blocks
        .iter()
        .map(|block| (block.tag.clone().expect("tag"), block.target_date.clone()))
        .collect();
    let complete =
        crate::release_dates::reconcile_release_dates(&tags, &policy, &changelog, &notes);
    assert!(complete.drift.is_empty(), "{:?}", complete.drift);
    for removed in tags.keys() {
        let mut remaining = tags.clone();
        remaining.remove(removed);
        assert!(!remaining.is_empty());
        let report =
            crate::release_dates::reconcile_release_dates(&remaining, &policy, &changelog, &notes);
        assert_eq!(
            report.drift.len(),
            1,
            "removed {removed}: {:?}",
            report.drift
        );
        let finding = &report.drift[0];
        assert_eq!(finding.kind, ReleaseDateFindingKind::MissingSurface);
        assert_eq!(finding.surface, Surface::Tag);
        assert_eq!(&finding.tag, removed);
        assert_eq!(finding.expected, None);
        assert_eq!(finding.actual, None);
        assert_eq!(
            report.gaps,
            Vec::<crate::release_dates::ReleaseDateFinding>::new()
        );
    }
}

#[test]
fn release_date_reconciliation_allows_untagged_preparation() {
    for status in ["prep", "planned"] {
        let policy = COMPLETED_NEXT_RELEASE_POLICY
            .replace("status = \"prep\"", &format!("status = \"{status}\""));
        let report = crate::release_dates::reconcile_release_dates(
            &BTreeMap::new(),
            &policy,
            "",
            &BTreeMap::new(),
        );
        assert!(report.drift.is_empty(), "{status}: {:?}", report.drift);
        assert_eq!(
            report.gaps,
            Vec::<crate::release_dates::ReleaseDateFinding>::new()
        );
    }
}

#[test]
fn release_policy_list_presence_requires_actual_assignments() {
    for fields in [
        "# scope = [\n# non_goals = [\n",
        "explanation = \"scope = [ non_goals = [\"\n",
        "scope = \"scope = [\"\nnon_goals = \"non_goals = [\"\n",
        "explanation = '''\nscope = [\nnon_goals = [\n'''\n",
    ] {
        let policy = format!(
            "[release_notes.v0_8_0_alpha_1]\ntag = \"v0.8.0-alpha.1\"\ntarget_date = \"2026-06-28\"\nstatus = \"published\"\n{fields}"
        );
        let report = crate::release_dates::reconcile_release_dates(
            &BTreeMap::from([("v0.8.0-alpha.1".to_owned(), annotated_tag("2026-06-28"))]),
            &policy,
            "## [v0.8.0-alpha.1] - 2026-06-28\n",
            &BTreeMap::from([("v0.8.0-alpha.1".to_owned(), Some("2026-06-28".to_owned()))]),
        );
        let fields: Vec<_> = report
            .drift
            .iter()
            .map(|finding| {
                assert_eq!(finding.kind, ReleaseDateFindingKind::MissingSurface);
                assert_eq!(finding.tag, "v0.8.0-alpha.1");
                finding.surface
            })
            .collect();
        assert_eq!(fields, vec![Surface::PolicyScope, Surface::PolicyNonGoals]);
        assert_eq!(
            report.gaps,
            Vec::<crate::release_dates::ReleaseDateFinding>::new()
        );
    }
}

#[test]
fn release_policy_lists_accept_toml_assignment_spacing() {
    let root = repo_root().expect("root");
    let policy = fs::read_to_string(root.join("docs/topics/release-process/policy.toml"))
        .expect("policy")
        .replace("scope = [", "scope=[")
        .replace("non_goals = [", "'non_goals' = [");
    assert_release_policy_structure(&policy);
}
