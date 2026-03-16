use std::collections::{BTreeMap, HashMap};

use prost::Message;
use serde::{Deserialize, Serialize};

use crate::state::{DirectConnectedPeerInfo, PeerId};

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
struct RpcCompressionInfo {
    #[prost(enumeration = "CompressionAlgoPb", tag = "1")]
    pub algo: i32,
    #[prost(enumeration = "CompressionAlgoPb", tag = "2")]
    pub accepted_algo: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
enum CompressionAlgoPb {
    Invalid = 0,
    None = 1,
    Zstd = 2,
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
struct RpcErrorOther {
    #[prost(string, tag = "1")]
    pub error_message: String,
}

#[derive(Clone, PartialEq, Message)]
struct RpcErrorInvalidMethodIndex {
    #[prost(string, tag = "1")]
    pub service_name: String,
    #[prost(uint32, tag = "2")]
    pub method_index: u32,
}

#[derive(Clone, PartialEq, Message)]
struct RpcErrorInvalidService {
    #[prost(string, tag = "1")]
    pub service_name: String,
}

#[derive(Clone, PartialEq, Message)]
struct RpcErrorExecuteError {
    #[prost(string, tag = "1")]
    pub error_message: String,
}

#[derive(Clone, PartialEq, Message)]
struct RpcErrorMalformatRpcPacket {
    #[prost(string, tag = "1")]
    pub error_message: String,
}

#[derive(Clone, PartialEq, Message)]
struct RpcErrorTimeout {
    #[prost(string, tag = "1")]
    pub error_message: String,
}

#[derive(Clone, PartialEq, Message)]
struct RpcError {
    #[prost(oneof = "rpc_error::ErrorKind", tags = "1, 2, 3, 4, 5, 6, 7, 8")]
    pub error_kind: Option<rpc_error::ErrorKind>,
}

mod rpc_error {
    use prost::Oneof;

    use super::{
        RpcErrorExecuteError, RpcErrorInvalidMethodIndex, RpcErrorInvalidService,
        RpcErrorMalformatRpcPacket, RpcErrorOther, RpcErrorTimeout,
    };

    #[derive(Clone, PartialEq, Oneof)]
    pub enum ErrorKind {
        #[prost(message, tag = "1")]
        OtherError(RpcErrorOther),
        #[prost(message, tag = "2")]
        InvalidMethodIndex(RpcErrorInvalidMethodIndex),
        #[prost(message, tag = "3")]
        InvalidService(RpcErrorInvalidService),
        #[prost(message, tag = "4")]
        ProstDecodeError(()),
        #[prost(message, tag = "5")]
        ProstEncodeError(()),
        #[prost(message, tag = "6")]
        ExecuteError(RpcErrorExecuteError),
        #[prost(message, tag = "7")]
        MalformatRpcPacket(RpcErrorMalformatRpcPacket),
        #[prost(message, tag = "8")]
        Timeout(RpcErrorTimeout),
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
    pub direct_peers: HashMap<u32, PbDirectConnectedPeerInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct PbReportPeersRequest {
    #[prost(uint32, tag = "1")]
    pub my_peer_id: u32,
    #[prost(message, optional, tag = "2")]
    pub peer_infos: Option<PbPeerInfoForGlobalMap>,
}

#[derive(Clone, PartialEq, Message)]
struct PbReportPeersResponse {}

#[derive(Clone, PartialEq, Message)]
struct PbGetGlobalPeerMapRequest {
    #[prost(uint64, tag = "1")]
    pub digest: u64,
}

#[derive(Clone, PartialEq, Message)]
struct PbGetGlobalPeerMapResponse {
    #[prost(map = "uint32, message", tag = "1")]
    pub global_peer_map: HashMap<u32, PbPeerInfoForGlobalMap>,
    #[prost(uint64, optional, tag = "2")]
    pub digest: Option<u64>,
}

#[derive(Clone, PartialEq, Message)]
struct PbPeerFeatureFlag {
    #[prost(bool, tag = "1")]
    pub is_public_server: bool,
    #[prost(bool, tag = "2")]
    pub avoid_relay_data: bool,
    #[prost(bool, tag = "3")]
    pub kcp_input: bool,
    #[prost(bool, tag = "4")]
    pub no_relay_kcp: bool,
    #[prost(bool, tag = "5")]
    pub support_conn_list_sync: bool,
    #[prost(bool, tag = "6")]
    pub quic_input: bool,
    #[prost(bool, tag = "7")]
    pub no_relay_quic: bool,
    #[prost(bool, tag = "8")]
    pub is_credential_peer: bool,
}

#[derive(Clone, PartialEq, Message)]
struct PbTimestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

#[derive(Clone, PartialEq, Message)]
struct PbUuid {
    #[prost(uint32, tag = "1")]
    pub part1: u32,
    #[prost(uint32, tag = "2")]
    pub part2: u32,
    #[prost(uint32, tag = "3")]
    pub part3: u32,
    #[prost(uint32, tag = "4")]
    pub part4: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PbIpv4Addr {
    #[prost(uint32, tag = "1")]
    pub addr: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PbIpv6Addr {
    #[prost(uint32, tag = "1")]
    pub part1: u32,
    #[prost(uint32, tag = "2")]
    pub part2: u32,
    #[prost(uint32, tag = "3")]
    pub part3: u32,
    #[prost(uint32, tag = "4")]
    pub part4: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PbIpv6Inet {
    #[prost(message, optional, tag = "1")]
    pub address: Option<PbIpv6Addr>,
    #[prost(uint32, tag = "2")]
    pub network_length: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PbRoutePeerInfo {
    #[prost(uint32, tag = "1")]
    pub peer_id: u32,
    #[prost(message, optional, tag = "2")]
    pub inst_id: Option<PbUuid>,
    #[prost(uint32, tag = "3")]
    pub cost: u32,
    #[prost(message, optional, tag = "4")]
    pub ipv4_addr: Option<PbIpv4Addr>,
    #[prost(string, repeated, tag = "5")]
    pub proxy_cidrs: Vec<String>,
    #[prost(string, optional, tag = "6")]
    pub hostname: Option<String>,
    #[prost(int32, tag = "7")]
    pub udp_nat_type: i32,
    #[prost(message, optional, tag = "8")]
    pub last_update: Option<PbTimestamp>,
    #[prost(uint32, tag = "9")]
    pub version: u32,
    #[prost(string, tag = "10")]
    pub easytier_version: String,
    #[prost(message, optional, tag = "11")]
    pub feature_flag: Option<PbPeerFeatureFlag>,
    #[prost(uint64, tag = "12")]
    pub peer_route_id: u64,
    #[prost(uint32, tag = "13")]
    pub network_length: u32,
    #[prost(uint32, optional, tag = "14")]
    pub quic_port: Option<u32>,
    #[prost(message, optional, tag = "15")]
    pub ipv6_addr: Option<PbIpv6Inet>,
    #[prost(int32, tag = "17")]
    pub tcp_nat_type: i32,
    #[prost(bytes = "vec", tag = "18")]
    pub noise_static_pubkey: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct PbRoutePeerInfos {
    #[prost(message, repeated, tag = "1")]
    pub items: Vec<PbRoutePeerInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct PbPeerIdVersion {
    #[prost(uint32, tag = "1")]
    pub peer_id: u32,
    #[prost(uint32, tag = "2")]
    pub version: u32,
}

#[derive(Clone, PartialEq, Message)]
struct PbRouteConnBitmap {
    #[prost(message, repeated, tag = "1")]
    pub peer_ids: Vec<PbPeerIdVersion>,
    #[prost(bytes = "vec", tag = "2")]
    pub bitmap: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
struct PbRouteConnPeerListPeerConnInfo {
    #[prost(message, optional, tag = "1")]
    pub peer_id: Option<PbPeerIdVersion>,
    #[prost(uint32, repeated, tag = "2")]
    pub connected_peer_ids: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
struct PbRouteConnPeerList {
    #[prost(message, repeated, tag = "1")]
    pub peer_conn_infos: Vec<PbRouteConnPeerListPeerConnInfo>,
}

#[derive(Clone, PartialEq, Message)]
struct PbRouteForeignNetworkInfos {}

#[derive(Clone, PartialEq, Message)]
struct PbSyncRouteInfoRequest {
    #[prost(uint32, tag = "1")]
    pub my_peer_id: u32,
    #[prost(uint64, tag = "2")]
    pub my_session_id: u64,
    #[prost(bool, tag = "3")]
    pub is_initiator: bool,
    #[prost(message, optional, tag = "4")]
    pub peer_infos: Option<PbRoutePeerInfos>,
    #[prost(oneof = "pb_sync_route_info_request::ConnInfo", tags = "5, 7")]
    pub conn_info: Option<pb_sync_route_info_request::ConnInfo>,
    #[prost(message, optional, tag = "6")]
    pub foreign_network_infos: Option<PbRouteForeignNetworkInfos>,
}

mod pb_sync_route_info_request {
    use prost::Oneof;

    use super::{PbRouteConnBitmap, PbRouteConnPeerList};

    #[derive(Clone, PartialEq, Oneof)]
    pub enum ConnInfo {
        #[prost(message, tag = "5")]
        ConnBitmap(PbRouteConnBitmap),
        #[prost(message, tag = "7")]
        ConnPeerList(PbRouteConnPeerList),
    }
}

#[derive(Clone, PartialEq, Message)]
struct PbSyncRouteInfoResponse {
    #[prost(bool, tag = "1")]
    pub is_initiator: bool,
    #[prost(uint64, tag = "2")]
    pub session_id: u64,
    #[prost(int32, optional, tag = "3")]
    pub error: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethodKind {
    PeerCenterReportPeers,
    PeerCenterGetGlobalPeerMap,
    OspfSyncRouteInfo,
    Other(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcDescriptorView {
    pub from_peer: PeerId,
    pub to_peer: PeerId,
    pub domain_name: String,
    pub proto_name: String,
    pub service_name: String,
    pub method_index: u32,
    pub method_kind: RpcMethodKind,
    pub is_request: bool,
    pub transaction_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportPeersView {
    pub peer_id: PeerId,
    pub direct_peers: BTreeMap<PeerId, DirectConnectedPeerInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetGlobalPeerMapView {
    pub digest: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalPeerMapView {
    pub digest: Option<u64>,
    pub peers: BTreeMap<PeerId, BTreeMap<PeerId, DirectConnectedPeerInfo>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcClassification {
    PeerCenterReport(RpcDescriptorView),
    PeerCenterGetGlobalPeerMap(RpcDescriptorView),
    Other(RpcDescriptorView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OspfPeerFeatureFlagView {
    pub is_public_server: bool,
    pub avoid_relay_data: bool,
    pub kcp_input: bool,
    pub no_relay_kcp: bool,
    pub support_conn_list_sync: bool,
    pub quic_input: bool,
    pub no_relay_quic: bool,
    pub is_credential_peer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OspfPeerInfoView {
    pub peer_id: PeerId,
    pub version: u32,
    pub peer_route_id: u64,
    pub ipv4_addr: Option<u32>,
    pub proxy_cidrs: Vec<String>,
    pub hostname: Option<String>,
    pub udp_nat_type: i32,
    pub tcp_nat_type: i32,
    pub easytier_version: String,
    pub network_length: u32,
    pub ipv6_addr: Option<(u32, u32, u32, u32, u32)>,
    pub noise_static_pubkey: Vec<u8>,
    pub feature_flag: Option<OspfPeerFeatureFlagView>,
    pub raw_payload: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OspfConnEntryView {
    pub peer_id: PeerId,
    pub version: u32,
    pub connected_peer_ids: Vec<PeerId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OspfRouteStateView {
    pub my_peer_id: PeerId,
    pub my_session_id: u64,
    pub is_initiator: bool,
    pub peer_infos: Vec<OspfPeerInfoView>,
    pub conn_entries: Vec<OspfConnEntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OspfSyncRouteInfoView {
    pub my_peer_id: PeerId,
    pub my_session_id: u64,
    pub is_initiator: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OspfSyncRouteResponseView {
    pub is_initiator: bool,
    pub session_id: u64,
    pub error: Option<i32>,
    pub rpc_error: Option<String>,
}

fn decode_rpc_packet(payload: &[u8]) -> Option<RpcPacket> {
    let packet = RpcPacket::decode(payload).ok()?;
    if packet.total_pieces > 1 || packet.piece_idx > 0 {
        return None;
    }
    Some(packet)
}

fn decode_rpc_request_body(packet: &RpcPacket) -> Option<Vec<u8>> {
    if !packet.is_request {
        return None;
    }
    let request = RpcRequest::decode(packet.body.as_slice()).ok()?;
    Some(request.request)
}

fn rpc_error_to_string(error: &RpcError) -> Option<String> {
    use rpc_error::ErrorKind;

    match error.error_kind.as_ref()? {
        ErrorKind::OtherError(err) => Some(format!("other:{}", err.error_message)),
        ErrorKind::InvalidMethodIndex(err) => Some(format!(
            "invalid_method:{}:{}",
            err.service_name, err.method_index
        )),
        ErrorKind::InvalidService(err) => Some(format!("invalid_service:{}", err.service_name)),
        ErrorKind::ProstDecodeError(()) => Some("prost_decode_error".to_string()),
        ErrorKind::ProstEncodeError(()) => Some("prost_encode_error".to_string()),
        ErrorKind::ExecuteError(err) => Some(format!("execute:{}", err.error_message)),
        ErrorKind::MalformatRpcPacket(err) => Some(format!("malformat:{}", err.error_message)),
        ErrorKind::Timeout(err) => Some(format!("timeout:{}", err.error_message)),
    }
}

fn decode_rpc_response(packet: &RpcPacket) -> Option<RpcResponse> {
    if packet.is_request {
        return None;
    }
    RpcResponse::decode(packet.body.as_slice()).ok()
}

fn decode_rpc_response_body(packet: &RpcPacket) -> Option<Vec<u8>> {
    let response = decode_rpc_response(packet)?;
    Some(response.response)
}

fn rpc_method_kind(service_name: &str, method_index: u32) -> RpcMethodKind {
    match (service_name, method_index) {
        ("PeerCenterRpc", 1) => RpcMethodKind::PeerCenterReportPeers,
        ("PeerCenterRpc", 2) => RpcMethodKind::PeerCenterGetGlobalPeerMap,
        ("OspfRouteRpc", 1) => RpcMethodKind::OspfSyncRouteInfo,
        (_, n) => RpcMethodKind::Other(n),
    }
}

fn descriptor_view(packet: &RpcPacket, descriptor: RpcDescriptor) -> RpcDescriptorView {
    let method_index = descriptor.method_index;
    let method_kind = rpc_method_kind(&descriptor.service_name, method_index);

    RpcDescriptorView {
        from_peer: packet.from_peer,
        to_peer: packet.to_peer,
        domain_name: descriptor.domain_name,
        proto_name: descriptor.proto_name,
        service_name: descriptor.service_name,
        method_index,
        method_kind,
        is_request: packet.is_request,
        transaction_id: packet.transaction_id,
    }
}

fn feature_flag_view(flag: PbPeerFeatureFlag) -> OspfPeerFeatureFlagView {
    OspfPeerFeatureFlagView {
        is_public_server: flag.is_public_server,
        avoid_relay_data: flag.avoid_relay_data,
        kcp_input: flag.kcp_input,
        no_relay_kcp: flag.no_relay_kcp,
        support_conn_list_sync: flag.support_conn_list_sync,
        quic_input: flag.quic_input,
        no_relay_quic: flag.no_relay_quic,
        is_credential_peer: flag.is_credential_peer,
    }
}

fn feature_flag_proto(flag: OspfPeerFeatureFlagView) -> PbPeerFeatureFlag {
    PbPeerFeatureFlag {
        is_public_server: flag.is_public_server,
        avoid_relay_data: flag.avoid_relay_data,
        kcp_input: flag.kcp_input,
        no_relay_kcp: flag.no_relay_kcp,
        support_conn_list_sync: flag.support_conn_list_sync,
        quic_input: flag.quic_input,
        no_relay_quic: flag.no_relay_quic,
        is_credential_peer: flag.is_credential_peer,
    }
}

fn route_peer_info_view(info: PbRoutePeerInfo, raw_payload: Option<Vec<u8>>) -> OspfPeerInfoView {
    OspfPeerInfoView {
        peer_id: info.peer_id,
        version: info.version,
        peer_route_id: info.peer_route_id,
        ipv4_addr: info.ipv4_addr.map(|addr| addr.addr),
        proxy_cidrs: info.proxy_cidrs,
        hostname: info.hostname,
        udp_nat_type: info.udp_nat_type,
        tcp_nat_type: info.tcp_nat_type,
        easytier_version: info.easytier_version,
        network_length: info.network_length,
        ipv6_addr: info.ipv6_addr.map(|addr| {
            let ip = addr.address.unwrap_or(PbIpv6Addr {
                part1: 0,
                part2: 0,
                part3: 0,
                part4: 0,
            });
            (
                ip.part1,
                ip.part2,
                ip.part3,
                ip.part4,
                addr.network_length,
            )
        }),
        noise_static_pubkey: info.noise_static_pubkey,
        feature_flag: info.feature_flag.map(feature_flag_view),
        raw_payload,
    }
}

fn route_peer_info_proto(info: OspfPeerInfoView) -> PbRoutePeerInfo {
    PbRoutePeerInfo {
        peer_id: info.peer_id,
        inst_id: None,
        cost: 0,
        ipv4_addr: info.ipv4_addr.map(|addr| PbIpv4Addr { addr }),
        proxy_cidrs: info.proxy_cidrs,
        hostname: info.hostname,
        udp_nat_type: info.udp_nat_type,
        last_update: None,
        version: info.version,
        easytier_version: info.easytier_version,
        feature_flag: info.feature_flag.map(feature_flag_proto),
        peer_route_id: info.peer_route_id,
        network_length: info.network_length,
        quic_port: None,
        ipv6_addr: info.ipv6_addr.map(
            |(part1, part2, part3, part4, network_length)| PbIpv6Inet {
                address: Some(PbIpv6Addr {
                    part1,
                    part2,
                    part3,
                    part4,
                }),
                network_length,
            },
        ),
        tcp_nat_type: info.tcp_nat_type,
        noise_static_pubkey: info.noise_static_pubkey,
    }
}

fn conn_entries_from_conn_info(conn_info: Option<pb_sync_route_info_request::ConnInfo>) -> Vec<OspfConnEntryView> {
    match conn_info {
        Some(pb_sync_route_info_request::ConnInfo::ConnPeerList(list)) => list
            .peer_conn_infos
            .into_iter()
            .filter_map(|entry| {
                let peer = entry.peer_id?;
                Some(OspfConnEntryView {
                    peer_id: peer.peer_id,
                    version: peer.version,
                    connected_peer_ids: entry.connected_peer_ids,
                })
            })
            .collect(),
        Some(pb_sync_route_info_request::ConnInfo::ConnBitmap(bitmap)) => bitmap
            .peer_ids
            .into_iter()
            .map(|peer| OspfConnEntryView {
                peer_id: peer.peer_id,
                version: peer.version,
                connected_peer_ids: Vec::new(),
            })
            .collect(),
        None => Vec::new(),
    }
}

pub fn classify_rpc_payload(payload: &[u8]) -> Option<RpcClassification> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = packet.descriptor.clone()?;
    let view = descriptor_view(&packet, descriptor);

    if view.service_name != "PeerCenterRpc" {
        return Some(RpcClassification::Other(view));
    }

    Some(match view.method_kind {
        RpcMethodKind::PeerCenterReportPeers => RpcClassification::PeerCenterReport(view),
        RpcMethodKind::PeerCenterGetGlobalPeerMap => RpcClassification::PeerCenterGetGlobalPeerMap(view),
        RpcMethodKind::OspfSyncRouteInfo | RpcMethodKind::Other(_) => RpcClassification::Other(view),
    })
}

pub fn inspect_rpc_payload(payload: &[u8]) -> Option<RpcDescriptorView> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = packet.descriptor.clone()?;
    Some(descriptor_view(&packet, descriptor))
}

pub fn decode_report_peers_request(payload: &[u8]) -> Option<(RpcDescriptorView, ReportPeersView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "PeerCenterRpc"
        || descriptor.method_kind != RpcMethodKind::PeerCenterReportPeers
        || !descriptor.is_request
    {
        return None;
    }

    let body = PbReportPeersRequest::decode(decode_rpc_request_body(&packet)?.as_slice()).ok()?;
    let direct_peers = body
        .peer_infos
        .unwrap_or(PbPeerInfoForGlobalMap {
            direct_peers: HashMap::new(),
        })
        .direct_peers
        .into_iter()
        .map(|(peer_id, info)| {
            (
                peer_id,
                DirectConnectedPeerInfo {
                    latency_ms: info.latency_ms,
                },
            )
        })
        .collect();

    Some((
        descriptor,
        ReportPeersView {
            peer_id: body.my_peer_id,
            direct_peers,
        },
    ))
}

pub fn decode_get_global_peer_map_request(
    payload: &[u8],
) -> Option<(RpcDescriptorView, GetGlobalPeerMapView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "PeerCenterRpc"
        || descriptor.method_kind != RpcMethodKind::PeerCenterGetGlobalPeerMap
        || !descriptor.is_request
    {
        return None;
    }

    let body = PbGetGlobalPeerMapRequest::decode(decode_rpc_request_body(&packet)?.as_slice()).ok()?;
    Some((descriptor, GetGlobalPeerMapView { digest: body.digest }))
}

pub fn decode_ospf_sync_route_info_request(
    payload: &[u8],
) -> Option<(RpcDescriptorView, OspfSyncRouteInfoView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "OspfRouteRpc" || !descriptor.is_request {
        return None;
    }

    let body = PbSyncRouteInfoRequest::decode(decode_rpc_request_body(&packet)?.as_slice()).ok()?;
    Some((
        descriptor,
        OspfSyncRouteInfoView {
            my_peer_id: body.my_peer_id,
            my_session_id: body.my_session_id,
            is_initiator: body.is_initiator,
        },
    ))
}

pub fn decode_ospf_sync_route_request_detail(
    payload: &[u8],
) -> Option<(RpcDescriptorView, OspfRouteStateView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "OspfRouteRpc" || !descriptor.is_request {
        return None;
    }

    let request_bytes = decode_rpc_request_body(&packet)?;
    let body = PbSyncRouteInfoRequest::decode(request_bytes.as_slice()).ok()?;
    let raw_peer_payloads = body
        .peer_infos
        .as_ref()
        .map(|infos| infos.items.iter().map(|info| info.encode_to_vec()).collect::<Vec<_>>())
        .unwrap_or_default();
    Some((
        descriptor,
        OspfRouteStateView {
            my_peer_id: body.my_peer_id,
            my_session_id: body.my_session_id,
            is_initiator: body.is_initiator,
            peer_infos: body
                .peer_infos
                .map(|infos| {
                    infos
                        .items
                        .into_iter()
                        .enumerate()
                        .map(|(idx, info)| route_peer_info_view(info, raw_peer_payloads.get(idx).cloned()))
                        .collect()
                })
                .unwrap_or_default(),
            conn_entries: conn_entries_from_conn_info(body.conn_info),
        },
    ))
}

pub fn decode_ospf_sync_route_response(
    payload: &[u8],
) -> Option<(RpcDescriptorView, OspfSyncRouteResponseView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "OspfRouteRpc" || descriptor.is_request {
        return None;
    }

    let rpc_response = decode_rpc_response(&packet)?;
    let rpc_error = rpc_response.error.as_ref().and_then(rpc_error_to_string);
    let body = PbSyncRouteInfoResponse::decode(rpc_response.response.as_slice()).ok()?;
    Some((
        descriptor,
        OspfSyncRouteResponseView {
            is_initiator: body.is_initiator,
            session_id: body.session_id,
            error: body.error,
            rpc_error,
        },
    ))
}

fn encode_rpc_response_packet(
    request: &RpcDescriptorView,
    from_peer: PeerId,
    to_peer: PeerId,
    method_index: u32,
    response_body: Vec<u8>,
) -> Vec<u8> {
    RpcPacket {
        from_peer,
        to_peer,
        transaction_id: request.transaction_id,
        descriptor: Some(RpcDescriptor {
            domain_name: request.domain_name.clone(),
            proto_name: request.proto_name.clone(),
            service_name: request.service_name.clone(),
            method_index,
        }),
        body: RpcResponse {
            response: response_body,
            error: None,
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
    .encode_to_vec()
}

pub fn encode_report_peers_response(request: &RpcDescriptorView, from_peer: PeerId, to_peer: PeerId) -> Vec<u8> {
    encode_rpc_response_packet(
        request,
        from_peer,
        to_peer,
        1,
        PbReportPeersResponse::default().encode_to_vec(),
    )
}

pub fn encode_get_global_peer_map_response(
    request: &RpcDescriptorView,
    from_peer: PeerId,
    to_peer: PeerId,
    global_peer_map: GlobalPeerMapView,
) -> Vec<u8> {
    let body = PbGetGlobalPeerMapResponse {
        global_peer_map: global_peer_map
            .peers
            .into_iter()
            .map(|(src_peer, peers)| {
                (
                    src_peer,
                    PbPeerInfoForGlobalMap {
                        direct_peers: peers
                            .into_iter()
                            .map(|(peer_id, info)| {
                                (
                                    peer_id,
                                    PbDirectConnectedPeerInfo {
                                        latency_ms: info.latency_ms,
                                    },
                                )
                            })
                            .collect::<HashMap<_, _>>(),
                    },
                )
            })
            .collect::<HashMap<_, _>>(),
        digest: global_peer_map.digest,
    };

    encode_rpc_response_packet(request, from_peer, to_peer, 2, body.encode_to_vec())
}

pub fn encode_empty_ospf_sync_route_response(
    request: &RpcDescriptorView,
    from_peer: PeerId,
    to_peer: PeerId,
) -> Vec<u8> {
    encode_rpc_response_packet(
        request,
        from_peer,
        to_peer,
        0,
        PbSyncRouteInfoResponse {
            is_initiator: false,
            session_id: 1,
            error: None,
        }
        .encode_to_vec(),
    )
}

pub fn encode_ospf_sync_route_request(
    domain_name: &str,
    from_peer: PeerId,
    to_peer: PeerId,
    session_id: u64,
    is_initiator: bool,
    peer_infos: &[OspfPeerInfoView],
    conn_entries: &[OspfConnEntryView],
) -> Vec<u8> {
    let conn_info = if conn_entries.is_empty() {
        None
    } else {
        Some(pb_sync_route_info_request::ConnInfo::ConnPeerList(PbRouteConnPeerList {
            peer_conn_infos: conn_entries
                .iter()
                .cloned()
                .map(|entry| PbRouteConnPeerListPeerConnInfo {
                    peer_id: Some(PbPeerIdVersion {
                        peer_id: entry.peer_id,
                        version: entry.version,
                    }),
                    connected_peer_ids: entry.connected_peer_ids,
                })
                .collect(),
        }))
    };

    RpcPacket {
        from_peer,
        to_peer,
        transaction_id: session_id as i64,
        descriptor: Some(RpcDescriptor {
            domain_name: domain_name.to_string(),
            proto_name: "OspfRouteRpc".to_string(),
            service_name: "OspfRouteRpc".to_string(),
            method_index: 1,
        }),
        body: RpcRequest {
            descriptor: None,
            request: PbSyncRouteInfoRequest {
                my_peer_id: from_peer,
                my_session_id: session_id,
                is_initiator,
                peer_infos: Some(PbRoutePeerInfos {
                    items: peer_infos
                        .iter()
                        .cloned()
                        .map(|info| {
                            info.raw_payload
                                .as_ref()
                                .and_then(|raw| PbRoutePeerInfo::decode(raw.as_slice()).ok())
                                .unwrap_or_else(|| route_peer_info_proto(info))
                        })
                        .collect(),
                }),
                conn_info,
                foreign_network_infos: None,
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

pub fn decode_get_global_peer_map_response(
    payload: &[u8],
) -> Option<(RpcDescriptorView, GlobalPeerMapView)> {
    let packet = decode_rpc_packet(payload)?;
    let descriptor = descriptor_view(&packet, packet.descriptor.clone()?);
    if descriptor.service_name != "PeerCenterRpc"
        || descriptor.method_kind != RpcMethodKind::PeerCenterGetGlobalPeerMap
        || descriptor.is_request
    {
        return None;
    }

    let body = PbGetGlobalPeerMapResponse::decode(decode_rpc_response_body(&packet)?.as_slice()).ok()?;
    Some((
        descriptor,
        GlobalPeerMapView {
            digest: body.digest,
            peers: body
                .global_peer_map
                .into_iter()
                .map(|(src_peer, peers)| {
                    (
                        src_peer,
                        peers
                            .direct_peers
                            .into_iter()
                            .map(|(peer_id, info)| {
                                (
                                    peer_id,
                                    DirectConnectedPeerInfo {
                                        latency_ms: info.latency_ms,
                                    },
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        },
    ))
}
