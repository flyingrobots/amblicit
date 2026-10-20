# Core Canonical Fixtures

This directory contains reviewed Core artifact fixtures generated from the
executable compiler and canonical encoder.

## Current Fixtures

`bounded-hello` is generated from
[`fixtures/lang/bounds/bounded-hello.edict`](../../lang/bounds/bounded-hello.edict)
using the deterministic compiler context facts:

- `hello.readOnly` -> `continuum.profile.read-only/v1`
- `hello.tinyBudget` -> `maxSteps=64`, `maxAllocatedBytes=4096`,
  `maxOutputBytes=4096`

Artifacts:

- `bounded-hello.core.cbor`: exact `edict.canonical-cbor/v1` bytes for the
  compiled Core module.
- `bounded-hello.core.sha256`: review rendering of the
  `edict.core.module/v1` digest.

`workspace-snapshot` is generated from
[`fixtures/lang/external-actions/workspace-snapshot.edict`](../../lang/external-actions/workspace-snapshot.edict)
using the deterministic `workspace.read` profile and `workspace.tiny` budget
facts. Its `.core.cbor` and `.core.sha256` files bind every external-request
field and exact capability closure into reviewed Core identity.

The digest preimage is the canonical CBOR encoding of:

```text
["edict.digest/v1", "edict.core.module/v1", <Core module value>]
```

## Regeneration

Check fixtures without changing files:

```sh
cargo xtask core-goldens --check
```

Regenerate fixtures after an intentional Core semantic or canonical-encoding
change:

```sh
cargo xtask core-goldens --write
```

Review the byte and digest diff before committing regenerated artifacts.
