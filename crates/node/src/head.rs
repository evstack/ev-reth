//! Push-based forkchoice sharing between ev-reth peers.
//!
//! Peers exchange only chain identity and forkchoice references over WebSocket. Missing block
//! headers and bodies are fetched by Reth's native P2P downloader after the local engine receives
//! the pushed forkchoice state.

use crate::EvolveEngineTypes;
use alloy_eips::BlockNumHash;
use alloy_primitives::B256;
use alloy_rpc_types_engine::{ForkchoiceState, PayloadStatusEnum};
use async_trait::async_trait;
use ev_primitives::EvPrimitives;
use futures::{Stream, StreamExt};
use jsonrpsee::{
    core::SubscriptionResult, proc_macros::rpc, ws_client::WsClientBuilder,
    PendingSubscriptionSink, SubscriptionMessage,
};
use reth_engine_primitives::{ConsensusEngineEvent, ConsensusEngineHandle};
use reth_storage_api::{BlockIdReader, BlockNumReader};
use reth_tasks::TaskExecutor;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{
    sync::watch,
    time::{interval, sleep, timeout, MissedTickBehavior},
};
use tracing::{debug, error, info, warn};
use url::Url;

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_INTERVAL: Duration = Duration::from_secs(3);
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
const SUBSCRIPTION_BUFFER: usize = 16;

/// A complete, self-identifying forkchoice update pushed by an ev-reth peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadUpdate {
    /// Chain ID of the publishing peer.
    pub chain_id: u64,
    /// Genesis hash of the publishing peer.
    pub genesis_hash: B256,
    /// Forkchoice state accepted by the publishing peer.
    pub forkchoice_state: ForkchoiceState,
    /// Block number corresponding to `head_block_hash`.
    pub head_block_number: u64,
    /// Block number corresponding to `safe_block_hash`.
    pub safe_block_number: u64,
    /// Block number corresponding to `finalized_block_hash`.
    pub finalized_block_number: u64,
}

/// Push-based ev-reth head sharing API.
#[rpc(server, client, namespace = "ev")]
pub trait HeadApi {
    /// Subscribes to the latest valid forkchoice state accepted by this node.
    #[subscription(
        name = "subscribeForkchoice",
        unsubscribe = "unsubscribeForkchoice",
        item = HeadUpdate
    )]
    fn subscribe_forkchoice(&self) -> SubscriptionResult;
}

/// Latest-value publisher backing `ev_subscribeForkchoice`.
///
/// A watch channel intentionally retains only one update. Slow subscribers can reconnect and
/// receive the newest valid forkchoice without accumulating an unbounded history.
#[derive(Clone, Debug)]
pub struct HeadPublisher {
    chain_id: u64,
    genesis_hash: B256,
    latest: watch::Sender<Option<HeadUpdate>>,
}

impl HeadPublisher {
    /// Creates a publisher for one chain.
    pub fn new(chain_id: u64, genesis_hash: B256) -> Self {
        let (latest, receiver) = watch::channel(None);
        drop(receiver);
        Self {
            chain_id,
            genesis_hash,
            latest,
        }
    }

    /// Publishes a valid forkchoice state with its corresponding block numbers.
    ///
    /// Returns `true` when the latest value changed.
    pub fn publish(
        &self,
        forkchoice_state: ForkchoiceState,
        head_block_number: u64,
        safe_block_number: u64,
        finalized_block_number: u64,
    ) -> bool {
        let update = HeadUpdate {
            chain_id: self.chain_id,
            genesis_hash: self.genesis_hash,
            forkchoice_state,
            head_block_number,
            safe_block_number,
            finalized_block_number,
        };
        self.latest.send_if_modified(|current| {
            if *current == Some(update) {
                false
            } else {
                *current = Some(update);
                true
            }
        })
    }

    /// Number of active subscription receivers.
    pub fn subscriber_count(&self) -> usize {
        self.latest.receiver_count()
    }

    fn publish_state<P>(&self, provider: &P, state: ForkchoiceState) -> Result<bool, String>
    where
        P: BlockNumReader,
    {
        let head = block_number(provider, state.head_block_hash, "head")?;
        let safe = block_number(provider, state.safe_block_hash, "safe")?;
        let finalized = block_number(provider, state.finalized_block_hash, "finalized")?;
        Ok(self.publish(state, head, safe, finalized))
    }

    fn publish_provider_snapshot<P>(&self, provider: &P) -> Result<bool, String>
    where
        P: BlockNumReader + BlockIdReader,
    {
        let chain_info = provider.chain_info().map_err(|error| error.to_string())?;
        let safe = provider
            .safe_block_num_hash()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "local safe block is unavailable".to_string())?;
        let finalized = provider
            .finalized_block_num_hash()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "local finalized block is unavailable".to_string())?;
        let head = BlockNumHash::new(chain_info.best_number, chain_info.best_hash);
        Ok(self.publish(
            ForkchoiceState {
                head_block_hash: head.hash,
                safe_block_hash: safe.hash,
                finalized_block_hash: finalized.hash,
            },
            head.number,
            safe.number,
            finalized.number,
        ))
    }
}

impl HeadApiServer for HeadPublisher {
    fn subscribe_forkchoice(
        &self,
        pending_subscription: PendingSubscriptionSink,
    ) -> SubscriptionResult {
        let mut updates = self.latest.subscribe();
        tokio::spawn(async move {
            let sink = match pending_subscription.accept().await {
                Ok(sink) => sink,
                Err(error) => {
                    warn!(%error, "failed to accept forkchoice subscription");
                    return;
                }
            };

            loop {
                let update = *updates.borrow_and_update();
                if let Some(update) = update {
                    let message = match serde_json::value::to_raw_value(&update) {
                        Ok(value) => SubscriptionMessage::from(value),
                        Err(error) => {
                            error!(%error, "failed to serialize forkchoice update");
                            return;
                        }
                    };
                    if sink.send(message).await.is_err() {
                        return;
                    }
                }

                tokio::select! {
                    _ = sink.closed() => return,
                    changed = updates.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Ok(())
    }
}

/// Publishes valid forkchoice events from the local Reth engine.
pub fn spawn_publisher<P, S>(
    executor: TaskExecutor,
    provider: P,
    mut engine_events: S,
    publisher: HeadPublisher,
) where
    P: BlockNumReader + BlockIdReader + Send + Sync + 'static,
    S: Stream<Item = ConsensusEngineEvent<EvPrimitives>> + Send + Unpin + 'static,
{
    executor.spawn_with_graceful_shutdown_signal(move |shutdown| async move {
        if let Err(error) = publisher.publish_provider_snapshot(&provider) {
            debug!(%error, "valid forkchoice snapshot is not available yet");
        }

        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => {
                    info!("forkchoice publisher stopped during node shutdown");
                    return;
                }
                event = engine_events.next() => {
                    let Some(event) = event else {
                        warn!("local engine event stream ended; forkchoice publisher stopped");
                        return;
                    };
                    if let ConsensusEngineEvent::ForkchoiceUpdated(state, status) = event {
                        if status.is_valid() {
                            match publisher.publish_state(&provider, state) {
                                Ok(true) => {
                                    debug!(head = ?state.head_block_hash, "published valid forkchoice");
                                }
                                Ok(false) => {}
                                Err(error) => {
                                    warn!(%error, ?state, "could not resolve valid forkchoice locally");
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

/// Starts the non-critical, shutdown-aware peer subscriber.
///
/// An unavailable peer is deliberately not node-fatal: the local node continues to serve its
/// existing chain and P2P connections while this task reconnects with bounded backoff.
pub fn spawn_subscriber(
    executor: TaskExecutor,
    engine: ConsensusEngineHandle<EvolveEngineTypes>,
    peer_url: Url,
    expected_chain_id: u64,
    expected_genesis_hash: B256,
) {
    spawn_with_sink(
        executor,
        InProcessForkchoiceSink(engine),
        peer_url,
        expected_chain_id,
        expected_genesis_hash,
    );
}

/// Starts a peer subscriber with a caller-provided local forkchoice sink.
pub fn spawn_with_sink<S>(
    executor: TaskExecutor,
    sink: S,
    peer_url: Url,
    expected_chain_id: u64,
    expected_genesis_hash: B256,
) where
    S: ForkchoiceSink + 'static,
{
    executor.spawn_with_graceful_shutdown_signal(move |shutdown| async move {
        tokio::select! {
            _ = shutdown => info!("peer head subscriber stopped during node shutdown"),
            () = subscribe(sink, peer_url, expected_chain_id, expected_genesis_hash) => {}
        }
    });
}

/// Runs the peer subscription until its endpoint violates the trust policy or the future is
/// cancelled by its owner.
pub async fn subscribe<S: ForkchoiceSink>(
    sink: S,
    peer_url: Url,
    expected_chain_id: u64,
    expected_genesis_hash: B256,
) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    let mut tracker = ForkchoiceTracker::default();

    loop {
        let session_started = Instant::now();
        match run_session(
            &sink,
            &peer_url,
            expected_chain_id,
            expected_genesis_hash,
            &mut tracker,
        )
        .await
        {
            Ok(()) => {
                warn!(endpoint = %peer_url, "peer forkchoice subscription ended; reconnecting");
            }
            Err(
                SubscriptionError::IncompatibleChain(message)
                | SubscriptionError::MalformedFinality(message),
            ) => {
                error!(endpoint = %peer_url, %message, "peer violated head subscription trust policy; stopping subscriber");
                return;
            }
            Err(error) => {
                warn!(endpoint = %peer_url, %error, "peer head subscription failed; reconnecting");
            }
        }

        if session_started.elapsed() >= MAX_RECONNECT_DELAY {
            reconnect_delay = INITIAL_RECONNECT_DELAY;
        }
        sleep(reconnect_delay).await;
        reconnect_delay = reconnect_delay.saturating_mul(2).min(MAX_RECONNECT_DELAY);
    }
}

async fn run_session<S: ForkchoiceSink>(
    sink: &S,
    peer_url: &Url,
    expected_chain_id: u64,
    expected_genesis_hash: B256,
    tracker: &mut ForkchoiceTracker,
) -> Result<(), SubscriptionError> {
    let client = timeout(
        RPC_TIMEOUT,
        WsClientBuilder::default()
            .request_timeout(RPC_TIMEOUT)
            .max_concurrent_requests(4)
            .max_buffer_capacity_per_subscription(SUBSCRIPTION_BUFFER)
            .build(peer_url.as_str()),
    )
    .await
    .map_err(|_| SubscriptionError::Transient("WebSocket connection timed out".into()))?
    .map_err(|error| SubscriptionError::Transient(error.to_string()))?;

    let mut updates = timeout(RPC_TIMEOUT, HeadApiClient::subscribe_forkchoice(&client))
        .await
        .map_err(|_| SubscriptionError::Transient("forkchoice subscription timed out".into()))?
        .map_err(|error| SubscriptionError::Transient(error.to_string()))?;
    info!(endpoint = %peer_url, "subscribed to peer forkchoice updates");

    let mut retry = interval(RETRY_INTERVAL);
    retry.set_missed_tick_behavior(MissedTickBehavior::Skip);
    retry.tick().await;

    loop {
        tokio::select! {
            update = updates.next() => {
                let update = update
                    .ok_or_else(|| SubscriptionError::Transient("forkchoice subscription ended".into()))?
                    .map_err(|error| SubscriptionError::Transient(error.to_string()))?;
                process_update(
                    sink,
                    tracker,
                    update,
                    expected_chain_id,
                    expected_genesis_hash,
                )
                .await?;
            }
            _ = retry.tick() => {
                apply_pending(sink, tracker).await?;
            }
        }
    }
}

async fn process_update<S: ForkchoiceSink>(
    sink: &S,
    tracker: &mut ForkchoiceTracker,
    update: HeadUpdate,
    expected_chain_id: u64,
    expected_genesis_hash: B256,
) -> Result<(), SubscriptionError> {
    if update.chain_id != expected_chain_id {
        return Err(SubscriptionError::IncompatibleChain(format!(
            "chain ID is {}, expected {expected_chain_id}",
            update.chain_id
        )));
    }
    if update.genesis_hash != expected_genesis_hash {
        return Err(SubscriptionError::IncompatibleChain(format!(
            "genesis hash is {}, expected {expected_genesis_hash}",
            update.genesis_hash
        )));
    }

    let state = update.forkchoice_state;
    let head = RemoteBlock {
        hash: state.head_block_hash,
        number: update.head_block_number,
    };
    let safe = RemoteBlock {
        hash: state.safe_block_hash,
        number: update.safe_block_number,
    };
    let finalized = RemoteBlock {
        hash: state.finalized_block_hash,
        number: update.finalized_block_number,
    };
    validate_forkchoice(head, safe, finalized, tracker.last_finality)?;

    let resolved = ResolvedForkchoice {
        state,
        finality: Finality { safe, finalized },
    };
    if tracker.should_skip(resolved) {
        return Ok(());
    }
    tracker.queue(resolved);
    apply_pending(sink, tracker).await
}

async fn apply_pending<S: ForkchoiceSink>(
    sink: &S,
    tracker: &mut ForkchoiceTracker,
) -> Result<(), SubscriptionError> {
    let Some(pending) = tracker.pending else {
        return Ok(());
    };

    let status = sink
        .apply_forkchoice(pending.state)
        .await
        .map_err(SubscriptionError::Transient)?;
    match &status {
        PayloadStatusEnum::Valid => {
            tracker.accept(pending);
            debug!(head = ?pending.state.head_block_hash, "applied peer forkchoice to local engine");
        }
        PayloadStatusEnum::Syncing | PayloadStatusEnum::Accepted => {
            debug!(head = ?pending.state.head_block_hash, ?status, "local engine is fetching peer target through P2P");
        }
        PayloadStatusEnum::Invalid { validation_error } => {
            error!(head = ?pending.state.head_block_hash, %validation_error, "local engine rejected peer forkchoice; waiting for a changed state");
            tracker.reject(pending.state);
        }
    }
    Ok(())
}

fn block_number<P: BlockNumReader>(provider: &P, hash: B256, label: &str) -> Result<u64, String> {
    if hash.is_zero() {
        return Err(format!("{label} block hash is zero"));
    }
    provider
        .block_number(hash)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("{label} block {hash} is unavailable locally"))
}

fn validate_forkchoice(
    head: RemoteBlock,
    safe: RemoteBlock,
    finalized: RemoteBlock,
    previous: Option<Finality>,
) -> Result<(), SubscriptionError> {
    if finalized.number > safe.number || safe.number > head.number {
        return Err(SubscriptionError::MalformedFinality(format!(
            "finalized={}, safe={}, head={}",
            finalized.number, safe.number, head.number
        )));
    }
    if (finalized.number == safe.number && finalized.hash != safe.hash)
        || (safe.number == head.number && safe.hash != head.hash)
    {
        return Err(SubscriptionError::MalformedFinality(
            "equal-height canonical references have different hashes".into(),
        ));
    }
    if let Some(previous) = previous {
        reject_regression("safe", safe, previous.safe)?;
        reject_regression("finalized", finalized, previous.finalized)?;
    }
    Ok(())
}

fn reject_regression(
    name: &str,
    current: RemoteBlock,
    previous: RemoteBlock,
) -> Result<(), SubscriptionError> {
    if current.number < previous.number
        || (current.number == previous.number && current.hash != previous.hash)
    {
        return Err(SubscriptionError::MalformedFinality(format!(
            "{name} regressed from {} ({}) to {} ({})",
            previous.number, previous.hash, current.number, current.hash
        )));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RemoteBlock {
    hash: B256,
    number: u64,
}

#[derive(Clone, Copy, Debug)]
struct Finality {
    safe: RemoteBlock,
    finalized: RemoteBlock,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedForkchoice {
    state: ForkchoiceState,
    finality: Finality,
}

#[derive(Default)]
struct ForkchoiceTracker {
    accepted: Option<ForkchoiceState>,
    pending: Option<ResolvedForkchoice>,
    rejected: Option<ForkchoiceState>,
    last_finality: Option<Finality>,
}

impl ForkchoiceTracker {
    fn should_skip(&self, resolved: ResolvedForkchoice) -> bool {
        self.accepted == Some(resolved.state) || self.rejected == Some(resolved.state)
    }

    const fn queue(&mut self, resolved: ResolvedForkchoice) {
        self.rejected = None;
        self.pending = Some(resolved);
    }

    const fn accept(&mut self, resolved: ResolvedForkchoice) {
        self.accepted = Some(resolved.state);
        self.pending = None;
        self.last_finality = Some(resolved.finality);
    }

    const fn reject(&mut self, state: ForkchoiceState) {
        self.pending = None;
        self.rejected = Some(state);
    }
}

#[derive(Debug)]
enum SubscriptionError {
    IncompatibleChain(String),
    MalformedFinality(String),
    Transient(String),
}

impl std::fmt::Display for SubscriptionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleChain(message)
            | Self::MalformedFinality(message)
            | Self::Transient(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SubscriptionError {}

/// Applies a payload-free forkchoice update to the local subscriber.
#[async_trait]
pub trait ForkchoiceSink: Send + Sync {
    /// Returns the local engine's payload status. Implementations must not submit a payload.
    async fn apply_forkchoice(&self, state: ForkchoiceState) -> Result<PayloadStatusEnum, String>;
}

/// Production sink which communicates directly with the local Reth consensus engine.
#[derive(Debug)]
pub struct InProcessForkchoiceSink(ConsensusEngineHandle<EvolveEngineTypes>);

#[async_trait]
impl ForkchoiceSink for InProcessForkchoiceSink {
    async fn apply_forkchoice(&self, state: ForkchoiceState) -> Result<PayloadStatusEnum, String> {
        self.0
            .fork_choice_updated(state, None)
            .await
            .map(|response| response.payload_status.status)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        process_update, Finality, ForkchoiceSink, ForkchoiceTracker, HeadPublisher, HeadUpdate,
        RemoteBlock, ResolvedForkchoice, SubscriptionError,
    };
    use alloy_primitives::B256;
    use alloy_rpc_types_engine::{ForkchoiceState, PayloadStatusEnum};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    fn block(number: u64, byte: u8) -> RemoteBlock {
        RemoteBlock {
            hash: B256::repeat_byte(byte),
            number,
        }
    }

    fn update(
        chain_id: u64,
        genesis_hash: B256,
        head: RemoteBlock,
        safe: RemoteBlock,
        finalized: RemoteBlock,
    ) -> HeadUpdate {
        HeadUpdate {
            chain_id,
            genesis_hash,
            forkchoice_state: ForkchoiceState {
                head_block_hash: head.hash,
                safe_block_hash: safe.hash,
                finalized_block_hash: finalized.hash,
            },
            head_block_number: head.number,
            safe_block_number: safe.number,
            finalized_block_number: finalized.number,
        }
    }

    #[derive(Clone, Default)]
    struct RecordingSink(Arc<Mutex<Vec<ForkchoiceState>>>);

    #[async_trait]
    impl ForkchoiceSink for RecordingSink {
        async fn apply_forkchoice(
            &self,
            state: ForkchoiceState,
        ) -> Result<PayloadStatusEnum, String> {
            self.0.lock().expect("recording sink lock").push(state);
            Ok(PayloadStatusEnum::Valid)
        }
    }

    #[test]
    fn publisher_retains_only_latest_update() {
        let publisher = HeadPublisher::new(10, B256::repeat_byte(1));
        let state = ForkchoiceState {
            head_block_hash: B256::repeat_byte(4),
            safe_block_hash: B256::repeat_byte(3),
            finalized_block_hash: B256::repeat_byte(2),
        };
        assert!(publisher.publish(state, 4, 3, 2));
        assert!(!publisher.publish(state, 4, 3, 2));

        let receiver = publisher.latest.subscribe();
        assert_eq!(
            *receiver.borrow(),
            Some(HeadUpdate {
                chain_id: 10,
                genesis_hash: B256::repeat_byte(1),
                forkchoice_state: state,
                head_block_number: 4,
                safe_block_number: 3,
                finalized_block_number: 2,
            })
        );
    }

    #[tokio::test]
    async fn applies_valid_push_without_remote_block_queries() {
        let genesis = B256::repeat_byte(1);
        let head = block(10, 10);
        let safe = block(9, 9);
        let finalized = block(8, 8);
        let sink = RecordingSink::default();
        let mut tracker = ForkchoiceTracker::default();

        process_update(
            &sink,
            &mut tracker,
            update(7, genesis, head, safe, finalized),
            7,
            genesis,
        )
        .await
        .expect("valid pushed update");

        assert_eq!(
            sink.0.lock().expect("recording sink lock").as_slice(),
            &[ForkchoiceState {
                head_block_hash: head.hash,
                safe_block_hash: safe.hash,
                finalized_block_hash: finalized.hash,
            }]
        );
    }

    #[tokio::test]
    async fn rejects_incompatible_peer_identity() {
        let genesis = B256::repeat_byte(1);
        let error = process_update(
            &RecordingSink::default(),
            &mut ForkchoiceTracker::default(),
            update(8, genesis, block(10, 10), block(9, 9), block(8, 8)),
            7,
            genesis,
        )
        .await
        .expect_err("chain identity must match");
        assert!(matches!(error, SubscriptionError::IncompatibleChain(_)));
    }

    #[tokio::test]
    async fn applies_lower_head_reorg_without_finality_regression() {
        let genesis = B256::repeat_byte(1);
        let safe = block(8, 8);
        let sink = RecordingSink::default();
        let mut tracker = ForkchoiceTracker::default();

        process_update(
            &sink,
            &mut tracker,
            update(7, genesis, block(10, 10), safe, safe),
            7,
            genesis,
        )
        .await
        .expect("initial update");
        process_update(
            &sink,
            &mut tracker,
            update(7, genesis, block(9, 19), safe, safe),
            7,
            genesis,
        )
        .await
        .expect("lower canonical head is a valid reorg");

        let states = sink.0.lock().expect("recording sink lock");
        assert_eq!(states.len(), 2);
        assert_eq!(states[1].head_block_hash, B256::repeat_byte(19));
    }

    #[test]
    fn retains_finality_across_reconnects() {
        let accepted = ResolvedForkchoice {
            state: ForkchoiceState {
                head_block_hash: block(10, 10).hash,
                safe_block_hash: block(9, 9).hash,
                finalized_block_hash: block(8, 8).hash,
            },
            finality: Finality {
                safe: block(9, 9),
                finalized: block(8, 8),
            },
        };
        let mut tracker = ForkchoiceTracker::default();
        tracker.accept(accepted);

        let error = super::validate_forkchoice(
            block(11, 11),
            block(8, 8),
            block(8, 8),
            tracker.last_finality,
        )
        .expect_err("a reconnect must not forget accepted finality");
        assert!(matches!(error, SubscriptionError::MalformedFinality(_)));
    }

    #[test]
    fn invalid_forkchoice_is_not_retried_until_state_changes() {
        let resolved = ResolvedForkchoice {
            state: ForkchoiceState {
                head_block_hash: block(10, 10).hash,
                safe_block_hash: block(9, 9).hash,
                finalized_block_hash: block(8, 8).hash,
            },
            finality: Finality {
                safe: block(9, 9),
                finalized: block(8, 8),
            },
        };
        let mut tracker = ForkchoiceTracker::default();
        tracker.reject(resolved.state);
        assert!(tracker.should_skip(resolved));
    }
}
