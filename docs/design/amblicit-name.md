# Amblicit Name Decision

Status: accepted decision with staged implementation. Stage 1 (product prose)
is current. Stages 2 to 4 are planned. Until each stage lands, the names it
replaces remain the current names, and this page lists them.

## Decision

The language and project are named **Amblicit**, pronounced am-BLISS-it. The
name was previously Edict.

- **Slogan:** *Use only as declared.*
- **Description:** lawful computation within explicit bounds.

The word joins *ambit* (the bounds of what something may reach), *licit*
(lawful) and *explicit*. It follows the pattern of *implicit* and *explicit*.

The term **lawpack** does not change. A lawpack is a digest-locked law package
that an Amblicit program imports.

## Reasons

1. An edict is a decree from an authority. An Amblicit operation does not
   command. It proposes a change, and the destination runtime decides whether
   to admit it. Spec invariants I-014 (assurance is evidence, not authority)
   and I-028 (admission evidence is external) state this rule.
2. Other projects use the name Edict. On 2026-10-23, the
   [`edict` crate](https://crates.io/crates/edict) on crates.io was a published
   entity-component-system library, and the most-starred GitHub repository
   named `edict` was
   [`cft0808/edict`](https://github.com/cft0808/edict), a multi-agent
   orchestration system.
3. On 2026-10-23, `amblicit` and `amblicit-lang` were not registered on
   crates.io: the crates.io API returned HTTP 404 for
   `https://crates.io/api/v1/crates/amblicit` and
   `https://crates.io/api/v1/crates/amblicit-lang`. A
   [GitHub repository search for `amblicit`](https://github.com/search?q=amblicit&type=repositories)
   returned no repositories.

## Stages

Each stage is one pull request. Each stage leaves `main` in a working state.

| Stage | Scope | Posture |
| --- | --- | --- |
| 1 | Product prose: the root `README.md`, this note, and one entry in the documentation router that links to this note | Current |
| 2 | Crate names `amblicit-*`, the facade crate `amblicit` and the CLI binary `amblicit` | Planned |
| 3 | Source extension `.amblicit`, editor grammars and the scope name `source.amblicit` | Planned |
| 4 | Wire identifiers `amblicit.*/vN` as new versions | Planned |

Rules for the stages:

- Stage 4 adds new versioned identifiers. Each existing `edict.*/vN`
  identifier keeps its meaning at its existing version. Two names must not
  produce the same canonical bytes.
- Stage 4 changes canonical bytes, digests and golden artifacts. Downstream
  consumers, such as Echo and Hello Echo, change after Stage 4 merges.
- After Stage 1 merges, the GitHub repository will be renamed from
  `flyingrobots/edict` to `flyingrobots/amblicit`. GitHub redirects the old
  URLs.
- Other prose, such as the specifications and topic shelves, changes with the
  stage that renames the identifiers it describes.

## Current names

Until the matching stage lands, these names are current. Use them in commands,
code and artifacts:

| Surface | Current name | Planned name | Stage |
| --- | --- | --- | --- |
| Crates | `edict`, `edict-cli`, `edict-syntax`, `edict-provider-schema`, `edict-provider-host-wasmtime` | `amblicit`, `amblicit-cli`, `amblicit-syntax`, `amblicit-provider-schema`, `amblicit-provider-host-wasmtime` | 2 |
| CLI binary | `edict` | `amblicit` | 2 |
| Source extension | `.edict` | `.amblicit` | 3 |
| Editor scope | `source.edict` | `source.amblicit` | 3 |
| Wire identifiers | `edict.*/vN` | `amblicit.*/vN`, new versions | 4 |
| Repository | `flyingrobots/edict` | `flyingrobots/amblicit` | after 1 |

## Relationships

| Relationship | Targets |
| --- | --- |
| `refines` | none |
| `supersedes` | none |
| `depends_on` | [DOCS-REQ-007](../topics/documentation/README.md#durable-decision-discipline) |
| `related` | [Language specification](../SPEC_edict-language-v1.md), [README](../../README.md) |
