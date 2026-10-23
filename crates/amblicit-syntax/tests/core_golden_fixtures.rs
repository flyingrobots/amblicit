//! Reviewed Core golden artifact behavior.
//!
//! These tests assert stable software behavior: the executable canonical encoder
//! and digest function must reproduce the reviewed Core artifact fixtures.

mod common;

use std::fs;
use std::path::PathBuf;

use amblicit_syntax::{digest_core_module, encode_core_module};
use common::bounded_hello_core;

const GOLDEN_BYTES: &str = "fixtures/core/canonical/bounded-hello.core.cbor";
const GOLDEN_DIGEST: &str = "fixtures/core/canonical/bounded-hello.core.sha256";

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn reviewed_core_golden_bytes_match_executable_encoder() {
    let core = bounded_hello_core();
    let actual = encode_core_module(&core).expect("Core module encodes");
    let expected = fs::read(repo_path(GOLDEN_BYTES)).expect("reviewed Core byte fixture exists");

    assert_eq!(actual, expected);
}

#[test]
fn reviewed_core_digest_matches_exact_fixture() {
    let core = bounded_hello_core();
    let actual = digest_core_module(&core)
        .expect("Core module digests")
        .to_string();
    let expected =
        fs::read_to_string(repo_path(GOLDEN_DIGEST)).expect("reviewed Core digest fixture exists");

    assert_eq!(format!("{actual}\n"), expected);
}

#[test]
fn core_module_digest_is_stable_for_equivalent_core_ordering() {
    let core = bounded_hello_core();
    let mut reordered = core.clone();
    reordered.types = core
        .types
        .iter()
        .rev()
        .map(|(name, ty)| (name.clone(), ty.clone()))
        .collect();
    reordered.intents = core
        .intents
        .iter()
        .rev()
        .map(|(name, intent)| (name.clone(), intent.clone()))
        .collect();

    assert_eq!(
        digest_core_module(&core).expect("Core module digests"),
        digest_core_module(&reordered).expect("reordered Core module digests")
    );
}

#[test]
fn core_module_digest_changes_when_core_meaning_changes() {
    let core = bounded_hello_core();
    let mut changed = core.clone();
    changed
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .core_evaluation_budget
        .max_steps += 1;

    assert_ne!(
        digest_core_module(&core).expect("Core module digests"),
        digest_core_module(&changed).expect("changed Core module digests")
    );
}

#[test]
fn core_module_digest_changes_when_local_identity_changes() {
    let core = bounded_hello_core();
    let mut changed = core.clone();
    changed
        .intents
        .get_mut("sayHello")
        .expect("intent exists")
        .body
        .locals
        .first_mut()
        .expect("input local exists")
        .id = "arg.changed".to_owned();

    assert_ne!(
        digest_core_module(&core).expect("Core module digests"),
        digest_core_module(&changed).expect("changed Core module digests")
    );
}
