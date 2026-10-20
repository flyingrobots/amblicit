//! Parameter calculations for the application-owned causal-cell example.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub key_scalars: u64,
    pub value_scalars: u64,
    pub replacement_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteBudgets {
    pub output: u64,
    pub write: u64,
    pub allocated: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitError {
    Zero,
    Overflow,
    ScalarBytesExceedCap,
}

impl Limits {
    pub fn budgets(self) -> Result<ByteBudgets, LimitError> {
        if self.key_scalars == 0 || self.value_scalars == 0 || self.replacement_bytes == 0 {
            return Err(LimitError::Zero);
        }
        let key_bytes = self
            .key_scalars
            .checked_mul(4)
            .ok_or(LimitError::Overflow)?;
        let value_bytes = self
            .value_scalars
            .checked_mul(4)
            .ok_or(LimitError::Overflow)?;
        if value_bytes > self.replacement_bytes {
            return Err(LimitError::ScalarBytesExceedCap);
        }
        // Canonical result is { key: text, value: text }, including all headers.
        let output_bytes = sum(&[
            1,
            text_size(3)?,
            text_size(key_bytes)?,
            text_size(5)?,
            text_size(value_bytes)?,
        ])?;
        // The selected create profile reserves 64 bytes beyond replacement data.
        let write_bytes = self
            .replacement_bytes
            .checked_add(64)
            .ok_or(LimitError::Overflow)?;
        // Input adds a 128-scalar basis field to the same two-field shape.
        let input_bytes = sum(&[output_bytes, text_size(5)?, text_size(128 * 4)?])?;
        let allocated_bytes = sum(&[input_bytes, output_bytes, self.replacement_bytes])?;
        Ok(ByteBudgets {
            output: output_bytes,
            write: write_bytes,
            allocated: allocated_bytes,
        })
    }
}

fn text_size(bytes: u64) -> Result<u64, LimitError> {
    let header = match bytes {
        0..=23 => 1,
        24..=255 => 2,
        256..=65_535 => 3,
        65_536..=4_294_967_295 => 5,
        _ => 9,
    };
    bytes.checked_add(header).ok_or(LimitError::Overflow)
}

fn sum(values: &[u64]) -> Result<u64, LimitError> {
    values.iter().try_fold(0_u64, |total, value| {
        total.checked_add(*value).ok_or(LimitError::Overflow)
    })
}
