use alloy_consensus::{
    transaction::{SignerRecoverable, TxHashRef},
    Transaction as _, TxReceipt, Typed2718,
};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, TxKind};
use ev_exex_remote::{
    encode_notification_envelope,
    proto::{remote_ex_ex_server::RemoteExEx, NotificationEnvelope, SubscribeRequest},
    wire::{
        RemoteBlockMetadataV1, RemoteBlockRangeV1, RemoteBlockV1, RemoteCallV1, RemoteLogV1,
        RemoteNotificationV1, RemoteReceiptV1, RemoteTransactionV1,
    },
    RemoteExExServer,
};
use ev_primitives::{Call, EvPrimitives, EvTxEnvelope};
use eyre::Result;
use futures::TryStreamExt;
use reth_execution_types::Chain;
use reth_exex::{ExExContext, ExExEvent, ExExNotification};
use reth_node_api::{FullNodeComponents, NodeTypes};
use reth_tasks::TaskExecutor;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, info};

/// Stable identifier used when installing the built-in Atlas-style remote `ExEx`.
pub const REMOTE_EXEX_ID: &str = "atlas-remote-exex";

/// Shared best-effort notification fan-out for connected remote `ExEx` subscribers.
pub type RemoteNotificationSender = Arc<broadcast::Sender<NotificationEnvelope>>;

/// Runtime configuration for the built-in remote `ExEx` bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteExExConfig {
    /// Socket address where the gRPC service should listen.
    pub grpc_listen_addr: SocketAddr,
    /// Bounded broadcast capacity shared by live subscribers.
    pub buffer: usize,
}

impl RemoteExExConfig {
    /// Creates a new remote `ExEx` configuration.
    pub const fn new(grpc_listen_addr: SocketAddr, buffer: usize) -> Self {
        Self {
            grpc_listen_addr,
            buffer,
        }
    }
}

#[derive(Debug)]
struct RemoteExExService {
    notifications: RemoteNotificationSender,
}

#[tonic::async_trait]
impl RemoteExEx for RemoteExExService {
    type SubscribeStream = ReceiverStream<Result<NotificationEnvelope, Status>>;

    async fn subscribe(
        &self,
        _request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let mut notifications = self.notifications.subscribe();
        let (tx, rx) = mpsc::channel(1);

        tokio::spawn(async move {
            loop {
                match notifications.recv().await {
                    Ok(notification) => {
                        if tx.send(Ok(notification)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        let status = Status::resource_exhausted(format!(
                            "remote exex subscriber lagged by {skipped} messages"
                        ));
                        let _ = tx.send(Err(status)).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Spawns the best-effort gRPC server that streams remote `ExEx` notifications.
pub fn spawn_remote_exex_grpc_server(
    task_executor: &TaskExecutor,
    config: RemoteExExConfig,
    notifications: RemoteNotificationSender,
) {
    let listen_addr = config.grpc_listen_addr;
    task_executor.spawn_critical_task(
        "remote exex grpc server",
        Box::pin(async move {
            if let Err(err) = serve_remote_exex_grpc_server(listen_addr, notifications).await {
                tracing::error!(%listen_addr, error = ?err, "remote exex gRPC server exited");
            }
        }),
    );
}

async fn serve_remote_exex_grpc_server(
    listen_addr: SocketAddr,
    notifications: RemoteNotificationSender,
) -> Result<()> {
    info!(%listen_addr, "starting remote exex gRPC server");

    Server::builder()
        .add_service(RemoteExExServer::new(RemoteExExService { notifications }))
        .serve(listen_addr)
        .await?;

    Ok(())
}

/// Forwards `ExEx` notifications into the bounded remote subscriber broadcast channel.
pub async fn remote_exex_task<Node>(
    mut ctx: ExExContext<Node>,
    notifications: RemoteNotificationSender,
) -> Result<()>
where
    Node: FullNodeComponents<Types: NodeTypes<Primitives = EvPrimitives>>,
{
    while let Some(notification) = ctx.notifications.try_next().await? {
        let (remote_notification, finished_height) = match &notification {
            ExExNotification::ChainCommitted { new } => (
                RemoteNotificationV1::ChainCommitted {
                    range: chain_range(new),
                    blocks: chain_blocks(new)?,
                },
                Some(new.tip().num_hash()),
            ),
            ExExNotification::ChainReorged { old, new } => (
                RemoteNotificationV1::ChainReorged {
                    reverted: chain_range(old),
                    reverted_blocks: chain_blocks(old)?,
                    committed: chain_range(new),
                    committed_blocks: chain_blocks(new)?,
                },
                Some(new.tip().num_hash()),
            ),
            ExExNotification::ChainReverted { old } => (
                RemoteNotificationV1::ChainReverted {
                    reverted: chain_range(old),
                    reverted_blocks: chain_blocks(old)?,
                },
                None,
            ),
        };

        let envelope = encode_notification_envelope(&remote_notification)?;
        match notifications.send(envelope) {
            Ok(receivers) => {
                debug!(receivers, "queued remote exex notification");
            }
            Err(_) => {
                debug!("remote exex notification dropped because no subscribers are connected");
            }
        }

        if let Some(finished_height) = finished_height {
            ctx.events
                .send(ExExEvent::FinishedHeight(finished_height))?;
        }
    }

    Ok(())
}

fn chain_range(chain: &Chain<EvPrimitives>) -> RemoteBlockRangeV1 {
    let range = chain.range();
    RemoteBlockRangeV1::new(*range.start(), *range.end())
}

fn chain_blocks(chain: &Chain<EvPrimitives>) -> Result<Vec<RemoteBlockV1>> {
    chain
        .blocks_and_receipts()
        .map(remote_block)
        .collect::<Result<Vec<_>>>()
}

fn remote_block(
    (block, receipts): (
        &reth_primitives_traits::RecoveredBlock<ev_primitives::Block>,
        &Vec<ev_primitives::Receipt>,
    ),
) -> Result<RemoteBlockV1> {
    let metadata = RemoteBlockMetadataV1 {
        number: block.header().number,
        hash: block.hash(),
        parent_hash: block.header().parent_hash,
        timestamp: block.header().timestamp,
        gas_limit: block.header().gas_limit,
        gas_used: block.header().gas_used,
        fee_recipient: block.header().beneficiary,
        base_fee_per_gas: block.header().base_fee_per_gas.map(u128::from),
    };

    let txs = block.body().transactions.as_slice();
    if txs.len() != receipts.len() {
        eyre::bail!(
            "transaction/receipt mismatch for block {}: {} txs vs {} receipts",
            metadata.number,
            txs.len(),
            receipts.len()
        );
    }

    let mut block_log_index = 0u64;
    let mut previous_cumulative_gas_used = 0u64;
    let mut remote_txs = Vec::with_capacity(txs.len());
    let mut remote_receipts = Vec::with_capacity(receipts.len());

    for (tx, receipt) in txs.iter().zip(receipts.iter()) {
        let sender = tx.recover_signer().map_err(|err| {
            eyre::eyre!("failed to recover signer for tx {:?}: {err}", tx.tx_hash())
        })?;
        let fee_payer = fee_payer(tx, sender);

        remote_txs.push(remote_transaction(tx, sender, fee_payer));
        remote_receipts.push(remote_receipt(
            tx,
            receipt,
            fee_payer,
            &mut block_log_index,
            &mut previous_cumulative_gas_used,
        ));
    }

    Ok(RemoteBlockV1::new(metadata, remote_txs, remote_receipts))
}

fn remote_transaction(
    tx: &EvTxEnvelope,
    sender: Address,
    fee_payer: Option<Address>,
) -> RemoteTransactionV1 {
    RemoteTransactionV1::new(
        *tx.tx_hash(),
        sender,
        tx.ty(),
        tx.nonce(),
        tx.gas_limit(),
        tx.gas_price(),
        tx.max_fee_per_gas(),
        tx.max_priority_fee_per_gas(),
        match tx.kind() {
            TxKind::Call(to) => Some(to),
            TxKind::Create => None,
        },
        tx.value(),
        tx.input().clone(),
        tx.encoded_2718().into(),
        fee_payer,
        batch_calls(tx),
    )
}

fn remote_receipt(
    tx: &EvTxEnvelope,
    receipt: &ev_primitives::Receipt,
    fee_payer: Option<Address>,
    block_log_index: &mut u64,
    previous_cumulative_gas_used: &mut u64,
) -> RemoteReceiptV1 {
    let mut tx_log_index = 0u64;
    let logs = receipt
        .logs()
        .iter()
        .map(|log| {
            let remote_log = RemoteLogV1 {
                address: log.address,
                topics: log.data.topics().to_vec(),
                data: log.data.data.clone(),
                log_index: *block_log_index,
                transaction_log_index: Some(tx_log_index),
            };
            *block_log_index += 1;
            tx_log_index += 1;
            remote_log
        })
        .collect();
    let gas_used = receipt
        .cumulative_gas_used
        .saturating_sub(*previous_cumulative_gas_used);
    *previous_cumulative_gas_used = receipt.cumulative_gas_used;

    RemoteReceiptV1::new(
        *tx.tx_hash(),
        receipt.status(),
        gas_used,
        receipt.cumulative_gas_used,
        None,
        logs,
        fee_payer,
    )
}

fn fee_payer(tx: &EvTxEnvelope, sender: Address) -> Option<Address> {
    match tx {
        EvTxEnvelope::EvNode(ev) => ev
            .tx()
            .fee_payer_signature
            .as_ref()
            .and_then(|signature| ev.tx().recover_sponsor(sender, signature).ok()),
        EvTxEnvelope::Ethereum(_) => None,
    }
}

fn batch_calls(tx: &EvTxEnvelope) -> Vec<RemoteCallV1> {
    match tx {
        EvTxEnvelope::EvNode(ev) => ev.tx().calls.iter().map(remote_call).collect(),
        EvTxEnvelope::Ethereum(_) => Vec::new(),
    }
}

fn remote_call(call: &Call) -> RemoteCallV1 {
    RemoteCallV1 {
        to: match call.to {
            TxKind::Call(address) => Some(address),
            TxKind::Create => None,
        },
        value: call.value,
        input: call.input.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_consensus::{SignableTransaction, TxLegacy};
    use alloy_eips::eip2930::AccessList;
    use alloy_primitives::{Address, Bytes, Signature, B256, U256};
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use ev_exex_remote::{
        decode_notification_envelope, encode_notification_envelope, RemoteExExClient,
    };
    use ev_primitives::{Call, EvNodeTransaction};
    use reth_primitives::Transaction;
    use tokio::{
        sync::broadcast,
        time::{sleep, timeout, Duration},
    };
    use tokio_stream::StreamExt;

    #[test]
    fn remote_exex_config_is_constructible() {
        let config = RemoteExExConfig::new("127.0.0.1:30001".parse().unwrap(), 32);
        assert_eq!(config.grpc_listen_addr, "127.0.0.1:30001".parse().unwrap());
        assert_eq!(config.buffer, 32);
    }

    #[test]
    fn fee_payer_recovers_only_for_evnode_transactions() {
        let signed = alloy_consensus::Signed::new_unhashed(
            Transaction::Legacy(TxLegacy {
                chain_id: Some(1234),
                nonce: 0,
                gas_price: 1,
                gas_limit: 21_000,
                to: TxKind::Create,
                value: U256::ZERO,
                input: Bytes::default(),
            }),
            Signature::test_signature(),
        );
        let tx = EvTxEnvelope::Ethereum(reth_ethereum_primitives::TransactionSigned::from(signed));
        assert_eq!(fee_payer(&tx, Address::ZERO), None);
    }

    #[test]
    fn remote_transaction_captures_sponsored_evnode_metadata() {
        let executor = PrivateKeySigner::from_slice(&[7u8; 32]).expect("executor key");
        let sponsor = PrivateKeySigner::from_slice(&[9u8; 32]).expect("sponsor key");
        let executor_address = executor.address();
        let sponsor_address = sponsor.address();

        let tx = EvNodeTransaction {
            chain_id: 1234,
            nonce: 3,
            max_priority_fee_per_gas: 1_000_000_000,
            max_fee_per_gas: 2_000_000_000,
            gas_limit: 100_000,
            calls: vec![Call {
                to: TxKind::Call(Address::repeat_byte(0x44)),
                value: U256::from(123u64),
                input: Bytes::from_static(b"call"),
            }],
            access_list: AccessList::default(),
            fee_payer_signature: None,
        };

        let executor_sig = executor
            .sign_hash_sync(&tx.signature_hash())
            .expect("executor signature");
        let mut signed = tx.into_signed(executor_sig);
        let sponsor_sig = sponsor
            .sign_hash_sync(&signed.tx().sponsor_signing_hash(executor_address))
            .expect("sponsor signature");
        signed.tx_mut().fee_payer_signature = Some(sponsor_sig);

        let envelope = EvTxEnvelope::EvNode(signed);
        let remote = remote_transaction(
            &envelope,
            executor_address,
            fee_payer(&envelope, executor_address),
        );

        assert_eq!(remote.sender, executor_address);
        assert_eq!(remote.fee_payer, Some(sponsor_address));
        assert_eq!(remote.calls.len(), 1);
        assert_eq!(remote.calls[0].to, Some(Address::repeat_byte(0x44)));
    }

    #[tokio::test]
    async fn lagging_subscriber_receives_resource_exhausted() {
        let notifications = Arc::new(broadcast::channel(1).0);
        let service = RemoteExExService {
            notifications: notifications.clone(),
        };

        let response = service
            .subscribe(Request::new(SubscribeRequest {}))
            .await
            .expect("subscribe");
        let mut stream = response.into_inner();

        notifications
            .send(NotificationEnvelope {
                schema_version: 1,
                encoding: "bincode/v1".to_string(),
                payload: vec![1],
            })
            .expect("first notification");
        tokio::task::yield_now().await;

        notifications
            .send(NotificationEnvelope {
                schema_version: 1,
                encoding: "bincode/v1".to_string(),
                payload: vec![2],
            })
            .expect("second notification");
        tokio::task::yield_now().await;

        notifications
            .send(NotificationEnvelope {
                schema_version: 1,
                encoding: "bincode/v1".to_string(),
                payload: vec![3],
            })
            .expect("third notification");
        notifications
            .send(NotificationEnvelope {
                schema_version: 1,
                encoding: "bincode/v1".to_string(),
                payload: vec![4],
            })
            .expect("fourth notification");

        let first = stream
            .next()
            .await
            .expect("stream item")
            .expect("first notification should arrive");
        assert_eq!(first.payload, vec![1]);

        let second = stream
            .next()
            .await
            .expect("stream item")
            .expect("second notification should arrive");
        assert_eq!(second.payload, vec![2]);

        let lagged = stream
            .next()
            .await
            .expect("stream item")
            .expect_err("lagged error");
        assert_eq!(lagged.code(), tonic::Code::ResourceExhausted);
    }

    #[tokio::test]
    async fn grpc_server_streams_notifications_to_subscribers() {
        let notifications = Arc::new(broadcast::channel(4).0);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let listen_addr = listener.local_addr().expect("listener addr");
        drop(listener);

        let server = tokio::spawn(serve_remote_exex_grpc_server(
            listen_addr,
            notifications.clone(),
        ));

        let endpoint = format!("http://{listen_addr}");
        let mut client = loop {
            match RemoteExExClient::connect(endpoint.clone()).await {
                Ok(client) => break client,
                Err(_) => sleep(Duration::from_millis(25)).await,
            }
        };

        let mut stream = client
            .subscribe(SubscribeRequest {})
            .await
            .expect("subscribe")
            .into_inner();

        let notification = RemoteNotificationV1::ChainCommitted {
            range: RemoteBlockRangeV1::new(7, 7),
            blocks: vec![RemoteBlockV1::new(
                RemoteBlockMetadataV1 {
                    number: 7,
                    hash: B256::repeat_byte(7),
                    parent_hash: B256::repeat_byte(6),
                    timestamp: 7,
                    gas_limit: 30_000_000,
                    gas_used: 21_000,
                    fee_recipient: Address::repeat_byte(8),
                    base_fee_per_gas: Some(1),
                },
                vec![],
                vec![],
            )],
        };

        notifications
            .send(encode_notification_envelope(&notification).expect("encode envelope"))
            .expect("queued notification");

        let received = timeout(Duration::from_secs(5), stream.message())
            .await
            .expect("notification timeout")
            .expect("stream should remain healthy")
            .expect("notification payload");

        let decoded = decode_notification_envelope(&received).expect("decode envelope");
        assert_eq!(decoded, notification);

        server.abort();
        let _ = server.await;
    }

    #[test]
    fn encoded_envelope_roundtrips() {
        let notification = RemoteNotificationV1::ChainCommitted {
            range: RemoteBlockRangeV1::new(1, 1),
            blocks: vec![RemoteBlockV1::new(
                RemoteBlockMetadataV1 {
                    number: 1,
                    hash: B256::repeat_byte(1),
                    parent_hash: B256::ZERO,
                    timestamp: 1,
                    gas_limit: 30_000_000,
                    gas_used: 21_000,
                    fee_recipient: Address::repeat_byte(2),
                    base_fee_per_gas: Some(1),
                },
                vec![RemoteTransactionV1::new(
                    B256::repeat_byte(3),
                    Address::repeat_byte(4),
                    0x76,
                    0,
                    21_000,
                    None,
                    1,
                    Some(1),
                    None,
                    U256::ZERO,
                    Bytes::default(),
                    Bytes::default(),
                    None,
                    vec![],
                )],
                vec![RemoteReceiptV1::new(
                    B256::repeat_byte(3),
                    true,
                    21_000,
                    21_000,
                    None,
                    vec![],
                    None,
                )],
            )],
        };

        let envelope = encode_notification_envelope(&notification).expect("encode envelope");
        let decoded = decode_notification_envelope(&envelope).expect("decode envelope");
        assert_eq!(decoded, notification);
    }
}
