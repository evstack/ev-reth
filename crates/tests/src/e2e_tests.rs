use alloy_consensus::TxEnvelope;
use alloy_network::eip2718::Decodable2718;
use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::{
    eth::{Block, BlockTransactions, Header, Receipt, Transaction, TransactionRequest},
    BlockId,
};
use alloy_rpc_types_engine::{ForkchoiceState, PayloadAttributes, PayloadStatusEnum};
use eyre::Result;
use futures::future;
use reth_e2e_test_utils::{
    testsuite::{
        actions::MakeCanonical,
        setup::{NetworkSetup, Setup},
        Environment, TestBuilder,
    },
    transaction::TransactionTestContext,
    wallet::Wallet,
};
use reth_rpc_api::clients::{EngineApiClient, EthApiClient};

use crate::common::{create_test_chain_spec, create_test_chain_spec_with_base_fee_sink};
use ev_node::{EvolveEnginePayloadAttributes, EvolveEngineTypes, EvolveNode};

#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_single_node_produces_blocks() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_spec = create_test_chain_spec();

    let setup = Setup::<EvolveEngineTypes>::new()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true);

    TestBuilder::new()
        .with_setup(setup)
        .with_action(reth_e2e_test_utils::testsuite::actions::ProduceBlocks::<
            EvolveEngineTypes,
        >::new(2))
        .with_action(MakeCanonical::new())
        .with_action(|env: &Environment<EvolveEngineTypes>| {
            let latest = env
                .current_block_info()
                .expect("latest block info available");
            assert_eq!(
                latest.number, 2,
                "expected head block #2 after producing two blocks"
            );
            future::ready(Ok(()))
        })
        .run::<EvolveNode>()
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_base_fee_sink_receives_base_fee() -> Result<()> {
    reth_tracing::init_test_tracing();

    let fee_sink = Address::repeat_byte(0xAA);
    let chain_spec = create_test_chain_spec_with_base_fee_sink(Some(fee_sink));
    let chain_id = chain_spec.chain().id();

    let mut setup = Setup::<EvolveEngineTypes>::new()
        .with_chain_spec(chain_spec)
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true);

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let initial_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            fee_sink,
            Some(BlockId::latest()),
        )
        .await?;

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("parent block should exist");
    let parent_hash = parent_block.header.hash;
    let parent_timestamp = parent_block.header.inner.timestamp;
    let parent_number = parent_block.header.inner.number;
    let gas_limit = parent_block.header.inner.gas_limit;

    let mut wallets = Wallet::default().with_chain_id(chain_id).wallet_gen();
    let sender = wallets.remove(0);
    let raw_tx = TransactionTestContext::transfer_tx_bytes(chain_id, sender).await;

    let payload_attributes = EvolveEnginePayloadAttributes {
        inner: PayloadAttributes {
            timestamp: parent_timestamp + 12,
            prev_randao: B256::random(),
            suggested_fee_recipient: fee_sink,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
        },
        transactions: Some(vec![raw_tx.clone()]),
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
        Some(payload_attributes.clone()),
    )
    .await?;
    let payload_id = fcu_response.payload_id.expect("payload id returned");

    let payload_envelope =
        EngineApiClient::<EvolveEngineTypes>::get_payload_v3(&engine_client, payload_id).await?;

    let execution_payload = payload_envelope.execution_payload.clone();
    let new_payload_status = EngineApiClient::<EvolveEngineTypes>::new_payload_v3(
        &engine_client,
        execution_payload.clone(),
        vec![],
        B256::ZERO,
    )
    .await?;
    assert!(
        matches!(new_payload_status.status, PayloadStatusEnum::Valid),
        "expected payload to be valid, got {:?}",
        new_payload_status.status
    );

    let new_block_hash = execution_payload.payload_inner.payload_inner.block_hash;
    let new_block_number = execution_payload.payload_inner.payload_inner.block_number;
    let new_block_timestamp = execution_payload.payload_inner.payload_inner.timestamp;

    assert_eq!(
        new_block_number,
        parent_number + 1,
        "expected block number to increment"
    );

    let finalize_fcu = ForkchoiceState {
        head_block_hash: new_block_hash,
        safe_block_hash: new_block_hash,
        finalized_block_hash: new_block_hash,
    };
    EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        finalize_fcu,
        None,
    )
    .await?;

    env.set_current_block_info(reth_e2e_test_utils::testsuite::BlockInfo {
        hash: new_block_hash,
        number: new_block_number,
        timestamp: new_block_timestamp,
    })?;
    env.active_node_state_mut()?.latest_header_time = new_block_timestamp;

    let latest_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("latest block should exist");

    match &latest_block.transactions {
        BlockTransactions::Full(txs) => assert!(
            !txs.is_empty(),
            "expected at least one transaction in the produced block"
        ),
        BlockTransactions::Hashes(hashes) => assert!(
            !hashes.is_empty(),
            "expected at least one transaction hash in the produced block"
        ),
        BlockTransactions::Uncle => panic!("unexpected uncle block representation"),
    }

    let base_fee = U256::from(
        execution_payload
            .payload_inner
            .payload_inner
            .base_fee_per_gas,
    );
    let gas_used = execution_payload.payload_inner.payload_inner.gas_used;
    assert!(gas_used > 0, "expected block to consume gas");

    let expected_base_fee = base_fee * U256::from(gas_used);
    assert!(expected_base_fee > U256::ZERO, "expected non-zero credit");

    let mut raw_slice = raw_tx.as_ref();
    let envelope = TxEnvelope::decode_2718(&mut raw_slice).unwrap();
    let (max_fee_per_gas, max_priority_fee_per_gas) = match &envelope {
        TxEnvelope::Legacy(tx) => {
            let gas_price = U256::from(tx.tx().gas_price);
            (gas_price, gas_price)
        }
        TxEnvelope::Eip2930(tx) => {
            let gas_price = U256::from(tx.tx().gas_price);
            (gas_price, gas_price)
        }
        TxEnvelope::Eip1559(tx) => (
            U256::from(tx.tx().max_fee_per_gas),
            U256::from(tx.tx().max_priority_fee_per_gas),
        ),
        _ => panic!("unexpected transaction type for base-fee sink test"),
    };

    let tip_cap = max_fee_per_gas.saturating_sub(base_fee);
    let tip_per_gas = tip_cap.min(max_priority_fee_per_gas);
    let expected_tip = tip_per_gas * U256::from(gas_used);
    let expected_total_credit = expected_base_fee + expected_tip;

    let final_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            fee_sink,
            Some(BlockId::latest()),
        )
        .await?;

    let credited = final_balance.saturating_sub(initial_balance);
    assert_eq!(
        credited, expected_total_credit,
        "base fee sink should collect base fee plus tip"
    );

    drop(setup);

    Ok(())
}
