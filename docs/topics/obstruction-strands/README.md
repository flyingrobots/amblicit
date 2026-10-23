# Obstruction Strands

Status: current HEAD boundary. First-class obstruction-strand source syntax is
implemented through Core lowering and Echo Target IR requirements for the
currently lowerable `require` subset.

This topic records the current Edict boundary for obstruction-strand planning.
The parser currently accepts `require ... else <obstruction>` as a source
statement carrying a predicate and terminal typed obstruction target. It also
accepts `require ... else continue obstructed { reason: ... }` as a distinct
source AST arm that lowers into a distinct Core require-failure arm when the
surrounding source is inside the current compiler-spine subset. Echo Target IR
now preserves that arm as an explicit requirement with a
`continueObstructed` failure disposition. Participant receipts and runtime
execution for preserved obstruction strands are not implemented in HEAD.

No Edict-owned participant receipt or runtime behavior exists for preserving a
blocked attempt as a repairable obstruction strand. Echo receipt bridge work is
downstream runtime evidence, not an Edict runtime claim. Future design work is
tracked in issue #116 and the non-topic design note
[`docs/design/obstruction-strands-v0.md`](../../design/obstruction-strands-v0.md).
That document is planning material, not a topic README contract for landed
behavior.

## Current Contract

Current parser-supported syntax:

```amblicit
require jim.basisFresh(input.basis)
  else jim.EditObstruction.StaleBase;
```

First-class parser-supported obstruction-strand syntax:

```amblicit
require jim.basisFresh(input.basis)
  else continue obstructed {
    reason: jim.EditObstruction.StaleBase,
    providedBasis: input.basis,
  };
```

Current source-level shape:

```text
Stmt::Require {
  predicate,
  arm: RequireElseArm::Terminal(...)
     | RequireElseArm::ContinueObstructed(...),
}
```

Current lowerable Core shape:

```text
CoreNode::Require {
  predicate,
  arm: CoreRequireFailureArm::Terminal { reason }
     | CoreRequireFailureArm::ContinueObstructed { reason },
}
```

Current lowerable Echo Target IR shape:

```text
TargetIrIntent {
  requirements: [
    TargetIrRequirement {
      predicate,
      on_failure: TargetIrRequireFailure::Terminal { reason }
                | TargetIrRequireFailure::ContinueObstructed { reason },
    }
  ],
  steps: [...]
}
```

The Core reason envelope is closed around a stable `reason.kind` coordinate and
a canonical map of opaque payload fields. The `reason` field in source selects
the reason kind; the other `continue obstructed { ... }` fields become the
payload. Duplicate payload fields reject before Core digesting.

The `continue obstructed { ... }` form is contextual to a `require ... else`
arm. It is not an expression, and a helper-shaped obstruction constructor such
as `continueInObstructedStrand({ reason: ... })` remains terminal obstruction
syntax rather than hidden control flow.

## Invariants

- Terminal obstruction and resumable obstruction are different planned semantics.
- Current `else <obstruction>` parses as an obstruction constructor, not as a
  hidden continuation or recovery workflow.
- Current `else continue obstructed { reason: ... }` parses as source AST and
  lowers to a distinct Core require-failure arm for lowerable require
  predicates.
- Current Core lowering distinguishes terminal require obstruction from
  preserved obstruction continuation in the canonical Core preimage.
- Current Echo Target IR lowering distinguishes terminal require obstruction
  from preserved obstruction continuation in the canonical Target IR preimage.
- Current git-warp Target IR lowering rejects Core require nodes with a stable
  unsupported-feature failure before artifact emission.
- Cross-project taxonomy now distinguishes not-admitted scheduler
  counterfactuals, admitted obstructed strands, and hard rejections without
  making Edict, Graft, or jedit semantic authorities for Echo runtime outcomes.
- A helper-like obstruction constructor such as
  `continueInObstructedStrand(...)` must not acquire hidden control-flow
  semantics without a first-class language/runtime contract.

## Related Work

- Syntax should remain terminal until a dedicated grammar and lowering contract
  exist.
- Planned parser, Core, Target IR, and receipt work is tracked in
  [test-plan.md](./test-plan.md).
- Future design material lives in
  [`docs/design/obstruction-strands-v0.md`](../../design/obstruction-strands-v0.md).
