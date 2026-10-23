# Workspace Snapshot Lawpack Fixture

This generator-owned closure gives a typed external-action application the
profile, budget, and opaque target-configuration facts needed to construct a
bounded workspace-observation request.

It grants no callable semantic effect:

- `operationProfiles` declares `workspace.snapshot@1.observeRequest`;
- `semanticEffects` is empty;
- `budgetObligation` binds the compiler budget;
- `targetConfiguration` binds one exact request-profile resource;
- `effectImplementations` is empty.

`observe-workspace.amblicit` imports both the exact lawpack manifest and the
requestable `workspace.snapshot.observe@1` capability using the same manifest
digest. The generated Core and Target IR therefore preserve the complete
capability closure while Target IR contains one external-action request and
zero callable steps.

The same generator emits `input-schema.cbor`, `settlement-schema.cbor`, and
`reconciliation-law.cbor` plus their domain-framed digest sidecars. The source
pins those exact identities. A public external-action application lists all
three paths in `externalActionResources`; the build independently decodes the
closed meta-contracts, recomputes their identities, and rejects an incomplete,
substituted, duplicate, disconnected, malformed, or placeholder closure before
publication.

This real-lawpack corpus is distinct from
`fixtures/lang/external-actions/workspace-snapshot.amblicit`. That earlier
compiler fixture uses synthetic context facts and a placeholder capability
digest to isolate request encoding. The files here use the generated manifest,
profile, and budget closure, so their canonical Core and Target IR identities
are intentionally different.

Artifacts are generated only through:

```sh
cargo xtask lawpack-goldens --write
```

Checked bytes are verified through:

```sh
cargo xtask lawpack-goldens --check
```

The fixture constructs request data only. It does not observe a workspace,
perform I/O, invoke a target intrinsic, admit a settlement, or resume an Edict
program.
