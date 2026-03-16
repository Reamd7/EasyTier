# EasyTier Worker Relay Design

## Goal

Build a new Cloudflare Workers based EasyTier public relay node that is compatible with existing EasyTier clients without modifying any code in the current EasyTier project. The new implementation must run as a standalone project in a new folder, use only Cloudflare Worker APIs plus the official Rust Workers SDK, and support only `wss` transport.

The relay must support:

- Existing EasyTier clients connecting to a `wss://...` endpoint as an ordinary EasyTier public node
- Relay traffic forwarding between peers when direct connectivity is unavailable
- Multiple EasyTier networks hosted by the same Worker deployment
- Private mode node discovery semantics compatible with EasyTier's existing `PeerCenterRpc`

It explicitly does not need to implement the full EasyTier runtime.

## Existing EasyTier Behaviors To Preserve

The new Worker project must preserve the following observable client behaviors from the existing repository:

- WebSocket transport behavior from `easytier/src/tunnel/websocket.rs`
  - Incoming and outgoing frames are binary
  - Frames carry EasyTier tunnel payload bytes
  - Clients treat the endpoint as a normal `ws` or `wss` listener
- Public node connection behavior from `easytier/src/connector/mod.rs`
  - Existing EasyTier clients must continue to connect with `wss://...` peer URLs
- Private mode discovery semantics from:
  - `easytier/src/peer_center/server.rs`
  - `easytier/src/peer_center/instance.rs`
  - `easytier/src/proto/peer_rpc.proto`
- Private mode isolation behavior from `easytier/src/peers/peer_manager.rs`
  - Networks with mismatched identity must not be allowed to mix in private mode

The Worker must not invent a new client-visible discovery API. It must instead emulate the current EasyTier public-node behavior closely enough that existing clients continue to work.

## Scope

### In Scope

- New standalone project for Cloudflare Workers in a new repository folder
- Rust implementation using Cloudflare's official Workers Rust SDK
- Durable Objects for connection ownership and shared network state
- `wss` entrypoint only
- Multi-network hosting in a single deployment
- Relay forwarding for EasyTier binary traffic over WebSocket
- Private mode discovery compatibility for the minimum `PeerCenterRpc` subset:
  - `ReportPeers`
  - `GetGlobalPeerMap`
- Connection lifecycle handling
- Peer presence cleanup and discovery state expiry
- Local development and deployment configuration for Workers

### Out of Scope

- TCP, UDP, QUIC, WireGuard, fake TCP, or HTTP tunnel listeners
- NAT traversal or hole punching
- Full EasyTier peer implementation in Workers
- Virtual NIC, routing table management, subnet proxying, VPN portal, DNS server, or management UI
- Rewriting or modifying the existing EasyTier repository runtime behavior
- Performance tuning beyond what is needed for correctness and a small initial rollout

## Architectural Options Considered

### Option A: One Durable Object per network

Each EasyTier network maps to one Durable Object that stores all WebSocket connections and all discovery state.

Pros:

- Simplest implementation
- Minimal cross-object coordination

Cons:

- Single hotspot for busy networks
- Connection I/O and shared discovery state contend in the same object

### Option B: Directory Durable Object plus relay shard Durable Objects

Each network maps to one directory Durable Object for shared state plus multiple relay shard Durable Objects for WebSocket ownership and forwarding.

Pros:

- Supports multiple networks cleanly
- Keeps global state separate from hot WebSocket handling
- Allows future scaling within a busy network

Cons:

- More internal coordination
- More implementation work than Option A

### Option C: One Durable Object per peer

Each peer gets a dedicated Durable Object while discovery state lives separately.

Pros:

- Strong per-peer isolation

Cons:

- Expensive cross-object routing
- Harder to reason about protocol ordering
- Excessive complexity for the first implementation

## Recommended Architecture

Use Option B: a per-network `NetworkDirectoryDO` plus one or more `RelayShardDO`s.

This design keeps network-wide discovery state centralized per network while allowing high-frequency WebSocket traffic to be distributed across relay shards. It matches the product goal best: a multi-network public relay node compatible with existing clients, while remaining implementable as a focused Worker project instead of a full EasyTier port.

## Project Structure

Create a new standalone Worker project under a new folder in this monorepo:

- `easytier-worker/`

Expected high-level files:

- `easytier-worker/Cargo.toml`
- `easytier-worker/wrangler.toml`
- `easytier-worker/src/lib.rs`
- `easytier-worker/src/router.rs`
- `easytier-worker/src/do/network_directory.rs`
- `easytier-worker/src/do/relay_shard.rs`
- `easytier-worker/src/protocol/`
- `easytier-worker/src/state/`
- `easytier-worker/tests/` or protocol-focused unit tests within `src/`

The new project should not depend on the existing EasyTier runtime crate for execution. It may reference the existing repository as protocol documentation and may copy or reimplement protocol structures in the new Worker project as needed.

## Components

### Worker Entrypoint

Responsibilities:

- Accept HTTP requests and WebSocket upgrade requests
- Route requests to network-specific Durable Objects
- Provide lightweight health and debug endpoints for development

Non-responsibilities:

- No long-lived shared network state
- No direct relay routing logic beyond the initial handoff

### `NetworkDirectoryDO`

Responsibilities:

- Own all network-level shared state for one EasyTier network
- Track online peers and the relay shard currently responsible for each peer
- Maintain private mode discovery state compatible with `PeerCenterRpc`
- Maintain and return the current discovery `digest`
- Expire stale peer reports and stale discovery edges
- Serve internal RPC-style operations invoked by relay shards

State stored here:

- `peer_id -> shard_id`
- Last-seen timestamps for online peers
- `src_peer_id -> { dst_peer_id -> latency_ms }` report data
- Discovery digest
- Optional network metadata needed for routing or validation

### `RelayShardDO`

Responsibilities:

- Own WebSocket connections for a subset of peers in one network
- Bind a socket to a peer once enough protocol identity is known
- Receive binary EasyTier frames
- Classify frames into:
  - ordinary relay traffic to another peer
  - private mode discovery traffic to be handled locally by the network directory
  - connection lifecycle and registration traffic
- Forward frames to local peers or remote shards
- Remove peers on disconnect and notify the network directory

State stored here:

- `peer_id -> WebSocket`
- Connection metadata such as `last_seen_at`
- Minimal buffering only where required for in-flight routing lookups

## Protocol Compatibility Strategy

The Worker must preserve existing EasyTier client-visible behavior rather than define a new protocol.

### WebSocket Transport

The Worker must behave like a normal EasyTier `wss` listener:

- accept binary WebSocket frames
- avoid altering frame boundaries unnecessarily
- preserve EasyTier payload bytes exactly unless the frame is one that the public relay itself must answer

### Relay Traffic

For ordinary relay data, the Worker acts as a switching fabric:

- inspect the minimum required fields to determine source peer and destination peer
- locate the destination shard through `NetworkDirectoryDO`
- forward the original binary payload to the correct destination socket

### Private Mode Discovery

The Worker must emulate the current EasyTier discovery semantics rather than expose a new REST or JSON endpoint.

Minimum supported semantics:

- `ReportPeers`
  - update the reporting peer's current direct-peer snapshot
- `GetGlobalPeerMap`
  - return the current global peer map when digest differs
  - return the equivalent of an empty update when digest matches

This behavior must track the observable semantics in:

- `easytier/src/peer_center/server.rs`
- `easytier/src/peer_center/instance.rs`
- `easytier/src/proto/peer_rpc.proto`

### Security and Isolation

- Multiple EasyTier networks must be isolated from one another
- Isolation must not rely only on URL path routing
- The Worker must respect existing network identity semantics observed in EasyTier handshakes and private mode behavior
- Private mode requests must not mix discovery state across different networks

## Network Partitioning Strategy

The deployment must host multiple EasyTier networks.

Recommended partitioning approach:

- Normalize a network identifier derived from the EasyTier handshake context
- Map each network identifier to a single `NetworkDirectoryDO`
- Within a network, assign peers to `RelayShardDO`s by a stable shard key derived from peer identity or a connection hash

This approach provides:

- deterministic isolation between networks
- room to scale a single busy network later
- a stable lookup path for `peer_id -> shard_id`

## Message Flow

### Connection Establishment

1. EasyTier client connects to the Worker `wss://...` endpoint.
2. Worker upgrades the request and routes it into a relay shard.
3. Relay shard reads the earliest EasyTier frames necessary to determine network identity and peer identity.
4. Relay shard registers the peer in `NetworkDirectoryDO`.
5. The connection is then available for relay forwarding and discovery traffic handling.

### Relay Forwarding

1. A peer sends a binary EasyTier frame.
2. The relay shard determines whether it is a peer-to-peer forwardable frame.
3. The relay shard queries the network directory for the destination peer's shard.
4. If the destination is local, the frame is written directly to the destination socket.
5. If the destination is remote, the frame is forwarded to the owning relay shard, which writes it to the destination socket.

### Discovery Update

1. A peer emits discovery-related traffic corresponding to `ReportPeers`.
2. The relay shard recognizes this traffic and calls into the network directory.
3. The network directory updates peer reports, recalculates digest if needed, and records timestamps.
4. A later `GetGlobalPeerMap` request is answered from the current state.

### Disconnect Handling

1. A WebSocket closes or times out.
2. The relay shard removes local socket state.
3. The relay shard tells the network directory that the peer is offline.
4. The network directory removes the online index entry and eventually expires stale report data.

## Data Model

### `NetworkDirectoryDO`

Minimum logical state:

- `online_peers: Map<PeerId, ShardId>`
- `peer_presence: Map<PeerId, LastSeen>`
- `peer_reports: Map<PeerId, ReportSnapshot>`
- `global_peer_map: Map<PeerId, Map<PeerId, DirectConnectedPeerInfo>>`
- `digest: u64`

`ReportSnapshot` should contain:

- reporting `peer_id`
- direct peer entries with latency
- `updated_at`

### `RelayShardDO`

Minimum logical state:

- `sockets: Map<PeerId, WebSocket>`
- `peer_meta: Map<PeerId, PeerConnectionMeta>`
- minimal temporary routing state needed to bridge lookups and socket writes

`PeerConnectionMeta` should contain:

- `peer_id`
- `network_id`
- `connected_at`
- `last_seen_at`

## Failure Handling

- Unknown or malformed frames should be rejected or dropped conservatively with debug logging
- Destination peer not found should produce the minimal behavior required to keep the relay stable, without poisoning state
- Stale online index entries must be cleaned when a shard reports disconnect or on periodic sweeps
- Discovery report entries older than the EasyTier-compatible expiration window should be removed
- Digest must be recomputed only when state changes that affect `GetGlobalPeerMap`

## Development Phases

### Phase 1: Relay minimum viable path

Deliver:

- standalone Worker project
- `wss` upgrade path
- relay shard socket ownership
- peer registration into directory DO
- peer-to-peer binary forwarding for a single network

Do not attempt in this phase:

- full private mode compatibility
- multi-network production hardening

### Phase 2: Private mode and multi-network completion

Deliver:

- network-based DO partitioning
- private mode discovery compatibility
- digest behavior
- stale state cleanup
- validation against multiple networks

## Testing Strategy

### Unit Tests

- network identifier normalization
- shard assignment logic
- discovery digest recomputation
- peer report expiry behavior
- binary frame classification helpers

### Integration Tests

- local Worker dev environment with Durable Objects
- two unmodified EasyTier clients connect via `wss`
- relay traffic succeeds through the Worker
- private mode discovery updates function correctly
- multiple networks remain isolated
- disconnect cleanup occurs within the configured time window

### Success Criteria

The project is successful when:

- unmodified EasyTier clients can use the Worker endpoint as a `wss` public node
- relay forwarding between peers works through the Worker
- private mode node discovery works through Worker-managed state
- multiple EasyTier networks can coexist without state leakage

## Risks

- The main implementation risk is not WebSocket support itself, but accurately identifying which EasyTier binary frames must be forwarded versus answered locally.
- The Worker project will need a minimal EasyTier protocol compatibility layer in Rust, because there is no prebuilt Worker-native runtime for these protocol structures.
- Durable Object coordination must avoid race conditions during peer reconnects and shard reassignment.

## Final Recommendation

Proceed with a new Rust Cloudflare Workers project using a per-network `NetworkDirectoryDO` plus per-network `RelayShardDO`s. Deliver the relay path first, then complete private mode discovery compatibility in the second phase.
