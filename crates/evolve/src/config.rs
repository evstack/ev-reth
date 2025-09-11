use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Default maximum bytes for txpool transactions (1.85 MiB)
pub const DEFAULT_MAX_TXPOOL_BYTES: u64 = 1_939_865; // 1.85 MiB = 1,939,865 bytes

/// Default maximum gas for txpool transactions selection
/// This caps how much total gas worth of transactions the txpool RPC returns.
pub const DEFAULT_MAX_TXPOOL_GAS: u64 = 30_000_000; // 30M gas

/// Configuration for Rollkit-specific functionality
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollkitConfig {
    /// Maximum bytes of transactions to return from the txpool
    pub max_txpool_bytes: u64,
    /// Maximum gas of transactions to return from the txpool
    pub max_txpool_gas: u64,
}

impl Default for RollkitConfig {
    fn default() -> Self {
        Self {
            max_txpool_bytes: DEFAULT_MAX_TXPOOL_BYTES,
            max_txpool_gas: DEFAULT_MAX_TXPOOL_GAS,
        }
    }
}

impl RollkitConfig {
    /// Creates a new `RollkitConfig` with the given max txpool bytes
    pub const fn new(max_txpool_bytes: u64) -> Self {
        Self {
            max_txpool_bytes,
            max_txpool_gas: DEFAULT_MAX_TXPOOL_GAS,
        }
    }

    /// Creates a new `RollkitConfig` with the given max txpool bytes and gas
    pub const fn new_with_gas(max_txpool_bytes: u64, max_txpool_gas: u64) -> Self {
        Self {
            max_txpool_bytes,
            max_txpool_gas,
        }
    }
}

/// Tracks the most recent effective block gas limit selected by the payload builder.
///
/// Initialized to the default txpool gas cap so selection has a sensible value
/// before the first payload is built.
pub static CURRENT_BLOCK_GAS_LIMIT: AtomicU64 = AtomicU64::new(DEFAULT_MAX_TXPOOL_GAS);

/// Helper to set the current block gas limit.
#[inline]
pub fn set_current_block_gas_limit(gas_limit: u64) {
    CURRENT_BLOCK_GAS_LIMIT.store(gas_limit, Ordering::Relaxed);
}

/// Helper to read the current block gas limit.
#[inline]
pub fn current_block_gas_limit() -> u64 {
    CURRENT_BLOCK_GAS_LIMIT.load(Ordering::Relaxed)
}
