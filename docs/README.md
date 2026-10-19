# Edict Specifications

The current specification set is:

- [Topic Shelves](./topics/README.md): current reference chapters and
  verification plans for landed subsystems. A topic shelf is a contract graph,
  not a proposal: `README.md` says what is true in HEAD, `test-plan.md` says how
  it is verified and where gaps remain, and optional `architecture.md` /
  `rationale.md` files explain machinery and still-relevant tradeoffs.
- [Testing Workflow Topic](./topics/tests/): RED/GREEN development discipline,
  fixture reuse, and local verification workflow.
- [v2 Design Topic](./topics/v2-design/): current HEAD boundary for future
  adapter composition work.
- [Authority Fact Governance Design Note](./design/authority-fact-governance.md):
  non-topic planning note for trusted lawpack and target-profile authorship.
- [v2 Obligation-Closure Design Note](./design/v2-obligation-closure.md):
  non-topic planning note for future adapter composition and
  obligation-closure resolution.
- [Obstruction Strands Design Note](./design/obstruction-strands-v0.md):
  non-topic planning note for future repairable obstruction preservation.
- [Documentation Standards Topic](./topics/documentation/): reader-task page
  types, documentation coverage, examples, and docs-impact rules.
- [Release Roadmap](../ROADMAP.md): scheduled alpha milestones, release gates,
  and the GitHub issue/milestone map.
- [v0.11 Release Notes](./releases/v0.11.0-alpha.1.md): release-prep notes for
  the contract-bundle assembly and canonical Target IR artifact freeze alpha.
- [v0.10 Release Notes](./releases/v0.10.0-alpha.1.md): published notes for the
  first public CLI and structured-diagnostics alpha.
- [v0.9 Release Notes](./releases/v0.9.0-alpha.1.md): published notes for the
  Echo and git-warp Target IR alpha.
- [v0.8 Release Notes](./releases/v0.8.0-alpha.1.md): published notes for
  the minimal effectful compiler-spine alpha.
- [v0.7 Release Notes](./releases/v0.7.0-alpha.1.md): published notes for
  the file-backed authority-facts alpha.
- [v0.6 Release Notes](./releases/v0.6.0-alpha.1.md): published notes for the
  developer-tooling alpha.
- [v0.5 Release Notes](./releases/v0.5.0-alpha.1.md): published notes for the
  Gate C admission-boundary alpha.
- [v0.4 Release Notes](./releases/v0.4.0-alpha.1.md): published notes for
  the target-profile, lowerability, and contract-bundle validation alpha.
- [v0.3 Release Notes](./releases/v0.3.0-alpha.1.md): published notes for
  the compiler-spine, canonical Core encoder, reviewed golden bytes, and exact
  digest alpha.
- [v0.2 Release Notes](./releases/v0.2.0-alpha.1.md): published notes for the
  Core semantic model and normative schema alpha.
- [v0.1 Release Notes](./releases/v0.1.0-alpha.1.md): published notes for the
  first front-end alpha milestone.
- [Release Process Topic](./topics/release-process/): tag-triggered GitHub
  Release publication contract and verification matrix.
- [Review Process Topic](./topics/review-process/): pull request review-bot
  fallback policy for CodeRabbit outages and alternate bot review.
- [Rust Standards Topic](./topics/rust-standards/): Rust safety, determinism,
  error design, I/O boundary, dependency, generated-artifact, and review policy.
- [Admission Topic](./topics/admission/): typed Gate C admission-boundary checks
  for Edict-owned artifact and invocation evidence semantics.
- [Assurance Topic](./topics/assurance/): optional, hash-bound HOLMES, Watson,
  and Moriarty evidence references at the contract-bundle boundary.
- [Authority Facts Topic](./topics/authority-facts/): JSON and canonical-CBOR
  compiler context facts with digest-bound source identity, normative CDDL, and
  stable load/decode failures.
- [CLI Topic](./topics/cli/): JSONL-only `edict` command-line boundary for
  stdin request records, structured stdout/stderr events, and compiler settings.
- [Contract Bundles Topic](./topics/contract-bundles/): typed v1
  participant-neutral bundle and assurance evidence manifest validation.
- [Fixtures Topic](./topics/fixtures/): shared executable fixture corpus and
  reviewed Core golden artifact contract.
- [Numeric Foundation](./topics/numeric-foundation/): the checked Bunny Q32.32
  compiler API and its conformance boundary.
- [Lawpacks Topic](./topics/lawpacks/): lawpack import, direct-adapter, bundle
  reference, and deferred manifest-validation boundary.
- [Lowerability Topic](./topics/lowerability/): typed v1 lowering
  requirements, target-profile facts, and direct-only support classification.
- [Target Profiles Topic](./topics/target-profiles/): typed v1 target-profile
  manifest conformance and runtime-neutral profile validation.
- [Compiler Spine Topic](./topics/compiler-spine/): executable
  source-AST-to-in-memory-Core stage contract for the initial lowerable subset.
- [Core IR Topic](./topics/core-ir/): current contract and verification matrix
  for the `edict.core/v1` semantic model and CDDL schema.
- [Compiler Settings JSON Schema](./schemas/edict.compiler-settings.v1.schema.json):
  stable JSON Schema artifact for `edict.compiler.settings/v1` input records.
- [Compiler Input JSON Schema](./schemas/edict.compiler-input.v1.schema.json):
  stable JSON Schema artifact for `edict.compiler.input/v1` input records.
- [CLI Check Result JSON Schema](./schemas/edict.cli-check-result.v1.schema.json):
  stable JSON Schema artifact for `edict.cli.check-result/v1` stdout records.
- [CLI Diagnostic JSON Schema](./schemas/edict.cli-diagnostic.v1.schema.json):
  stable JSON Schema artifact for `edict.cli.diagnostic/v1` stderr records.
- [CLI Event JSON Schema](./schemas/edict.cli-event.v1.schema.json):
  stable JSON Schema artifact for `edict.cli.event/v1` terminal status records.
- [CLI Info JSON Schema](./schemas/edict.cli-info.v1.schema.json):
  stable JSON Schema artifact for `edict.cli.info/v1` `--help`/`--version` records.
- [Syntax Topic](./topics/syntax/): current contract and verification matrix for
  the Phase 1 `edict-syntax` lexer/parser.
- [SPEC - Edict Language v1](./SPEC_edict-language-v1.md): source syntax, type
  system, effect rules, Core IR, and language-level canonical value semantics.
- [SPEC - Edict Lawpack ABI v1](./SPEC_edict-lawpack-abi-v1.md): lawpack
  manifest and dependency graph, exported types/constants, pure helper and
  semantic effect signatures, proof-only vs runtime-materialized classification,
  typed obstruction payloads, footprint/cost obligations, and target adapters.
- [SPEC - Edict Target Profile ABI v1](./SPEC_edict-target-profile-abi-v1.md):
  intrinsic signatures, effect signatures, target lowering, application model,
  verifier ABI, footprint algebra, and cost algebra. Canonical schemas live in
  [`abi/`](./abi/) (`edict-target-profile.cddl`,
  `edict-authority-facts.cddl`, `edict-target-provider.wit`).
- [SPEC - Continuum Contract Bundle v1](./SPEC_continuum-contract-bundle-v1.md):
  participant-neutral contract bundle identity, artifact graph, provenance
  references, canonical CBOR/hash framing, and attestation roles.
- [SPEC - Continuum Admission v1](./SPEC_continuum-admission-v1.md):
  participant descriptors, policy epochs, admission requests, admission
  receipts, capability receipts, and participant-specific decisions.
- [GUIDE - Edict Assurance and Transparency](./GUIDE_edict-assurance-transparency.md):
  HOLMES, Watson, Moriarty, transparency logs, nutrition labels, profile diffs,
  relapse fuzzing, the hash ladder, the Aperture Ledger, and the two-lowerer
  trial.
- [REQUIREMENTS - Fixture Constitution](./REQUIREMENTS.md): every normative
  requirement ID, its owner spec, and its positive/negative/golden fixtures.
- [Design Baseline](./DESIGN_runtime-neutral-edict-sha-lock-assurance.md):
  original runtime-neutral Edict/SHA-lock design packet retained as
  non-normative context.

Machine-readable ABIs live in [`abi/`](./abi/) and are the single source of
truth for the artifacts they describe; prose JSON in the specs is generated from
them.
