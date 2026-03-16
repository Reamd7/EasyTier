use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

use serde::{Deserialize, Serialize};

use crate::internal_api::{
    BindPeerRequest, DeliverFrameToPeerRequest, DiscoverySnapshotResponse, ForwardFrameRequest,
    ForwardFrameResponse, GetDiscoverySnapshotRequest, ListOspfStatesRequest, ListPeersRequest,
    ListPeersResponse, LocalDelivery, LookupPeerRequest, LookupPeerResponse, RegisterPeerRequest,
    RelayFrameEnvelope, ReportPeersRequest, UnregisterPeerRequest, UpsertOspfStateRequest,
};
use crate::protocol::{
    build_handshake_response_frame, build_peer_manager_frame, build_pong_frame,
    decode_get_global_peer_map_request, decode_handshake_request, extract_network_name,
    decode_ospf_sync_route_info_request, decode_ospf_sync_route_request_detail,
    decode_ospf_sync_route_response,
    decode_report_peers_request, encode_get_global_peer_map_response,
    encode_report_peers_response, extract_network_identity, inspect_rpc_payload,
    parse_peer_manager_frame, GlobalPeerMapView, PacketType,
};
use crate::state::PeerId;
use worker::*;

const DEFAULT_SHARD_ID: &str = "default";
const DEFAULT_NETWORK_ID: &str = "default";
const SHARED_NODE_PEER_ID: PeerId = 1;
const SHARED_NODE_NETWORK_NAME: &str = "easytier-worker";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PeerKey {
    network_id: String,
    peer_id: PeerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PeerSocketAttachment {
    network_id: String,
    network_name: String,
    peer_id: PeerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RelayShardState {
    connections: BTreeMap<PeerKey, ()>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RelayDecision {
    RejectConflict,
    Bound,
    DeliverLocal(LocalDelivery),
    ForwardToDirectory(RelayFrameEnvelope),
    Ignore,
}

impl RelayShardState {
    fn bind_peer(&mut self, req: BindPeerRequest) -> RelayDecision {
        let key = PeerKey {
            network_id: req.network_id,
            peer_id: req.peer_id,
        };

        self.connections.insert(key, ());
        RelayDecision::Bound
    }

    fn has_peer(&self, network_id: &str, peer_id: PeerId) -> bool {
        self.connections.contains_key(&PeerKey {
            network_id: network_id.to_string(),
            peer_id,
        })
    }

    fn classify_and_route(&self, network_id: &str, frame: &[u8]) -> RelayDecision {
        let Some(parsed) = parse_peer_manager_frame(frame) else {
            return RelayDecision::Ignore;
        };

        let should_route = matches!(
            parsed.header.packet_type,
            x if x == PacketType::Data as u8
                || x == PacketType::RelayHandshake as u8
                || x == PacketType::RelayHandshakeAck as u8
                || x == PacketType::TaRpc as u8
                || x == PacketType::RpcReq as u8
                || x == PacketType::RpcResp as u8
        );

        if !should_route {
            return RelayDecision::Ignore;
        }

        let envelope = RelayFrameEnvelope {
            src_peer_id: parsed.header.from_peer_id,
            dst_peer_id: parsed.header.to_peer_id,
            payload: frame.to_vec(),
        };

        if self.has_peer(network_id, parsed.header.to_peer_id) {
            RelayDecision::DeliverLocal(LocalDelivery {
                connection_id: local_delivery_key(network_id, parsed.header.to_peer_id),
                frame: envelope,
            })
        } else {
            RelayDecision::ForwardToDirectory(envelope)
        }
    }

    fn remove_peer(&mut self, network_id: &str, peer_id: PeerId) {
        self.connections.remove(&PeerKey {
            network_id: network_id.to_string(),
            peer_id,
        });
    }
}

fn local_delivery_key(network_id: &str, peer_id: PeerId) -> String {
    format!("{network_id}:{peer_id}")
}

fn parse_local_delivery_key(key: &str) -> Option<(&str, PeerId)> {
    let (network_id, peer_id) = key.rsplit_once(':')?;
    Some((network_id, peer_id.parse().ok()?))
}

fn network_directory_name(network_id: &str) -> String {
    format!("network:{network_id}")
}

async fn directory_fetch<TReq: Serialize, TResp: serde::de::DeserializeOwned>(
    env: &Env,
    network_id: &str,
    path: &str,
    payload: &TReq,
) -> Result<TResp> {
    let namespace = env.durable_object("NETWORK_DIRECTORY")?;
    let directory_name = network_directory_name(network_id);
    let stub = namespace.get_by_name(&directory_name)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(serde_json::to_vec(payload)?.into()));

    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    init.with_headers(headers);

    let mut response = stub.fetch_with_request(Request::new_with_init(path, &init)?).await?;
    response.json().await
}

async fn directory_post_for_network<TReq: Serialize>(
    env: &Env,
    network_id: &str,
    path: &str,
    payload: &TReq,
) -> Result<()> {
    let namespace = env.durable_object("NETWORK_DIRECTORY")?;
    let directory_name = network_directory_name(network_id);
    let stub = namespace.get_by_name(&directory_name)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(serde_json::to_vec(payload)?.into()));

    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    init.with_headers(headers);

    let _ = stub.fetch_with_request(Request::new_with_init(path, &init)?).await?;
    Ok(())
}

async fn discovery_snapshot(
    env: &Env,
    network_id: &str,
    digest: u64,
) -> Result<Option<DiscoverySnapshotResponse>> {
    let namespace = env.durable_object("NETWORK_DIRECTORY")?;
    let directory_name = network_directory_name(network_id);
    let stub = namespace.get_by_name(&directory_name)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post).with_body(Some(
        serde_json::to_vec(&GetDiscoverySnapshotRequest {
            network_id: network_id.into(),
            digest,
        })?
        .into(),
    ));

    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    init.with_headers(headers);

    let response = stub
        .fetch_with_request(Request::new_with_init("http://directory/discovery-snapshot", &init)?)
        .await?;
    let mut response = response;

    if response.status_code() == 204 {
        return Ok(None);
    }

    Ok(Some(response.json().await?))
}

fn rebuild_rpc_response_frame(request_frame: &[u8], rpc_payload: Vec<u8>) -> Option<Vec<u8>> {
    let parsed = parse_peer_manager_frame(request_frame)?;
    let mut frame = Vec::with_capacity(16 + rpc_payload.len());
    frame.extend_from_slice(&parsed.header.to_peer_id.to_le_bytes());
    frame.extend_from_slice(&parsed.header.from_peer_id.to_le_bytes());
    frame.push(crate::protocol::PacketType::RpcResp as u8);
    frame.push(parsed.header.flags);
    frame.push(parsed.header.forward_counter);
    frame.push(0);
    frame.extend_from_slice(&(rpc_payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&rpc_payload);
    Some(frame)
}

fn rewrite_frame_destination(frame: &[u8], dst_peer_id: PeerId) -> Option<Vec<u8>> {
    let _ = parse_peer_manager_frame(frame)?;
    let mut rewritten = frame.to_vec();
    rewritten[4..8].copy_from_slice(&dst_peer_id.to_le_bytes());
    Some(rewritten)
}

async fn relay_fetch<TReq: Serialize>(
    env: &Env,
    shard_id: &str,
    path: &str,
    payload: &TReq,
) -> Result<ForwardFrameResponse> {
    let relay_namespace = env.durable_object("RELAY_SHARD")?;
    let relay_stub = relay_namespace.get_by_name(shard_id)?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_body(Some(serde_json::to_vec(payload)?.into()));

    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    init.with_headers(headers);

    let mut response = relay_stub.fetch_with_request(Request::new_with_init(path, &init)?).await?;
    response.json().await
}

#[durable_object]
pub struct RelayShardDO {
    state: State,
    env: Env,
    memory: Rc<RefCell<RelayShardState>>,
}

impl RelayShardDO {
    async fn ensure_peer_registered(
        &self,
        peer_id: PeerId,
        network_id: &str,
        network_name: &str,
        shard_id: &str,
    ) -> Result<()> {
        directory_post_for_network(
            &self.env,
            network_id,
            "http://directory/register-peer",
            &RegisterPeerRequest {
                network_id: network_id.into(),
                network_name: network_name.into(),
                peer_id,
                shard_id: shard_id.into(),
                connected_at_unix_ms: Date::now().as_millis() as u64,
            },
        )
        .await
    }

    async fn unregister_peer(&self, peer_id: PeerId, network_id: &str, shard_id: &str) -> Result<()> {
        self.memory.borrow_mut().remove_peer(network_id, peer_id);

        directory_post_for_network(
            &self.env,
            network_id,
            "http://directory/unregister-peer",
            &UnregisterPeerRequest {
                network_id: network_id.into(),
                peer_id,
                shard_id: shard_id.into(),
            },
        )
        .await
    }

    fn deliver_local(&self, delivery: LocalDelivery) -> Result<()> {
        for socket in self.state.get_websockets() {
            let Some(attachment) = socket.deserialize_attachment::<PeerSocketAttachment>()? else {
                continue;
            };

            if let Some((network_id, peer_id)) = parse_local_delivery_key(&delivery.connection_id) {
                if attachment.peer_id == peer_id && attachment.network_id == network_id {
                    socket.send_with_bytes(&delivery.frame.payload)?;
                }
            }
        }

        Ok(())
    }

    async fn cleanup_socket(&self, ws: WebSocket) -> Result<()> {
        if let Some(attachment) = ws.deserialize_attachment::<PeerSocketAttachment>()? {
            self.unregister_peer(attachment.peer_id, &attachment.network_id, DEFAULT_SHARD_ID)
                .await?;
        }

        Ok(())
    }

    async fn handle_peer_center_rpc(
        &self,
        ws: &WebSocket,
        network_id: &str,
        frame: &[u8],
    ) -> Result<bool> {
        let Some(parsed) = parse_peer_manager_frame(frame) else {
            return Ok(false);
        };
        if parsed.header.to_peer_id != SHARED_NODE_PEER_ID {
            return Ok(false);
        }

        if let Some((request, report)) = decode_report_peers_request(frame) {
            directory_post_for_network(
                &self.env,
                network_id,
                "http://directory/report-peers",
                &ReportPeersRequest {
                    network_id: network_id.into(),
                    peer_id: report.peer_id,
                    direct_peers: report.direct_peers.into_iter().collect(),
                    updated_at_unix_ms: Date::now().as_millis() as u64,
                },
            )
            .await?;

            if let Some(response_frame) = rebuild_rpc_response_frame(
                frame,
                encode_report_peers_response(&request, request.to_peer, request.from_peer),
            ) {
                ws.send_with_bytes(&response_frame)?;
            }

            return Ok(true);
        }

        if let Some((request, get_map)) = decode_get_global_peer_map_request(frame) {
            let snapshot = discovery_snapshot(&self.env, network_id, get_map.digest).await?;
            let global_map = snapshot
                .map(|snapshot| GlobalPeerMapView {
                    digest: Some(snapshot.digest),
                    peers: snapshot
                        .global_peer_map
                        .into_iter()
                        .map(|(src, peers)| (src, peers.into_iter().collect()))
                        .collect(),
                })
                .unwrap_or(GlobalPeerMapView {
                    digest: None,
                    peers: BTreeMap::new(),
                });

            if let Some(response_frame) = rebuild_rpc_response_frame(
                frame,
                encode_get_global_peer_map_response(
                    &request,
                    request.to_peer,
                    request.from_peer,
                    global_map,
                ),
            ) {
                ws.send_with_bytes(&response_frame)?;
            }

            return Ok(true);
        }

        Ok(false)
    }

    async fn list_online_peers(&self, network_id: &str) -> Result<ListPeersResponse> {
        directory_fetch(
            &self.env,
            network_id,
            "http://directory/list-peers",
            &ListPeersRequest {
                network_id: network_id.to_string(),
            },
        )
        .await
    }

    async fn upsert_ospf_state(&self, network_id: &str, peer_id: PeerId, frame: &[u8]) -> Result<()> {
        directory_post_for_network(
            &self.env,
            network_id,
            "http://directory/upsert-ospf-state",
            &UpsertOspfStateRequest {
                network_id: network_id.to_string(),
                peer_id,
                frame: frame.to_vec(),
                updated_at_unix_ms: Date::now().as_millis() as u64,
            },
        )
        .await
    }

    async fn list_ospf_states(
        &self,
        network_id: &str,
    ) -> Result<Vec<crate::protocol::OspfRouteStateView>> {
        let response: crate::internal_api::ListOspfStatesResponse = directory_fetch(
            &self.env,
            network_id,
            "http://directory/list-ospf-states",
            &ListOspfStatesRequest {
                network_id: network_id.to_string(),
            },
        )
        .await?;

        Ok(response
            .states
            .into_iter()
            .filter_map(|state| {
                let parsed = parse_peer_manager_frame(&state.frame)?;
                let (_, ospf) = decode_ospf_sync_route_request_detail(parsed.payload)?;
                Some(ospf)
            })
            .collect())
    }

    fn synthesize_shared_peer_info(network_name: &str) -> crate::protocol::OspfPeerInfoView {
        crate::protocol::OspfPeerInfoView {
            peer_id: SHARED_NODE_PEER_ID,
            version: 1,
            peer_route_id: 1,
            ipv4_addr: None,
            proxy_cidrs: Vec::new(),
            hostname: Some(SHARED_NODE_NETWORK_NAME.to_string()),
            udp_nat_type: 0,
            tcp_nat_type: 0,
            easytier_version: format!("cf-worker:{network_name}"),
            network_length: 24,
            ipv6_addr: None,
            noise_static_pubkey: Vec::new(),
            feature_flag: Some(crate::protocol::OspfPeerFeatureFlagView {
                is_public_server: true,
                avoid_relay_data: true,
                kcp_input: false,
                no_relay_kcp: true,
                support_conn_list_sync: true,
                quic_input: false,
                no_relay_quic: true,
                is_credential_peer: false,
            }),
            raw_payload: None,
        }
    }

    fn synthesize_network_ospf_payload(
        network_name: &str,
        online_peers: &[(PeerId, String)],
        shared_conn_version: u32,
        target_peer_id: PeerId,
        states: &[crate::protocol::OspfRouteStateView],
    ) -> (Vec<crate::protocol::OspfPeerInfoView>, Vec<crate::protocol::OspfConnEntryView>) {
        let mut peer_infos = BTreeMap::<PeerId, crate::protocol::OspfPeerInfoView>::new();
        let mut conn_entries = BTreeMap::<PeerId, crate::protocol::OspfConnEntryView>::new();

        let shared_info = Self::synthesize_shared_peer_info(network_name);
        peer_infos.insert(shared_info.peer_id, shared_info.clone());

        let mut all_peers = BTreeMap::<PeerId, u32>::new();
        all_peers.insert(SHARED_NODE_PEER_ID, 1);

        for (peer_id, _) in online_peers {
            if *peer_id == SHARED_NODE_PEER_ID {
                continue;
            }
            all_peers.entry(*peer_id).or_insert(1);
        }

        for state in states {
            all_peers.entry(state.my_peer_id).or_insert(1);
            for info in &state.peer_infos {
                if info.peer_id == SHARED_NODE_PEER_ID {
                    continue;
                }
                all_peers
                    .entry(info.peer_id)
                    .and_modify(|version| *version = (*version).max(info.version.max(1)))
                    .or_insert(info.version.max(1));
                peer_infos.insert(info.peer_id, info.clone());
            }
        }

        for (peer_id, version) in &all_peers {
            if *peer_id == SHARED_NODE_PEER_ID {
                continue;
            }
            conn_entries.insert(
                *peer_id,
                crate::protocol::OspfConnEntryView {
                    peer_id: *peer_id,
                    version: *version,
                    connected_peer_ids: vec![SHARED_NODE_PEER_ID],
                },
            );
        }

        let connected_peers = all_peers
            .keys()
            .copied()
            .filter(|peer_id| *peer_id != SHARED_NODE_PEER_ID)
            .collect::<Vec<_>>();
        conn_entries.insert(
            SHARED_NODE_PEER_ID,
            crate::protocol::OspfConnEntryView {
                peer_id: SHARED_NODE_PEER_ID,
                version: shared_conn_version.max(1),
                connected_peer_ids: connected_peers,
            },
        );

        (peer_infos.into_values().collect(), conn_entries.into_values().collect())
    }

    async fn send_synthesized_ospf_to_peer(
        &self,
        network_id: &str,
        network_name: &str,
        peer_id: PeerId,
        shard_id: &str,
        peer_infos: &[crate::protocol::OspfPeerInfoView],
        conn_entries: &[crate::protocol::OspfConnEntryView],
    ) -> Result<()> {
        console_log!(
            "send ospf req network={} peer={} infos={:?} conns={:?}",
            network_name,
            peer_id,
            peer_infos,
            conn_entries
        );

        let rpc_payload = crate::protocol::encode_ospf_sync_route_request(
            network_name,
            SHARED_NODE_PEER_ID,
            peer_id,
            1,
            false,
            peer_infos,
            conn_entries,
        );
        let delivery_frame = build_peer_manager_frame(
            PacketType::RpcReq,
            &rpc_payload,
            SHARED_NODE_PEER_ID,
            peer_id,
        );

        if shard_id == DEFAULT_SHARD_ID {
            self.deliver_local(LocalDelivery {
                connection_id: local_delivery_key(network_id, peer_id),
                frame: RelayFrameEnvelope {
                    src_peer_id: SHARED_NODE_PEER_ID,
                    dst_peer_id: peer_id,
                    payload: delivery_frame,
                },
            })?;
        } else {
            let _ = relay_fetch(
                &self.env,
                shard_id,
                "http://relay/deliver-frame-to-peer",
                &DeliverFrameToPeerRequest {
                    network_id: network_id.to_string(),
                    peer_id,
                    frame: delivery_frame,
                },
            )
            .await?;
        }

        Ok(())
    }

    async fn broadcast_ospf_state_to_network(&self, network_id: &str) -> Result<()> {
        let peers = self.list_online_peers(network_id).await?;
        let ospf_states = self.list_ospf_states(network_id).await?;
        let network_name = peers
            .network_name
            .clone()
            .unwrap_or_else(|| network_id.to_string());

        let online_peers = peers.peers;
        let shared_conn_version = peers.topology_version;

        for (peer_id, shard_id) in &online_peers {
            let (peer_infos, conn_entries) = Self::synthesize_network_ospf_payload(
                &network_name,
                &online_peers,
                shared_conn_version,
                *peer_id,
                &ospf_states,
            );
            self.send_synthesized_ospf_to_peer(
                network_id,
                &network_name,
                *peer_id,
                shard_id,
                &peer_infos,
                &conn_entries,
            )
            .await?;
        }

        Ok(())
    }

    async fn handle_ospf_route_rpc(
        &self,
        ws: &WebSocket,
        network_id: &str,
        frame: &[u8],
    ) -> Result<bool> {
        let Some(parsed) = parse_peer_manager_frame(frame) else {
            return Ok(false);
        };
        if parsed.header.to_peer_id != SHARED_NODE_PEER_ID {
            return Ok(false);
        }

        let Some((rpc, ospf)) = decode_ospf_sync_route_info_request(parsed.payload) else {
            return Ok(false);
        };

        self.upsert_ospf_state(network_id, ospf.my_peer_id, frame).await?;

        let response_payload = crate::protocol::encode_empty_ospf_sync_route_response(
            &rpc,
            SHARED_NODE_PEER_ID,
            rpc.from_peer,
        );

        if let Some(response_frame) = rebuild_rpc_response_frame(frame, response_payload) {
            ws.send_with_bytes(&response_frame)?;
        }

        self.broadcast_ospf_state_to_network(network_id).await?;
        Ok(true)
    }

    fn handle_handshake(&self, ws: &WebSocket, frame: &[u8]) -> Result<bool> {
        let Some(request) = decode_handshake_request(frame) else {
            return Ok(false);
        };

        if request.my_peer_id == SHARED_NODE_PEER_ID {
            ws.close(Some(1011), Some("peer id conflict"))?;
            return Ok(true);
        }

        if let Some(response_frame) = build_handshake_response_frame(
            frame,
            SHARED_NODE_PEER_ID,
            SHARED_NODE_NETWORK_NAME,
        ) {
            ws.send_with_bytes(&response_frame)?;
            return Ok(true);
        }

        Ok(false)
    }

    fn handle_ping(&self, ws: &WebSocket, frame: &[u8]) -> Result<bool> {
        if let Some(response_frame) = build_pong_frame(frame) {
            ws.send_with_bytes(&response_frame)?;
            return Ok(true);
        }

        Ok(false)
    }
}

impl worker::DurableObject for RelayShardDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            memory: Rc::new(RefCell::new(RelayShardState::default())),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let path = req.path();

        match (req.method(), path.as_str()) {
            (Method::Get, "/") => Response::ok("relay-shard"),
            (Method::Get, "/relay") => {
                let pair = WebSocketPair::new()?;
                self.state.accept_web_socket(&pair.server);
                Response::from_websocket(pair.client)
            }
            (Method::Post, "/bind-peer") => {
                let payload: BindPeerRequest = serde_json::from_slice(&req.bytes().await?)?;
                match self.memory.borrow_mut().bind_peer(payload) {
                    RelayDecision::Bound => Response::empty(),
                    RelayDecision::RejectConflict => Response::error("conflicting peer binding", 409),
                    _ => Response::error("unexpected bind result", 500),
                }
            }
            (Method::Post, "/forward-frame") => {
                let payload: ForwardFrameRequest = serde_json::from_slice(&req.bytes().await?)?;
                let response = match self
                    .memory
                    .borrow()
                    .classify_and_route(&payload.network_id, &payload.frame.payload)
                {
                    RelayDecision::DeliverLocal(delivery) => {
                        self.deliver_local(delivery.clone())?;
                        ForwardFrameResponse::DeliverLocal(delivery)
                    }
                    RelayDecision::ForwardToDirectory(frame) => {
                        let lookup: LookupPeerResponse = directory_fetch(
                            &self.env,
                            &payload.network_id,
                            "http://directory/lookup-peer",
                            &LookupPeerRequest {
                                network_id: payload.network_id.clone(),
                                peer_id: frame.dst_peer_id,
                            },
                        )
                        .await?;

                        match lookup.shard_id {
                            Some(shard_id) if shard_id == DEFAULT_SHARD_ID => {
                                if self.memory.borrow().has_peer(&payload.network_id, frame.dst_peer_id) {
                                    let delivery = LocalDelivery {
                                        connection_id: local_delivery_key(
                                            &payload.network_id,
                                            frame.dst_peer_id,
                                        ),
                                        frame,
                                    };
                                    self.deliver_local(delivery.clone())?;
                                    ForwardFrameResponse::DeliverLocal(delivery)
                                } else {
                                    ForwardFrameResponse::Ignore
                                }
                            }
                            Some(shard_id) => {
                                relay_fetch(
                                    &self.env,
                                    &shard_id,
                                    "http://relay/forward-frame",
                                    &ForwardFrameRequest {
                                        network_id: payload.network_id.clone(),
                                        frame,
                                    },
                                )
                                .await?
                            }
                            None => ForwardFrameResponse::Ignore,
                        }
                    }
                    RelayDecision::Ignore => ForwardFrameResponse::Ignore,
                    RelayDecision::RejectConflict | RelayDecision::Bound => {
                        return Response::error("unexpected forward result", 500)
                    }
                };
                Response::from_json(&response)
            }
            (Method::Post, "/deliver-frame-to-peer") => {
                let payload: DeliverFrameToPeerRequest = serde_json::from_slice(&req.bytes().await?)?;
                self.deliver_local(LocalDelivery {
                    connection_id: local_delivery_key(&payload.network_id, payload.peer_id),
                    frame: RelayFrameEnvelope {
                        src_peer_id: SHARED_NODE_PEER_ID,
                        dst_peer_id: payload.peer_id,
                        payload: payload.frame,
                    },
                })?;
                Response::from_json(&ForwardFrameResponse::Ignore)
            }
            _ => Response::error("Not Found", 404),
        }
    }

    async fn websocket_message(
        &self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let WebSocketIncomingMessage::Binary(bytes) = message else {
            return Ok(());
        };

        let Some(parsed) = parse_peer_manager_frame(&bytes) else {
            return Ok(());
        };

        console_log!(
            "relay frame type={} from={} to={} len={}",
            parsed.header.packet_type,
            parsed.header.from_peer_id,
            parsed.header.to_peer_id,
            bytes.len()
        );

        if parsed.header.packet_type == crate::protocol::PacketType::RpcReq as u8 {
            if let Some(rpc) = inspect_rpc_payload(parsed.payload) {
                console_log!(
                    "rpc req domain={} service={} method_index={} method_kind={:?} tx={}",
                    rpc.domain_name,
                    rpc.service_name,
                    rpc.method_index,
                    rpc.method_kind,
                    rpc.transaction_id
                );
            }
        }

        if parsed.header.packet_type == crate::protocol::PacketType::RpcResp as u8 {
            if let Some((rpc, resp)) = decode_ospf_sync_route_response(parsed.payload) {
                console_log!(
                    "ospf resp domain={} tx={} session_id={} is_initiator={} error={:?} rpc_error={:?}",
                    rpc.domain_name,
                    rpc.transaction_id,
                    resp.session_id,
                    resp.is_initiator,
                    resp.error,
                    resp.rpc_error
                );
            }
        }

        let existing_attachment = ws.deserialize_attachment::<PeerSocketAttachment>()?;
        let network_name = extract_network_name(&bytes)
            .or_else(|| existing_attachment.as_ref().map(|it| it.network_name.clone()))
            .unwrap_or_else(|| DEFAULT_NETWORK_ID.to_string());
        let network_id = extract_network_identity(&bytes)
            .or_else(|| existing_attachment.as_ref().map(|it| it.network_id.clone()))
            .unwrap_or_else(|| DEFAULT_NETWORK_ID.to_string());
        let peer_id = parsed.header.from_peer_id;

        if existing_attachment.as_ref() != Some(&PeerSocketAttachment {
            network_id: network_id.clone(),
            network_name: network_name.clone(),
            peer_id,
        }) {
            if let Some(previous) = existing_attachment.as_ref() {
                self.unregister_peer(previous.peer_id, &previous.network_id, DEFAULT_SHARD_ID)
                    .await?;
            }

            ws.serialize_attachment(PeerSocketAttachment {
                network_id: network_id.clone(),
                network_name: network_name.clone(),
                peer_id,
            })?;

            let bind_result = self.memory.borrow_mut().bind_peer(BindPeerRequest {
                network_id: network_id.clone(),
                peer_id,
                connection_id: local_delivery_key(&network_id, peer_id),
            });

            match bind_result {
                RelayDecision::RejectConflict => {
                    ws.close(Some(1011), Some("conflicting peer binding"))?;
                    return Ok(());
                }
                RelayDecision::Bound => {
                    self.ensure_peer_registered(peer_id, &network_id, &network_name, DEFAULT_SHARD_ID)
                        .await?;
                }
                RelayDecision::DeliverLocal(_) | RelayDecision::ForwardToDirectory(_) | RelayDecision::Ignore => {
                    return Err(Error::RustError("unexpected bind state".into()));
                }
            }
        }

        if self.handle_handshake(&ws, &bytes)? {
            return Ok(());
        }

        if self.handle_ping(&ws, &bytes)? {
            return Ok(());
        }

        if self.handle_ospf_route_rpc(&ws, &network_id, &bytes).await? {
            return Ok(());
        }

        if self.handle_peer_center_rpc(&ws, &network_id, &bytes).await? {
            return Ok(());
        }

        match self.memory.borrow().classify_and_route(&network_id, &bytes) {
            RelayDecision::DeliverLocal(delivery) => {
                self.deliver_local(delivery)?;
            }
            RelayDecision::ForwardToDirectory(frame) => {
                let response = relay_fetch(
                    &self.env,
                    DEFAULT_SHARD_ID,
                    "http://relay/forward-frame",
                    &ForwardFrameRequest { network_id, frame },
                )
                .await?;

                if let ForwardFrameResponse::DeliverLocal(delivery) = response {
                    self.deliver_local(delivery)?;
                }
            }
            RelayDecision::Ignore | RelayDecision::RejectConflict | RelayDecision::Bound => {}
        }

        Ok(())
    }

    async fn websocket_close(
        &self,
        ws: WebSocket,
        _code: usize,
        _reason: String,
        _was_clean: bool,
    ) -> Result<()> {
        self.cleanup_socket(ws).await
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        self.cleanup_socket(ws).await
    }
}

#[cfg(test)]
mod tests {
    use crate::internal_api::BindPeerRequest;
    use crate::protocol::{OspfRouteStateView, PacketType};

    use super::{local_delivery_key, RelayDecision, RelayShardDO, RelayShardState, SHARED_NODE_PEER_ID};

    fn build_peer_manager_frame(packet_type: u8, payload: Vec<u8>, from_peer: u32, to_peer: u32) -> Vec<u8> {
        let mut frame = Vec::with_capacity(16 + payload.len());
        frame.extend_from_slice(&from_peer.to_le_bytes());
        frame.extend_from_slice(&to_peer.to_le_bytes());
        frame.push(packet_type);
        frame.push(0);
        frame.push(0);
        frame.push(0);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        frame
    }

    fn build_rpc_packet(service_name: &str, method_index: u32) -> Vec<u8> {
        use prost::Message;

        #[derive(Clone, PartialEq, Message)]
        struct RpcDescriptor {
            #[prost(string, tag = "1")]
            pub domain_name: String,
            #[prost(string, tag = "2")]
            pub proto_name: String,
            #[prost(string, tag = "3")]
            pub service_name: String,
            #[prost(uint32, tag = "4")]
            pub method_index: u32,
        }

        #[derive(Clone, PartialEq, Message)]
        struct RpcPacket {
            #[prost(uint32, tag = "1")]
            pub from_peer: u32,
            #[prost(uint32, tag = "2")]
            pub to_peer: u32,
            #[prost(int64, tag = "3")]
            pub transaction_id: i64,
            #[prost(message, optional, tag = "4")]
            pub descriptor: Option<RpcDescriptor>,
            #[prost(bytes = "vec", tag = "5")]
            pub body: Vec<u8>,
            #[prost(bool, tag = "6")]
            pub is_request: bool,
        }

        RpcPacket {
            from_peer: 1,
            to_peer: 2,
            transaction_id: 7,
            descriptor: Some(RpcDescriptor {
                domain_name: "network-a".into(),
                proto_name: "peer_rpc".into(),
                service_name: service_name.into(),
                method_index,
            }),
            body: Vec::new(),
            is_request: true,
        }
        .encode_to_vec()
    }

    #[test]
    fn binds_same_peer_id_for_different_networks() {
        let mut state = RelayShardState::default();

        assert_eq!(
            state.bind_peer(BindPeerRequest {
                network_id: "net-a".into(),
                peer_id: 10,
                connection_id: "ignored".into(),
            }),
            RelayDecision::Bound
        );
        assert_eq!(
            state.bind_peer(BindPeerRequest {
                network_id: "net-b".into(),
                peer_id: 10,
                connection_id: "ignored".into(),
            }),
            RelayDecision::Bound
        );
        assert!(state.has_peer("net-a", 10));
        assert!(state.has_peer("net-b", 10));
    }

    #[test]
    fn delivers_to_local_connection_when_destination_peer_is_present_in_same_network() {
        let mut state = RelayShardState::default();
        let _ = state.bind_peer(BindPeerRequest {
            network_id: "net-a".into(),
            peer_id: 20,
            connection_id: "ignored".into(),
        });

        let frame = build_peer_manager_frame(PacketType::Data as u8, vec![1, 2, 3], 10, 20);
        match state.classify_and_route("net-a", &frame) {
            RelayDecision::DeliverLocal(delivery) => {
                assert_eq!(delivery.connection_id, local_delivery_key("net-a", 20));
                assert_eq!(delivery.frame.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn forwards_to_directory_when_destination_peer_is_on_another_network() {
        let mut state = RelayShardState::default();
        let _ = state.bind_peer(BindPeerRequest {
            network_id: "net-b".into(),
            peer_id: 20,
            connection_id: "ignored".into(),
        });

        let frame = build_peer_manager_frame(PacketType::Data as u8, vec![1, 2, 3], 10, 20);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn forwards_to_directory_when_destination_peer_is_remote() {
        let state = RelayShardState::default();
        let frame = build_peer_manager_frame(PacketType::Data as u8, vec![1, 2, 3], 10, 20);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn delegates_peer_center_frames_to_directory_path() {
        let state = RelayShardState::default();
        let payload = build_rpc_packet("PeerCenterRpc", 1);
        let frame = build_peer_manager_frame(PacketType::RpcReq as u8, payload, 1, 2);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.src_peer_id, 1);
                assert_eq!(envelope.dst_peer_id, 2);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn forwards_peer_to_peer_rpc_frames_to_directory() {
        let state = RelayShardState::default();
        let payload = build_rpc_packet("OtherService", 7);
        let frame = build_peer_manager_frame(PacketType::RpcReq as u8, payload, 10, 20);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.src_peer_id, 10);
                assert_eq!(envelope.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn forwards_undecodable_rpc_frames_by_packet_type() {
        let state = RelayShardState::default();
        let frame = build_peer_manager_frame(PacketType::RpcReq as u8, vec![1, 2, 3], 10, 20);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.src_peer_id, 10);
                assert_eq!(envelope.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn relay_handshake_frames_are_forwarded_like_relay_data() {
        let state = RelayShardState::default();
        let frame = build_peer_manager_frame(PacketType::RelayHandshake as u8, vec![7, 8], 10, 20);

        match state.classify_and_route("net-a", &frame) {
            RelayDecision::ForwardToDirectory(envelope) => {
                assert_eq!(envelope.src_peer_id, 10);
                assert_eq!(envelope.dst_peer_id, 20);
            }
            other => panic!("unexpected relay decision: {other:?}"),
        }
    }

    #[test]
    fn remove_peer_clears_network_mapping() {
        let mut state = RelayShardState::default();
        let _ = state.bind_peer(BindPeerRequest {
            network_id: "net-a".into(),
            peer_id: 10,
            connection_id: "ignored".into(),
        });

        state.remove_peer("net-a", 10);

        assert!(!state.has_peer("net-a", 10));
    }

    #[test]
    fn synthesized_ospf_payload_does_not_emit_placeholder_peers_without_reported_peer_infos() {
        let states = vec![
            OspfRouteStateView {
                my_peer_id: 1001,
                my_session_id: 7,
                is_initiator: true,
                peer_infos: Vec::new(),
                conn_entries: Vec::new(),
            },
            OspfRouteStateView {
                my_peer_id: 1002,
                my_session_id: 9,
                is_initiator: false,
                peer_infos: Vec::new(),
                conn_entries: Vec::new(),
            },
        ];

        let (peer_infos, conn_entries) = RelayShardDO::synthesize_network_ospf_payload(
            "test-net",
            &[(1001, "default".into()), (1002, "default".into())],
            9,
            1001,
            &states,
        );

        assert!(peer_infos.iter().all(|info| info.peer_id != 1001));
        assert!(peer_infos.iter().all(|info| info.peer_id != 1002));

        let shared = peer_infos
            .iter()
            .find(|info| info.peer_id == SHARED_NODE_PEER_ID)
            .expect("shared peer should be present");
        assert_eq!(shared.version, 1);

        let shared_conn = conn_entries
            .iter()
            .find(|entry| entry.peer_id == SHARED_NODE_PEER_ID)
            .expect("shared conn should exist");
        assert_eq!(shared_conn.version, 9);
        assert_eq!(shared_conn.connected_peer_ids, vec![1001, 1002]);

        let conn_1001 = conn_entries
            .iter()
            .find(|entry| entry.peer_id == 1001)
            .expect("peer 1001 conn should exist");
        assert_eq!(conn_1001.version, 1);
        assert_eq!(conn_1001.connected_peer_ids, vec![SHARED_NODE_PEER_ID]);

        let conn_1002 = conn_entries
            .iter()
            .find(|entry| entry.peer_id == 1002)
            .expect("peer 1002 conn should exist");
        assert_eq!(conn_1002.version, 1);
        assert_eq!(conn_1002.connected_peer_ids, vec![SHARED_NODE_PEER_ID]);
    }
}
