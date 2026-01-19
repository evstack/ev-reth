//! Base-fee redirect extensions for `revm`.

pub mod api;
pub mod base_fee;
pub mod config;
pub mod evm;
pub mod factory;
pub mod handler;
/// EV-specific transaction environment extensions.
pub mod tx_env;

pub use api::EvBuilder;
pub use base_fee::{BaseFeeRedirect, BaseFeeRedirectError};
pub use config::{BaseFeeConfig, ConfigError};
pub use evm::{DefaultEvEvm, EvEvm};
pub use factory::{
    with_ev_handler, BaseFeeRedirectSettings, ContractSizeLimitSettings, EvEvmFactory, EvTxEvmFactory,
    MintPrecompileSettings,
};
pub use handler::EvHandler;
pub use tx_env::EvTxEnv;
