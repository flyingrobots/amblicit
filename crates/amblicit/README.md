# Edict for Rust

`flyingrobots-amblicit` exposes the Rust library name `amblicit`. It is the curated
facade for Edict source checking, stable diagnostic classifications, and
canonical semantic-artifact identity operations.

The complete application-build boundary remains the JSONL `amblicit` CLI. This
facade deliberately does not expose the implementation crate's full module
tree.

```rust
use amblicit::{check, CheckOutcome};

assert_eq!(
    check("package examples.public_facade@1;\n"),
    CheckOutcome::Valid
);
```

## Publication status

This package currently has `publish = false`. Its archive and dependency
closure are under release-engineering review. Nothing in this package grants
permission to publish it to crates.io.
