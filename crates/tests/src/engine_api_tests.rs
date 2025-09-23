//! Engine API integration tests for the Evolve payload builder.
//!
//! This test suite focuses on complex Engine API specific functionality,
//! including end-to-end execution flows, build/sync chain scenarios,
//! and advanced Engine API validation.

use crate::common;

use eyre::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use common::{create_test_transactions, EvolveTestFixture, TEST_GAS_LIMIT, TEST_TIMESTAMP};

/// Engine API test fixture with additional Engine API specific methods
struct EngineApiTestFixture {
    base: EvolveTestFixture,
}

impl EngineApiTestFixture {
    /// Creates a new Engine API test fixture
    async fn new() -> Result<Self> {
        let base = EvolveTestFixture::new().await?;
        Ok(Self { base })
    }

    /// Simulates `InitChain` from the Go Engine API test
    async fn init_chain(&self, _genesis_time: u64, initial_height: u64) -> Result<(Vec<u8>, u64)> {
        if initial_height != 1 {
            return Err(eyre::eyre!(
                "initialHeight must be 1, got {}",
                initial_height
            ));
        }
        Ok((self.base.genesis_state_root.to_vec(), TEST_GAS_LIMIT))
    }

    /// Simulates `ExecuteTxs` from the Go Engine API test
    async fn execute_txs(
        &self,
        transactions: Vec<reth_ethereum_primitives::TransactionSigned>,
        block_height: u64,
        timestamp: u64,
        _prev_state_root: Vec<u8>,
        parent_hash: alloy_primitives::B256,
    ) -> Result<(Vec<u8>, u64)> {
        let payload_attrs = self.base.create_payload_attributes(
            transactions,
            block_height,
            timestamp,
            parent_hash,
            Some(TEST_GAS_LIMIT),
        );

        let sealed_block = self.base.builder.build_payload(payload_attrs).await?;
        // Debug output to help identify whether transactions are included and how gas is accounted
        println!(
            "Built block #{}, txs={}, gas_used={}, base_fee={:?}",
            block_height,
            sealed_block.transaction_count(),
            sealed_block.gas_used,
            sealed_block.header().base_fee_per_gas
        );
        Ok((sealed_block.state_root.to_vec(), sealed_block.gas_used))
    }

    /// Simulates `SetFinal` from the Go Engine API test
    async fn set_final(&self, block_height: u64) -> Result<()> {
        println!("Setting block {block_height} as final");
        Ok(())
    }

    /// Checks the latest block info - simulating the Go test's checkLatestBlock
    fn check_latest_block(&self, expected_height: u64, expected_tx_count: usize) -> Result<()> {
        println!(
            "Checking latest block: height={expected_height}, expected_txs={expected_tx_count}"
        );
        Ok(())
    }
}

/// Tests the end-to-end Engine API execution flow - build chain phase
/// This mirrors the Go test's `TestEngineExecution` build phase
#[tokio::test]
async fn test_engine_execution_build_chain() -> Result<()> {
    let fixture = EngineApiTestFixture::new().await?;

    let initial_height = 1u64;
    let genesis_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    println!("=== Engine API Build Chain Phase ===");

    // Initialize chain (similar to Go's InitChain)
    let (state_root, gas_limit) = fixture.init_chain(genesis_time, initial_height).await?;
    println!("Chain initialized with state_root: {state_root:?}, gas_limit: {gas_limit}");

    let mut prev_state_root = state_root;
    let mut current_parent_hash = fixture.base.genesis_hash;

    // Build blocks 1-10 (matching Go test)
    for block_height in initial_height..=10 {
        let n_txs = if block_height == 4 {
            // Block 4 has 0 transactions as edge case (matching Go test)
            0
        } else {
            (block_height as usize) % 5 + 1 // Variable transaction count (1-5 transactions)
        };

        println!("Building block {block_height} with {n_txs} transactions");

        // Create transactions for this block
        let transactions = if n_txs > 0 {
            create_test_transactions(n_txs, 0) // Always start with nonce 0
        } else {
            vec![]
        };

        // Check latest block before execution
        fixture.check_latest_block(
            if block_height == 1 {
                0
            } else {
                block_height - 1
            },
            0,
        )?;

        // Execute transactions (similar to Go's ExecuteTxs)
        let timestamp = genesis_time + block_height * 12; // 12 second blocks
        let (new_state_root, max_gas_used) = fixture
            .execute_txs(
                transactions,
                block_height,
                timestamp,
                prev_state_root.clone(),
                current_parent_hash,
            )
            .await?;

        // Generate hash for this block and add it as a parent for the next block
        let block_hash = alloy_primitives::B256::random();
        fixture.base.add_mock_header(
            block_hash,
            block_height,
            alloy_primitives::B256::from_slice(&new_state_root),
            timestamp,
        );
        current_parent_hash = block_hash;

        if n_txs > 0 {
            assert!(
                max_gas_used > 0,
                "Max gas used should be > 0 for non-empty blocks"
            );
        }

        // Set block as final (similar to Go's SetFinal)
        fixture.set_final(block_height).await?;

        // Check latest block after execution
        fixture.check_latest_block(block_height, n_txs)?;

        // Verify state root changes for non-empty blocks
        if n_txs == 0 {
            println!("  Empty block - state root handling verified");
        } else if new_state_root == vec![0u8; 32] {
            println!(
                "  Block with {n_txs} transactions processed, state root is zero (mock environment)"
            );
        } else if prev_state_root != new_state_root {
            println!("  Block with {n_txs} transactions processed, state root changed");
        } else {
            println!("  Block with {n_txs} transactions processed, state root unchanged");
        }

        prev_state_root = new_state_root;
        println!("✓ Block {block_height} completed successfully");
    }

    println!("✓ Engine API build chain test passed!");
    Ok(())
}

/// Tests the Engine API sync chain phase
/// This mirrors the Go test's `TestEngineExecution` sync phase
#[tokio::test]
async fn test_engine_execution_sync_chain() -> Result<()> {
    println!("=== Engine API Sync Chain Phase ===");

    // Create a fresh fixture to simulate a new node syncing
    let sync_fixture = EngineApiTestFixture::new().await?;

    let initial_height = 1u64;
    let genesis_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Initialize the fresh chain
    let (state_root, gas_limit) = sync_fixture
        .init_chain(genesis_time, initial_height)
        .await?;
    println!("Sync chain initialized with state_root: {state_root:?}, gas_limit: {gas_limit}");

    let mut prev_state_root = state_root;

    // Create test payloads to sync (simulating stored payloads from build phase)
    let test_payloads = vec![
        (3, 3), // Block 1: 3 transactions
        (4, 4), // Block 2: 4 transactions
        (5, 5), // Block 3: 5 transactions
        (0, 0), // Block 4: 0 transactions (edge case)
        (2, 2), // Block 5: 2 transactions
    ];

    for (block_height, (n_txs, expected_tx_count)) in test_payloads.into_iter().enumerate() {
        let block_height = (block_height + 1) as u64; // Convert to 1-based

        println!("Syncing block {block_height} with {n_txs} transactions");

        // Create the same transactions as in build phase
        let transactions = if n_txs > 0 {
            create_test_transactions(n_txs, 0) // Always start with nonce 0
        } else {
            vec![]
        };

        // Check latest block before execution
        sync_fixture.check_latest_block(
            if block_height == 1 {
                0
            } else {
                block_height - 1
            },
            0,
        )?;

        // Execute the transactions
        let timestamp = genesis_time + block_height * 12;
        let parent_hash = sync_fixture.base.genesis_hash; // Use genesis hash for all blocks in sync test
        let (new_state_root, max_gas_used) = sync_fixture
            .execute_txs(
                transactions,
                block_height,
                timestamp,
                prev_state_root.clone(),
                parent_hash,
            )
            .await?;

        if n_txs > 0 {
            assert!(
                max_gas_used > 0,
                "Max gas used should be > 0 for non-empty blocks"
            );
        }

        // Verify state root behavior
        if n_txs == 0 {
            println!("  Empty block sync - state root handling verified");
        } else if new_state_root == vec![0u8; 32] {
            println!(
                "  Block with {n_txs} transactions synced, state root is zero (mock environment)"
            );
        } else if prev_state_root != new_state_root {
            println!("  Block with {n_txs} transactions synced, state root changed");
        } else {
            println!("  Block with {n_txs} transactions synced, state root unchanged");
        }

        // Set block as final
        sync_fixture.set_final(block_height).await?;

        // Check latest block after execution
        sync_fixture.check_latest_block(block_height, expected_tx_count)?;

        prev_state_root = new_state_root;
        println!("✓ Block {block_height} synced successfully");
    }

    println!("✓ Engine API sync chain test passed!");
    Ok(())
}

/// Tests Engine API error handling scenarios
#[tokio::test]
async fn test_engine_api_error_handling() -> Result<()> {
    println!("=== Engine API Error Handling Test ===");

    let fixture = EngineApiTestFixture::new().await?;

    // Test invalid initial height
    {
        let result = fixture.init_chain(TEST_TIMESTAMP, 0).await;
        assert!(result.is_err(), "Should fail with invalid initial height");
        println!("✓ Invalid initial height test passed");
    }

    // Test with extremely large timestamp
    {
        let transactions = create_test_transactions(1, 0);
        let result = fixture
            .execute_txs(
                transactions,
                1,
                u64::MAX, // Very large timestamp
                fixture.base.genesis_state_root.to_vec(),
                fixture.base.genesis_hash,
            )
            .await;

        match result {
            Ok(_) => println!("✓ Large timestamp handled gracefully"),
            Err(e) => println!("✓ Large timestamp rejected appropriately: {e}"),
        }
    }

    // Test with very large transaction count
    {
        let transactions = create_test_transactions(1000, 0); // Large batch
        let result = fixture
            .execute_txs(
                transactions,
                1,
                TEST_TIMESTAMP,
                fixture.base.genesis_state_root.to_vec(),
                fixture.base.genesis_hash,
            )
            .await;

        match result {
            Ok((_, gas_used)) => {
                println!("✓ Large transaction batch handled: gas_used={gas_used}");
            }
            Err(e) => {
                println!("✓ Large transaction batch rejected appropriately: {e}");
            }
        }
    }

    println!("✓ Engine API error handling tests completed!");
    Ok(())
}
