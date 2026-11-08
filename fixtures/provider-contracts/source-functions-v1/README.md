# Source-function provider contract publication

This publication adds the optional nonempty Core `functions` table. Its
`edict-provider-contracts.cddl` and `manifest.json` are preserved together as
an exact publication. The current generator writes the separate
[bounded-state-fold publication](../bounded-state-fold-v1/README.md). Assembly without
the optional source-closure fragment still reproduces this pair exactly.

Consumers select these exact explicit bytes and verify the manifest's schema
SHA-256 and embedded byte identity. The API/domain coordinates retain v1; the
new raw schema digest distinguishes this publication from the preserved
[function-free v1 pair](../v1/README.md). A provider using the prior pair rejects
function-bearing Core and continues accepting its supported function-free
artifacts. Selecting the new schema alone is not executable support: lowerers,
independent verifiers and runtimes must validate and execute source functions
under their own declared capability, type, depth and budget contracts.

Both files retain the deterministic transport, root mappings, explicit Edict
resource closure and Apache-2.0 licensing of the prior publication. No consumer
is switched by source-coordinate discovery or by copying a new file over its
pinned provider package.
