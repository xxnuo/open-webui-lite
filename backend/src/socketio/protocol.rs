/// Socket.IO Protocol v5 implementation
///
/// This implements the Socket.IO protocol as specified in:
/// https://github.com/socketio/socket.io-protocol
///
/// Key changes in v5 (used by Socket.IO v3+):
/// - CONNECT packet must include a data payload with {sid: "..."}
/// - Client must send CONNECT for default namespace
/// - CONNECT_ERROR for connection failures
use serde_json::Value as JsonValue;

/// Engine.IO packet types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnginePacketType {
    Open = 0,    // Sent from server immediately after connection
    Close = 1,   // Request closing of transport
    Ping = 2,    // Sent by client for ping
    Pong = 3,    // Sent by server for pong
    Message = 4, // Actual message
    Upgrade = 5, // Before engine.io switches transport
    Noop = 6,    // Used for forcing packet flush
}

impl EnginePacketType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Open),
            1 => Some(Self::Close),
            2 => Some(Self::Ping),
            3 => Some(Self::Pong),
            4 => Some(Self::Message),
            5 => Some(Self::Upgrade),
            6 => Some(Self::Noop),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Socket.IO packet types (sent within Engine.IO MESSAGE packets)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketPacketType {
    Connect = 0,      // Connect to namespace
    Disconnect = 1,   // Disconnect from namespace
    Event = 2,        // Event with data
    Ack = 3,          // Acknowledgement
    ConnectError = 4, // Error during connection
    BinaryEvent = 5,  // Event with binary data
    BinaryAck = 6,    // Ack with binary data
}

impl SocketPacketType {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Connect),
            1 => Some(Self::Disconnect),
            2 => Some(Self::Event),
            3 => Some(Self::Ack),
            4 => Some(Self::ConnectError),
            5 => Some(Self::BinaryEvent),
            6 => Some(Self::BinaryAck),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Engine.IO packet
#[derive(Debug, Clone)]
pub struct EnginePacket {
    pub packet_type: EnginePacketType,
    pub data: Vec<u8>,
}

impl EnginePacket {
    pub fn new(packet_type: EnginePacketType, data: Vec<u8>) -> Self {
        Self { packet_type, data }
    }

    pub fn open(sid: &str, ping_interval: u64, ping_timeout: u64) -> Self {
        let open_packet = serde_json::json!({
            "sid": sid,
            "upgrades": ["websocket"],
            "pingInterval": ping_interval,
            "pingTimeout": ping_timeout,
            "maxPayload": 1_000_000,
        });
        Self::new(EnginePacketType::Open, open_packet.to_string().into_bytes())
    }

    pub fn message(data: Vec<u8>) -> Self {
        Self::new(EnginePacketType::Message, data)
    }

    pub fn pong(data: Vec<u8>) -> Self {
        Self::new(EnginePacketType::Pong, data)
    }

    pub fn close() -> Self {
        Self::new(EnginePacketType::Close, Vec::new())
    }

    /// Encode packet to string format (for websocket text frames)
    pub fn encode(&self) -> String {
        let type_char = char::from_digit(self.packet_type.to_u8() as u32, 10).unwrap();
        if self.data.is_empty() {
            format!("{}", type_char)
        } else {
            format!("{}{}", type_char, String::from_utf8_lossy(&self.data))
        }
    }

    /// Encode packet to binary format (for websocket binary frames)
    #[allow(dead_code)]
    pub fn encode_binary(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(1 + self.data.len());
        result.push(self.packet_type.to_u8());
        result.extend_from_slice(&self.data);
        result
    }

    /// Decode packet to string format
    #[allow(dead_code)]
    pub fn decode(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("Empty packet".to_string());
        }

        let first_char = s.chars().next().unwrap();
        let packet_type = first_char
            .to_digit(10)
            .and_then(|d| EnginePacketType::from_u8(d as u8))
            .ok_or_else(|| format!("Invalid packet type: {}", first_char))?;

        let data = if s.len() > 1 {
            s[1..].as_bytes().to_vec()
        } else {
            Vec::new()
        };

        Ok(Self { packet_type, data })
    }

    /// Decode packet from binary format
    #[allow(dead_code)]
    pub fn decode_binary(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("Empty packet".to_string());
        }

        let packet_type = EnginePacketType::from_u8(bytes[0])
            .ok_or_else(|| format!("Invalid packet type: {}", bytes[0]))?;

        let data = if bytes.len() > 1 {
            bytes[1..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self { packet_type, data })
    }
}

/// Socket.IO packet
#[derive(Debug, Clone)]
pub struct SocketPacket {
    pub packet_type: SocketPacketType,
    pub namespace: String,
    pub data: Option<JsonValue>,
    pub id: Option<u64>,
}

/// Acknowledgment tracker for reliable message delivery
#[allow(dead_code)]
pub struct AckTracker {
    next_id: std::sync::atomic::AtomicU64,
    pending_acks: Arc<RwLock<HashMap<u64, PendingAck>>>,
}

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

struct PendingAck {
    created_at: std::time::Instant,
    timeout: std::time::Duration,
    callback: Option<Box<dyn FnOnce(JsonValue) + Send>>,
}

#[allow(dead_code)]
impl AckTracker {
    pub fn new() -> Self {
        Self {
            next_id: std::sync::atomic::AtomicU64::new(1),
            pending_acks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate a new ACK ID
    pub fn next_ack_id(&self) -> u64 {
        self.next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Register a pending ACK with callback
    pub async fn register_ack<F>(&self, id: u64, timeout: std::time::Duration, callback: F)
    where
        F: FnOnce(JsonValue) + Send + 'static,
    {
        let mut pending = self.pending_acks.write().await;
        pending.insert(
            id,
            PendingAck {
                created_at: std::time::Instant::now(),
                timeout,
                callback: Some(Box::new(callback)),
            },
        );
    }

    /// Process an ACK response
    pub async fn process_ack(&self, id: u64, data: JsonValue) -> bool {
        let mut pending = self.pending_acks.write().await;
        if let Some(ack) = pending.remove(&id) {
            if let Some(callback) = ack.callback {
                callback(data);
            }
            true
        } else {
            false
        }
    }

    /// Clean up expired ACKs
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending_acks.write().await;
        let now = std::time::Instant::now();

        pending.retain(|_, ack| now.duration_since(ack.created_at) < ack.timeout);
    }

    /// Get pending count
    pub async fn pending_count(&self) -> usize {
        self.pending_acks.read().await.len()
    }
}

impl Default for AckTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SocketPacket {
    pub fn new(packet_type: SocketPacketType) -> Self {
        Self {
            packet_type,
            namespace: "/".to_string(),
            data: None,
            id: None,
        }
    }

    /// Create a CONNECT packet (server response)
    /// In Socket.IO v5, the server must send back a sid in the data field
    pub fn connect(namespace: &str, sid: Option<&str>) -> Self {
        let data = sid.map(|s| serde_json::json!({"sid": s}));
        Self {
            packet_type: SocketPacketType::Connect,
            namespace: namespace.to_string(),
            data,
            id: None,
        }
    }

    /// Create a CONNECT request packet (client sends)
    /// In Socket.IO v5, client can send auth data in the CONNECT packet
    #[allow(dead_code)]
    pub fn connect_request(namespace: &str, auth: Option<JsonValue>) -> Self {
        Self {
            packet_type: SocketPacketType::Connect,
            namespace: namespace.to_string(),
            data: auth,
            id: None,
        }
    }

    #[allow(dead_code)]
    pub fn disconnect(namespace: &str) -> Self {
        Self {
            packet_type: SocketPacketType::Disconnect,
            namespace: namespace.to_string(),
            data: None,
            id: None,
        }
    }

    pub fn event(namespace: &str, event: &str, data: JsonValue) -> Self {
        Self {
            packet_type: SocketPacketType::Event,
            namespace: namespace.to_string(),
            data: Some(serde_json::json!([event, data])),
            id: None,
        }
    }

    /// Create an event with ACK ID for reliable delivery
    #[allow(dead_code)]
    pub fn event_with_ack(namespace: &str, event: &str, data: JsonValue, ack_id: u64) -> Self {
        Self {
            packet_type: SocketPacketType::Event,
            namespace: namespace.to_string(),
            data: Some(serde_json::json!([event, data])),
            id: Some(ack_id),
        }
    }

    /// Create a binary event packet
    #[allow(dead_code)]
    pub fn binary_event(
        namespace: &str,
        event: &str,
        data: JsonValue,
        _attachments: usize,
    ) -> Self {
        Self {
            packet_type: SocketPacketType::BinaryEvent,
            namespace: namespace.to_string(),
            data: Some(serde_json::json!([event, data])),
            id: None,
        }
    }

    #[allow(dead_code)]
    pub fn ack(namespace: &str, id: u64, data: JsonValue) -> Self {
        Self {
            packet_type: SocketPacketType::Ack,
            namespace: namespace.to_string(),
            data: Some(data),
            id: Some(id),
        }
    }

    /// Create a CONNECT_ERROR packet
    /// In Socket.IO v5, error data must be an object instead of plain string
    #[allow(dead_code)]
    pub fn connect_error(namespace: &str, message: &str) -> Self {
        Self {
            packet_type: SocketPacketType::ConnectError,
            namespace: namespace.to_string(),
            data: Some(serde_json::json!({"message": message})),
            id: None,
        }
    }

    /// Encode Socket.IO packet to string
    pub fn encode(&self) -> String {
        let mut result = String::new();

        // Packet type
        result.push_str(&self.packet_type.to_u8().to_string());

        // Namespace (if not default)
        if self.namespace != "/" {
            result.push_str(&self.namespace);
            result.push(',');
        }

        // Ack ID
        if let Some(id) = self.id {
            result.push_str(&id.to_string());
        }

        // Data
        if let Some(ref data) = self.data {
            result.push_str(&data.to_string());
        }

        result
    }

    /// Decode Socket.IO packet from string
    pub fn decode(s: &str) -> Result<Self, String> {
        if s.is_empty() {
            return Err("Empty packet".to_string());
        }

        let mut chars = s.chars();

        // Parse packet type
        let packet_type = chars
            .next()
            .and_then(|c| c.to_digit(10))
            .and_then(|d| SocketPacketType::from_u8(d as u8))
            .ok_or("Invalid packet type")?;

        let rest: String = chars.collect();
        let mut namespace = "/".to_string();
        let mut id = None;
        let mut data = None;
        let mut position = 0;

        // Parse namespace
        if rest.starts_with('/') {
            if let Some(comma_pos) = rest.find(',') {
                namespace = rest[..comma_pos].to_string();
                position = comma_pos + 1;
            } else {
                // Namespace without comma (e.g., "/admin")
                let space_or_bracket = rest
                    .find(|c: char| c == '[' || c == '{' || c.is_whitespace())
                    .unwrap_or(rest.len());
                namespace = rest[..space_or_bracket].to_string();
                position = space_or_bracket;
            }
        }

        let remaining = &rest[position..];

        // Parse ack ID
        let data_start = remaining
            .find(|c: char| c == '[' || c == '{')
            .unwrap_or(remaining.len());

        if data_start > 0 {
            let id_str = remaining[..data_start].trim();
            if !id_str.is_empty() {
                id = id_str.parse::<u64>().ok();
            }
        }

        // Parse data
        if data_start < remaining.len() {
            let json_str = &remaining[data_start..];
            data = serde_json::from_str(json_str).ok();
        }

        Ok(Self {
            packet_type,
            namespace,
            data,
            id,
        })
    }

    /// Get event name and data from Event packet
    pub fn get_event(&self) -> Option<(String, JsonValue)> {
        if self.packet_type != SocketPacketType::Event {
            return None;
        }

        let data = self.data.as_ref()?;
        let arr = data.as_array()?;

        if arr.len() < 2 {
            return None;
        }

        let event = arr[0].as_str()?.to_string();
        let event_data = arr[1].clone();

        Some((event, event_data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_packet_encode_decode() {
        let packet = EnginePacket::message(b"hello".to_vec());
        let encoded = packet.encode();
        assert_eq!(encoded, "4hello");

        let decoded = EnginePacket::decode(&encoded).unwrap();
        assert_eq!(decoded.packet_type, EnginePacketType::Message);
        assert_eq!(decoded.data, b"hello");
    }

    #[test]
    fn test_socket_packet_event() {
        let packet =
            SocketPacket::event("/", "chat-events", serde_json::json!({"message": "hello"}));
        let encoded = packet.encode();

        let decoded = SocketPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.packet_type, SocketPacketType::Event);

        let (event, data) = decoded.get_event().unwrap();
        assert_eq!(event, "chat-events");
        assert_eq!(data, serde_json::json!({"message": "hello"}));
    }

    #[test]
    fn test_socket_packet_with_namespace() {
        let packet = SocketPacket::event("/admin", "test", serde_json::json!({"data": 123}));
        let encoded = packet.encode();
        assert!(encoded.starts_with("2/admin,"));

        let decoded = SocketPacket::decode(&encoded).unwrap();
        assert_eq!(decoded.namespace, "/admin");
    }
}
