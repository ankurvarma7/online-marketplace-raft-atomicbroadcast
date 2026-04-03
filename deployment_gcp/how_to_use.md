# GCP deployment walkthrough (PA3 Option C)

Run commands from the **repository root** unless noted.

## Prerequisites

- `gcloud` authenticated; quota for **5×** `e2-micro` (or larger) in one zone (typical **in-use external IP** limits: request a quota increase if you need more VMs).
- Rust cross-compile: `rustup target add x86_64-unknown-linux-gnu`; macOS: `brew install zig && cargo install cargo-zigbuild`.
- `deploy_to_vms.sh` builds `product_db`, `seller_server`, `buyer_server`, `financial_transactions` for `x86_64-unknown-linux-gnu`.

## 1. Create VMs

[`create_vms.sh`](../create_vms.sh) creates `customer-stack`, `product-stack`, `seller-frontend`, `buyer-frontend`, `client-runner` and firewall rule `marketplace-pa3-internal` (TCP `50051`–`50056` for stacked gRPC, plus REST/SOAP/UDP for customer broadcast and frontends).

**Zone / capacity:** Defaults are `ZONE=us-east5-a` and `MACHINE_TYPE=e2-micro` (override as needed). If GCP returns **`ZONE_RESOURCE_POOL_EXHAUSTED`**, try another zone or `MACHINE_TYPE=e2-small`, and keep **`ZONE` the same** for `create_vms.sh`, `deploy_to_vms.sh`, and `export_evaluator_env.sh`.

**Deploy:** `deploy_to_vms.sh` **scp**s binaries plus `deployment_gcp/remote_run_*.sh` runner scripts and a generated `~/pa3_deploy.env` (multi-port processes on each VM). If **`gcloud compute ssh` exits 255** after a successful scp, try **IAP tunneling**: `GCP_IAP_SSH=1 ./deploy_to_vms.sh`.

## 2. Customer DB (your step)

On **`customer-stack`**, install and run your Python `customer_db` (e.g. copy [`python_customer_db`](../python_customer_db) and use [`install_customer_node.sh`](install_customer_node.sh), or your own process). Ensure gRPC listen ports match what seller/buyer will use.

If your ports differ from the default five addresses on `customer-stack` (`50051`–`50055`), **before** step 3 run:

```bash
export CUSTOMER_DB_ADDRS='internal_ip:port,...'   # five comma-separated replicas
```

## 3. Deploy Rust stack

```bash
./deploy_to_vms.sh
```

This:

1. Deploys five `product_db` Raft processes on **`product-stack`** (`RAFT_PEERS` on `127.0.0.1`, ports `50052`–`50056`).
2. Starts four `seller_server` processes on **seller-frontend**.
3. Starts `financial_transactions` then four `buyer_server` processes on **buyer-frontend** (`FINANCIAL_TX_ADDR=127.0.0.1:8085`).

## 4. Optional systemd

[`setup_services.sh`](../setup_services.sh) installs units for `product_db_*`, sellers, `financial_transactions`, and buyers. **Do not** duplicate if processes are already running via `nohup` from step 3—stop old processes first or use systemd only on fresh VMs.

## Networking

- **Between VMs:** Internal IPs; firewall tag `marketplace` + rule `marketplace-pa3-internal`.
- **Evaluator / clients from your laptop:** Allow `tcp:8082,8083,8084,8085,8086,8087,8088,8089,8090` from your IP to `seller-frontend` and `buyer-frontend` (separate firewall rule with `--source-ranges`).

## Client environment

[`seller_client`](../seller_client/src/main.rs) / [`buyer_client`](../buyer_client/src/main.rs) use **`SELLER_SERVER_ADDRS`** and **`BUYER_SERVER_ADDRS`** (comma-separated, four URLs each). Build from external IPs + ports in [`PA3_TOPOLOGY.md`](PA3_TOPOLOGY.md) or use [`export_evaluator_env.sh`](export_evaluator_env.sh).

## Raft leader

`PRODUCT_DB_ADDR` on frontends defaults to **`product-stack:50052`**. If another node is leader, update env and restart seller/buyer processes (or rely on redirect metadata if your clients support it).
