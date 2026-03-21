//! Contract-style documentation tests for the remote `ExEx` stream.
//!
//! This file is intentionally standalone so the main implementation can wire it
//! into `crates/tests/src/lib.rs` later without rewriting the checks.

#[test]
fn block_logger_example_mentions_finished_height() {
    let source = include_str!("../../../bin/ev-reth/examples/block_logger.rs");
    assert!(source.contains("ExExEvent::FinishedHeight"));
    assert!(source.contains("install_exex(\"block-logger\""));
    assert!(source.contains("committed_range"));
}

#[test]
fn remote_consumer_example_mentions_message_limits() {
    let source = include_str!("../../../bin/ev-reth/examples/remote_consumer.rs");
    assert!(source.contains("max_encoding_message_size(usize::MAX)"));
    assert!(source.contains("max_decoding_message_size(usize::MAX)"));
    assert!(source.contains("RemoteNotificationV1"));
    assert!(source.contains("NotificationEnvelope"));
    assert!(source.contains("sponsor_count"));
    assert!(source.contains("fee_payer"));
}

#[test]
fn readme_mentions_best_effort_streaming() {
    let readme = include_str!("../../../README.md");
    assert!(readme.contains("best-effort"));
    assert!(readme.contains("Remote ExEx"));
    assert!(readme.contains("Atlas"));
}
