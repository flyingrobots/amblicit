# Author a causal-cell lawpack with your own limits

Use the application-owned Rust example to generate a lawpack document and source
whose Unicode bounds, replacement-byte cap and result budget agree. This recipe
builds a create-if-absent operation; successful compilation does not execute it.
It uses the existing JSON authoring API without changing the stock generator.

## Build the tools

From the Edict checkout, build both binaries:

```sh
cargo build -p edict-cli --bin edict --example configurable-causal-cell
EDICT_BIN="$(pwd)/target/debug/edict"
EDICT_CELL_HELPER="$(pwd)/target/debug/examples/configurable-causal-cell"
```

If you set `CARGO_TARGET_DIR`, select the binaries in that target directory
instead. Run compilation and tests in the project's bounded Docker environment
when working under this repository's local validation policy.

## Choose explicit units

The helper arguments are **key scalars, value scalars, replacement bytes**.
Each Unicode scalar may require four UTF-8 bytes. The helper rejects a value
scalar bound whose worst-case bytes exceed the physical cap.

| Variant | Key scalars | Value scalars | Replacement bytes |
| --- | ---: | ---: | ---: |
| Stock byte cap | 64 | 64 | 256 |
| Larger byte cap | 64 | 1024 | 4096 |

The first row matches the stock *byte cap*, not its 256-scalar value type. A
4,096-byte cap supports 1,024 four-byte scalars in this conservative recipe.
The result budget includes canonical CBOR map, field-name and text headers.
The allocation budget is a declared reservation, not a runtime allocation proof.

The consistency rule belongs to this helper. The pinned v1 provider can build
and verify a manually reauthored lawpack whose physical cap is smaller than the
value type's worst-case UTF-8 size. Such a build does not promise that every
value admitted by the type will fit the operation's byte cap. Use the helper's
paired limits; do not treat provider verification as a general consistency check.

## Author a fresh application

Create a new application directory. These commands use the larger variant:

```sh
EDICT_APP="$(mktemp -d "${TMPDIR:-/tmp}/edict-cell.XXXXXX")"
mkdir "$EDICT_APP/src"
"$EDICT_CELL_HELPER" lawpack 64 1024 4096 > "$EDICT_APP/edict.lawpack.json"
(
  cd "$EDICT_APP"
  "$EDICT_BIN" <<'JSON'
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","lawpack":"edict.lawpack.json"}
JSON
)
```

Expect a successful build status and an owned `vendor/cell/` tree containing the
manifest, exports, adapter, configuration, local declaration resources and
SHA-256 sidecars. Edict computes these identities. The helper does not patch
CBOR or calculate generated pins.

Generate source using the exact authored manifest identity, then the application
manifest:

```sh
EDICT_CELL_DIGEST="$(cat "$EDICT_APP/vendor/cell/manifest.sha256")"
"$EDICT_CELL_HELPER" source 64 1024 4096 "$EDICT_CELL_DIGEST" > "$EDICT_APP/src/create.edict"
"$EDICT_CELL_HELPER" application > "$EDICT_APP/edict.application.json"
```

Use the same three numeric arguments for lawpack and source generation. The
source returns `{ key, value }` and passes the application input directly to the
create effect. Changing bounds requires reauthoring and regenerating the source
with the new pin.

## Supply the exact provider and compile

The exercised provider comes from Echo commit
`6626d265841a5a51b2e1473155f9f131b3d59778`, at
`schemas/edict-provider/package/v1/`. Its declared provider identity is
`sha256:21e5267310b6b4c06bf24a814d76790cbd58a4a9dcc33205dddd6fc1937594dd`.
It accepts the `echo.operation-lowering-configuration/v1` create configuration.
Use the complete package, including its components and generated resources.

Set `ECHO_CHECKOUT` to an Echo Git checkout containing that commit, then export
only its package into the fresh application's `provider` directory:

```sh
mkdir "$EDICT_APP/provider"
git -C "$ECHO_CHECKOUT" archive 6626d265841a5a51b2e1473155f9f131b3d59778 schemas/edict-provider/package/v1 |
  tar -x --strip-components=4 -C "$EDICT_APP/provider"
(
  cd "$EDICT_APP"
  "$EDICT_BIN" <<'JSON'
{"schema":"edict.compiler.settings/v1","type":"compilerSettings","operation":"build","application":"edict.application.json"}
JSON
)
```

A successful build places `executable-operation-package.cbor` and
`verification-report.cbor` in `.build/application/`, alongside compiler artifacts.
Edict validates the package closure and invokes the selected lowerer and verifier.
The integration witness builds both variants in different directories and checks
identical executable-package and report bytes for identical inputs.

## Diagnose a refusal

- A malformed number or digest, zero limit, overflow, or inconsistent scalar/byte
  pair makes the helper exit 2 without printing a document.
- Reauthoring while keeping the old source pin produces
  `InvalidApplicationClosure`; regenerate source from the new manifest sidecar.
- A repinned lawpack with an insufficient `maxOutputBytes` budget produces
  `InvalidBound`. Keep the helper's derived budget or justify a larger one.
- A reauthored, repinned zero replacement cap produces
  `InvalidProviderInvocation` during provider schema admission. Aliasing the key
  and replacement fields produces `ProviderLowererRefused`. Updating hashes
  does not make either configuration valid.
- Failed application compilation preserves the last successfully published
  package and report; it does not make those old files the output of the failure.

## Understand the evidence boundary

Local rules, compatibility and fixture resources are authored declarations;
their hashes are not independent assurance. The witness proves authoring,
compilation and the pinned provider's verification, not Echo execution. This
v1 recipe does not add general ingress schema validation or runtime key-scalar
validation. Those remain separate provider/runtime contracts.
