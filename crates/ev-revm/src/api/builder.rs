//! Convenience helpers for building an EV-configured EVM from an execution context.

use crate::{
    base_fee::BaseFeeRedirect,
    evm::{DefaultEvEvm, EvEvm},
};
use reth_revm::revm::handler::MainBuilder;

/// Extension trait mirroring `MainBuilder` but returning [`EvEvm`].
pub trait EvBuilder: MainBuilder {
    /// Builds an EVM instance without an inspector.
    fn build_ev(
        self,
        redirect: Option<BaseFeeRedirect>,
    ) -> DefaultEvEvm<<Self as MainBuilder>::Context>;

    /// Builds an EVM instance with a custom inspector.
    fn build_ev_with_inspector<INSP>(
        self,
        inspector: INSP,
        redirect: Option<BaseFeeRedirect>,
    ) -> EvEvm<<Self as MainBuilder>::Context, INSP>;
}

impl<T> EvBuilder for T
where
    T: MainBuilder,
{
    fn build_ev(
        self,
        redirect: Option<BaseFeeRedirect>,
    ) -> DefaultEvEvm<<Self as MainBuilder>::Context> {
        EvEvm::from_inner(self.build_mainnet(), redirect, false)
    }

    fn build_ev_with_inspector<INSP>(
        self,
        inspector: INSP,
        redirect: Option<BaseFeeRedirect>,
    ) -> EvEvm<<Self as MainBuilder>::Context, INSP> {
        EvEvm::from_inner(self.build_mainnet_with_inspector(inspector), redirect, true)
    }
}
