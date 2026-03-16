#![allow(dead_code)]

mod frame;
mod network_identity;
mod peer_center;
mod tests;

pub(crate) use frame::{
    build_handshake_response_frame, build_peer_manager_frame, build_pong_frame, classify_incoming_frame,
    decode_handshake_request, extract_network_identity, extract_network_name, parse_peer_manager_frame,
    FrameClassification, IncomingFrame,
};
pub(crate) use frame::PacketType;
pub(crate) use network_identity::{network_id_from_name, normalized_network_name};
pub(crate) use peer_center::{
    decode_get_global_peer_map_request, decode_get_global_peer_map_response,
    decode_ospf_sync_route_info_request, decode_ospf_sync_route_request_detail,
    decode_ospf_sync_route_response,
    decode_report_peers_request, encode_empty_ospf_sync_route_response,
    encode_get_global_peer_map_response, encode_ospf_sync_route_request,
    encode_report_peers_response, inspect_rpc_payload, GlobalPeerMapView,
    OspfConnEntryView, OspfPeerFeatureFlagView, OspfPeerInfoView, OspfRouteStateView,
};
