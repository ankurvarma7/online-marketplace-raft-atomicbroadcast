# GCP deployment (PA3)

Documents **PA3 Option C** (5 VMs): stacked replicas on fewer hosts to stay under typical regional **external IP** quotas; Rust `product_db`, `seller_server`, `buyer_server`, and `financial_transactions` via repo-root scripts. **Python `customer_db` on `customer-stack` is not deployed by this repo** — install and run it yourself (see [`install_customer_node.sh`](install_customer_node.sh) if helpful). See [`PA3_TOPOLOGY.md`](PA3_TOPOLOGY.md) for the port matrix.

| Goal | Where to look |
|------|----------------|
| Local dev (`run_*.sh`, MySQL Docker on laptop) | [`deployment/`](../deployment/) |
| Create VMs, deploy, optional systemd | [`create_vms.sh`](../create_vms.sh), [`deploy_to_vms.sh`](../deploy_to_vms.sh), [`setup_services.sh`](../setup_services.sh) |
| Remote runner scripts (scp’d by deploy) | [`remote_run_product_stack.sh`](remote_run_product_stack.sh), [`remote_run_seller.sh`](remote_run_seller.sh), [`remote_run_buyer.sh`](remote_run_buyer.sh) |
| PA3 ports / topology | [`PA3_TOPOLOGY.md`](PA3_TOPOLOGY.md) |
| Python customer (manual on `customer-stack`) | [`install_customer_node.sh`](install_customer_node.sh) |
| Evaluator env exports | [`export_evaluator_env.sh`](export_evaluator_env.sh) |
| Metrics and failure tests | [`pa3_evaluation.md`](pa3_evaluation.md) |

## Topology summary

- **`customer-stack`:** You run Python atomic broadcast + Docker MySQL; before [`deploy_to_vms.sh`](../deploy_to_vms.sh), optionally `export CUSTOMER_DB_ADDRS='...'` so seller/buyer match your gRPC ports (defaults: five ports `50051`–`50055` on `customer-stack`’s internal IP).
- **`product-stack`:** five `product_db` processes (`50052`–`50056`, Raft peers on localhost).
- **`seller-frontend`:** four `seller_server` processes on ports `8082`, `8088`, `8089`, `8090`.
- **`buyer-frontend`:** `financial_transactions` on `8085` and four `buyer_server` processes on `8083`, `8084`, `8086`, `8087`.
- **`client-runner`:** optional VM for running evaluator/clients in the cloud.

## Quick start (from repo root)

Default zone is **`us-east5-a`** (set `ZONE` consistently for create/deploy/evaluator). If you hit **`ZONE_RESOURCE_POOL_EXHAUSTED`**, try another zone or `MACHINE_TYPE=e2-small`. Deploy uses **scp + remote runner scripts**; if SSH fails, try `GCP_IAP_SSH=1 ./deploy_to_vms.sh`. See [`how_to_use.md`](how_to_use.md).

```bash
./create_vms.sh
./deploy.sh --linux   # optional full workspace build; deploy_to_vms builds needed crates too
# Optional: export CUSTOMER_DB_ADDRS if your Python ports differ from defaults
./deploy_to_vms.sh
# optional:
./setup_services.sh
```

Details: [`how_to_use.md`](how_to_use.md).

## Evaluator

```bash
eval "$(./deployment_gcp/export_evaluator_env.sh)"
cargo run --release -p evaluator_distributed -- --runs 10 --ops 1000
```

Uses **four distinct URLs** per role (different ports on `seller-frontend` and `buyer-frontend`).
