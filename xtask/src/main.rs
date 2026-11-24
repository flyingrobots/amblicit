#![allow(clippy::print_stderr)]
#![allow(clippy::print_stdout)]

mod contract_check;
mod effect_cost_cddl;
mod goldens;
mod lawpack_goldens;
mod provider_components;
mod provider_contract_pack;
mod provider_dependencies;
mod release_dates;
mod release_prep;
mod util;

#[cfg(test)]
mod tests;

use std::env;
use std::path::Path;
use std::process::ExitCode;

use contract_check::contract_check;
use goldens::{
    authority_facts_goldens, bundle_goldens, cli_goldens, core_goldens, target_ir_goldens,
    target_profile_resource_goldens, AuthorityFactsGoldenMode, BundleGoldenMode, CliGoldenMode,
    CoreGoldenMode, TargetIrGoldenMode, TargetProfileResourceGoldenMode,
};
use lawpack_goldens::{lawpack_goldens, LawpackGoldenMode};
use provider_components::{provider_component_fixtures, ProviderComponentFixtureMode};
use provider_contract_pack::{provider_contract_pack, ProviderContractPackMode};
use provider_dependencies::provider_runtime_dependencies;
use release_dates::release_dates;
use release_prep::release_prep;
use util::{diff_check_base, repo_root, run_cmd, run_cmd_slice};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("contract-check") => contract_check(&repo_root()?),
        Some("core-goldens") => {
            let mode = match args.next().as_deref() {
                Some("--write") => CoreGoldenMode::Write,
                Some("--check") | None => CoreGoldenMode::Check,
                Some(flag) => return Err(format!("unknown core-goldens flag `{flag}`")),
            };
            if let Some(extra) = args.next() {
                return Err(format!("unexpected core-goldens argument `{extra}`"));
            }
            core_goldens(&repo_root()?, mode)
        }
        Some("authority-facts-goldens") => run_authority_facts_goldens(&mut args),
        Some("target-profile-resource-goldens") => {
            run_target_profile_resource_goldens(&mut args)
        }
        Some("bundle-goldens") => {
            let mode = match args.next().as_deref() {
                Some("--write") => BundleGoldenMode::Write,
                Some("--check") | None => BundleGoldenMode::Check,
                Some(flag) => return Err(format!("unknown bundle-goldens flag `{flag}`")),
            };
            if let Some(extra) = args.next() {
                return Err(format!("unexpected bundle-goldens argument `{extra}`"));
            }
            bundle_goldens(&repo_root()?, mode)
        }
        Some("cli-goldens") => {
            let mode = match args.next().as_deref() {
                Some("--write") => CliGoldenMode::Write,
                Some("--check") | None => CliGoldenMode::Check,
                Some(flag) => return Err(format!("unknown cli-goldens flag `{flag}`")),
            };
            if let Some(extra) = args.next() {
                return Err(format!("unexpected cli-goldens argument `{extra}`"));
            }
            cli_goldens(&repo_root()?, mode)
        }
        Some("target-ir-goldens") => {
            let mode = match args.next().as_deref() {
                Some("--write") => TargetIrGoldenMode::Write,
                Some("--check") | None => TargetIrGoldenMode::Check,
                Some(flag) => return Err(format!("unknown target-ir-goldens flag `{flag}`")),
            };
            if let Some(extra) = args.next() {
                return Err(format!("unexpected target-ir-goldens argument `{extra}`"));
            }
            target_ir_goldens(&repo_root()?, mode)
        }
        Some("lawpack-goldens") => run_lawpack_goldens(&mut args),
        Some("release-prep") => run_release_prep(&mut args),
        Some("provider-component-fixtures") => {
            let mode = match args.next().as_deref() {
                Some("--write") => ProviderComponentFixtureMode::Write,
                Some("--check") | None => ProviderComponentFixtureMode::Check,
                Some(flag) => {
                    return Err(format!(
                        "unknown provider-component-fixtures flag `{flag}`"
                    ));
                }
            };
            if let Some(extra) = args.next() {
                return Err(format!(
                    "unexpected provider-component-fixtures argument `{extra}`"
                ));
            }
            provider_component_fixtures(&repo_root()?, mode)
        }
        Some("provider-contract-pack") => run_provider_contract_pack(&mut args),
        Some("provider-runtime-dependencies") => {
            if let Some(extra) = args.next() {
                return Err(format!(
                    "unexpected provider-runtime-dependencies argument `{extra}`"
                ));
            }
            provider_runtime_dependencies(&repo_root()?)
        }
        Some("release-dates") => run_release_dates(&mut args),
        Some("verify") => verify(&repo_root()?),
        Some(cmd) => Err(format!("unknown xtask command `{cmd}`")),
        None => Err(
            "usage: cargo xtask <verify|contract-check|release-dates|authority-facts-goldens|target-profile-resource-goldens|core-goldens|bundle-goldens|cli-goldens|target-ir-goldens|lawpack-goldens|provider-component-fixtures|provider-contract-pack|provider-runtime-dependencies|release-prep [--date YYYY-MM-DD]>"
                .into(),
        ),
    }
}

fn run_release_prep(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask release-prep <version> [--date YYYY-MM-DD]";
    let version = args.next().ok_or_else(|| USAGE.to_owned())?;
    let date = match args.next().as_deref() {
        None => None,
        Some("--date") => Some(
            args.next()
                .ok_or_else(|| "release-prep `--date` requires YYYY-MM-DD".to_owned())?,
        ),
        Some(flag) => return Err(format!("unknown release-prep flag `{flag}`; {USAGE}")),
    };
    if let Some(extra) = args.next() {
        return Err(format!("unexpected release-prep argument `{extra}`"));
    }
    release_prep(
        &repo_root()?,
        &version,
        date.as_deref(),
        std::time::SystemTime::now(),
    )
}

fn run_release_dates(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    match args.next().as_deref() {
        Some("--check") | None => {}
        Some(flag) => return Err(format!("unknown release-dates flag `{flag}`")),
    }
    if let Some(extra) = args.next() {
        return Err(format!("unexpected release-dates argument `{extra}`"));
    }
    release_dates(&repo_root()?)
}

fn run_authority_facts_goldens(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mode = match args.next().as_deref() {
        Some("--write") => AuthorityFactsGoldenMode::Write,
        Some("--check") | None => AuthorityFactsGoldenMode::Check,
        Some(flag) => return Err(format!("unknown authority-facts-goldens flag `{flag}`")),
    };
    if let Some(extra) = args.next() {
        return Err(format!(
            "unexpected authority-facts-goldens argument `{extra}`"
        ));
    }
    authority_facts_goldens(&repo_root()?, mode)
}

fn run_lawpack_goldens(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mode = match args.next().as_deref() {
        Some("--write") => LawpackGoldenMode::Write,
        Some("--check") | None => LawpackGoldenMode::Check,
        Some(flag) => return Err(format!("unknown lawpack-goldens flag `{flag}`")),
    };
    if let Some(extra) = args.next() {
        return Err(format!("unexpected lawpack-goldens argument `{extra}`"));
    }
    lawpack_goldens(&repo_root()?, mode)
}

fn run_target_profile_resource_goldens(
    args: &mut impl Iterator<Item = String>,
) -> Result<(), String> {
    let mode = match args.next().as_deref() {
        Some("--write") => TargetProfileResourceGoldenMode::Write,
        Some("--check") | None => TargetProfileResourceGoldenMode::Check,
        Some(flag) => {
            return Err(format!(
                "unknown target-profile-resource-goldens flag `{flag}`"
            ));
        }
    };
    if let Some(extra) = args.next() {
        return Err(format!(
            "unexpected target-profile-resource-goldens argument `{extra}`"
        ));
    }
    target_profile_resource_goldens(&repo_root()?, mode)
}

fn run_provider_contract_pack(args: &mut impl Iterator<Item = String>) -> Result<(), String> {
    let mode = match args.next().as_deref() {
        Some("--write") => ProviderContractPackMode::Write,
        Some("--check") | None => ProviderContractPackMode::Check,
        Some(flag) => return Err(format!("unknown provider-contract-pack flag `{flag}`")),
    };
    if let Some(extra) = args.next() {
        return Err(format!(
            "unexpected provider-contract-pack argument `{extra}`"
        ));
    }
    provider_contract_pack(&repo_root()?, mode)
}

fn verify(root: &Path) -> Result<(), String> {
    verify_rust_commands_with(root, run_cmd_slice)?;
    authority_facts_goldens(root, AuthorityFactsGoldenMode::Check)?;
    target_profile_resource_goldens(root, TargetProfileResourceGoldenMode::Check)?;
    core_goldens(root, CoreGoldenMode::Check)?;
    target_ir_goldens(root, TargetIrGoldenMode::Check)?;
    lawpack_goldens(root, LawpackGoldenMode::Check)?;
    bundle_goldens(root, BundleGoldenMode::Check)?;
    cli_goldens(root, CliGoldenMode::Check)?;
    provider_component_fixtures(root, ProviderComponentFixtureMode::Check)?;
    provider_contract_pack(root, ProviderContractPackMode::Check)?;
    provider_runtime_dependencies(root)?;
    contract_check(root)?;
    release_dates(root)?;
    let base = diff_check_base(root)?;
    run_cmd(root, "git", ["diff", "--check", &format!("{base}...HEAD")])?;
    Ok(())
}

fn verify_rust_commands_with(
    root: &Path,
    mut run: impl FnMut(&Path, &str, &[&str]) -> Result<(), String>,
) -> Result<(), String> {
    run(root, "cargo", &["fmt", "--all", "--check"])?;
    run(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(root, "cargo", &["test", "--workspace", "--all-features"])?;
    Ok(())
}
