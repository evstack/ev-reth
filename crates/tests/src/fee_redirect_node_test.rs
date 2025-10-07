//! End-to-end test that boots the ev-reth node binary, submits a signed transaction, and
//! asserts that the configured base-fee sink receives the burn portion instead of the
//! canonical burn address.

use alloy_primitives::{address, Address, Bytes, ChainId, B256, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use eyre::{bail, ensure, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::{
    path::Path,
    str::FromStr,
    time::{Duration, Instant},
};
use tempfile::TempDir;
use tokio::{fs, process::Command, time::sleep};

const BASE_FEE_SINK: Address = address!("0x00000000000000000000000000000000000000fe");
const CALLER_KEY: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const GAS_PRICE: u128 = 1_000_000_000; // 1 gwei

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Requires the ev-reth binary and spawns a full node; run manually when needed"]
async fn node_redirects_base_fee_and_preserves_burn_sink() -> Result<()> {
    let mut signer = PrivateKeySigner::from_str(CALLER_KEY)?;
    signer.set_chain_id(Some(ChainId::from(1u64)));
    let caller = signer.address();

    let temp = TempDir::new().wrap_err("failed to create temp dir")?;
    let datadir = temp.path().join("datadir");
    fs::create_dir_all(&datadir)
        .await
        .wrap_err("failed to create datadir")?;

    let genesis_path = temp.path().join("genesis.json");
    write_genesis(&genesis_path, caller).await?;

    run_init(&datadir, &genesis_path).await?;

    let http_port: u16 = 19545;
    let auth_port: u16 = 19546;
    let rpc_url = format!("http://127.0.0.1:{http_port}");

    let mut node = spawn_node(&datadir, &genesis_path, http_port, auth_port).await?;

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .wrap_err("failed to build reqwest client")?;

    wait_for_rpc(&client, &rpc_url).await?;

    let fee_recipient = Address::random();

    // Record initial balances for assertions later.
    let initial_sink_balance = get_balance_at(&client, &rpc_url, BASE_FEE_SINK, "latest").await?;
    let initial_burn_balance = get_balance_at(&client, &rpc_url, Address::ZERO, "latest").await?;

    let (tx_bytes, tx_hash) = build_signed_legacy_transaction(&signer, GAS_PRICE)?;

    let _block_hash =
        build_block_with_transaction(&client, &rpc_url, &tx_bytes, fee_recipient).await?;

    let receipt = wait_for_receipt(&client, &rpc_url, tx_hash).await?;
    let block_number = receipt
        .get("blockNumber")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("missing blockNumber in receipt"))?;

    let block = get_block_by_number(&client, &rpc_url, block_number).await?;
    let block_number_value = hex_to_u128(block_number);
    let base_fee = hex_to_u128(
        block
            .get("baseFeePerGas")
            .and_then(Value::as_str)
            .unwrap_or("0x0"),
    );

    let gas_used = receipt
        .get("gasUsed")
        .and_then(Value::as_str)
        .map(hex_to_u128)
        .unwrap_or(0);

    ensure!(gas_used > 0, "transaction consumed no gas");
    ensure!(
        base_fee > 0,
        "base fee is zero, redirect cannot be verified"
    );

    let expected_redirect = U256::from(base_fee) * U256::from(gas_used);
    let sink_balance = get_balance_at(&client, &rpc_url, BASE_FEE_SINK, "latest").await?;
    let sink_delta = sink_balance.saturating_sub(initial_sink_balance);
    assert_eq!(
        sink_delta, expected_redirect,
        "base-fee sink should receive the burned amount"
    );

    let burn_balance = get_balance_at(&client, &rpc_url, Address::ZERO, "latest").await?;
    assert_eq!(
        burn_balance, initial_burn_balance,
        "burn address should remain unchanged"
    );

    let miner = block
        .get("miner")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("missing miner field"))?;
    let miner_address = Address::from_str(miner).wrap_err("invalid miner address")?;
    ensure!(
        miner_address == fee_recipient,
        "unexpected fee recipient in block"
    );
    let previous_tag = if block_number_value == 0 {
        "0x0".to_string()
    } else {
        format!("0x{:x}", block_number_value - 1)
    };
    let miner_before = get_balance_at(&client, &rpc_url, miner_address, &previous_tag).await?;
    let miner_after = get_balance_at(&client, &rpc_url, miner_address, "latest").await?;
    let miner_delta = miner_after.saturating_sub(miner_before);

    let base_fee_u256 = U256::from(base_fee);
    let effective_gas_price = U256::from(GAS_PRICE);
    let priority_fee = effective_gas_price
        .checked_sub(base_fee_u256)
        .unwrap_or_default();
    let expected_tip = priority_fee * U256::from(gas_used);
    assert_eq!(
        miner_delta, expected_tip,
        "miner should only receive the priority fee"
    );

    // Shut the node down to keep tests tidy.
    if let Some(id) = node.id() {
        tracing::info!(pid = id, "terminating ev-reth test node");
    }
    if let Err(err) = node.start_kill() {
        tracing::warn!(error = ?err, "failed to kill ev-reth test node child");
    }
    let _ = node.wait().await;

    Ok(())
}

async fn write_genesis(path: &Path, caller: Address) -> Result<()> {
    let alloc_balance = "0x56bc75e2d63100000"; // 100 ETH
    let genesis = json!({
        "config": {
            "chainId": 1,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "constantinopleBlock": 0,
            "petersburgBlock": 0,
            "istanbulBlock": 0,
            "berlinBlock": 0,
            "londonBlock": 0,
            "parisBlock": 0,
            "shanghaiTime": 0,
            "cancunTime": 0,
            "terminalTotalDifficulty": 0,
            "terminalTotalDifficultyPassed": true,
            "ev_reth": {
                "baseFeeSink": format!("{:#x}", BASE_FEE_SINK)
            }
        },
        "baseFeePerGas": "0x64",
        "difficulty": "0x1",
        "gasLimit": "0x1c9c380",
        "alloc": {
            format!("{:#x}", caller): { "balance": alloc_balance }
        }
    });

    let contents = serde_json::to_vec_pretty(&genesis)?;
    fs::write(path, contents)
        .await
        .wrap_err("failed to write genesis file")?;
    Ok(())
}

async fn run_init(datadir: &Path, genesis_path: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "run",
            "-p",
            "ev-reth",
            "--bin",
            "ev-reth",
            "--",
            "init",
            "--datadir",
            datadir.to_string_lossy().as_ref(),
            "--chain",
            genesis_path.to_string_lossy().as_ref(),
            "--log.file.max-files",
            "0",
            "--nat",
            "none",
        ])
        .status()
        .await
        .wrap_err("failed to run ev-reth init")?;

    ensure!(status.success(), "ev-reth init command failed");
    Ok(())
}

async fn spawn_node(
    datadir: &Path,
    genesis_path: &Path,
    http_port: u16,
    auth_port: u16,
) -> Result<tokio::process::Child> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-p",
        "ev-reth",
        "--bin",
        "ev-reth",
        "--",
        "node",
        "--datadir",
        datadir.to_string_lossy().as_ref(),
        "--chain",
        genesis_path.to_string_lossy().as_ref(),
        "--log.file.max-files",
        "0",
        "--nat",
        "none",
        "--http",
        "--http.addr",
        "127.0.0.1",
        "--http.port",
        &http_port.to_string(),
        "--authrpc.port",
        &auth_port.to_string(),
        "--port",
        "0",
        "--discovery.port",
        "0",
    ]);
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    let child = cmd.spawn().wrap_err("failed to spawn ev-reth node")?;
    Ok(child)
}

async fn wait_for_rpc(client: &Client, rpc_url: &str) -> Result<()> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "web3_clientVersion",
        "params": []
    });

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if Instant::now() > deadline {
            bail!("timed out waiting for ev-reth rpc");
        }

        match client.post(rpc_url).json(&payload).send().await {
            Ok(resp) if resp.status().is_success() => return Ok(()),
            Ok(_) | Err(_) => {}
        }

        sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_receipt(client: &Client, rpc_url: &str, tx_hash: B256) -> Result<Value> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionReceipt",
        "params": [format!("0x{}", hex::encode(tx_hash.as_slice()))]
    });

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if Instant::now() > deadline {
            bail!("timed out waiting for transaction receipt");
        }

        let resp = client.post(rpc_url).json(&payload).send().await?;
        let value: Value = resp.json().await?;
        if let Some(result) = value.get("result") {
            if !result.is_null() {
                return Ok(result.clone());
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}

async fn get_block_by_number(client: &Client, rpc_url: &str, number: &str) -> Result<Value> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getBlockByNumber",
        "params": [number, false]
    });

    let resp = client.post(rpc_url).json(&payload).send().await?;
    let value: Value = resp.json().await?;
    value
        .get("result")
        .cloned()
        .ok_or_else(|| eyre::eyre!("missing block result"))
}

async fn build_block_with_transaction(
    client: &Client,
    rpc_url: &str,
    tx_rlp: &Bytes,
    fee_recipient: Address,
) -> Result<B256> {
    let latest_block = get_block_by_number(client, rpc_url, "latest").await?;
    let parent_hash = latest_block
        .get("hash")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("missing parent hash"))?;
    let parent_timestamp = hex_to_u128(
        latest_block
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("0x0"),
    ) as u64;
    let parent_gas_limit = hex_to_u128(
        latest_block
            .get("gasLimit")
            .and_then(Value::as_str)
            .unwrap_or("0x0"),
    ) as u64;

    let forkchoice_state = json!({
        "headBlockHash": parent_hash,
        "safeBlockHash": parent_hash,
        "finalizedBlockHash": parent_hash,
    });

    let payload_attributes = json!({
        "timestamp": format!("0x{:x}", parent_timestamp + 1),
        "prevRandao": format!("0x{:064x}", 1u64),
        "suggestedFeeRecipient": format!("{:#x}", fee_recipient),
        "gasLimit": format!("0x{:x}", parent_gas_limit),
        "transactions": [format!("0x{}", hex::encode(tx_rlp.as_ref()))],
        "withdrawals": [],
        "parentBeaconBlockRoot": format!("0x{:064x}", 0u64),
    });

    let fcu_result = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "engine_forkchoiceUpdatedV3",
            "params": [forkchoice_state, payload_attributes]
        }))
        .send()
        .await?
        .json::<Value>()
        .await?;

    let status = fcu_result
        .get("payloadStatus")
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("INVALID");
    ensure!(status == "VALID", "forkchoiceUpdated returned {status}");

    let payload_id = fcu_result
        .get("payloadId")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("forkchoiceUpdated did not return payloadId"))?;

    let payload_response = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "engine_getPayloadV3",
            "params": [payload_id]
        }))
        .send()
        .await?
        .json::<Value>()
        .await?;

    let execution_payload = payload_response
        .get("executionPayload")
        .cloned()
        .ok_or_else(|| eyre::eyre!("engine_getPayloadV3 missing executionPayload"))?;

    let block_hash = execution_payload
        .get("blockHash")
        .and_then(Value::as_str)
        .ok_or_else(|| eyre::eyre!("executionPayload missing blockHash"))?;

    let new_payload_result = client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "engine_newPayloadV3",
            "params": [execution_payload.clone(), Vec::<String>::new(), format!("0x{:064x}", 0u64)]
        }))
        .send()
        .await?
        .json::<Value>()
        .await?;

    let new_payload_status = new_payload_result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("INVALID");
    ensure!(
        new_payload_status == "VALID",
        "newPayload returned {new_payload_status}"
    );

    client
        .post(rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "engine_forkchoiceUpdatedV3",
            "params": [
                json!({
                    "headBlockHash": block_hash,
                    "safeBlockHash": block_hash,
                    "finalizedBlockHash": block_hash,
                }),
                Value::Null
            ]
        }))
        .send()
        .await?
        .json::<Value>()
        .await?;

    B256::from_str(block_hash).map_err(|err| eyre::eyre!("invalid block hash: {err}"))
}

async fn get_balance_at(
    client: &Client,
    rpc_url: &str,
    address: Address,
    block_tag: &str,
) -> Result<U256> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getBalance",
        "params": [format!("{:#x}", address), block_tag]
    });

    let resp = client.post(rpc_url).json(&payload).send().await?;
    let value: Value = resp.json().await?;
    let balance_hex = value.get("result").and_then(Value::as_str).unwrap_or("0x0");
    U256::from_str_radix(balance_hex.trim_start_matches("0x"), 16)
        .map_err(|err| eyre::eyre!("failed to parse balance: {err}"))
}

fn hex_to_u128(value: &str) -> u128 {
    u128::from_str_radix(value.trim_start_matches("0x"), 16).unwrap_or(0)
}

fn build_signed_legacy_transaction(
    signer: &PrivateKeySigner,
    gas_price: u128,
) -> Result<(Bytes, B256)> {
    use alloy_consensus::{Signed, TxLegacy};
    use alloy_primitives::{ChainId, TxKind};

    let mut tx = TxLegacy {
        chain_id: Some(ChainId::from(1u64)),
        nonce: 0,
        gas_price,
        gas_limit: 21_000,
        to: TxKind::Call(Address::ZERO),
        value: U256::from(1_u64),
        input: Bytes::new(),
    };

    let signature = alloy_network::TxSignerSync::sign_transaction_sync(&signer, &mut tx)?;
    let signed = Signed::new_unhashed(tx, signature);

    let mut buf = Vec::with_capacity(signed.rlp_encoded_length());
    signed.rlp_encode(&mut buf);
    Ok((Bytes::from(buf), *signed.hash()))
}
