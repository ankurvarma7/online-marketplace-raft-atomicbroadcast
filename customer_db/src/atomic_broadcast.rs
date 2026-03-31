use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

pub type NodeId = u32;
pub type LocalSeq = u64;
pub type GlobalSeq = u64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestId {
    pub node_id: NodeId,
    pub local_seq: LocalSeq,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientRequest {
    // Customer DB specific requests will be mirrored here
    CreateSeller { name: String, pass: String },
    CreateBuyer { name: String, pass: String },
    // ... other request types
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RequestMessage {
    pub request_id: RequestId,
    pub client_request: ClientRequest,
    // Metadata: highest sequence number delivered by the sender
    pub highest_delivered: GlobalSeq,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SequenceMessage {
    pub global_seq: GlobalSeq,
    pub request_id: RequestId,
    // Metadata: highest sequence number received by the sequencer
    pub highest_received: GlobalSeq,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RetransmitRequest {
    Request(RequestId),
    Sequence(GlobalSeq),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ProtocolMessage {
    Request(RequestMessage),
    Sequence(SequenceMessage),
    Retransmit(RetransmitRequest),
}

pub struct AtomicBroadcastNode {
    pub id: NodeId,
    num_nodes: u32,
    socket: Arc<UdpSocket>,
    peers: HashMap<NodeId, SocketAddr>,
    
    // --- Protocol State ---
    local_seq: LocalSeq,
    global_seq: GlobalSeq,
    
    // Buffers for holding messages
    pending_requests: HashMap<RequestId, RequestMessage>,
    sequenced_requests: BTreeMap<GlobalSeq, RequestId>,
    delivered_requests: HashSet<RequestId>,

    // Tracking for delivery conditions and retransmissions
    received_sequences: HashSet<GlobalSeq>,
    node_delivery_acks: HashMap<NodeId, GlobalSeq>,

    // Connection to the actual database
    db_conn: Arc<Mutex<rusqlite::Connection>>,
}

impl AtomicBroadcastNode {
    pub async fn new(
        id: NodeId,
        peers: HashMap<NodeId, SocketAddr>,
        db_conn: Arc<Mutex<rusqlite::Connection>>,
    ) -> anyhow::Result<Self> {
        let num_nodes = peers.len() as u32;
        let own_addr = peers.get(&id).unwrap();
        let socket = Arc::new(UdpSocket::bind(own_addr).await?);

        Ok(Self {
            id,
            num_nodes,
            socket,
            peers,
            local_seq: 0,
            global_seq: 0,
            pending_requests: HashMap::new(),
            sequenced_requests: BTreeMap::new(),
            delivered_requests: HashSet::new(),
            received_sequences: HashSet::new(),
            node_delivery_acks: HashMap::new(),
            db_conn,
        })
    }

    pub async fn run(&mut self) {
        // Main event loop will go here
        // 1. Listen for incoming UDP packets
        // 2. Handle client requests (from gRPC service)
        // 3. Process protocol messages
        // 4. Check for message delivery
        // 5. Handle retransmissions
    }

    async fn broadcast(&self, message: &ProtocolMessage) -> anyhow::Result<()> {
        let bytes = bincode::serialize(message)?;
        for (id, addr) in &self.peers {
            if *id != self.id {
                self.socket.send_to(&bytes, addr).await?;
            }
        }
        Ok(())
    }
    
    // ... other protocol logic methods ...
}
