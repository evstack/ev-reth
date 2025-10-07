//! Helpers for redirecting the EIP-1559 base fee to a configured sink account.

use alloy_primitives::{Address, U256};
use reth_revm::revm::{
    context_interface::{journaled_state::JournalTr, Block, ContextTr},
    database_interface::Database,
};
use thiserror::Error;

/// Encapsulates the policy of crediting EIP-1559 base fees to a specific address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseFeeRedirect {
    fee_sink: Address,
}

impl BaseFeeRedirect {
    /// Creates a new redirect policy for the provided sink address.
    pub const fn new(fee_sink: Address) -> Self {
        Self { fee_sink }
    }

    /// Returns the configured sink address.
    pub const fn fee_sink(&self) -> Address {
        self.fee_sink
    }

    /// Credits the sink address with the base-fee portion of the transaction cost.
    ///
    /// Returns the amount that was credited (in wei).
    pub fn apply<CTX>(
        &self,
        ctx: &mut CTX,
        gas_used: u64,
    ) -> Result<U256, BaseFeeRedirectError<<CTX::Db as Database>::Error>>
    where
        CTX: ContextTr,
        CTX::Journal: JournalTr<Database = CTX::Db>,
        CTX::Db: Database,
        <CTX::Db as Database>::Error: std::error::Error,
    {
        let base_fee = ctx.block().basefee();
        if gas_used == 0 || base_fee == 0 {
            return Ok(U256::ZERO);
        }

        let amount = U256::from(base_fee) * U256::from(gas_used);
        if amount.is_zero() {
            return Ok(amount);
        }

        let journal = ctx.journal_mut();
        journal
            .load_account(self.fee_sink)
            .map_err(BaseFeeRedirectError::Database)?;
        journal
            .balance_incr(self.fee_sink, amount)
            .map_err(BaseFeeRedirectError::Database)?;
        Ok(amount)
    }
}

impl From<Address> for BaseFeeRedirect {
    fn from(value: Address) -> Self {
        Self::new(value)
    }
}

/// Errors that can occur when crediting the base-fee sink account.
#[derive(Debug, Error)]
pub enum BaseFeeRedirectError<DbError> {
    /// Underlying database error propagated from the journal/state.
    #[error("failed to update fee sink account: {0}")]
    Database(#[from] DbError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::address;
    use reth_revm::{
        revm::{context::Context, database::EmptyDB},
        MainContext,
    };

    #[test]
    fn credits_sink_balance() {
        let fee_sink = address!("0x00000000000000000000000000000000000000fe");
        let mut ctx = Context::mainnet().with_db(EmptyDB::default());
        ctx.block.basefee = 100;

        let redirect = BaseFeeRedirect::new(fee_sink);
        let amount = redirect.apply(&mut ctx, 50_000).expect("credit succeeds");
        assert_eq!(amount, U256::from(5_000_000));

        let account = ctx.journal().account(fee_sink);
        assert_eq!(account.info.balance, amount);
    }

    #[test]
    fn skips_when_no_basefee_or_gas() {
        let fee_sink = address!("0x00000000000000000000000000000000000000ef");
        let mut ctx = Context::mainnet().with_db(EmptyDB::default());
        ctx.block.basefee = 0;

        let redirect = BaseFeeRedirect::new(fee_sink);
        let amount = redirect.apply(&mut ctx, 42_000).expect("credit succeeds");
        assert!(amount.is_zero());

        ctx.block.basefee = 100;
        let amount = redirect.apply(&mut ctx, 0).expect("credit succeeds");
        assert!(amount.is_zero());
    }
}
