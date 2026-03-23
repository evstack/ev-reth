//! Minimal remote `ExEx` consumer example for ev-reth.
use ev_exex_remote::{
    decode_notification_envelope, wire::RemoteNotificationV1, RemoteExExClient, SubscribeRequest,
};
use tracing::info;

fn summarize(notification: &RemoteNotificationV1) -> (&'static str, usize, usize, usize) {
    match notification {
        RemoteNotificationV1::ChainCommitted { blocks, .. } => {
            let tx_count = blocks.iter().map(|block| block.transactions.len()).sum();
            let sponsor_count = blocks
                .iter()
                .flat_map(|block| &block.transactions)
                .filter(|tx| tx.fee_payer.is_some())
                .count();
            ("commit", blocks.len(), tx_count, sponsor_count)
        }
        RemoteNotificationV1::ChainReorged {
            reverted_blocks,
            committed_blocks,
            ..
        } => (
            "reorg",
            reverted_blocks.len() + committed_blocks.len(),
            reverted_blocks
                .iter()
                .map(|block| block.transactions.len())
                .sum::<usize>()
                + committed_blocks
                    .iter()
                    .map(|block| block.transactions.len())
                    .sum::<usize>(),
            reverted_blocks
                .iter()
                .flat_map(|block| &block.transactions)
                .filter(|tx| tx.fee_payer.is_some())
                .count()
                + committed_blocks
                    .iter()
                    .flat_map(|block| &block.transactions)
                    .filter(|tx| tx.fee_payer.is_some())
                    .count(),
        ),
        RemoteNotificationV1::ChainReverted {
            reverted_blocks, ..
        } => {
            let tx_count = reverted_blocks
                .iter()
                .map(|block| block.transactions.len())
                .sum();
            let sponsor_count = reverted_blocks
                .iter()
                .flat_map(|block| &block.transactions)
                .filter(|tx| tx.fee_payer.is_some())
                .count();
            ("revert", reverted_blocks.len(), tx_count, sponsor_count)
        }
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let endpoint = std::env::var("REMOTE_EXEX_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:10000".to_string());

    const MAX_GRPC_MESSAGE_SIZE: usize = 64 * 1024 * 1024;
    let mut client = RemoteExExClient::connect(endpoint)
        .await?
        .max_encoding_message_size(MAX_GRPC_MESSAGE_SIZE)
        .max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE);

    let mut stream = client.subscribe(SubscribeRequest {}).await?.into_inner();
    while let Some(message) = stream.message().await? {
        let notification = decode_notification_envelope(&message)?;
        let (kind, block_count, tx_count, sponsor_count) = summarize(&notification);

        info!(
            schema_version = message.schema_version,
            encoding = ?message.encoding,
            kind,
            block_count,
            tx_count,
            sponsor_count,
            "received remote ExEx notification"
        );
    }

    Ok(())
}
