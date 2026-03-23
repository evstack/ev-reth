use crate::{
    error::CodecError,
    proto::NotificationEnvelope,
    types::{RemoteNotificationV1, REMOTE_EXEX_SCHEMA_VERSION_V1},
};

/// Bincode encoding identifier stored in the transport envelope.
pub const REMOTE_EXEX_ENCODING_BINCODE: &str = "bincode/v2";
/// Schema version for the v1 remote notification payload.
pub const REMOTE_EXEX_SCHEMA_VERSION: u32 = REMOTE_EXEX_SCHEMA_VERSION_V1;

/// Bincode configuration for the wire format.
const BINCODE_CONFIG: bincode::config::Configuration = bincode::config::standard();

/// Returns the schema version used by the remote `ExEx` payloads.
pub const fn remote_notification_schema_version() -> u32 {
    REMOTE_EXEX_SCHEMA_VERSION
}

/// Encodes a remote notification into a bincode byte vector.
pub fn encode_remote_notification(
    notification: &RemoteNotificationV1,
) -> Result<Vec<u8>, CodecError> {
    Ok(bincode::serde::encode_to_vec(notification, BINCODE_CONFIG)?)
}

/// Decodes a remote notification from a bincode byte slice.
pub fn decode_remote_notification(bytes: &[u8]) -> Result<RemoteNotificationV1, CodecError> {
    let (value, _len) = bincode::serde::decode_from_slice(bytes, BINCODE_CONFIG)?;
    Ok(value)
}

/// Wraps a remote notification in the protobuf envelope.
pub fn encode_notification_envelope(
    notification: &RemoteNotificationV1,
) -> Result<NotificationEnvelope, CodecError> {
    Ok(NotificationEnvelope {
        schema_version: REMOTE_EXEX_SCHEMA_VERSION,
        encoding: REMOTE_EXEX_ENCODING_BINCODE.to_string(),
        payload: encode_remote_notification(notification)?,
    })
}

/// Decodes a protobuf envelope into a remote notification.
pub fn decode_notification_envelope(
    envelope: &NotificationEnvelope,
) -> Result<RemoteNotificationV1, CodecError> {
    if envelope.schema_version != REMOTE_EXEX_SCHEMA_VERSION {
        return Err(CodecError::InvalidEnvelope("unexpected schema version"));
    }
    if envelope.encoding != REMOTE_EXEX_ENCODING_BINCODE {
        return Err(CodecError::InvalidEnvelope("unexpected encoding"));
    }
    decode_remote_notification(&envelope.payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        RemoteBlockMetadataV1, RemoteBlockRangeV1, RemoteBlockV1, RemoteCallV1, RemoteLogV1,
        RemoteReceiptV1, RemoteTransactionV1,
    };
    use alloy_primitives::{Address, Bytes, B256, U256};

    fn sample_notification() -> RemoteNotificationV1 {
        let tx = RemoteTransactionV1::new(
            B256::repeat_byte(0x11),
            Address::repeat_byte(0x22),
            0x76,
            7,
            21_000,
            Some(1_000),
            1_500,
            Some(500),
            Some(Address::repeat_byte(0x33)),
            U256::from(99u64),
            Bytes::from_static(b"input"),
            Bytes::from_static(b"raw"),
            Some(Address::repeat_byte(0x44)),
            vec![RemoteCallV1 {
                to: Some(Address::repeat_byte(0x55)),
                value: U256::from(7u64),
                input: Bytes::from_static(b"call"),
            }],
        );
        let receipt = RemoteReceiptV1::new(
            B256::repeat_byte(0x11),
            true,
            21_000,
            21_000,
            None,
            vec![RemoteLogV1 {
                address: Address::repeat_byte(0x66),
                topics: vec![B256::repeat_byte(0x77)],
                data: Bytes::from_static(b"log"),
                log_index: 0,
                transaction_log_index: Some(0),
            }],
            Some(Address::repeat_byte(0x44)),
        );
        let block = RemoteBlockV1::new(
            RemoteBlockMetadataV1 {
                number: 42,
                hash: B256::repeat_byte(0x88),
                parent_hash: B256::repeat_byte(0x99),
                timestamp: 1_700_000_000,
                gas_limit: 30_000_000,
                gas_used: 21_000,
                fee_recipient: Address::repeat_byte(0xaa),
                base_fee_per_gas: Some(1),
            },
            vec![tx],
            vec![receipt],
        );

        RemoteNotificationV1::ChainCommitted {
            range: RemoteBlockRangeV1::new(42, 42),
            blocks: vec![block],
        }
    }

    #[test]
    fn notification_roundtrip() {
        let notification = sample_notification();
        let encoded = encode_remote_notification(&notification).expect("encode");
        let decoded = decode_remote_notification(&encoded).expect("decode");
        assert_eq!(notification, decoded);
    }

    #[test]
    fn envelope_roundtrip() {
        let notification = sample_notification();
        let envelope = encode_notification_envelope(&notification).expect("envelope");
        assert_eq!(envelope.schema_version, REMOTE_EXEX_SCHEMA_VERSION);
        assert_eq!(envelope.encoding, REMOTE_EXEX_ENCODING_BINCODE);

        let decoded = decode_notification_envelope(&envelope).expect("decode envelope");
        assert_eq!(notification, decoded);
    }

    #[test]
    fn reorg_and_revert_variants_roundtrip() {
        let block = match sample_notification() {
            RemoteNotificationV1::ChainCommitted { blocks, .. } => {
                blocks.into_iter().next().unwrap()
            }
            _ => unreachable!("sample notification should be committed"),
        };

        let reorg = RemoteNotificationV1::ChainReorged {
            reverted: RemoteBlockRangeV1::new(40, 40),
            reverted_blocks: vec![block.clone()],
            committed: RemoteBlockRangeV1::new(40, 40),
            committed_blocks: vec![block.clone()],
        };
        let revert = RemoteNotificationV1::ChainReverted {
            reverted: RemoteBlockRangeV1::new(39, 40),
            reverted_blocks: vec![block],
        };

        for notification in [reorg, revert] {
            let envelope = encode_notification_envelope(&notification).expect("encode");
            let decoded = decode_notification_envelope(&envelope).expect("decode");
            assert_eq!(notification, decoded);
        }
    }
}
