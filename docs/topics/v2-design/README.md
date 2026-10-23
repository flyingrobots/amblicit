# v2 Design Topic

Status: current HEAD boundary. No v2 adapter-composition behavior is
implemented in HEAD.

This shelf records the current executable boundary for the future v2 adapter
composition track. It does not claim that v2 obligation-closure resolution
exists in HEAD. Planned requirements and cases live in the test plan until the
owning implementation lands.

The non-topic design note for issue #4 is
[`docs/design/v2-obligation-closure.md`](../../design/v2-obligation-closure.md).
That document is planning material, not a topic README contract for landed
behavior.

## Current Contract

- v2 adapter declarations do not exist in `amblicit_syntax`.
- No adapter-composition resolver exists.
- No closure evidence artifact, canonicalization rule, or digest exists.
- No v2 diagnostic enum exists.
- v1 lowerability remains native-or-direct-adapter only.
- v1 rejects floating, chained, composite, or ambiguous direct-adapter claims
  with stable failure kinds. [V2DESIGN-REQ-007]
- v1 lowerability produces no Target IR, verifier report, bundle, admission
  request, or admission receipt. [V2DESIGN-REQ-007]

## v1 Boundary

The current executable behavior remains:

- native support succeeds only from explicit target-profile facts;
- exactly one digest-locked direct adapter may discharge a semantic effect;
- floating, chained, composite, or ambiguous adapters classify as unsupported;
- no Target IR, verifier report, bundle, admission request, or admission receipt
  is produced by the lowerability checker. [V2DESIGN-REQ-007]

The verification plan is tracked in [test-plan.md](./test-plan.md).
