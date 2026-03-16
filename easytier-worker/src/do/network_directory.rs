use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

use crate::internal_api::{
    DiscoverySnapshotResponse, GetDiscoverySnapshotRequest, ListOspfStatesRequest,
    ListOspfStatesResponse, ListPeersRequest, ListPeersResponse, LookupPeerRequest,
    LookupPeerResponse, OspfPeerStateView, RegisterPeerRequest, ReportPeersRequest,
    UnregisterPeerRequest, UpsertOspfStateRequest,
};
use crate::protocol::{
    build_peer_manager_frame, decode_ospf_sync_route_request_detail,
    encode_ospf_sync_route_request, parse_peer_manager_frame, OspfRouteStateView, PacketType,
};
use crate::state::{
    compute_global_peer_map_digest, LastSeen, NetworkState, PeerId, PeerReport,
};
use serde::{Deserialize, Deserializer, Serialize};
use worker::*;

const DIRECTORY_STATE_KEY: &str = "directory-state";
const ONLINE_PEER_TTL_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OspfPeerState {
    frame: Vec<u8>,
    updated_at_unix_ms: u64,
}

impl Default for OspfPeerState {
    fn default() -> Self {
        Self {
            frame: Vec::new(),
            updated_at_unix_ms: 0,
        }
    }
}

impl Serialize for OspfPeerState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct StoredOspfPeerState<'a> {
            frame: &'a [u8],
            updated_at_unix_ms: u64,
        }

        StoredOspfPeerState {
            frame: &self.frame,
            updated_at_unix_ms: self.updated_at_unix_ms,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OspfPeerState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct StoredOspfPeerState {
            frame: Option<Vec<u8>>,
            state: Option<OspfRouteStateView>,
            updated_at_unix_ms: u64,
        }

        let stored = StoredOspfPeerState::deserialize(deserializer)?;
        let frame = match (stored.frame, stored.state) {
            (Some(frame), _) => frame,
            (None, Some(state)) => encode_ospf_state_frame("", &state),
            (None, None) => Vec::new(),
        };

        Ok(Self {
            frame,
            updated_at_unix_ms: stored.updated_at_unix_ms,
        })
    }
}

fn decode_ospf_state_frame(frame: &[u8]) -> Option<OspfRouteStateView> {
    let parsed = parse_peer_manager_frame(frame)?;
    let (_, state) = decode_ospf_sync_route_request_detail(parsed.payload)?;
    Some(state)
}

fn encode_ospf_state_frame(domain_name: &str, state: &OspfRouteStateView) -> Vec<u8> {
    let payload = encode_ospf_sync_route_request(
        domain_name,
        state.my_peer_id,
        1,
        state.my_session_id,
        state.is_initiator,
        &state.peer_infos,
        &state.conn_entries,
    );
    build_peer_manager_frame(PacketType::RpcReq, &payload, state.my_peer_id, 1)
}

fn merge_ospf_state(
    current: &mut OspfRouteStateView,
    mut incoming: OspfRouteStateView,
) {
    current.my_session_id = incoming.my_session_id;
    current.is_initiator = incoming.is_initiator;

    for incoming_info in incoming.peer_infos.drain(..) {
        if let Some(existing) = current
            .peer_infos
            .iter_mut()
            .find(|existing| existing.peer_id == incoming_info.peer_id)
        {
            if incoming_info.version >= existing.version {
                *existing = incoming_info;
            }
        } else {
            current.peer_infos.push(incoming_info);
        }
    }

    for incoming_conn in incoming.conn_entries.drain(..) {
        if let Some(existing) = current
            .conn_entries
            .iter_mut()
            .find(|existing| existing.peer_id == incoming_conn.peer_id)
        {
            if incoming_conn.version >= existing.version {
                *existing = incoming_conn;
            }
        } else {
            current.conn_entries.push(incoming_conn);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
struct PerNetworkDirectoryState {
    network_name: Option<String>,
    online_peers: BTreeMap<PeerId, String>,
    online_peer_timestamps: BTreeMap<PeerId, u64>,
    topology_version: u32,
    network_state: NetworkState,
    ospf_states: BTreeMap<PeerId, OspfPeerState>,
}

impl PerNetworkDirectoryState {
    fn register_peer(&mut self, req: RegisterPeerRequest) {
        self.network_name = Some(req.network_name.clone());
        let changed = self.online_peers.get(&req.peer_id) != Some(&req.shard_id);
        self.online_peers.insert(req.peer_id, req.shard_id);
        self.online_peer_timestamps
            .insert(req.peer_id, req.connected_at_unix_ms);
        if changed {
            self.bump_topology_version();
        }
    }

    fn unregister_peer(&mut self, req: UnregisterPeerRequest) {
        if self
            .online_peers
            .get(&req.peer_id)
            .is_some_and(|current| current == &req.shard_id)
        {
            self.online_peers.remove(&req.peer_id);
            self.online_peer_timestamps.remove(&req.peer_id);
            self.network_state.reports.remove(&req.peer_id);
            self.ospf_states.remove(&req.peer_id);
            self.bump_topology_version();
        }
    }

    fn touch_peer(&mut self, peer_id: PeerId, unix_ms: u64) {
        if self.online_peers.contains_key(&peer_id) {
            self.online_peer_timestamps.insert(peer_id, unix_ms);
        }
    }

    fn expire_stale_online_peers(&mut self, now_unix_ms: u64) -> bool {
        let stale_peer_ids = self
            .online_peers
            .keys()
            .copied()
            .filter(|peer_id| {
                self.online_peer_timestamps
                    .get(peer_id)
                    .is_none_or(|last_seen| now_unix_ms.saturating_sub(*last_seen) > ONLINE_PEER_TTL_MS)
            })
            .collect::<Vec<_>>();

        if stale_peer_ids.is_empty() {
            return false;
        }

        for peer_id in stale_peer_ids {
            self.online_peers.remove(&peer_id);
            self.online_peer_timestamps.remove(&peer_id);
            self.network_state.reports.remove(&peer_id);
            self.ospf_states.remove(&peer_id);
        }
        self.bump_topology_version();
        true
    }

    fn lookup_peer(&self, peer_id: PeerId) -> LookupPeerResponse {
        LookupPeerResponse {
            shard_id: self.online_peers.get(&peer_id).cloned(),
        }
    }

    fn bump_topology_version(&mut self) {
        self.topology_version = self.topology_version.wrapping_add(1).max(1);
    }

    fn list_peers(&self) -> ListPeersResponse {
        ListPeersResponse {
            peers: self
                .online_peers
                .iter()
                .map(|(peer_id, shard_id)| (*peer_id, shard_id.clone()))
                .collect(),
            network_name: self.network_name.clone(),
            topology_version: self.topology_version.max(1),
        }
    }

    fn report_peers(&mut self, req: ReportPeersRequest) {
        self.touch_peer(req.peer_id, req.updated_at_unix_ms);
        let direct_peers = req.direct_peers.into_iter().collect::<BTreeMap<_, _>>();
        self.network_state.replace_report(PeerReport {
            peer_id: req.peer_id,
            direct_peers,
            updated_at: LastSeen {
                unix_ms: req.updated_at_unix_ms,
            },
        });
    }

    fn upsert_ospf_state(&mut self, req: UpsertOspfStateRequest) {
        let Some(parsed_frame) = parse_peer_manager_frame(&req.frame) else {
            return;
        };
        let Some((rpc, incoming_state)) =
            decode_ospf_sync_route_request_detail(parsed_frame.payload)
        else {
            return;
        };
        let domain_name = rpc.domain_name;
        self.touch_peer(req.peer_id, req.updated_at_unix_ms);

        self.ospf_states
            .entry(req.peer_id)
            .and_modify(|state| {
                if let Some(mut current) = decode_ospf_state_frame(&state.frame) {
                    merge_ospf_state(&mut current, incoming_state.clone());
                    state.frame = encode_ospf_state_frame(&domain_name, &current);
                } else {
                    state.frame = req.frame.clone();
                }
                state.updated_at_unix_ms = req.updated_at_unix_ms;
            })
            .or_insert(OspfPeerState {
                frame: req.frame,
                updated_at_unix_ms: req.updated_at_unix_ms,
            });
    }

    fn list_ospf_states(&self) -> ListOspfStatesResponse {
        ListOspfStatesResponse {
            states: self
                .ospf_states
                .iter()
                .filter_map(|(peer_id, state)| {
                    if !self.online_peers.contains_key(peer_id) {
                        return None;
                    }

                    if state.frame.is_empty() {
                        return None;
                    }

                    Some(OspfPeerStateView {
                        peer_id: *peer_id,
                        frame: state.frame.clone(),
                        updated_at_unix_ms: state.updated_at_unix_ms,
                    })
                })
                .collect(),
        }
    }

    #[cfg(test)]
    fn expire_reports_older_than(&mut self, min_unix_ms: u64) {
        self.network_state.expire_reports_older_than(min_unix_ms);
        self.ospf_states
            .retain(|_, state| state.updated_at_unix_ms >= min_unix_ms);
        self.online_peers.retain(|peer_id, _| {
            self.online_peer_timestamps
                .get(peer_id)
                .is_some_and(|updated_at| *updated_at >= min_unix_ms)
        });
        self.online_peer_timestamps
            .retain(|_, updated_at| *updated_at >= min_unix_ms);
    }

    fn discovery_snapshot(
        &self,
        req: GetDiscoverySnapshotRequest,
    ) -> Option<DiscoverySnapshotResponse> {
        let global_map = self.network_state.global_peer_map();
        let digest = compute_global_peer_map_digest(&global_map);

        if req.digest == digest && digest != 0 {
            return None;
        }

        let global_peer_map = global_map
            .peers
            .into_iter()
            .map(|(src_peer, peers)| (src_peer, peers.into_iter().collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        Some(DiscoverySnapshotResponse {
            digest,
            global_peer_map,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
struct NetworkDirectoryState {
    networks: BTreeMap<String, PerNetworkDirectoryState>,
}

impl NetworkDirectoryState {
    fn network_mut(&mut self, network_id: &str) -> &mut PerNetworkDirectoryState {
        self.networks.entry(network_id.to_string()).or_default()
    }

    fn network(&self, network_id: &str) -> Option<&PerNetworkDirectoryState> {
        self.networks.get(network_id)
    }

    fn register_peer(&mut self, req: RegisterPeerRequest) {
        self.network_mut(&req.network_id).register_peer(req);
    }

    fn expire_stale_online_peers(&mut self, now_unix_ms: u64) -> bool {
        let mut changed = false;
        for network in self.networks.values_mut() {
            changed |= network.expire_stale_online_peers(now_unix_ms);
        }
        changed
    }

    fn unregister_peer(&mut self, req: UnregisterPeerRequest) {
        if let Some(network) = self.networks.get_mut(&req.network_id) {
            network.unregister_peer(req);
        }
    }

    fn lookup_peer(&self, req: LookupPeerRequest) -> LookupPeerResponse {
        self.network(&req.network_id)
            .map(|network| network.lookup_peer(req.peer_id))
            .unwrap_or(LookupPeerResponse { shard_id: None })
    }

    fn list_peers(&self, req: ListPeersRequest) -> ListPeersResponse {
        self.network(&req.network_id)
            .map(PerNetworkDirectoryState::list_peers)
            .unwrap_or(ListPeersResponse {
                peers: Vec::new(),
                network_name: None,
                topology_version: 1,
            })
    }

    fn report_peers(&mut self, req: ReportPeersRequest) {
        self.network_mut(&req.network_id).report_peers(req);
    }

    fn upsert_ospf_state(&mut self, req: UpsertOspfStateRequest) {
        self.network_mut(&req.network_id).upsert_ospf_state(req);
    }

    fn list_ospf_states(&self, req: ListOspfStatesRequest) -> ListOspfStatesResponse {
        self.network(&req.network_id)
            .map(PerNetworkDirectoryState::list_ospf_states)
            .unwrap_or(ListOspfStatesResponse { states: Vec::new() })
    }

    fn discovery_snapshot(
        &self,
        req: GetDiscoverySnapshotRequest,
    ) -> Option<DiscoverySnapshotResponse> {
        self.network(&req.network_id)
            .and_then(|network| network.discovery_snapshot(req))
    }
}

async fn load_directory_state(storage: &Storage) -> Result<NetworkDirectoryState> {
    Ok(storage.get(DIRECTORY_STATE_KEY).await?.unwrap_or_default())
}

async fn save_directory_state(storage: &Storage, state: &NetworkDirectoryState) -> Result<()> {
    storage.put(DIRECTORY_STATE_KEY, state).await
}

#[durable_object]
pub struct NetworkDirectoryDO {
    state: State,
    env: Env,
    memory: Rc<RefCell<NetworkDirectoryState>>,
}

impl worker::DurableObject for NetworkDirectoryDO {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            memory: Rc::new(RefCell::new(NetworkDirectoryState::default())),
        }
    }

    async fn fetch(&self, mut req: Request) -> Result<Response> {
        let _ = (&self.state, &self.env);
        let path = req.path();

        let mut state = load_directory_state(&self.state.storage()).await?;
        if state.expire_stale_online_peers(Date::now().as_millis() as u64) {
            save_directory_state(&self.state.storage(), &state).await?;
        }
        *self.memory.borrow_mut() = state.clone();

        match (req.method(), path.as_str()) {
            (Method::Get, "/") => Response::ok("network-directory"),
            (Method::Post, "/register-peer") => {
                let payload: RegisterPeerRequest = serde_json::from_slice(&req.bytes().await?)?;
                state.register_peer(payload);
                save_directory_state(&self.state.storage(), &state).await?;
                *self.memory.borrow_mut() = state;
                Response::empty()
            }
            (Method::Post, "/unregister-peer") => {
                let payload: UnregisterPeerRequest = serde_json::from_slice(&req.bytes().await?)?;
                state.unregister_peer(payload);
                save_directory_state(&self.state.storage(), &state).await?;
                *self.memory.borrow_mut() = state;
                Response::empty()
            }
            (Method::Post, "/lookup-peer") => {
                let payload: LookupPeerRequest = serde_json::from_slice(&req.bytes().await?)?;
                Response::from_json(&state.lookup_peer(payload))
            }
            (Method::Post, "/list-peers") => {
                let payload: ListPeersRequest = serde_json::from_slice(&req.bytes().await?)?;
                Response::from_json(&state.list_peers(payload))
            }
            (Method::Post, "/report-peers") => {
                let payload: ReportPeersRequest = serde_json::from_slice(&req.bytes().await?)?;
                state.report_peers(payload);
                save_directory_state(&self.state.storage(), &state).await?;
                *self.memory.borrow_mut() = state;
                Response::empty()
            }
            (Method::Post, "/upsert-ospf-state") => {
                let payload: UpsertOspfStateRequest = serde_json::from_slice(&req.bytes().await?)?;
                state.upsert_ospf_state(payload);
                save_directory_state(&self.state.storage(), &state).await?;
                *self.memory.borrow_mut() = state;
                Response::empty()
            }
            (Method::Post, "/list-ospf-states") => {
                let payload: ListOspfStatesRequest = serde_json::from_slice(&req.bytes().await?)?;
                Response::from_json(&state.list_ospf_states(payload))
            }
            (Method::Post, "/discovery-snapshot") => {
                let payload: GetDiscoverySnapshotRequest =
                    serde_json::from_slice(&req.bytes().await?)?;
                match state.discovery_snapshot(payload) {
                    Some(snapshot) => Response::from_json(&snapshot),
                    None => Response::empty(),
                }
            }
            _ => Response::error("Not Found", 404),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_ospf_state_frame, NetworkDirectoryState};
    use crate::internal_api::{
        GetDiscoverySnapshotRequest, ListOspfStatesRequest, ListPeersRequest, LookupPeerRequest,
        RegisterPeerRequest, ReportPeersRequest, UnregisterPeerRequest, UpsertOspfStateRequest,
    };
    use crate::protocol::{encode_ospf_sync_route_request, OspfConnEntryView, OspfPeerInfoView};
    use crate::state::DirectConnectedPeerInfo;

    fn test_peer_info(peer_id: u32, version: u32, hostname: &str) -> OspfPeerInfoView {
        OspfPeerInfoView {
            peer_id,
            version,
            peer_route_id: peer_id as u64,
            ipv4_addr: None,
            proxy_cidrs: Vec::new(),
            hostname: Some(hostname.into()),
            udp_nat_type: 0,
            tcp_nat_type: 0,
            easytier_version: "2.5.0".into(),
            network_length: 24,
            ipv6_addr: None,
            noise_static_pubkey: Vec::new(),
            feature_flag: None,
            raw_payload: None,
        }
    }

    fn build_ospf_request(
        from_peer: u32,
        session_id: u64,
        peer_infos: Vec<OspfPeerInfoView>,
        conn_entries: Vec<OspfConnEntryView>,
    ) -> Vec<u8> {
        crate::protocol::build_peer_manager_frame(
            crate::protocol::PacketType::RpcReq,
            &encode_ospf_sync_route_request(
                "test-net",
                from_peer,
                1,
                session_id,
                false,
                &peer_infos,
                &conn_entries,
            ),
            from_peer,
            1,
        )
    }

    #[test]
    fn register_and_lookup_peer() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100,
        });

        assert_eq!(
            state
                .lookup_peer(LookupPeerRequest {
                    network_id: "net-a".into(),
                    peer_id: 1,
                })
                .shard_id
                .as_deref(),
            Some("shard-1")
        );
    }

    #[test]
    fn lookup_is_isolated_per_network() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100,
        });

        assert_eq!(
            state.lookup_peer(LookupPeerRequest {
                network_id: "net-b".into(),
                peer_id: 1,
            }),
            crate::internal_api::LookupPeerResponse { shard_id: None }
        );
    }

    #[test]
    fn reconnect_moves_peer_to_new_shard() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100,
        });
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-2".into(),
            connected_at_unix_ms: 200,
        });

        assert_eq!(
            state
                .lookup_peer(LookupPeerRequest {
                    network_id: "net-a".into(),
                    peer_id: 1,
                })
                .shard_id
                .as_deref(),
            Some("shard-2")
        );
    }

    #[test]
    fn unregister_only_removes_matching_shard_owner() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-2".into(),
            connected_at_unix_ms: 100,
        });
        state.unregister_peer(UnregisterPeerRequest {
            network_id: "net-a".into(),
            peer_id: 1,
            shard_id: "shard-1".into(),
        });

        assert_eq!(
            state
                .lookup_peer(LookupPeerRequest {
                    network_id: "net-a".into(),
                    peer_id: 1,
                })
                .shard_id
                .as_deref(),
            Some("shard-2")
        );
    }

    #[test]
    fn unregister_matching_owner_clears_report_and_ospf_state() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100,
        });
        state.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            direct_peers: vec![(8, DirectConnectedPeerInfo { latency_ms: 11 })],
            updated_at_unix_ms: 200,
        });
        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            frame: build_ospf_request(
                7,
                1,
                vec![test_peer_info(7, 1, "node-7")],
                vec![],
            ),
            updated_at_unix_ms: 200,
        });

        state.unregister_peer(UnregisterPeerRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
        });

        assert_eq!(
            state.list_ospf_states(ListOspfStatesRequest {
                network_id: "net-a".into(),
            }),
            crate::internal_api::ListOspfStatesResponse { states: Vec::new() }
        );
        let snapshot = state
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: 0,
            })
            .expect("snapshot should still be returned for a changed digest");
        assert!(snapshot.global_peer_map.is_empty());
    }

    #[test]
    fn report_peers_updates_discovery_state() {
        let mut state = NetworkDirectoryState::default();
        state.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            direct_peers: vec![(8, DirectConnectedPeerInfo { latency_ms: 11 })],
            updated_at_unix_ms: 200,
        });

        let snapshot = state
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: 0,
            })
            .unwrap();

        assert_eq!(snapshot.global_peer_map.len(), 1);
        assert_eq!(snapshot.global_peer_map[0].0, 7);
        assert_eq!(snapshot.global_peer_map[0].1[0].0, 8);
        assert_eq!(snapshot.global_peer_map[0].1[0].1.latency_ms, 11);
    }

    #[test]
    fn digest_match_returns_empty_update() {
        let mut state = NetworkDirectoryState::default();
        state.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            direct_peers: vec![(8, DirectConnectedPeerInfo { latency_ms: 11 })],
            updated_at_unix_ms: 200,
        });

        let first = state
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: 0,
            })
            .unwrap();

        assert!(state
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: first.digest,
            })
            .is_none());
    }

    #[test]
    fn per_network_state_projects_global_map() {
        let mut state = NetworkDirectoryState::default();
        state.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 1,
            direct_peers: vec![(2, DirectConnectedPeerInfo { latency_ms: 5 })],
            updated_at_unix_ms: 10,
        });
        state.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 3,
            direct_peers: vec![(4, DirectConnectedPeerInfo { latency_ms: 7 })],
            updated_at_unix_ms: 20,
        });

        let snapshot = state
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: 0,
            })
            .unwrap();

        assert_eq!(snapshot.global_peer_map.len(), 2);
    }

    #[test]
    fn expiring_reports_removes_old_entries() {
        let mut per_network = super::PerNetworkDirectoryState::default();
        per_network.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 1,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 50,
        });
        per_network.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 3,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 150,
        });
        per_network.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 1,
            direct_peers: vec![(2, DirectConnectedPeerInfo { latency_ms: 1 })],
            updated_at_unix_ms: 50,
        });
        per_network.report_peers(ReportPeersRequest {
            network_id: "net-a".into(),
            peer_id: 3,
            direct_peers: vec![(4, DirectConnectedPeerInfo { latency_ms: 2 })],
            updated_at_unix_ms: 150,
        });
        per_network.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 1,
            frame: build_ospf_request(
                1,
                1,
                vec![test_peer_info(1, 1, "node-1")],
                vec![],
            ),
            updated_at_unix_ms: 50,
        });
        per_network.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 3,
            frame: build_ospf_request(
                3,
                1,
                vec![test_peer_info(3, 1, "node-3")],
                vec![],
            ),
            updated_at_unix_ms: 150,
        });

        per_network.expire_reports_older_than(100);

        let snapshot = per_network
            .discovery_snapshot(GetDiscoverySnapshotRequest {
                network_id: "net-a".into(),
                digest: 0,
            })
            .unwrap();
        assert_eq!(snapshot.global_peer_map.len(), 1);
        assert_eq!(snapshot.global_peer_map[0].0, 3);

        let ospf = per_network.list_ospf_states();
        assert_eq!(ospf.states.len(), 1);
        assert_eq!(ospf.states[0].peer_id, 3);
    }

    #[test]
    fn expiring_online_peers_removes_stale_peer_entries_and_ospf_states() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 10,
        });
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 8,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100_000,
        });
        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            frame: build_ospf_request(7, 1, vec![test_peer_info(7, 1, "stale-peer")], vec![]),
            updated_at_unix_ms: 10,
        });
        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 8,
            frame: build_ospf_request(8, 1, vec![test_peer_info(8, 1, "live-peer")], vec![]),
            updated_at_unix_ms: 100_000,
        });

        assert!(state.expire_stale_online_peers(100_000));

        let peers = state.list_peers(ListPeersRequest {
            network_id: "net-a".into(),
        });
        assert_eq!(peers.peers, vec![(8, "shard-1".into())]);

        let ospf = state.list_ospf_states(ListOspfStatesRequest {
            network_id: "net-a".into(),
        });
        assert_eq!(ospf.states.len(), 1);
        assert_eq!(ospf.states[0].peer_id, 8);
    }

    #[test]
    fn upsert_and_list_ospf_states() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 200,
        });
        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            frame: build_ospf_request(
                7,
                1,
                vec![test_peer_info(7, 1, "node-7")],
                vec![],
            ),
            updated_at_unix_ms: 200,
        });

        let response = state.list_ospf_states(ListOspfStatesRequest {
            network_id: "net-a".into(),
        });

        assert_eq!(response.states.len(), 1);
        assert_eq!(response.states[0].peer_id, 7);
        let decoded = decode_ospf_state_frame(&response.states[0].frame).unwrap();
        assert_eq!(decoded.my_peer_id, 7);
        assert_eq!(decoded.peer_infos[0].hostname.as_deref(), Some("node-7"));
    }

    #[test]
    fn ospf_upsert_merges_incremental_updates_without_dropping_peer_info() {
        let mut state = NetworkDirectoryState::default();
        state.register_peer(RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 100,
        });

        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            frame: build_ospf_request(
                7,
                1,
                vec![test_peer_info(9, 4, "verify-a")],
                vec![OspfConnEntryView {
                    peer_id: 9,
                    version: 4,
                    connected_peer_ids: vec![1],
                }],
            ),
            updated_at_unix_ms: 100,
        });

        state.upsert_ospf_state(UpsertOspfStateRequest {
            network_id: "net-a".into(),
            peer_id: 7,
            frame: build_ospf_request(
                7,
                2,
                vec![],
                vec![
                    OspfConnEntryView {
                        peer_id: 7,
                        version: 5,
                        connected_peer_ids: vec![1, 9],
                    },
                ],
            ),
            updated_at_unix_ms: 200,
        });

        let response = state.list_ospf_states(ListOspfStatesRequest {
            network_id: "net-a".into(),
        });

        let state = decode_ospf_state_frame(&response.states[0].frame).unwrap();
        assert_eq!(state.my_session_id, 2);
        assert!(state
            .peer_infos
            .iter()
            .any(|info| info.peer_id == 9 && info.hostname.as_deref() == Some("verify-a")));
        assert!(state
            .conn_entries
            .iter()
            .any(|entry| entry.peer_id == 7 && entry.connected_peer_ids == vec![1, 9]));
    }
}
