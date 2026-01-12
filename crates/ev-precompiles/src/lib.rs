//! # Evolve Custom EVM Precompiles
//!
//! This crate provides precompiled contracts that extend the EVM with
//! Evolve-specific functionality for sovereign rollups.
//!
//! ## Available Precompiles
//!
//! | Address | Name | Description |
//! |---------|------|-------------|
//! | `0xF100` | [`mint`] | Native token supply management (mint/burn) |
//! | `0x00FD` | [`token_duality`] | Native token as ERC-20 (Celo-style transfer) |
//!
//! ## Architecture
//!
//! All precompiles follow the same pattern:
//!
//! 1. **Authorization**: Admin-based access control with runtime allowlist
//! 2. **State Management**: Direct balance manipulation via `EvmInternals`
//! 3. **Safety**: Checked arithmetic, zero-address validation, rate limiting
//! 4. **Consistency**: Follows Reth/Revm precompile conventions
//!
//! ## Integration
//!
//! Precompiles are registered via `ev_revm::factory::EvEvmFactory` which
//! wraps the standard `EthEvmFactory` and injects custom precompiles.
//!
//! ```ignore
//! use ev_revm::factory::EvEvmFactory;
//! use ev_precompiles::token_duality::TokenDualityConfig;
//!
//! let factory = EvEvmFactory::with_token_duality(
//!     EthEvmFactory::default(),
//!     None, // base fee redirect
//!     Some(mint_admin),
//!     Some(TokenDualityConfig::with_admin(token_admin)),
//! );
//! ```
//!
//! ## Security Considerations
//!
//! - All precompiles require explicit admin configuration
//! - Rate limiting prevents abuse (per-call and per-block caps)
//! - State changes are atomic (all-or-nothing)
//! - No reentrancy risk (native execution)
//!
//! ## References
//!
//! - [Celo Token Duality](https://specs.celo.org/token_duality.html)
//! - [Reth Precompiles](https://reth.rs)
//! - [Revm Documentation](https://bluealloy.github.io/revm/)

pub mod mint;
pub mod token_duality;
