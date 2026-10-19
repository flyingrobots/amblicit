# Fixtures Topic

Status: current HEAD contract.

This shelf describes the fixture corpus as a shared verification surface. The
fixtures are not examples copied from prose; they are executable inputs or
reviewed artifacts consumed by parser, validation, compiler, tooling, and Core
golden checks.

## Public Surface

The root fixture constitution is [`fixtures/README.md`](../../../fixtures/README.md).
It defines positive, negative, and golden fixture roles and records the current
placeholder-digest rule for runnable source fixtures.

Current checked-in fixture families are:

- source fixtures under [`fixtures/lang/`](../../../fixtures/lang/) for parser,
  semantic-validation, compiler-spine, and developer-tooling behavior;
- reviewed Core artifacts under
  [`fixtures/core/canonical/`](../../../fixtures/core/canonical/) for exact
  canonical bytes and the `edict.core.module/v1` digest;
- reviewed authority-facts artifacts under
  [`fixtures/authority-facts/canonical/`](../../../fixtures/authority-facts/canonical/)
  for exact canonical bytes and the `edict.authority-facts/v1` digest;
- reviewed target-profile contract resources under
  [`fixtures/target-profile/contract-resources/`](../../../fixtures/target-profile/contract-resources/)
  for the five Edict-owned resource coordinates and their coordinate-framed
  digests;
- the provider contract pack under
  [`fixtures/provider-contracts/source-modules-v1/`](../../../fixtures/provider-contracts/source-modules-v1/)
  for self-contained CDDL, root bindings, exact schema/resource bytes, raw and
  domain-framed digests, and reviewed provenance;
- golden CLI cases under [`fixtures/cli/`](../../../fixtures/cli/) replayed
  end-to-end through the `edict` binary for byte-exact stdout, stderr, and exit
  code.

`cargo xtask core-goldens --check` verifies the reviewed Core artifacts against
the executable compiler and encoder. `cargo xtask core-goldens --write`
regenerates them after an intentional Core semantic or canonical-encoding
change. [FIXTURES-REQ-002]

`cargo xtask authority-facts-goldens --check` verifies the reviewed
authority-facts bytes and digest against the existing JSON loader plus the
canonical codec. `--write` regenerates them after an intentional authority-facts
ABI change. [FIXTURES-REQ-005]

`cargo xtask target-profile-resource-goldens --check` regenerates all five
contract resources from their executable semantic model and compares exact
bytes and digests. `--write` updates them after intentional contract review.
[FIXTURES-REQ-006]

`cargo xtask provider-contract-pack --check` assembles the complete CDDL pack
and manifest from explicit Edict-owned inputs, compares both exact files, and
never rewrites drift. `--write` is reserved for intentional ABI review.
[FIXTURES-REQ-007]

## Current Contract

- Source fixtures are executable behavior inputs. Tests consume them through
  public parser, validator, compiler, highlighter, grammar, and encoder APIs.
  [FIXTURES-REQ-001]
- Source fixture digests use lexable `sha256:` review strings. Prose ellipses
  such as `sha256:...` are illustrative only and are not valid runnable fixture
  input. [FIXTURES-REQ-001]
- Reviewed Core golden fixtures are derived from executable behavior, then
  checked in as exact bytes and exact digest review renderings.
  [FIXTURES-REQ-002]
- Reviewed authority-facts golden fixtures are derived from a validated JSON
  review/input document, then checked as exact canonical bytes and a
  domain-framed digest. [FIXTURES-REQ-005]
- Reviewed target-profile contract-resource fixtures are derived from one
  executable runtime-neutral model and checked as exact canonical bytes plus
  coordinate-framed digests. [FIXTURES-REQ-006]
- The provider contract pack is generated from reviewed ABI fragments and the
  five validated contract resources. Its checked manifest binds exact bytes,
  root mappings, digests, provenance, and Apache-2.0 licensing.
  [FIXTURES-REQ-007]
- Topic-shelf test plans may cite fixtures as executable evidence inputs. The
  contract graph check rejects fixture paths that do not exist.
  [FIXTURES-REQ-003]

## Deferred

The fixture constitution names future families for runtime-owned target
profiles, lawpacks, bundles, admission, and conformance. Those directories are
not populated yet; the present target-profile contract resources are
Edict-owned inputs, not a generated runtime target profile. They should be added
only when the owning implementation slice has executable behavior to verify.
[FIXTURES-REQ-004]

The verification matrix is tracked in [test-plan.md](./test-plan.md).
