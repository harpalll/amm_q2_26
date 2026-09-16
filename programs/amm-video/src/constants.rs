use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";

/// Precision multiplier for curve math (10^6 for 6-decimal tokens).
/// Note: `ConstantProduct::init` takes decimals (e.g. `Some(6)`), but the
/// static `xy_*_amounts_from_l` helpers take the expanded precision value.
pub const PRECISION: u32 = 1_000_000;
