use anyhow::Context;
use async_raft::config::Config;
use async_raft::Raft;
use async_raft::RaftStorage;
use async_raft::NodeId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Server;

pub mod proto {
    tonic::include_proto!("product_db");
}

mod grpc;
mod network;
mod service;
mod storage;
mod types;

use crate::grpc::{ProductDbRaft, RaftGrpcServer};
use crate::network::ProductDbRouter;
use crate::proto::product_database_server::ProductDatabaseServer;
use crate::proto::raft_service_server::RaftServiceServer;
use crate::service::ProductDb;
use crate::storage::ProductDbStore;

fn parse_raft_peers(s: &str) -> HashMap<NodeId, String> {
    s.split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (k, v) = part.split_once('=')?;
            let id: NodeId = k.trim().parse().ok()?;
            Some((id, v.trim().to_string()))
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    let node_id: NodeId = std::env::var("NODE_ID")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .context("NODE_ID")?;
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:50052".into());
    let addr: std::net::SocketAddr = bind_addr.parse().context("BIND_ADDR")?;

    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| format!("./data/{}", node_id));
    let init_delay_secs: u64 = std::env::var("RAFT_INIT_DELAY_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    let peers_full = std::env::var("RAFT_PEERS").unwrap_or_else(|_| {
        format!(
            "1=http://127.0.0.1:50052,2=http://127.0.0.1:50053,3=http://127.0.0.1:50054,4=http://127.0.0.1:50055,5=http://127.0.0.1:50056"
        )
    });
    let all_peers = parse_raft_peers(&peers_full);

    let mut router_peers = all_peers.clone();
    router_peers.remove(&node_id);

    let leader_addrs: Arc<HashMap<NodeId, String>> = Arc::new(all_peers.clone());

    let storage = Arc::new(ProductDbStore::new(node_id, &data_dir).context("storage")?);
    let router = Arc::new(ProductDbRouter::new(router_peers));

    let config = Arc::new(
        Config::build("product-db-cluster".into())
            .snapshot_policy(async_raft::config::SnapshotPolicy::LogsSinceLast(500))
            .validate()
            .map_err(|e| anyhow::anyhow!("raft config: {:?}", e))?,
    );

    let raft: Arc<ProductDbRaft> = Arc::new(Raft::new(node_id, config, router, storage.clone()));

    let members: HashSet<NodeId> = all_peers.keys().copied().collect();

    let raft_for_init = raft.clone();
    let storage_for_init = storage.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(init_delay_secs)).await;
        let initial = match storage_for_init.as_ref().get_initial_state().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("get_initial_state: {}", e);
                return;
            }
        };
        if initial.last_log_index == 0 && initial.hard_state.current_term == 0 {
            match raft_for_init.initialize(members).await {
                Ok(()) => tracing::info!("Raft cluster initialized"),
                Err(e) => tracing::warn!("Raft initialize (ignore if already running): {:?}", e),
            }
        }
    });

    let db = ProductDb {
        raft: raft.clone(),
        store: storage,
        leader_addrs,
    };

    let raft_grpc = RaftGrpcServer { raft: raft.clone() };

    tracing::info!(
        "product_db node_id={} listening on {} (DATA_DIR={})",
        node_id,
        addr,
        data_dir
    );

    Server::builder()
        .add_service(ProductDatabaseServer::new(db))
        .add_service(RaftServiceServer::new(raft_grpc))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_raft_peers_basic() {
        let m = parse_raft_peers("1=http://a:1,2=http://b:2");
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(&1).map(String::as_str), Some("http://a:1"));
        assert_eq!(m.get(&2).map(String::as_str), Some("http://b:2"));
    }
}
