use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerState {
    Disconnected,
    Connecting,
    Authenticating,
    Ready,
    Syncing,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub state: PeerState,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub height: u64,
    pub latest_hash: String,
    pub protocol_version: u16,
    pub connected_at: DateTime<Utc>,
    pub latency_ms: Option<u64>,
    pub failure_count: u32,
}
