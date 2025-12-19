//! Token Duality Precompile
//!
//! Enables native tokens to be used as ERC-20 compatible tokens without wrapping.
//! Based on the Celo token duality pattern, adapted for the Evolve ecosystem.
//!
//! ## Overview
//!
//! This precompile allows smart contracts to interact with native tokens using
//! familiar ERC-20 patterns. The key function is `transfer(from, to, amount)`
//! which directly manipulates native balances.
//!
//! ## Important: ERC-20 Compatibility
//!
//! **This precompile is NOT a full ERC-20 implementation.** It provides only the
//! transfer mechanism for native tokens. To achieve full ERC-20 compatibility,
//! deploy an ERC-20 wrapper contract that:
//!
//! - Calls this precompile for `transfer()` and `transferFrom()` operations
//! - Provides `totalSupply()`, `balanceOf()`, `approve()`, `allowance()` views
//! - Emits `Transfer` and `Approval` events
//!
//! This design follows the Celo token duality pattern where the native token
//! precompile and ERC-20 wrapper are separate components.
//!
//! ## Address
//!
//! The precompile is deployed at address `0x00..00FD` (253).
//!
//! ## Interface
//!
//! ```solidity
//! interface ITokenDuality {
//!     function transfer(address from, address to, uint256 amount) external;
//!     function addToAllowList(address account) external;
//!     function removeFromAllowList(address account) external;
//!     function allowlist(address account) external view returns (bool);
//!     function transferredThisBlock() external view returns (uint256);
//!     function perCallCap() external view returns (uint256);
//!     function perBlockCap() external view returns (uint256);
//! }
//! ```
//!
//! ## Features
//!
//! - **Native token transfers**: Direct balance manipulation via precompile interface
//! - **Admin authorization**: Only admin or allowlisted addresses can transfer
//! - **Rate limiting**: Optional per-call and per-block transfer caps
//! - **Block tracking**: Automatic reset of transfer counters per block
//!
//! ## Security Properties
//!
//! - **Authorization**: Only admin or allowlisted callers can invoke transfer
//! - **Rate limiting**: Prevents large-scale token movements in single call/block
//! - **Zero address protection**: Transfers to zero address are rejected
//! - **Overflow protection**: All arithmetic uses checked operations
//! - **Atomic operations**: State changes are atomic (all-or-nothing)
//! - **No reentrancy risk**: Precompiles execute natively without delegatecall
//!
//! ## Storage Layout
//!
//! The precompile uses the following storage slots in its account (`0x..FD`):
//!
//! | Slot | Description |
//! |------|-------------|
//! | `U256(address)` | Allowlist entry (1 = allowed, 0 = not allowed) |
//!
//! Note: The allowlist key is derived directly from the address. Since addresses
//! are 20 bytes and occupy the lower bytes of a 32-byte word, there's no collision
//! risk with other storage as long as no other data is stored in this account.
//!
//! ## Gas Costs
//!
//! Gas is accounted at the transaction level. The precompile returns `gas_used = 0`
//! as the EVM charges gas based on the call context. Recommended external gas
//! estimation for callers:
//!
//! - Base operation: ~9,000 gas (similar to Celo token duality)
//! - State reads (allowlist check): ~2,100 gas per SLOAD
//! - State writes (balance update): ~20,000 gas per SSTORE
//!
//! ## Usage Example
//!
//! ```solidity
//! // In your ERC-20 token contract
//! address constant TOKEN_DUALITY = address(0xFD);
//!
//! function transfer(address to, uint256 amount) external returns (bool) {
//!     (bool success,) = TOKEN_DUALITY.call(
//!         abi.encodeCall(ITokenDuality.transfer, (msg.sender, to, amount))
//!     );
//!     require(success, "Transfer failed");
//!     return true;
//! }
//! ```
//!
//! ## References
//!
//! - [Celo Token Duality Specification](https://specs.celo.org/token_duality.html)
//! - [ERC-20 Token Standard](https://eips.ethereum.org/EIPS/eip-20)

use alloy::{
    sol,
    sol_types::{SolInterface, SolValue},
};
use alloy_evm::{
    precompiles::{Precompile, PrecompileInput},
    revm::precompile::{PrecompileError, PrecompileId, PrecompileResult},
    EvmInternals, EvmInternalsError,
};
use alloy_primitives::{address, Address, Bytes, U256};
use parking_lot::RwLock;
use revm::{bytecode::Bytecode, precompile::PrecompileOutput};
use std::sync::{Arc, OnceLock};

sol! {
    /// Token Duality interface for native token as ERC-20
    interface ITokenDuality {
        /// Transfer native tokens between addresses
        /// @param from Source address
        /// @param to Destination address
        /// @param amount Amount to transfer in wei
        function transfer(address from, address to, uint256 amount) external;

        /// Add an address to the authorized callers list
        /// @param account Address to authorize
        function addToAllowList(address account) external;

        /// Remove an address from the authorized callers list
        /// @param account Address to remove
        function removeFromAllowList(address account) external;

        /// Check if an address is in the allowlist
        /// @param account Address to check
        /// @return True if the address is authorized
        function allowlist(address account) external view returns (bool);

        /// Get total amount transferred in current block
        /// @return Total transferred amount in wei
        function transferredThisBlock() external view returns (uint256);

        /// Get the per-call transfer cap
        /// @return Maximum amount per single transfer
        function perCallCap() external view returns (uint256);

        /// Get the per-block transfer cap
        /// @return Maximum total amount per block
        function perBlockCap() external view returns (uint256);
    }
}

/// Token Duality Precompile Address: 0x00..fd (253)
pub const TOKEN_DUALITY_PRECOMPILE_ADDR: Address =
    address!("0x00000000000000000000000000000000000000fd");

// =============================================================================
// Gas Cost Constants (for external reference only)
// =============================================================================
//
// IMPORTANT: Gas is charged at the transaction level by Revm/EVM, NOT by this
// precompile. These constants are provided for documentation and external gas
// estimation purposes only. They should NOT be used in precompile logic.
//
// Gas costs vary by EVM hardfork:
// - Cold SSTORE: 20,000 gas (EIP-2929)
// - Warm SSTORE: 5,000 gas
// - Cold SLOAD: 2,100 gas (EIP-2929)
// - Warm SLOAD: 100 gas

/// Base gas cost for token duality operations (Celo-compatible).
pub const GAS_BASE: u64 = 9_000;

/// Gas cost per storage read (SLOAD, cold).
pub const GAS_SLOAD: u64 = 2_100;

/// Gas cost per storage write (SSTORE, cold).
pub const GAS_SSTORE_COLD: u64 = 20_000;

/// Estimated gas for a transfer operation (base + 2 balance updates).
pub const GAS_TRANSFER_ESTIMATE: u64 = GAS_BASE + (2 * GAS_SSTORE_COLD);

/// Estimated gas for an allowlist check.
pub const GAS_ALLOWLIST_CHECK: u64 = GAS_BASE + GAS_SLOAD;

// =============================================================================
// Default Rate Limiting Configuration
// =============================================================================

/// Default per-call cap: 1 million tokens (with 18 decimals)
const DEFAULT_PER_CALL_CAP: u128 = 1_000_000;

/// Default per-block cap: 10 million tokens (with 18 decimals)
const DEFAULT_PER_BLOCK_CAP: u128 = 10_000_000;

/// Multiplier for token decimals (10^18)
fn decimals_multiplier() -> U256 {
    U256::from(10u64).pow(U256::from(18))
}

/// Configuration for the Token Duality Precompile
#[derive(Clone, Debug)]
pub struct TokenDualityConfig {
    /// Admin address that can manage the allowlist
    pub admin: Address,
    /// Maximum amount per single transfer (None = unlimited)
    pub per_call_cap: Option<U256>,
    /// Maximum total amount per block (None = unlimited)
    pub per_block_cap: Option<U256>,
}

impl Default for TokenDualityConfig {
    /// Default configuration for testing only.
    ///
    /// # Warning
    ///
    /// This creates a configuration with `Address::ZERO` as admin, which is
    /// NOT production-ready. Use [`TokenDualityConfig::with_admin`] or
    /// [`TokenDualityConfig::new`] for production deployments.
    ///
    /// The default caps are:
    /// - Per-call: 1 million tokens (10^24 wei)
    /// - Per-block: 10 million tokens (10^25 wei)
    fn default() -> Self {
        Self {
            admin: Address::ZERO,
            per_call_cap: Some(U256::from(DEFAULT_PER_CALL_CAP) * decimals_multiplier()),
            per_block_cap: Some(U256::from(DEFAULT_PER_BLOCK_CAP) * decimals_multiplier()),
        }
    }
}

impl TokenDualityConfig {
    /// Create config with only admin (no caps)
    ///
    /// # Panics
    /// Panics if admin is the zero address.
    pub fn with_admin(admin: Address) -> Self {
        assert!(
            !admin.is_zero(),
            "token duality admin cannot be zero address"
        );
        Self {
            admin,
            per_call_cap: None,
            per_block_cap: None,
        }
    }

    /// Create config with admin and caps
    ///
    /// # Panics
    /// - Panics if admin is the zero address.
    /// - Panics if per_call_cap exceeds per_block_cap.
    pub fn new(admin: Address, per_call_cap: Option<U256>, per_block_cap: Option<U256>) -> Self {
        assert!(
            !admin.is_zero(),
            "token duality admin cannot be zero address"
        );

        // Validate cap relationship
        if let (Some(call_cap), Some(block_cap)) = (per_call_cap, per_block_cap) {
            assert!(
                call_cap <= block_cap,
                "per_call_cap ({call_cap}) cannot exceed per_block_cap ({block_cap})"
            );
        }

        Self {
            admin,
            per_call_cap,
            per_block_cap,
        }
    }

    /// Check if the configuration is valid for production use
    pub fn is_production_ready(&self) -> bool {
        !self.admin.is_zero()
    }
}

/// Per-block transfer tracking state.
///
/// # Architecture Assumption
///
/// This tracker assumes that a single precompile instance is shared across all
/// transactions within a block. This is the case in Reth's architecture where:
///
/// 1. A single `EvEvm` instance is created per block during payload building
/// 2. All transactions in that block share the same EVM and precompile instances
/// 3. The `Arc<RwLock>` ensures thread-safe access for parallel transaction execution
///
/// If a deployment uses multiple independent EVM instances for the same block,
/// each would have its own tracker and the per-block cap could be exceeded.
/// In such cases, consider moving the tracker to persistent state storage.
///
/// # Block Number Tracking
///
/// The tracker resets when `block_number` changes. This assumes block numbers
/// increase monotonically within a normal execution context. Reorgs during
/// testing may cause the tracker to accumulate transfers across different blocks.
#[derive(Clone, Debug, Default)]
struct BlockTransferTracker {
    block_number: u64,
    total_transferred: U256,
}

/// Token Duality Precompile
///
/// Enables native tokens to function as ERC-20 compatible tokens.
#[derive(Clone, Debug)]
pub struct TokenDualityPrecompile {
    config: TokenDualityConfig,
    block_tracker: Arc<RwLock<BlockTransferTracker>>,
}

impl TokenDualityPrecompile {
    /// Lazily-initialized precompile ID
    pub fn id() -> &'static PrecompileId {
        static ID: OnceLock<PrecompileId> = OnceLock::new();
        ID.get_or_init(|| PrecompileId::custom("token_duality"))
    }

    /// Bytecode marker for the precompile account.
    ///
    /// Precompile accounts are marked with the invalid instruction `0xFE`
    /// to prevent them from being cleared during state root pruning,
    /// while also indicating they shouldn't be executed as normal contracts.
    /// This is standard practice for EVM precompile implementations.
    fn bytecode() -> &'static Bytecode {
        static BYTECODE: OnceLock<Bytecode> = OnceLock::new();
        BYTECODE.get_or_init(|| Bytecode::new_raw(Bytes::from_static(&[0xFE])))
    }

    /// Create new precompile with configuration
    pub fn new(config: TokenDualityConfig) -> Self {
        Self {
            config,
            block_tracker: Arc::new(RwLock::new(BlockTransferTracker::default())),
        }
    }

    /// Create precompile with admin address only (no caps)
    pub fn with_admin(admin: Address) -> Self {
        Self::new(TokenDualityConfig::with_admin(admin))
    }

    /// Get the admin address
    pub fn admin(&self) -> Address {
        self.config.admin
    }

    /// Get the per-call cap
    pub fn per_call_cap(&self) -> Option<U256> {
        self.config.per_call_cap
    }

    /// Get the per-block cap
    pub fn per_block_cap(&self) -> Option<U256> {
        self.config.per_block_cap
    }

    // === Error Handling ===

    fn map_internals_error(err: EvmInternalsError) -> PrecompileError {
        PrecompileError::Other(err.to_string())
    }

    // === Account Management ===

    fn ensure_account_created(
        internals: &mut EvmInternals<'_>,
        addr: Address,
    ) -> Result<(), PrecompileError> {
        let account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;

        if account.is_loaded_as_not_existing() {
            if addr == TOKEN_DUALITY_PRECOMPILE_ADDR {
                internals.set_code(addr, Self::bytecode().clone());
                internals.nonce_bump_journal_entry(addr);
            }
            internals.touch_account(addr);
        }

        Ok(())
    }

    // === Balance Operations ===

    /// Adds balance to an account with overflow protection.
    ///
    /// # Safety
    ///
    /// Uses `checked_add` to verify the addition won't overflow before
    /// calling `set_balance`. This mirrors the implementation of `sub_balance`
    /// for consistency and avoids potential double account loads.
    fn add_balance(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        amount: U256,
    ) -> Result<(), PrecompileError> {
        let account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;

        // SECURITY: Verify addition won't overflow before mutating state.
        let new_balance = account
            .info
            .balance
            .checked_add(amount)
            .ok_or_else(|| PrecompileError::Other("balance overflow".to_string()))?;

        internals
            .set_balance(addr, new_balance)
            .map_err(Self::map_internals_error)?;
        Ok(())
    }

    fn sub_balance(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        amount: U256,
    ) -> Result<(), PrecompileError> {
        let account = internals
            .load_account(addr)
            .map_err(Self::map_internals_error)?;
        let new_balance = account
            .info
            .balance
            .checked_sub(amount)
            .ok_or_else(|| PrecompileError::Other("insufficient balance".to_string()))?;
        internals
            .set_balance(addr, new_balance)
            .map_err(Self::map_internals_error)?;
        Ok(())
    }

    // === Authorization ===

    fn ensure_admin(&self, caller: Address) -> Result<(), PrecompileError> {
        if caller == self.config.admin {
            Ok(())
        } else {
            Err(PrecompileError::Other("unauthorized caller".to_string()))
        }
    }

    fn ensure_authorized(
        &self,
        internals: &mut EvmInternals<'_>,
        caller: Address,
    ) -> Result<(), PrecompileError> {
        if caller == self.config.admin {
            tracing::debug!(target: "token_duality", ?caller, "authorization granted: admin");
            return Ok(());
        }

        let allowlisted = Self::is_allowlisted(internals, caller)?;
        if allowlisted {
            tracing::debug!(target: "token_duality", ?caller, "authorization granted: allowlist");
            Ok(())
        } else {
            tracing::warn!(target: "token_duality", ?caller, "authorization denied");
            Err(PrecompileError::Other("unauthorized caller".to_string()))
        }
    }

    // === Allowlist Storage ===

    /// Check if an address is in the allowlist.
    ///
    /// Note: This is a read-only operation but requires `&mut EvmInternals`
    /// because Revm's `sload` requires mutable journal access for warm/cold
    /// slot tracking. No state is actually modified by this function.
    fn is_allowlisted(
        internals: &mut EvmInternals<'_>,
        addr: Address,
    ) -> Result<bool, PrecompileError> {
        Self::ensure_account_created(internals, TOKEN_DUALITY_PRECOMPILE_ADDR)?;
        let key = Self::allowlist_key(addr);
        let value = internals
            .sload(TOKEN_DUALITY_PRECOMPILE_ADDR, key)
            .map_err(Self::map_internals_error)?;
        let raw_value = *value;
        let allowlisted = !raw_value.is_zero();
        tracing::debug!(
            target: "token_duality",
            ?addr,
            slot = %key,
            value = %raw_value,
            allowlisted,
            "allowlist lookup"
        );
        Ok(allowlisted)
    }

    fn set_allowlisted(
        internals: &mut EvmInternals<'_>,
        addr: Address,
        allowed: bool,
    ) -> Result<(), PrecompileError> {
        Self::ensure_account_created(internals, TOKEN_DUALITY_PRECOMPILE_ADDR)?;
        let value = if allowed { U256::from(1) } else { U256::ZERO };
        internals
            .sstore(
                TOKEN_DUALITY_PRECOMPILE_ADDR,
                Self::allowlist_key(addr),
                value,
            )
            .map_err(Self::map_internals_error)?;
        internals.touch_account(TOKEN_DUALITY_PRECOMPILE_ADDR);
        Ok(())
    }

    /// Converts an address to a storage key for the allowlist.
    ///
    /// Uses the address's 32-byte word representation (zero-padded on the left)
    /// for deterministic storage slot derivation.
    ///
    /// Note: `U256::from(addr)` is not available in this version of alloy_primitives,
    /// so we use `into_word().into()` which achieves the same result.
    fn allowlist_key(addr: Address) -> U256 {
        addr.into_word().into()
    }

    // === Rate Limiting ===

    fn validate_and_track_transfer(
        &self,
        amount: U256,
        block_number: u64,
    ) -> Result<(), PrecompileError> {
        // Check per-call cap
        if let Some(cap) = self.config.per_call_cap {
            if amount > cap {
                return Err(PrecompileError::Other(format!(
                    "transfer exceeds per-call cap: {amount} > {cap}"
                )));
            }
        }

        // Check and update per-block cap
        if let Some(cap) = self.config.per_block_cap {
            // parking_lot::RwLock never poisons - safe to use directly
            let mut tracker = self.block_tracker.write();

            // Reset tracker on new block
            if tracker.block_number != block_number {
                tracker.block_number = block_number;
                tracker.total_transferred = U256::ZERO;
            }

            let new_total = tracker
                .total_transferred
                .checked_add(amount)
                .ok_or_else(|| PrecompileError::Other("transfer tracking overflow".to_string()))?;

            if new_total > cap {
                return Err(PrecompileError::Other(format!(
                    "transfer exceeds per-block cap: {new_total} > {cap}"
                )));
            }

            tracker.total_transferred = new_total;
        }

        Ok(())
    }

    fn get_transferred_this_block(&self) -> U256 {
        // parking_lot::RwLock never poisons - safe to use directly
        self.block_tracker.read().total_transferred
    }

    // === Transfer Execution ===

    /// Execute a native token transfer.
    ///
    /// # Atomicity
    ///
    /// State changes are atomic via Revm's JournaledState. If any operation fails
    /// (e.g., insufficient balance), the entire transaction is reverted by the EVM.
    /// This prevents partial state writes that could lead to fund loss or corruption.
    ///
    /// # Validation Order
    ///
    /// 1. Recipient validation (non-zero)
    /// 2. Amount validation (skip zero)
    /// 3. Self-transfer optimization
    /// 4. Rate limit validation
    /// 5. Balance operations (atomic via journal)
    fn execute_transfer(
        &self,
        internals: &mut EvmInternals<'_>,
        from: Address,
        to: Address,
        amount: U256,
        block_number: u64,
    ) -> Result<(), PrecompileError> {
        // 1. Validate recipient
        if to.is_zero() {
            return Err(PrecompileError::Other(
                "cannot transfer to zero address".to_string(),
            ));
        }

        // 2. Skip zero amount transfers
        if amount.is_zero() {
            return Ok(());
        }

        // 3. Skip self-transfers (no-op optimization)
        if from == to {
            tracing::debug!(target: "token_duality", ?from, "skipping self-transfer");
            return Ok(());
        }

        // 4. Validate and track rate limits
        self.validate_and_track_transfer(amount, block_number)?;

        tracing::info!(
            target: "token_duality",
            ?from,
            ?to,
            %amount,
            block_number,
            "executing transfer"
        );

        // 5. Execute balance transfer (atomic via JournaledState)
        // If sub_balance fails, the entire precompile call fails and
        // no state changes are committed.
        Self::ensure_account_created(internals, from)?;
        Self::ensure_account_created(internals, to)?;

        Self::sub_balance(internals, from, amount)?;
        Self::add_balance(internals, to, amount)?;

        // Mark accounts as touched for state trie updates
        internals.touch_account(from);
        internals.touch_account(to);

        tracing::info!(target: "token_duality", "transfer successful");
        Ok(())
    }
}

impl Default for TokenDualityPrecompile {
    /// Default precompile for testing only.
    ///
    /// # Warning
    ///
    /// This creates a precompile with `Address::ZERO` as admin, which is
    /// NOT production-ready. Use [`TokenDualityPrecompile::with_admin`] or
    /// [`TokenDualityPrecompile::new`] for production deployments.
    ///
    /// The `is_production_ready()` check on the config will return `false`
    /// for precompiles created with this default.
    fn default() -> Self {
        Self::new(TokenDualityConfig::default())
    }
}

impl Precompile for TokenDualityPrecompile {
    fn precompile_id(&self) -> &PrecompileId {
        Self::id()
    }

    fn call(&self, mut input: PrecompileInput<'_>) -> PrecompileResult {
        let caller: Address = input.caller;
        let gas_limit = input.gas;
        let data_len = input.data.len();

        tracing::info!(
            target: "token_duality",
            ?caller,
            gas = gas_limit,
            calldata_len = data_len,
            "precompile call invoked"
        );

        // Decode ABI
        let decoded = match ITokenDuality::ITokenDualityCalls::abi_decode(input.data) {
            Ok(v) => v,
            Err(e) => return Err(PrecompileError::Other(e.to_string())),
        };
        let internals = input.internals_mut();

        // Get block number for rate limiting
        let block_number = internals.block_number().to::<u64>();

        // Dispatch to handler
        match decoded {
            ITokenDuality::ITokenDualityCalls::transfer(call) => {
                self.ensure_authorized(internals, caller)?;
                self.execute_transfer(internals, call.from, call.to, call.amount, block_number)?;
                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            ITokenDuality::ITokenDualityCalls::addToAllowList(call) => {
                self.ensure_admin(caller)?;
                Self::set_allowlisted(internals, call.account, true)?;
                tracing::info!(target: "token_duality", account = ?call.account, "added to allowlist");
                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            ITokenDuality::ITokenDualityCalls::removeFromAllowList(call) => {
                self.ensure_admin(caller)?;
                Self::set_allowlisted(internals, call.account, false)?;
                tracing::info!(target: "token_duality", account = ?call.account, "removed from allowlist");
                Ok(PrecompileOutput::new(0, Bytes::new()))
            }
            ITokenDuality::ITokenDualityCalls::allowlist(call) => {
                let is_allowed = Self::is_allowlisted(internals, call.account)?;
                let result = is_allowed.abi_encode();
                Ok(PrecompileOutput::new(0, result.into()))
            }
            ITokenDuality::ITokenDualityCalls::transferredThisBlock(_) => {
                let transferred = self.get_transferred_this_block();
                let result = transferred.abi_encode();
                Ok(PrecompileOutput::new(0, result.into()))
            }
            ITokenDuality::ITokenDualityCalls::perCallCap(_) => {
                let cap = self.config.per_call_cap.unwrap_or(U256::MAX);
                let result = cap.abi_encode();
                Ok(PrecompileOutput::new(0, result.into()))
            }
            ITokenDuality::ITokenDualityCalls::perBlockCap(_) => {
                let cap = self.config.per_block_cap.unwrap_or(U256::MAX);
                let result = cap.abi_encode();
                Ok(PrecompileOutput::new(0, result.into()))
            }
        }
    }

    fn is_pure(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::sol_types::SolCall;
    use alloy_primitives::address;
    use revm::{
        context::{
            journal::{Journal, JournalInner},
            BlockEnv,
        },
        database::{CacheDB, EmptyDB},
        primitives::hardfork::SpecId,
    };

    type TestJournal = Journal<CacheDB<EmptyDB>>;

    const GAS_LIMIT: u64 = 1_000_000;

    fn setup_context() -> (TestJournal, BlockEnv) {
        let mut journal = Journal::new_with_inner(CacheDB::default(), JournalInner::new());
        journal.inner.set_spec_id(SpecId::PRAGUE);
        let block_env = BlockEnv::default();
        (journal, block_env)
    }

    fn run_call<'a>(
        journal: &'a mut TestJournal,
        block_env: &'a BlockEnv,
        precompile: &TokenDualityPrecompile,
        caller: Address,
        data: &'a [u8],
    ) -> PrecompileResult {
        let input = PrecompileInput {
            data,
            gas: GAS_LIMIT,
            caller,
            value: U256::ZERO,
            target_address: TOKEN_DUALITY_PRECOMPILE_ADDR,
            bytecode_address: TOKEN_DUALITY_PRECOMPILE_ADDR,
            internals: EvmInternals::new(journal, block_env),
        };

        precompile.call(input)
    }

    fn account_balance(journal: &TestJournal, address: Address) -> Option<U256> {
        journal
            .inner
            .state
            .get(&address)
            .map(|account| account.info.balance)
    }

    fn set_balance(journal: &mut TestJournal, address: Address, balance: U256) {
        use revm::state::AccountInfo;
        journal.inner.state.entry(address).or_default().info = AccountInfo {
            balance,
            nonce: 0,
            code_hash: Default::default(),
            code: None,
        };
    }

    // === Test: Transfer Success ===

    #[test]
    fn transfer_moves_balance() {
        let admin = address!("0x00000000000000000000000000000000000000a1");
        let sender = address!("0x00000000000000000000000000000000000000b1");
        let recipient = address!("0x00000000000000000000000000000000000000c1");
        let amount = U256::from(1000u64);
        let initial_balance = U256::from(5000u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, initial_balance);

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();

        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("transfer should succeed");
        assert_eq!(output.gas_used, 0, "precompile should not consume gas");

        let sender_balance = account_balance(&journal, sender).expect("sender exists");
        let recipient_balance = account_balance(&journal, recipient).expect("recipient exists");

        assert_eq!(
            sender_balance,
            initial_balance - amount,
            "sender balance should decrease"
        );
        assert_eq!(recipient_balance, amount, "recipient should receive amount");
    }

    // === Test: Unauthorized Caller ===

    #[test]
    fn unauthorized_caller_is_denied() {
        let admin = address!("0x00000000000000000000000000000000000000a2");
        let unauthorized = address!("0x00000000000000000000000000000000000000ff");
        let sender = address!("0x00000000000000000000000000000000000000b2");
        let recipient = address!("0x00000000000000000000000000000000000000c2");
        let amount = U256::from(100u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();

        let result = run_call(
            &mut journal,
            &block_env,
            &precompile,
            unauthorized,
            &calldata,
        );

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(msg, "unauthorized caller", "expected unauthorized error")
            }
            other => panic!("expected unauthorized error, got {other:?}"),
        }
    }

    // === Test: Allowlist Authorization ===

    #[test]
    fn allowlisted_caller_can_transfer() {
        let admin = address!("0x00000000000000000000000000000000000000a3");
        let operator = address!("0x00000000000000000000000000000000000000b3");
        let sender = address!("0x00000000000000000000000000000000000000c3");
        let recipient = address!("0x00000000000000000000000000000000000000d3");
        let amount = U256::from(500u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        // Add operator to allowlist
        let add_calldata = ITokenDuality::addToAllowListCall { account: operator }.abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("admin should add to allowlist");

        // Transfer as operator
        let transfer_calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();
        run_call(
            &mut journal,
            &block_env,
            &precompile,
            operator,
            &transfer_calldata,
        )
        .expect("allowlisted operator should transfer");

        let recipient_balance = account_balance(&journal, recipient).expect("recipient exists");
        assert_eq!(recipient_balance, amount, "recipient receives amount");
    }

    // === Test: Remove from Allowlist ===

    #[test]
    fn removing_from_allowlist_revokes_access() {
        let admin = address!("0x00000000000000000000000000000000000000a4");
        let operator = address!("0x00000000000000000000000000000000000000b4");
        let sender = address!("0x00000000000000000000000000000000000000c4");
        let recipient = address!("0x00000000000000000000000000000000000000d4");
        let amount = U256::from(100u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        // Add then remove from allowlist
        let add_calldata = ITokenDuality::addToAllowListCall { account: operator }.abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("add to allowlist");

        let remove_calldata =
            ITokenDuality::removeFromAllowListCall { account: operator }.abi_encode();
        run_call(
            &mut journal,
            &block_env,
            &precompile,
            admin,
            &remove_calldata,
        )
        .expect("remove from allowlist");

        // Try transfer as removed operator
        let transfer_calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();
        let result = run_call(
            &mut journal,
            &block_env,
            &precompile,
            operator,
            &transfer_calldata,
        );

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(msg, "unauthorized caller", "revoked access should deny")
            }
            other => panic!("expected unauthorized error, got {other:?}"),
        }
    }

    // === Test: Non-Admin Cannot Modify Allowlist ===

    #[test]
    fn non_admin_cannot_modify_allowlist() {
        let admin = address!("0x00000000000000000000000000000000000000a5");
        let unauthorized = address!("0x00000000000000000000000000000000000000f5");
        let target = address!("0x00000000000000000000000000000000000000b5");

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();

        let calldata = ITokenDuality::addToAllowListCall { account: target }.abi_encode();

        let result = run_call(
            &mut journal,
            &block_env,
            &precompile,
            unauthorized,
            &calldata,
        );

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(
                    msg, "unauthorized caller",
                    "non-admin must not modify allowlist"
                )
            }
            other => panic!("expected unauthorized error, got {other:?}"),
        }
    }

    // === Test: Insufficient Balance ===

    #[test]
    fn insufficient_balance_is_rejected() {
        let admin = address!("0x00000000000000000000000000000000000000a6");
        let sender = address!("0x00000000000000000000000000000000000000b6");
        let recipient = address!("0x00000000000000000000000000000000000000c6");
        let amount = U256::from(1000u64);
        let initial_balance = U256::from(500u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, initial_balance);

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(
                    msg, "insufficient balance",
                    "expected insufficient balance error"
                )
            }
            other => panic!("expected insufficient balance error, got {other:?}"),
        }
    }

    // === Test: Zero Address Transfer Rejected ===

    #[test]
    fn transfer_to_zero_address_is_rejected() {
        let admin = address!("0x00000000000000000000000000000000000000a7");
        let sender = address!("0x00000000000000000000000000000000000000b7");
        let amount = U256::from(100u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: Address::ZERO,
            amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert_eq!(
                    msg, "cannot transfer to zero address",
                    "expected zero address error"
                )
            }
            other => panic!("expected zero address error, got {other:?}"),
        }
    }

    // === Test: Per-Call Cap Enforcement ===

    #[test]
    fn per_call_cap_is_enforced() {
        let admin = address!("0x00000000000000000000000000000000000000a8");
        let sender = address!("0x00000000000000000000000000000000000000b8");
        let recipient = address!("0x00000000000000000000000000000000000000c8");
        let cap = U256::from(1000u64);
        let amount = U256::from(1001u64);

        let config = TokenDualityConfig::new(admin, Some(cap), None);
        let precompile = TokenDualityPrecompile::new(config);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(10000u64));

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();

        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert!(
                    msg.contains("per-call cap"),
                    "expected per-call cap error, got: {msg}"
                )
            }
            other => panic!("expected per-call cap error, got {other:?}"),
        }
    }

    // === Test: Per-Block Cap Enforcement ===

    #[test]
    fn per_block_cap_is_enforced() {
        let admin = address!("0x00000000000000000000000000000000000000a9");
        let sender = address!("0x00000000000000000000000000000000000000b9");
        let recipient = address!("0x00000000000000000000000000000000000000c9");
        let block_cap = U256::from(1000u64);
        let amount = U256::from(600u64);

        let config = TokenDualityConfig::new(admin, None, Some(block_cap));
        let precompile = TokenDualityPrecompile::new(config);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(10000u64));

        // First transfer: 600 (within cap)
        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("first transfer should succeed");

        // Second transfer: 600 more (total 1200 > cap)
        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);

        match result {
            Err(PrecompileError::Other(msg)) => {
                assert!(
                    msg.contains("per-block cap"),
                    "expected per-block cap error, got: {msg}"
                )
            }
            other => panic!("expected per-block cap error, got {other:?}"),
        }
    }

    // === Test: Query Functions ===

    #[test]
    fn query_functions_return_correct_values() {
        let admin = address!("0x00000000000000000000000000000000000000aa");
        let per_call = U256::from(5000u64);
        let per_block = U256::from(50000u64);

        let config = TokenDualityConfig::new(admin, Some(per_call), Some(per_block));
        let precompile = TokenDualityPrecompile::new(config);

        let (mut journal, block_env) = setup_context();

        // Query perCallCap
        let calldata = ITokenDuality::perCallCapCall {}.abi_encode();
        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("perCallCap query should succeed");
        let result = U256::abi_decode(&output.bytes).expect("decode result");
        assert_eq!(result, per_call, "perCallCap should match");

        // Query perBlockCap
        let calldata = ITokenDuality::perBlockCapCall {}.abi_encode();
        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("perBlockCap query should succeed");
        let result = U256::abi_decode(&output.bytes).expect("decode result");
        assert_eq!(result, per_block, "perBlockCap should match");
    }

    // === Test: Zero Amount Transfer ===

    #[test]
    fn zero_amount_transfer_succeeds() {
        let admin = address!("0x00000000000000000000000000000000000000ab");
        let sender = address!("0x00000000000000000000000000000000000000bb");
        let recipient = address!("0x00000000000000000000000000000000000000cb");

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount: U256::ZERO,
        }
        .abi_encode();

        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("zero amount transfer should succeed");
        assert_eq!(output.gas_used, 0);
    }

    // === Test: Self-Transfer Optimization ===

    #[test]
    fn self_transfer_is_noop() {
        let admin = address!("0x00000000000000000000000000000000000000ac");
        let account = address!("0x00000000000000000000000000000000000000bc");
        let initial_balance = U256::from(1000u64);
        let amount = U256::from(500u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, account, initial_balance);

        // Transfer to self
        let calldata = ITokenDuality::transferCall {
            from: account,
            to: account,
            amount,
        }
        .abi_encode();

        let output = run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("self-transfer should succeed");
        assert_eq!(output.gas_used, 0);

        // Balance should remain unchanged
        let balance = account_balance(&journal, account).expect("account exists");
        assert_eq!(
            balance, initial_balance,
            "self-transfer should not change balance"
        );
    }

    // === Test: Config Validation ===

    #[test]
    #[should_panic(expected = "token duality admin cannot be zero address")]
    fn config_rejects_zero_admin() {
        TokenDualityConfig::with_admin(Address::ZERO);
    }

    #[test]
    #[should_panic(expected = "per_call_cap")]
    fn config_rejects_invalid_caps() {
        let admin = address!("0x00000000000000000000000000000000000000ad");
        // per_call_cap > per_block_cap should panic
        TokenDualityConfig::new(admin, Some(U256::from(1000)), Some(U256::from(100)));
    }

    #[test]
    fn config_is_production_ready() {
        let admin = address!("0x00000000000000000000000000000000000000ae");
        let config = TokenDualityConfig::with_admin(admin);
        assert!(config.is_production_ready());
    }

    // === Test: Block Boundary Rate Limiting ===

    #[test]
    fn per_block_cap_resets_on_new_block() {
        let admin = address!("0x00000000000000000000000000000000000000b0");
        let sender = address!("0x00000000000000000000000000000000000000c0");
        let recipient = address!("0x00000000000000000000000000000000000000d0");
        let block_cap = U256::from(1000u64);
        let amount = U256::from(600u64);

        let config = TokenDualityConfig::new(admin, None, Some(block_cap));
        let precompile = TokenDualityPrecompile::new(config);

        let (mut journal, mut block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(100000u64));

        // Block 0: First transfer (600)
        block_env.number = U256::from(0);
        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("first transfer should succeed");

        // Block 0: Second transfer would exceed cap (600 + 600 > 1000)
        let result = run_call(&mut journal, &block_env, &precompile, admin, &calldata);
        assert!(
            matches!(result, Err(PrecompileError::Other(msg)) if msg.contains("per-block cap")),
            "second transfer in same block should fail"
        );

        // Block 1: Cap should reset, transfer succeeds
        block_env.number = U256::from(1);
        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("transfer in new block should succeed");

        // Block 1: Can transfer more until cap
        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect_err("should hit cap again in block 1");
    }

    // === Test: Transferred This Block Query ===

    #[test]
    fn transferred_this_block_tracks_correctly() {
        let admin = address!("0x00000000000000000000000000000000000000b1");
        let sender = address!("0x00000000000000000000000000000000000000c1");
        let recipient = address!("0x00000000000000000000000000000000000000d1");
        let amount = U256::from(500u64);

        let config = TokenDualityConfig::new(admin, None, Some(U256::from(10000u64)));
        let precompile = TokenDualityPrecompile::new(config);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(10000u64));

        // Query before any transfers
        let query_calldata = ITokenDuality::transferredThisBlockCall {}.abi_encode();
        let output = run_call(&mut journal, &block_env, &precompile, admin, &query_calldata)
            .expect("query should succeed");
        let initial = U256::abi_decode(&output.bytes).expect("decode result");
        assert_eq!(initial, U256::ZERO, "should start at zero");

        // Make a transfer
        let transfer_calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &transfer_calldata)
            .expect("transfer should succeed");

        // Query after transfer
        let output = run_call(&mut journal, &block_env, &precompile, admin, &query_calldata)
            .expect("query should succeed");
        let after = U256::abi_decode(&output.bytes).expect("decode result");
        assert_eq!(after, amount, "should track transferred amount");
    }

    // === Test: Edge Cases ===

    #[test]
    fn invalid_calldata_returns_error() {
        let admin = address!("0x00000000000000000000000000000000000000b2");
        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();

        // Random invalid calldata
        let invalid_data = b"not_valid_abi_data";
        let result = run_call(&mut journal, &block_env, &precompile, admin, invalid_data);
        assert!(result.is_err(), "invalid calldata should return error");
    }

    #[test]
    fn transfer_exact_balance_succeeds() {
        let admin = address!("0x00000000000000000000000000000000000000b3");
        let sender = address!("0x00000000000000000000000000000000000000c3");
        let recipient = address!("0x00000000000000000000000000000000000000d3");
        let balance = U256::from(1000u64);

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, balance);

        // Transfer exact balance
        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount: balance,
        }
        .abi_encode();

        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("exact balance transfer should succeed");

        let sender_balance = account_balance(&journal, sender).expect("sender exists");
        let recipient_balance = account_balance(&journal, recipient).expect("recipient exists");

        assert_eq!(sender_balance, U256::ZERO, "sender should have zero balance");
        assert_eq!(recipient_balance, balance, "recipient should have full balance");
    }

    #[test]
    fn transfer_one_wei_succeeds() {
        let admin = address!("0x00000000000000000000000000000000000000b4");
        let sender = address!("0x00000000000000000000000000000000000000c4");
        let recipient = address!("0x00000000000000000000000000000000000000d4");

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, U256::from(1000u64));

        // Transfer 1 wei
        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount: U256::from(1),
        }
        .abi_encode();

        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("1 wei transfer should succeed");

        let recipient_balance = account_balance(&journal, recipient).expect("recipient exists");
        assert_eq!(recipient_balance, U256::from(1), "recipient should receive 1 wei");
    }

    #[test]
    fn large_amount_transfer_succeeds() {
        let admin = address!("0x00000000000000000000000000000000000000b5");
        let sender = address!("0x00000000000000000000000000000000000000c5");
        let recipient = address!("0x00000000000000000000000000000000000000d5");
        // 100 million tokens with 18 decimals
        let amount = U256::from(100_000_000u64) * U256::from(10u64).pow(U256::from(18));

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();
        set_balance(&mut journal, sender, amount);

        let calldata = ITokenDuality::transferCall {
            from: sender,
            to: recipient,
            amount,
        }
        .abi_encode();

        run_call(&mut journal, &block_env, &precompile, admin, &calldata)
            .expect("large transfer should succeed");

        let recipient_balance = account_balance(&journal, recipient).expect("recipient exists");
        assert_eq!(recipient_balance, amount, "recipient should receive full amount");
    }

    // === Test: Allowlist Query ===

    #[test]
    fn allowlist_query_returns_correct_status() {
        let admin = address!("0x00000000000000000000000000000000000000b6");
        let operator = address!("0x00000000000000000000000000000000000000c6");

        let precompile = TokenDualityPrecompile::with_admin(admin);

        let (mut journal, block_env) = setup_context();

        // Query before adding
        let query_calldata = ITokenDuality::allowlistCall { account: operator }.abi_encode();
        let output = run_call(&mut journal, &block_env, &precompile, admin, &query_calldata)
            .expect("query should succeed");
        let is_allowed = bool::abi_decode(&output.bytes).expect("decode result");
        assert!(!is_allowed, "should not be allowlisted initially");

        // Add to allowlist
        let add_calldata = ITokenDuality::addToAllowListCall { account: operator }.abi_encode();
        run_call(&mut journal, &block_env, &precompile, admin, &add_calldata)
            .expect("add should succeed");

        // Query after adding
        let output = run_call(&mut journal, &block_env, &precompile, admin, &query_calldata)
            .expect("query should succeed");
        let is_allowed = bool::abi_decode(&output.bytes).expect("decode result");
        assert!(is_allowed, "should be allowlisted after adding");
    }
}
