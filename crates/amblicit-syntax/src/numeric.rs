//! Checked fixed-point arithmetic for compiler consumers.
//!
//! Bunny 0.6.0 owns the arithmetic. This module selects its checked Q32.32
//! subset without exposing saturating operators or floating-point conversions.
//! It does not add source syntax, a Core value tag, or Target instructions.

use bunny_num::FixedQ32_32;

/// Edict's checked integration profile, distinct from Bunny's SDL `q32.32` name.
pub const Q32_32_PROFILE: &str = "bunny.q32_32.checked/v1";

/// Stable failures from checked fixed-point evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericError {
    /// The exact add/sub/neg result, or rounded mul/div result, exceeds i64.
    Overflow,
    /// Division has a zero raw divisor, including zero divided by zero.
    DivisionByZero,
}

/// A signed Q32.32 value whose mathematical value is `raw / 2^32`.
///
/// Every raw i64 is valid. Equality and ordering compare raw values exactly.
/// The Bunny representation stays private so callers cannot accidentally use
/// its saturating arithmetic operators through this checked API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Q32_32(FixedQ32_32);

impl Q32_32 {
    /// Preserves the supplied raw bits exactly, without integer scaling.
    #[must_use]
    pub const fn from_raw(raw: i64) -> Self {
        Self(FixedQ32_32::from_raw(raw))
    }

    /// Returns the exact signed raw representation, not a whole-number cast.
    #[must_use]
    pub const fn raw(self) -> i64 {
        self.0.raw()
    }

    /// Adds two raw values exactly through Bunny's checked arithmetic.
    ///
    /// # Errors
    /// Returns [`NumericError::Overflow`] when the exact sum cannot fit.
    pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    /// Subtracts two raw values exactly through Bunny's checked arithmetic.
    ///
    /// # Errors
    /// Returns [`NumericError::Overflow`] when the exact difference cannot fit.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, NumericError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    /// Negates a raw value exactly through Bunny's checked arithmetic.
    ///
    /// # Errors
    /// Returns [`NumericError::Overflow`] for the minimum raw i64 value.
    pub fn checked_neg(self) -> Result<Self, NumericError> {
        self.0.checked_neg().map(Self).ok_or(NumericError::Overflow)
    }

    /// Multiplies with Bunny's wide intermediate and ties-to-even rounding.
    ///
    /// Quantization precedes the range check; a tiny nonzero product may round
    /// to zero successfully.
    ///
    /// # Errors
    /// Returns [`NumericError::Overflow`] when the rounded raw result cannot fit.
    pub fn checked_mul(self, rhs: Self) -> Result<Self, NumericError> {
        self.0
            .checked_mul(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    /// Divides with Bunny's wide intermediate and ties-to-even rounding.
    ///
    /// Quantization precedes the range check; this is distinct from the
    /// truncation-toward-zero rule for Edict's exact signed integer division.
    ///
    /// # Errors
    /// Returns [`NumericError::DivisionByZero`] for a zero divisor, or
    /// [`NumericError::Overflow`] when the rounded raw quotient cannot fit.
    pub fn checked_div(self, rhs: Self) -> Result<Self, NumericError> {
        if rhs.raw() == 0 {
            return Err(NumericError::DivisionByZero);
        }
        self.0
            .checked_div(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }
}
