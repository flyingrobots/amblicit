#![deny(clippy::expect_used, clippy::unwrap_used)]

mod application_build;
#[cfg(not(windows))]
mod lawpack_build;
#[cfg(windows)]
#[path = "lawpack_build_windows.rs"]
mod lawpack_build;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use edict_cli::{
    CHECK_RESULT_SCHEMA, COMPILER_INPUT_SCHEMA as INPUT_SCHEMA, DEFAULT_MAX_STDIN_BYTES,
    DIAGNOSTIC_SCHEMA, EVENT_SCHEMA, INFO_SCHEMA, MAX_STDIN_BYTES_ENV, PROJECTION_CORE_SCHEMA,
    PROJECTION_DIAGNOSTICS_SCHEMA, PROJECTION_SYNTAX_SCHEMA, PROJECTION_TARGET_IR_SCHEMA,
};
use edict_syntax::{
    CheckOutcome, CompilerContext, CompilerError, CompilerErrorKind, CompilerStage, CoreBlock,
    CoreBudget, CoreExpr, CoreImport, CoreIntent, CoreNode, CoreObstructionArm,
    CoreObstructionReason, CorePredicate, CoreRequireFailureArm, CoreType, CoreValue,
    HighlightRole, InputConstraint, InputConstraintSource, ParseError, ResourceRef, SemanticError,
    Span, TargetEffectLowering, TargetIrArtifact, TargetIrExternalActionRequest, TargetIrIntent,
    TargetIrLoweringFacts, TargetIrRequireFailure, TargetIrRequirement, TargetIrSemanticClosure,
    TargetIrStep, TargetLoweringFailure, TargetLoweringFailureKind, WriteClass,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const COMMAND_CHECK: &str = "check";
const COMMAND_BUILD: &str = "build";
const COMMAND_PROJECT: &str = "project";
const EXIT_OK: i32 = 0;
const EXIT_CHECK_FAILED: i32 = 1;
const EXIT_CLI_FAILED: i32 = 2;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompilerSettings {
    schema: String,
    #[serde(rename = "type")]
    record_type: String,
    operation: Operation,
    application: Option<PathBuf>,
    lawpack: Option<PathBuf>,
    #[serde(default)]
    check_only: bool,
    #[serde(default)]
    emit: Vec<ProjectionEmit>,
    compiler_context: Option<ProjectionCompilerContext>,
    target: Option<ProjectionTargetSettings>,
    input_root: Option<PathBuf>,
    #[serde(default = "default_directory_extensions")]
    directory_extensions: Vec<String>,
    #[serde(default)]
    follow_symlinks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Operation {
    Build,
    Check,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ProjectionEmit {
    Syntax,
    Diagnostics,
    Core,
    TargetIr,
    Digests,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionCompilerContext {
    #[serde(default)]
    operation_profiles: Vec<ProjectionOperationProfileFact>,
    #[serde(default)]
    effect_write_classes: Vec<ProjectionEffectWriteClassFact>,
    #[serde(default)]
    budgets: Vec<ProjectionBudgetFact>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionOperationProfileFact {
    source: String,
    core: String,
    #[serde(default)]
    allowed_write_classes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionEffectWriteClassFact {
    effect: String,
    write_class: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionBudgetFact {
    source: String,
    budget: ProjectionBudget,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionBudget {
    #[serde(rename = "maxSteps")]
    steps: u64,
    #[serde(rename = "maxAllocatedBytes")]
    allocated_bytes: u64,
    #[serde(rename = "maxOutputBytes")]
    output_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionTargetSettings {
    coordinate: String,
    profile_digest: String,
    ir_domain: String,
    #[serde(default)]
    operation_profiles: Vec<String>,
    #[serde(default)]
    obstruction_coordinates: Vec<String>,
    #[serde(default)]
    effect_lowerings: Vec<ProjectionTargetEffectLowering>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionTargetEffectLowering {
    effect: String,
    target_intrinsic: String,
}

#[derive(Debug, Clone)]
struct Request {
    settings: CompilerSettings,
    inputs: Vec<CompilerInput>,
}

#[derive(Debug, Clone)]
enum CompilerInput {
    Source { name: String, source: String },
    Path { path: PathBuf },
    PathList { paths: Vec<PathBuf> },
    Directory { path: PathBuf },
    Glob { pattern: String },
}

#[derive(Debug, Clone)]
struct SourceDocument {
    input: Value,
    source: String,
}

#[derive(Debug, Clone)]
struct CliFailure {
    command: &'static str,
    kind: &'static str,
    line: Option<usize>,
    message: String,
}

impl CliFailure {
    fn with_command(mut self, command: &'static str) -> Self {
        self.command = command;
        self
    }
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut args = std::env::args_os().skip(1);
    if let Some(first) = args.next() {
        let only_arg = args.next().is_none();
        match first.to_str() {
            Some("--help" | "-h") if only_arg => {
                write_info(&help_record());
                return EXIT_OK;
            }
            Some("--version" | "-V") if only_arg => {
                write_info(&version_record());
                return EXIT_OK;
            }
            _ => {
                let failure = CliFailure {
                    command: COMMAND_CHECK,
                    kind: "InvalidArguments",
                    line: None,
                    message: "edict reads JSONL request records on stdin and takes no positional \
                              arguments; run `edict --help` for the request schema, or see \
                              docs/topics/cli/README.md"
                        .to_owned(),
                };
                write_cli_failure(&failure);
                return EXIT_CLI_FAILED;
            }
        }
    }

    let input = match read_stdin_bounded() {
        Ok(input) => input,
        Err(failure) => {
            write_cli_failure(&failure);
            return EXIT_CLI_FAILED;
        }
    };

    match parse_request(&input) {
        Ok(request) => match run_request(&request) {
            Ok(exit_code) => exit_code,
            Err(failure) => {
                write_cli_failure(&failure);
                EXIT_CLI_FAILED
            }
        },
        Err(failure) => {
            write_cli_failure(&failure);
            EXIT_CLI_FAILED
        }
    }
}

fn read_stdin_bounded() -> Result<String, CliFailure> {
    let limit = configured_stdin_limit()?;
    let max_read = limit.checked_add(1).ok_or_else(|| CliFailure {
        command: COMMAND_CHECK,
        kind: "InvalidStdinLimit",
        line: None,
        message: format!("{MAX_STDIN_BYTES_ENV} must be below usize::MAX"),
    })?;
    let max_read = u64::try_from(max_read).map_err(|_| CliFailure {
        command: COMMAND_CHECK,
        kind: "InvalidStdinLimit",
        line: None,
        message: format!("{MAX_STDIN_BYTES_ENV} exceeds the supported byte limit"),
    })?;
    let mut bytes = Vec::new();
    io::stdin()
        .take(max_read)
        .read_to_end(&mut bytes)
        .map_err(|err| CliFailure {
            command: COMMAND_CHECK,
            kind: "StdinRead",
            line: None,
            message: err.to_string(),
        })?;
    if bytes.len() > limit {
        return Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "InputTooLarge",
            line: None,
            message: format!("stdin exceeds the configured maximum of {limit} bytes"),
        });
    }
    String::from_utf8(bytes).map_err(|err| CliFailure {
        command: COMMAND_CHECK,
        kind: "StdinRead",
        line: None,
        message: err.to_string(),
    })
}

fn configured_stdin_limit() -> Result<usize, CliFailure> {
    match std::env::var(MAX_STDIN_BYTES_ENV) {
        Ok(raw) => {
            let limit = raw.parse::<usize>().map_err(|_| CliFailure {
                command: COMMAND_CHECK,
                kind: "InvalidStdinLimit",
                line: None,
                message: format!("{MAX_STDIN_BYTES_ENV} must be a positive byte count"),
            })?;
            if limit == 0 {
                return Err(CliFailure {
                    command: COMMAND_CHECK,
                    kind: "InvalidStdinLimit",
                    line: None,
                    message: format!("{MAX_STDIN_BYTES_ENV} must be a positive byte count"),
                });
            }
            Ok(limit)
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_MAX_STDIN_BYTES),
        Err(std::env::VarError::NotUnicode(_)) => Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidStdinLimit",
            line: None,
            message: format!("{MAX_STDIN_BYTES_ENV} must be valid UTF-8"),
        }),
    }
}

fn parse_request(input: &str) -> Result<Request, CliFailure> {
    if input.trim().is_empty() {
        return Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "EmptyInput",
            line: None,
            message: "stdin must contain at least one JSONL record".to_owned(),
        });
    }

    let mut settings = None;
    let mut request_command = COMMAND_CHECK;
    let mut inputs = Vec::new();
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            return Err(CliFailure {
                command: request_command,
                kind: "BlankLine",
                line: Some(line_number),
                message: "JSONL input must not contain blank lines".to_owned(),
            });
        }
        let value = serde_json::from_str::<Value>(line).map_err(|err| CliFailure {
            command: request_command,
            kind: "InvalidJsonl",
            line: Some(line_number),
            message: err.to_string(),
        })?;
        let object = value.as_object().ok_or_else(|| CliFailure {
            command: request_command,
            kind: "InvalidRecord",
            line: Some(line_number),
            message: "each JSONL record must be a JSON object".to_owned(),
        })?;
        let schema = object
            .get("schema")
            .and_then(Value::as_str)
            .ok_or_else(|| CliFailure {
                command: request_command,
                kind: "InvalidRecord",
                line: Some(line_number),
                message: "JSONL record missing string field `schema`".to_owned(),
            })?;

        match schema {
            edict_cli::COMPILER_SETTINGS_SCHEMA => {
                if settings.is_some() {
                    return Err(CliFailure {
                        command: request_command,
                        kind: "DuplicateSettings",
                        line: Some(line_number),
                        message: "request may contain only one compiler settings record".to_owned(),
                    });
                }
                let parsed_settings = parse_settings(value, line_number)?;
                request_command = command_for_operation(parsed_settings.operation);
                settings = Some(parsed_settings);
            }
            INPUT_SCHEMA => inputs.push(
                parse_compiler_input(object, line_number)
                    .map_err(|failure| failure.with_command(request_command))?,
            ),
            _ => {
                return Err(CliFailure {
                    command: request_command,
                    kind: "InvalidRecord",
                    line: Some(line_number),
                    message: format!("unsupported JSONL schema `{schema}`"),
                });
            }
        }
    }

    let settings = settings.ok_or_else(|| CliFailure {
        command: request_command,
        kind: "MissingSettings",
        line: None,
        message: "request missing compiler settings record".to_owned(),
    })?;
    if settings.operation == Operation::Build && !inputs.is_empty() {
        return Err(CliFailure {
            command: command_for_operation(settings.operation),
            kind: "InvalidInputRecord",
            line: None,
            message: "build requests must not contain compiler input records".to_owned(),
        });
    }
    if inputs.is_empty() && settings.operation != Operation::Build {
        return Err(CliFailure {
            command: command_for_operation(settings.operation),
            kind: "MissingInput",
            line: None,
            message: "request must contain at least one compiler input record".to_owned(),
        });
    }

    Ok(Request { settings, inputs })
}

fn parse_settings(value: Value, line: usize) -> Result<CompilerSettings, CliFailure> {
    let command = settings_value_command(&value);
    if let Some(field) = null_compiler_settings_field(&value) {
        return Err(CliFailure {
            command,
            kind: "InvalidSettings",
            line: Some(line),
            message: format!("compiler settings {field} must not be null"),
        });
    }
    if command == COMMAND_BUILD {
        if let Some(field) = forbidden_build_settings_field(&value) {
            return Err(CliFailure {
                command,
                kind: "InvalidSettings",
                line: Some(line),
                message: format!("build does not accept the `{field}` setting"),
            });
        }
        if value.get("application").is_some() && value.get("checkOnly").is_some() {
            return Err(CliFailure {
                command,
                kind: "InvalidSettings",
                line: Some(line),
                message: "application builds do not accept the `checkOnly` setting".to_owned(),
            });
        }
    } else if let Some(field) = forbidden_nonbuild_settings_field(&value) {
        return Err(CliFailure {
            command,
            kind: "InvalidSettings",
            line: Some(line),
            message: format!("{command} does not accept the `{field}` setting"),
        });
    }
    let settings = serde_json::from_value::<CompilerSettings>(value).map_err(|err| CliFailure {
        command,
        kind: "InvalidSettings",
        line: Some(line),
        message: err.to_string(),
    })?;
    let command = command_for_operation(settings.operation);
    if settings.schema != edict_cli::COMPILER_SETTINGS_SCHEMA {
        return Err(CliFailure {
            command,
            kind: "InvalidSettings",
            line: Some(line),
            message: "compiler settings schema field does not match the settings schema".to_owned(),
        });
    }
    let expected_record_type = "compilerSettings";
    if settings.record_type != expected_record_type {
        return Err(CliFailure {
            command,
            kind: "InvalidSettings",
            line: Some(line),
            message: format!(
                "compiler settings record type for `{command}` must be `{expected_record_type}`"
            ),
        });
    }
    validate_operation_settings(&settings, line)?;
    if settings.directory_extensions.iter().any(|ext| {
        ext.len() < 2
            || !ext.starts_with('.')
            || !ext
                .chars()
                .skip(1)
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    }) {
        return Err(CliFailure {
            command,
            kind: "InvalidSettings",
            line: Some(line),
            message: "directoryExtensions entries must be dotted ASCII extensions".to_owned(),
        });
    }
    Ok(settings)
}

fn forbidden_build_settings_field(value: &Value) -> Option<&'static str> {
    [
        "emit",
        "compilerContext",
        "target",
        "inputRoot",
        "directoryExtensions",
        "followSymlinks",
    ]
    .into_iter()
    .find(|field| value.get(field).is_some())
}

fn forbidden_nonbuild_settings_field(value: &Value) -> Option<&'static str> {
    ["application", "lawpack", "checkOnly"]
        .into_iter()
        .find(|field| value.get(field).is_some())
}

fn null_compiler_settings_field(value: &Value) -> Option<&'static str> {
    [
        "application",
        "lawpack",
        "checkOnly",
        "inputRoot",
        "compilerContext",
        "target",
    ]
    .into_iter()
    .find(|field| value.get(field).is_some_and(Value::is_null))
}

fn settings_value_command(value: &Value) -> &'static str {
    value
        .get("operation")
        .and_then(Value::as_str)
        .and_then(command_for_operation_name)
        .unwrap_or(COMMAND_CHECK)
}

fn command_for_operation(operation: Operation) -> &'static str {
    match operation {
        Operation::Build => COMMAND_BUILD,
        Operation::Check => COMMAND_CHECK,
        Operation::Project => COMMAND_PROJECT,
    }
}

fn command_for_operation_name(operation: &str) -> Option<&'static str> {
    match operation {
        COMMAND_BUILD => Some(COMMAND_BUILD),
        COMMAND_CHECK => Some(COMMAND_CHECK),
        COMMAND_PROJECT => Some(COMMAND_PROJECT),
        _ => None,
    }
}

fn validate_operation_settings(settings: &CompilerSettings, line: usize) -> Result<(), CliFailure> {
    match settings.operation {
        Operation::Build => {
            if settings.application.is_some() == settings.lawpack.is_some() {
                return Err(CliFailure {
                    command: COMMAND_BUILD,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "build requires exactly one of `application` or `lawpack`".to_owned(),
                });
            }
            if settings
                .lawpack
                .as_ref()
                .is_some_and(|path| path.as_os_str().is_empty())
            {
                return Err(CliFailure {
                    command: COMMAND_BUILD,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "lawpack build document path must not be empty".to_owned(),
                });
            }
            if settings.check_only && settings.lawpack.is_none() {
                return Err(CliFailure {
                    command: COMMAND_BUILD,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "`checkOnly` is valid only for a lawpack build".to_owned(),
                });
            }
            if !settings.emit.is_empty()
                || settings.compiler_context.is_some()
                || settings.target.is_some()
                || settings.input_root.is_some()
                || settings.follow_symlinks
                || settings.directory_extensions != default_directory_extensions()
            {
                return Err(CliFailure {
                    command: COMMAND_BUILD,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "build accepts one build document plus optional lawpack `checkOnly`"
                        .to_owned(),
                });
            }
        }
        Operation::Check => {
            if settings.application.is_some() || settings.lawpack.is_some() || settings.check_only {
                return Err(CliFailure {
                    command: COMMAND_CHECK,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "`application`, `lawpack`, and `checkOnly` are build-only settings"
                        .to_owned(),
                });
            }
            if !settings.emit.is_empty()
                || settings.compiler_context.is_some()
                || settings.target.is_some()
            {
                return Err(CliFailure {
                    command: COMMAND_CHECK,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "`emit`, `compilerContext`, and `target` are project-only settings"
                        .to_owned(),
                });
            }
        }
        Operation::Project => {
            if settings.application.is_some() || settings.lawpack.is_some() || settings.check_only {
                return Err(CliFailure {
                    command: COMMAND_PROJECT,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "`application`, `lawpack`, and `checkOnly` are build-only settings"
                        .to_owned(),
                });
            }
            if settings.emit.is_empty() {
                return Err(CliFailure {
                    command: COMMAND_PROJECT,
                    kind: "InvalidSettings",
                    line: Some(line),
                    message: "project operation requires a non-empty `emit` list".to_owned(),
                });
            }
            if let Some(target) = &settings.target {
                validate_project_target(target, line)?;
            }
        }
    }
    Ok(())
}

fn validate_project_target(
    target: &ProjectionTargetSettings,
    line: usize,
) -> Result<(), CliFailure> {
    if target.coordinate.is_empty() {
        return Err(CliFailure {
            command: COMMAND_PROJECT,
            kind: "InvalidSettings",
            line: Some(line),
            message: "project target coordinate must not be empty".to_owned(),
        });
    }
    if !is_lowercase_sha256_digest(&target.profile_digest) {
        return Err(CliFailure {
            command: COMMAND_PROJECT,
            kind: "InvalidSettings",
            line: Some(line),
            message: "project target profileDigest must match sha256:<64 lowercase hex>".to_owned(),
        });
    }
    if target.ir_domain.is_empty() {
        return Err(CliFailure {
            command: COMMAND_PROJECT,
            kind: "InvalidSettings",
            line: Some(line),
            message: "project target irDomain must not be empty".to_owned(),
        });
    }
    Ok(())
}

fn is_lowercase_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn parse_compiler_input(
    object: &serde_json::Map<String, Value>,
    line: usize,
) -> Result<CompilerInput, CliFailure> {
    require_string_field(object, "type", line).and_then(|record_type| {
        if record_type == "compilerInput" {
            Ok(())
        } else {
            Err(CliFailure {
                command: COMMAND_CHECK,
                kind: "InvalidInputRecord",
                line: Some(line),
                message: "compiler input record type must be `compilerInput`".to_owned(),
            })
        }
    })?;

    let kind = require_string_field(object, "kind", line)?;
    let allowed: &[&str] = match kind {
        "source" => &["name", "source"],
        "path" | "directory" => &["path"],
        "pathList" => &["paths"],
        "glob" => &["pattern"],
        _ => {
            return Err(CliFailure {
                command: COMMAND_CHECK,
                kind: "InvalidInputRecord",
                line: Some(line),
                message: format!("unsupported compiler input kind `{kind}`"),
            });
        }
    };
    // Match the published `edict.compiler.input/v1` schema, which pins
    // `additionalProperties: false` and mutually exclusive input kinds: reject
    // any field outside the envelope and this kind's own variant fields so the
    // binary accepts exactly what the schema accepts.
    reject_foreign_input_fields(object, kind, allowed, line)?;
    match kind {
        "source" => Ok(CompilerInput::Source {
            name: optional_string_field(object, "name")
                .unwrap_or_else(|| "inline.edict".to_owned()),
            source: require_string_field(object, "source", line)?.to_owned(),
        }),
        "path" => Ok(CompilerInput::Path {
            path: PathBuf::from(require_string_field(object, "path", line)?),
        }),
        "pathList" => {
            let paths = object
                .get("paths")
                .and_then(Value::as_array)
                .ok_or_else(|| CliFailure {
                    command: COMMAND_CHECK,
                    kind: "InvalidInputRecord",
                    line: Some(line),
                    message: "pathList input records require an array field `paths`".to_owned(),
                })?
                .iter()
                .map(|value| {
                    value.as_str().map(PathBuf::from).ok_or_else(|| CliFailure {
                        command: COMMAND_CHECK,
                        kind: "InvalidInputRecord",
                        line: Some(line),
                        message: "pathList `paths` entries must be strings".to_owned(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(CompilerInput::PathList { paths })
        }
        "directory" => Ok(CompilerInput::Directory {
            path: PathBuf::from(require_string_field(object, "path", line)?),
        }),
        "glob" => Ok(CompilerInput::Glob {
            pattern: require_string_field(object, "pattern", line)?.to_owned(),
        }),
        _ => Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidInputRecord",
            line: Some(line),
            message: format!("unsupported compiler input kind `{kind}`"),
        }),
    }
}

fn reject_foreign_input_fields(
    object: &serde_json::Map<String, Value>,
    kind: &str,
    allowed: &[&str],
    line: usize,
) -> Result<(), CliFailure> {
    for key in object.keys() {
        let key = key.as_str();
        if matches!(key, "schema" | "type" | "kind") || allowed.contains(&key) {
            continue;
        }
        return Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidInputRecord",
            line: Some(line),
            message: format!("`{kind}` compiler input record has unexpected field `{key}`"),
        });
    }
    Ok(())
}

fn require_string_field<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    line: usize,
) -> Result<&'a str, CliFailure> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidInputRecord",
            line: Some(line),
            message: format!("record missing string field `{key}`"),
        })
}

fn optional_string_field(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn run_request(request: &Request) -> Result<i32, CliFailure> {
    let command = command_for_operation(request.settings.operation);
    if request.settings.operation == Operation::Build {
        return run_build_request(&request.settings);
    }
    let sources = expand_inputs(&request.settings, &request.inputs)
        .map_err(|failure| failure.with_command(command))?;
    if sources.is_empty() {
        return Err(CliFailure {
            command,
            kind: "MissingInput",
            line: None,
            message: "request did not expand to any source inputs".to_owned(),
        });
    }
    match request.settings.operation {
        Operation::Build => unreachable!("build requests return before input expansion"),
        Operation::Check => Ok(run_check_request(&sources)),
        Operation::Project => run_project_request(&request.settings, &sources),
    }
}

fn run_build_request(settings: &CompilerSettings) -> Result<i32, CliFailure> {
    if let Some(path) = &settings.application {
        if let Err(failure) = application_build::build_application(path) {
            if let Some(diagnostics) = &failure.source_diagnostics {
                write_application_source_failure(&failure, diagnostics);
                return Ok(EXIT_CLI_FAILED);
            }
            return Err(CliFailure {
                command: COMMAND_BUILD,
                kind: failure.kind,
                line: None,
                message: failure.message,
            });
        }
    } else if let Some(path) = &settings.lawpack {
        lawpack_build::build_lawpack(path, settings.check_only).map_err(|failure| CliFailure {
            command: COMMAND_BUILD,
            kind: failure.kind,
            line: None,
            message: failure.message,
        })?;
    } else {
        return Err(CliFailure {
            command: COMMAND_BUILD,
            kind: "InvalidSettings",
            line: None,
            message: "build requires exactly one of `application` or `lawpack`".to_owned(),
        });
    }
    let mut stdout = io::stdout().lock();
    write_record(
        &mut stdout,
        &status_record(COMMAND_BUILD, "ok", 1, 0, EXIT_OK),
    );
    Ok(EXIT_OK)
}

fn run_check_request(sources: &[SourceDocument]) -> i32 {
    let report = check_sources(sources);
    if report.diagnostics.is_empty() {
        let mut stdout = io::stdout().lock();
        write_records(&mut stdout, &report.results);
        let status = status_record(COMMAND_CHECK, "ok", sources.len(), 0, EXIT_OK);
        write_record(&mut stdout, &status);
        EXIT_OK
    } else {
        let mut stderr = io::stderr().lock();
        write_records(&mut stderr, &report.diagnostics);
        let status = status_record(
            COMMAND_CHECK,
            "error",
            report.results.len(),
            report.diagnostics.len(),
            EXIT_CHECK_FAILED,
        );
        write_record(&mut stderr, &status);
        EXIT_CHECK_FAILED
    }
}

fn run_project_request(
    settings: &CompilerSettings,
    sources: &[SourceDocument],
) -> Result<i32, CliFailure> {
    let compiler_context = project_compiler_context(settings)?;
    let mut stdout = io::stdout().lock();
    let mut errors = 0usize;
    for document in sources {
        let report = project_source(settings, &compiler_context, document)?;
        errors += report.errors;
        write_records(&mut stdout, &report.records);
    }
    let status = status_record(COMMAND_PROJECT, "ok", sources.len(), errors, EXIT_OK);
    write_record(&mut stdout, &status);
    Ok(EXIT_OK)
}

#[derive(Debug)]
struct ProjectionReport {
    records: Vec<Value>,
    errors: usize,
}

fn project_source(
    settings: &CompilerSettings,
    compiler_context: &CompilerContext,
    document: &SourceDocument,
) -> Result<ProjectionReport, CliFailure> {
    let emit = &settings.emit;
    let mut records = Vec::new();
    let mut diagnostics = Vec::new();

    if emit.contains(&ProjectionEmit::Syntax) {
        match syntax_projection_record(document) {
            Ok(record) => records.push(record),
            Err(error) => diagnostics.push(lex_diagnostic_item(&error)),
        }
    }

    let needs_authoritative_projection = emit.contains(&ProjectionEmit::Diagnostics)
        || emit.contains(&ProjectionEmit::Core)
        || emit.contains(&ProjectionEmit::TargetIr)
        || emit.contains(&ProjectionEmit::Digests);
    let core = if needs_authoritative_projection {
        let module = match edict_syntax::parse_module(&document.source) {
            Ok(module) => Some(module),
            Err(error) => {
                diagnostics.push(parse_diagnostic_item(&error));
                None
            }
        };
        module.as_ref().and_then(|module| {
            match edict_syntax::compile_to_core(module, compiler_context) {
                Ok(core) => Some(core),
                Err(errors) => {
                    diagnostics.extend(errors.iter().map(compiler_diagnostic_item));
                    None
                }
            }
        })
    } else {
        None
    };

    let diagnostics_would_be_hidden = !diagnostics.is_empty()
        && !emit.contains(&ProjectionEmit::Diagnostics)
        && !emit.contains(&ProjectionEmit::Core)
        && !emit.contains(&ProjectionEmit::TargetIr)
        && !emit.contains(&ProjectionEmit::Digests);
    if emit.contains(&ProjectionEmit::Diagnostics) || diagnostics_would_be_hidden {
        records.push(projection_diagnostics_record(document, &diagnostics));
    }

    if emit.contains(&ProjectionEmit::Core) || emit.contains(&ProjectionEmit::Digests) {
        records.push(core_projection_record(
            document,
            core.as_ref(),
            &diagnostics,
        )?);
    }

    if emit.contains(&ProjectionEmit::TargetIr) || emit.contains(&ProjectionEmit::Digests) {
        records.push(target_ir_projection_record(
            settings,
            document,
            core.as_ref(),
            &diagnostics,
        )?);
    }

    Ok(ProjectionReport {
        errors: diagnostics.len(),
        records,
    })
}

fn syntax_projection_record(document: &SourceDocument) -> Result<Value, edict_syntax::LexError> {
    let spans = edict_syntax::highlight_source(&document.source)?
        .into_iter()
        .map(|token| {
            json!({
                "role": highlight_role_name(token.role),
                "span": span_value(token.span),
                "lexeme": token.lexeme(&document.source),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "schema": PROJECTION_SYNTAX_SCHEMA,
        "type": "syntax",
        "command": COMMAND_PROJECT,
        "input": document.input,
        "spans": spans,
    }))
}

fn projection_diagnostics_record(document: &SourceDocument, diagnostics: &[Value]) -> Value {
    json!({
        "schema": PROJECTION_DIAGNOSTICS_SCHEMA,
        "type": "diagnostics",
        "command": COMMAND_PROJECT,
        "input": document.input,
        "diagnostics": diagnostics,
    })
}

fn core_projection_record(
    document: &SourceDocument,
    core: Option<&edict_syntax::CoreModule>,
    diagnostics: &[Value],
) -> Result<Value, CliFailure> {
    let Some(core) = core else {
        return Ok(json!({
            "schema": PROJECTION_CORE_SCHEMA,
            "type": "core",
            "command": COMMAND_PROJECT,
            "input": document.input,
            "state": "blocked",
            "reason": diagnostics,
        }));
    };
    let digest = edict_syntax::digest_core_module(core).map_err(|err| CliFailure {
        command: COMMAND_PROJECT,
        kind: "CoreDigest",
        line: None,
        message: format!("{:?}", err.kind()),
    })?;
    Ok(json!({
        "schema": PROJECTION_CORE_SCHEMA,
        "type": "core",
        "command": COMMAND_PROJECT,
        "input": document.input,
        "state": "available",
        "digest": digest.to_review_string(),
        "review": core_review(core),
    }))
}

fn target_ir_projection_record(
    settings: &CompilerSettings,
    document: &SourceDocument,
    core: Option<&edict_syntax::CoreModule>,
    diagnostics: &[Value],
) -> Result<Value, CliFailure> {
    let Some(core) = core else {
        return Ok(json!({
            "schema": PROJECTION_TARGET_IR_SCHEMA,
            "type": "targetIr",
            "command": COMMAND_PROJECT,
            "input": document.input,
            "state": "blocked",
            "reason": diagnostics,
        }));
    };
    let Some(target) = &settings.target else {
        return Ok(target_ir_failed_record(
            document,
            None,
            json!({
                "kind": "missing_target_profile",
                "message": "project target settings are required for Target IR projection",
            }),
        ));
    };
    let facts = project_target_facts(target);
    let report = edict_syntax::lower_to_target_ir(core, &facts);
    let Some(artifact) = report.artifact else {
        return Ok(target_ir_failed_record(
            document,
            Some(target),
            json!({
                "kind": "lowering_error",
                "failures": report
                    .failures
                    .iter()
                    .map(target_lowering_failure_review)
                    .collect::<Vec<_>>(),
            }),
        ));
    };
    let digest = edict_syntax::digest_target_ir_artifact(&artifact).map_err(|err| CliFailure {
        command: COMMAND_PROJECT,
        kind: "TargetIrDigest",
        line: None,
        message: format!("{:?}", err.kind()),
    })?;
    Ok(json!({
        "schema": PROJECTION_TARGET_IR_SCHEMA,
        "type": "targetIr",
        "command": COMMAND_PROJECT,
        "input": document.input,
        "state": "available",
        "domain": artifact.domain,
        "target": resource_ref_review(&artifact.target_profile),
        "digest": digest.to_review_string(),
        "review": target_ir_review(&artifact),
    }))
}

fn target_ir_failed_record(
    document: &SourceDocument,
    target: Option<&ProjectionTargetSettings>,
    error: Value,
) -> Value {
    let mut record = json!({
        "schema": PROJECTION_TARGET_IR_SCHEMA,
        "type": "targetIr",
        "command": COMMAND_PROJECT,
        "input": document.input,
        "state": "failed",
    });
    if let Some(object) = record.as_object_mut() {
        object.insert("error".to_owned(), error);
        if let Some(target) = target {
            object.insert("domain".to_owned(), Value::String(target.ir_domain.clone()));
            object.insert(
                "target".to_owned(),
                resource_ref_review(&ResourceRef {
                    coordinate: target.coordinate.clone(),
                    digest: Some(target.profile_digest.clone()),
                }),
            );
        }
    }
    record
}

fn project_compiler_context(settings: &CompilerSettings) -> Result<CompilerContext, CliFailure> {
    let Some(raw) = &settings.compiler_context else {
        return Ok(CompilerContext::new());
    };
    let mut context = CompilerContext::new();
    for profile in &raw.operation_profiles {
        let write_classes = profile
            .allowed_write_classes
            .iter()
            .map(|value| parse_write_class(value))
            .collect::<Result<Vec<_>, _>>()?;
        context = context
            .with_operation_profile(profile.source.clone(), profile.core.clone())
            .with_operation_profile_write_classes(profile.source.clone(), write_classes);
    }
    for effect in &raw.effect_write_classes {
        context = context.with_effect_write_class(
            effect.effect.clone(),
            parse_write_class(&effect.write_class)?,
        );
    }
    for budget in &raw.budgets {
        context = context.with_budget(
            budget.source.clone(),
            CoreBudget {
                max_steps: budget.budget.steps,
                max_allocated_bytes: budget.budget.allocated_bytes,
                max_output_bytes: budget.budget.output_bytes,
            },
        );
    }
    Ok(context)
}

fn project_target_facts(target: &ProjectionTargetSettings) -> TargetIrLoweringFacts {
    TargetIrLoweringFacts {
        target_profile: ResourceRef {
            coordinate: target.coordinate.clone(),
            digest: Some(target.profile_digest.clone()),
        },
        target_ir_domain: target.ir_domain.clone(),
        operation_profiles: target.operation_profiles.clone(),
        obstruction_coordinates: target.obstruction_coordinates.clone(),
        effect_lowerings: target
            .effect_lowerings
            .iter()
            .map(|lowering| TargetEffectLowering {
                effect: lowering.effect.clone(),
                target_intrinsic: lowering.target_intrinsic.clone(),
                failure_mappings: std::collections::BTreeMap::new(),
            })
            .collect(),
        effect_signatures: Vec::new(),
        pure_functions: Vec::new(),
    }
}

fn parse_write_class(value: &str) -> Result<WriteClass, CliFailure> {
    match value {
        "none" => Ok(WriteClass::None),
        "read" => Ok(WriteClass::Read),
        "create" => Ok(WriteClass::Create),
        "ensure" => Ok(WriteClass::Ensure),
        "append" => Ok(WriteClass::Append),
        "replace" => Ok(WriteClass::Replace),
        "delete" => Ok(WriteClass::Delete),
        _ => Err(CliFailure {
            command: COMMAND_PROJECT,
            kind: "InvalidSettings",
            line: None,
            message: format!("unsupported write class `{value}`"),
        }),
    }
}

fn lex_diagnostic_item(error: &edict_syntax::LexError) -> Value {
    json!({
        "stage": "lex",
        "kind": "Lex",
        "severity": "error",
        "span": span_value(error.span),
        "message": error.message,
    })
}

fn parse_diagnostic_item(error: &ParseError) -> Value {
    json!({
        "stage": "parse",
        "kind": error.kind.code(),
        "severity": "error",
        "span": span_value(error.span),
    })
}

fn compiler_diagnostic_item(error: &CompilerError) -> Value {
    let mut record = json!({
        "stage": compiler_stage_name(error.stage),
        "kind": compiler_error_kind_name(error.kind),
        "severity": "error",
        "span": span_value(error.span),
    });
    if let Some(detail) = &error.signature_mismatch {
        record["signatureMismatch"] = json!(detail);
    }
    record
}

fn span_value(span: Span) -> Value {
    json!({
        "start": span.start,
        "end": span.end,
    })
}

fn highlight_role_name(role: HighlightRole) -> &'static str {
    match role {
        HighlightRole::Comment => "comment",
        HighlightRole::Identifier => "identifier",
        HighlightRole::Keyword => "keyword",
        HighlightRole::Number => "number",
        HighlightRole::Operator => "operator",
        HighlightRole::Punctuation => "punctuation",
        HighlightRole::String => "string",
        HighlightRole::TypeIdentifier => "typeIdentifier",
    }
}

fn compiler_stage_name(stage: CompilerStage) -> &'static str {
    match stage {
        CompilerStage::SurfaceValidation => "surfaceValidation",
        CompilerStage::Resolve => "resolve",
        CompilerStage::TypeCheck => "typeCheck",
        CompilerStage::LowerCore => "lowerCore",
    }
}

fn compiler_error_kind_name(kind: CompilerErrorKind) -> &'static str {
    match kind {
        CompilerErrorKind::SurfaceValidation => "SurfaceValidation",
        CompilerErrorKind::MissingContextFact => "MissingContextFact",
        CompilerErrorKind::UnsupportedSourceShape => "UnsupportedSourceShape",
        CompilerErrorKind::UnresolvedType => "UnresolvedType",
        CompilerErrorKind::UnresolvedFunction => "UnresolvedFunction",
        CompilerErrorKind::EffectWithoutFailureMapping => "EffectWithoutFailureMapping",
        CompilerErrorKind::InvalidBound => "InvalidBound",
        CompilerErrorKind::UnknownField => "UnknownField",
        CompilerErrorKind::TypeMismatch => "TypeMismatch",
        CompilerErrorKind::ExpectedPredicate => "ExpectedPredicate",
        CompilerErrorKind::ProfileEffectMismatch => "ProfileEffectMismatch",
        CompilerErrorKind::UnrequestableExternalOperation => "UnrequestableExternalOperation",
        CompilerErrorKind::DuplicateObstructionFailure => "DuplicateObstructionFailure",
        CompilerErrorKind::DuplicateObstructionPayloadField => "DuplicateObstructionPayloadField",
        CompilerErrorKind::ReservedTypeIdentity => "ReservedTypeIdentity",
        CompilerErrorKind::InvalidCoreTypeIntegrity => "InvalidCoreTypeIntegrity",
    }
}

fn core_review(core: &edict_syntax::CoreModule) -> Value {
    let mut review = json!({
        "apiVersion": core.api_version,
        "coordinate": core.coordinate,
        "imports": core.imports.iter().map(core_import_review).collect::<Vec<_>>(),
        "types": core.types
            .iter()
            .map(|(name, ty)| (name.clone(), core_type_review(ty)))
            .collect::<BTreeMap<_, _>>(),
        "intents": core.intents
            .iter()
            .map(|(name, intent)| (name.clone(), core_intent_review(intent)))
            .collect::<BTreeMap<_, _>>(),
        "requiredCoreCapabilities": core.required_core_capabilities,
    });
    if !core.functions.is_empty() {
        review["functions"] = Value::Object(core.functions.iter().map(|(name, function)| (name.clone(), json!({
            "params": function.params.iter().map(local_ref_review).collect::<Vec<_>>(),
            "returnType": function.return_type,
            "body": {
                "locals": function.body.locals.iter().map(local_ref_review).collect::<Vec<_>>(),
                "bindings": function.body.bindings.iter().map(|binding| json!({
                    "kind": "let", "binding": local_ref_review(&binding.binding), "value": core_expr_review(&binding.value)
                })).collect::<Vec<_>>(),
                "result": core_expr_review(&function.body.result)
            }
        }))).collect());
    }
    review
}

fn core_intent_review(intent: &CoreIntent) -> Value {
    let mut review = json!({
        "input": intent.input,
        "output": intent.output,
        "requiredOperationProfile": intent.required_operation_profile,
        "inputConstraints": intent
            .input_constraints
            .iter()
            .map(input_constraint_review)
            .collect::<Vec<_>>(),
        "coreEvaluationBudget": core_budget_review(&intent.core_evaluation_budget),
        "body": core_block_review(&intent.body),
    });
    insert_optional_review_field(
        &mut review,
        "basis",
        intent.basis.as_ref().map(core_expr_review),
    );
    review
}

fn target_ir_review(artifact: &TargetIrArtifact) -> Value {
    let mut review = json!({
        "domain": artifact.domain,
        "targetProfile": resource_ref_review(&artifact.target_profile),
        "sourceCoreCoordinate": artifact.source_core_coordinate,
        "intents": artifact.intents
            .iter()
            .map(|(name, intent)| (name.clone(), target_ir_intent_review(intent)))
            .collect::<BTreeMap<_, _>>(),
    });
    insert_optional_review_field(
        &mut review,
        "semanticClosure",
        artifact
            .semantic_closure
            .as_ref()
            .map(target_ir_semantic_closure_review),
    );
    review
}

fn target_ir_semantic_closure_review(closure: &TargetIrSemanticClosure) -> Value {
    json!({
        "sourceCore": resource_ref_review(&closure.source_core),
        "lawpacks": closure
            .lawpacks
            .iter()
            .map(resource_ref_review)
            .collect::<Vec<_>>(),
        "capabilities": closure
            .capabilities
            .iter()
            .map(resource_ref_review)
            .collect::<Vec<_>>(),
    })
}

fn target_ir_intent_review(intent: &TargetIrIntent) -> Value {
    let mut review = json!({
        "operationProfile": intent.operation_profile,
        "inputConstraints": intent
            .input_constraints
            .iter()
            .map(input_constraint_review)
            .collect::<Vec<_>>(),
        "coreEvaluationBudget": core_budget_review(&intent.core_evaluation_budget),
        "requirements": intent
            .requirements
            .iter()
            .map(target_ir_requirement_review)
            .collect::<Vec<_>>(),
        "steps": intent.steps.iter().map(target_ir_step_review).collect::<Vec<_>>(),
        "externalActionRequests": intent
            .external_action_requests
            .iter()
            .map(target_ir_external_action_request_review)
            .collect::<Vec<_>>(),
        "result": core_expr_review(&intent.result),
    });
    insert_optional_review_field(
        &mut review,
        "basis",
        intent.basis.as_ref().map(core_expr_review),
    );
    insert_optional_review_field(
        &mut review,
        "executionOrder",
        intent.execution_order.as_ref().map(|order| json!(order)),
    );
    if !intent.pure_bindings.is_empty() {
        insert_optional_review_field(
            &mut review,
            "pureBindings",
            Some(json!(intent
                .pure_bindings
                .iter()
                .map(|binding| json!({
                    "id": binding.id,
                    "binding": local_ref_review(&binding.binding),
                    "value": core_expr_review(&binding.value),
                }))
                .collect::<Vec<_>>())),
        );
    }
    review
}

fn insert_optional_review_field(review: &mut Value, key: &str, value: Option<Value>) {
    if let (Value::Object(fields), Some(value)) = (review, value) {
        fields.insert(key.to_owned(), value);
    }
}

fn target_ir_requirement_review(requirement: &TargetIrRequirement) -> Value {
    json!({
        "id": requirement.id,
        "predicate": core_predicate_review(&requirement.predicate),
        "onFailure": target_ir_require_failure_review(&requirement.on_failure),
    })
}

fn target_ir_require_failure_review(failure: &TargetIrRequireFailure) -> Value {
    match failure {
        TargetIrRequireFailure::Terminal { reason } => json!({
            "kind": "terminal",
            "reason": core_obstruction_reason_review(reason),
        }),
        TargetIrRequireFailure::ContinueObstructed { reason } => json!({
            "kind": "continueObstructed",
            "reason": core_obstruction_reason_review(reason),
        }),
    }
}

fn target_ir_step_review(step: &TargetIrStep) -> Value {
    json!({
        "id": step.id,
        "binding": local_ref_review(&step.binding),
        "effect": step.effect,
        "targetIntrinsic": step.target_intrinsic,
        "input": core_expr_review(&step.input),
        "obstructionFailures": step.obstruction_failures,
        "obstructionArms": step.obstruction_arms
            .iter()
            .map(|(name, arm)| (name.clone(), obstruction_arm_review(arm)))
            .collect::<BTreeMap<_, _>>(),
    })
}

fn target_ir_external_action_request_review(request: &TargetIrExternalActionRequest) -> Value {
    json!({
        "id": request.id,
        "binding": local_ref_review(&request.binding),
        "operation": resource_ref_review(&request.operation),
        "inputType": request.input_type,
        "settlementType": request.settlement_type,
        "inputSchema": resource_ref_review(&request.input_schema),
        "settlementSchema": resource_ref_review(&request.settlement_schema),
        "input": core_expr_review(&request.input),
        "authorityScope": core_expr_review(&request.authority_scope),
        "basis": core_expr_review(&request.basis),
        "budget": {
            "maxSettlementBytes": core_expr_review(&request.budget.max_settlement_bytes),
            "maxAttempts": core_expr_review(&request.budget.max_attempts),
        },
        "reconciliationLaw": resource_ref_review(&request.reconciliation_law),
        "state": "awaitingSettlement",
        "settlementAdmission": "schemaRequired",
    })
}

fn core_import_review(import: &CoreImport) -> Value {
    json!({
        "kind": import.kind.as_str(),
        "resource": resource_ref_review(&import.resource),
        "alias": import.alias,
    })
}

fn resource_ref_review(resource: &ResourceRef) -> Value {
    json!({
        "coordinate": resource.coordinate,
        "digest": resource.digest,
    })
}

fn core_type_review(ty: &CoreType) -> Value {
    match ty {
        CoreType::Unit => json!({ "kind": "unit" }),
        CoreType::Bool => json!({ "kind": "bool" }),
        CoreType::Int { width } => json!({ "kind": "int", "width": width }),
        CoreType::String { max, canonical } => {
            json!({ "kind": "string", "max": max, "canonical": canonical })
        }
        CoreType::Bytes { min, max } => json!({ "kind": "bytes", "min": min, "max": max }),
        CoreType::Nominal {
            contract,
            representation,
        } => json!({
            "kind": "nominal",
            "contract": contract,
            "representation": representation,
        }),
        CoreType::Record { fields } => json!({ "kind": "record", "fields": fields }),
        CoreType::Variant { cases } => json!({ "kind": "variant", "cases": cases }),
        CoreType::Option { item } => json!({ "kind": "option", "item": item }),
        CoreType::List { item, max } => json!({ "kind": "list", "item": item, "max": max }),
        CoreType::Map { key, value, max } => {
            json!({ "kind": "map", "key": key, "value": value, "max": max })
        }
        CoreType::CapabilityRef { item } => json!({ "kind": "capabilityRef", "item": item }),
        CoreType::ExternalActionRequest { settlement } => {
            json!({ "kind": "externalActionRequest", "settlement": settlement })
        }
    }
}

fn input_constraint_review(constraint: &InputConstraint) -> Value {
    json!({
        "coordinate": constraint.coordinate,
        "source": input_constraint_source_name(constraint.source),
        "predicate": core_predicate_review(&constraint.predicate),
    })
}

fn input_constraint_source_name(source: InputConstraintSource) -> &'static str {
    match source {
        InputConstraintSource::Where => "where",
        InputConstraintSource::Compiler => "compiler",
    }
}

fn core_budget_review(budget: &CoreBudget) -> Value {
    json!({
        "maxSteps": budget.max_steps,
        "maxAllocatedBytes": budget.max_allocated_bytes,
        "maxOutputBytes": budget.max_output_bytes,
    })
}

fn core_block_review(block: &CoreBlock) -> Value {
    json!({
        "locals": block.locals.iter().map(local_ref_review).collect::<Vec<_>>(),
        "nodes": block.nodes.iter().map(core_node_review).collect::<Vec<_>>(),
        "result": core_expr_review(&block.result),
    })
}

fn core_node_review(node: &CoreNode) -> Value {
    match node {
        CoreNode::Let { binding, value } => json!({
            "kind": "let",
            "binding": local_ref_review(binding),
            "value": core_expr_review(value),
        }),
        CoreNode::Require { predicate, arm } => json!({
            "kind": "require",
            "predicate": core_predicate_review(predicate),
            "onFailure": core_require_failure_arm_review(arm),
        }),
        CoreNode::Effect {
            binding,
            effect,
            input,
            obstruction_map,
        } => json!({
            "kind": "effect",
            "binding": local_ref_review(binding),
            "effect": effect,
            "input": core_expr_review(input),
            "obstructionMap": obstruction_map
                .iter()
                .map(|(name, arm)| (name.clone(), obstruction_arm_review(arm)))
                .collect::<BTreeMap<_, _>>(),
        }),
        CoreNode::ExternalActionRequest {
            binding,
            operation,
            input_type,
            settlement_type,
            input_schema,
            settlement_schema,
            input,
            authority_scope,
            basis,
            budget,
            reconciliation_law,
        } => json!({
            "kind": "externalActionRequest",
            "binding": local_ref_review(binding),
            "operation": resource_ref_review(operation),
            "inputType": input_type,
            "settlementType": settlement_type,
            "inputSchema": resource_ref_review(input_schema),
            "settlementSchema": resource_ref_review(settlement_schema),
            "input": core_expr_review(input),
            "authorityScope": core_expr_review(authority_scope),
            "basis": core_expr_review(basis),
            "budget": {
                "maxSettlementBytes": core_expr_review(&budget.max_settlement_bytes),
                "maxAttempts": core_expr_review(&budget.max_attempts),
            },
            "reconciliationLaw": resource_ref_review(reconciliation_law),
            "state": "awaitingSettlement",
            "settlementAdmission": "schemaRequired",
        }),
        CoreNode::For {
            binder,
            iter,
            bound,
            body,
        } => json!({
            "kind": "for",
            "binder": local_ref_review(binder),
            "iter": core_expr_review(iter),
            "bound": match bound {
                edict_syntax::CoreBound::Literal(value) => json!({ "kind": "literal", "value": value }),
                edict_syntax::CoreBound::Coordinate(reference) => json!({ "kind": "coordinate", "ref": reference }),
            },
            "body": core_block_review(body),
        }),
        CoreNode::Branch {
            binding,
            predicate,
            then_block,
            else_block,
        } => {
            let mut review = json!({
                "kind": "branch",
                "predicate": core_predicate_review(predicate),
                "then": core_block_review(then_block),
                "else": core_block_review(else_block),
            });
            if let Some(binding) = binding {
                review["binding"] = local_ref_review(binding);
            }
            review
        }
    }
}

fn core_require_failure_arm_review(arm: &CoreRequireFailureArm) -> Value {
    match arm {
        CoreRequireFailureArm::Terminal { reason } => json!({
            "kind": "terminal",
            "reason": core_obstruction_reason_review(reason),
        }),
        CoreRequireFailureArm::ContinueObstructed { reason } => json!({
            "kind": "continueObstructed",
            "reason": core_obstruction_reason_review(reason),
        }),
    }
}

fn core_obstruction_reason_review(reason: &CoreObstructionReason) -> Value {
    json!({
        "reasonKind": reason.kind,
        "payload": reason
            .payload
            .iter()
            .map(|(name, expr)| (name.clone(), core_expr_review(expr)))
            .collect::<BTreeMap<_, _>>(),
    })
}

fn obstruction_arm_review(arm: &CoreObstructionArm) -> Value {
    json!({
        "binder": local_ref_review(&arm.binder),
        "value": core_expr_review(&arm.value),
    })
}

fn local_ref_review(reference: &edict_syntax::LocalRef) -> Value {
    json!({
        "id": reference.id,
        "alphaName": reference.alpha_name,
        "ty": reference.ty,
    })
}

fn core_expr_review(expr: &CoreExpr) -> Value {
    match expr {
        CoreExpr::Local { reference } => json!({
            "kind": "local",
            "reference": local_ref_review(reference),
        }),
        CoreExpr::Const(value) => json!({
            "kind": "const",
            "value": core_value_review(value),
        }),
        CoreExpr::Record { fields } => json!({
            "kind": "record",
            "fields": fields
                .iter()
                .map(|(name, value)| (name.clone(), core_expr_review(value)))
                .collect::<BTreeMap<_, _>>(),
        }),
        CoreExpr::List { values } => json!({
            "kind": "list",
            "values": values.iter().map(core_expr_review).collect::<Vec<_>>(),
        }),
        CoreExpr::Field { base, field } => json!({
            "kind": "field",
            "base": core_expr_review(base),
            "field": field,
        }),
        CoreExpr::Call {
            callee,
            type_args,
            args,
        } => json!({
            "kind": "call",
            "callee": callee,
            "typeArgs": type_args,
            "args": args.iter().map(core_expr_review).collect::<Vec<_>>(),
        }),
        CoreExpr::If {
            predicate,
            then_value,
            else_value,
        } => json!({
            "kind": "if",
            "predicate": core_predicate_review(predicate),
            "then": core_expr_review(then_value),
            "else": core_expr_review(else_value),
        }),
    }
}

fn core_value_review(value: &CoreValue) -> Value {
    match value {
        CoreValue::Null => json!({ "kind": "null" }),
        CoreValue::Bool(value) => json!({ "kind": "bool", "value": value }),
        CoreValue::Int { width, value } => {
            json!({ "kind": "int", "width": width, "value": value })
        }
        CoreValue::String(value) => json!({ "kind": "string", "value": value }),
        CoreValue::Bytes(value) => json!({ "kind": "bytes", "value": value }),
    }
}

fn core_predicate_review(predicate: &CorePredicate) -> Value {
    match predicate {
        CorePredicate::True => json!({ "kind": "true" }),
        CorePredicate::False => json!({ "kind": "false" }),
        CorePredicate::Not(value) => json!({
            "kind": "not",
            "value": core_predicate_review(value),
        }),
        CorePredicate::All(values) => json!({
            "kind": "all",
            "values": values.iter().map(core_predicate_review).collect::<Vec<_>>(),
        }),
        CorePredicate::Any(values) => json!({
            "kind": "any",
            "values": values.iter().map(core_predicate_review).collect::<Vec<_>>(),
        }),
        CorePredicate::Compare { op, left, right } => json!({
            "kind": "compare",
            "op": compare_op_name(*op),
            "left": core_expr_review(left),
            "right": core_expr_review(right),
        }),
    }
}

fn compare_op_name(op: edict_syntax::CompareOp) -> &'static str {
    match op {
        edict_syntax::CompareOp::Eq => "eq",
        edict_syntax::CompareOp::Ne => "ne",
        edict_syntax::CompareOp::Lt => "lt",
        edict_syntax::CompareOp::Le => "le",
        edict_syntax::CompareOp::Gt => "gt",
        edict_syntax::CompareOp::Ge => "ge",
    }
}

fn target_lowering_failure_review(failure: &TargetLoweringFailure) -> Value {
    json!({
        "kind": target_lowering_failure_kind_name(failure.kind),
        "intent": failure.intent,
        "nodeIndex": failure.node_index,
        "detail": failure.detail,
    })
}

fn target_lowering_failure_kind_name(kind: TargetLoweringFailureKind) -> &'static str {
    match kind {
        TargetLoweringFailureKind::UnsupportedTargetProfile => "UnsupportedTargetProfile",
        TargetLoweringFailureKind::UnsupportedTargetIrDomain => "UnsupportedTargetIrDomain",
        TargetLoweringFailureKind::UndigestedTargetProfile => "UndigestedTargetProfile",
        TargetLoweringFailureKind::UnsupportedTargetFeature => "UnsupportedTargetFeature",
        TargetLoweringFailureKind::UnsupportedCoreNode => "UnsupportedCoreNode",
        TargetLoweringFailureKind::MissingOperationProfile => "MissingOperationProfile",
        TargetLoweringFailureKind::MissingObstruction => "MissingObstruction",
        TargetLoweringFailureKind::AmbiguousObstructionMapping => "AmbiguousObstructionMapping",
        TargetLoweringFailureKind::MissingEffectLowering => "MissingEffectLowering",
        TargetLoweringFailureKind::AmbiguousEffectLowering => "AmbiguousEffectLowering",
        TargetLoweringFailureKind::UnsupportedLowerabilityReport => "UnsupportedLowerabilityReport",
        TargetLoweringFailureKind::UnsupportedTargetIntrinsic => "UnsupportedTargetIntrinsic",
        TargetLoweringFailureKind::UnsupportedCoreAbi => "UnsupportedCoreAbi",
        TargetLoweringFailureKind::UnsupportedCoreCapability => "UnsupportedCoreCapability",
        TargetLoweringFailureKind::UndigestedCoreImport => "UndigestedCoreImport",
        TargetLoweringFailureKind::InvalidCoreIdentity => "InvalidCoreIdentity",
        TargetLoweringFailureKind::NoTargetSteps => "NoTargetSteps",
    }
}

#[derive(Debug)]
struct CheckReport {
    results: Vec<Value>,
    diagnostics: Vec<Value>,
}

fn check_sources(sources: &[SourceDocument]) -> CheckReport {
    let mut results = Vec::new();
    let mut diagnostics = Vec::new();
    for document in sources {
        match edict_syntax::check(&document.source) {
            CheckOutcome::Valid => results.push(check_result_record(&document.input)),
            CheckOutcome::ParseFailed(error) => {
                diagnostics.push(parse_diagnostic(&document.input, &error));
            }
            CheckOutcome::SemanticFailed(errors) => diagnostics.extend(
                errors
                    .iter()
                    .map(|error| semantic_diagnostic(&document.input, error)),
            ),
        }
    }
    CheckReport {
        results,
        diagnostics,
    }
}

fn expand_inputs(
    settings: &CompilerSettings,
    inputs: &[CompilerInput],
) -> Result<Vec<SourceDocument>, CliFailure> {
    let input_root = canonical_input_root(settings)?;
    let input_root = input_root.as_deref();
    let mut sources = Vec::new();
    for input in inputs {
        match input {
            CompilerInput::Source { name, source } => sources.push(SourceDocument {
                input: json!({
                    "kind": "source",
                    "name": name,
                }),
                source: source.clone(),
            }),
            CompilerInput::Path { path } => {
                expand_path(settings, input_root, path, "path", &mut sources)?;
            }
            CompilerInput::PathList { paths } => {
                for path in paths {
                    expand_path(settings, input_root, path, "pathList", &mut sources)?;
                }
            }
            CompilerInput::Directory { path } => {
                expand_directory(settings, input_root, path, "directory", &mut sources)?;
            }
            CompilerInput::Glob { pattern } => expand_glob(pattern, input_root, &mut sources)?,
        }
    }
    Ok(sources)
}

fn canonical_input_root(settings: &CompilerSettings) -> Result<Option<PathBuf>, CliFailure> {
    let Some(root) = &settings.input_root else {
        return Ok(None);
    };
    let canonical =
        fs::canonicalize(root).map_err(|err| path_failure("InputRootRead", root, &err))?;
    let metadata =
        fs::metadata(&canonical).map_err(|err| path_failure("InputRootRead", &canonical, &err))?;
    if !metadata.is_dir() {
        return Err(CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidInputRoot",
            line: None,
            message: format!("inputRoot is not a directory: {}", root.display()),
        });
    }
    Ok(Some(canonical))
}

fn expand_path(
    settings: &CompilerSettings,
    input_root: Option<&Path>,
    path: &Path,
    origin: &str,
    sources: &mut Vec<SourceDocument>,
) -> Result<(), CliFailure> {
    let path = confined_input_path(input_root, path, "PathRead")?;
    let metadata = fs::metadata(&path).map_err(|err| path_failure("PathRead", &path, &err))?;
    if metadata.is_dir() {
        expand_directory(settings, input_root, &path, origin, sources)
    } else {
        read_source_file(&path, origin, input_root).map(|source| sources.push(source))
    }
}

fn expand_directory(
    settings: &CompilerSettings,
    input_root: Option<&Path>,
    path: &Path,
    origin: &str,
    sources: &mut Vec<SourceDocument>,
) -> Result<(), CliFailure> {
    let path = confined_input_path(input_root, path, "DirectoryRead")?;
    let mut files = Vec::new();
    collect_directory_files(settings, input_root, &path, &mut files)?;
    files.sort();
    for file in files {
        sources.push(read_source_file(&file, origin, input_root)?);
    }
    Ok(())
}

fn collect_directory_files(
    settings: &CompilerSettings,
    input_root: Option<&Path>,
    path: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), CliFailure> {
    let mut entries = fs::read_dir(path)
        .map_err(|err| path_failure("DirectoryRead", path, &err))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| CliFailure {
            command: COMMAND_CHECK,
            kind: "DirectoryRead",
            line: None,
            message: err.to_string(),
        })?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let metadata = if settings.follow_symlinks {
            fs::metadata(&path)
        } else {
            fs::symlink_metadata(&path)
        }
        .map_err(|err| path_failure("DirectoryRead", &path, &err))?;
        if metadata.file_type().is_symlink() && !settings.follow_symlinks {
            continue;
        }
        let path = confined_input_path(input_root, &path, "DirectoryRead")?;
        if metadata.is_dir() {
            collect_directory_files(settings, input_root, &path, files)?;
        } else if metadata.is_file() && directory_extension_matches(settings, &path) {
            files.push(path);
        }
    }
    Ok(())
}

fn expand_glob(
    pattern: &str,
    input_root: Option<&Path>,
    sources: &mut Vec<SourceDocument>,
) -> Result<(), CliFailure> {
    reject_glob_prefix_outside_root(pattern, input_root)?;
    let mut paths = glob::glob(pattern)
        .map_err(|err| CliFailure {
            command: COMMAND_CHECK,
            kind: "InvalidGlob",
            line: None,
            message: err.to_string(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| CliFailure {
            command: COMMAND_CHECK,
            kind: "GlobRead",
            line: None,
            message: err.to_string(),
        })?;
    paths.sort();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let path = confined_input_path(input_root, &path, "PathRead")?;
        sources.push(read_source_file(&path, "glob", input_root)?);
    }
    Ok(())
}

fn reject_glob_prefix_outside_root(
    pattern: &str,
    input_root: Option<&Path>,
) -> Result<(), CliFailure> {
    let Some(root) = input_root else {
        return Ok(());
    };
    let literal = pattern
        .find(['*', '?', '[', '{'])
        .map_or(pattern, |index| &pattern[..index]);
    let probe = if literal.is_empty() {
        Path::new(".")
    } else {
        let literal_path = Path::new(literal);
        if literal.ends_with(std::path::MAIN_SEPARATOR) || literal.ends_with('/') {
            literal_path
        } else {
            literal_path.parent().unwrap_or_else(|| Path::new("."))
        }
    };
    let Ok(canonical) = fs::canonicalize(probe) else {
        return Ok(());
    };
    if canonical.starts_with(root) {
        return Ok(());
    }
    Err(CliFailure {
        command: COMMAND_CHECK,
        kind: "InputPathOutsideRoot",
        line: None,
        message: format!("{pattern} resolves outside configured inputRoot"),
    })
}

fn read_source_file(
    path: &Path,
    origin: &str,
    input_root: Option<&Path>,
) -> Result<SourceDocument, CliFailure> {
    let path = confined_input_path(input_root, path, "PathRead")?;
    let source = fs::read_to_string(&path).map_err(|err| path_failure("PathRead", &path, &err))?;
    Ok(SourceDocument {
        input: json!({
            "kind": origin,
            "path": path.display().to_string(),
        }),
        source,
    })
}

fn confined_input_path(
    input_root: Option<&Path>,
    path: &Path,
    failure_kind: &'static str,
) -> Result<PathBuf, CliFailure> {
    let Some(root) = input_root else {
        return Ok(path.to_path_buf());
    };
    let canonical = fs::canonicalize(path).map_err(|err| path_failure(failure_kind, path, &err))?;
    if canonical.starts_with(root) {
        return Ok(canonical);
    }
    Err(CliFailure {
        command: COMMAND_CHECK,
        kind: "InputPathOutsideRoot",
        line: None,
        message: format!("{} resolves outside configured inputRoot", path.display()),
    })
}

fn directory_extension_matches(settings: &CompilerSettings, path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    settings
        .directory_extensions
        .iter()
        .any(|allowed| allowed.strip_prefix('.') == Some(extension))
}

fn path_failure(kind: &'static str, path: &Path, err: &io::Error) -> CliFailure {
    CliFailure {
        command: COMMAND_CHECK,
        kind,
        line: None,
        message: format!("{}: {err}", path.display()),
    }
}

fn check_result_record(input: &Value) -> Value {
    record_value(CheckResultRecord {
        command: COMMAND_CHECK,
        input,
        schema: CHECK_RESULT_SCHEMA,
        status: "ok",
        record_type: "checkResult",
    })
}

fn parse_diagnostic(input: &Value, error: &ParseError) -> Value {
    diagnostic_record(
        COMMAND_CHECK,
        "parse",
        error.kind.code(),
        input,
        Some(error.span),
        None,
        None,
    )
}

fn semantic_diagnostic(input: &Value, error: &SemanticError) -> Value {
    diagnostic_record(
        COMMAND_CHECK,
        "semantic",
        error.kind.code(),
        input,
        Some(error.span),
        None,
        None,
    )
}

fn cli_diagnostic(failure: &CliFailure) -> Value {
    diagnostic_record(
        failure.command,
        "cli",
        failure.kind,
        &json!({ "kind": "stdin" }),
        None,
        failure.line,
        Some(failure.message.as_str()),
    )
}

fn diagnostic_record(
    command: &'static str,
    stage: &str,
    kind: &str,
    input: &Value,
    span: Option<Span>,
    line: Option<usize>,
    message: Option<&str>,
) -> Value {
    record_value(DiagnosticRecord {
        command,
        input,
        kind,
        line,
        message,
        schema: DIAGNOSTIC_SCHEMA,
        severity: "error",
        span: span.map(|span| SpanRecord {
            start: span.start,
            end: span.end,
        }),
        stage,
        record_type: "diagnostic",
    })
}

fn status_record(
    command: &'static str,
    status: &str,
    checked: usize,
    errors: usize,
    exit_code: i32,
) -> Value {
    record_value(StatusRecord {
        checked,
        command,
        errors,
        exit_code,
        schema: EVENT_SCHEMA,
        status,
        record_type: "status",
    })
}

fn version_record() -> Value {
    record_value(VersionInfoRecord {
        schema: INFO_SCHEMA,
        topic: "version",
        record_type: "info",
        version: env!("CARGO_PKG_VERSION"),
    })
}

fn help_record() -> Value {
    record_value(HelpInfoRecord {
        docs: "docs/topics/cli/README.md",
        exit_codes: &[
            ExitCodeRecord {
                code: EXIT_OK,
                meaning: "request completed successfully",
            },
            ExitCodeRecord {
                code: EXIT_CHECK_FAILED,
                meaning: "check operation compiler or validation diagnostics were produced",
            },
            ExitCodeRecord {
                code: EXIT_CLI_FAILED,
                meaning: "CLI input or usage was invalid, or a build failed",
            },
        ],
        request_schemas: &[edict_cli::COMPILER_SETTINGS_SCHEMA, INPUT_SCHEMA],
        schema: INFO_SCHEMA,
        topic: "help",
        record_type: "info",
        usage: "edict reads JSONL request records on stdin and emits only JSONL records on \
                stdout and stderr; it takes no positional arguments. A request is one compiler \
                settings record, followed by compiler inputs only for check or project.",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckResultRecord<'a> {
    command: &'static str,
    input: &'a Value,
    schema: &'static str,
    status: &'static str,
    #[serde(rename = "type")]
    record_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticRecord<'a> {
    command: &'static str,
    input: &'a Value,
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    schema: &'static str,
    severity: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<SpanRecord>,
    stage: &'a str,
    #[serde(rename = "type")]
    record_type: &'static str,
}

#[derive(Serialize)]
struct SpanRecord {
    start: usize,
    end: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusRecord<'a> {
    checked: usize,
    command: &'static str,
    errors: usize,
    exit_code: i32,
    schema: &'static str,
    status: &'a str,
    #[serde(rename = "type")]
    record_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionInfoRecord {
    schema: &'static str,
    topic: &'static str,
    #[serde(rename = "type")]
    record_type: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HelpInfoRecord<'a> {
    docs: &'static str,
    exit_codes: &'a [ExitCodeRecord],
    request_schemas: &'a [&'static str],
    schema: &'static str,
    topic: &'static str,
    #[serde(rename = "type")]
    record_type: &'static str,
    usage: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ExitCodeRecord {
    code: i32,
    meaning: &'static str,
}

fn record_value(record: impl Serialize) -> Value {
    match serde_json::to_value(record) {
        Ok(value) => value,
        Err(error) => unreachable!("CLI record structs serialize to JSON values: {error}"),
    }
}

fn write_info(record: &Value) {
    let mut stdout = io::stdout().lock();
    write_record(&mut stdout, record);
}

fn write_application_source_failure(
    failure: &application_build::ApplicationBuildFailure,
    diagnostics: &[application_build::ApplicationSourceDiagnostics],
) {
    use application_build::ApplicationSourceError;

    let mut stderr = io::stderr().lock();
    for diagnostics in diagnostics {
        let path = diagnostics.path.display().to_string();
        let input = json!({ "kind": "path", "path": path });
        let positions = application_source_positions(&diagnostics.source, &diagnostics.errors);
        for (error, (line, column)) in diagnostics.errors.iter().zip(positions) {
            let (stage, kind, message) = match error {
                ApplicationSourceError::Parse(error) => {
                    ("parse", error.kind.code(), error.message.clone())
                }
                ApplicationSourceError::Compiler(error) => (
                    compiler_stage_name(error.stage),
                    compiler_error_kind_name(error.kind),
                    error.message.clone(),
                ),
                ApplicationSourceError::Linker(error) => {
                    use edict_syntax::source_linker::SourceLinkErrorKind;
                    let kind = match error.kind {
                        SourceLinkErrorKind::UnresolvedName => "UnresolvedName",
                        SourceLinkErrorKind::UnknownExport => "UnknownExport",
                        SourceLinkErrorKind::ImportedIntent => "ImportedIntent",
                        SourceLinkErrorKind::NameCollision => "NameCollision",
                        SourceLinkErrorKind::SpanOverflow => "SpanOverflow",
                        SourceLinkErrorKind::DepthExceeded => "DepthExceeded",
                        SourceLinkErrorKind::ReservedTypeIdentity => "ReservedTypeIdentity",
                    };
                    ("resolve", kind, error.to_string())
                }
            };
            let mut record = diagnostic_record(
                COMMAND_BUILD,
                stage,
                kind,
                &input,
                Some(error.span()),
                None,
                Some(&message),
            );
            record["sourceLocation"] = json!({ "path": path, "line": line, "column": column });
            if let ApplicationSourceError::Compiler(error) = error {
                if let Some(detail) = &error.signature_mismatch {
                    record["signatureMismatch"] = json!(detail);
                }
            }
            write_record(&mut stderr, &record);
        }
    }
    let summary = CliFailure {
        command: COMMAND_BUILD,
        kind: failure.kind,
        line: None,
        message: failure.message.clone(),
    };
    write_record(&mut stderr, &cli_diagnostic(&summary));
    write_record(
        &mut stderr,
        &status_record(
            COMMAND_BUILD,
            "error",
            0,
            diagnostics
                .iter()
                .map(|group| group.errors.len())
                .sum::<usize>()
                + 1,
            EXIT_CLI_FAILED,
        ),
    );
}

// Traverse source once in byte-offset order, then restore diagnostic order.
// This avoids rescanning the whole source for every error or indexing UTF-8.
fn application_source_positions(
    source: &str,
    errors: &[application_build::ApplicationSourceError],
) -> Vec<(usize, usize)> {
    let mut offsets = errors
        .iter()
        .enumerate()
        .map(|(ordinal, error)| (error.span().start, ordinal))
        .collect::<Vec<_>>();
    offsets.sort_unstable();
    let mut characters = source.char_indices().peekable();
    let mut line = 1;
    let mut column = 1;
    let mut positions = Vec::with_capacity(offsets.len());
    for (offset, ordinal) in offsets {
        while let Some(&(byte, character)) = characters.peek() {
            if byte >= offset {
                break;
            }
            characters.next();
            if character == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        positions.push((ordinal, line, column));
    }
    positions.sort_unstable_by_key(|(ordinal, _, _)| *ordinal);
    positions
        .into_iter()
        .map(|(_, line, column)| (line, column))
        .collect()
}

fn write_cli_failure(failure: &CliFailure) {
    let mut stderr = io::stderr().lock();
    let diagnostic = cli_diagnostic(failure);
    write_record(&mut stderr, &diagnostic);
    let status = status_record(failure.command, "error", 0, 1, EXIT_CLI_FAILED);
    write_record(&mut stderr, &status);
}

fn write_records(writer: &mut dyn Write, records: &[Value]) {
    for record in records {
        write_record(writer, record);
    }
}

fn write_record(writer: &mut dyn Write, record: &Value) {
    if serde_json::to_writer(&mut *writer, record).is_ok() {
        let _ = writer.write_all(b"\n");
    }
}

fn default_directory_extensions() -> Vec<String> {
    vec![".edict".to_owned()]
}

#[cfg(test)]
mod tests {
    use super::{core_type_review, CoreType};

    #[test]
    fn core_type_review_is_total_for_unit() {
        assert_eq!(
            core_type_review(&CoreType::Unit),
            serde_json::json!({ "kind": "unit" })
        );
    }
}
