use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as StdMutex},
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::{Mutex, RwLock, broadcast},
    task::JoinHandle,
    time::timeout,
};
use uuid::Uuid;

use crate::{
    Block, Blockchain, Transaction,
    crypto::ValidatorIdentity,
    error::{MiniChainError, Result},
    mempool::Mempool,
    storage::{RedbStorage, Storage},
};

use super::{
    peer::{Peer, PeerState},
    protocol::{
        Envelope, MAX_SYNC_BLOCKS, MessagePayload, NetworkStatus, PROTOCOL_VERSION, read_envelope,
        write_envelope,
    },
};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(5);
const SEEN_MESSAGE_CAPACITY: usize = 4_096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedPeer {
    pub address: String,
    pub public_key: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiRole {
    Viewer,
    Operator,
    Admin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiTokenConfig {
    pub identity: String,
    pub role: ApiRole,
    pub token_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApiConfig {
    pub listen_address: String,
    pub allowed_origins: Vec<String>,
    pub max_body_bytes: usize,
    pub rate_window_ms: u64,
    pub read_requests_per_window: u32,
    pub write_requests_per_window: u32,
    pub admin_requests_per_window: u32,
    pub tokens: Vec<ApiTokenConfig>,
}

impl ApiConfig {
    pub fn validate(&self) -> Result<()> {
        let mut identities = HashSet::new();
        let mut digests = HashSet::new();
        let valid_tokens = !self.tokens.is_empty()
            && self.tokens.iter().all(|token| {
                let digest = crate::crypto::decode_hex(&token.token_sha256)
                    .ok()
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok());
                !token.identity.is_empty()
                    && identities.insert(token.identity.clone())
                    && digest.is_some_and(|value| digests.insert(value))
            });
        if self.listen_address.is_empty()
            || self.allowed_origins.is_empty()
            || self.allowed_origins.iter().any(|origin| origin == "*")
            || self.max_body_bytes == 0
            || self.rate_window_ms < 100
            || self.read_requests_per_window == 0
            || self.write_requests_per_window == 0
            || self.admin_requests_per_window == 0
            || !valid_tokens
        {
            return Err(MiniChainError::InvalidConfiguration);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    pub node_id: String,
    pub listen_address: String,
    pub trusted_peers: HashMap<String, TrustedPeer>,
    pub max_peers: usize,
    pub chain_id: Uuid,
    pub network_id: String,
    pub storage_path: PathBuf,
    pub identity_path: PathBuf,
    pub heartbeat_interval_ms: u64,
    #[serde(default)]
    pub api: Option<ApiConfig>,
}

impl NodeConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|_| MiniChainError::InvalidConfiguration)?;
        let config: Self =
            toml::from_str(&text).map_err(|_| MiniChainError::InvalidConfiguration)?;
        if config.node_id.is_empty()
            || config.network_id.is_empty()
            || config.max_peers == 0
            || config.heartbeat_interval_ms < 100
            || config.trusted_peers.contains_key(&config.node_id)
        {
            return Err(MiniChainError::InvalidConfiguration);
        }
        if let Some(api) = &config.api {
            api.validate()?;
        }
        Ok(config)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Synced,
    Behind,
    Syncing,
    Diverged,
    Offline,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum NetworkEvent {
    PeerConnected(String),
    PeerAuthenticated(String),
    PeerUnhealthy(String),
    TransactionReceived(Uuid),
    TransactionBroadcast(Uuid),
    BlockReceived(u64),
    BlockCommitted(u64),
    SyncStarted(String),
    SyncProgress { peer: String, height: u64 },
    SyncCompleted(String),
    DivergenceDetected { peer: String, height: u64 },
}

#[derive(Clone)]
pub struct NetworkNode {
    config: Arc<NodeConfig>,
    identity: Arc<ValidatorIdentity>,
    storage: Arc<RedbStorage>,
    mempool: Mempool,
    peers: Arc<RwLock<HashMap<String, Peer>>>,
    seen: Arc<StdMutex<SeenMessages>>,
    storage_mutation: Arc<Mutex<()>>,
    events: broadcast::Sender<NetworkEvent>,
}

pub struct RunningNode {
    pub node: NetworkNode,
    listener: JoinHandle<()>,
    maintenance: JoinHandle<()>,
}

impl Drop for RunningNode {
    fn drop(&mut self) {
        self.listener.abort();
        self.maintenance.abort();
    }
}

impl NetworkNode {
    pub fn new(
        config: NodeConfig,
        identity: ValidatorIdentity,
        storage: Arc<RedbStorage>,
        mempool: Mempool,
    ) -> Result<Self> {
        if config.node_id != identity.validator_id() || config.max_peers == 0 {
            return Err(MiniChainError::InvalidPeerIdentity {
                peer: config.node_id,
            });
        }
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            config: Arc::new(config),
            identity: Arc::new(identity),
            storage,
            mempool,
            peers: Arc::new(RwLock::new(HashMap::new())),
            seen: Arc::new(StdMutex::new(SeenMessages::new(SEEN_MESSAGE_CAPACITY))),
            storage_mutation: Arc::new(Mutex::new(())),
            events,
        })
    }

    pub async fn start(self) -> Result<RunningNode> {
        let listener = TcpListener::bind(&self.config.listen_address)
            .await
            .map_err(|_| MiniChainError::PeerUnavailable {
                peer: self.config.listen_address.clone(),
            })?;
        let node = self.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let node = node.clone();
                tokio::spawn(async move {
                    let _ = node.handle_connection(stream).await;
                });
            }
        });
        let maintenance_node = self.clone();
        let maintenance = tokio::spawn(async move {
            let interval = Duration::from_millis(maintenance_node.config.heartbeat_interval_ms);
            loop {
                tokio::time::sleep(interval).await;
                maintenance_node.maintain_peers_once().await;
            }
        });
        Ok(RunningNode {
            node: self,
            listener: task,
            maintenance,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NetworkEvent> {
        self.events.subscribe()
    }

    pub async fn peers(&self) -> Vec<Peer> {
        self.peers.read().await.values().cloned().collect()
    }

    pub async fn mempool_len(&self) -> usize {
        self.mempool.len().await
    }

    pub async fn mempool_transaction(&self, id: Uuid) -> Option<Transaction> {
        self.mempool.get(id).await
    }

    pub async fn connect(&self, peer_id: &str) -> Result<NetworkStatus> {
        let (mut stream, status) = self.open_session(peer_id).await?;
        stream
            .shutdown()
            .await
            .map_err(|_| MiniChainError::PeerUnavailable {
                peer: peer_id.to_owned(),
            })?;
        Ok(status)
    }

    pub async fn maintain_peers_once(&self) {
        let peer_ids = self
            .config
            .trusted_peers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for peer_id in peer_ids {
            match self.connect(&peer_id).await {
                Ok(remote) => {
                    if let Ok(local) = self.local_status().await
                        && remote.height > local.height
                    {
                        let _ = self.sync_from(&peer_id).await;
                    } else {
                        let _ = self.heartbeat_once(&peer_id).await;
                    }
                }
                Err(_) => self.mark_failed(&peer_id).await,
            }
        }
    }

    pub async fn broadcast_transaction(&self, transaction: Transaction) -> Result<()> {
        self.mempool.insert(transaction.clone()).await?;
        let peer_ids = self
            .config
            .trusted_peers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut first_error = None;
        for peer_id in peer_ids {
            if let Err(error) = self
                .send_and_expect_status(&peer_id, MessagePayload::Transaction(transaction.clone()))
                .await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        let _ = self
            .events
            .send(NetworkEvent::TransactionBroadcast(transaction.id));
        first_error.map_or(Ok(()), Err)
    }

    pub async fn broadcast_block(&self, block: Block) -> Result<()> {
        let peer_ids = self
            .config
            .trusted_peers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut first_error = None;
        for peer_id in peer_ids {
            if let Err(error) = self
                .send_and_expect_status(&peer_id, MessagePayload::Block(block.clone()))
                .await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn commit_and_broadcast_block(&self, block: Block) -> Result<()> {
        let height = block.header.index;
        let transaction_ids = block
            .transactions
            .iter()
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        {
            let _mutation = self.storage_mutation.lock().await;
            let storage = Arc::clone(&self.storage);
            let committed_block = block.clone();
            tokio::task::spawn_blocking(move || storage.commit_block(committed_block))
                .await
                .map_err(|_| MiniChainError::StorageUnavailable)??;
        }
        self.mempool.remove_committed(&transaction_ids).await;
        let _ = self.events.send(NetworkEvent::BlockCommitted(height));
        self.broadcast_block(block).await
    }

    pub async fn heartbeat_once(&self, peer_id: &str) -> Result<Duration> {
        let nonce = Uuid::new_v4();
        let started = Instant::now();
        let response = match self.request(peer_id, MessagePayload::Ping { nonce }).await {
            Ok(response) => response,
            Err(error) => {
                self.mark_failed(peer_id).await;
                return Err(error);
            }
        };
        if response != (MessagePayload::Pong { nonce }) {
            self.mark_failed(peer_id).await;
            return Err(MiniChainError::InvalidMessage);
        }
        let latency = started.elapsed();
        if let Some(peer) = self.peers.write().await.get_mut(peer_id) {
            peer.last_heartbeat = Some(Utc::now());
            peer.latency_ms = Some(latency.as_millis() as u64);
            peer.failure_count = 0;
            peer.state = PeerState::Ready;
        }
        Ok(latency)
    }

    pub async fn sync_from(&self, peer_id: &str) -> Result<NetworkStatus> {
        let remote = match self.request(peer_id, MessagePayload::GetStatus).await? {
            MessagePayload::Status(status) => status,
            _ => return Err(MiniChainError::SyncFailed),
        };
        let _mutation = self.storage_mutation.lock().await;
        let local = self.local_status().await?;
        if remote.height == local.height {
            if remote.latest_hash != local.latest_hash {
                self.mark_diverged(peer_id, local.height).await;
                return Err(MiniChainError::DivergenceDetected {
                    height: local.height,
                });
            }
            return Ok(remote);
        }
        if remote.height < local.height {
            return Ok(remote);
        }

        let _ = self
            .events
            .send(NetworkEvent::SyncStarted(peer_id.to_owned()));
        self.set_peer_state(peer_id, PeerState::Syncing).await;
        let mut next = local.height + 1;
        while next <= remote.height {
            let end = (next + MAX_SYNC_BLOCKS - 1).min(remote.height);
            let response = self
                .request(
                    peer_id,
                    MessagePayload::SyncRequest {
                        from_height: next,
                        to_height: end,
                    },
                )
                .await?;
            let MessagePayload::SyncResponse { blocks } = response else {
                return Err(MiniChainError::SyncFailed);
            };
            if blocks.len() != (end - next + 1) as usize {
                return Err(MiniChainError::SyncFailed);
            }
            for block in blocks {
                if block.header.index != next {
                    return Err(MiniChainError::SyncFailed);
                }
                let transaction_ids = block
                    .transactions
                    .iter()
                    .map(|transaction| transaction.id)
                    .collect::<Vec<_>>();
                let storage = Arc::clone(&self.storage);
                tokio::task::spawn_blocking(move || storage.commit_block(block))
                    .await
                    .map_err(|_| MiniChainError::SyncFailed)??;
                self.mempool.remove_committed(&transaction_ids).await;
                let _ = self.events.send(NetworkEvent::SyncProgress {
                    peer: peer_id.to_owned(),
                    height: next,
                });
                next += 1;
            }
        }
        let current = self.local_status().await?;
        if current.height != remote.height || current.latest_hash != remote.latest_hash {
            self.mark_diverged(peer_id, current.height).await;
            return Err(MiniChainError::DivergenceDetected {
                height: current.height,
            });
        }
        self.set_peer_state(peer_id, PeerState::Ready).await;
        let _ = self
            .events
            .send(NetworkEvent::SyncCompleted(peer_id.to_owned()));
        Ok(remote)
    }

    pub async fn local_status(&self) -> Result<NetworkStatus> {
        let storage = Arc::clone(&self.storage);
        let node_id = self.config.node_id.clone();
        tokio::task::spawn_blocking(move || {
            let metadata = storage.metadata()?;
            Ok(NetworkStatus {
                node_id,
                height: metadata.current_height,
                latest_hash: metadata.latest_block_hash,
                protocol_version: PROTOCOL_VERSION,
            })
        })
        .await
        .map_err(|_| MiniChainError::StorageUnavailable)?
    }

    pub async fn restore_snapshot(&self, id: Uuid, force: bool) -> Result<Blockchain> {
        let _mutation = self.storage_mutation.lock().await;
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || storage.restore_snapshot(id, force))
            .await
            .map_err(|_| MiniChainError::StorageUnavailable)?
    }

    async fn handle_connection(&self, mut stream: TcpStream) -> Result<()> {
        let hello = timeout(NETWORK_TIMEOUT, read_envelope(&mut stream))
            .await
            .map_err(|_| MiniChainError::NetworkTimeout)??;
        let MessagePayload::Hello {
            listen_address: _,
            status,
        } = &hello.payload
        else {
            return Err(MiniChainError::HandshakeFailed);
        };
        self.authenticate(&hello)?;
        if status.node_id != hello.sender {
            return Err(MiniChainError::InvalidPeerIdentity {
                peer: status.node_id.clone(),
            });
        }
        self.update_peer(status.clone(), PeerState::Ready).await?;
        let ack = Envelope::signed(
            MessagePayload::HelloAck {
                status: self.local_status().await?,
            },
            &self.identity,
        )?;
        write_envelope(&mut stream, &ack).await?;
        let _ = self
            .events
            .send(NetworkEvent::PeerAuthenticated(hello.sender.clone()));

        let message = timeout(NETWORK_TIMEOUT, read_envelope(&mut stream))
            .await
            .map_err(|_| MiniChainError::NetworkTimeout)??;
        self.authenticate(&message)?;
        if !self.remember(message.message_id) {
            return self.write_status(&mut stream).await;
        }
        self.handle_message(&hello.sender, message.payload, &mut stream)
            .await
    }

    async fn handle_message(
        &self,
        sender: &str,
        payload: MessagePayload,
        stream: &mut TcpStream,
    ) -> Result<()> {
        match payload {
            MessagePayload::Ping { nonce } => {
                self.write_payload(stream, MessagePayload::Pong { nonce })
                    .await
            }
            MessagePayload::GetStatus => self.write_status(stream).await,
            MessagePayload::Transaction(transaction) => {
                self.mempool.insert(transaction.clone()).await?;
                let _ = self
                    .events
                    .send(NetworkEvent::TransactionReceived(transaction.id));
                self.write_status(stream).await
            }
            MessagePayload::Block(block) => {
                self.receive_block(sender, block).await?;
                self.write_status(stream).await
            }
            MessagePayload::SyncRequest {
                from_height,
                to_height,
            } => {
                if from_height > to_height || to_height - from_height + 1 > MAX_SYNC_BLOCKS {
                    return Err(MiniChainError::InvalidMessage);
                }
                let storage = Arc::clone(&self.storage);
                let blocks =
                    tokio::task::spawn_blocking(move || storage.get_blocks(from_height, to_height))
                        .await
                        .map_err(|_| MiniChainError::SyncFailed)??;
                self.write_payload(stream, MessagePayload::SyncResponse { blocks })
                    .await
            }
            _ => Err(MiniChainError::InvalidMessage),
        }
    }

    async fn receive_block(&self, sender: &str, block: Block) -> Result<()> {
        let _mutation = self.storage_mutation.lock().await;
        let local = self.local_status().await?;
        if block.header.index <= local.height {
            let storage = Arc::clone(&self.storage);
            let height = block.header.index;
            let existing = tokio::task::spawn_blocking(move || storage.get_block(height))
                .await
                .map_err(|_| MiniChainError::StorageUnavailable)??;
            if existing.hash != block.hash {
                self.mark_diverged(sender, height).await;
                return Err(MiniChainError::DivergenceDetected { height });
            }
            return Ok(());
        }
        if block.header.index != local.height + 1 {
            self.set_peer_state(sender, PeerState::Syncing).await;
            return Err(MiniChainError::SyncFailed);
        }
        let height = block.header.index;
        let transaction_ids = block
            .transactions
            .iter()
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>();
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || storage.commit_block(block))
            .await
            .map_err(|_| MiniChainError::StorageUnavailable)??;
        self.mempool.remove_committed(&transaction_ids).await;
        let _ = self.events.send(NetworkEvent::BlockReceived(height));
        let _ = self.events.send(NetworkEvent::BlockCommitted(height));
        Ok(())
    }

    async fn request(&self, peer_id: &str, payload: MessagePayload) -> Result<MessagePayload> {
        let (mut stream, _) = self.open_session(peer_id).await?;
        let message = Envelope::signed(payload, &self.identity)?;
        write_envelope(&mut stream, &message).await?;
        let response = timeout(NETWORK_TIMEOUT, read_envelope(&mut stream))
            .await
            .map_err(|_| MiniChainError::NetworkTimeout)??;
        self.authenticate(&response)?;
        Ok(response.payload)
    }

    async fn send_and_expect_status(
        &self,
        peer_id: &str,
        payload: MessagePayload,
    ) -> Result<NetworkStatus> {
        match self.request(peer_id, payload).await? {
            MessagePayload::Status(status) => Ok(status),
            MessagePayload::Reject { .. } => Err(MiniChainError::InvalidMessage),
            _ => Err(MiniChainError::InvalidMessage),
        }
    }

    async fn open_session(&self, peer_id: &str) -> Result<(TcpStream, NetworkStatus)> {
        let trusted = self.config.trusted_peers.get(peer_id).ok_or_else(|| {
            MiniChainError::InvalidPeerIdentity {
                peer: peer_id.to_owned(),
            }
        })?;
        self.set_peer_state(peer_id, PeerState::Connecting).await;
        let mut stream = timeout(NETWORK_TIMEOUT, TcpStream::connect(&trusted.address))
            .await
            .map_err(|_| MiniChainError::NetworkTimeout)?
            .map_err(|_| MiniChainError::PeerUnavailable {
                peer: peer_id.to_owned(),
            })?;
        self.set_peer_state(peer_id, PeerState::Authenticating)
            .await;
        let hello = Envelope::signed(
            MessagePayload::Hello {
                listen_address: self.config.listen_address.clone(),
                status: self.local_status().await?,
            },
            &self.identity,
        )?;
        write_envelope(&mut stream, &hello).await?;
        let ack = timeout(NETWORK_TIMEOUT, read_envelope(&mut stream))
            .await
            .map_err(|_| MiniChainError::NetworkTimeout)??;
        self.authenticate(&ack)?;
        let MessagePayload::HelloAck { status } = ack.payload else {
            return Err(MiniChainError::HandshakeFailed);
        };
        if status.node_id != peer_id {
            return Err(MiniChainError::InvalidPeerIdentity {
                peer: status.node_id,
            });
        }
        self.update_peer(status.clone(), PeerState::Ready).await?;
        let _ = self
            .events
            .send(NetworkEvent::PeerConnected(peer_id.to_owned()));
        Ok((stream, status))
    }

    fn authenticate(&self, envelope: &Envelope) -> Result<()> {
        let trusted = self
            .config
            .trusted_peers
            .get(&envelope.sender)
            .ok_or_else(|| MiniChainError::InvalidPeerIdentity {
                peer: envelope.sender.clone(),
            })?;
        envelope.verify(&envelope.sender, &trusted.public_key)
    }

    async fn write_status(&self, stream: &mut TcpStream) -> Result<()> {
        self.write_payload(stream, MessagePayload::Status(self.local_status().await?))
            .await
    }

    async fn write_payload(&self, stream: &mut TcpStream, payload: MessagePayload) -> Result<()> {
        let envelope = Envelope::signed(payload, &self.identity)?;
        write_envelope(stream, &envelope).await
    }

    async fn update_peer(&self, status: NetworkStatus, state: PeerState) -> Result<()> {
        let trusted = self
            .config
            .trusted_peers
            .get(&status.node_id)
            .ok_or_else(|| MiniChainError::InvalidPeerIdentity {
                peer: status.node_id.clone(),
            })?;
        let mut peers = self.peers.write().await;
        if !peers.contains_key(&status.node_id) && peers.len() >= self.config.max_peers {
            return Err(MiniChainError::PeerUnavailable {
                peer: status.node_id,
            });
        }
        peers.insert(
            status.node_id.clone(),
            Peer {
                id: status.node_id,
                address: trusted.address.clone(),
                state,
                last_heartbeat: Some(Utc::now()),
                height: status.height,
                latest_hash: status.latest_hash,
                protocol_version: status.protocol_version,
                connected_at: Utc::now(),
                latency_ms: None,
                failure_count: 0,
            },
        );
        Ok(())
    }

    async fn set_peer_state(&self, peer_id: &str, state: PeerState) {
        if let Some(peer) = self.peers.write().await.get_mut(peer_id) {
            peer.state = state;
        }
    }

    async fn mark_failed(&self, peer_id: &str) {
        if let Some(peer) = self.peers.write().await.get_mut(peer_id) {
            peer.state = PeerState::Failed;
            peer.failure_count = peer.failure_count.saturating_add(1);
        }
        let _ = self
            .events
            .send(NetworkEvent::PeerUnhealthy(peer_id.to_owned()));
    }

    async fn mark_diverged(&self, peer_id: &str, height: u64) {
        self.set_peer_state(peer_id, PeerState::Failed).await;
        let _ = self.events.send(NetworkEvent::DivergenceDetected {
            peer: peer_id.to_owned(),
            height,
        });
    }

    fn remember(&self, id: Uuid) -> bool {
        self.seen.lock().is_ok_and(|mut seen| seen.insert(id))
    }
}

struct SeenMessages {
    ids: HashSet<Uuid>,
    order: VecDeque<Uuid>,
    capacity: usize,
}

impl SeenMessages {
    fn new(capacity: usize) -> Self {
        Self {
            ids: HashSet::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn insert(&mut self, id: Uuid) -> bool {
        if !self.ids.insert(id) {
            return false;
        }
        self.order.push_back(id);
        if self.order.len() > self.capacity
            && let Some(expired) = self.order.pop_front()
        {
            self.ids.remove(&expired);
        }
        true
    }
}
