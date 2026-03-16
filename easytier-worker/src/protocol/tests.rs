#[cfg(test)]
mod tests {
    use prost::Message;

    use crate::protocol::{
        build_handshake_response_frame, build_peer_manager_frame, build_pong_frame,
        classify_incoming_frame,
        decode_get_global_peer_map_request, decode_get_global_peer_map_response,
        decode_handshake_request, decode_ospf_sync_route_request_detail,
        decode_ospf_sync_route_response, decode_report_peers_request, extract_network_identity,
        network_id_from_name, normalized_network_name,
        encode_get_global_peer_map_response, encode_ospf_sync_route_request,
        encode_report_peers_response, GlobalPeerMapView, OspfConnEntryView,
        OspfPeerFeatureFlagView, OspfPeerInfoView, FrameClassification, IncomingFrame,
        PacketType,
    };
    use crate::state::DirectConnectedPeerInfo;

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

    #[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
    #[repr(i32)]
    enum CompressionAlgoPb {
        Invalid = 0,
        None = 1,
        Zstd = 2,
    }

    #[derive(Clone, PartialEq, Message)]
    struct RpcCompressionInfo {
        #[prost(enumeration = "CompressionAlgoPb", tag = "1")]
        pub algo: i32,
        #[prost(enumeration = "CompressionAlgoPb", tag = "2")]
        pub accepted_algo: i32,
    }

    #[derive(Clone, PartialEq, Message)]
    struct RpcRequest {
        #[prost(message, optional, tag = "1")]
        pub descriptor: Option<RpcDescriptor>,
        #[prost(bytes = "vec", tag = "2")]
        pub request: Vec<u8>,
        #[prost(int32, tag = "3")]
        pub timeout_ms: i32,
    }

    #[derive(Clone, PartialEq, Message)]
    struct RpcErrorInvalidService {
        #[prost(string, tag = "1")]
        pub service_name: String,
    }

    #[derive(Clone, PartialEq, Message)]
    struct RpcError {
        #[prost(oneof = "rpc_error::ErrorKind", tags = "3")]
        pub error_kind: Option<rpc_error::ErrorKind>,
    }

    mod rpc_error {
        use prost::Oneof;

        use super::RpcErrorInvalidService;

        #[derive(Clone, PartialEq, Oneof)]
        pub enum ErrorKind {
            #[prost(message, tag = "3")]
            InvalidService(RpcErrorInvalidService),
        }
    }

    #[derive(Clone, PartialEq, Message)]
    struct RpcResponse {
        #[prost(bytes = "vec", tag = "1")]
        pub response: Vec<u8>,
        #[prost(message, optional, tag = "2")]
        pub error: Option<RpcError>,
        #[prost(uint64, tag = "3")]
        pub runtime_us: u64,
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
        #[prost(uint32, tag = "7")]
        pub total_pieces: u32,
        #[prost(uint32, tag = "8")]
        pub piece_idx: u32,
        #[prost(int32, tag = "9")]
        pub trace_id: i32,
        #[prost(message, optional, tag = "10")]
        pub compression_info: Option<RpcCompressionInfo>,
    }

    #[derive(Clone, PartialEq, Message)]
    struct PbDirectConnectedPeerInfo {
        #[prost(int32, tag = "1")]
        pub latency_ms: i32,
    }

    #[derive(Clone, PartialEq, Message)]
    struct PbPeerInfoForGlobalMap {
        #[prost(map = "uint32, message", tag = "1")]
        pub direct_peers: std::collections::HashMap<u32, PbDirectConnectedPeerInfo>,
    }

    #[derive(Clone, PartialEq, Message)]
    struct PbReportPeersRequest {
        #[prost(uint32, tag = "1")]
        pub my_peer_id: u32,
        #[prost(message, optional, tag = "2")]
        pub peer_infos: Option<PbPeerInfoForGlobalMap>,
    }

    #[derive(Clone, PartialEq, Message)]
    struct HandshakeRequest {
        #[prost(uint32, tag = "1")]
        pub magic: u32,
        #[prost(uint32, tag = "2")]
        pub my_peer_id: u32,
        #[prost(uint32, tag = "3")]
        pub version: u32,
        #[prost(string, repeated, tag = "4")]
        pub features: Vec<String>,
        #[prost(string, tag = "5")]
        pub network_name: String,
        #[prost(bytes = "vec", tag = "6")]
        pub network_secret_digest: Vec<u8>,
    }

    fn build_test_peer_manager_frame(packet_type: u8, payload: Vec<u8>, from_peer: u32, to_peer: u32) -> Vec<u8> {
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

    fn build_rpc_payload(service_name: &str, method_index: u32) -> Vec<u8> {
        RpcPacket {
            from_peer: 1,
            to_peer: 2,
            transaction_id: 99,
            descriptor: Some(RpcDescriptor {
                domain_name: "network-a".into(),
                proto_name: "peer_rpc".into(),
                service_name: service_name.into(),
                method_index,
            }),
            body: RpcRequest {
                descriptor: None,
                request: Vec::new(),
                timeout_ms: 3000,
            }.encode_to_vec(),
            is_request: true,
            total_pieces: 0,
            piece_idx: 0,
            trace_id: 0,
            compression_info: Some(RpcCompressionInfo {
                algo: CompressionAlgoPb::None as i32,
                accepted_algo: CompressionAlgoPb::Zstd as i32,
            }),
        }
        .encode_to_vec()
    }

    fn build_report_peers_rpc_payload() -> Vec<u8> {
        RpcPacket {
            from_peer: 1,
            to_peer: 2,
            transaction_id: 99,
            descriptor: Some(RpcDescriptor {
                domain_name: "network-a".into(),
                proto_name: "peer_rpc".into(),
                service_name: "PeerCenterRpc".into(),
                method_index: 1,
            }),
            body: RpcRequest {
                descriptor: None,
                request: PbReportPeersRequest {
                    my_peer_id: 1,
                    peer_infos: Some(PbPeerInfoForGlobalMap {
                        direct_peers: [(
                            2,
                            PbDirectConnectedPeerInfo { latency_ms: 9 },
                        )]
                        .into_iter()
                        .collect(),
                    }),
                }
                .encode_to_vec(),
                timeout_ms: 3000,
            }
            .encode_to_vec(),
            is_request: true,
            total_pieces: 0,
            piece_idx: 0,
            trace_id: 0,
            compression_info: Some(RpcCompressionInfo {
                algo: CompressionAlgoPb::None as i32,
                accepted_algo: CompressionAlgoPb::Zstd as i32,
            }),
        }
        .encode_to_vec()
    }

    fn build_handshake_payload(network_name: &str) -> Vec<u8> {
        HandshakeRequest {
            magic: 1,
            my_peer_id: 1,
            version: 1,
            features: Vec::new(),
            network_name: network_name.into(),
            network_secret_digest: vec![0; 32],
        }
        .encode_to_vec()
    }

    #[test]
    fn rejects_non_binary_frames() {
        assert_eq!(
            classify_incoming_frame(IncomingFrame::Text("hello")),
            FrameClassification::Unknown
        );
    }

    #[test]
    fn derives_network_identity_from_name() {
        assert_eq!(normalized_network_name("  my-net  "), Some("my-net".to_string()));
        assert!(network_id_from_name("my-net").is_some());
        assert_eq!(network_id_from_name("   "), None);
    }

    #[test]
    fn classifies_data_frame_as_relay_data() {
        let frame = build_test_peer_manager_frame(PacketType::Data as u8, vec![1, 2, 3], 10, 20);

        match classify_incoming_frame(IncomingFrame::Binary(&frame)) {
            FrameClassification::RelayData(parsed) => {
                assert_eq!(parsed.header.from_peer_id, 10);
                assert_eq!(parsed.header.to_peer_id, 20);
            }
            other => panic!("unexpected classification: {other:?}"),
        }
    }

    #[test]
    fn classifies_peer_center_report_rpc() {
        let payload = build_rpc_payload("PeerCenterRpc", 1);
        let frame = build_test_peer_manager_frame(PacketType::RpcReq as u8, payload, 10, 20);

        assert!(matches!(
            classify_incoming_frame(IncomingFrame::Binary(&frame)),
            FrameClassification::PeerCenterReport(_)
        ));
    }

    #[test]
    fn classifies_peer_center_get_global_map_rpc() {
        let payload = build_rpc_payload("PeerCenterRpc", 2);
        let frame = build_test_peer_manager_frame(PacketType::RpcReq as u8, payload, 10, 20);

        assert!(matches!(
            classify_incoming_frame(IncomingFrame::Binary(&frame)),
            FrameClassification::PeerCenterGetGlobalPeerMap(_)
        ));
    }

    #[test]
    fn classifies_other_rpc_as_rpc_other() {
        let payload = build_rpc_payload("OtherService", 0);
        let frame = build_test_peer_manager_frame(PacketType::RpcReq as u8, payload, 10, 20);

        assert!(matches!(
            classify_incoming_frame(IncomingFrame::Binary(&frame)),
            FrameClassification::RpcOther(_)
        ));
    }

    #[test]
    fn classifies_ta_rpc_as_rpc_other() {
        let payload = build_rpc_payload("OtherService", 0);
        let frame = build_test_peer_manager_frame(PacketType::TaRpc as u8, payload, 10, 20);

        assert!(matches!(
            classify_incoming_frame(IncomingFrame::Binary(&frame)),
            FrameClassification::RpcOther(_)
        ));
    }

    #[test]
    fn extracts_network_identity_from_rpc_domain_name() {
        let payload = build_rpc_payload("PeerCenterRpc", 1);
        let frame = build_test_peer_manager_frame(PacketType::RpcReq as u8, payload, 10, 20);

        assert_eq!(
            extract_network_identity(&frame),
            network_id_from_name("network-a")
        );
    }

    #[test]
    fn extracts_network_identity_from_handshake_network_name() {
        let payload = build_handshake_payload("net-z");
        let frame = build_test_peer_manager_frame(PacketType::HandShake as u8, payload, 10, 0);

        assert_eq!(extract_network_identity(&frame), network_id_from_name("net-z"));
    }

    #[test]
    fn decodes_handshake_request() {
        let payload = build_handshake_payload("net-z");
        let frame = build_test_peer_manager_frame(PacketType::HandShake as u8, payload, 10, 0);

        let request = decode_handshake_request(&frame).unwrap();
        assert_eq!(request.my_peer_id, 1);
        assert_eq!(request.network_name, "net-z");
        assert_eq!(request.network_secret_digest.len(), 32);
    }

    #[test]
    fn builds_handshake_response_frame() {
        let payload = build_handshake_payload("net-z");
        let request = build_test_peer_manager_frame(PacketType::HandShake as u8, payload, 10, 0);

        let response = build_handshake_response_frame(&request, 7, "easytier-worker").unwrap();
        let parsed = decode_handshake_request(&response).unwrap();

        assert_eq!(parsed.my_peer_id, 7);
        assert_eq!(parsed.network_name, "easytier-worker");
        assert_eq!(parsed.network_secret_digest, vec![0; 32]);
    }

    #[test]
    fn builds_pong_from_ping() {
        let ping = build_test_peer_manager_frame(PacketType::Ping as u8, vec![1, 2, 3, 4], 10, 7);
        let pong = build_pong_frame(&ping).unwrap();
        let parsed = crate::protocol::parse_peer_manager_frame(&pong).unwrap();

        assert_eq!(parsed.header.packet_type, PacketType::Pong as u8);
        assert_eq!(parsed.header.from_peer_id, 10);
        assert_eq!(parsed.header.to_peer_id, 7);
        assert_eq!(parsed.payload, &[1, 2, 3, 4]);
    }

    #[test]
    fn decodes_report_peers_request_body() {
        let payload = build_report_peers_rpc_payload();
        let (request, body) = decode_report_peers_request(&payload).unwrap();

        assert_eq!(request.domain_name, "network-a");
        assert_eq!(body.peer_id, 1);
        assert_eq!(body.direct_peers.get(&2).map(|info| info.latency_ms), Some(9));
    }

    #[test]
    fn decodes_get_global_peer_map_request_body() {
        let payload = RpcPacket {
            from_peer: 1,
            to_peer: 2,
            transaction_id: 99,
            descriptor: Some(RpcDescriptor {
                domain_name: "network-a".into(),
                proto_name: "peer_rpc".into(),
                service_name: "PeerCenterRpc".into(),
                method_index: 2,
            }),
            body: RpcRequest {
                descriptor: None,
                request: vec![8, 123],
                timeout_ms: 3000,
            }.encode_to_vec(),
            is_request: true,
            total_pieces: 0,
            piece_idx: 0,
            trace_id: 0,
            compression_info: Some(RpcCompressionInfo {
                algo: CompressionAlgoPb::None as i32,
                accepted_algo: CompressionAlgoPb::Zstd as i32,
            }),
        }
        .encode_to_vec();

        let (request, body) = decode_get_global_peer_map_request(&payload).unwrap();
        assert_eq!(request.transaction_id, 99);
        assert_eq!(body.digest, 123);
    }

    #[test]
    fn decodes_ospf_response_rpc_error() {
        #[derive(Clone, PartialEq, Message)]
        struct PbSyncRouteInfoResponse {
            #[prost(bool, tag = "1")]
            pub is_initiator: bool,
            #[prost(uint64, tag = "2")]
            pub session_id: u64,
            #[prost(int32, optional, tag = "3")]
            pub error: Option<i32>,
        }

        let payload = RpcPacket {
            from_peer: 1,
            to_peer: 2,
            transaction_id: 99,
            descriptor: Some(RpcDescriptor {
                domain_name: "network-a".into(),
                proto_name: "peer_rpc".into(),
                service_name: "OspfRouteRpc".into(),
                method_index: 0,
            }),
            body: RpcResponse {
                response: PbSyncRouteInfoResponse {
                    is_initiator: false,
                    session_id: 0,
                    error: None,
                }
                .encode_to_vec(),
                error: Some(RpcError {
                    error_kind: Some(rpc_error::ErrorKind::InvalidService(
                        RpcErrorInvalidService {
                            service_name: "OspfRouteRpc".into(),
                        },
                    )),
                }),
                runtime_us: 0,
            }
            .encode_to_vec(),
            is_request: false,
            total_pieces: 0,
            piece_idx: 0,
            trace_id: 0,
            compression_info: Some(RpcCompressionInfo {
                algo: CompressionAlgoPb::None as i32,
                accepted_algo: CompressionAlgoPb::Zstd as i32,
            }),
        }
        .encode_to_vec();

        let (_rpc, resp) = decode_ospf_sync_route_response(&payload).unwrap();
        assert_eq!(resp.session_id, 0);
        assert_eq!(resp.rpc_error.as_deref(), Some("invalid_service:OspfRouteRpc"));
    }

    #[test]
    fn encodes_synthesized_ospf_request_with_shared_node_metadata() {
        let payload = encode_ospf_sync_route_request(
            "network-a",
            1,
            22,
            1,
            false,
            &[
                OspfPeerInfoView {
                    peer_id: 1,
                    version: 1,
                    peer_route_id: 1,
                    ipv4_addr: Some(u32::from_be_bytes([10, 126, 126, 254])),
                    proxy_cidrs: vec!["10.10.0.0/24".into()],
                    hostname: Some("easytier-worker".into()),
                    udp_nat_type: 0,
                    tcp_nat_type: 0,
                    easytier_version: "cf-worker:network-a".into(),
                    network_length: 24,
                    ipv6_addr: None,
                    noise_static_pubkey: Vec::new(),
                    feature_flag: Some(OspfPeerFeatureFlagView {
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
                },
                OspfPeerInfoView {
                    peer_id: 11,
                    version: 3,
                    peer_route_id: 11,
                    ipv4_addr: Some(u32::from_be_bytes([10, 126, 126, 11])),
                    proxy_cidrs: Vec::new(),
                    hostname: Some("node-11".into()),
                    udp_nat_type: 0,
                    tcp_nat_type: 0,
                    easytier_version: "1.2.3".into(),
                    network_length: 24,
                    ipv6_addr: None,
                    noise_static_pubkey: vec![1, 2, 3],
                    feature_flag: None,
                    raw_payload: None,
                },
            ],
            &[
                OspfConnEntryView {
                    peer_id: 1,
                    version: 1,
                    connected_peer_ids: vec![11, 22],
                },
                OspfConnEntryView {
                    peer_id: 11,
                    version: 3,
                    connected_peer_ids: vec![1],
                },
            ],
        );
        let frame = build_peer_manager_frame(PacketType::RpcReq, &payload, 1, 22);
        let parsed = crate::protocol::parse_peer_manager_frame(&frame).unwrap();

        let (rpc, body) = decode_ospf_sync_route_request_detail(parsed.payload).unwrap();
        assert_eq!(rpc.service_name, "OspfRouteRpc");
        assert_eq!(body.my_peer_id, 1);
        assert_eq!(body.my_session_id, 1);
        assert_eq!(body.peer_infos.len(), 2);
        assert_eq!(
            body.peer_infos
                .iter()
                .find(|info| info.peer_id == 11)
                .and_then(|info| info.ipv4_addr),
            Some(u32::from_be_bytes([10, 126, 126, 11]))
        );
        assert!(body
            .peer_infos
            .iter()
            .find(|info| info.peer_id == 1)
            .and_then(|info| info.feature_flag.as_ref())
            .is_some_and(|flag| flag.is_public_server && flag.avoid_relay_data));
        assert!(body
            .peer_infos
            .iter()
            .find(|info| info.peer_id == 1)
            .is_some_and(|info| info.proxy_cidrs == vec!["10.10.0.0/24"]));
        assert!(body
            .conn_entries
            .iter()
            .find(|entry| entry.peer_id == 1)
            .is_some_and(|entry| entry.connected_peer_ids == vec![11, 22]));
    }

    #[test]
    fn encodes_peer_center_responses() {
        let payload = build_report_peers_rpc_payload();
        let (request, _) = decode_report_peers_request(&payload).unwrap();

        let report_response = encode_report_peers_response(&request, 2, 1);
        assert!(!report_response.is_empty());

        let map_response = encode_get_global_peer_map_response(
            &request,
            2,
            1,
            GlobalPeerMapView {
                digest: Some(7),
                peers: [(
                    1,
                    [(2, DirectConnectedPeerInfo { latency_ms: 9 })]
                        .into_iter()
                        .collect(),
                )]
                .into_iter()
                .collect(),
            },
        );
        let (response, body) = decode_get_global_peer_map_response(&map_response).unwrap();
        assert_eq!(response.transaction_id, request.transaction_id);
        assert_eq!(body.digest, Some(7));
        assert_eq!(
            body.peers
                .get(&1)
                .and_then(|peers| peers.get(&2))
                .map(|info| info.latency_ms),
            Some(9)
        );
    }
}
