# Causal Cell Lawpack Fixture

This generator-owned fixture is the portable capability closure currently used
by standalone Edict applications that target Echo:

```text
causal.cell@1.createIfAbsent
```

The fixture is not a Hello Echo provider. Application coordinates and operation
names remain in external Edict source; this closure owns only the portable
create-if-absent capability and its typed `AlreadyExists` obstruction.

- `manifest.cbor` and `manifest.sha256` bind the canonical
  `edict.lawpack/v1` manifest.
- `exports.cbor` and `exports.sha256` bind the portable capability surface,
  including bounded `CreateInput`, `CreateReceipt`, and `ExistingValue` record
  definitions used by the effect and failure-payload signatures.
- `adapter.cbor` and `adapter.sha256` bind the direct declarative Echo adapter.
- `echo-operation-configuration.cbor` and its digest sidecar bind the generic
  Echo operation-lowering configuration.

## Source Import Digest

Copy the full review string from `manifest.sha256` into the source lawpack
import. This sidecar holds the `edict.lawpack/v1` domain-framed manifest identity.
A raw file hash from `shasum -a 256 manifest.cbor` identifies different bytes
and fails source preparation with `SourceImportMismatch`. The failure obligation
names the required manifest domain and matching digest. The exports, adapter,
and target-configuration sidecars cannot be substituted for this import value.

Regenerate only through:

```sh
cargo xtask lawpack-goldens --write
```

Check without modifying reviewed bytes:

```sh
cargo xtask lawpack-goldens --check
```

Before emitting these artifacts, the generator validates the lawpack bundle and
adapter, constructs an Edict source witness that imports the exact manifest
digest and consumes those imported record types, compiles it to Core, and
requires successful Target IR lowering. The
source witness is deliberately not published as fixture authority; it proves
the generated portable closure remains usable by a real Edict application.
