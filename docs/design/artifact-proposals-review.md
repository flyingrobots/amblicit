# Artifact proposals: independent review and reconciliation

This note explains how the independent critique changed the two proposed designs. It records design evidence, not software acceptance or permission to enable a feature.

The proposals are [Keep build artifacts](keep-build-artifacts.md) and [encrypted artifacts](encrypted-build-artifacts.md). Agy reviewed local commit `5909244508aaba1b921c5eb8bb4b5a7885031bfd` against Edict main `618aa4f84ff88a0873849f8d9cfa55e1f340ffb9`. Its final verdict was `APPROVE`, with proposed refinements and a complete checklist. It ran no tests or builds. The complete 22,894-byte report remains unchanged in Reader receipt `ca2912ae-deb4-4b8a-b014-c8e2d214c457`.

The reviewed commit is local and unpublished. GitHub commit links in the raw report may not resolve. The final author refinements are later changes; the earlier verdict is not a fresh approval of those changes.

## Disposition of feedback

| Reviewer item | Author disposition | Result in the proposals |
| --- | --- | --- |
| P2-1: Public Keep ingestion gap | Accepted as an explicit implementation prerequisite. Keep #82 remains open. | A2 requires an accepted public, bounded production ingestion contract. Private or test-only staging is prohibited. |
| P2-2: Cache-key policy inputs | Accepted as a precision improvement. The example of currently configurable host limits is not established: Edict supplies fixed limits at the cited source. | Effective ProviderHostLimits, schema artifacts/registry mapping, and external-action resources are explicitly bound. A1 defines reuse policy and separates output destinations. |
| P2-3: Storage accumulation | Accepted. Retiring roots does not establish physical reclamation while GC remains absent. | Physical accounting covers objects, metadata, partial stages, indexes, and rotation. Publication refuses before capacity exhaustion. Cleanup cannot evade aggregate limits. |
| P3-1: Existing output publication | Accepted. | Cache hits use each artifact role's existing publication contract. Application pairs retain directory locking, synced staging, and rollback. |
| P3-2: Constant-time and uniform errors | Partially accepted; the whole-protocol inference is rejected. RFC 5116 does not require indistinguishable public parsing, key-routing, and authentication failure paths. | Use vetted secret-dependent authentication checks and coarse cryptographic refusal for untrusted callers. Public format and size errors retain stable kinds. No universal timing guarantee is claimed. |
| P4-1: Local platform experience | Accepted with an admission qualification. A container alone does not establish ext4 admission. | Durable Keep use requires an admitted Linux environment. Ordinary host builds retain the filesystem default. No currently implemented backend-selection flag is invented. |
| P5-1: Immutable links | Already satisfied; retain the practice. | Existing source links keep their exact upstream revisions. |

These are proposed dependency edges and acceptance gates. No tracker issues or recorded blocker edges were created. The original author-study repair graph is unchanged.

## Report corrections and limits

The report's introduction says six findings but lists seven numbered items. The disposition table accounts for all seven.

The report calls 16 MiB a default Keep segment limit. In the pinned README, that number is an example budget for aggregate catalog-selected segment bytes. Catalog bytes and decoded metadata allocate separately. Neither proposal adopts that example as a default or total-memory limit.

The report names proposed fields and operating scenarios as review aids. They do not establish a frozen schema, public Rust API, implemented CLI flag, or current compiler configuration option.

The selected cryptographic suite and library remain open. Authentication behavior needs review against the selected implementation. A small timing test cannot establish whole-protocol constant-time operation.

## Validation posture

The initial proposal branch passed guarded Docker `cargo xtask verify`, including existing workspace tests, goldens, provider checks, and topic validation. The gate establishes compatibility of the unchanged software. It does not implement or verify the proposed artifact-store and encryption requirements.

The author checked local proposal cross-links and `git diff --check`. No behavior changed and no fabricated RED/GREEN cycle was added. All acceptance rows remain planned. New implementation slices require real RED/GREEN evidence and their own review gates.

## Evidence

- Keep [issue #82](https://github.com/flyingrobots/keep/issues/82): public bounded production ingestion prerequisite; observed open on 2026-10-07.
- Keep [DurableStore at 3165890](https://github.com/flyingrobots/keep/blob/3165890e9291cfb5fe10e81a9d7cd151f3e59464/src/adapters/durable/store.rs): current public read/snapshot facade.
- Keep [README at 3165890](https://github.com/flyingrobots/keep/blob/3165890e9291cfb5fe10e81a9d7cd151f3e59464/README.md): platform, collection gaps, and aggregate-segment reader example.
- Edict [application build at 618aa4f](https://github.com/flyingrobots/edict/blob/618aa4f84ff88a0873849f8d9cfa55e1f340ffb9/crates/edict-cli/src/application_build.rs): host limits, schema registry, external resources, and paired output publication.
- [RFC 5116](https://www.rfc-editor.org/rfc/rfc5116.html): AEAD interface and limits; no whole-protocol access-control or timing proof is supplied by the RFC.
