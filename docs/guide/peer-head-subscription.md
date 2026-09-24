# Peer Head Subscription Guide

This guide explains how to run ev-reth full nodes without an EV node. The sequencer still runs its
EV node and ev-reth. Other ev-reth nodes subscribe to canonical forkchoice updates over WebSocket
and fetch the referenced blocks through native Reth P2P.

## When to Use This Mode

Use `--subscribe-peer` for an ev-reth full node that:

- does not produce blocks;
- does not run an EV node;
- needs to follow the canonical head selected by another ev-reth node; and
- has native P2P connectivity to at least one node that stores the referenced blocks.

Do not use this mode to replace the sequencer or its EV node. The subscription distributes
forkchoice decisions; it does not create blocks or run consensus.

## Architecture

The control plane and block-data plane are separate:

```text
                                    control plane: WebSocket
EV node ──Engine API──> ev-reth A ──ev_subscribeForkchoice──> ev-reth B
                        sequencer or                         subscribing
                        synchronized peer                   full node
                                                                  │
                                                                  │ native Reth P2P
                                                                  ▼
                                                            ev-reth C
                                                            block-data peer
```

`ev-reth A` and `ev-reth C` may be the same node. They do not have to be. A synchronized full node
can also act as `ev-reth A`, which allows subscriber-to-subscriber relay without adding an EV node.

The two connections serve different purposes:

| Connection | Carries | Establishes |
| --- | --- | --- |
| WebSocket | Chain identity and head, safe, and finalized references | Which valid forkchoice the subscriber should follow |
| Native Reth P2P | Headers, block bodies, and transactions | Where the subscriber obtains block data |

The WebSocket never carries execution payloads or blocks. The subscriber does not poll
`eth_blockNumber` or call `eth_getBlockByNumber` on the publishing peer.

## How an Update Moves Through the System

1. The sequencer's EV node builds and imports a block through the Engine API as usual.
2. The publishing ev-reth validates the block and accepts a forkchoice update as `VALID`.
3. The publisher resolves the head, safe, and finalized block numbers from its local database.
4. `ev_subscribeForkchoice` pushes the update to connected subscribers.
5. A subscriber checks the chain ID, genesis hash, and finality ordering.
6. The subscriber passes the payload-free forkchoice state directly to its local Reth engine.
7. If the target is unknown, Reth returns `SYNCING` and downloads the missing chain through P2P.
8. The subscriber retries the pending forkchoice locally every three seconds until Reth returns
   `VALID` or `INVALID`.
9. After accepting the update as `VALID`, the subscriber can publish the same state to downstream
   subscribers.

Only locally valid forkchoices are published. A node that is still downloading a target does not
relay it as valid.

## Requirements

Before starting a subscriber, confirm:

- The publishing and subscribing nodes run an ev-reth version that supports
  `ev_subscribeForkchoice`.
- Both nodes use the same chain ID and genesis hash. Use the same chainspec for the safest setup.
- The publishing peer has WebSocket RPC enabled.
- The subscriber can reach the publishing peer's WebSocket endpoint.
- The subscriber has at least one native P2P peer that has the announced blocks.
- Firewalls permit the selected WebSocket and P2P ports.

Default ports used in the examples:

| Service | Default | Required direction |
| --- | --- | --- |
| WebSocket RPC | TCP 8546 | Subscriber to publishing peer |
| Reth P2P | TCP 30303 | Between the subscriber and block-data peers |
| Discv4 discovery | UDP 30303 | Optional when static trusted peers are configured |
| HTTP RPC | TCP 8545 | Optional, for monitoring and user RPC |
| Authenticated Engine API | TCP 8551 | EV node to sequencer ev-reth only |

The subscription does not use the authenticated Engine API on the full node.

## Configure a Publishing Peer

Every ev-reth node starts the forkchoice publisher internally. Enable WebSocket RPC to make the
subscription available:

```bash
./target/release/ev-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/ev-reth-source \
  --ws \
  --ws.addr 0.0.0.0 \
  --ws.port 8546 \
  --addr 0.0.0.0 \
  --port 30303 \
  --http \
  --http.addr 127.0.0.1 \
  --http.api eth,net
```

The sequencer's EV node and Engine API configuration remain unchanged. Enabling this WebSocket
endpoint does not move block production into ev-reth.

The `ev_subscribeForkchoice` method is a custom module merged into every enabled WebSocket server.
Do not add `ev` to `--ws.api`; `ev` is not a standard Reth module name and no `--ws.api` change is
required.

Binding `--ws.addr 0.0.0.0` exposes the endpoint on every interface. Use it only on a private
network or with firewall rules that restrict subscriber addresses.

## Configure Native P2P

The subscriber needs a native Reth peer with the announced blocks. For a static private topology,
configure one or more comma-separated enode URLs:

```text
--trusted-peers enode://<public-key>@<host>:30303
```

The publishing peer is usually the simplest block-data peer, but a different synchronized ev-reth
node works as well. The WebSocket URL and `--trusted-peers` enode do not need to identify the same
machine.

For a closed topology, add:

```text
--trusted-only --disable-discovery
```

This restricts the node to configured trusted peers and disables discovery. Provide more than one
trusted peer in production if the node must continue syncing through a peer outage.

`--trusted-peers` controls connection policy. It does not make block data authoritative. Reth still
validates downloaded headers, bodies, transactions, and state transitions locally.

## Start a Subscribing Full Node

Start the full node with the same chainspec, a peer WebSocket URL, and native P2P peers:

```bash
./target/release/ev-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/ev-reth-subscriber \
  --subscribe-peer wss://peer.example/rpc \
  --trusted-peers enode://<public-key>@<block-peer>:30303 \
  --http \
  --http.addr 127.0.0.1 \
  --http.api eth,net,web3
```

For a direct private-network connection without TLS, use `ws://host:8546`. Use `wss://` when a TLS
proxy terminates the connection in front of ev-reth.

The equivalent environment variable is:

```bash
EV_SUBSCRIBE_PEER=wss://peer.example/rpc \
./target/release/ev-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/ev-reth-subscriber \
  --trusted-peers enode://<public-key>@<block-peer>:30303
```

Only one subscription endpoint can be configured. If that endpoint is unavailable, the full node
continues running and reconnects to the same endpoint with bounded backoff.

## Relay Through a Synchronized Full Node

A subscriber can serve downstream subscribers after its local engine validates the received
forkchoice. Enable WebSocket RPC on that node:

```bash
./target/release/ev-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/ev-reth-relay \
  --subscribe-peer wss://upstream.example/rpc \
  --trusted-peers enode://<public-key>@<block-peer>:30303 \
  --ws \
  --ws.addr 0.0.0.0 \
  --ws.port 8546
```

Downstream nodes can then use `wss://relay.example/rpc` as their `--subscribe-peer` endpoint. The
relay still needs P2P connectivity because it validates and stores blocks before publishing their
forkchoice.

## Verify the Setup

### 1. Verify the WebSocket method

With `websocat` installed, connect to the publishing peer:

```bash
websocat ws://127.0.0.1:8546
```

Send:

```json
{"jsonrpc":"2.0","id":1,"method":"ev_subscribeForkchoice","params":[]}
```

The server returns a subscription ID. If a valid state is already available, it also sends the
latest update. The update result has this shape:

```json
{
  "chainId": 1234,
  "genesisHash": "0x...",
  "forkchoiceState": {
    "headBlockHash": "0x...",
    "safeBlockHash": "0x...",
    "finalizedBlockHash": "0x..."
  },
  "headBlockNumber": 42,
  "safeBlockNumber": 42,
  "finalizedBlockNumber": 42
}
```

### 2. Check subscriber logs

Use module-specific debug logging:

```bash
RUST_LOG=info,ev_node::head=debug \
./target/release/ev-reth node \
  --chain /path/to/genesis.json \
  --datadir /var/lib/ev-reth-subscriber \
  --subscribe-peer ws://peer.internal:8546 \
  --trusted-peers enode://<public-key>@<block-peer>:30303
```

Expected messages include:

```text
subscribed to peer forkchoice updates
local engine is fetching peer target through P2P
applied peer forkchoice to local engine
```

The second message is logged only while the target is missing, and the last two require debug
logging.

### 3. Check P2P connectivity

If HTTP RPC exposes the `net` module:

```bash
curl -s \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"net_peerCount","params":[]}' \
  http://127.0.0.1:8545
```

A zero peer count explains why an unknown head cannot be downloaded.

### 4. Compare block height

Query `eth_blockNumber` on the publishing and subscribing nodes:

```bash
curl -s \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' \
  http://127.0.0.1:8545
```

The subscriber may lag while it downloads and executes blocks. It should converge to the
publishing peer's height when both the WebSocket control path and P2P data path are healthy.

## Failure Handling

The subscription task is non-critical. A transient subscription failure does not stop the node or
its other RPC and P2P services.

| Symptom or log | Meaning | Action |
| --- | --- | --- |
| `peer head subscription failed; reconnecting` | The WebSocket connection, RPC method, or subscription failed | Check the URL, proxy, firewall, TLS certificate, and ev-reth version on the peer |
| `peer forkchoice subscription ended; reconnecting` | An established WebSocket subscription closed | Check proxy idle timeouts and peer restarts |
| `local engine is fetching peer target through P2P` repeats | The control path works, but the target is not locally available yet | Check `net_peerCount`, enode addresses, P2P ports, network ID, and whether peers have the block |
| `peer violated head subscription trust policy; stopping subscriber` | Chain identity differs or safe/finalized regressed | Compare chainspecs and upstream state; correct the endpoint, then restart the subscriber |
| `local engine rejected peer forkchoice` | The downloaded chain or announced forkchoice failed local validation | Inspect the validation error and publishing peer; do not bypass validation |
| `could not resolve valid forkchoice locally` on a publisher | The publisher accepted an event but cannot resolve every referenced hash in its local database | Check local database/provider health and confirm safe and finalized references are available |
| WebSocket connects but no update arrives | The publisher has not accepted a complete valid forkchoice yet | Check block production and Engine API activity on the publishing peer |

Connection and subscription requests time out after 15 seconds. Reconnect delay starts at one
second and grows to at most 30 seconds. A stable session resets the delay.

The publisher retains one latest state instead of an unbounded history. A slow or reconnecting
subscriber receives current state and uses P2P to fill any missing history.

## Trust and Security

The configured WebSocket endpoint is authoritative for forkchoice. It can choose which valid fork
the subscriber follows. Local execution validation prevents invalid blocks from being imported,
but it does not choose between two otherwise valid forks.

Apply these controls:

- Restrict the WebSocket endpoint to known subscriber networks.
- Terminate `wss://` at a TLS proxy because ev-reth's WS listener is plain WebSocket.
- Do not expose `admin`, `debug`, or other unnecessary RPC modules on the subscription endpoint.
- Monitor unexpected finality regressions and repeated validation failures.
- Use multiple P2P peers for data availability, even when one WebSocket peer controls forkchoice.

`--subscribe-peer` currently accepts only a URL. It has no option for custom authorization headers
or client certificates. Protect the endpoint with network controls or a compatible TLS proxy, and
do not assume the Engine API JWT protects the regular WebSocket server. The Engine API and its JWT
remain a separate connection between the EV node and the sequencer's ev-reth.

## Current Limitations

- One authoritative subscription endpoint per node.
- No quorum or comparison across multiple forkchoice publishers.
- No block or payload transfer over WebSocket.
- No historical event replay; the publisher retains only the latest valid state.
- No custom WebSocket authentication headers from the subscriber.
- Vanilla Reth does not expose `ev_subscribeForkchoice`; the publishing peer must run ev-reth.
