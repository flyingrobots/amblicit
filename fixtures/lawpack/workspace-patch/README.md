# Workspace Patch Lawpack Fixture

This generator-owned closure defines the compiler side of one basis-bound
validated workspace patch request
[claim:workspace-patch-closure, confidence:1.00]. It binds:

- `workspace.patch.applyValidated@1` as the requestable operation;
- `workspace.patch.input@1` as the canonical patch-input schema;
- the exact workspace-root basis and writable-path-policy authority classes;
- CI-workflow exclusion and canonical validated-patch policy identities;
- exact settlement and reconciliation resources;
- postcondition evidence as an exact resulting workspace root; and
- bounded request construction and settlement budgets.

`input-schema.cbor`, `settlement-schema.cbor`, and
`reconciliation-law.cbor` are canonical
`edict.external-action-resource/v1` artifacts with generator-owned digest
sidecars [claim:workspace-patch-resources, confidence:1.00].
`apply-validated-patch.amblicit` pins those identities rather than sentinel
strings. A public external-action application supplies the exact three artifact
paths through `externalActionResources`; Edict recomputes and validates the
complete closure before publishing Core or Target IR.

The closure grants no callable write effect
[claim:workspace-patch-request-only, confidence:1.00]. Its operation profile
has empty `semanticEffects`, its adapter has empty `effectImplementations`, and
generated Target IR contains one external-action request with zero callable
steps. Edict constructs request data; it does not open, validate, or mutate a
workspace.

Artifacts are owned by:

```sh
cargo xtask lawpack-goldens --write
```

Checked bytes are verified by:

```sh
cargo xtask lawpack-goldens
```

This fixture leaves dynamic schema admission, exact basis and path-policy
validation, durable execution, settlement, reconciliation, recovery, and
effect-free replay to the host boundary
[claim:workspace-patch-host-boundary, confidence:0.99].

<details>
<summary>Appendix: Citations</summary>

| Claim | Evidence | Confidence | Notes |
| --- | --- | ---: | --- |
| `claim:workspace-patch-closure` | `xtask/src/lawpack_goldens.rs#493@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `xtask/src/lawpack_goldens.rs#692@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `xtask/src/lawpack_goldens.rs#746@67fe6682ee1b77d1c5dbdea15f45efcb311b5750` | 1.00 | Generator, target configuration, and emitted source bind the complete request closure. |
| `claim:workspace-patch-resources` | `xtask/src/lawpack_goldens.rs#1437@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `crates/edict-cli/src/application_build.rs#481@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `crates/edict-cli/src/application_build.rs#534@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `crates/edict-cli/src/application_build.rs#848@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `public_external_action_build_rejects_invalid_request_resource_closure` and `fixed_seed_request_resource_mutations_fail_closed` in `crates/edict-cli/src/application_build.rs` | 1.00 | The owner generates canonical identities; the public build decodes, validates, binds, and negatively exercises them. |
| `claim:workspace-patch-request-only` | `xtask/src/lawpack_goldens.rs#652@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `xtask/src/lawpack_goldens.rs#1460@67fe6682ee1b77d1c5dbdea15f45efcb311b5750` | 1.00 | The adapter exposes no effect implementation and the golden compiler requires one request with zero callable steps. |
| `claim:workspace-patch-host-boundary` | `xtask/src/lawpack_goldens.rs#692@67fe6682ee1b77d1c5dbdea15f45efcb311b5750`; `xtask/src/lawpack_goldens.rs#746@67fe6682ee1b77d1c5dbdea15f45efcb311b5750` | 0.99 | The compiler-owned artifacts describe the boundary crossing without performing it. |

</details>
