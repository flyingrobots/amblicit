# Topic Shelves

Topic shelves are current reference material for landed subsystems. They are not
RFCs, proposals, or dated retrospectives.

Each topic uses this shape:

- `README.md`: what is true in HEAD.
- `test-plan.md`: how those truths are verified, including fixtures, oracles,
  implemented evidence, planned cases, and known gaps.
- `architecture.md`: optional machinery/dataflow notes when the structure earns
  a separate page.
- `rationale.md`: optional still-relevant tradeoffs, with historical decisions
  cited rather than duplicated.

The local contract graph is checked by:

```text
cargo xtask contract-check
```

Test-plan row statuses are `implemented`, `planned`, `gap`, and `policy`.
`policy` is reserved for human-review workflow contracts; it is not a substitute
for executable evidence when software behavior is at stake.

The full local gate is:

```text
cargo xtask verify
```

## Topics

- [Admission](./admission/README.md): typed Gate C admission-boundary checks
  for Edict-owned artifact and invocation evidence semantics.
- [Assurance](./assurance/README.md): optional, hash-bound HOLMES, Watson, and
  Moriarty evidence references at the contract-bundle boundary.
- [Authority Facts](./authority-facts/README.md): JSON and canonical-CBOR
  compiler context facts with digest-bound source identity, normative CDDL, and
  stable load/decode failures.
- [CLI](./cli/README.md): JSONL-only `edict` command-line boundary for stdin
  request records, structured stdout/stderr events, and compiler settings.
- [Compiler Spine](./compiler-spine/README.md): executable source-AST to
  in-memory Core IR stage contract for the initial lowerable subset.
- [Contract Bundles](./contract-bundles/README.md): typed v1
  participant-neutral contract bundle and assurance evidence manifest
  validation.
- [Core IR](./core-ir/README.md): `edict.core/v1` semantic model and normative
  CDDL schema boundary for the Core contract.
- [Documentation Standards](./documentation/README.md): reader-task page types,
  documentation coverage, examples, and docs-impact rules.
- [Developer Tooling](./developer-tooling/README.md): editor-facing source
  highlighting roles, Tree-sitter grammar source, TextMate grammar scopes, and
  fixture-backed tooling behavior.
- [External-Action Requests](./external-action-requests/README.md):
  deterministic typed requests for Echo-coordinated external effects, with
  digest-locked capability closure and no ambient execution authority.
- [Fixtures](./fixtures/README.md): shared executable fixture corpus and
  reviewed Core golden artifact contract.
- [Lawpacks](./lawpacks/README.md): lawpack import, direct-adapter, bundle
  reference, request-only profile, and canonical manifest-validation boundary.
- [Lowerability](./lowerability/README.md): typed v1 lowering requirements,
  target-profile facts, and direct-only support classification.
- [Numeric Foundation](./numeric-foundation/README.md): checked Bunny Q32.32
  arithmetic for compiler consumers, with explicit source and artifact limits.
- [Obstruction Strands](./obstruction-strands/README.md): current terminal and
  preserved-obstruction source/Core boundary plus planned Target IR/runtime
  verification ledger.
- [Providers](./providers/README.md): runtime-neutral provider manifest
  validation, explicit in-process compatibility adapters for the current
  built-in target lowerers, the external provider WIT contract, and pure
  invocation request/result validation with host-authored output identity.
- [Public Rust API](./public-rust-api/README.md): curated non-publishing Rust
  facade for source checking, stable diagnostic kinds, and canonical artifact
  identities.
- [Result Projections](./result-projections/README.md): compiler-owned,
  canonical, bounded application-result assembly from declared input and
  capability-result sources with independent reverse verification.
- [Release Process](./release-process/README.md): tag-triggered GitHub Release
  publication contract, release runbook, and verification matrix.
- [Review Process](./review-process/README.md): pull request review-bot fallback
  policy for CodeRabbit outages and alternate bot review.
- [Rust Standards](./rust-standards/README.md): Rust safety, determinism, error
  design, I/O boundary, dependency, generated-artifact, and review policy.
- [Semantic Validation](./semantic-validation/README.md): source/surface
  `validate_surface` stage contract and verification matrix.
- [Syntax](./syntax/README.md): Phase 1 `edict-syntax` lexer/parser contract and
  verification matrix.
- [Target IR](./target-ir/README.md): first target-owned artifact generation
  boundary for Echo Span IR and later target profiles.
- [Target Profiles](./target-profiles/README.md): typed v1 target-profile
  manifest conformance and runtime-neutral profile validation.
- [Testing Workflow](./tests/README.md): RED/GREEN development discipline,
  fixture reuse, and local verification workflow.
- [v2 Design](./v2-design/README.md): current HEAD boundary and planned test
  ledger for future adapter composition work.
