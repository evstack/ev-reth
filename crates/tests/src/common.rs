//! Common test utilities and fixtures for evolve tests.
//!
//! This module provides shared test setup, fixtures, and helper functions
//! to eliminate code duplication across different test files.

use std::sync::Arc;

use alloy_consensus::{transaction::SignerRecoverable, TxLegacy, TypedTransaction};
use alloy_genesis::Genesis;
use alloy_primitives::{Address, Bytes, ChainId, Signature, TxKind, B256, U256};
use ev_primitives::{EvTxEnvelope, TransactionSigned};
use ev_revm::{
    BaseFeeRedirect, BaseFeeRedirectSettings, ContractSizeLimitSettings, EvTxEvmFactory,
    MintPrecompileSettings,
};
use eyre::Result;
use reth_chainspec::{ChainSpec, ChainSpecBuilder};
use reth_primitives::{Header, Transaction};
use reth_provider::test_utils::{ExtendedAccount, MockEthProvider};
use serde_json::json;
use tempfile::TempDir;

use ev_node::{EvolveEvmConfig, EvolvePayloadBuilder, EvolvePayloadBuilderConfig};
use evolve_ev_reth::EvolvePayloadAttributes;

// Test constants
/// Test chain ID used in tests
pub const TEST_CHAIN_ID: u64 = 1234;
/// Genesis block hash for test setup
pub const GENESIS_HASH: &str = "0x2b8bbb1ea1e04f9c9809b4b278a8687806edc061a356c7dbc491930d8e922503";
/// Genesis state root for test setup
pub const GENESIS_STATEROOT: &str =
    "0x05e9954443da80d86f2104e56ffdfd98fe21988730684360104865b3dc8191b4";
/// Test address for transactions
pub const TEST_TO_ADDRESS: &str = "0x944fDcD1c868E3cC566C78023CcB38A32cDA836E";
/// Test timestamp for blocks
pub const TEST_TIMESTAMP: u64 = 1710338135;
/// Test gas limit for blocks
pub const TEST_GAS_LIMIT: u64 = 30_000_000;
/// Base fee used in mock headers to satisfy post-London/EIP-4844 requirements
pub const TEST_BASE_FEE: u64 = 0;

fn to_ev_envelope(transaction: Transaction, signature: Signature) -> TransactionSigned {
    let signed = alloy_consensus::Signed::new_unhashed(transaction, signature);
    EvTxEnvelope::Ethereum(reth_ethereum_primitives::TransactionSigned::from(signed))
}

/// Creates a reusable chain specification for tests.
pub fn create_test_chain_spec() -> Arc<ChainSpec> {
    create_test_chain_spec_with_extras(None, None)
}

/// Creates a reusable chain specification with an optional base fee sink address.
pub fn create_test_chain_spec_with_base_fee_sink(base_fee_sink: Option<Address>) -> Arc<ChainSpec> {
    create_test_chain_spec_with_extras(base_fee_sink, None)
}

/// Creates a reusable chain specification with a configured mint admin address.
pub fn create_test_chain_spec_with_mint_admin(mint_admin: Address) -> Arc<ChainSpec> {
    create_test_chain_spec_with_extras(None, Some(mint_admin))
}

fn create_test_chain_spec_with_extras(
    base_fee_sink: Option<Address>,
    mint_admin: Option<Address>,
) -> Arc<ChainSpec> {
    let mut genesis: Genesis =
        serde_json::from_str(include_str!("../assets/genesis.json")).expect("valid genesis");

    if base_fee_sink.is_some() || mint_admin.is_some() {
        let mut extras = serde_json::Map::new();
        if let Some(sink) = base_fee_sink {
            extras.insert("baseFeeSink".to_string(), json!(sink));
        }
        if let Some(admin) = mint_admin {
            extras.insert("mintAdmin".to_string(), json!(admin));
        }
        genesis
            .config
            .extra_fields
            .insert("evolve".to_string(), serde_json::Value::Object(extras));
    }

    Arc::new(
        ChainSpecBuilder::default()
            .chain(reth_chainspec::Chain::from_id(TEST_CHAIN_ID))
            .genesis(genesis)
            .cancun_activated()
            .build(),
    )
}

/// Shared test fixture for evolve payload builder tests
#[derive(Debug)]
pub struct EvolveTestFixture {
    /// The evolve payload builder instance
    pub builder: EvolvePayloadBuilder<MockEthProvider>,
    /// Mock Ethereum provider for testing
    pub provider: MockEthProvider,
    /// Genesis block hash
    pub genesis_hash: B256,
    /// Genesis state root
    pub genesis_state_root: B256,
    /// Temporary directory for test data
    #[allow(dead_code)]
    pub temp_dir: TempDir,
}

impl EvolveTestFixture {
    /// Creates a new test fixture with mock provider and genesis state
    pub async fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let provider = MockEthProvider::default();

        let genesis_hash = B256::from_slice(&hex::decode(&GENESIS_HASH[2..]).unwrap());
        let genesis_state_root = B256::from_slice(&hex::decode(&GENESIS_STATEROOT[2..]).unwrap());

        // Setup genesis header with all required fields for modern Ethereum
        let genesis_header = Header {
            state_root: genesis_state_root,
            number: 0,
            gas_limit: TEST_GAS_LIMIT,
            timestamp: TEST_TIMESTAMP,
            base_fee_per_gas: Some(TEST_BASE_FEE),
            excess_blob_gas: Some(0),
            blob_gas_used: Some(0),
            parent_beacon_block_root: Some(B256::ZERO),
            ..Default::default()
        };

        provider.add_header(genesis_hash, genesis_header);

        // Create a test chain spec with our test chain ID
        let test_chainspec = create_test_chain_spec();
        let config = EvolvePayloadBuilderConfig::from_chain_spec(test_chainspec.as_ref()).unwrap();
        config.validate().unwrap();

        let base_fee_redirect = config
            .base_fee_redirect_settings()
            .map(|(sink, activation)| {
                BaseFeeRedirectSettings::new(BaseFeeRedirect::new(sink), activation)
            });
        let mint_precompile = config
            .mint_precompile_settings()
            .map(|(admin, activation)| MintPrecompileSettings::new(admin, activation));
        let contract_size_limit = config
            .contract_size_limit_settings()
            .map(|(limit, activation)| ContractSizeLimitSettings::new(limit, activation));
        let evm_factory =
            EvTxEvmFactory::new(base_fee_redirect, mint_precompile, contract_size_limit);
        let wrapped_evm = EvolveEvmConfig::new_with_evm_factory(test_chainspec, evm_factory);

        let builder = EvolvePayloadBuilder::new(Arc::new(provider.clone()), wrapped_evm, config);

        let fixture = Self {
            builder,
            provider,
            genesis_hash,
            genesis_state_root,
            temp_dir,
        };

        fixture.setup_test_accounts();
        Ok(fixture)
    }

    /// Setup test accounts with sufficient balances
    pub fn setup_test_accounts(&self) {
        let account = ExtendedAccount::new(
            0,
            U256::from(1000_u64) * U256::from(1_000_000_000_000_000_000u64),
        );

        // Find which address the test signature resolves to
        let test_signed = to_ev_envelope(
            Transaction::Legacy(TxLegacy {
                chain_id: Some(ChainId::from(TEST_CHAIN_ID)),
                nonce: 0,
                gas_price: 0,
                gas_limit: 21_000,
                to: TxKind::Call(Address::ZERO),
                value: U256::ZERO,
                input: Bytes::default(),
            }),
            Signature::test_signature(),
        );

        if let Ok(recovered) = test_signed.recover_signer() {
            self.provider.add_account(recovered, account);
        }
    }

    /// Adds a mock header to the provider for proper parent lookups
    pub fn add_mock_header(&self, hash: B256, number: u64, state_root: B256, timestamp: u64) {
        let header = Header {
            number,
            state_root,
            gas_limit: TEST_GAS_LIMIT,
            timestamp,
            base_fee_per_gas: Some(TEST_BASE_FEE),
            excess_blob_gas: Some(0),
            blob_gas_used: Some(0),
            parent_beacon_block_root: Some(B256::ZERO),
            ..Default::default()
        };

        self.provider.add_header(hash, header);
    }

    /// Creates payload attributes for testing
    pub fn create_payload_attributes(
        &self,
        transactions: Vec<TransactionSigned>,
        block_number: u64,
        timestamp: u64,
        parent_hash: B256,
        gas_limit: Option<u64>,
    ) -> EvolvePayloadAttributes {
        EvolvePayloadAttributes::new(
            transactions,
            gas_limit,
            timestamp,
            B256::random(),    // prev_randao
            Address::random(), // suggested_fee_recipient
            parent_hash,
            block_number,
        )
    }
}

/// Creates test transactions with specified count and starting nonce
pub fn create_test_transactions(count: usize, nonce_start: u64) -> Vec<TransactionSigned> {
    let mut transactions = Vec::with_capacity(count);
    let to_address = Address::from_slice(&hex::decode(&TEST_TO_ADDRESS[2..]).unwrap());

    for i in 0..count {
        let nonce = nonce_start + i as u64;

        let legacy_tx = TxLegacy {
            chain_id: Some(ChainId::from(TEST_CHAIN_ID)),
            nonce,
            gas_price: 0, // Zero gas price for testing
            gas_limit: 21_000,
            to: TxKind::Call(to_address),
            value: U256::from(0), // No value transfer
            input: Bytes::default(),
        };

        let typed_tx = TypedTransaction::Legacy(legacy_tx);
        let transaction = Transaction::from(typed_tx);
        let signed_tx = to_ev_envelope(transaction, Signature::test_signature());
        transactions.push(signed_tx);
    }

    transactions
}

/// Creates a single test transaction with specified nonce
pub fn create_test_transaction(nonce: u64) -> TransactionSigned {
    create_test_transactions(1, nonce)
        .into_iter()
        .next()
        .unwrap()
}
