# EasyTier Worker Relay Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone Cloudflare Workers Rust project that behaves as a multi-network EasyTier `wss` public relay node for unmodified EasyTier clients, with relay forwarding first and private-mode discovery compatibility second.

**Architecture:** Create a new Worker project under `easytier-worker` using Cloudflare's Rust Workers SDK. Route each EasyTier network to a `NetworkDirectoryDO` for shared state and to `RelayShardDO`s for WebSocket ownership and forwarding. Add a minimal Rust compatibility layer that can classify the EasyTier binary traffic needed for relay routing and private-mode discovery without depending on the existing EasyTier runtime crate.

**Tech Stack:** Rust, Cloudflare Workers Rust SDK, Durable Objects, WebSocket API, Wasm target, Wrangler

---

## File Structure

### New project files

- Create: `easytier-worker/Cargo.toml`
- Create: `easytier-worker/package.json`
- Create: `easytier-worker/wrangler.toml`
- Create: `easytier-worker/README.md`
- Create: `easytier-worker/src/lib.rs`
- Create: `easytier-worker/src/router.rs`
- Create: `easytier-worker/src/config.rs`
- Create: `easytier-worker/src/do/mod.rs`
- Create: `easytier-worker/src/do/network_directory.rs`
- Create: `easytier-worker/src/do/relay_shard.rs`
- Create: `easytier-worker/src/state/mod.rs`
- Create: `easytier-worker/src/state/types.rs`
- Create: `easytier-worker/src/state/digest.rs`
- Create: `easytier-worker/src/protocol/mod.rs`
- Create: `easytier-worker/src/protocol/frame.rs`
- Create: `easytier-worker/src/protocol/network_identity.rs`
- Create: `easytier-worker/src/protocol/peer_center.rs`
- Create: `easytier-worker/src/internal_api/mod.rs`
- Create: `easytier-worker/src/internal_api/network_directory.rs`
- Create: `easytier-worker/src/internal_api/relay_shard.rs`

### Test files

- Create: `easytier-worker/src/protocol/tests.rs`
- Create: `easytier-worker/src/state/tests.rs`
- Create: `easytier-worker/src/internal_api/tests.rs`

### Reference files to inspect during implementation

- Reference: `easytier/src/tunnel/websocket.rs`
- Reference: `easytier/src/connector/mod.rs`
- Reference: `easytier/src/peer_center/server.rs`
- Reference: `easytier/src/peer_center/instance.rs`
- Reference: `easytier/src/proto/peer_rpc.proto`
- Reference: `easytier/src/peers/peer_manager.rs`

## Chunk 1: Bootstrap the standalone Worker project

### Task 1: Create the project manifest and Wrangler config

**Files:**
- Create: `easytier-worker/Cargo.toml`
- Create: `easytier-worker/package.json`
- Create: `easytier-worker/wrangler.toml`
- Create: `easytier-worker/README.md`

- [ ] **Step 1: Write the failing smoke check expectation in the README and config comments**

Document the exact expected local commands and expected outputs:

```text
cargo test
cargo check --target wasm32-unknown-unknown
npx wrangler deploy --dry-run
```

Expected before code exists:

- `cargo test` fails because source files are missing
- `cargo check` fails because crate source files are missing

- [ ] **Step 2: Run the failing cargo check to verify the empty project fails for the right reason**

Run: `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`

Expected: FAIL with missing `src/lib.rs` or unresolved manifest targets.

- [ ] **Step 3: Add the minimal project files and dependencies**

Create a Rust Worker crate configured for `cdylib` and Workers deployment, including only the initial dependencies required for:

- worker runtime
- serde
- serde_json
- thiserror
- futures
- wasm-bindgen-compatible support as required by the Worker SDK

Keep dependencies minimal. Do not add protocol-specific crates until later tasks need them.

- [ ] **Step 4: Add a minimal Worker entrypoint that responds to health checks**

Implement `src/lib.rs` with a simple Worker fetch handler returning `200 OK` for a path such as `/healthz`.

- [ ] **Step 5: Run cargo check to verify the project builds**

Run: `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`

Expected: PASS.

- [ ] **Step 6: Run tests to verify the empty baseline stays green**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml`

Expected: PASS with zero or minimal tests.

- [ ] **Step 7: Commit**

```bash
git add easytier-worker
git commit -m "feat(worker): bootstrap relay worker project"
```

## Chunk 2: Add network routing and Durable Object skeletons

### Task 2: Define the Worker router and Durable Object registration

**Files:**
- Modify: `easytier-worker/src/lib.rs`
- Create: `easytier-worker/src/router.rs`
- Create: `easytier-worker/src/config.rs`
- Create: `easytier-worker/src/do/mod.rs`
- Create: `easytier-worker/src/do/network_directory.rs`
- Create: `easytier-worker/src/do/relay_shard.rs`

- [ ] **Step 1: Write a failing router test for path and upgrade dispatch**

Add a test that describes the intended routing behavior at a pure-function level:

- `/healthz` returns health handler
- WebSocket upgrade requests route to the relay path
- non-upgrade requests to relay path are rejected

- [ ] **Step 2: Run the router test to verify it fails**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml router`

Expected: FAIL because routing helpers do not exist yet.

- [ ] **Step 3: Implement the router and register Durable Object classes**

Add:

- path and upgrade detection helpers
- Worker fetch dispatch
- registration stubs for `NetworkDirectoryDO` and `RelayShardDO`
- stable helper functions for deriving object names from network IDs and shard IDs

- [ ] **Step 4: Run the router tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml router`

Expected: PASS.

- [ ] **Step 5: Run cargo check for the full crate**

Run: `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add easytier-worker/src
git commit -m "feat(worker): add router and durable object skeletons"
```

## Chunk 3: Build state types and digest behavior for private-mode discovery

### Task 3: Implement serializable network state and digest recomputation

**Files:**
- Create: `easytier-worker/src/state/mod.rs`
- Create: `easytier-worker/src/state/types.rs`
- Create: `easytier-worker/src/state/digest.rs`
- Create: `easytier-worker/src/state/tests.rs`

- [ ] **Step 1: Write failing tests for report replacement, expiry, and digest stability**

Cover these behaviors:

- replacing a peer's report updates only that peer's snapshot
- digest is stable for logically identical ordered data
- expiring outdated reports removes them from the derived global map

- [ ] **Step 2: Run the state tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml state`

Expected: FAIL because state types and digest functions do not exist.

- [ ] **Step 3: Implement the state types and digest helper**

Implement plain serializable types for:

- peer report snapshots
- direct connected peer info
- global peer map projection
- last-seen metadata
- digest recomputation using stable ordering

Model the semantics after `easytier/src/peer_center/server.rs`.

- [ ] **Step 4: Run the state tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml state`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker/src/state
git commit -m "feat(worker): add discovery state and digest model"
```

## Chunk 4: Build the minimal internal API between directory and shards

### Task 4: Define internal request and response contracts

**Files:**
- Create: `easytier-worker/src/internal_api/mod.rs`
- Create: `easytier-worker/src/internal_api/network_directory.rs`
- Create: `easytier-worker/src/internal_api/relay_shard.rs`
- Create: `easytier-worker/src/internal_api/tests.rs`

- [ ] **Step 1: Write failing serialization tests for internal API payloads**

Cover:

- register peer request
- unregister peer request
- lookup destination shard response
- report peers request
- get global map response

- [ ] **Step 2: Run the internal API tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml internal_api`

Expected: FAIL because internal API types do not exist.

- [ ] **Step 3: Implement minimal JSON or structured message payloads for DO-to-DO calls**

Ensure these contracts are explicit and versionable. Do not overload raw strings or ad hoc maps.

- [ ] **Step 4: Run the internal API tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml internal_api`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker/src/internal_api
git commit -m "feat(worker): define durable object internal api"
```

## Chunk 5: Implement network identity extraction and frame classification scaffolding

### Task 5: Add protocol compatibility helpers for relay routing

**Files:**
- Create: `easytier-worker/src/protocol/mod.rs`
- Create: `easytier-worker/src/protocol/frame.rs`
- Create: `easytier-worker/src/protocol/network_identity.rs`
- Create: `easytier-worker/src/protocol/peer_center.rs`
- Create: `easytier-worker/src/protocol/tests.rs`

- [ ] **Step 1: Write failing tests for the first compatibility helpers**

Add tests for pure parsing and classification helpers only. The tests should describe the intended API surface, not the final full protocol implementation. Cover:

- rejecting non-binary frames
- extracting a normalized network identifier from the minimal handshake context once enough bytes are available
- classifying a frame as `unknown`, `relay_data`, `peer_center_report`, or `peer_center_get_map`

- [ ] **Step 2: Run the protocol tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml protocol`

Expected: FAIL because helpers do not exist.

- [ ] **Step 3: Implement the first protocol helper layer**

Important constraints:

- keep this layer intentionally narrow
- do not attempt to port the whole EasyTier protocol stack
- add enough structure to support connection registration, relay routing, and peer center detection
- document exactly which EasyTier messages are recognized and which remain opaque

- [ ] **Step 4: Run the protocol tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml protocol`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker/src/protocol
git commit -m "feat(worker): add protocol classification helpers"
```

## Chunk 6: Implement `NetworkDirectoryDO`

### Task 6: Add peer registration, lookup, and private-mode state handling

**Files:**
- Modify: `easytier-worker/src/do/network_directory.rs`
- Modify: `easytier-worker/src/state/mod.rs`
- Modify: `easytier-worker/src/internal_api/network_directory.rs`
- Modify: `easytier-worker/src/protocol/peer_center.rs`

- [ ] **Step 1: Write failing tests for directory state transitions**

Cover pure state-machine or handler-level behavior for:

- registering a peer to a shard
- moving a peer to a new shard on reconnect
- unregistering a peer
- storing peer reports
- returning an empty update when digest matches

- [ ] **Step 2: Run the directory tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml network_directory`

Expected: FAIL because directory handlers are incomplete.

- [ ] **Step 3: Implement the directory Durable Object logic**

Add handlers for:

- register peer
- unregister peer
- lookup shard by peer ID
- report peer connections
- get global peer map
- periodic or request-triggered stale-entry cleanup

- [ ] **Step 4: Run the directory tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml network_directory`

Expected: PASS.

- [ ] **Step 5: Run cargo check to ensure Worker-compatible compilation still passes**

Run: `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add easytier-worker/src/do/network_directory.rs easytier-worker/src/state easytier-worker/src/internal_api easytier-worker/src/protocol
git commit -m "feat(worker): implement network directory durable object"
```

## Chunk 7: Implement `RelayShardDO` with WebSocket ownership and local routing

### Task 7: Add WebSocket session ownership and local peer delivery

**Files:**
- Modify: `easytier-worker/src/do/relay_shard.rs`
- Modify: `easytier-worker/src/router.rs`
- Modify: `easytier-worker/src/protocol/frame.rs`

- [ ] **Step 1: Write failing tests for local session registration and send-path selection**

Cover handler-level logic for:

- binding an identified peer to a socket entry
- rejecting conflicting registration without replacement rules
- choosing local delivery when destination peer is on the same shard
- delegating discovery messages to the directory path

- [ ] **Step 2: Run the relay shard tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml relay_shard`

Expected: FAIL because relay shard logic is incomplete.

- [ ] **Step 3: Implement relay shard WebSocket handling**

Add logic for:

- WebSocket acceptance from the Worker router
- initial frame processing to identify network and peer
- peer registration in the network directory
- local socket lookup and write path
- disconnect cleanup notification to the network directory

- [ ] **Step 4: Run the relay shard tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml relay_shard`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker/src/do/relay_shard.rs easytier-worker/src/router.rs easytier-worker/src/protocol/frame.rs
git commit -m "feat(worker): implement relay shard websocket handling"
```

## Chunk 8: Add cross-shard relay forwarding

### Task 8: Forward peer traffic across relay shards

**Files:**
- Modify: `easytier-worker/src/do/relay_shard.rs`
- Modify: `easytier-worker/src/internal_api/relay_shard.rs`
- Modify: `easytier-worker/src/internal_api/network_directory.rs`

- [ ] **Step 1: Write failing tests for cross-shard forwarding decisions**

Cover:

- destination on another shard triggers remote-forward request
- destination not found is handled safely
- directory lookup result controls local versus remote forwarding

- [ ] **Step 2: Run the cross-shard tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml cross_shard`

Expected: FAIL because remote forwarding behavior does not exist.

- [ ] **Step 3: Implement remote shard forwarding**

Ensure that the forwarded payload remains the original EasyTier binary frame, with no Worker-defined framing visible to clients.

- [ ] **Step 4: Run the cross-shard tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml cross_shard`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker/src/do/relay_shard.rs easytier-worker/src/internal_api
git commit -m "feat(worker): add cross-shard relay forwarding"
```

## Chunk 9: Implement private-mode discovery response path

### Task 9: Answer `PeerCenterRpc`-compatible discovery requests through the directory

**Files:**
- Modify: `easytier-worker/src/protocol/peer_center.rs`
- Modify: `easytier-worker/src/do/relay_shard.rs`
- Modify: `easytier-worker/src/do/network_directory.rs`

- [ ] **Step 1: Write failing tests for discovery request handling**

Cover:

- `ReportPeers` updates the network directory state
- `GetGlobalPeerMap` returns current map when digest differs
- `GetGlobalPeerMap` returns empty update when digest matches

- [ ] **Step 2: Run the discovery tests to verify they fail**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml peer_center`

Expected: FAIL because end-to-end discovery request handling is incomplete.

- [ ] **Step 3: Implement the discovery response path**

Add only the minimum request parsing and response encoding required to preserve existing client behavior for the current EasyTier `PeerCenterRpc` subset.

- [ ] **Step 4: Run the discovery tests to verify they pass**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml peer_center`

Expected: PASS.

- [ ] **Step 5: Run the full test suite**

Run: `cargo test --manifest-path easytier-worker/Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add easytier-worker/src/protocol/peer_center.rs easytier-worker/src/do
git commit -m "feat(worker): add private mode discovery compatibility"
```

## Chunk 10: Add operational docs and deployment verification

### Task 10: Finalize usage documentation and verify deployability

**Files:**
- Modify: `easytier-worker/README.md`
- Modify: `easytier-worker/wrangler.toml`
- Modify: `easytier-worker/package.json`

- [ ] **Step 1: Write failing documentation checks by listing the missing required sections**

The README must include:

- local dev commands
- deployment commands
- Durable Object bindings
- example `wss://...` peer URL usage
- current known limitations

- [ ] **Step 2: Run the verification commands before documentation is finalized**

Run:

- `cargo test --manifest-path easytier-worker/Cargo.toml`
- `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`
- `npx wrangler deploy --dry-run --config easytier-worker/wrangler.toml`

Expected: If any command fails, fix the project before documenting it as complete.

- [ ] **Step 3: Finalize README and project scripts**

Document:

- how the Worker is structured
- what EasyTier features are supported
- how to deploy it
- how to point unmodified EasyTier clients at the Worker
- what is intentionally unsupported

- [ ] **Step 4: Re-run the verification commands and capture the exact outcome**

Run:

- `cargo test --manifest-path easytier-worker/Cargo.toml`
- `cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown`
- `npx wrangler deploy --dry-run --config easytier-worker/wrangler.toml`

Expected: PASS for all commands.

- [ ] **Step 5: Commit**

```bash
git add easytier-worker
git commit -m "docs(worker): document deployment and usage"
```

Plan complete and saved to `docs/superpowers/plans/2026-03-15-easytier-worker-relay.md`. Ready to execute?
