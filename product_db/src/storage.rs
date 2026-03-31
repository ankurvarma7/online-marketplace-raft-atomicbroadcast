use crate::types::{ProductDbRequest, ProductDbResponse};
use anyhow::{anyhow, Context, Result};
use async_raft::raft::{Entry, EntryPayload, MembershipConfig};
use async_raft::storage::{CurrentSnapshotData, HardState, InitialState};
use async_raft::RaftStorage;
use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt};
use uuid::Uuid;

/// Application-specific fatal error (triggers Raft shutdown if returned from apply).
#[derive(Debug)]
pub struct StorageShutdown;

impl std::fmt::Display for StorageShutdown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage shutdown")
    }
}

impl std::error::Error for StorageShutdown {}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotMeta {
    term: u64,
    index: u64,
    membership: MembershipConfig,
}

pub struct ProductDbStore {
    pub node_id: u64,
    db_path: PathBuf,
    snapshot_dir: PathBuf,
    pub conn: Arc<Mutex<Connection>>,
    /// Cached current snapshot id + meta for `get_current_snapshot`.
    snapshot_info: Arc<tokio::sync::RwLock<Option<(String, SnapshotMeta)>>>,
}

impl ProductDbStore {
    pub fn new(node_id: u64, data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir).context("create data dir")?;
        let snapshot_dir = data_dir.join("snapshots");
        std::fs::create_dir_all(&snapshot_dir).context("create snapshot dir")?;

        let db_path = data_dir.join(format!("product_db_{}.sqlite", node_id));
        let conn = Connection::open(&db_path).context("open sqlite")?;
        init_business_schema(&conn)?;
        init_raft_schema(&conn)?;

        Ok(Self {
            node_id,
            db_path,
            snapshot_dir,
            conn: Arc::new(Mutex::new(conn)),
            snapshot_info: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    fn decode_entry(data: &[u8]) -> Result<Entry<ProductDbRequest>> {
        bincode::deserialize(data).context("decode entry")
    }

    fn encode_entry(entry: &Entry<ProductDbRequest>) -> Result<Vec<u8>> {
        Ok(bincode::serialize(entry)?)
    }

    fn set_last_applied(&self, idx: u64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO raft_meta (k, v) VALUES ('last_applied', ?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![idx.to_string()],
        )?;
        Ok(())
    }

    async fn term_at_index(&self, index: u64) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let t: u64 = conn.query_row(
            "SELECT term FROM raft_log WHERE idx = ?1",
            params![index],
            |r| r.get(0),
        )?;
        Ok(t)
    }

    async fn last_applied_index(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let row: Option<String> = conn
            .query_row(
                "SELECT v FROM raft_meta WHERE k = 'last_applied'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(row.and_then(|s| s.parse().ok()).unwrap_or(0))
    }
}

fn init_business_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS items (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, category INTEGER NOT NULL,
            keywords TEXT, condition TEXT NOT NULL, price REAL NOT NULL,
            quantity INTEGER NOT NULL, seller_id TEXT NOT NULL,
            thumbs_up INTEGER NOT NULL, thumbs_down INTEGER NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS carts (
            buyer_id TEXT NOT NULL, item_id TEXT NOT NULL, quantity INTEGER NOT NULL,
            PRIMARY KEY (buyer_id, item_id)
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS purchase_history (
            buyer_id TEXT NOT NULL, item_id TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

fn init_raft_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS raft_log (
            idx INTEGER PRIMARY KEY,
            term INTEGER NOT NULL,
            payload BLOB NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS raft_meta (
            k TEXT PRIMARY KEY,
            v TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

fn apply_request(
    conn: &Connection,
    data: &ProductDbRequest,
) -> rusqlite::Result<ProductDbResponse> {
    use crate::types::CartItemData;
    match data {
        ProductDbRequest::CreateItem(item) => {
            let item_id = Uuid::new_v4().to_string();
            let keywords = item.keywords.join(",");
            conn.execute(
                "INSERT INTO items (id, name, category, keywords, condition, price, quantity, seller_id, thumbs_up, thumbs_down) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    item_id,
                    item.item_name,
                    item.item_category,
                    keywords,
                    item.condition,
                    item.sale_price,
                    item.quantity,
                    item.seller_id,
                    item.thumbs_up,
                    item.thumbs_down
                ],
            )?;
            Ok(ProductDbResponse {
                data: Some(item_id),
            })
        }
        ProductDbRequest::UpdateItem(item) => {
            let keywords = item.keywords.join(",");
            conn.execute(
                "UPDATE items SET name = ?1, category = ?2, keywords = ?3, condition = ?4, price = ?5, quantity = ?6, thumbs_up = ?7, thumbs_down = ?8 WHERE id = ?9",
                params![
                    item.item_name,
                    item.item_category,
                    keywords,
                    item.condition,
                    item.sale_price,
                    item.quantity,
                    item.thumbs_up,
                    item.thumbs_down,
                    item.item_id
                ],
            )?;
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::AddToCart {
            buyer_id,
            item_id,
            quantity,
        } => {
            conn.execute(
                "INSERT INTO carts (buyer_id, item_id, quantity) VALUES (?1, ?2, ?3) ON CONFLICT(buyer_id, item_id) DO UPDATE SET quantity = quantity + ?3",
                params![buyer_id, item_id, quantity],
            )?;
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::RemoveFromCart {
            buyer_id,
            item_id,
            quantity,
        } => {
            let current_quantity: i32 = conn
                .query_row(
                    "SELECT quantity FROM carts WHERE buyer_id = ?1 AND item_id = ?2",
                    params![buyer_id, item_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);

            if current_quantity <= *quantity {
                conn.execute(
                    "DELETE FROM carts WHERE buyer_id = ?1 AND item_id = ?2",
                    params![buyer_id, item_id],
                )?;
            } else {
                conn.execute(
                    "UPDATE carts SET quantity = quantity - ?1 WHERE buyer_id = ?2 AND item_id = ?3",
                    params![quantity, buyer_id, item_id],
                )?;
            }
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::ClearCart(buyer_id) => {
            conn.execute(
                "DELETE FROM carts WHERE buyer_id = ?1",
                params![buyer_id],
            )?;
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::SaveCart { buyer_id, items } => {
            conn.execute("DELETE FROM carts WHERE buyer_id = ?1", params![buyer_id])?;
            for ci in items {
                let CartItemData { item_id, quantity } = ci;
                conn.execute(
                    "INSERT INTO carts (buyer_id, item_id, quantity) VALUES (?1, ?2, ?3)",
                    params![buyer_id, item_id, quantity],
                )?;
            }
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::AddPurchaseHistory { buyer_id, item_id } => {
            conn.execute(
                "INSERT INTO purchase_history (buyer_id, item_id) VALUES (?1, ?2)",
                params![buyer_id, item_id],
            )?;
            Ok(ProductDbResponse { data: None })
        }
        ProductDbRequest::Reset => {
            conn.execute("DELETE FROM items", [])?;
            conn.execute("DELETE FROM carts", [])?;
            conn.execute("DELETE FROM purchase_history", [])?;
            Ok(ProductDbResponse { data: None })
        }
    }
}

#[async_trait]
impl RaftStorage<ProductDbRequest, ProductDbResponse> for ProductDbStore {
    type Snapshot = File;
    type ShutdownError = StorageShutdown;

    async fn get_membership_config(&self) -> Result<MembershipConfig> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT payload FROM raft_log ORDER BY idx DESC")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let payload: Vec<u8> = row.get(0)?;
            if let Ok(entry) = Self::decode_entry(&payload) {
                match entry.payload {
                    EntryPayload::ConfigChange(cc) => return Ok(cc.membership),
                    EntryPayload::SnapshotPointer(ptr) => return Ok(ptr.membership),
                    _ => {}
                }
            }
        }
        Ok(MembershipConfig::new_initial(self.node_id))
    }

    async fn get_initial_state(&self) -> Result<InitialState> {
        let conn = self.conn.lock().unwrap();
        let last_log_index: u64 = conn
            .query_row("SELECT COALESCE(MAX(idx), 0) FROM raft_log", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        let last_log_term: u64 = if last_log_index == 0 {
            0
        } else {
            conn.query_row(
                "SELECT term FROM raft_log WHERE idx = ?1",
                params![last_log_index],
                |r| r.get(0),
            )?
        };
        let last_applied: u64 = conn
            .query_row(
                "SELECT v FROM raft_meta WHERE k = 'last_applied'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let hs: HardState = conn
            .query_row(
                "SELECT v FROM raft_meta WHERE k = 'hard_state'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or(HardState {
                current_term: 0,
                voted_for: None,
            });

        let membership = {
            let mut stmt = conn.prepare("SELECT payload FROM raft_log ORDER BY idx DESC")?;
            let mut rows = stmt.query([])?;
            let mut m = None;
            while let Some(row) = rows.next()? {
                let payload: Vec<u8> = row.get(0)?;
                if let Ok(entry) = Self::decode_entry(&payload) {
                    match entry.payload {
                        EntryPayload::ConfigChange(cc) => {
                            m = Some(cc.membership);
                            break;
                        }
                        EntryPayload::SnapshotPointer(ptr) => {
                            m = Some(ptr.membership);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            m.unwrap_or_else(|| MembershipConfig::new_initial(self.node_id))
        };

        Ok(InitialState {
            last_log_index: last_log_index,
            last_log_term,
            last_applied_log: last_applied,
            hard_state: hs,
            membership,
        })
    }

    async fn save_hard_state(&self, hs: &HardState) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let s = serde_json::to_string(hs)?;
        conn.execute(
            "INSERT INTO raft_meta (k, v) VALUES ('hard_state', ?1) ON CONFLICT(k) DO UPDATE SET v = excluded.v",
            params![s],
        )?;
        Ok(())
    }

    async fn get_log_entries(&self, start: u64, stop: u64) -> Result<Vec<Entry<ProductDbRequest>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT payload FROM raft_log WHERE idx >= ?1 AND idx < ?2 ORDER BY idx ASC")?;
        let mut rows = stmt.query(params![start, stop])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let payload: Vec<u8> = row.get(0)?;
            out.push(Self::decode_entry(&payload)?);
        }
        Ok(out)
    }

    async fn delete_logs_from(&self, start: u64, stop: Option<u64>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        match stop {
            Some(s) => conn.execute(
                "DELETE FROM raft_log WHERE idx >= ?1 AND idx < ?2",
                params![start, s],
            )?,
            None => conn.execute("DELETE FROM raft_log WHERE idx >= ?1", params![start])?,
        };
        Ok(())
    }

    async fn append_entry_to_log(&self, entry: &Entry<ProductDbRequest>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let payload = Self::encode_entry(entry)?;
        conn.execute(
            "INSERT INTO raft_log (idx, term, payload) VALUES (?1, ?2, ?3)",
            params![entry.index, entry.term, payload],
        )?;
        Ok(())
    }

    async fn replicate_to_log(&self, entries: &[Entry<ProductDbRequest>]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for entry in entries {
            let payload = Self::encode_entry(entry)?;
            conn.execute(
                "INSERT OR REPLACE INTO raft_log (idx, term, payload) VALUES (?1, ?2, ?3)",
                params![entry.index, entry.term, payload],
            )?;
        }
        Ok(())
    }

    async fn apply_entry_to_state_machine(
        &self,
        index: &u64,
        data: &ProductDbRequest,
    ) -> Result<ProductDbResponse> {
        let conn = self.conn.lock().unwrap();
        let res = apply_request(&conn, data).map_err(|e| anyhow!(e))?;
        drop(conn);
        self.set_last_applied(*index)?;
        Ok(res)
    }

    async fn replicate_to_state_machine(&self, entries: &[(&u64, &ProductDbRequest)]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for (_idx, data) in entries {
            apply_request(&conn, data).map_err(|e| anyhow!(e))?;
        }
        drop(conn);
        if let Some((last_idx, _)) = entries.last() {
            self.set_last_applied(**last_idx)?;
        }
        Ok(())
    }

    async fn do_log_compaction(&self) -> Result<CurrentSnapshotData<Self::Snapshot>> {
        let last_applied = self.last_applied_index().await?;
        let term = if last_applied == 0 {
            0
        } else {
            self.term_at_index(last_applied).await?
        };
        let membership = self.get_membership_config().await?;
        let snap_id = Uuid::new_v4().to_string();
        let snap_path = self.snapshot_dir.join(format!("{}.db", snap_id));
        let meta_path = self.snapshot_dir.join(format!("{}.meta", snap_id));

        let src = self.db_path.clone();
        fs::copy(&src, &snap_path)
            .await
            .context("snapshot copy")?;

        let meta = SnapshotMeta {
            term,
            index: last_applied,
            membership: membership.clone(),
        };
        fs::write(&meta_path, serde_json::to_vec(&meta)?).await?;

        let mut info = self.snapshot_info.write().await;
        *info = Some((snap_id.clone(), meta.clone()));

        let file = File::open(&snap_path).await?;
        Ok(CurrentSnapshotData {
            term: meta.term,
            index: meta.index,
            membership: meta.membership,
            snapshot: Box::new(file),
        })
    }

    async fn create_snapshot(&self) -> Result<(String, Box<Self::Snapshot>)> {
        let id = Uuid::new_v4().to_string();
        let path = self.snapshot_dir.join(format!("recv_{}.dat", id));
        let f = File::create(&path).await?;
        Ok((path.to_string_lossy().to_string(), Box::new(f)))
    }

    async fn finalize_snapshot_installation(
        &self,
        _index: u64,
        _term: u64,
        _delete_through: Option<u64>,
        id: String,
        mut snapshot: Box<Self::Snapshot>,
    ) -> Result<()> {
        snapshot.flush().await?;
        snapshot.shutdown().await?;
        drop(snapshot);

        let tmp_path = PathBuf::from(&id);
        if !tmp_path.exists() {
            return Err(anyhow!("snapshot path missing"));
        }

        // Release lock on current DB file, then replace.
        {
            let mut guard = self.conn.lock().unwrap();
            *guard = Connection::open(":memory:")?;
        }

        fs::copy(&tmp_path, &self.db_path).await?;
        let _ = fs::remove_file(&tmp_path).await;

        {
            let mut guard = self.conn.lock().unwrap();
            *guard = Connection::open(&self.db_path).context("reopen after snapshot")?;
        }

        Ok(())
    }

    async fn get_current_snapshot(&self) -> Result<Option<CurrentSnapshotData<Self::Snapshot>>> {
        let info = self.snapshot_info.read().await.clone();
        let Some((snap_id, meta)) = info else {
            return Ok(None);
        };
        let snap_path = self.snapshot_dir.join(format!("{}.db", snap_id));
        if !snap_path.exists() {
            return Ok(None);
        }
        let file = File::open(&snap_path).await?;
        Ok(Some(CurrentSnapshotData {
            term: meta.term,
            index: meta.index,
            membership: meta.membership,
            snapshot: Box::new(file),
        }))
    }
}
