---
description: This skill should be used when the user asks to "add a test", "write tests for", "understand the test setup", "run integration tests", "test ev-reth", "test the Engine API", or needs guidance on e2e test patterns, test fixtures, or EvolveTestFixture setup.
---

# Testing Infrastructure Onboarding

## Overview

Tests live in `crates/tests/` and include e2e and integration tests. The test suite uses `reth-e2e-test-utils` for realistic node testing.

## Key Files

### E2E Tests
- `crates/tests/src/e2e_tests.rs` - End-to-end tests (~1,060 lines)
- `crates/tests/src/test_evolve_engine_api.rs` - Engine API specific tests (~370 lines)

### Test Utilities
- `crates/tests/src/common.rs` - Test utilities and fixtures (~265 lines)
- `crates/tests/src/lib.rs` - Module exports

## Test Utilities (common.rs)

### Chain Spec Helpers

```rust
// Basic test chain spec
pub fn create_test_chain_spec() -> Arc<ChainSpec>

// With base fee redirect configured
pub fn create_test_chain_spec_with_base_fee_sink(
    base_fee_sink: Option<Address>
) -> Arc<ChainSpec>

// With mint precompile admin configured
pub fn create_test_chain_spec_with_mint_admin(
    mint_admin: Address
) -> Arc<ChainSpec>
```

### EvolveTestFixture

The main test fixture for payload builder tests. See `crates/tests/src/common.rs:89-103`:

```rust
pub struct EvolveTestFixture {
    pub builder: EvolvePayloadBuilder<MockEthProvider>,
    pub provider: MockEthProvider,
    pub genesis_hash: B256,
    pub genesis_state_root: B256,
    pub temp_dir: TempDir,
}

impl EvolveTestFixture {
    pub async fn new() -> Result<Self>
    pub fn setup_test_accounts(&self)
    pub fn add_mock_header(&self, hash: B256, number: u64, state_root: B256, timestamp: u64)
    pub fn create_payload_attributes(
        &self,
        transactions: Vec<TransactionSigned>,
        block_number: u64,
        timestamp: u64,
        parent_hash: B256,
        gas_limit: Option<u64>,
    ) -> EvolvePayloadAttributes
}
```

### Transaction Helpers

```rust
// Create multiple test transactions
pub fn create_test_transactions(count: usize, nonce_start: u64) -> Vec<TransactionSigned>

// Create a single test transaction
pub fn create_test_transaction(nonce: u64) -> TransactionSigned
```

## Test Patterns

### Basic Fixture Test

```rust
#[tokio::test]
async fn test_payload_with_transactions() {
    // Setup fixture
    let fixture = EvolveTestFixture::new().await.unwrap();

    // Create transactions
    let transactions = create_test_transactions(2, 0);

    // Create payload attributes
    let attrs = fixture.create_payload_attributes(
        transactions,
        1,                      // block_number
        TEST_TIMESTAMP + 12,    // timestamp
        fixture.genesis_hash,   // parent_hash
        None,                   // gas_limit
    );

    // Build payload using the builder
    let result = fixture.builder.build_payload(attrs).await;

    // Assert on result
    assert!(result.is_ok());
}
```

### Testing with Custom Chain Spec

```rust
#[tokio::test]
async fn test_base_fee_redirect() {
    let fee_sink = Address::random();
    let chain_spec = create_test_chain_spec_with_base_fee_sink(Some(fee_sink));

    // Build EVM config with redirect
    let evm_config = EthEvmConfig::new(chain_spec.clone());
    let config = EvolvePayloadBuilderConfig::from_chain_spec(chain_spec.as_ref()).unwrap();

    let base_fee_redirect = config
        .base_fee_redirect_settings()
        .map(|(sink, activation)| {
            BaseFeeRedirectSettings::new(BaseFeeRedirect::new(sink), activation)
        });

    let wrapped_evm = with_ev_handler(evm_config, base_fee_redirect, None, None);
    // ... continue with test
}
```

## Test Constants

Defined in `crates/tests/src/common.rs`:

```rust
pub const TEST_CHAIN_ID: u64 = 1234;
pub const TEST_TIMESTAMP: u64 = 1710338135;
pub const TEST_GAS_LIMIT: u64 = 30_000_000;
pub const TEST_BASE_FEE: u64 = 0;
```

## Development Commands

```bash
make test              # Run all tests
make test-verbose      # Run with output
make test-integration  # Integration tests only

# Run specific test
cargo test -p ev-reth-tests test_payload_with_transactions

# Run tests matching pattern
cargo test -p ev-reth-tests mint
```

## Key Design Decisions

1. **MockEthProvider** - Uses Reth's mock provider for unit testing
2. **Fixture Pattern** - `EvolveTestFixture` encapsulates common setup
3. **Chain Spec Variants** - Helpers for different config scenarios
4. **Test Transactions** - `create_test_transactions` uses `Signature::test_signature()`

## Exploration Starting Points

1. Start with `crates/tests/src/common.rs` for fixture and helpers
2. Read a simple test in `crates/tests/src/e2e_tests.rs`
3. Check how chain spec is configured for different test scenarios
4. See `EvolveTestFixture::new()` for the full setup flow

<!-- Last reviewed: 2026-02-20 -->
