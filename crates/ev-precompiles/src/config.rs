
use alloy_primitives::{Address, U256};
use std::collections::HashSet;
use std::str::FromStr;

/// Configuration for the native mint precompile.
#[derive(Clone, Debug)]
pub struct MintConfig {
    /// The address of the native mint precompile.
    pub precompile_address: Address,
    /// A static list of addresses that are allowed to call the precompile.
    pub allow_list: HashSet<Address>,
    /// The maximum amount that can be minted in a single call.
    pub per_call_cap: U256,
    /// The maximum amount that can be minted in a single block.
    pub per_block_cap: Option<U256>,
}

impl MintConfig {
    /// Creates a new `MintConfig` from environment variables.
    pub fn from_env() -> eyre::Result<Self> {
        let precompile_address = std::env::var("EV_MINT_PRECOMPILE_ADDR")
            .map(|s| Address::from_str(&s))??;

        let allow_list = std::env::var("EV_MINT_ALLOWLIST")?
            .split(',')
            .map(|s| Address::from_str(s.trim()))
            .collect::<Result<HashSet<_>, _>>()?;

        let per_call_cap = std::env::var("EV_MINT_PER_CALL_CAP")
            .map(|s| U256::from_str(&s))??;

        let per_block_cap = std::env::var("EV_MINT_PER_BLOCK_CAP")
            .ok()
            .map(|s| U256::from_str(&s))
            .transpose()?;

        Ok(Self {
            precompile_address,
            allow_list,
            per_call_cap,
            per_block_cap,
        })
    }
}
