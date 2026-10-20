use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use edict::{
    check,
    diagnostic::{ParseErrorKind, SemanticErrorKind},
    CheckOutcome,
};

#[test]
fn curated_facade_checks_source_and_reports_stable_failures() {
    assert_eq!(
        check("package examples.public_surface@1;\n"),
        CheckOutcome::Valid
    );
    let CheckOutcome::ParseFailed(error) = check("package ;") else {
        panic!("malformed package must fail parsing");
    };
    assert_eq!(error.kind, ParseErrorKind::ExpectedIdentifier);

    let CheckOutcome::SemanticFailed(errors) =
        check("package examples.public_surface@1; type Input = { name: String };")
    else {
        panic!("unbounded string must fail semantic checking");
    };
    let kinds: Vec<_> = errors.iter().map(|error| error.kind).collect();
    assert_eq!(kinds, vec![SemanticErrorKind::UnboundedScalar]);
}

fn check_consumer(root: &Path, source: &str) -> Output {
    fs::write(root.join("src/main.rs"), source).expect("consumer source");
    Command::new("cargo")
        .args(["check", "--offline", "--message-format=json"])
        // An independent target directory avoids contending with the parent
        // cargo-test invocation; subsequent controls reuse the dependency build.
        .arg("--target-dir")
        .arg(root.join("target"))
        .current_dir(root)
        .output()
        .expect("check independent facade consumer")
}

fn compiler_error_codes(output: &Output) -> Vec<String> {
    String::from_utf8(output.stdout.clone())
        .expect("Cargo diagnostic stream")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("Cargo JSON record"))
        .filter(|record| record["reason"] == "compiler-message")
        .filter(|record| record["message"]["level"] == "error")
        .filter_map(|record| {
            record["message"]["code"]["code"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

#[test]
fn implementation_modules_are_unavailable_to_consumers() {
    let root = std::env::temp_dir().join(format!("edict-facade-consumer-{}", std::process::id()));
    fs::create_dir(&root).expect("fresh consumer workspace");
    fs::create_dir(root.join("src")).expect("consumer source directory");
    let facade_path =
        serde_json::to_string(env!("CARGO_MANIFEST_DIR")).expect("quote the facade path for TOML");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"facade-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dependencies]\nedict = {{ package = \"flyingrobots-edict\", path = {facade_path} }}\n"
        ),
    )
    .expect("consumer manifest");

    let positive = check_consumer(
        &root,
        "use edict::{check, CheckOutcome}; fn main() { assert_eq!(check(\"package examples.consumer@1;\"), CheckOutcome::Valid); }",
    );
    assert!(
        positive.status.success(),
        "supported consumer must compile: {}",
        String::from_utf8_lossy(&positive.stderr)
    );
    assert_eq!(compiler_error_codes(&positive), Vec::<String>::new());

    let negative = check_consumer(
        &root,
        "use edict::parser::parse_module; fn main() { let _ = parse_module(\"package examples.consumer@1;\"); }",
    );
    assert!(!negative.status.success());
    assert_eq!(compiler_error_codes(&negative), vec!["E0432"]);
    fs::remove_dir_all(&root).expect("remove owned consumer workspace");
}
