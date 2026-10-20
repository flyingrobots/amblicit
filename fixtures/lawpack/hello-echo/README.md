# Hello Echo Lawpack Fixture

This is the first reviewed `edict.lawpack/v1` fixture. It exists to expose the
next real Edict-to-Echo crossing without GraphQL, native application callbacks,
a fake transport, or a handwritten Echo executable package.

- `manifest.cbor` is the canonical lawpack manifest.
- `manifest.sha256` is its `edict.lawpack/v1` domain-framed identity.
- `exports.cbor` is the canonical export surface, including authenticated
  bounded record definitions for `CreateGreetingInput`, `GreetingReceipt`, and
  the `ExistingGreeting` failure payload.
- `exports.sha256` is its `hello.echo.exports/v1` domain-framed identity.
- `adapter.cbor` is the canonical direct declarative Echo target adapter.
- `adapter.sha256` is its manifest-bound domain-framed identity.
- `echo-operation-configuration.cbor` is the target-owned, adapter-bound
  program, budget, authority, and invocation-binding configuration that Echo
  may interpret.
- `echo-operation-configuration.sha256` is its
  `hello.echo.echo-operation-configuration/v1` domain-framed identity.
- `create-greeting.edict` imports the exact manifest digest and consumes those
  lawpack-owned input/receipt types in the bounded `createGreeting` action with
  typed `AlreadyExists` mapping.
- `create-greeting.core.cbor` is the canonical Core module compiled from that
  exact source and lawpack closure.
- `create-greeting.core.sha256` is its `edict.core.module/v1`
  domain-framed identity.
- `create-greeting.target-ir.cbor` is the canonical `echo.span-ir/v1`
  artifact lowered from that exact Core module.
- `create-greeting.target-ir.sha256` is its
  `edict.target-ir.artifact/v1` domain-framed identity.
- `create-greeting.result-projection.cbor` is the canonical
  `edict.result-projection/v1` assembly of the authored success value from
  declared application input and the capability-step result.
- `create-greeting.result-projection.sha256` is its
  `edict.result-projection.artifact/v1` domain-framed identity.

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

Check reviewed artifacts without modifying them:

```sh
cargo xtask lawpack-goldens --check
```

The loader validates the manifest, exports, and exact direct adapter. The
compiler derives Core and `echo.span-ir/v1` facts from that closure without a
handwritten compiler context. The golden command compiles and lowers the source
before reproducing the reviewed Core, Target IR, and result-projection bytes
and identities. This fixture does not yet bind the projected result during
Echo execution. Edict corroborates the target-configuration reference but
deliberately leaves its Echo-specific semantics and runtime evaluation to the
Echo-owned target provider.

## Result Budget Compatibility

The stock output budget is 2,048 bytes. `GreetingCreated` has a worst-case
canonical size of 1,299 bytes when its bounded strings use four-byte scalars.
The application compiler rejects an ordinary result above its selected budget.
The corrected budget changes adapter, manifest, Core, Target IR, and result
projection identities. Use the regenerated source import and complete closure.
This change does not update an external consumer's frozen producer lock or prove
runtime compatibility for a different producer pair.
