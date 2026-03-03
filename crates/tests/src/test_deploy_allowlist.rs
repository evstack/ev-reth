use alloy_consensus::TxReceipt;
use alloy_eips::{eip2718::Encodable2718, BlockNumberOrTag};
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::{
    eth::{
        Block, BlockTransactions, Header, Receipt, Transaction, TransactionInput,
        TransactionRequest,
    },
    BlockId,
};
use eyre::Result;
use reth_e2e_test_utils::{
    testsuite::{
        setup::{NetworkSetup, Setup},
        Environment,
    },
    transaction::TransactionTestContext,
    wallet::Wallet,
};
use reth_rpc_api::clients::EthApiClient;

use crate::{
    common::{create_test_chain_spec_with_deploy_allowlist, e2e_test_tree_config, TEST_CHAIN_ID},
    e2e_tests::{build_block_with_transactions, contract_address_from_nonce},
};
use ev_node::{EvolveEngineTypes, EvolveNode};

/// Initcode for a minimal CREATE2 factory contract.
///
/// Runtime behavior: reads salt from calldata[0:32] and child initcode from
/// calldata[32:], then deploys the child using CREATE2 with value=0.
/// Returns the deployed child address as a 32-byte value.
const CREATE2_FACTORY_INITCODE: [u8; 40] = alloy_primitives::hex!(
    "601c600c600039601c6000f3"  // initcode: copies 28-byte runtime to memory and returns it
    "36602090038060206000376000359060006000f560005260206000f3"  // runtime
);

/// Initcode for a minimal child contract deployed via the CREATE2 factory.
///
/// The deployed contract stores 0x42 in memory and returns it (32 bytes) when called.
const CREATE2_CHILD_INITCODE: [u8; 22] = alloy_primitives::hex!(
    "600a600c600039600a6000f3"  // initcode: copies 10-byte runtime to memory and returns it
    "604260005260206000f3"      // runtime: returns 0x42
);

/// Tests that a non-allowlisted account can deploy contracts indirectly through a
/// factory contract using CREATE2, even when the deploy allowlist is active.
///
/// # Test Flow
/// 1. Create a deploy allowlist containing only `allowed_deployer`
/// 2. `non_allowlisted` attempts a direct top-level CREATE — rejected by the allowlist
/// 3. `allowed_deployer` deploys a CREATE2 factory contract (top-level CREATE, allowed)
/// 4. `non_allowlisted` account calls the factory, which internally uses CREATE2
/// 5. Verify the child contract is deployed at the expected deterministic address
///
/// # What It Tests
/// - deploy allowlist blocks top-level CREATE from non-allowlisted accounts
/// - deploy allowlist permits top-level CREATE from allowlisted accounts
/// - contract-to-contract CREATE2 bypasses the allowlist (by design)
/// - the factory pattern works as an indirect deployment mechanism
///
/// # Success Criteria
/// - non-allowlisted top-level CREATE is excluded from the block
/// - factory deploys successfully from allowlisted account
/// - non-allowlisted account's call to the factory succeeds
/// - child contract appears at the predicted CREATE2 address
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_deploy_allowlist_permits_create2_via_factory() -> Result<()> {
    reth_tracing::init_test_tracing();

    let mut wallets = Wallet::new(2).with_chain_id(TEST_CHAIN_ID).wallet_gen();
    let allowed_deployer = wallets.remove(0);
    let non_allowlisted = wallets.remove(0);

    let chain_spec = create_test_chain_spec_with_deploy_allowlist(vec![allowed_deployer.address()]);
    let chain_id = chain_spec.chain().id();

    let mut setup = Setup::<EvolveEngineTypes>::default()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true)
        .with_tree_config(e2e_test_tree_config());

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("parent block should exist");
    let mut parent_hash = parent_block.header.hash;
    let mut parent_timestamp = parent_block.header.inner.timestamp;
    let mut parent_number = parent_block.header.inner.number;
    let gas_limit = parent_block.header.inner.gas_limit;

    // non-allowlisted account attempts a direct top-level CREATE — should be rejected
    let denied_deploy_tx = TransactionRequest {
        nonce: Some(0),
        gas: Some(1_000_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        value: Some(U256::ZERO),
        to: Some(TxKind::Create),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from_static(&CREATE2_FACTORY_INITCODE)),
        },
        ..Default::default()
    };

    let denied_envelope =
        TransactionTestContext::sign_tx(non_allowlisted.clone(), denied_deploy_tx).await;
    let denied_raw: Bytes = denied_envelope.encoded_2718().into();

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        Some(gas_limit),
        vec![denied_raw],
        Address::ZERO,
    )
    .await?;

    let latest_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("latest block available after denied deploy");
    let denied_tx_count = match latest_block.transactions {
        BlockTransactions::Full(ref txs) => txs.len(),
        BlockTransactions::Hashes(ref hashes) => hashes.len(),
        BlockTransactions::Uncle => 0,
    };
    assert_eq!(
        denied_tx_count, 0,
        "non-allowlisted top-level CREATE should be excluded from the block"
    );

    let denied_address = contract_address_from_nonce(non_allowlisted.address(), 0);
    let denied_code =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header, Bytes>::get_code(
            &env.node_clients[0].rpc,
            denied_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert!(
        denied_code.is_empty(),
        "non-allowlisted deploy should not create contract code"
    );

    // allowlisted account deploys the factory via top-level CREATE
    let deploy_factory_tx = TransactionRequest {
        nonce: Some(0),
        gas: Some(1_000_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        value: Some(U256::ZERO),
        to: Some(TxKind::Create),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from_static(&CREATE2_FACTORY_INITCODE)),
        },
        ..Default::default()
    };

    let factory_envelope =
        TransactionTestContext::sign_tx(allowed_deployer.clone(), deploy_factory_tx).await;
    let factory_raw: Bytes = factory_envelope.encoded_2718().into();

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        Some(gas_limit),
        vec![factory_raw],
        Address::ZERO,
    )
    .await?;

    let factory_address = contract_address_from_nonce(allowed_deployer.address(), 0);
    let factory_code =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header, Bytes>::get_code(
            &env.node_clients[0].rpc,
            factory_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert!(
        !factory_code.is_empty(),
        "factory contract should be deployed by allowlisted account"
    );

    // non-allowlisted account calls the factory to deploy a child via CREATE2
    let salt = B256::left_padding_from(&[42u8]);
    let mut calldata = Vec::new();
    calldata.extend_from_slice(salt.as_slice());
    calldata.extend_from_slice(&CREATE2_CHILD_INITCODE);

    let deploy_child_tx = TransactionRequest {
        nonce: Some(0),
        gas: Some(1_000_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        value: Some(U256::ZERO),
        to: Some(TxKind::Call(factory_address)),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from(calldata)),
        },
        ..Default::default()
    };

    let child_envelope =
        TransactionTestContext::sign_tx(non_allowlisted.clone(), deploy_child_tx).await;
    let child_raw: Bytes = child_envelope.encoded_2718().into();
    let child_tx_hash = *child_envelope.tx_hash();

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        Some(gas_limit),
        vec![child_raw],
        Address::ZERO,
    )
    .await?;

    // verify the transaction was included and succeeded
    let child_receipt = EthApiClient::<
        TransactionRequest,
        Transaction,
        Block,
        Receipt,
        Header,
        Bytes,
    >::transaction_receipt(&env.node_clients[0].rpc, child_tx_hash)
    .await?
    .expect("factory call receipt should be available");
    assert!(
        child_receipt.status(),
        "non-allowlisted account calling factory should succeed"
    );

    // verify child contract exists at the expected CREATE2 address
    let init_code_hash = alloy_primitives::keccak256(&CREATE2_CHILD_INITCODE);
    let expected_child_address = factory_address.create2(salt, init_code_hash);
    let child_code =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header, Bytes>::get_code(
            &env.node_clients[0].rpc,
            expected_child_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert!(
        !child_code.is_empty(),
        "child contract should be deployed via CREATE2 despite caller not being on the allowlist"
    );

    drop(setup);

    Ok(())
}
