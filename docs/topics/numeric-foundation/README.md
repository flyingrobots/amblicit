# Numeric Foundation

The compiler library exposes a checked Q32.32 numeric API backed by exact
`bunny-num = "=0.6.0"`. The normative owner is the language specification's
[fixed-point numeric authority](../../SPEC_edict-language-v1.md#fixed-point-numeric-authority).
Bunny owns the arithmetic implementation; Edict selects its checked subset and
provides stable failures. [NUMERIC-REQ-001] [NUMERIC-REQ-002]

## Public boundary

`edict::numeric` exports `Q32_32`, `NumericError`, and `Q32_32_PROFILE`.
`edict_syntax::numeric` provides the same implementation to compiler consumers.
The profile value is `bunny.q32_32.checked/v1`; it is Edict's integration label,
not a new name for Bunny's SDL `q32.32` scalar profile.

| API | Contract |
| --- | --- |
| `Q32_32::from_raw` / `raw` | Preserve every raw signed i64 bit pattern, without integer scaling. |
| Equality / ordering | Compare raw values exactly. |
| `checked_add`, `checked_sub`, `checked_neg` | Return the exact result or `NumericError::Overflow`. |
| `checked_mul`, `checked_div` | Use Bunny's wide intermediate, ties-to-even quantization, then reject an out-of-range rounded result as `Overflow`. |
| Division by zero | Return `NumericError::DivisionByZero`, including zero divided by zero. |

The wrapper keeps Bunny's representation private and exposes no saturating
arithmetic traits or float conversions. Tiny nonzero products and quotients
may round to zero successfully. Expected raw values in the
[consumer tests](../../../crates/edict/tests/numeric_foundation.rs) are literal
oracles, not values calculated through Bunny. [NUMERIC-REQ-002]

## Compatibility and implementation limits

Existing integer semantics and canonical artifacts retain their meaning.
There is no automatic integer/fixed-point coercion. This library API is a
foundation for compiler consumers, not an implemented source-language folding
pass. Source fixed-point syntax, Core/Target tags and runtime execution are not
added by it. [NUMERIC-REQ-003]

Bunny's raw wire profile is eight-byte little-endian i64. Edict has no fixed-point
canonical-CBOR tag today. Future source lowering, encoding and runtime support
must satisfy the specification's explicit representation, capability and
conformance obligations before claiming this profile. Dependency upgrades must
review raw results, failures and identity compatibility; an exact manifest pin
must not float implicitly. The checked subset does not adopt every API Bunny
exports, and this foundation grants no application or provider authority.

## Ownership relationships

The specification owns the numeric law; this shelf owns the implementation
boundary and its verification map.

| Relationship | Targets |
| --- | --- |
| `refines` | [Fixed-point numeric authority](../../SPEC_edict-language-v1.md#fixed-point-numeric-authority) |
| `supersedes` | none |
| `depends_on` | [Pinned Bunny Numeric Constitution](https://github.com/flyingrobots/bunny/blob/9bf43600d08ff8e2a0ab888713948b409e386513/docs/NUMERIC_CONSTITUTION.md) |
| `related` | [Public Rust API](../public-rust-api/README.md), [Rust standards](../rust-standards/README.md), [Test plan](./test-plan.md) |
