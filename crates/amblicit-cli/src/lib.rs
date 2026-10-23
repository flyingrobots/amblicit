//! Edict CLI support crate.
//!
//! The `amblicit` binary owns the JSONL stream contract for compiler request,
//! result, status, and diagnostic records.

#![deny(clippy::expect_used, clippy::unwrap_used)]

/// Stable schema identifier for compiler settings records.
pub const COMPILER_SETTINGS_SCHEMA: &str = "edict.compiler.settings/v1";

/// Stable schema identifier for compiler input records.
pub const COMPILER_INPUT_SCHEMA: &str = "edict.compiler.input/v1";

/// Stable schema identifier for `check` success result records.
pub const CHECK_RESULT_SCHEMA: &str = "edict.cli.check-result/v1";

/// Stable schema identifier for syntax projection records.
pub const PROJECTION_SYNTAX_SCHEMA: &str = "edict.projection.syntax/v1";

/// Stable schema identifier for diagnostics projection records.
pub const PROJECTION_DIAGNOSTICS_SCHEMA: &str = "edict.projection.diagnostics/v1";

/// Stable schema identifier for Core projection records.
pub const PROJECTION_CORE_SCHEMA: &str = "edict.projection.core/v1";

/// Stable schema identifier for Target IR projection records.
pub const PROJECTION_TARGET_IR_SCHEMA: &str = "edict.projection.target-ir/v1";

/// Stable schema identifier for CLI diagnostic records.
pub const DIAGNOSTIC_SCHEMA: &str = "edict.cli.diagnostic/v1";

/// Stable schema identifier for CLI terminal status (event) records.
pub const EVENT_SCHEMA: &str = "edict.cli.event/v1";

/// Stable schema identifier for CLI informational (`--help`/`--version`) records.
pub const INFO_SCHEMA: &str = "edict.cli.info/v1";

/// Environment variable that overrides the maximum stdin byte count.
pub const MAX_STDIN_BYTES_ENV: &str = "EDICT_CLI_MAX_STDIN_BYTES";

/// Default maximum stdin byte count accepted before request parsing.
pub const DEFAULT_MAX_STDIN_BYTES: usize = 8 * 1024 * 1024;
