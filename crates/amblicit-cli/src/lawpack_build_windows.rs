//! Fail-closed Windows boundary for lawpack builds.

use std::path::Path;

#[derive(Debug)]
pub(crate) struct LawpackBuildFailure {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

/// Refuse Windows lawpack builds before reading the document or its namespace.
pub(crate) fn build_lawpack(
    _document_path: &Path,
    check_only: bool,
) -> Result<(), LawpackBuildFailure> {
    let (kind, mode) = if check_only {
        ("LawpackCheckUnsupported", "check-only")
    } else {
        ("LawpackOutputWriteUnsupported", "write")
    };
    Err(LawpackBuildFailure {
        kind,
        message: format!(
            "lawpack {mode} builds are unsupported on Windows because Edict has no native transactional publication backend"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::build_lawpack;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn windows_lawpack_build_fails_closed_before_document_io() {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "edict-windows-lawpack-unsupported-{}-{unique}",
            std::process::id()
        ));
        let output = root.join("generated");
        fs::create_dir_all(&output).unwrap_or_else(|error| {
            panic!("failed to create Windows lawpack sentinel output: {error}")
        });
        let sentinel = output.join("sentinel");
        fs::write(&sentinel, b"untouched")
            .unwrap_or_else(|error| panic!("failed to write Windows lawpack sentinel: {error}"));
        let missing_document = root.join("missing-lawpack-build.json");

        let write_failure = build_lawpack(&missing_document, false)
            .err()
            .unwrap_or_else(|| panic!("Windows lawpack write must fail closed"));
        assert_eq!(write_failure.kind, "LawpackOutputWriteUnsupported");
        assert_eq!(
            fs::read(&sentinel).unwrap_or_else(|error| {
                panic!("failed to reread Windows lawpack sentinel: {error}")
            }),
            b"untouched"
        );
        assert!(!missing_document.exists());

        let check_failure = build_lawpack(&missing_document, true)
            .err()
            .unwrap_or_else(|| panic!("Windows lawpack check must fail closed"));
        assert_eq!(check_failure.kind, "LawpackCheckUnsupported");
        assert_eq!(
            fs::read(&sentinel).unwrap_or_else(|error| {
                panic!("failed to reread Windows lawpack sentinel: {error}")
            }),
            b"untouched"
        );

        fs::remove_dir_all(root)
            .unwrap_or_else(|error| panic!("failed to remove Windows lawpack test tree: {error}"));
    }
}
