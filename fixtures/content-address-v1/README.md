# ContentAddressV1 identity vectors

Six frozen identities from Echo's `ContentAddressV1` law, vendored byte for
byte. The source fixture
[`content-address-v1.amblicit`](../lang/intrinsics/content-address-v1.amblicit)
computes one output per node or edge vector. A provider that runs the compiled
fixture on the inputs below must return exactly these identities.

Amblicit does not evaluate identity intrinsics. Its tests check that Echo's
catalog is admitted and that each output reaches its intrinsic with exactly the
inputs listed here (`crates/amblicit-provider-schema/tests/content_address_v1_vectors.rs`).
Echo checks the vectors against an independent preimage of its identity law.

Each file is one lowercase, 32-byte hexadecimal digest followed by a newline.

## Source

The values are Echo's frozen vectors in
`crates/warp-core/tests/content_address_v1_tests.rs#70@b611df32cbc8c8a46e38aa1b44be583082af3eab`.
The files have the same names and bytes as Echo's
`crates/warp-core/tests/fixtures/content-address-v1/` in
[flyingrobots/echo#972](https://github.com/flyingrobots/echo/pull/972) at
`ea9a42a5dbdc719a124a63c9d3c5a7d7285fbc66`. Echo's
[identity law](https://github.com/flyingrobots/echo/blob/b611df32cbc8c8a46e38aa1b44be583082af3eab/docs/architecture/authored-operation-effects.md#identity-laws)
owns the domain separators, framing, sorting and duplicate-edge refusal. Do
not regenerate these files from an implementation under test.

## Inputs

Every identifier is 32 repetitions of the named byte. The atom is the three
raw bytes `6e6577` (`new`), not a CBOR string.

| Input | Byte |
| --- | --- |
| `warp` | `03` |
| `nodeType` | `07` |
| `atomType` | `02` |
| `edgeA` | `06` |
| `edgeB` | `08` |
| `low` | `04` |
| `high` | `05` |

| File | Fixture output | Identity inputs beyond the common values |
| --- | --- | --- |
| `attachment-new.hex` | none | Atom type and atom bytes |
| `node-zero-edges.hex` | `nodeZeroEdges` | No outgoing edges |
| `node-one-edge.hex` | `nodeOneEdge` | One outgoing edge: A to high |
| `node-two-edges.hex` | `nodeTwoEdges`, `nodeTwoEdgesReordered` | B to low and A to high, in either input order |
| `content-edge.hex` | `contentEdge` | Source low, edge type A, target high |
| `slot-edge.hex` | `slotEdge` | Source low, edge type A; no target |

The attachment digest is not a separate source call; `contentAddress` computes
it from the atom type and atom bytes. Its vector checks that step on its own.
