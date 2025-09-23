//! Tests for Evolve consensus implementation

use evolve_ev_reth::consensus::EvolveConsensus;
use reth_chainspec::MAINNET;
use reth_consensus::{ConsensusError, HeaderValidator};
use reth_primitives::{Header, SealedHeader};

fn create_test_header(number: u64, parent_hash: [u8; 32], timestamp: u64) -> SealedHeader {
    let header = Header {
        number,
        parent_hash: parent_hash.into(),
        timestamp,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };

    SealedHeader::new(header, [0u8; 32].into())
}

#[test]
fn test_evolve_consensus_allows_same_timestamp() {
    let chain_spec = MAINNET.clone();
    let consensus = EvolveConsensus::new(chain_spec);

    // Create parent block
    let parent = create_test_header(1, [0u8; 32], 1000);

    // Create child block with SAME timestamp (this should be allowed)
    let child_header = reth_primitives::Header {
        number: 2,
        parent_hash: parent.hash(),
        timestamp: 1000,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };
    let child = SealedHeader::new(child_header, [1u8; 32].into());

    // This should NOT return an error for Evolve consensus
    let result = consensus.validate_header_against_parent(&child, &parent);
    if let Err(e) = &result {
        eprintln!("Error validating same timestamp: {e:?}");
    }
    assert!(
        result.is_ok(),
        "Evolve consensus should allow same timestamp"
    );
}

#[test]
fn test_evolve_consensus_rejects_past_timestamp() {
    let chain_spec = MAINNET.clone();
    let consensus = EvolveConsensus::new(chain_spec);

    // Create parent block
    let parent = create_test_header(1, [0u8; 32], 1000);

    // Create child block with timestamp in the past
    let child_header = Header {
        number: 2,
        parent_hash: parent.hash(),
        timestamp: 999,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };
    let child = SealedHeader::new(child_header, [1u8; 32].into());

    // This should return an error
    let result = consensus.validate_header_against_parent(&child, &parent);
    assert!(
        result.is_err(),
        "Evolve consensus should reject past timestamp"
    );

    match result {
        Err(ConsensusError::TimestampIsInPast {
            parent_timestamp,
            timestamp,
        }) => {
            assert_eq!(parent_timestamp, 1000);
            assert_eq!(timestamp, 999);
        }
        _ => panic!("Expected TimestampIsInPast error"),
    }
}

#[test]
fn test_evolve_consensus_allows_future_timestamp() {
    let chain_spec = MAINNET.clone();
    let consensus = EvolveConsensus::new(chain_spec);

    // Create parent block
    let parent = create_test_header(1, [0u8; 32], 1000);

    // Create child block with future timestamp
    let child_header = Header {
        number: 2,
        parent_hash: parent.hash(),
        timestamp: 1001,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };
    let child = SealedHeader::new(child_header, [1u8; 32].into());

    // This should be allowed
    let result = consensus.validate_header_against_parent(&child, &parent);
    if let Err(e) = &result {
        eprintln!("Error validating future timestamp: {e:?}");
    }
    assert!(
        result.is_ok(),
        "Evolve consensus should allow future timestamp"
    );
}

#[test]
fn test_evolve_consensus_validates_parent_hash() {
    let chain_spec = MAINNET.clone();
    let consensus = EvolveConsensus::new(chain_spec);

    // Create parent block
    let parent = create_test_header(1, [0u8; 32], 1000);

    // Create child block with wrong parent hash
    let child_header = Header {
        number: 2,
        parent_hash: [99u8; 32].into(),
        timestamp: 1000,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };
    let child = SealedHeader::new(child_header, [1u8; 32].into());

    // This should return an error
    let result = consensus.validate_header_against_parent(&child, &parent);
    assert!(
        result.is_err(),
        "Evolve consensus should validate parent hash"
    );
}

#[test]
fn test_evolve_consensus_validates_block_number() {
    let chain_spec = MAINNET.clone();
    let consensus = EvolveConsensus::new(chain_spec);

    // Create parent block
    let parent = create_test_header(1, [0u8; 32], 1000);

    // Create child block with wrong block number
    let child_header = Header {
        number: 3,
        parent_hash: parent.hash(),
        timestamp: 1000,
        gas_limit: 30_000_000,
        gas_used: 0,
        ..Default::default()
    };
    let child = SealedHeader::new(child_header, [1u8; 32].into());

    // This should return an error
    let result = consensus.validate_header_against_parent(&child, &parent);
    assert!(
        result.is_err(),
        "Evolve consensus should validate block number"
    );
}
