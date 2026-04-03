# EasyTier v2.4.5 → v2.6.0 WebSocket 支持变化对比

## 涉及的 Commit

| Commit | 描述 |
|---|---|
| `88a45d1` | use 80/443 as ws/wss default port (#1700) |
| `73291a3` | feat: Update Cargo.toml to add support for tls1.2 when use wss (#1917) |
| `b56bcfb` | fix: increase websocket peer connection timeout to 20 seconds (#1939) |
| `fe4dff5` | perf: simplify method signatures and reduce clone across multiple files (#1663) |
| `d4c1b0e` | fix: read X-Forwarded-For from HTTP header of WS/WSS (#2019) |
| `443c3ca` | fix: append address of reverse proxy to remote_addr (#2034) |

## 涉及文件

| 文件 | 变化量 |
|---|---|
| `easytier/src/tunnel/websocket.rs` | 348行 → 443行（**+95行**，核心变更）|
| `easytier/src/tunnel/mod.rs` | 默认端口变更、`PROTO_PORT_OFFSET` 新增 `faketcp` 条目 |
| `easytier/src/connector/manual.rs` | 连接超时调整 |
| `easytier/src/common/dns.rs` | 兼容性注释补充 |
| `easytier/Cargo.toml` | 依赖升级+新增 |
| `easytier/src/tunnel/insecure_tls.rs` | 微小重构（影响 WSS TLS 证书生成）|

---

## 1. 反向代理支持（X-Forwarded-For / Forwarded） — 最重要的新功能

**v2.4.5：** 无反向代理感知能力。无论 WebSocket 连接是否经由 Nginx/Caddy 等代理，`remote_addr` 始终记录的是代理服务器的 IP。

**最新代码：** 完整支持读取反向代理转发的客户端真实 IP。

```rust
// 新增：可信代理网络段定义
static TRUSTED_PROXIES: LazyLock<Vec<IpNetwork>> = LazyLock::new(|| {
    [
        "127.0.0.0/8",   // Loopback
        "10.0.0.0/8",    // Private
        "172.16.0.0/12",
        "192.168.0.0/16",
        "::1/128",
        "fc00::/7",
    ].into_iter().map(|s| s.parse().unwrap()).collect()
});
```

- 支持 **`Forwarded`** 标准头（RFC 7239）
- 支持 **`X-Forwarded-For`** 通用头
- 仅在连接来源 IP 属于 `TRUSTED_PROXIES` 列表时才信任转发头
- 提取到真实 IP 后更新 `remote_addr`，并在 query 中附加 `proxy=<代理IP>` 用于溯源

**相关 PR：** #2019（d4c1b0e）、#2034（443c3ca）

---

## 2. 服务端 accept 流程重构

**v2.4.5：** TLS 和非 TLS 分支各自独立完成 WebSocket accept + split + 创建 TunnelWrapper，代码重复：

```rust
// 旧代码：两个分支各自做 accept → split → new TunnelWrapper
if is_wss {
    let stream = acceptor.accept(stream).await?;
    let (write, read) = server_bulder.accept(stream).await?.split();
    Box::new(TunnelWrapper::new(read.filter_map(...), write.with(...), Some(info)))
} else {
    let (write, read) = server_bulder.accept(stream).await?.split();
    Box::new(TunnelWrapper::new(read.filter_map(...), write.with(...), Some(info)))
};
```

**最新代码：** 使用 `Either` 统一 TLS / 非 TLS 流，在 WS accept 后集中处理：

```rust
// 新代码：先统一为 Either<TLS, Plain>
let stream = if is_wss { Either::Left(tls_accept) } else { Either::Right(stream) };
let (request, stream) = ServerBuilder::new().accept(stream).await?;
// ... 统一处理 Forwarded 头 ...
let (write, read) = stream.split();
// 统一创建 TunnelWrapper
```

消除了代码重复，且 `ServerBuilder::accept()` 现在返回 HTTP request 对象，使得读取请求头成为可能。

---

## 3. 默认端口变更

**v2.4.5：**
- `ws` → 端口 `11011`
- `wss` → 端口 `11012`

**最新代码：**
- `ws` → 端口 **`80`**
- `wss` → 端口 **`443`**

这使得 WebSocket 连接可以直接使用标准的 HTTP/HTTPS 端口，便于通过 Nginx/Caddy 等反向代理转发，与上面的反向代理支持功能相呼应。

**相关 PR：** #1700（88a45d1）

---

## 4. 连接超时时间增加

**v2.4.5：** WebSocket 连接使用默认的 **2 秒**超时。

**最新代码：** WebSocket 连接使用 **20 秒**长超时（与 HTTP/TXT/SRV 同级）：

```rust
let use_long_timeout = dead_url.scheme() == "http"
    || dead_url.scheme() == "https"
    || dead_url.scheme() == "ws"       // 新增
    || dead_url.scheme() == "wss"      // 新增
    || dead_url.scheme() == "txt"
    || dead_url.scheme() == "srv";
```

**相关 PR：** #1939（b56bcfb）

---

## 5. 依赖变化

| 依赖 | v2.4.5 | 最新代码 |
|---|---|---|
| `tokio-websockets` | `0.8` | **`0.13.2`** |
| `forwarded-header-value` | 无 | **新增 `0.1.1`** |
| `tokio_util::either::Either` | 无 | **新增使用** |
| `rustls` | `ring` | `ring` + **`tls12`**（支持 TLS 1.2）|

`rustls` 新增 `tls12` feature，使得 WSS 连接可以兼容仅支持 TLS 1.2 的服务器。

**相关 PR：** #1917（73291a3）

---

## 6. `build_url_from_socket_addr` 健壮性改进

`easytier/src/tunnel/mod.rs` 中的 `build_url_from_socket_addr` 函数被 WebSocket 隧道的 listener 和 connector 双方使用，用于构造 `remote_addr` / `local_addr`。

**v2.4.5：**
```rust
let mut ret_url = url::Url::parse(format!("{}://0.0.0.0", scheme).as_str()).unwrap();
```

**最新代码：** 增加了 `unwrap_or_else` 并输出有意义的 panic 信息：
```rust
let url_str = format!("{}://0.0.0.0", scheme);
let mut ret_url = url::Url::parse(url_str.as_str())
    .unwrap_or_else(|_| panic!("invalid url: {}", url_str));
```

这对排查自定义 scheme 导致的解析失败更加友好。

---

## 7. `try_accept` 签名变更

```rust
// v2.4.5：&mut self
async fn try_accept(&mut self, stream: TcpStream) -> Result<...>

// 最新代码：&self（不再需要可变引用）
async fn try_accept(&self, stream: TcpStream) -> Result<...>
```

这属于性能/签名简化的重构（PR #1663）。

---

## 8. DNS 解析层面的小调整

`easytier/src/common/dns.rs` 的 `socket_addrs` 函数（被 `SocketAddr::from_url` 使用，WS/WSS connector 依赖它做地址解析）有两处微调：

- `url.host_str()` 改为 `url.host()`，使用结构化 `Host` 枚举（`Ipv4`/`Ipv6`/`Domain`）代替字符串解析，更安全地处理 IPv6 地址
- 端口为 0 时的 fallback 逻辑增加了注释 `// here is for compatibility with old version`，表明 ws→80、wss→443 的映射在此处是旧版兼容路径

---

## 9. 新增测试

新增 `ws_forwarded` 集成测试，验证：
- 通过原始 TCP 发送带 `X-Forwarded-For` 头的 WebSocket 握手
- 服务端能正确提取到 `203.0.113.5` 作为真实客户端 IP
- 代理地址 `127.0.0.1:25560` 被附加到 query 参数 `proxy` 中

---

## 总结

| 变化 | 重要性 | 说明 |
|---|---|---|
| 反向代理 Forwarded 头支持 | ⭐⭐⭐ | 核心功能，使 WS 可以部署在 Nginx/Caddy 后面 |
| 默认端口 80/443 | ⭐⭐⭐ | 与反向代理场景天然配合 |
| accept 流程重构 | ⭐⭐ | 代码质量提升，消除了重复分支 |
| 连接超时 20s | ⭐⭐ | 改善网络延迟较大时的连接可靠性 |
| TLS 1.2 支持 | ⭐⭐ | 向后兼容更多 WSS 服务器 |
| tokio-websockets 0.8→0.13.2 | ⭐ | 依赖升级，获得新 API（返回 request 对象） |
| `build_url_from_socket_addr` 健壮性 | ⭐ | 更好的 panic 信息，便于排查 |
| DNS 解析层 IPv6 处理优化 | ⭐ | 使用结构化 `Host` 枚举代替字符串解析 |

**整体趋势：** v2.4.5 → v2.6.0 的 WebSocket 变化主要围绕 **生产环境反向代理部署** 场景进行全面强化。

---

## 附：未变化的部分（确认项）

以下与 WebSocket 相关的部分在 v2.4.5 → v2.6.0 之间 **没有发生变化**：

- **WSTunnelConnector 客户端连接流程**：TLS/非 TLS 分支、SNI 处理逻辑完全一致
- **WebSocket 消息编解码**：`sink_from_zc_packet` / `map_from_ws_message` 函数未改动
- **WS/WSS 协议判断**：`is_wss()` 函数不变
- **Listener listen() 逻辑**：socket2 绑定、端口设置流程不变
- **accept() 外层循环**：TCP accept + 3秒超时 + 重试逻辑不变
- **`easytier-web` 中的 WS 监听**：仅 import 路径重组，`"ws" => Box::new(WSTunnelListener::new(...))` 功能不变
- **Magisk/contrib 配置**：示例配置中的 `ws://0.0.0.0:11011/` 和 `wss://0.0.0.0:11012/` 未随默认端口变更而更新（潜在的文档/配置不一致）
