# easytier-worker

Cloudflare Worker implementation of an EasyTier-compatible websocket shared-node relay for private-mode discovery.

## Scope

This worker is intentionally limited to the websocket relay/shared-node use case:

- accepts EasyTier websocket peers on `GET /relay`
- maintains per-network peer presence with Durable Objects
- synthesizes shared-node OSPF state so private-mode discovery works through the worker
- relays peer-manager data, relay handshake, and peer RPC traffic between peers
- supports multiple EasyTier networks concurrently, isolated by network name

It does not try to run the full EasyTier daemon in Workers, and it does not expose non-websocket transports.

## Verified behavior

The current implementation has been verified locally with real EasyTier nodes and `wrangler dev`:

- two EasyTier nodes can connect to the worker through `ws://127.0.0.1:8791/relay`
- private-mode discovery works through the worker
- peer-to-peer RPC traffic is relayed correctly
- nodes can discover each other and then converge from relay to direct `p2p` connectivity when direct hole punching succeeds
- `easytier-cli ... peer-center`, `peer list`, and `route` return expected results during local validation

## Architecture

There are two Durable Objects:

- `NetworkDirectoryDO`: per-network peer registry, peer-center reports, and OSPF state snapshots
- `RelayShardDO`: websocket connection ownership, shared-node behavior, and frame forwarding

High-level flow:

1. EasyTier peers connect to `/relay` over websocket.
2. The worker extracts `network_name` and binds the websocket to a per-network peer identity.
3. OSPF sync requests addressed to shared node `1` are intercepted, stored, and answered by the worker.
4. The worker synthesizes shared-node route state and broadcasts it back to peers.
5. All other relay/data/RPC frames are forwarded by destination peer id, even when the worker cannot decode the inner RPC payload.

## Local development

Install dependencies in this subproject:

```bash
cd easytier-worker
npm install
```

Start the local Worker:

```bash
npx wrangler dev --local --port 8791
```

Health endpoint:

```text
GET /healthz
```

Relay endpoint:

```text
GET /relay
Upgrade: websocket
```

## Real-node smoke test

Example node A:

```bash
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-core \
  --network-name test-net \
  --network-secret secret123 \
  --hostname worker-test-a \
  --instance-name worker-test-a \
  --dhcp \
  --no-tun true \
  --external-node ws://127.0.0.1:8791/relay \
  --rpc-portal 127.0.0.1:16881 \
  --console-log-level debug
```

Example node B:

```bash
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-core \
  --network-name test-net \
  --network-secret secret123 \
  --hostname worker-test-b \
  --instance-name worker-test-b \
  --dhcp \
  --no-tun true \
  --external-node ws://127.0.0.1:8791/relay \
  --rpc-portal 127.0.0.1:16882 \
  --listeners tcp://0.0.0.0:12010 udp://0.0.0.0:12010 wg://0.0.0.0:12011 ws://0.0.0.0:12011/ wss://0.0.0.0:12012/ faketcp://0.0.0.0:12013 \
  --console-log-level debug
```

Useful checks:

```bash
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16881 peer-center
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16882 peer-center
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16881 peer list
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16882 peer list
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16881 route
/Users/gemini/Documents/playground/EasyTier/target/debug/easytier-cli -p 127.0.0.1:16882 route
```

## Limitations

- websocket transport only
- no attempt to support running the full EasyTier runtime inside Workers
- local `wrangler dev` Durable Object state can retain stale peers between runs; clearing `.wrangler/state` may be necessary during debugging
- auth hardening and operational polish still need work
- some cleanup warnings remain in the Rust crate and can be trimmed later

## Production domain

The worker can be bound to a custom hostname via Wrangler routes. The current config binds it to `relay.440711.xyz`, so the production relay URL is:

```text
wss://relay.440711.xyz/relay
```

After changing `wrangler.toml`, deploy again to apply the custom-domain trigger:

```bash
npx wrangler deploy
```

The build pipeline applies a small post-build patch to `build/index.js` because the generated runtime may call `__wbindgen_start()` during Wasm reinitialization even when the exported symbol is absent. This avoids runtime upgrade failures on Cloudflare while keeping the Rust/Wasm output unchanged.
```

Cloudflare will provision and attach the hostname inside the `440711.xyz` zone.

## Commands

```bash
cargo test --manifest-path easytier-worker/Cargo.toml
cargo check --manifest-path easytier-worker/Cargo.toml --target wasm32-unknown-unknown
npx wrangler dev --local --port 8791
```
