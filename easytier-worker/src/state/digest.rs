use std::hash::{Hash, Hasher};

use crate::state::GlobalPeerMap;

pub fn compute_global_peer_map_digest(global_peer_map: &GlobalPeerMap) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    for (src_peer, direct_peers) in &global_peer_map.peers {
        src_peer.hash(&mut hasher);

        for (dst_peer, info) in direct_peers {
            dst_peer.hash(&mut hasher);
            info.latency_ms.hash(&mut hasher);
        }
    }

    hasher.finish()
}
