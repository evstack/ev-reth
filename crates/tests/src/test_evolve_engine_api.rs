//! Engine API end-to-end tests for Evolve.
//!
//! These tests exercise the Engine API through the shared e2e harness,
//! ensuring that forkchoice updates, payload construction, and finalization
//! happen against a live ev-reth node instead of mock fixtures.

use crate::{
    common::{
        create_test_chain_spec, create_test_chain_spec_with_osaka_amsterdam, e2e_test_tree_config,
    },
    e2e_tests::build_block_with_transactions,
};

use alloy_consensus::{TxEnvelope, TxReceipt};
use alloy_eips::{eip2718::Encodable2718, eip7685::RequestsOrHash};
use alloy_network::eip2718::Decodable2718;
use alloy_primitives::{Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::{
    eth::{Block, BlockTransactions, Header, Receipt, Transaction, TransactionRequest},
    BlockNumberOrTag,
};
use alloy_rpc_types_engine::{ForkchoiceState, PayloadAttributes, PayloadStatusEnum};
use alloy_signer_local::PrivateKeySigner;
use ev_node::{EvolveEnginePayloadAttributes, EvolveEngineTypes, EvolveNode};
use eyre::Result;
use reth_e2e_test_utils::{
    testsuite::{
        setup::{NetworkSetup, Setup},
        BlockInfo, Environment,
    },
    transaction::TransactionTestContext,
    wallet::Wallet,
};
use reth_rpc_api::clients::{EngineApiClient, EthApiClient};

async fn make_transfer_batch(
    chain_id: u64,
    sender: &PrivateKeySigner,
    nonce_start: u64,
    count: usize,
) -> Vec<Bytes> {
    let mut batch = Vec::with_capacity(count);
    for i in 0..count {
        let tx = TransactionRequest {
            nonce: Some(nonce_start + i as u64),
            gas: Some(21_000),
            max_fee_per_gas: Some(20_000_000_000),
            max_priority_fee_per_gas: Some(2_000_000_000),
            chain_id: Some(chain_id),
            to: Some(TxKind::Call(Address::random())),
            value: Some(U256::from(100u64)),
            ..Default::default()
        };
        let envelope = TransactionTestContext::sign_tx(sender.clone(), tx).await;
        batch.push(envelope.encoded_2718().into());
    }
    batch
}

#[derive(Clone, Copy, Debug)]
enum ScheduledEngineFork {
    Prague,
    Osaka,
}

struct ScheduledBlockInfo {
    hash: B256,
    number: u64,
    timestamp: u64,
}

async fn build_empty_block_for_scheduled_fork(
    env: &mut Environment<EvolveEngineTypes>,
    parent_hash: B256,
    timestamp: u64,
    gas_limit: u64,
    suggested_fee_recipient: Address,
    fork: ScheduledEngineFork,
) -> Result<ScheduledBlockInfo> {
    let payload_attributes = EvolveEnginePayloadAttributes {
        inner: PayloadAttributes {
            timestamp,
            prev_randao: B256::random(),
            suggested_fee_recipient,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
            slot_number: None,
        },
        transactions: Some(Vec::new()),
        gas_limit: Some(gas_limit),
    };

    let fork_choice = ForkchoiceState {
        head_block_hash: parent_hash,
        safe_block_hash: parent_hash,
        finalized_block_hash: parent_hash,
    };
    let engine_client = env.node_clients[0].engine.http_client();
    let fcu_response = EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        fork_choice,
        Some(payload_attributes),
    )
    .await?;
    let payload_id = fcu_response.payload_id.expect("payload id returned");

    let (new_payload_status, block_info) = match fork {
        ScheduledEngineFork::Prague => {
            let payload_envelope =
                EngineApiClient::<EvolveEngineTypes>::get_payload_v4(&engine_client, payload_id)
                    .await?;
            let execution_payload = payload_envelope.envelope_inner.execution_payload.clone();
            let new_payload_status = EngineApiClient::<EvolveEngineTypes>::new_payload_v4(
                &engine_client,
                execution_payload.clone(),
                vec![],
                B256::ZERO,
                RequestsOrHash::default(),
            )
            .await?;
            let payload_inner = execution_payload.payload_inner.payload_inner;
            assert!(
                payload_inner.transactions.is_empty(),
                "Prague payload should be empty"
            );

            (
                new_payload_status,
                ScheduledBlockInfo {
                    hash: payload_inner.block_hash,
                    number: payload_inner.block_number,
                    timestamp: payload_inner.timestamp,
                },
            )
        }
        ScheduledEngineFork::Osaka => {
            let payload_envelope =
                EngineApiClient::<EvolveEngineTypes>::get_payload_v5(&engine_client, payload_id)
                    .await?;
            let execution_payload = payload_envelope.execution_payload.clone();
            let new_payload_status = EngineApiClient::<EvolveEngineTypes>::new_payload_v4(
                &engine_client,
                execution_payload.clone(),
                vec![],
                B256::ZERO,
                RequestsOrHash::default(),
            )
            .await?;
            let payload_inner = execution_payload.payload_inner.payload_inner;
            assert!(
                payload_inner.transactions.is_empty(),
                "Osaka payload should be empty"
            );

            (
                new_payload_status,
                ScheduledBlockInfo {
                    hash: payload_inner.block_hash,
                    number: payload_inner.block_number,
                    timestamp: payload_inner.timestamp,
                },
            )
        }
    };

    assert!(
        matches!(new_payload_status.status, PayloadStatusEnum::Valid),
        "expected {fork:?} payload to be valid, got {:?}",
        new_payload_status.status
    );

    let canonical_fork_choice = ForkchoiceState {
        head_block_hash: block_info.hash,
        safe_block_hash: block_info.hash,
        finalized_block_hash: block_info.hash,
    };
    EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        canonical_fork_choice,
        None,
    )
    .await?;

    env.set_current_block_info(BlockInfo {
        hash: block_info.hash,
        number: block_info.number,
        timestamp: block_info.timestamp,
    })?;
    env.active_node_state_mut()?.latest_header_time = block_info.timestamp;

    Ok(block_info)
}

async fn build_empty_block_with_pre_amsterdam_ev_node_flow(
    env: &Environment<EvolveEngineTypes>,
    parent_hash: B256,
    timestamp: u64,
    gas_limit: u64,
    suggested_fee_recipient: Address,
) -> Result<ScheduledBlockInfo> {
    let payload_attributes = EvolveEnginePayloadAttributes {
        inner: PayloadAttributes {
            timestamp,
            prev_randao: B256::random(),
            suggested_fee_recipient,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
            slot_number: None,
        },
        transactions: Some(Vec::new()),
        gas_limit: Some(gas_limit),
    };

    let fork_choice = ForkchoiceState {
        head_block_hash: parent_hash,
        safe_block_hash: parent_hash,
        finalized_block_hash: parent_hash,
    };
    let engine_client = env.node_clients[0].engine.http_client();
    let fcu_response = EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        fork_choice,
        Some(payload_attributes),
    )
    .await?;
    let payload_id = fcu_response.payload_id.expect("payload id returned");

    let payload_envelope =
        EngineApiClient::<EvolveEngineTypes>::get_payload_v5(&engine_client, payload_id).await?;
    let execution_payload = payload_envelope.execution_payload.clone();
    let new_payload_status = EngineApiClient::<EvolveEngineTypes>::new_payload_v4(
        &engine_client,
        execution_payload.clone(),
        vec![],
        B256::ZERO,
        RequestsOrHash::default(),
    )
    .await?;
    assert!(
        matches!(new_payload_status.status, PayloadStatusEnum::Valid),
        "expected pre-Amsterdam ev-node payload to be valid, got {:?}",
        new_payload_status.status
    );

    let payload_inner = execution_payload.payload_inner.payload_inner;
    Ok(ScheduledBlockInfo {
        hash: payload_inner.block_hash,
        number: payload_inner.block_number,
        timestamp: payload_inner.timestamp,
    })
}

/// Verifies Engine API payload versioning up to Osaka, then rejects current ev-node behavior at
/// Amsterdam.
///
/// This intentionally models ev-node before the Amsterdam update: it still sends V3 forkchoice
/// payload attributes without `slotNumber` and continues the Osaka-era payload flow at the
/// Amsterdam timestamp. ev-reth should reject that leg until ev-node switches to
/// `forkchoiceUpdatedV4`, `getPayloadV6`, `newPayloadV5`, and supplies `slotNumber`.
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_engine_api_cycles_before_osaka_to_amsterdam_rejects_pre_amsterdam_ev_node_flow(
) -> Result<()> {
    reth_tracing::init_test_tracing();

    const SECONDS_PER_SLOT: u64 = 12;
    const OSAKA_TIMESTAMP: u64 = 24;
    const AMSTERDAM_TIMESTAMP: u64 = 36;

    let chain_spec =
        create_test_chain_spec_with_osaka_amsterdam(OSAKA_TIMESTAMP, AMSTERDAM_TIMESTAMP);

    let mut setup = Setup::<EvolveEngineTypes>::default()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(false)
        .with_tree_config(e2e_test_tree_config());

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("genesis block should exist");

    let mut parent_hash = parent_block.header.hash;
    let mut expected_number = parent_block.header.inner.number;
    let gas_limit = parent_block.header.inner.gas_limit;
    let fee_recipient = Address::repeat_byte(0xAB);

    let fork_steps = [
        (
            ScheduledEngineFork::Prague,
            OSAKA_TIMESTAMP - SECONDS_PER_SLOT,
        ),
        (ScheduledEngineFork::Osaka, OSAKA_TIMESTAMP),
    ];

    for (fork, timestamp) in fork_steps {
        let built_block = build_empty_block_for_scheduled_fork(
            &mut env,
            parent_hash,
            timestamp,
            gas_limit,
            fee_recipient,
            fork,
        )
        .await?;

        expected_number += 1;
        assert_eq!(
            built_block.number, expected_number,
            "{fork:?} block number should advance by one"
        );
        assert_eq!(
            built_block.timestamp, timestamp,
            "{fork:?} block timestamp should match the scheduled fork step"
        );

        let rpc_block = env.node_clients[0]
            .get_block_by_number(BlockNumberOrTag::Number(built_block.number))
            .await?
            .expect("new block should be retrievable via RPC");
        assert_eq!(
            rpc_block.header.hash, built_block.hash,
            "{fork:?} RPC block hash should match the Engine API payload"
        );
        assert_eq!(
            rpc_block.header.inner.timestamp, timestamp,
            "{fork:?} RPC block timestamp should match the Engine API payload"
        );

        parent_hash = built_block.hash;
    }

    let amsterdam_result = build_empty_block_with_pre_amsterdam_ev_node_flow(
        &env,
        parent_hash,
        AMSTERDAM_TIMESTAMP,
        gas_limit,
        fee_recipient,
    )
    .await;
    let Err(error) = amsterdam_result else {
        panic!("pre-Amsterdam ev-node Engine API flow should fail once Amsterdam activates");
    };
    let error_text = format!("{error:?}");
    assert!(
        error_text.contains("Invalid payload attributes")
            || error_text.contains("slot")
            || error_text.contains("Amsterdam")
            || error_text.contains("Unsupported fork"),
        "unexpected Amsterdam rejection error: {error_text}"
    );

    let current_block_info = env
        .current_block_info()
        .expect("environment should track latest block info");
    assert_eq!(
        current_block_info.number, expected_number,
        "environment should remain at the last accepted Osaka block"
    );
    assert_eq!(
        current_block_info.hash, parent_hash,
        "environment hash should match the last accepted Osaka head"
    );

    drop(setup);

    Ok(())
}

/// Verifies that a forkchoice update including custom transactions succeeds against a live node.
///
/// The test builds two blocks:
/// 1. An empty block to advance the chain head from genesis.
/// 2. A block containing a signed transfer produced via the Engine API helpers.
///
/// It then asserts that:
/// - The execution payload returns the submitted transaction bytes.
/// - The block is persisted via the ETH RPC and consumes gas.
/// - The transaction receipt reports success and the environment tracks the new head.
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_engine_api_fork_choice_with_transactions() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_spec = create_test_chain_spec();
    let chain_id = chain_spec.chain().id();

    let mut setup = Setup::<EvolveEngineTypes>::default()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(false)
        .with_tree_config(e2e_test_tree_config());

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("parent block should exist");

    let mut parent_hash = parent_block.header.hash;
    let mut parent_number = parent_block.header.inner.number;
    let mut parent_timestamp = parent_block.header.inner.timestamp;
    let gas_limit = parent_block.header.inner.gas_limit;
    let fee_recipient = Address::repeat_byte(0xAB);

    let mut wallets = Wallet::new(1).with_chain_id(chain_id).wallet_gen();
    let sender_wallet = wallets.remove(0);
    let mut next_nonce = 0u64;

    let empty_payload = build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        Some(gas_limit),
        Vec::<Bytes>::new(),
        fee_recipient,
    )
    .await?;
    let empty_execution_payload = empty_payload.execution_payload.clone();
    assert!(
        empty_execution_payload
            .payload_inner
            .payload_inner
            .transactions
            .is_empty(),
        "empty payload should not contain transactions"
    );

    let mut last_block_hash: B256 = empty_execution_payload
        .payload_inner
        .payload_inner
        .block_hash;

    let scenarios = [
        (1usize, "single transfer"),
        (4usize, "small batch"),
        (20usize, "large batch"),
    ];

    for (count, label) in scenarios {
        let batch = make_transfer_batch(chain_id, &sender_wallet, next_nonce, count).await;
        next_nonce += count as u64;
        let payload_envelope = build_block_with_transactions(
            &mut env,
            &mut parent_hash,
            &mut parent_number,
            &mut parent_timestamp,
            Some(gas_limit),
            batch.clone(),
            fee_recipient,
        )
        .await?;

        let execution_payload = payload_envelope.execution_payload.clone();
        let payload_inner = execution_payload.payload_inner.payload_inner;
        assert_eq!(
            payload_inner.transactions.len(),
            count,
            "{label} payload should contain expected number of transactions"
        );
        assert_eq!(
            payload_inner.transactions, batch,
            "{label} payload bytes should match submission order"
        );

        let produced_block_number = payload_inner.block_number;
        assert_eq!(
            produced_block_number, parent_number,
            "parent tracking should advance to the latest block number"
        );

        let produced_block = env.node_clients[0]
            .get_block_by_number(BlockNumberOrTag::Number(produced_block_number))
            .await?
            .expect("new block should be retrievable via RPC");

        if count == 0 {
            assert_eq!(
                produced_block.header.inner.gas_used, 0,
                "{label} block should have zero gas used"
            );
        } else {
            assert!(
                produced_block.header.inner.gas_used > 0,
                "{label} block should consume gas"
            );
        }
        assert_eq!(
            produced_block.header.inner.number, produced_block_number,
            "RPC block number should match the execution payload"
        );

        last_block_hash = produced_block.header.hash;

        match &produced_block.transactions {
            BlockTransactions::Full(txs) => assert_eq!(
                txs.len(),
                count,
                "{label} block should contain the expected number of transactions (full variant)"
            ),
            BlockTransactions::Hashes(hashes) => assert_eq!(
                hashes.len(),
                count,
                "{label} block should contain the expected number of transaction hashes"
            ),
            BlockTransactions::Uncle => panic!("unexpected uncle-only block representation"),
        };

        for raw_tx in &batch {
            let mut raw_slice = raw_tx.as_ref();
            let tx_envelope =
                TxEnvelope::decode_2718(&mut raw_slice).expect("transaction should decode");
            let tx_hash = *tx_envelope.tx_hash();

            let receipt: Receipt = EthApiClient::<
                TransactionRequest,
                Transaction,
                Block,
                Receipt,
                Header,
                Bytes,
            >::transaction_receipt(
                &env.node_clients[0].rpc, tx_hash
            )
            .await?
            .expect("transaction receipt should exist");
            assert!(receipt.status(), "{label} transaction should succeed");
        }
    }

    let current_block_info = env
        .current_block_info()
        .expect("environment should track latest block info");
    assert_eq!(
        current_block_info.number, parent_number,
        "environment should advance to the newly produced block"
    );
    assert_eq!(
        current_block_info.hash, last_block_hash,
        "environment hash should match the RPC block hash"
    );

    drop(setup);

    Ok(())
}

/// Validates gas limit defaults and error handling for Engine API payloads.
///
/// This test first omits the gas limit and ensures the builder inherits the parent's value.
/// It then attempts to build a payload with an explicit zero gas limit and verifies the builder
/// falls back to the parent limit without advancing the canonical head.
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_engine_api_gas_limit_handling() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_spec = create_test_chain_spec();
    let chain_id = chain_spec.chain().id();

    let mut setup = Setup::<EvolveEngineTypes>::default()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(false)
        .with_tree_config(e2e_test_tree_config());

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("parent block should exist");

    let mut parent_hash = parent_block.header.hash;
    let mut parent_number = parent_block.header.inner.number;
    let mut parent_timestamp = parent_block.header.inner.timestamp;
    let parent_gas_limit = parent_block.header.inner.gas_limit;
    let fee_recipient = Address::repeat_byte(0xCD);

    let mut wallets = Wallet::new(1).with_chain_id(chain_id).wallet_gen();
    let sender_wallet = wallets.remove(0);
    let mut next_nonce = 0u64;

    let inherit_batch = make_transfer_batch(chain_id, &sender_wallet, next_nonce, 1).await;
    next_nonce += 1;
    let envelope = build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        None,
        inherit_batch,
        fee_recipient,
    )
    .await?;

    let inherited_payload = envelope.execution_payload.clone();
    let inherited_inner = inherited_payload.payload_inner.payload_inner;
    assert_eq!(
        inherited_inner.gas_limit, parent_gas_limit,
        "payload without explicit gas limit should inherit parent gas limit"
    );

    let inherited_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Number(parent_number))
        .await?
        .expect("inherited block should exist");
    assert_eq!(
        inherited_block.header.inner.gas_limit, parent_gas_limit,
        "block header should reflect inherited gas limit"
    );

    let invalid_batch = make_transfer_batch(chain_id, &sender_wallet, next_nonce, 1).await;
    let next_timestamp = parent_timestamp + 12;
    let fork_choice = ForkchoiceState {
        head_block_hash: parent_hash,
        safe_block_hash: parent_hash,
        finalized_block_hash: parent_hash,
    };
    let invalid_attrs = EvolveEnginePayloadAttributes {
        inner: PayloadAttributes {
            timestamp: next_timestamp,
            prev_randao: B256::random(),
            suggested_fee_recipient: fee_recipient,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
            slot_number: None,
        },
        transactions: Some(invalid_batch),
        gas_limit: Some(0),
    };

    let engine_client = env.node_clients[0].engine.http_client();
    let response = EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        fork_choice,
        Some(invalid_attrs),
    )
    .await?;

    assert!(
        matches!(response.payload_status.status, PayloadStatusEnum::Valid),
        "zero gas limit payload should be treated as valid"
    );

    if let Some(payload_id) = response.payload_id {
        match EngineApiClient::<EvolveEngineTypes>::get_payload_v3(&engine_client, payload_id).await
        {
            Ok(payload) => {
                let effective_limit = payload
                    .execution_payload
                    .payload_inner
                    .payload_inner
                    .gas_limit;
                assert_eq!(
                    effective_limit, parent_gas_limit,
                    "zero gas limit should fall back to parent gas limit"
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Unknown payload"),
                    "unexpected error retrieving payload: {msg}"
                );
            }
        }
    }

    let head_info = env
        .current_block_info()
        .expect("environment should track latest block info");
    assert_eq!(
        head_info.number, parent_number,
        "zero gas limit request must not advance the canonical head without finalization"
    );
    assert_eq!(
        head_info.hash, parent_hash,
        "zero gas limit request must not change the canonical hash"
    );

    drop(setup);

    Ok(())
}
