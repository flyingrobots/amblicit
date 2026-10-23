# Target-Profile Contract Resources

These files are the reviewed canonical authority artifacts for the five
Edict-owned resource slots in `edict.target-profile/v1`:

| Coordinate | Canonical bytes | Domain-framed digest |
| --- | --- | --- |
| `edict.canonical-cbor/v1` | `canonical-cbor.cbor` | `canonical-cbor.sha256` |
| `edict.determinism/v1` | `determinism.cbor` | `determinism.sha256` |
| `edict.diagnostics/v1` | `diagnostics.cbor` | `diagnostics.sha256` |
| `edict.fuel/v1` | `fuel.cbor` | `fuel.sha256` |
| `edict.wasm-component/v1` | `wasm-component.cbor` | `wasm-component.sha256` |

The executable semantic model lives in
`crates/amblicit-syntax/src/target_profile_contract_resources.rs`. Each digest is
SHA-256 over `['edict.digest/v1', <coordinate>, <canonical value>]`, not the raw
file bytes or this review document.

Use `cargo xtask target-profile-resource-goldens --check` to verify every file.
Use `--write` only after intentional semantic review. Runtime-owned generators
receive these resources as explicit values; the source paths are provenance for
review and packaging, not filesystem discovery handles.
