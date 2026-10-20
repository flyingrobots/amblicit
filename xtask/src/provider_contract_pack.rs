use std::fs;
use std::path::Path;

use edict_provider_schema::{assemble_provider_contract_pack, ProviderContractPackInput};
use edict_syntax::canonical_target_profile_contract_resources;

use crate::goldens::{check_golden_file_with_command, write_golden_file};

const COMMON_CDDL: &str = "docs/abi/edict-common.cddl";
const CORE_CDDL: &str = "docs/abi/edict-core.cddl";
const LAWPACK_CDDL: &str = "docs/abi/edict-lawpack.cddl";
const LAWPACK_ADAPTER_CDDL: &str = "docs/abi/edict-lawpack-adapter.cddl";
const TARGET_PROFILE_CDDL: &str = "docs/abi/edict-target-profile.cddl";
const AUTHORITY_FACTS_CDDL: &str = "docs/abi/edict-authority-facts.cddl";
const RESULT_PROJECTION_CDDL: &str = "docs/abi/edict-result-projection.cddl";
const TARGET_IR_CDDL: &str = "docs/abi/edict-target-ir.cddl";

pub(crate) const CONTRACT_PACK_CDDL: &str =
    "fixtures/provider-contracts/source-modules-v1/edict-provider-contracts.cddl";
pub(crate) const CONTRACT_PACK_MANIFEST: &str =
    "fixtures/provider-contracts/source-modules-v1/manifest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderContractPackMode {
    Check,
    Write,
}

pub(crate) fn provider_contract_pack(
    root: &Path,
    mode: ProviderContractPackMode,
) -> Result<(), String> {
    let common_cddl = read(root, COMMON_CDDL)?;
    let core_cddl = read(root, CORE_CDDL)?;
    let lawpack_cddl = read(root, LAWPACK_CDDL)?;
    let lawpack_adapter_cddl = read(root, LAWPACK_ADAPTER_CDDL)?;
    let target_profile_cddl = read(root, TARGET_PROFILE_CDDL)?;
    let authority_facts_cddl = read(root, AUTHORITY_FACTS_CDDL)?;
    let result_projection_cddl = read(root, RESULT_PROJECTION_CDDL)?;
    let target_ir_cddl = read(root, TARGET_IR_CDDL)?;
    let source_closure_cddl = read(root, "docs/abi/edict-source-closure.cddl")?;
    let pack = assemble_provider_contract_pack(ProviderContractPackInput {
        common_cddl: &common_cddl,
        core_cddl: &core_cddl,
        lawpack_cddl: &lawpack_cddl,
        lawpack_adapter_cddl: &lawpack_adapter_cddl,
        target_profile_cddl: &target_profile_cddl,
        authority_facts_cddl: &authority_facts_cddl,
        result_projection_cddl: &result_projection_cddl,
        target_ir_cddl: &target_ir_cddl,
        source_closure_cddl: Some(&source_closure_cddl),
        contract_resources: canonical_target_profile_contract_resources(),
    })
    .map_err(|failures| format!("assemble provider contract pack: {failures:?}"))?;
    let manifest = pack.manifest_bytes();
    serde_json::from_slice::<serde_json::Value>(&manifest)
        .map_err(|error| format!("parse generated provider contract-pack manifest: {error}"))?;

    match mode {
        ProviderContractPackMode::Check => {
            check_golden_file_with_command(
                root,
                CONTRACT_PACK_CDDL,
                pack.cddl_bytes(),
                "cargo xtask provider-contract-pack --write",
            )?;
            check_golden_file_with_command(
                root,
                CONTRACT_PACK_MANIFEST,
                &manifest,
                "cargo xtask provider-contract-pack --write",
            )?;
        }
        ProviderContractPackMode::Write => {
            write_golden_file(&root.join(CONTRACT_PACK_CDDL), pack.cddl_bytes())?;
            write_golden_file(&root.join(CONTRACT_PACK_MANIFEST), &manifest)?;
        }
    }

    println!(
        "provider-contract-pack: {} schema and manifest",
        match mode {
            ProviderContractPackMode::Check => "checked",
            ProviderContractPackMode::Write => "written",
        }
    );
    Ok(())
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let path = root.join(relative);
    fs::read(&path).map_err(|err| format!("read {}: {err}", path.display()))
}
