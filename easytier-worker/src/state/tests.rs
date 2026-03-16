#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::state::{
        compute_global_peer_map_digest, DirectConnectedPeerInfo, LastSeen, NetworkState,
        PeerReport,
    };

    fn report(peer_id: u32, entries: &[(u32, i32)], updated_at: u64) -> PeerReport {
        let mut direct_peers = BTreeMap::new();
        for (dst, latency_ms) in entries {
            direct_peers.insert(*dst, DirectConnectedPeerInfo { latency_ms: *latency_ms });
        }

        PeerReport {
            peer_id,
            direct_peers,
            updated_at: LastSeen {
                unix_ms: updated_at,
            },
        }
    }

    #[test]
    fn replace_report_updates_only_the_target_peer() {
        let mut state = NetworkState::default();
        state.replace_report(report(1, &[(2, 10)], 100));
        state.replace_report(report(2, &[(1, 12)], 100));
        state.replace_report(report(1, &[(3, 20)], 200));

        let global = state.global_peer_map();
        assert_eq!(global.peers.len(), 2);
        assert_eq!(global.peers.get(&1).unwrap().len(), 1);
        assert!(global.peers.get(&1).unwrap().contains_key(&3));
        assert!(global.peers.get(&2).unwrap().contains_key(&1));
    }

    #[test]
    fn digest_is_stable_for_identical_logical_data() {
        let mut state_a = NetworkState::default();
        state_a.replace_report(report(1, &[(2, 10), (3, 20)], 100));
        state_a.replace_report(report(4, &[(5, 30)], 100));

        let mut state_b = NetworkState::default();
        state_b.replace_report(report(4, &[(5, 30)], 100));
        state_b.replace_report(report(1, &[(2, 10), (3, 20)], 100));

        let digest_a = compute_global_peer_map_digest(&state_a.global_peer_map());
        let digest_b = compute_global_peer_map_digest(&state_b.global_peer_map());

        assert_eq!(digest_a, digest_b);
    }

    #[test]
    fn expiring_outdated_reports_removes_them_from_global_map() {
        let mut state = NetworkState::default();
        state.replace_report(report(1, &[(2, 10)], 100));
        state.replace_report(report(3, &[(4, 11)], 250));

        state.expire_reports_older_than(200);

        let global = state.global_peer_map();
        assert_eq!(global.peers.len(), 1);
        assert!(global.peers.contains_key(&3));
        assert!(!global.peers.contains_key(&1));
    }
}
