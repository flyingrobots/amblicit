//! Deterministic document rendering for the standalone example command.
use super::limits::{LimitError, Limits};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    Usage,
    InvalidNumber,
    InvalidDigest,
    Limits(LimitError),
    Encoding,
}

pub fn render(arguments: &[&str]) -> Result<String, CommandError> {
    match arguments {
        ["lawpack", key, value, bytes] => {
            let document = super::document::document(parameters(key, value, bytes)?)
                .map_err(CommandError::Limits)?;
            serde_json::to_string_pretty(&document).map_err(|_| CommandError::Encoding)
        }
        ["source", key, value, bytes, digest] => {
            let limits = parameters(key, value, bytes)?;
            limits.budgets().map_err(CommandError::Limits)?;
            if !digest.strip_prefix("sha256:").is_some_and(|hex| {
                hex.len() == 64
                    && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }) {
                return Err(CommandError::InvalidDigest);
            }
            Ok(source(limits, digest))
        }
        ["application"] => serde_json::to_string_pretty(&json!({
            "schema": "edict.application/v1", "coordinate": "examples.configurable_cell@1",
            "sources": ["src/create.edict"],
            "lawpacks": [{"manifest": "vendor/cell/manifest.cbor", "exports": "vendor/cell/exports.cbor",
                "adapter": "vendor/cell/adapter.cbor", "targetConfiguration": "vendor/cell/configuration.cbor"}],
            "target": {"profile": "echo.dpo@1", "providerPackage": "provider"},
            "outputDirectory": ".build/application"
        })).map_err(|_| CommandError::Encoding),
        _ => Err(CommandError::Usage),
    }
}

fn parameters(key: &str, value: &str, bytes: &str) -> Result<Limits, CommandError> {
    let parse = |text: &str| text.parse().map_err(|_| CommandError::InvalidNumber);
    Ok(Limits {
        key_scalars: parse(key)?,
        value_scalars: parse(value)?,
        replacement_bytes: parse(bytes)?,
    })
}

fn source(limits: Limits, digest: &str) -> String {
    format!(
        r#"package examples.configurable_cell@1;

use lawpack example.cell@1 digest "{digest}" as cell;

type Created = {{
  key: String<max={key}>,
  value: String<max={value}>,
}};

intent create(input: cell.CreateInput) returns Created
  profile cell.createIfAbsent
  basis input.basis
  budget <= cell.createBudget
{{
  let receipt: cell.CreateReceipt = cell.createIfAbsent(input)
    else {{ alreadyExists(existing) => cell.AlreadyExists }};
  return {{ key: receipt.key, value: input.value }};
}}
"#,
        key = limits.key_scalars,
        value = limits.value_scalars,
    )
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Usage => "usage: configurable-causal-cell lawpack KEY_SCALARS VALUE_SCALARS VALUE_BYTES | source KEY_SCALARS VALUE_SCALARS VALUE_BYTES MANIFEST_DIGEST | application",
            Self::InvalidNumber => "limits must be unsigned 64-bit integers",
            Self::InvalidDigest => "manifest digest must be sha256: followed by 64 lowercase hexadecimal digits",
            Self::Limits(LimitError::Zero) => "all limits must be greater than zero",
            Self::Limits(LimitError::Overflow) => "limits overflow checked byte or budget arithmetic",
            Self::Limits(LimitError::ScalarBytesExceedCap) => "value scalar maximum requires more UTF-8 bytes than the replacement cap",
            Self::Encoding => "document JSON encoding failed",
        };
        formatter.write_str(message)
    }
}
