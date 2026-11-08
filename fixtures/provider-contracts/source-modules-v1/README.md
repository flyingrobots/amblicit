# Source-module provider contract publication

This publication adds `source-closure` and `edict.source-closure/v1` to the
[self-contained source-function contract pack](../source-functions-v1/README.md).
The old CDDL and manifest pair remain byte-identical. This pair is preserved
as an exact publication; the current generator writes the separate
[bounded-state-fold publication](../bounded-state-fold-v1/README.md), and
tests reassemble this pair from the frozen pre-v3 Target IR schema.

The new root validates the closed canonical source-closure envelope, bounded
module-map cardinality and typed SHA-256 digests. It rejects a legacy source
byte string under the closure domain. Import pins, root membership,
reachability, topology and aggregate source-byte/depth budgets are semantic
checks owned by the source-closure API, not propositions proved by CDDL.

The generic contract-pack API/domain coordinates remain v1. Exact schema and
manifest bytes distinguish this publication. Consumers must select those bytes
explicitly; this does not switch a pinned provider, implement module-aware
application building, or grant runtime authority. The assembler has no source
repository, filesystem or network discovery. Its optional
`source_closure_cddl` field chooses the extension; omission reproduces the
source-function publication.
