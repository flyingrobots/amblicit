//! Literal raw vectors for the public checked numeric foundation.
use edict::numeric::{NumericError, Q32_32, Q32_32_PROFILE};

fn raw(value: Result<Q32_32, NumericError>) -> Result<i64, NumericError> {
    value.map(Q32_32::raw)
}

#[test]
fn public_numeric_profile_has_the_normative_identity() {
    assert_eq!(Q32_32_PROFILE, "bunny.q32_32.checked/v1");
}

#[test]
fn raw_values_preserve_bits_and_order() {
    let values = [i64::MIN, -4_294_967_296, -1, 0, 1, 4_294_967_296, i64::MAX];
    for value in values {
        assert_eq!(Q32_32::from_raw(value).raw(), value);
    }
    for pair in values.windows(2) {
        assert!(Q32_32::from_raw(pair[0]) < Q32_32::from_raw(pair[1]));
    }
}

#[test]
fn checked_linear_operations_preserve_exact_boundaries() {
    for (left, right, expected) in [
        (0, 0, 0),
        (4_294_967_296, -1, 4_294_967_295),
        (i64::MAX - 1, 1, i64::MAX),
        (i64::MIN + 1, -1, i64::MIN),
    ] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_add(Q32_32::from_raw(right))),
            Ok(expected)
        );
    }
    for (left, right, expected) in [
        (0, 0, 0),
        (4_294_967_296, 1, 4_294_967_295),
        (i64::MAX - 1, -1, i64::MAX),
        (i64::MIN + 1, 1, i64::MIN),
    ] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_sub(Q32_32::from_raw(right))),
            Ok(expected)
        );
    }
    for (left, right) in [(i64::MAX, 1), (i64::MIN, -1)] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_add(Q32_32::from_raw(right))),
            Err(NumericError::Overflow)
        );
    }
    for (left, right) in [(i64::MAX, -1), (i64::MIN, 1)] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_sub(Q32_32::from_raw(right))),
            Err(NumericError::Overflow)
        );
    }
    for (value, expected) in [(0, 0), (1, -1), (-1, 1), (i64::MAX, -i64::MAX)] {
        assert_eq!(raw(Q32_32::from_raw(value).checked_neg()), Ok(expected));
    }
    assert_eq!(
        raw(Q32_32::from_raw(i64::MIN).checked_neg()),
        Err(NumericError::Overflow)
    );
}

#[test]
fn multiplication_uses_signed_ties_to_even() {
    for (left, right, expected) in [
        // Exact scaled product = i64::MAX + 8_365_928 / 2^32.
        // The fractional remainder rounds down before the range check.
        (199_032_858_228_936, 199_032_871_303_925, i64::MAX),
        (1, 2_147_483_647, 0),
        (1, 2_147_483_648, 0),
        (1, 2_147_483_649, 1),
        (3, 2_147_483_648, 2),
        (5, 2_147_483_648, 2),
        (-1, 2_147_483_647, 0),
        (-1, 2_147_483_648, 0),
        (-1, 2_147_483_649, -1),
        (-3, 2_147_483_648, -2),
        (-5, 2_147_483_648, -2),
        (3, -2_147_483_648, -2),
        (-3, -2_147_483_648, 2),
        (i64::MAX, 4_294_967_296, i64::MAX),
        (i64::MIN, 4_294_967_296, i64::MIN),
    ] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_mul(Q32_32::from_raw(right))),
            Ok(expected),
            "{left} * {right}"
        );
    }
}

#[test]
fn division_uses_signed_ties_to_even() {
    for (left, right, expected) in [
        (1, 8_589_934_593, 0),
        (1, 8_589_934_592, 0),
        (1, 8_589_934_591, 1),
        (3, 8_589_934_592, 2),
        (5, 8_589_934_592, 2),
        (-1, 8_589_934_593, 0),
        (-1, 8_589_934_592, 0),
        (-1, 8_589_934_591, -1),
        (-3, 8_589_934_592, -2),
        (-5, 8_589_934_592, -2),
        (3, -8_589_934_592, -2),
        (-3, -8_589_934_592, 2),
        (i64::MAX, 4_294_967_296, i64::MAX),
        (i64::MIN, 4_294_967_296, i64::MIN),
    ] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_div(Q32_32::from_raw(right))),
            Ok(expected),
            "{left} / {right}"
        );
    }
}

#[test]
fn checked_products_and_quotients_refuse_invalid_results() {
    for (left, right) in [
        (i64::MAX, 8_589_934_592),
        (i64::MIN, 8_589_934_592),
        (i64::MIN, -4_294_967_296),
    ] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_mul(Q32_32::from_raw(right))),
            Err(NumericError::Overflow)
        );
    }
    for (left, right) in [(i64::MAX, 1), (i64::MIN, 1), (i64::MIN, -4_294_967_296)] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_div(Q32_32::from_raw(right))),
            Err(NumericError::Overflow)
        );
    }
    for left in [i64::MIN, -1, 0, 1, i64::MAX] {
        assert_eq!(
            raw(Q32_32::from_raw(left).checked_div(Q32_32::from_raw(0))),
            Err(NumericError::DivisionByZero)
        );
    }
}
