# Stale-Basis Obstruction Strand Fixture

Status: corridor manifest for issue #116. This fixture directory grows one
layer at a time as the obstruction-strand implementation advances.

## Purpose

This fixture records one stale-basis obstruction strand from source syntax
through later Core, Target IR, Echo receipt, and projection surfaces.

Each implementation PR may add only the artifact for the layer it owns. A row in
this table identifies the first layer allowed to add that artifact; it does not
claim that every eligible artifact already exists.

PR #129 adds Core lowering for the currently lowerable `require` subset. PR #131
adds Echo Target IR requirements for synthetic Core-compatible obstruction
fixtures. This stale-basis source remains source-only until the compiler spine
supports non-`basis none` intent bases and the `jim.basisFresh(...)` predicate
form, so this directory still intentionally has no `core.review` or
`echo-target-ir.review` artifact.

| Artifact | First Eligible Layer | Authority |
| --- | --- | --- |
| `source.amblicit` | PR 1 | Edict parser |
| `source.parse.review` | PR 1 | Edict parser |
| `core.review` | PR 2 | Edict Core |
| `core.canonical.bytes` | PR 2, if canonical Core bytes change | Edict Core |
| `core.digest` | PR 2, if canonical Core bytes change | Edict Core |
| `echo-target-ir.review` | PR 3 | Edict target lowerer |
| `echo-target-ir.canonical.bytes` | PR 3 | Edict target lowerer |
| `echo-target-ir.digest` | PR 3 | Edict target lowerer |
| `echo-receipt.review` | PR 4 | Echo runtime |
| `echo-receipt.canonical.bytes` | Future, only if receipt bytes freeze | Echo runtime |
| `echo-receipt.digest` | Future, only if receipt bytes freeze | Echo runtime |

## Non-Claims

This manifest does not claim that this Jim-shaped source fixture currently
reaches Core, Echo Target IR, Echo execution receipts, canonical receipt bytes,
Graft projection, or jedit UI support before the corresponding compiler/runtime
layer lands for this exact source shape.
