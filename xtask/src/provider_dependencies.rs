use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const WASMTIME_VERSION: &str = "48.0.7";

pub(crate) fn provider_runtime_dependencies(root: &Path) -> Result<(), String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(root)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--all-features",
        ])
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse cargo metadata: {error}"))?;
    check_metadata(&metadata)?;
    println!("provider-runtime-dependencies: Wasmtime boundary verified");
    Ok(())
}

fn check_metadata(metadata: &Value) -> Result<(), String> {
    let expected_requirement = format!("={WASMTIME_VERSION}");
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata missing packages".to_owned())?;
    if packages
        .iter()
        .any(|package| package.get("name").and_then(Value::as_str) == Some("wasmtime-wasi"))
    {
        return Err("wasmtime-wasi must remain outside the provider host closure".to_owned());
    }

    let wasmtime_packages = packages
        .iter()
        .filter(|package| package.get("name").and_then(Value::as_str) == Some("wasmtime"))
        .collect::<Vec<_>>();
    let [wasmtime] = wasmtime_packages.as_slice() else {
        return Err(format!(
            "expected exactly one Wasmtime package, found {}",
            wasmtime_packages.len()
        ));
    };
    if wasmtime.get("version").and_then(Value::as_str) != Some(WASMTIME_VERSION) {
        return Err(format!("Wasmtime must remain pinned to {WASMTIME_VERSION}"));
    }
    let wasmtime_id = required_string(wasmtime, "id")?;

    let workspace_members = string_set(
        metadata
            .get("workspace_members")
            .and_then(Value::as_array)
            .ok_or_else(|| "cargo metadata missing workspace_members".to_owned())?,
    )?;
    let mut declaring_packages = Vec::new();
    for package in packages {
        let package_id = required_string(package, "id")?;
        if !workspace_members.contains(package_id) {
            continue;
        }
        for dependency in package
            .get("dependencies")
            .and_then(Value::as_array)
            .ok_or_else(|| "cargo metadata package missing dependencies".to_owned())?
        {
            if dependency.get("name").and_then(Value::as_str) != Some("wasmtime") {
                continue;
            }
            let package_name = required_string(package, "name")?;
            declaring_packages.push(package_name.to_owned());
            if package_name != "edict-provider-host-wasmtime" {
                return Err(format!(
                    "workspace package `{package_name}` must not declare Wasmtime"
                ));
            }
            if dependency.get("req").and_then(Value::as_str) != Some(expected_requirement.as_str())
                || dependency
                    .get("uses_default_features")
                    .and_then(Value::as_bool)
                    != Some(false)
            {
                return Err(format!(
                    "provider host must pin Wasmtime {WASMTIME_VERSION} with default features disabled"
                ));
            }
            let direct_features = string_set(
                dependency
                    .get("features")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "Wasmtime dependency missing features".to_owned())?,
            )?;
            let expected = BTreeSet::from(["component-model", "cranelift", "runtime", "std"]);
            if direct_features != expected {
                return Err(format!(
                    "unexpected direct Wasmtime features: {direct_features:?}"
                ));
            }
        }
    }
    if declaring_packages != ["edict-provider-host-wasmtime"] {
        return Err(format!(
            "expected only the provider host to declare Wasmtime, found {declaring_packages:?}"
        ));
    }

    check_resolved_features(metadata, wasmtime_id)
}

fn check_resolved_features(metadata: &Value, wasmtime_id: &str) -> Result<(), String> {
    let nodes = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata missing resolve nodes".to_owned())?;
    let node = nodes
        .iter()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(wasmtime_id))
        .ok_or_else(|| "cargo metadata missing resolved Wasmtime node".to_owned())?;
    let resolved_features = string_set(
        node.get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| "resolved Wasmtime node missing features".to_owned())?,
    )?;
    let expected_resolved = BTreeSet::from([
        "component-model",
        "cranelift",
        "once_cell",
        "runtime",
        "std",
        "wasmtime-jit-icache-coherence",
    ]);
    if resolved_features != expected_resolved {
        return Err(format!(
            "unexpected resolved Wasmtime features: {resolved_features:?}"
        ));
    }
    Ok(())
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("cargo metadata missing string field `{field}`"))
}

fn string_set(values: &[Value]) -> Result<BTreeSet<&str>, String> {
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "cargo metadata string array contains a non-string".to_owned())
        })
        .collect()
}
