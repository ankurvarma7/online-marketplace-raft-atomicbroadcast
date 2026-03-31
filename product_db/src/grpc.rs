use async_raft::raft::{
    AppendEntriesRequest, InstallSnapshotRequest, VoteRequest,
};
use async_raft::Raft;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::proto::raft_service_server::RaftService;
use crate::proto::{
    RaftAppendEntriesRequest, RaftAppendEntriesResponse, RaftInstallSnapshotRequest,
    RaftInstallSnapshotResponse, RaftVoteRequest, RaftVoteResponse,
};
use crate::storage::ProductDbStore;
use crate::types::{ProductDbRequest, ProductDbResponse};

pub type ProductDbRaft = Raft<ProductDbRequest, ProductDbResponse, crate::network::ProductDbRouter, ProductDbStore>;

pub struct RaftGrpcServer {
    pub raft: Arc<ProductDbRaft>,
}

#[tonic::async_trait]
impl RaftService for RaftGrpcServer {
    async fn append_entries(
        &self,
        request: Request<RaftAppendEntriesRequest>,
    ) -> Result<Response<RaftAppendEntriesResponse>, Status> {
        let rpc: AppendEntriesRequest<ProductDbRequest> =
            bincode::deserialize(&request.into_inner().data).map_err(|e| Status::internal(e.to_string()))?;
        let out = self
            .raft
            .append_entries(rpc)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let data = bincode::serialize(&out).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RaftAppendEntriesResponse { data }))
    }

    async fn install_snapshot(
        &self,
        request: Request<RaftInstallSnapshotRequest>,
    ) -> Result<Response<RaftInstallSnapshotResponse>, Status> {
        let rpc: InstallSnapshotRequest =
            bincode::deserialize(&request.into_inner().data).map_err(|e| Status::internal(e.to_string()))?;
        let out = self
            .raft
            .install_snapshot(rpc)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let data = bincode::serialize(&out).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RaftInstallSnapshotResponse { data }))
    }

    async fn vote(&self, request: Request<RaftVoteRequest>) -> Result<Response<RaftVoteResponse>, Status> {
        let rpc: VoteRequest =
            bincode::deserialize(&request.into_inner().data).map_err(|e| Status::internal(e.to_string()))?;
        let out = self
            .raft
            .vote(rpc)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let data = bincode::serialize(&out).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RaftVoteResponse { data }))
    }
}
