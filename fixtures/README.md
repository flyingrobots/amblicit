# Edict Fixture Corpus

This is the Phase 0 conformance corpus. Each fixture is owned by a requirement
ID in the [requirements registry](../docs/REQUIREMENTS.md).

> A requirement without a fixture is advisory. A fixture without a requirement
> is folklore.

## Layout

Fixtures are grouped by requirement domain, mirroring the registry's ID scheme:

```text
fixtures/
  lang/        # EDICT-LANG-*  source that parses, typechecks, or is rejected
  core/        # EDICT-CORE-*  golden Core IR canonicalization + hash cases
  authority-facts/ # EDICT-ABI-AUTHORITY-FACTS-* canonical facts + digest
  target-profile/ # EDICT-ABI-TARGET-PROFILE-* Edict contract resources
  provider-contracts/ # EDICT-ABI-PROVIDER-* portable schema authority
  optic/       # EDICT-OPTIC-* optic-contract preservation cases
  target/      # EDICT-TARGET-* target profile ABI cases
  lawpack/     # EDICT-LAWPACK-* lawpack ABI cases
  abi/         # EDICT-ABI-*  no-duplication / display-sidecar cases
  bundle/      # CONTINUUM-BUNDLE-* + CONTINUUM-SOURCEPATH-* identity/DAG cases
  admission/   # CONTINUUM-RECEIPT-* admission acyclicity cases
  conformance/ # EDICT-CONFORMANCE-* two-lowerer differential cases
```

## Fixture kinds

- **Positive (GREEN):** source/artifact that must be accepted; for hash-bearing
  cases a golden canonical artifact and expected digest accompany it.
- **Negative (RED):** source/artifact that must be rejected, with the expected
  diagnostic code (e.g. `EDICT-INPUT-CONSTRAINT`, or the owning requirement ID).
- **Golden:** exact expected canonical-CBOR bytes / digest, so formatting,
  comment, and alias changes can be proven not to change semantic identity.

## Status

The first Core canonical fixture lives in
[`core/canonical/`](./core/canonical/). The first contract-bundle assembly
digest golden lives in [`bundle/assembly/`](./bundle/assembly/). The first
authority-facts canonical fixture lives in
[`authority-facts/canonical/`](./authority-facts/canonical/). Additional
Edict-owned target-profile contract resources live in
[`target-profile/contract-resources/`](./target-profile/contract-resources/).
The first canonical lawpack manifest/export fixture and its digest-pinned Edict
source live in [`lawpack/hello-echo/`](./lawpack/hello-echo/).
The Rust-neutral provider contract pack and its exact manifest live in
[`provider-contracts/source-functions-v1/`](./provider-contracts/source-functions-v1/).
The prior [`provider-contracts/v1/`](./provider-contracts/v1/) publication remains
unchanged for explicitly pinned function-free consumers.
Additional source coverage, relapse-zoo cases, runtime-owned target fixtures,
and admission fixtures remain planned as their owning implementation slices
land.

**Placeholder digests:** prose in the README/specs writes `sha256:...` as a
human ellipsis, which is **not lexable** — the grammar's `digest-lit` requires
`sha256:` followed by exactly 64 hex characters. A GREEN fixture is therefore
**not** the prose verbatim: it uses a syntactically valid **dummy digest**
(e.g. `sha256:` + 64 `0`s) so it parses and typechecks. The placeholder only
defers **digest-lock validation** (matching the import against a *real* pinned
digest), not compilation. Until the Phase 0 tooling (the `spec.lock.json`
generator and a fixture validator) computes and pins real digests, that lock
step is skipped; the example is otherwise a valid GREEN fixture.
