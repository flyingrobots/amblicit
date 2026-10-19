//! Explicit application sources. The first path owns entries; remaining paths
//! are supplied dependencies, never a filesystem discovery request.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use edict_syntax::ast::{ImportKind, Module};
use edict_syntax::source_closure::{
    resolve_source_closure, SourceClosure, SourceClosureLimits, SOURCE_CLOSURE_DIGEST_DOMAIN,
};
use edict_syntax::{encode_canonical_cbor, parse_module, CanonicalValue};

use super::{
    confined_existing_path, failure, read, source_failure, ApplicationBuildFailure,
    ApplicationSourceDiagnostics, ApplicationSourceError, SOURCE_DOMAIN,
};

pub(super) struct LoadedSources {
    pub root_path: PathBuf,
    pub root_source: String,
    pub root_module: Module,
    pub closure: Option<SourceClosure>,
    units: Vec<(PathBuf, String)>,
}

#[derive(Debug)]
pub(super) struct SourceArtifact {
    pub domain: &'static str,
    pub kind: &'static str,
    pub bytes: Vec<u8>,
}

impl LoadedSources {
    pub fn load(root: &Path, paths: &[PathBuf]) -> Result<Self, ApplicationBuildFailure> {
        let limits = SourceClosureLimits::default();
        let mut units = Vec::new();
        let mut total = 0_usize;
        let mut seen = BTreeSet::new();

        for relative in paths {
            let path = confined_existing_path(
                root,
                relative,
                "sources",
                "ApplicationSourceReadFailed",
                "Edict source",
            )?;
            if !seen.insert(path.clone()) {
                return Err(failure(
                    "DuplicateApplicationSource",
                    "application source paths must identify distinct files",
                ));
            }
            let bytes = read(&path, "ApplicationSourceReadFailed", "Edict source")?;
            total = total
                .checked_add(bytes.len())
                .filter(|total| *total <= limits.max_source_bytes)
                .ok_or_else(|| {
                    failure(
                        "ApplicationSourceBudgetExceeded",
                        "aggregate application source bytes exceed the fixed source-closure limit",
                    )
                })?;
            let source = String::from_utf8(bytes).map_err(|error| {
                failure(
                    "InvalidApplicationSource",
                    format!("Edict source `{}` is not UTF-8: {error}", path.display()),
                )
            })?;
            units.push((path, source));
        }
        if units.is_empty() {
            return Err(failure(
                "InvalidApplicationConfig",
                "application requires a root source",
            ));
        }
        let (root_path, root_source) = units[0].clone();
        let root_module = parse_module(&root_source).map_err(|error| {
            source_failure(
                "InvalidApplicationSource",
                "Edict application source did not parse",
                &root_path,
                &root_source,
                vec![ApplicationSourceError::Parse(error)],
            )
        })?;
        for (path, source) in &units[1..] {
            parse_module(source).map_err(|error| {
                source_failure(
                    "InvalidApplicationSource",
                    "Edict dependency source did not parse",
                    path,
                    source,
                    vec![ApplicationSourceError::Parse(error)],
                )
            })?;
        }
        let closure = if units.len() > 1
            || root_module
                .imports
                .iter()
                .any(|import| import.kind == ImportKind::Module)
        {
            let dependencies = units[1..]
                .iter()
                .map(|(_, source)| source.as_str())
                .collect::<Vec<_>>();
            Some(
                resolve_source_closure(&root_source, &dependencies, limits).map_err(|error| {
                    failure(
                        "InvalidApplicationSourceClosure",
                        format!("source closure authentication failed: {error}"),
                    )
                })?,
            )
        } else {
            None
        };
        Ok(Self {
            root_path,
            root_source,
            root_module,
            closure,
            units,
        })
    }

    fn diagnostic_group(&self, coordinate: &str) -> Option<ApplicationSourceDiagnostics> {
        let unit = self.closure.as_ref()?.modules().get(coordinate)?;
        let (path, source) = self
            .units
            .iter()
            .find(|(_, source)| source == unit.source())?;
        Some(ApplicationSourceDiagnostics {
            path: path.clone(),
            source: source.clone(),
            errors: Vec::new(),
        })
    }

    pub fn link_failure(
        &self,
        error: edict_syntax::source_linker::SourceLinkError,
    ) -> ApplicationBuildFailure {
        let mut failure = failure("ApplicationSourceLinkingFailed", error.to_string());
        if let Some(mut group) = self.diagnostic_group(&error.coordinate) {
            group.errors.push(ApplicationSourceError::Linker(error));
            failure.source_diagnostics = Some(vec![group]);
        }
        failure
    }

    pub fn compiler_failure(
        &self,
        errors: Vec<edict_syntax::source_linker::SourceCompilerError>,
    ) -> ApplicationBuildFailure {
        let mut groups = BTreeMap::new();
        let mut unmapped = Vec::new();
        for error in errors {
            if !groups.contains_key(&error.coordinate) {
                if let Some(group) = self.diagnostic_group(&error.coordinate) {
                    groups.insert(error.coordinate.clone(), group);
                }
            }
            if let Some(group) = groups.get_mut(&error.coordinate) {
                group
                    .errors
                    .push(ApplicationSourceError::Compiler(error.error));
            } else {
                // No source-local ownership witness: preserve the error in the
                // summary rather than attributing a synthetic span to a file.
                unmapped.push(error);
            }
        }
        let message = if unmapped.is_empty() {
            "Edict application sources did not compile to Core".to_owned()
        } else {
            format!(
                "Edict application sources did not compile to Core; unlocated errors: {unmapped:?}"
            )
        };
        let mut failure = failure("ApplicationCompilationFailed", message);
        if !groups.is_empty() {
            failure.source_diagnostics = Some(groups.into_values().collect());
        }
        failure
    }

    pub fn artifact(&self) -> Result<SourceArtifact, ApplicationBuildFailure> {
        if let Some(closure) = &self.closure {
            Ok(SourceArtifact {
                domain: SOURCE_CLOSURE_DIGEST_DOMAIN,
                kind: "edict-source-closure",
                bytes: closure.canonical_bytes().to_vec(),
            })
        } else {
            let bytes =
                encode_canonical_cbor(&CanonicalValue::Bytes(self.root_source.as_bytes().to_vec()))
                    .map_err(|error| {
                        failure(
                            "ApplicationEncodingFailed",
                            format!("source canonical encoding failed: {error}"),
                        )
                    })?;
            Ok(SourceArtifact {
                domain: SOURCE_DOMAIN,
                kind: "edict-source",
                bytes,
            })
        }
    }
}
