# Deployment Scripts

All scripts live in `deployment/` and must be run from the **repository root**.

Each script supports three modes:

| Mode | What it does |
|------|--------------|
| `local` | Build (if needed) and start all replicas on this machine with default ports |
| `single` | Start exactly one replica on this machine — for multi-machine deployments |
| `stop` | Stop the local replica identified by `NODE_ID` or `BIND_ADDR` |

---

## Start order

Start services in this order so that each layer can reach its dependencies:

1. `run_customer_db.sh` — customer DB cluster (total-order broadcast)
2. `run_product_db.sh` — product DB Raft cluster
3. `run_buyer_server.sh` + `run_seller_server.sh` — application servers

---

## run_customer_db.sh

Python replicas using rotating-sequencer total-order broadcast.  
Stop is keyed by **`NODE_ID`** (0-indexed, 0–4).

### Local (all 5 nodes on one machine)

```bash
./deployment/run_customer_db.sh local
```

Default ports — gRPC: 50051–50055, UDP: 50000–50004, MySQL: 8000–8004.

### Single node (multi-machine)

Run once per machine. Set `PEERS` to the real IPs of all nodes.

```bash
NODE_ID=2 \
PEERS="0=10.0.1.1:50000,1=10.0.1.2:50000,2=10.0.1.3:50000,3=10.0.1.4:50000,4=10.0.1.5:50000" \
GRPC_BIND_ADDR="0.0.0.0:50051" \
UDP_BIND_ADDR="0.0.0.0:50000" \
CUSTOMER_DB_HOST="127.0.0.1" \
CUSTOMER_DB_PORT="8000" \
./deployment/run_customer_db.sh single
```

### Stop

```bash
NODE_ID=2 ./deployment/run_customer_db.sh stop
```

### Key env vars

| Variable | Default | Description |
|----------|---------|-------------|
| `NUM_NODES` | `5` | Total replica count |
| `PEERS` | all nodes on localhost | `<id>=<ip>:<udp_port>` for every node |
| `GRPC_BIND_ADDR` | `0.0.0.0:50051+NODE_ID` | gRPC listen address |
| `UDP_BIND_ADDR` | from `PEERS[NODE_ID]` | UDP listen address |
| `CUSTOMER_DB_HOST` | `127.0.0.1` | MySQL host |
| `CUSTOMER_DB_PORT` | `8000` | MySQL port for this node's DB |
| `CUSTOMER_DB_USER` | `root` | MySQL user |
| `CUSTOMER_DB_PASSWORD` | `my-secret-pw` | MySQL password |
| `CUSTOMER_DB_NAME` | `customer_db` | MySQL database name |
| `DELIVER_TIMEOUT` | `30.0` | Seconds a gRPC handler waits for delivery |
| `SESSION_TTL` | `300` | Session expiry in seconds |

---

## run_product_db.sh

Rust binary, 5-node Raft cluster.  
Stop is keyed by **`NODE_ID`** (1-indexed, 1–5).

### Local (all 5 nodes on one machine)

```bash
./deployment/run_product_db.sh local
```

Default ports: 50052–50056 (node N binds on 50051 + N).

### Single node (multi-machine)

```bash
NODE_ID=3 \
RAFT_PEERS="1=http://10.0.2.1:50052,2=http://10.0.2.2:50052,3=http://10.0.2.3:50052,4=http://10.0.2.4:50052,5=http://10.0.2.5:50052" \
BIND_ADDR="0.0.0.0:50052" \
DATA_DIR="/var/lib/product_db/node_3" \
./deployment/run_product_db.sh single
```

### Stop

```bash
NODE_ID=3 ./deployment/run_product_db.sh stop
```

### Key env vars

| Variable | Default | Description |
|----------|---------|-------------|
| `NUM_NODES` | `5` | Cluster size |
| `GRPC_BASE_PORT` | `50051` | Node N binds on BASE + N |
| `RAFT_PEERS` | all nodes on localhost | `<id>=http://<ip>:<port>` for every node |
| `RAFT_INIT_DELAY_SECS` | `2` | Wait before attempting Raft initialisation |
| `BIND_ADDR` | `0.0.0.0:50051+NODE_ID` | gRPC listen address (single mode) |
| `DATA_DIR` | `logs/product_db_data/<NODE_ID>` | Raft log and snapshot storage |
| `BINARY_DIR` | `./target/release` | Directory containing the compiled binary |

---

## run_buyer_server.sh

Rust binary, stateless REST replicas.  
Stop is keyed by **`BIND_ADDR`** (specifically the port).

### Local (4 replicas on one machine)

```bash
./deployment/run_buyer_server.sh local
```

Default ports: 8083, 8084, 8086, 8087.

### Single replica (multi-machine)

```bash
BIND_ADDR="0.0.0.0:8083" \
CUSTOMER_DB_ADDRS="10.0.1.1:50051,10.0.1.2:50051,10.0.1.3:50051,10.0.1.4:50051,10.0.1.5:50051" \
PRODUCT_DB_PEERS="127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056" \
FINANCIAL_TX_ADDR="10.0.3.1:8085" \
./deployment/run_buyer_server.sh single
```

### Stop

```bash
BIND_ADDR="0.0.0.0:8083" ./deployment/run_buyer_server.sh stop
```

### Key env vars

| Variable | Default | Description |
|----------|---------|-------------|
| `BUYER_PORTS` | `8083 8084 8086 8087` | Ports to bind in local mode |
| `CUSTOMER_DB_ADDRS` | 5 nodes on `127.0.0.1:50051-50055` | Comma-separated customer DB gRPC addresses |
| `PRODUCT_DB_ADDR` | `127.0.0.1:50052` | Initial product DB leader hint |
| `PRODUCT_DB_PEERS` | 5 nodes on `127.0.0.1:50052-50056` | All product DB nodes (for leader discovery) |
| `FINANCIAL_TX_ADDR` | `127.0.0.1:8085` | Financial transactions service address |

---

## run_seller_server.sh

Same structure as `run_buyer_server.sh` — no `FINANCIAL_TX_ADDR`.

### Local (4 replicas on one machine)

```bash
./deployment/run_seller_server.sh local
```

Default ports: 8082, 8088, 8089, 8090.

### Single replica (multi-machine)

```bash
BIND_ADDR="0.0.0.0:8082" \
CUSTOMER_DB_ADDRS="10.0.1.1:50051,10.0.1.2:50051,10.0.1.3:50051,10.0.1.4:50051,10.0.1.5:50051" \
PRODUCT_DB_PEERS="127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056" \
./deployment/run_seller_server.sh single
```

### Stop

```bash
BIND_ADDR="0.0.0.0:8082" ./deployment/run_seller_server.sh stop
```

### Key env vars

| Variable | Default | Description |
|----------|---------|-------------|
| `SELLER_PORTS` | `8082 8088 8089 8090` | Ports to bind in local mode |
| `CUSTOMER_DB_ADDRS` | 5 nodes on `127.0.0.1:50051-50055` | Comma-separated customer DB gRPC addresses |
| `PRODUCT_DB_ADDR` | `127.0.0.1:50052` | Initial product DB leader hint |
| `PRODUCT_DB_PEERS` | 5 nodes on `127.0.0.1:50052-50056` | All product DB nodes (for leader discovery) |

---

## Full local example

```bash
# Terminal 1 — customer DB
./deployment/run_customer_db.sh local

# Terminal 2 — product DB (wait ~3 s for Raft leader election)
./deployment/run_product_db.sh local

# Terminal 3 — application servers
./deployment/run_buyer_server.sh local
./deployment/run_seller_server.sh local
```

Then set client env vars as printed by each script and run `buyer_client` / `seller_client`.
