//! Shared remote `ExEx` transport for ev-reth.
//!
//! This crate provides a minimal gRPC service definition plus serde-friendly
//! wire types for transporting canonical block execution events.
//!
//! The transport intentionally does not reuse ev-reth's internal execution types directly.
//! This is a wire contract for external consumers such as Atlas, not an in-process API.
//! Using dedicated remote types keeps the stream versionable and decoupled from ev-reth's
//! internal crate graph, serde layout, and exact Reth version.
//!
//! The remote schema is also narrower and consumer-oriented. It preserves explicit
//! commit/reorg/revert semantics and carries EV-specific derived fields that Atlas needs
//! directly, such as paired receipts/logs, recovered fee-payer metadata, batch-call data,
//! and raw EIP-2718 transaction bytes for fallback decoding.

mod codec;
mod error;
mod types;

/// Generated gRPC/protobuf bindings.
#[allow(
    missing_docs,
    clippy::derive_partial_eq_without_eq,
    clippy::doc_markdown,
    clippy::missing_const_for_fn
)]
pub mod proto {
    tonic::include_proto!("exex.remote.v1");
}

/// Stable user-facing re-exports for consumers of the wire contract.
pub mod wire {
    pub use crate::{
        proto::NotificationEnvelope,
        types::{
            RemoteBlockMetadataV1, RemoteBlockRangeV1, RemoteBlockV1, RemoteCallV1, RemoteLogV1,
            RemoteNotificationV1, RemoteReceiptV1, RemoteTransactionV1,
        },
    };
}

pub use codec::{
    decode_notification_envelope, decode_remote_notification, encode_notification_envelope,
    encode_remote_notification, remote_notification_schema_version,
    REMOTE_EXEX_ENCODING_BINCODE_V1, REMOTE_EXEX_SCHEMA_VERSION,
};
pub use error::{CodecError, DecodeError};
pub use proto::{
    remote_ex_ex_client::RemoteExExClient,
    remote_ex_ex_server::{RemoteExEx, RemoteExExServer},
    NotificationEnvelope, SubscribeRequest,
};
pub use types::{
    RemoteBlockMetadataV1, RemoteBlockRangeV1, RemoteBlockV1, RemoteCallV1, RemoteLogV1,
    RemoteNotificationV1, RemoteReceiptV1, RemoteTransactionV1,
};
