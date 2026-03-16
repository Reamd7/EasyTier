use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub type PeerId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectConnectedPeerInfo {
    pub latency_ms: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastSeen {
    pub unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerReport {
    pub peer_id: PeerId,
    pub direct_peers: BTreeMap<PeerId, DirectConnectedPeerInfo>,
    pub updated_at: LastSeen,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GlobalPeerMap {
    pub peers: BTreeMap<PeerId, BTreeMap<PeerId, DirectConnectedPeerInfo>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NetworkState {
    pub reports: BTreeMap<PeerId, PeerReport>,
}

impl NetworkState {
    pub fn replace_report(&mut self, report: PeerReport) {
        self.reports.insert(report.peer_id, report);
    }

    pub fn expire_reports_older_than(&mut self, min_unix_ms: u64) {
        self.reports.retain(|_, report| report.updated_at.unix_ms >= min_unix_ms);
    }

    pub fn global_peer_map(&self) -> GlobalPeerMap {
        let mut peers = BTreeMap::new();

        for (peer_id, report) in &self.reports {
            peers.insert(*peer_id, report.direct_peers.clone());
        }

        GlobalPeerMap { peers }
    }
}
