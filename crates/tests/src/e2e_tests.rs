use alloy_consensus::TxEnvelope;
use alloy_eips::{eip2718::Encodable2718, BlockNumberOrTag};
use alloy_network::eip2718::Decodable2718;
use alloy_primitives::{keccak256, Address, Bytes, TxKind, B256, U256};
use alloy_rpc_types::{
    eth::{
        Block, BlockTransactions, Header, Receipt, Transaction, TransactionInput,
        TransactionRequest,
    },
    BlockId,
};
use alloy_rpc_types_engine::{ForkchoiceState, PayloadAttributes, PayloadStatusEnum};
use alloy_sol_types::{sol, SolCall};
use eyre::Result;
use futures::future;
use reth_e2e_test_utils::{
    testsuite::{
        actions::MakeCanonical,
        setup::{NetworkSetup, Setup},
        BlockInfo, Environment, TestBuilder,
    },
    transaction::TransactionTestContext,
    wallet::Wallet,
};
use reth_rpc_api::clients::{EngineApiClient, EthApiClient};

use crate::common::{
    create_test_chain_spec, create_test_chain_spec_with_base_fee_sink,
    create_test_chain_spec_with_mint_admin, TEST_CHAIN_ID,
};

sol! {
    contract MintAdminProxy {
        function mint(address to, uint256 amount);
        function burn(address from, uint256 amount);
    }
}

const ADMIN_PROXY_INITCODE: [u8; 54] = alloy_primitives::hex!(
    "602a600c600039602a6000f360006000363760006000366000600073000000000000000000000000000000000000f1005af1600080f3"
);

fn contract_address_from_nonce(deployer: Address, nonce: u64) -> Address {
    let mut nonce_encoded = Vec::new();
    if nonce == 0 {
        nonce_encoded.push(0x80);
    } else if nonce <= 0x7f {
        nonce_encoded.push(nonce as u8);
    } else {
        let mut bytes = Vec::new();
        let mut value = nonce;
        while value > 0 {
            bytes.push((value & 0xff) as u8);
            value >>= 8;
        }
        bytes.reverse();
        nonce_encoded.push(0x80 + bytes.len() as u8);
        nonce_encoded.extend(bytes);
    }

    let payload_length = 1 + deployer.as_slice().len() + nonce_encoded.len();
    debug_assert!(
        payload_length < 56,
        "payload_length exceeds single-byte encoding"
    );

    let mut rlp_encoded = Vec::with_capacity(1 + payload_length);
    rlp_encoded.push(0xc0 + payload_length as u8);
    rlp_encoded.push(0x80 + deployer.as_slice().len() as u8);
    rlp_encoded.extend_from_slice(deployer.as_slice());
    rlp_encoded.extend_from_slice(&nonce_encoded);

    let hash = keccak256(rlp_encoded);
    Address::from_slice(&hash[12..])
}

async fn build_block_with_transactions(
    env: &mut Environment<EvolveEngineTypes>,
    parent_hash: &mut B256,
    parent_number: &mut u64,
    parent_timestamp: &mut u64,
    gas_limit: u64,
    transactions: Vec<Bytes>,
    suggested_fee_recipient: Address,
) -> Result<alloy_rpc_types_engine::ExecutionPayloadEnvelopeV3> {
    let payload_attributes = EvolveEnginePayloadAttributes {
        inner: PayloadAttributes {
            timestamp: *parent_timestamp + 12,
            prev_randao: B256::random(),
            suggested_fee_recipient,
            withdrawals: Some(vec![]),
            parent_beacon_block_root: Some(B256::ZERO),
        },
        transactions: Some(transactions),
        gas_limit: Some(gas_limit),
    };

    let fork_choice = ForkchoiceState {
        head_block_hash: *parent_hash,
        safe_block_hash: *parent_hash,
        finalized_block_hash: *parent_hash,
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

    EngineApiClient::<EvolveEngineTypes>::fork_choice_updated_v3(
        &engine_client,
        ForkchoiceState {
            head_block_hash: new_block_hash,
            safe_block_hash: new_block_hash,
            finalized_block_hash: new_block_hash,
        },
        None,
    )
    .await?;

    env.set_current_block_info(BlockInfo {
        hash: new_block_hash,
        number: new_block_number,
        timestamp: new_block_timestamp,
    })?;
    env.active_node_state_mut()?.latest_header_time = new_block_timestamp;

    *parent_hash = new_block_hash;
    *parent_number = new_block_number;
    *parent_timestamp = new_block_timestamp;

    Ok(payload_envelope)
}
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
    let mut parent_hash = parent_block.header.hash;
    let mut parent_timestamp = parent_block.header.inner.timestamp;
    let mut parent_number = parent_block.header.inner.number;
    let gas_limit = parent_block.header.inner.gas_limit;

    let mut wallets = Wallet::new(3).with_chain_id(chain_id).wallet_gen();
    let sender = wallets.remove(0);
    let raw_tx = TransactionTestContext::transfer_tx_bytes(chain_id, sender).await;

    let payload_envelope = build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        gas_limit,
        vec![raw_tx.clone()],
        fee_sink,
    )
    .await?;

    let execution_payload = payload_envelope.execution_payload.clone();

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

#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_mint_precompile_via_contract() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_id = TEST_CHAIN_ID;

    let mut wallets = Wallet::new(4).with_chain_id(chain_id).wallet_gen();
    let deployer = wallets.remove(0);
    let mintee_address = wallets.remove(0).address();
    let deployer_address = deployer.address();

    let contract_address = contract_address_from_nonce(deployer_address, 0);
    let chain_spec = create_test_chain_spec_with_mint_admin(contract_address);

    let mut setup = Setup::<EvolveEngineTypes>::new()
        .with_chain_spec(chain_spec.clone())
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true);

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

    // Deploy proxy contract at the predetermined admin address.
    let deploy_raw = TransactionTestContext::deploy_tx_bytes(
        chain_id,
        1_000_000,
        Bytes::from_static(&ADMIN_PROXY_INITCODE),
        deployer.clone(),
    )
    .await;

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        gas_limit,
        vec![deploy_raw],
        Address::ZERO,
    )
    .await?;

    // Mint tokens via contract proxy.
    let mint_amount = U256::from(1_000_000_000_000_000u64);
    let mint_call = MintAdminProxy::mintCall {
        to: mintee_address,
        amount: mint_amount,
    }
    .abi_encode();

    let mint_tx = TransactionRequest {
        nonce: Some(1),
        gas: Some(150_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        to: Some(TxKind::Call(contract_address)),
        value: Some(U256::ZERO),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from(mint_call)),
        },
        ..Default::default()
    };

    let mint_envelope = TransactionTestContext::sign_tx(deployer.clone(), mint_tx).await;
    let mint_raw: Bytes = mint_envelope.encoded_2718().into();

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        gas_limit,
        vec![mint_raw],
        Address::ZERO,
    )
    .await?;

    let balance_after_mint =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            mintee_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert_eq!(
        balance_after_mint, mint_amount,
        "minted amount should credit recipient"
    );

    // Burn a portion through the same proxy contract.
    let burn_amount = mint_amount / U256::from(2u8);
    let burn_call = MintAdminProxy::burnCall {
        from: mintee_address,
        amount: burn_amount,
    }
    .abi_encode();

    let burn_tx = TransactionRequest {
        nonce: Some(2),
        gas: Some(150_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        to: Some(TxKind::Call(contract_address)),
        value: Some(U256::ZERO),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from(burn_call)),
        },
        ..Default::default()
    };

    let burn_envelope = TransactionTestContext::sign_tx(deployer, burn_tx).await;
    let burn_raw: Bytes = burn_envelope.encoded_2718().into();

    build_block_with_transactions(
        &mut env,
        &mut parent_hash,
        &mut parent_number,
        &mut parent_timestamp,
        gas_limit,
        vec![burn_raw],
        Address::ZERO,
    )
    .await?;

    let final_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            mintee_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert_eq!(
        final_balance,
        mint_amount - burn_amount,
        "burn should reduce the previously minted balance",
    );

    drop(setup);

    Ok(())
}
