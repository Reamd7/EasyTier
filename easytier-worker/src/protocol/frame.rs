use prost::Message;

use crate::protocol::network_identity::network_id_from_name;
use crate::protocol::peer_center::{classify_rpc_payload, RpcClassification};

pub const PEER_MANAGER_HEADER_SIZE: usize = 16;
pub const HANDSHAKE_MAGIC: u32 = 0xd1e1a5e1;
pub const HANDSHAKE_VERSION: u32 = 1;
pub const HANDSHAKE_NETWORK_SECRET_DIGEST_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    Data = 1,
    HandShake = 2,
    Ping = 4,
    Pong = 5,
    TaRpc = 6,
    RpcReq = 8,
    RpcResp = 9,
    RelayHandshake = 20,
    RelayHandshakeAck = 21,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerManagerHeader {
    pub from_peer_id: u32,
    pub to_peer_id: u32,
    pub packet_type: u8,
    pub flags: u8,
    pub forward_counter: u8,
    pub payload_len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedFrame<'a> {
    pub header: PeerManagerHeader,
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeView {
    pub my_peer_id: u32,
    pub network_name: String,
    pub network_secret_digest: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncomingFrame<'a> {
    Binary(&'a [u8]),
    Text(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameClassification<'a> {
    Unknown,
    RelayData(ParsedFrame<'a>),
    PeerCenterReport(ParsedFrame<'a>),
    PeerCenterGetGlobalPeerMap(ParsedFrame<'a>),
    RpcOther(ParsedFrame<'a>),
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

pub fn classify_incoming_frame(frame: IncomingFrame<'_>) -> FrameClassification<'_> {
    let IncomingFrame::Binary(bytes) = frame else {
        return FrameClassification::Unknown;
    };

    let Some(parsed) = parse_peer_manager_frame(bytes) else {
        return FrameClassification::Unknown;
    };

    match parsed.header.packet_type {
        x if x == PacketType::Data as u8
            || x == PacketType::RelayHandshake as u8
            || x == PacketType::RelayHandshakeAck as u8 => FrameClassification::RelayData(parsed),
        x if x == PacketType::TaRpc as u8
            || x == PacketType::RpcReq as u8
            || x == PacketType::RpcResp as u8 => {
            match classify_rpc_payload(parsed.payload) {
                Some(RpcClassification::PeerCenterReport(_)) => {
                    FrameClassification::PeerCenterReport(parsed)
                }
                Some(RpcClassification::PeerCenterGetGlobalPeerMap(_)) => {
                    FrameClassification::PeerCenterGetGlobalPeerMap(parsed)
                }
                Some(RpcClassification::Other(_)) => FrameClassification::RpcOther(parsed),
                None => FrameClassification::Unknown,
            }
        }
        _ => FrameClassification::Unknown,
    }
}

pub fn decode_handshake_request(frame: &[u8]) -> Option<HandshakeView> {
    let parsed = parse_peer_manager_frame(frame)?;
    if parsed.header.packet_type != PacketType::HandShake as u8 {
        return None;
    }

    let request = HandshakeRequest::decode(parsed.payload).ok()?;
    if request.network_secret_digest.len() != HANDSHAKE_NETWORK_SECRET_DIGEST_LEN {
        return None;
    }

    Some(HandshakeView {
        my_peer_id: request.my_peer_id,
        network_name: request.network_name,
        network_secret_digest: request.network_secret_digest,
    })
}

pub fn build_handshake_response_frame(
    request_frame: &[u8],
    responder_peer_id: u32,
    responder_network_name: &str,
) -> Option<Vec<u8>> {
    let request = decode_handshake_request(request_frame)?;
    let payload = HandshakeRequest {
        magic: HANDSHAKE_MAGIC,
        my_peer_id: responder_peer_id,
        version: HANDSHAKE_VERSION,
        features: Vec::new(),
        network_name: responder_network_name.to_string(),
        network_secret_digest: vec![0; HANDSHAKE_NETWORK_SECRET_DIGEST_LEN],
    }
    .encode_to_vec();

    let mut frame = Vec::with_capacity(PEER_MANAGER_HEADER_SIZE + payload.len());
    frame.extend_from_slice(&responder_peer_id.to_le_bytes());
    frame.extend_from_slice(&request.my_peer_id.to_le_bytes());
    frame.push(PacketType::HandShake as u8);
    frame.push(0);
    frame.push(0);
    frame.push(0);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&payload);
    Some(frame)
}


pub fn build_peer_manager_frame(
    packet_type: PacketType,
    payload: &[u8],
    from_peer_id: u32,
    to_peer_id: u32,
) -> Vec<u8> {
    let mut frame = Vec::with_capacity(PEER_MANAGER_HEADER_SIZE + payload.len());
    frame.extend_from_slice(&from_peer_id.to_le_bytes());
    frame.extend_from_slice(&to_peer_id.to_le_bytes());
    frame.push(packet_type as u8);
    frame.push(0);
    frame.push(0);
    frame.push(0);
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

pub fn build_pong_frame(frame: &[u8]) -> Option<Vec<u8>> {
    let parsed = parse_peer_manager_frame(frame)?;
    if parsed.header.packet_type != PacketType::Ping as u8 {
        return None;
    }

    let mut response = frame.to_vec();
    response[8] = PacketType::Pong as u8;
    Some(response)
}

pub fn extract_network_name(frame: &[u8]) -> Option<String> {
    let parsed = parse_peer_manager_frame(frame)?;

    match parsed.header.packet_type {
        x if x == PacketType::TaRpc as u8
            || x == PacketType::RpcReq as u8
            || x == PacketType::RpcResp as u8 => {
            let rpc = classify_rpc_payload(parsed.payload)?;
            let domain_name = match rpc {
                RpcClassification::PeerCenterReport(view)
                | RpcClassification::PeerCenterGetGlobalPeerMap(view)
                | RpcClassification::Other(view) => view.domain_name,
            };
            Some(domain_name)
        }
        x if x == PacketType::HandShake as u8 => decode_handshake_request(frame)
            .map(|request| request.network_name),
        _ => None,
    }
}

pub fn extract_network_identity(frame: &[u8]) -> Option<String> {
    let parsed = parse_peer_manager_frame(frame)?;

    match parsed.header.packet_type {
        x if x == PacketType::TaRpc as u8
            || x == PacketType::RpcReq as u8
            || x == PacketType::RpcResp as u8 => {
            let rpc = classify_rpc_payload(parsed.payload)?;
            let domain_name = match rpc {
                RpcClassification::PeerCenterReport(view)
                | RpcClassification::PeerCenterGetGlobalPeerMap(view)
                | RpcClassification::Other(view) => view.domain_name,
            };
            network_id_from_name(&domain_name)
        }
        x if x == PacketType::HandShake as u8 => decode_handshake_request(frame)
            .and_then(|request| network_id_from_name(&request.network_name)),
        _ => None,
    }
}

pub fn parse_peer_manager_frame(frame: &[u8]) -> Option<ParsedFrame<'_>> {
    if frame.len() < PEER_MANAGER_HEADER_SIZE {
        return None;
    }

    let from_peer_id = u32::from_le_bytes(frame[0..4].try_into().ok()?);
    let to_peer_id = u32::from_le_bytes(frame[4..8].try_into().ok()?);
    let packet_type = frame[8];
    let flags = frame[9];
    let forward_counter = frame[10];
    let payload_len = u32::from_le_bytes(frame[12..16].try_into().ok()?);

    let total_len = PEER_MANAGER_HEADER_SIZE.checked_add(payload_len as usize)?;
    if frame.len() < total_len {
        return None;
    }

    Some(ParsedFrame {
        header: PeerManagerHeader {
            from_peer_id,
            to_peer_id,
            packet_type,
            flags,
            forward_counter,
            payload_len,
        },
        payload: &frame[PEER_MANAGER_HEADER_SIZE..total_len],
    })
}
