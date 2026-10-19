//! Literal command evidence preserves bytes without relaxing source whitespace.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("Git fixture command starts")
}

#[test]
fn literal_execution_evidence_retains_source_whitespace_checks() {
    let root = std::env::temp_dir().join(format!(
        "edict-literal-git-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).expect("owned Git fixture directory");
    assert!(git(&root, &["init", "--quiet"]).status.success());
    fs::write(
        root.join(".gitattributes"),
        include_bytes!("../../.gitattributes"),
    )
    .unwrap();
    let log = "docs/plans/study-feedback/evidence/probe.txt";
    fs::create_dir_all(root.join("docs/plans/study-feedback/evidence")).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join(log), "observed output\n").unwrap();
    fs::write(root.join("src/probe.rs"), "fn main() {}\n").unwrap();
    assert!(git(&root, &["add", "."]).status.success());
    let baseline = git(
        &root,
        &[
            "-c",
            "user.name=validation",
            "-c",
            "user.email=validation@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture baseline",
        ],
    );
    assert!(baseline.status.success(), "{:?}", baseline.stderr);
    fs::write(root.join(log), "observed output\n\n").unwrap();
    let literal = git(&root, &["diff", "--check", "--", log]);
    assert_eq!(literal.status.code(), Some(0), "{:?}", literal.stdout);
    fs::write(root.join("src/probe.rs"), "fn main() {}\n\n").unwrap();
    let source = git(&root, &["diff", "--check", "--", "src/probe.rs"]);
    assert_eq!(
        source.status.code(),
        Some(2),
        "ordinary source retains the whitespace refusal"
    );
    fs::remove_dir_all(&root).expect("remove only the owned successful fixture");
}
