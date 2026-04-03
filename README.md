# Online Marketplace — PA3 (Distributed Replication)

## PA3 system design (brief)

The system extends PA2 by replicating all server-side state. **Customer data** for PA3 is implemented in **Python** only: [`python_customer_db/`](python_customer_db) (five replicas, MySQL per node, gRPC to frontends). The Rust **`customer_db`** crate in this workspace is an **optional** single-process helper for local development and does **not** implement the PA3 atomic-broadcast protocol. The Python stack uses a **rotating sequencer atomic broadcast** over **UDP**: replicas exchange *Request*, *Sequence*, and *Retransmit* messages; the sequencer for global sequence `k` is member `k mod n`; delivery respects global order and majority-stable conditions from the assignment. **Product data** is replicated by five **Rust** [`product_db`](product_db) nodes with [**async-raft**](https://crates.io/crates/async-raft); peers use gRPC `RaftService` (AppendEntries, InstallSnapshot, Vote); clients use `ProductDatabase` with leader redirects (`x-raft-leader-addr`). **Seller** and **buyer** interfaces are **stateless**: four [`seller_server`](seller_server) and four [`buyer_server`](buyer_server) processes plus [`financial_transactions`](financial_transactions); clients and [`evaluator_distributed`](evaluator_distributed) use **`SELLER_SERVER_ADDRS`** and **`BUYER_SERVER_ADDRS`** (comma-separated URLs) for failover. **Communication** is unchanged from PA2: **REST** (client ↔ frontend), **gRPC** (frontend ↔ databases), **SOAP** (buyer ↔ financial). Each replica runs as a **separate process**; on cloud we use **Option C** (stacked processes on different ports across VMs). Full **VM / port matrix**: [`deployment_gcp/PA3_TOPOLOGY.md`](deployment_gcp/PA3_TOPOLOGY.md).

## PA2 communication stack (unchanged)

- **Client ↔ Frontend Servers**: RESTful HTTP/JSON (`axum`)
- **Frontend Servers ↔ Backend Databases**: gRPC with Protocol Buffers (`tonic`)
- **Financial Transactions**: SOAP/WSDL for payment processing

Logical components (each runs as its own process; PA3 scales counts as above):

1. **Customer Database** — **Python** [`python_customer_db`](python_customer_db): five replicas, atomic broadcast over UDP, **MySQL** (Docker/local) per replica, gRPC `CustomerDatabase` to seller/buyer servers (not the Rust `customer_db` binary)
2. **Product Database** — five Raft replicas (Rust `product_db`)
3. **Seller Server** — four REST frontends (stateless)
4. **Buyer Server** — four REST frontends (stateless)
5. **Financial Transactions** — one SOAP service (shared by buyer frontends)
6. **Seller Client** / **Buyer Client** — REST CLIs with replica lists
7. **Evaluator** — [`evaluator`](evaluator) (single-host); **distributed evaluator** — [`evaluator_distributed`](evaluator_distributed) for PA3 scenarios

**Frontend servers are stateless**: sessions, carts, and catalog state live in the replicated backends.

### Assumptions

**PA3 (from spec / design):**

- **Customer database** is the **Python** implementation (`python_customer_db`); five replicas; **MySQL** per node for seller/buyer/session storage; **not** the Rust `customer_db` crate for PA3 replication.
- Customer-group replicas **do not crash**; **communication is unreliable** (handled via broadcast and retransmit).
- **Raft** product replicas **may crash**; communication unreliable; restarts keep the same address where required.
- Frontends are **stateless**; no extra replication protocol beyond client-side replica lists.

**PA2 (APIs and behavior):**

- Plaintext passwords (security deferred)
- Session timeout: 5 minutes of inactivity, refreshed on use
- UUIDs for identifiers
- Search: items match **all** provided keywords, ordered by keyword match count

## Product database — five-node Raft

The **product_db** service uses [async-raft](https://crates.io/crates/async-raft) (0.6.1). Each node runs the same binary with a distinct `NODE_ID` and `BIND_ADDR`. Inter-node traffic uses gRPC service `RaftService` (AppendEntries, InstallSnapshot, Vote) with bincode-serialized Raft payloads. Client-facing RPCs remain `ProductDatabase` (same `proto/product_db.proto` as PA2).

- **Writes** go through Raft (`client_write`). If the node is not the leader, the gRPC status is `FAILED_PRECONDITION` with metadata `x-raft-leader-id` and `x-raft-leader-addr` when known.
- **Reads** call `client_read` first (linearizable on the leader); followers return the same redirect metadata.
- **Cluster**: set `RAFT_PEERS` to all five members, e.g. `1=http://<host>:50052,...,5=http://<host>:50056` on a single stacked VM or distinct hosts. After startup, pristine nodes run `initialize` with the full member set (see `RAFT_INIT_DELAY_SECS`). Data files live under `DATA_DIR` (default `./data/<NODE_ID>`).
- **Build only product_db**: `cargo build -p product_db --release`

## Architecture diagram

```
Seller Client ──[REST]──▶ Seller Server (×4) ──[gRPC]──▶ Customer DB (×5)
                                              ──[gRPC]──▶ Product DB / Raft (×5)

Buyer Client  ──[REST]──▶ Buyer Server (×4)  ──[gRPC]──▶ Customer DB (×5)
                                              ──[gRPC]──▶ Product DB / Raft (×5)
                                              ──[SOAP]──▶ Financial Transactions
```

## Building

```bash
# Install protoc (Protocol Buffers compiler) if not already installed
# macOS: brew install protobuf
# Ubuntu: apt-get install protobuf-compiler

# Build all components
cargo build --release
```

## Running locally

**Customer database (PA3):** use the **Python** service, not the Rust `customer_db` binary, for replicated atomic broadcast. Start five replicas (or one for smoke tests) via [`deployment/run_customer_db.sh`](deployment/run_customer_db.sh) — see script header for `PEERS`, `GRPC_BIND_ADDR`, MySQL, and venv setup. Requirements are listed in [`deployment_gcp/python_customer_requirements.txt`](deployment_gcp/python_customer_requirements.txt).

**Other services** (separate terminals; point frontends at your Python gRPC addresses with `CUSTOMER_DB_ADDRS`):

```bash
# Product Database (Raft) — one or five nodes per your RAFT_PEERS layout
./target/release/product_db

# Seller Server (REST) — e.g. CUSTOMER_DB_ADDRS='127.0.0.1:50051,...' ./target/release/seller_server
./target/release/seller_server

# Buyer Server (REST)
./target/release/buyer_server

# Financial Transactions (SOAP)
./target/release/financial_transactions
```

**Optional dev shortcut:** `./target/release/customer_db` (Rust) is a **non-PA3** single-node-style server for quick local tests without the Python/MySQL stack; it does not run the UDP atomic-broadcast protocol.

## Client usage examples

### Seller Client

```bash
# Create account
./target/release/seller_client create-account -n "Alice" -p "pass123"

# Login
./target/release/seller_client login -n "Alice" -p "pass123"

# Register item (use session_id from login)
./target/release/seller_client register-item -s <SESSION_ID> -n "Laptop" -c 1 \
    -k "laptop,computer,tech" --condition new --price 999.99 -q 10

# Display items
./target/release/seller_client display-items -s <SESSION_ID>

# Change price
./target/release/seller_client change-price -s <SESSION_ID> -i <ITEM_ID> -n 899.99

# Logout
./target/release/seller_client logout -s <SESSION_ID>
```

### Buyer Client

```bash
# Create account
./target/release/buyer_client create-account -n "Bob" -p "pass456"

# Login
./target/release/buyer_client login -n "Bob" -p "pass456"

# Search items
./target/release/buyer_client search -s <SESSION_ID> -k "laptop"

# Add to cart
./target/release/buyer_client add-to-cart -s <SESSION_ID> -i <ITEM_ID> -q 1

# Make purchase
./target/release/buyer_client make-purchase -s <SESSION_ID> \
    --card-name "Bob Smith" --card-number "4111111111111111" \
    --expiration-date "12/28" --security-code "123"

# Logout
./target/release/buyer_client logout -s <SESSION_ID>
```

## Running performance evaluation

```bash
# Local evaluator (after servers are up)
./target/release/evaluator

# PA3 distributed scenarios (replica URLs via env)
export SELLER_SERVER_ADDRS='http://127.0.0.1:8082,...'
export BUYER_SERVER_ADDRS='http://127.0.0.1:8083,...'
./target/release/evaluator_distributed --runs 10 --ops 1000
```

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `CUSTOMER_DB_BIND_ADDR` | `0.0.0.0:50051` | Python: `GRPC_BIND_ADDR` in `customer_db_service`; Rust `customer_db` only if you use that binary |
| `SELLER_SERVER_BIND_ADDR` | `0.0.0.0:8082` | Seller server bind address |
| `BUYER_SERVER_BIND_ADDR` | `0.0.0.0:8083` | Buyer server bind address |
| `FINANCIAL_TX_BIND_ADDR` | `0.0.0.0:8085` | Financial Tx bind address |
| `CUSTOMER_DB_ADDR` | `127.0.0.1:50051` | One Python replica `host:port` for frontends (leader routing is via customer layer; typically list all in `CUSTOMER_DB_ADDRS`) |
| `PRODUCT_DB_ADDR` | `127.0.0.1:50052` | One Raft replica `host:port` for frontends (should be the **leader** for reliable reads/writes) |
| `FINANCIAL_TX_ADDR` | `127.0.0.1:8085` | Financial Tx address (for buyer server) |
| `SELLER_SERVER_ADDR` | `http://127.0.0.1:8082` | Seller server URL (single-replica clients) |
| `BUYER_SERVER_ADDR` | `http://127.0.0.1:8083` | Buyer server URL (single-replica clients) |
| `SELLER_SERVER_ADDRS` | (see seller_client) | Comma-separated seller URLs (PA3 failover) |
| `BUYER_SERVER_ADDRS` | (see buyer_client) | Comma-separated buyer URLs (PA3 failover) |
| `NODE_ID` | `1` | Raft node id (`product_db` only; 1–5 on `product-stack` in Option C) |
| `BIND_ADDR` | `127.0.0.1:50052` | gRPC listen address (`product_db`; use `0.0.0.0:50052` on GCP) |
| `DATA_DIR` | `./data/<NODE_ID>` | SQLite + snapshots (`product_db`) |
| `RAFT_PEERS` | (see `product_db` default in main.rs) | Comma-separated `id=http://host:port` listing **all** peers (same on every node) |
| `RAFT_INIT_DELAY_SECS` | `2` | Delay before `initialize` on pristine nodes |

## GCP deployment (Option C)

**Option C** uses **at least four VMs** (we use **five**): stacked processes on different ports to stay within typical cloud IP quotas. Roles:

| VM | Role |
|----|------|
| `customer-stack` | Five Python `customer_db` processes (operator-managed); gRPC `50051`–`50055` by default |
| `product-stack` | Five Rust `product_db` Raft nodes; ports `50052`–`50056` |
| `seller-frontend` | Four `seller_server` processes |
| `buyer-frontend` | `financial_transactions` + four `buyer_server` processes |
| `client-runner` | Optional host for evaluator / clients |

**Port and process details** are in [`deployment_gcp/PA3_TOPOLOGY.md`](deployment_gcp/PA3_TOPOLOGY.md). Scripts: [`create_vms.sh`](create_vms.sh), [`deploy_to_vms.sh`](deploy_to_vms.sh); optional [`setup_services.sh`](setup_services.sh). **`deploy_to_vms.sh` does not install Python customer_db**—start customer replicas first (or export `CUSTOMER_DB_ADDRS` before deploy to match your ports). More context: [`deployment_gcp/README.md`](deployment_gcp/README.md).

```bash
./create_vms.sh
./deploy_to_vms.sh
# optional:
./setup_services.sh
```

## REST API endpoints

### Seller Server

| Method | Endpoint | Description |
|---|---|---|
| POST | `/seller/create-account` | Create seller account |
| POST | `/seller/login` | Login |
| POST | `/seller/{session_id}/logout` | Logout |
| GET | `/seller/{session_id}/rating` | Get seller rating |
| POST | `/seller/{session_id}/items` | Register item for sale |
| GET | `/seller/{session_id}/items` | Display items for sale |
| PUT | `/seller/{session_id}/items/{item_id}/price` | Change item price |
| PUT | `/seller/{session_id}/items/{item_id}/quantity` | Update units for sale |

### Buyer Server

| Method | Endpoint | Description |
|---|---|---|
| POST | `/buyer/create-account` | Create buyer account |
| POST | `/buyer/login` | Login |
| POST | `/buyer/{session_id}/logout` | Logout |
| POST | `/buyer/{session_id}/search` | Search items |
| GET | `/buyer/{session_id}/items/{item_id}` | Get item details |
| POST | `/buyer/{session_id}/cart/add` | Add to cart |
| POST | `/buyer/{session_id}/cart/remove` | Remove from cart |
| POST | `/buyer/{session_id}/cart/save` | Save cart |
| POST | `/buyer/{session_id}/cart/clear` | Clear cart |
| GET | `/buyer/{session_id}/cart` | Display cart |
| POST | `/buyer/{session_id}/feedback` | Provide feedback |
| GET | `/buyer/{session_id}/seller-rating/{seller_id}` | Get seller rating |
| GET | `/buyer/{session_id}/purchases` | Get purchase history |
| POST | `/buyer/{session_id}/purchase` | Make purchase |

### Financial Transactions

| Method | Endpoint | Description |
|---|---|---|
| POST | `/financial/transaction` | Process SOAP transaction |
| GET | `/financial/wsdl` | Get WSDL definition |

## Current state

**Working:**

- PA2-equivalent APIs over **REST / gRPC / SOAP** with replicated backends as described above.
- **Product database**: five-node Raft (`product_db`), leader redirects, SQLite storage per node.
- **Customer database**: **Python** [`python_customer_db`](python_customer_db) only for PA3 (five replicas, UDP broadcast, MySQL). Rust `customer_db` is optional and not used for PA3-compliant replication.
- **Frontends**: multiple seller/buyer processes; clients and **`evaluator_distributed`** use replica URL lists and failover.
- **Evaluators**: [`evaluator`](evaluator) and [`evaluator_distributed`](evaluator_distributed) for throughput/response-time scenarios.
- **GCP**: [`create_vms.sh`](create_vms.sh), [`deploy_to_vms.sh`](deploy_to_vms.sh) for Option C; see [`deployment_gcp/`](deployment_gcp/).

**Limitations / notes:**

- On **GCP**, you **start Python `customer_db` yourself** on `customer-stack` (see [`deployment_gcp/install_customer_node.sh`](deployment_gcp/install_customer_node.sh) or your own process manager). `deploy_to_vms.sh` only deploys Rust binaries; set `CUSTOMER_DB_ADDRS` to match your Python gRPC ports before or after frontends run.
- **High concurrency** (e.g. evaluator Scenario 3) on **small VMs** may show timeouts or transport pressure; scale machine size or reduce load for stable measurements.
- **Performance numbers** belong in the separate **performance report** required by PA3 (not duplicated here).

Submit a **single `.zip`** with source, deployment files, this **README**, and that performance report, per the course instructions.

╔══════════════════════════════════════════════════════════════╗
║                      Results Summary                         ║
╚══════════════════════════════════════════════════════════════╝

Scenario 1 (1×1):
  Average Response Time:  4.72ms
  Average Throughput:     0.64 ops/sec
  Average Wall Time:      15.51s
  Total Operations:       83 ok / 100 total (17.0% failures) across 10 runs

Scenario 2 (10×10):
  Average Response Time:  9.20ms
  Average Throughput:     86.61 ops/sec
  Average Wall Time:      496.01ms
  Total Operations:       288 ok / 1000 total (71.2% failures) across 10 runs

Scenario 3 (100×100):
  Average Response Time:  111.37ms
  Average Throughput:     66.64 ops/sec
  Average Wall Time:      4.54s
  Total Operations:       3007 ok / 10000 total (69.9% failures) across 10 runs

=== Notes ===
- Clients are distributed round-robin across replicas.
- Sessions persist across replicas via customer_db total-order broadcast.
- Item visibility across replicas is guaranteed by product_db Raft replication.
- Failure rate > 0 indicates a replica is down or replication lag exceeded the 10s timeout.