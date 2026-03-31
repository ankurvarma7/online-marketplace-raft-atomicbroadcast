use crate::types::ProductDbRequest;
use anyhow::{anyhow, Result};
use async_raft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use async_raft::{NodeId, RaftNetwork};
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use crate::proto::raft_service_client::RaftServiceClient;

/// Outbound Raft RPCs to peer product_db nodes over gRPC.
#[derive(Clone)]
pub struct ProductDbRouter {
    /// Peer node id -> gRPC base URL (e.g. `http://127.0.0.1:50052`).
    pub peers: HashMap<NodeId, String>,
}

impl ProductDbRouter {
    pub fn new(peers: HashMap<NodeId, String>) -> Self {
        Self { peers }
    }
}

#[async_trait]
impl RaftNetwork<ProductDbRequest> for ProductDbRouter {
    async fn append_entries(
        &self,
        target: NodeId,
        rpc: AppendEntriesRequest<ProductDbRequest>,
    ) -> Result<AppendEntriesResponse> {
        let url = self
            .peers
            .get(&target)
            .ok_or_else(|| anyhow!("unknown raft peer {}", target))?
            .clone();
        let mut client = RaftServiceClient::connect(url)
            .await
            .map_err(|e| anyhow!("connect peer {}: {}", target, e))?;
        let bytes = bincode::serialize(&rpc)?;
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            client.append_entries(crate::proto::RaftAppendEntriesRequest { data: bytes }),
        )
        .await
        .map_err(|_| anyhow!("timeout append_entries to {}", target))?
        .map_err(|e| anyhow!("append_entries to {}: {}", target, e))?;
        Ok(bincode::deserialize(&resp.into_inner().data)?)
    }

    async fn install_snapshot(
        &self,
        target: NodeId,
        rpc: InstallSnapshotRequest,
    ) -> Result<InstallSnapshotResponse> {
        let url = self
            .peers
            .get(&target)
            .ok_or_else(|| anyhow!("unknown raft peer {}", target))?
            .clone();
        let mut client = RaftServiceClient::connect(url)
            .await
            .map_err(|e| anyhow!("connect peer {}: {}", target, e))?;
        let bytes = bincode::serialize(&rpc)?;
        let resp = tokio::time::timeout(
            Duration::from_secs(120),
            client.install_snapshot(crate::proto::RaftInstallSnapshotRequest { data: bytes }),
        )
        .await
        .map_err(|_| anyhow!("timeout install_snapshot to {}", target))?
        .map_err(|e| anyhow!("install_snapshot to {}: {}", target, e))?;
        Ok(bincode::deserialize(&resp.into_inner().data)?)
    }

    async fn vote(&self, target: NodeId, rpc: VoteRequest) -> Result<VoteResponse> {
        let url = self
            .peers
            .get(&target)
            .ok_or_else(|| anyhow!("unknown raft peer {}", target))?
            .clone();
        let mut client = RaftServiceClient::connect(url)
            .await
            .map_err(|e| anyhow!("connect peer {}: {}", target, e))?;
        let bytes = bincode::serialize(&rpc)?;
        let resp = tokio::time::timeout(
            Duration::from_secs(10),
            client.vote(crate::proto::RaftVoteRequest { data: bytes }),
        )
        .await
        .map_err(|_| anyhow!("timeout vote to {}", target))?
        .map_err(|e| anyhow!("vote to {}: {}", target, e))?;
        Ok(bincode::deserialize(&resp.into_inner().data)?)
    }
}
