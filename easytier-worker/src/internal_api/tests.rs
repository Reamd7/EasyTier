#[cfg(test)]
mod tests {
    use crate::internal_api::{
        BindPeerRequest, DeliverFrameToPeerRequest, DiscoverySnapshotResponse,
        ForwardFrameRequest, ForwardFrameResponse, GetDiscoverySnapshotRequest, LocalDelivery,
        LookupPeerRequest, LookupPeerResponse, RegisterPeerRequest, RelayFrameEnvelope,
        ReportPeersRequest, UnregisterPeerRequest,
    };
    use crate::state::DirectConnectedPeerInfo;

    #[test]
    fn register_peer_request_round_trips_as_json() {
        let req = RegisterPeerRequest {
            network_id: "net-a".into(),
            network_name: "test-net".into(),
            peer_id: 7,
            shard_id: "shard-1".into(),
            connected_at_unix_ms: 123,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: RegisterPeerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn unregister_peer_request_round_trips_as_json() {
        let req = UnregisterPeerRequest {
            network_id: "net-a".into(),
            peer_id: 8,
            shard_id: "shard-2".into(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: UnregisterPeerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn lookup_peer_response_round_trips_as_json() {
        let resp = LookupPeerResponse {
            shard_id: Some("shard-9".into()),
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: LookupPeerResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, resp);
    }

    #[test]
    fn lookup_peer_request_round_trips_as_json() {
        let req = LookupPeerRequest {
            network_id: "net-q".into(),
            peer_id: 9,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: LookupPeerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn report_peers_request_round_trips_as_json() {
        let req = ReportPeersRequest {
            network_id: "net-b".into(),
            peer_id: 42,
            direct_peers: vec![(9, DirectConnectedPeerInfo { latency_ms: 12 })],
            updated_at_unix_ms: 999,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: ReportPeersRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn discovery_snapshot_response_round_trips_as_json() {
        let resp = DiscoverySnapshotResponse {
            digest: 100,
            global_peer_map: vec![(1, vec![(2, DirectConnectedPeerInfo { latency_ms: 5 })])],
        };

        let json = serde_json::to_string(&resp).unwrap();
        let decoded: DiscoverySnapshotResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, resp);
    }

    #[test]
    fn get_discovery_snapshot_request_round_trips_as_json() {
        let req = GetDiscoverySnapshotRequest {
            network_id: "net-d".into(),
            digest: 1234,
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: GetDiscoverySnapshotRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn forward_frame_request_round_trips_as_json() {
        let req = ForwardFrameRequest {
            network_id: "net-c".into(),
            frame: RelayFrameEnvelope {
                src_peer_id: 1,
                dst_peer_id: 2,
                payload: vec![1, 2, 3],
            },
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: ForwardFrameRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn bind_peer_request_round_trips_as_json() {
        let req = BindPeerRequest {
            network_id: "net-e".into(),
            peer_id: 55,
            connection_id: "conn-1".into(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: BindPeerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn deliver_frame_to_peer_request_round_trips_as_json() {
        let req = DeliverFrameToPeerRequest {
            network_id: "net-f".into(),
            peer_id: 99,
            frame: vec![1, 2, 3, 4],
        };

        let json = serde_json::to_string(&req).unwrap();
        let decoded: DeliverFrameToPeerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn local_delivery_round_trips_as_json() {
        let delivery = LocalDelivery {
            connection_id: "conn-2".into(),
            frame: RelayFrameEnvelope {
                src_peer_id: 1,
                dst_peer_id: 2,
                payload: vec![9, 8, 7],
            },
        };

        let json = serde_json::to_string(&delivery).unwrap();
        let decoded: LocalDelivery = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, delivery);
    }

    #[test]
    fn forward_frame_response_round_trips_as_json() {
        let response = ForwardFrameResponse::DeliverLocal(LocalDelivery {
            connection_id: "conn-2".into(),
            frame: RelayFrameEnvelope {
                src_peer_id: 1,
                dst_peer_id: 2,
                payload: vec![9, 8, 7],
            },
        });

        let json = serde_json::to_string(&response).unwrap();
        let decoded: ForwardFrameResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, response);
    }
}
