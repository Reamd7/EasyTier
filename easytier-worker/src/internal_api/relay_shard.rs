use serde::{Deserialize, Serialize};

use crate::state::PeerId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindPeerRequest {
    pub network_id: String,
    pub peer_id: PeerId,
    pub connection_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayFrameEnvelope {
    pub src_peer_id: PeerId,
    pub dst_peer_id: PeerId,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardFrameRequest {
    pub network_id: String,
    pub frame: RelayFrameEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDelivery {
    pub connection_id: String,
    pub frame: RelayFrameEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverFrameToPeerRequest {
    pub network_id: String,
    pub peer_id: PeerId,
    pub frame: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ForwardFrameResponse {
    DeliverLocal(LocalDelivery),
    ForwardToDirectory(RelayFrameEnvelope),
    Ignore,
}
