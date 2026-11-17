//! This module owns the selection of the Core work schedule that converts
//! evaluation into the steps and bytes an intent budget bounds.

/// The Core work schedule a module is compiled and admitted under.
///
/// The copying schedule (`edict.core-cost/v1`) is the default and matches a
/// runtime that copies values on every read. The shared-read schedule
/// (`edict.core-cost/v2`) charges reads, arguments, results and fold state one
/// step and sizes only construction. It is recorded in Core as the required
/// capability `core.cost.shared-reads/v1`, so a consumer that does not meter
/// shared reads refuses the module instead of running it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoreCostSchedule {
    /// `edict.core-cost/v1`: every evaluated value is charged at full size.
    #[default]
    Copying,
    /// `edict.core-cost/v2`: values are shared; only construction is sized.
    SharedReads,
}
