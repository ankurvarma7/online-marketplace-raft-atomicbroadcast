# Online Marketplace - PA2 (REST/gRPC/SOAP)

## System Design

This online marketplace extends PA1 by replacing the TCP-based communication with industry-standard protocols:

- **Client ↔ Frontend Servers**: RESTful HTTP/JSON (using `axum`)
- **Frontend Servers ↔ Backend Databases**: gRPC with Protocol Buffers (using `tonic`)
- **Financial Transactions**: SOAP/WSDL service for payment processing

The system consists of 7 components, each deployable as a separate process on individual VMs:

1. **Customer Database** (gRPC server, port 50051) — stores sellers, buyers, and session data
2. **Product Database** (gRPC server, port 50052) — stores items, shopping carts, and purchase history
3. **Seller Server** (REST API, port 8082) — stateless frontend for seller operations
4. **Buyer Server** (REST API, port 8083) — stateless frontend for buyer operations
5. **Financial Transactions** (SOAP/WSDL, port 8085) — simple payment processing (90% approve, 10% decline)
6. **Seller Client** — CLI that communicates with Seller Server via REST
7. **Buyer Client** — CLI that communicates with Buyer Server via REST

**Frontend servers are stateless**: all state (sessions, carts, items) is stored in the backend databases via gRPC. Frontend servers can be restarted without affecting client sessions.

### Assumptions
- In-memory storage (data lost on database restart, same as PA1)
- Plaintext passwords (security deferred to future assignments)
- Session timeout: 5 minutes of inactivity, auto-refreshed on each request
- UUIDs used for all identifiers
- Search semantics: items matching ALL provided keywords, sorted by keyword match count

## Product database (PA3) — five-node Raft

The **product_db** service uses [async-raft](https://crates.io/crates/async-raft) (0.6.1). Each node runs the same binary with a distinct `NODE_ID` and `BIND_ADDR`. Inter-node traffic uses gRPC service `RaftService` (AppendEntries, InstallSnapshot, Vote) with bincode-serialized Raft payloads. Client-facing RPCs remain `ProductDatabase` (same `proto/product_db.proto` as PA2; Python or other languages can generate stubs from that file).

- **Writes** go through Raft (`client_write`). If the node is not the leader, the gRPC status is `FAILED_PRECONDITION` with metadata `x-raft-leader-id` and `x-raft-leader-addr` when known.
- **Reads** call `client_read` first (linearizable on the leader); followers return the same redirect metadata.
- **Cluster**: set `RAFT_PEERS` to all five members with the same gRPC port on each host, e.g. `1=http://<ip1>:50052,2=http://<ip2>:50052,...,5=http://<ip5>:50052` (GCP internal IPs from `deploy_to_vms.sh`). After startup, each pristine node runs `initialize` with the full member set (see `RAFT_INIT_DELAY_SECS`). Data files live under `DATA_DIR` (default `./data/<NODE_ID>`).
- **Build only product_db**: `cargo build -p product_db --release`

## Architecture Diagram

```
Seller Client ──[REST/HTTP]──▶ Seller Server ──[gRPC]──▶ Customer Database
                                             ──[gRPC]──▶ Product Database (Raft x5)

Buyer Client  ──[REST/HTTP]──▶ Buyer Server  ──[gRPC]──▶ Customer Database
                                             ──[gRPC]──▶ Product Database (Raft x5)
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

## Running Locally

Start each component in a separate terminal:

```bash
# Terminal 1: Customer Database (gRPC)
./target/release/customer_db

# Terminal 2: Product Database (gRPC)
./target/release/product_db

# Terminal 3: Seller Server (REST)
./target/release/seller_server

# Terminal 4: Buyer Server (REST)
./target/release/buyer_server
To override the replica list at runtime:                                                                          
  CUSTOMER_DB_ADDRS="host1:50051,host2:50051,host3:50051,host4:50051,host5:50051" ./buyer_server
  
# Terminal 5: Financial Transactions (SOAP)
./target/release/financial_transactions
```

## Client Usage Examples

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

# Make purchase (new in PA2)
./target/release/buyer_client make-purchase -s <SESSION_ID> \
    --card-name "Bob Smith" --card-number "4111111111111111" \
    --expiration-date "12/28" --security-code "123"

# Logout
./target/release/buyer_client logout -s <SESSION_ID>
```

## Running Performance Evaluation

```bash
# Start all 5 server components first, then:
./target/release/evaluator
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `CUSTOMER_DB_BIND_ADDR` | `0.0.0.0:50051` | Customer DB bind address (used if customer_db reads it) |
| `SELLER_SERVER_BIND_ADDR` | `0.0.0.0:8082` | Seller server bind address |
| `BUYER_SERVER_BIND_ADDR` | `0.0.0.0:8083` | Buyer server bind address |
| `FINANCIAL_TX_BIND_ADDR` | `0.0.0.0:8085` | Financial Tx bind address |
| `CUSTOMER_DB_ADDR` | `127.0.0.1:50051` | Customer DB address (for frontends) |
| `PRODUCT_DB_ADDR` | `127.0.0.1:50052` | One Raft replica `host:port` for frontends (should be the **leader** for reliable reads/writes) |
| `FINANCIAL_TX_ADDR` | `127.0.0.1:8085` | Financial Tx address (for buyer server) |
| `SELLER_SERVER_ADDR` | `http://127.0.0.1:8082` | Seller server URL (for clients) |
| `BUYER_SERVER_ADDR` | `http://127.0.0.1:8083` | Buyer server URL (for clients) |
| `NODE_ID` | `1` | Raft node id (`product_db` only; 1–5 in the five-VM deployment) |
| `BIND_ADDR` | `127.0.0.1:50052` | gRPC listen address (`product_db`; use `0.0.0.0:50052` on GCP) |
| `DATA_DIR` | `./data/<NODE_ID>` | SQLite + snapshots (`product_db`) |
| `RAFT_PEERS` | (see `product_db` default in main.rs) | Comma-separated `id=http://host:port` listing **all** peers (same on every node) |
| `RAFT_INIT_DELAY_SECS` | `2` | Delay before `initialize` on pristine nodes |

## GCP Deployment (PA3)

PA3 requires five **product_db** Raft replicas as separate processes using the network. This repo uses **five VMs** (`product-db-1` … `product-db-5`), one replica each, all listening on **port 50052**, plus `customer-db`, `seller-server`, and `buyer-server` (eight VMs total). Internal IPs are resolved with `gcloud`; `RAFT_PEERS` is built as `1=http://<ip1>:50052,...,5=http://<ip5>:50052` and is identical on every node. `./deploy_to_vms.sh` cross-compiles for `x86_64-unknown-linux-gnu` (same pattern as PA2: `cargo zigbuild` on macOS), uploads the `product_db` binary to each product VM, and sets `NODE_ID`, `BIND_ADDR`, `DATA_DIR`, and `RAFT_PEERS`. Seller and buyer get `PRODUCT_DB_ADDR` pointing at **product-db-1** by default; if that node is not the Raft leader, update `PRODUCT_DB_ADDR` to the leader (or use gRPC metadata `x-raft-leader-addr` from redirect responses). After first deploy, optional `./setup_services.sh` installs matching **systemd** units so services survive reboots.

```bash
# Create VMs (Ubuntu 22.04, marketplace tag, internal firewall for 50051–50052, 8082–8083, 8085)
./create_vms.sh

# Build Linux release binaries and scp to VMs (requires rustup target + zig/cargo-zigbuild on macOS)
./deploy_to_vms.sh

# Optional: systemd auto-start on each VM
./setup_services.sh
```

## REST API Endpoints

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
| POST | `/buyer/{session_id}/purchase` | Make purchase (PA2) |

### Financial Transactions
| Method | Endpoint | Description |
|---|---|---|
| POST | `/financial/transaction` | Process SOAP transaction |
| GET | `/financial/wsdl` | Get WSDL definition |

## What Works
- All PA1 APIs ported to REST/gRPC
- MakePurchase API with SOAP financial transactions
- Session management with 5-minute timeout
- Stateless frontend design
- Performance evaluation across 3 scenarios
- GCP deployment scripts
