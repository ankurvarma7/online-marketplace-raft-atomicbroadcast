# Running the Distributed Evaluator — Step-by-Step

This guide walks through starting every service layer from scratch and then running `evaluator_distributed`.

---

## Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust / cargo | 1.75+ | `rustup update stable` |
| Python | 3.9+ | for customer_db |
| Python venv | — | `python_customer_db` dependencies |
| MySQL | 8.0+ | 5 separate instances for customer_db (local) |
| Docker (optional) | — | easiest way to spin up 5 MySQL instances |

Install Python dependencies (once):

```bash
python3 -m venv newEnv
source newEnv/bin/activate
pip install -r requirements.txt
```

---

## Port reference (local single-machine layout)

To avoid conflicts between customer_db and product_db on the same machine, use
the port assignments below. Multi-machine deployments have no such conflict and
can use each service's defaults.

| Service | Protocol | Ports |
|---------|----------|-------|
| customer_db gRPC | TCP | 50061 – 50065 (nodes 0–4) |
| customer_db UDP | UDP | 50000 – 50004 (nodes 0–4) |
| product_db gRPC (Raft) | TCP | 50052 – 50056 (nodes 1–5) |
| buyer_server | TCP | 8083, 8084, 8086, 8087 |
| seller_server | TCP | 8082, 8088, 8089, 8090 |
| financial_transactions | TCP | 8085 |

---

## Step 1 — Start MySQL instances for customer_db

Each customer_db node needs its own MySQL instance. Run 5 Docker containers, one per node:

```bash
for i in 0 1 2 3 4; do
  docker run -d \
    --name customer_db_mysql_${i} \
    -e MYSQL_ROOT_PASSWORD=my-secret-pw \
    -e MYSQL_DATABASE=customer_db \
    -p $((8000 + i)):3306 \
    mysql:8.0
done
```

Wait ~15 seconds for MySQL to initialise, then apply the schema to each instance:

```bash
for i in 0 1 2 3 4; do
  mysql -h 127.0.0.1 -P $((8000 + i)) -u root -pmy-secret-pw \
    < DB/customer/scripts/create_table.sql
done
```

---

## Step 2 — Start customer_db (5 nodes)

Open a dedicated terminal. From the repo root:

```bash
source newEnv/bin/activate

GRPC_BASE_PORT=50061 \
./deployment/run_customer_db.sh local
```

This starts nodes 0–4 with:
- gRPC on `0.0.0.0:50061` – `0.0.0.0:50065`
- UDP on `0.0.0.0:50000` – `0.0.0.0:50004`
- MySQL on `127.0.0.1:8000` – `127.0.0.1:8004`

Check a node is up:

```bash
tail -f logs/customer_db/node_0.log
# Should print: [gRPC] Node 0 listening on 0.0.0.0:50061
```

---

## Step 3 — Start product_db (5-node Raft cluster)

Open a new terminal. From the repo root:

```bash
./deployment/run_product_db.sh local
```

This builds the binary (first run only) and starts nodes 1–5 on ports 50052–50056.

Wait ~3 seconds for Raft leader election, then verify:

```bash
tail -f logs/product_db/node_1.log
# Should print: product_db node_id=1 listening on 127.0.0.1:50052
```

---

## Step 4 — Start buyer_server replicas (4 replicas)

Open a new terminal. From the repo root:

```bash
CUSTOMER_DB_ADDRS="127.0.0.1:50061,127.0.0.1:50062,127.0.0.1:50063,127.0.0.1:50064,127.0.0.1:50065" \
PRODUCT_DB_PEERS="127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056" \
PRODUCT_DB_ADDR="127.0.0.1:50052" \
FINANCIAL_TX_ADDR="127.0.0.1:8085" \
./deployment/run_buyer_server.sh local
```

Replicas start on ports 8083, 8084, 8086, 8087.

---

## Step 5 — Start seller_server replicas (4 replicas)

Open a new terminal. From the repo root:

```bash
CUSTOMER_DB_ADDRS="127.0.0.1:50061,127.0.0.1:50062,127.0.0.1:50063,127.0.0.1:50064,127.0.0.1:50065" \
PRODUCT_DB_PEERS="127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056" \
PRODUCT_DB_ADDR="127.0.0.1:50052" \
./deployment/run_seller_server.sh local
```

Replicas start on ports 8082, 8088, 8089, 8090.

---

## Step 6 — Run the distributed evaluator

```bash
SELLER_SERVER_ADDRS="http://127.0.0.1:8082,http://127.0.0.1:8088,http://127.0.0.1:8089,http://127.0.0.1:8090" \
BUYER_SERVER_ADDRS="http://127.0.0.1:8083,http://127.0.0.1:8084,http://127.0.0.1:8086,http://127.0.0.1:8087" \
cargo run --release -p evaluator_distributed
```

### Optional flags

```bash
# Fewer runs / ops for a quick smoke test
cargo run --release -p evaluator_distributed -- --runs 3 --ops 200
```

### What the evaluator prints

1. **Cross-replica consistency test** — writes an item on each seller replica, then reads it from every buyer replica. Each pair prints `[PASS]` or `[FAIL]` with latency.
2. **Scenario 1 (1×1)** — baseline single client-pair throughput across replicas.
3. **Scenario 2 (10×10)** — moderate concurrency, 10 pairs round-robined across replicas.
4. **Scenario 3 (100×100)** — 100 pairs, 25 per replica (with 4 replicas).
5. **Results summary** — average throughput (ops/sec), average response time, and failure rate per scenario.

---

## Stopping everything

```bash
# Stop buyer_server replicas
for port in 8083 8084 8086 8087; do
  BIND_ADDR="0.0.0.0:${port}" ./deployment/run_buyer_server.sh stop
done

# Stop seller_server replicas
for port in 8082 8088 8089 8090; do
  BIND_ADDR="0.0.0.0:${port}" ./deployment/run_seller_server.sh stop
done

# Stop product_db nodes
for node in 1 2 3 4 5; do
  NODE_ID=${node} ./deployment/run_product_db.sh stop
done

# Stop customer_db nodes
for node in 0 1 2 3 4; do
  NODE_ID=${node} ./deployment/run_customer_db.sh stop
done

# Stop MySQL containers
for i in 0 1 2 3 4; do
  docker stop customer_db_mysql_${i} && docker rm customer_db_mysql_${i}
done
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| `[FAIL]` in cross-replica test | product_db Raft not yet converged | Wait 5 s and retry |
| `seller login failed` in setup | customer_db node unreachable | Check `logs/customer_db/node_N.log` |
| High failure rate in scenario 3 | `DELIVER_TIMEOUT` too short under load | Set `DELIVER_TIMEOUT=60` |
| `All product_db nodes are unreachable` | product_db not started or wrong ports | Check `logs/product_db/node_1.log` |
| MySQL connection refused | Container not ready yet | Wait 15 s after `docker run` |
| Port already in use | Previous run not fully stopped | Run the stop commands above |
