use serde::{Deserialize, Serialize};

use crate::state::{DirectConnectedPeerInfo, PeerId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPeerRequest {
    pub network_id: String,
    pub network_name: String,
    pub peer_id: PeerId,
    pub shard_id: String,
    pub connected_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnregisterPeerRequest {
    pub network_id: String,
    pub peer_id: PeerId,
    pub shard_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupPeerRequest {
    pub network_id: String,
    pub peer_id: PeerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetDiscoverySnapshotRequest {
    pub network_id: String,
    pub digest: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupPeerResponse {
    pub shard_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPeersRequest {
    pub network_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPeersResponse {
    pub peers: Vec<(PeerId, String)>,
    pub network_name: Option<String>,
    pub topology_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportPeersRequest {
    pub network_id: String,
    pub peer_id: PeerId,
    pub direct_peers: Vec<(PeerId, DirectConnectedPeerInfo)>,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoverySnapshotResponse {
    pub digest: u64,
    pub global_peer_map: Vec<(PeerId, Vec<(PeerId, DirectConnectedPeerInfo)>)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertOspfStateRequest {
    pub network_id: String,
    pub peer_id: PeerId,
    pub frame: Vec<u8>,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OspfPeerStateView {
    pub peer_id: PeerId,
    pub frame: Vec<u8>,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListOspfStatesRequest {
    pub network_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListOspfStatesResponse {
    pub states: Vec<OspfPeerStateView>,
}
