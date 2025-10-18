use alloy_consensus::{TxEnvelope, TxReceipt};
use alloy_eips::{eip2718::Encodable2718, BlockNumberOrTag};
use alloy_network::eip2718::Decodable2718;
use alloy_primitives::{address, Address, Bytes, TxKind, B256, U256};
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
    /// Mint and burn proxy contract interface.
    ///
    /// This contract acts as a proxy to the mint/burn precompile at address 0xf1.
    /// It forwards mint and burn calls to the precompile, allowing the designated
    /// mint admin contract to control token supply.
    contract MintAdminProxy {
        function mint(address to, uint256 amount);
        function burn(address from, uint256 amount);
    }
}

/// Bytecode for the `MintAdminProxy` contract.
///
/// This minimal proxy contract forwards all calls to the mint/burn precompile
/// at address 0x000000000000000000000000000000000000f1.
/// The bytecode delegates all function calls to the precompile using DELEGATECALL.
const ADMIN_PROXY_INITCODE: [u8; 54] = alloy_primitives::hex!(
    "602a600c600039602a6000f336600060003760006000366000600073000000000000000000000000000000000000f1005af1600080f3"
);

/// Test recipient address used in mint/burn tests.
const TEST_MINT_RECIPIENT: Address = address!("0x0101010101010101010101010101010101010101");

/// Computes the contract address that will be created by a deployer at a given nonce.
///
/// Uses the CREATE opcode address derivation formula: keccak256(rlp([sender, nonce])).
///
/// # Arguments
/// * `deployer` - Address of the contract deployer
/// * `nonce` - Nonce value for the deployment transaction
///
/// # Returns
/// The deterministic contract address that will be created
fn contract_address_from_nonce(deployer: Address, nonce: u64) -> Address {
    deployer.create(nonce)
}

/// Builds and submits a block containing the specified transactions via the Engine API.
///
/// This helper function orchestrates the complete block building process:
/// 1. Creates payload attributes with the provided transactions
/// 2. Calls `engine_forkchoiceUpdatedV3` to initiate payload building
/// 3. Retrieves the built payload via `engine_getPayloadV3`
/// 4. Submits the payload via `engine_newPayloadV3`
/// 5. Finalizes the block via another `engine_forkchoiceUpdatedV3` call
/// 6. Updates the environment state with the new block info
///
/// # Arguments
/// * `env` - Test environment containing the node client
/// * `parent_hash` - Hash of the parent block (updated to new block hash)
/// * `parent_number` - Number of the parent block (updated to new block number)
/// * `parent_timestamp` - Timestamp of the parent block (updated to new block timestamp)
/// * `gas_limit` - Gas limit for the new block
/// * `transactions` - RLP-encoded transactions to include in the block
/// * `suggested_fee_recipient` - Address to receive block rewards and fees
///
/// # Returns
/// The execution payload envelope for the newly built block
///
/// # Panics
/// Panics if the payload is not marked as valid by the engine
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
use ev_node::{
    EvolveEnginePayloadAttributes, EvolveEngineTypes, EvolveNode, EvolvePayloadBuilderConfig,
};

/// Tests that a single ev-reth node can successfully produce blocks.
///
/// # Test Flow
/// 1. Initializes a single-node test environment with dev mode enabled
/// 2. Produces 2 blocks using the Engine API
/// 3. Marks the chain as canonical
/// 4. Verifies the head block number is 2
///
/// # What It Tests
/// - Basic block production functionality
/// - Engine API integration
/// - Block chain progression
/// - Canonical chain management
///
/// # Success Criteria
/// - Node successfully produces exactly 2 blocks
/// - Final head block number is 2
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

/// Tests that the base fee sink address correctly receives base fees and priority tips.
///
/// # Test Flow
/// 1. Creates a chain spec with a designated base fee sink address (0xAAAA...AA)
/// 2. Records the sink's initial balance
/// 3. Builds a block containing a transfer transaction
/// 4. Calculates expected fees:
///    - Base fee = `base_fee_per_gas` × `gas_used`
///    - Priority tip = `min(tip_cap`, `max_priority_fee_per_gas`) × `gas_used`
/// 5. Verifies the sink receives exactly `base_fee` + `priority_tip`
///
/// # What It Tests
/// - Base fee sink mechanism (Evolve-specific feature)
/// - Fee calculation and distribution
/// - Transaction gas consumption
/// - Priority fee (tip) handling for different transaction types (Legacy, EIP-2930, EIP-1559)
///
/// # Success Criteria
/// - Block consumes gas (`gas_used` > 0)
/// - Base fee sink balance increases by exactly (`base_fee` + tip)
/// - Fee calculations match expected values for the transaction type
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

/// Tests minting and burning tokens to/from a dynamically generated wallet not in genesis.
///
/// # Test Flow
/// 1. Creates a fresh wallet address (not in genesis, zero initial balance)
/// 2. Deploys the `MintAdminProxy` contract at a predetermined address
/// 3. Mints 0.005 ETH to the new wallet via the mint precompile
/// 4. Verifies the wallet balance increases to exactly the minted amount
/// 5. Burns 0.002 ETH from the wallet via the burn precompile
/// 6. Verifies the wallet balance decreases by exactly the burned amount
///
/// # What It Tests
/// - Mint precompile functionality for non-genesis addresses
/// - Burn precompile functionality
/// - Balance state changes from mint/burn operations
/// - Admin proxy contract delegation to precompile (0xf1)
/// - Transaction receipt validation for mint/burn operations
///
/// # Success Criteria
/// - New wallet starts with zero balance (proving it's not in genesis)
/// - After minting: balance = `mint_amount` (0.005 ETH)
/// - After burning: balance = `mint_amount` - `burn_amount` (0.003 ETH)
/// - All transactions succeed (receipt status = true)
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_mint_and_burn_to_new_wallet() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_id = TEST_CHAIN_ID;

    // Create deployer wallet from the standard test wallets
    let mut wallets = Wallet::new(1).with_chain_id(chain_id).wallet_gen();
    let deployer = wallets.remove(0);
    let deployer_address = deployer.address();

    // Generate a truly random address not in genesis by using a random private key
    // We don't need the private key since we're only minting/burning TO this address
    let new_wallet_address = Address::random();

    println!("Deployer address: {}", deployer_address);
    println!("New wallet address: {}", new_wallet_address);

    let contract_address = contract_address_from_nonce(deployer_address, 0);
    let chain_spec = create_test_chain_spec_with_mint_admin(contract_address);
    let evolve_config =
        EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).expect("valid config");
    assert_eq!(
        evolve_config.mint_admin,
        Some(contract_address),
        "chainspec should propagate mint admin address"
    );

    let mut setup = Setup::<EvolveEngineTypes>::new()
        .with_chain_spec(chain_spec.clone())
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true);

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    // Check initial balance of new wallet (should be zero since not in genesis)
    let initial_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            new_wallet_address,
            Some(BlockId::latest()),
        )
        .await?;
    println!("New wallet initial balance: {}", initial_balance);
    assert_eq!(
        initial_balance,
        U256::ZERO,
        "randomly generated wallet should have zero balance (not in genesis)"
    );

    let parent_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("parent block should exist");
    let mut parent_hash = parent_block.header.hash;
    let mut parent_timestamp = parent_block.header.inner.timestamp;
    let mut parent_number = parent_block.header.inner.number;
    let gas_limit = parent_block.header.inner.gas_limit;

    // Deploy proxy contract at the predetermined admin address.
    let deploy_tx = TransactionRequest {
        nonce: Some(0),
        gas: Some(1_000_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        value: Some(U256::ZERO),
        to: Some(TxKind::Create),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from_static(&ADMIN_PROXY_INITCODE)),
        },
        ..Default::default()
    };

    let deploy_envelope = TransactionTestContext::sign_tx(deployer.clone(), deploy_tx).await;
    let deploy_raw: Bytes = deploy_envelope.encoded_2718().into();

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

    println!("Deployed admin proxy contract at: {}", contract_address);

    // Mint tokens to the new wallet via contract proxy.
    let mint_amount = U256::from(5_000_000_000_000_000u64); // 0.005 ETH
    let mint_call = MintAdminProxy::mintCall {
        to: new_wallet_address,
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

    let mint_tx_hash = *mint_envelope.tx_hash();
    let mint_receipt = EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::transaction_receipt(
        &env.node_clients[0].rpc,
        mint_tx_hash,
    )
    .await?
    .expect("mint transaction receipt available");
    println!(
        "Mint receipt status: {}, logs: {:?}",
        mint_receipt.status(),
        mint_receipt.logs
    );
    assert!(
        mint_receipt.status(),
        "mint proxy transaction should succeed"
    );

    let balance_after_mint =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            new_wallet_address,
            Some(BlockId::latest()),
        )
        .await?;
    println!("New wallet balance after mint: {}", balance_after_mint);
    assert_eq!(
        balance_after_mint, mint_amount,
        "new wallet should have exactly the minted amount"
    );

    // Burn tokens from the new wallet.
    let burn_amount = U256::from(2_000_000_000_000_000u64); // Burn 0.002 ETH
    let burn_call = MintAdminProxy::burnCall {
        from: new_wallet_address,
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

    let burn_tx_hash = *burn_envelope.tx_hash();
    let burn_receipt = EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::transaction_receipt(
        &env.node_clients[0].rpc,
        burn_tx_hash,
    )
    .await?
    .expect("burn transaction receipt available");
    println!(
        "Burn receipt status: {}, logs: {:?}",
        burn_receipt.status(),
        burn_receipt.logs
    );
    assert!(
        burn_receipt.status(),
        "burn proxy transaction should succeed"
    );

    let final_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            new_wallet_address,
            Some(BlockId::latest()),
        )
        .await?;
    println!("New wallet final balance after burn: {}", final_balance);

    let expected_final_balance = mint_amount - burn_amount;
    assert_eq!(
        final_balance, expected_final_balance,
        "burn should reduce the balance by the burned amount (expected: {}, got: {})",
        expected_final_balance, final_balance
    );

    println!(
        "Test passed! Minted {} to new wallet, burned {}, final balance: {}",
        mint_amount, burn_amount, final_balance
    );

    drop(setup);

    Ok(())
}

/// Tests the mint and burn precompile functionality via an admin proxy contract.
///
/// # Test Flow
/// 1. Computes the deployment address for the admin proxy contract
/// 2. Creates a chain spec designating that contract as the mint admin
/// 3. Records the recipient's initial balance (may be non-zero if in genesis)
/// 4. Deploys the `MintAdminProxy` contract to the predetermined address
/// 5. Mints tokens to a hardcoded test recipient (`TEST_MINT_RECIPIENT`)
/// 6. Verifies the mint succeeded and balance increased correctly
/// 7. Burns half the minted amount from the recipient
/// 8. Verifies the burn succeeded and balance decreased correctly
///
/// # What It Tests
/// - Mint admin authorization mechanism
/// - Contract deployment at predetermined address
/// - Mint precompile invocation via DELEGATECALL from admin contract
/// - Burn precompile invocation via DELEGATECALL from admin contract
/// - Balance queries at specific block numbers
/// - Transaction receipt validation
/// - Chain spec mint admin configuration propagation
///
/// # Success Criteria
/// - Chain spec correctly configures the mint admin address
/// - Admin proxy contract deploys successfully
/// - Mint transaction succeeds and increases balance by `mint_amount`
/// - Burn transaction succeeds and decreases balance by `burn_amount`
/// - Final balance = `initial_balance` + `mint_amount` - `burn_amount`
#[tokio::test(flavor = "multi_thread")]
async fn test_e2e_mint_precompile_via_contract() -> Result<()> {
    reth_tracing::init_test_tracing();

    let chain_id = TEST_CHAIN_ID;

    let mut wallets = Wallet::new(4).with_chain_id(chain_id).wallet_gen();
    let deployer = wallets.remove(0);
    let _unused_wallet = wallets.remove(0);
    let deployer_address = deployer.address();
    let recipient_address = TEST_MINT_RECIPIENT;

    let contract_address = contract_address_from_nonce(deployer_address, 0);
    let chain_spec = create_test_chain_spec_with_mint_admin(contract_address);
    let evolve_config =
        EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).expect("valid config");
    assert_eq!(
        evolve_config.mint_admin,
        Some(contract_address),
        "chainspec should propagate mint admin address"
    );

    let mut setup = Setup::<EvolveEngineTypes>::new()
        .with_chain_spec(chain_spec.clone())
        .with_network(NetworkSetup::single_node())
        .with_dev_mode(true);

    let mut env = Environment::<EvolveEngineTypes>::default();
    setup.apply::<EvolveNode>(&mut env).await?;

    let recipient_initial_balance =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            recipient_address,
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

    // Deploy proxy contract at the predetermined admin address.
    let deploy_tx = TransactionRequest {
        nonce: Some(0),
        gas: Some(1_000_000),
        max_fee_per_gas: Some(20_000_000_000),
        max_priority_fee_per_gas: Some(2_000_000_000),
        chain_id: Some(chain_id),
        value: Some(U256::ZERO),
        to: Some(TxKind::Create),
        input: TransactionInput {
            input: None,
            data: Some(Bytes::from_static(&ADMIN_PROXY_INITCODE)),
        },
        ..Default::default()
    };

    let deploy_envelope = TransactionTestContext::sign_tx(deployer.clone(), deploy_tx).await;
    let deploy_raw: Bytes = deploy_envelope.encoded_2718().into();

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

    let latest_block = env.node_clients[0]
        .get_block_by_number(BlockNumberOrTag::Latest)
        .await?
        .expect("latest block available after mint");
    let tx_count = match latest_block.transactions {
        BlockTransactions::Full(ref txs) => txs.len(),
        BlockTransactions::Hashes(ref hashes) => hashes.len(),
        BlockTransactions::Uncle => 0,
    };
    println!(
        "latest block number {} tx count {}",
        latest_block.number(),
        tx_count
    );

    // Mint tokens via contract proxy.
    let mint_amount = U256::from(1_000_000_000_000_000u64);
    let mint_call = MintAdminProxy::mintCall {
        to: recipient_address,
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

    let mint_tx_hash = *mint_envelope.tx_hash();
    let mint_receipt = EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::transaction_receipt(
        &env.node_clients[0].rpc,
        mint_tx_hash,
    )
    .await?
    .expect("mint transaction receipt available");
    println!(
        "mint receipt status: {}, logs: {:?}",
        mint_receipt.status(),
        mint_receipt.logs
    );
    assert!(
        mint_receipt.status(),
        "mint proxy transaction reverted on execution"
    );

    let balance_after_mint =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            recipient_address,
            Some(BlockId::latest()),
        )
        .await?;
    let balance_at_block =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            recipient_address,
            Some(BlockId::Number(BlockNumberOrTag::Number(parent_number))),
        )
        .await?;
    let contract_balance_after_mint =
        EthApiClient::<TransactionRequest, Transaction, Block, Receipt, Header>::balance(
            &env.node_clients[0].rpc,
            contract_address,
            Some(BlockId::latest()),
        )
        .await?;
    println!(
        "mintee balance diff: {} -> {} (latest) | {} (@block {}), contract balance now: {}",
        recipient_initial_balance,
        balance_after_mint,
        balance_at_block,
        parent_number,
        contract_balance_after_mint
    );
    assert_eq!(
        balance_after_mint.saturating_sub(recipient_initial_balance),
        mint_amount,
        "minted amount should credit recipient"
    );

    // Burn a portion through the same proxy contract.
    let burn_amount = mint_amount / U256::from(2u8);
    let burn_call = MintAdminProxy::burnCall {
        from: recipient_address,
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
            recipient_address,
            Some(BlockId::latest()),
        )
        .await?;
    assert_eq!(
        final_balance,
        recipient_initial_balance + mint_amount - burn_amount,
        "burn should reduce the previously minted balance",
    );

    drop(setup);

    Ok(())
}
