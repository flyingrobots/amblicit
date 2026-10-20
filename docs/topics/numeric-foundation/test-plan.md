# Numeric Foundation Test Plan

## Scope

The checked compiler numeric API and normative Bunny Q32.32 profile. Source
syntax, Core fixed-point values, folding source programs, Target instructions,
and runtime admission remain outside this implementation.

## Requirements

| ID | Status | Requirement | Source |
| --- | --- | --- | --- |
| NUMERIC-REQ-001 | implemented | The public Edict numeric API delegates Q32.32 arithmetic to exact Bunny 0.6.0 and preserves every supplied raw i64 bit pattern. | docs/SPEC_edict-language-v1.md |
| NUMERIC-REQ-002 | implemented | Checked addition, subtraction, multiplication, division and negation return exact raw results or stable Overflow/DivisionByZero failures, with multiplication and division rounding ties to even before range checking. | docs/SPEC_edict-language-v1.md |
| NUMERIC-REQ-003 | implemented | Existing exact integer domains and canonical Core artifacts remain unchanged; fixed-point support introduces no implicit integer conversion. | docs/SPEC_edict-language-v1.md |

## Test Cases

| ID | Status | Category | Requirement | Oracle | Evidence | Fixtures | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| NUMERIC-TP-001 | implemented | Public API | NUMERIC-REQ-001 | The facade profile equals literal bunny.q32_32.checked/v1; zero, extrema and signed fractional raw values round-trip through facade-only imports and retain exact ordering. | public_numeric_profile_has_the_normative_identity, raw_values_preserve_bits_and_order | crates/edict/tests/numeric_foundation.rs | No float ingress or artifact encoding claim. |
| NUMERIC-TP-002 | implemented | Arithmetic boundaries | NUMERIC-REQ-002 | Literal add/sub/neg results include exact endpoints; one-unit overflow and minimum negation return Overflow. | checked_linear_operations_preserve_exact_boundaries | crates/edict/tests/numeric_foundation.rs | Expected values do not call Bunny. |
| NUMERIC-TP-003 | implemented | Rounding | NUMERIC-REQ-002 | Positive and negative multiplication/division below, above and exactly halfway round to literal ties-even raw results, including underflow to zero and an exact product above i64::MAX that rounds back into range. | multiplication_uses_signed_ties_to_even, division_uses_signed_ties_to_even | crates/edict/tests/numeric_foundation.rs | Native integer division retains its separate truncation rule. |
| NUMERIC-TP-004 | implemented | Structured refusal | NUMERIC-REQ-002 | Multiplication/division overflow return Overflow; zero divisors return DivisionByZero for zero, signed and endpoint dividends. | checked_products_and_quotients_refuse_invalid_results | crates/edict/tests/numeric_foundation.rs | No panic, saturation or wrapping. |
| NUMERIC-TP-005 | implemented | Compatibility | NUMERIC-REQ-003 | Existing integer-domain and canonical-golden tests retain exact accepted domains and artifacts. | fixed_width_integer_types_and_suffixes_preserve_exact_domains, signed_fixed_width_minima_preserve_exact_domains, out_of_range_u64_and_cross_width_values_reject_before_core, canonical_core_rejects_values_outside_their_declared_integer_domain | crates/edict-syntax/tests/operation_prerequisites.rs | Existing behavioral oracles; no new integer implementation. |

## Known Gaps

- Source fixed-point types, literals, operations and constant folding are not
  implemented. Core/Target representation and provider/runtime conformance need
  separately verified, hash-bound contracts before source support can claim them.
- This API has no floating-point ingress, decimal parser or canonical artifact
  encoder. Raw host values alone carry no package or execution authority.
