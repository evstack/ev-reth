use std::time::Duration;

use alloy_eips::BlockNumberOrTag;
use alloy_primitives::{Address, U256};
use alloy_rpc_types::{
    eth::{Block, BlockTransactions, Header, Receipt, Transaction, TransactionRequest},
    BlockId,
};
use eyre::Result;
use futures::future;
use reth_e2e_test_utils::{
    testsuite::{
        actions::{
            Action, BroadcastNextNewPayload, GenerateNextPayload, GeneratePayloadAttributes,
            MakeCanonical, PickNextBlockProducer, UpdateBlockInfoToLatestPayload,
        },
        setup::{NetworkSetup, Setup},
        Environment, TestBuilder,
    },
    transaction::TransactionTestContext,
    wallet::Wallet,
};
use reth_rpc_api::clients::EthApiClient;
use tokio::time::sleep;

use crate::common::{create_test_chain_spec, create_test_chain_spec_with_base_fee_sink};
use ev_node::{EvolveEngineTypes, EvolveNode};

/// Helper that produces a block and marks it canonical within the provided environment.
async fn produce_canonical_block(env: &mut Environment<EvolveEngineTypes>) -> Result<()> {
    let mut pick_next = PickNextBlockProducer::default();
    pick_next.execute(env).await?;

    let mut generate_attributes = GeneratePayloadAttributes::default();
    generate_attributes.execute(env).await?;

    let mut generate_payload = GenerateNextPayload::default();
    generate_payload.execute(env).await?;

    let mut broadcast_payload = BroadcastNextNewPayload::default();
    broadcast_payload.execute(env).await?;

    let mut update_block = UpdateBlockInfoToLatestPayload::default();
    update_block.execute(env).await?;

    let mut make_canonical = MakeCanonical::new();
    make_canonical.execute(env).await?;

    Ok(())
}

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

    let mut wallets = Wallet::default().with_chain_id(chain_id).wallet_gen();
    let sender = wallets.remove(0);
    let raw_tx = TransactionTestContext::transfer_tx_bytes(chain_id, sender).await;

    let tx_hash = EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::send_raw_transaction(
        &env.node_clients[0].rpc,
        raw_tx,
    )
    .await?;

    sleep(Duration::from_millis(200)).await;

    produce_canonical_block(&mut env).await?;

    let latest_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("latest block should exist");

    let included = match &latest_block.transactions {
        BlockTransactions::Full(txs) => txs.iter().any(|tx| *tx.inner.hash() == tx_hash),
        BlockTransactions::Hashes(hashes) => hashes.contains(&tx_hash),
        BlockTransactions::Uncle => false,
    };
    assert!(
        included,
        "expected transaction {tx_hash:?} to be included in block {}",
        latest_block.header.number
    );

    let base_fee = latest_block
        .header
        .inner
        .base_fee_per_gas
        .expect("base fee should be present");
    let gas_used = latest_block.header.inner.gas_used;
    let expected_credit = U256::from(base_fee) * U256::from(gas_used);
    assert!(
        expected_credit > U256::ZERO,
        "expected non-zero base fee credit"
    );

    let final_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            fee_sink,
            Some(BlockId::latest()),
        )
        .await?;

    let credited = final_balance.saturating_sub(initial_balance);
    assert_eq!(
        credited, expected_credit,
        "base fee sink should collect base fees"
    );

    drop(setup);

    Ok(())
}
