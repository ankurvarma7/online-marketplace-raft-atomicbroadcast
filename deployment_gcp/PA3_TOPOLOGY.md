# PA3 GCP topology (Option C — quota-friendly)

This deployment uses **5 VMs** (PA3 allows multiple processes per VM on different ports; minimum 4 VMs is satisfied). Fewer external IPs avoids typical **`IN_USE_ADDRESSES`** regional limits (e.g. 8 in `us-central1`).

| VM name | Role | Processes / ports |
|---------|------|-------------------|
| `customer-stack` | Python atomic-broadcast `customer_db` (**you** install/run) | Default gRPC `50051`–`50055` on one internal IP; UDP `50000`–`50004`; MySQL (Docker) `3306` — match [`deploy_to_vms.sh`](../deploy_to_vms.sh) `CUSTOMER_DB_ADDRS` or export it before deploy |
| `product-stack` | Five Rust Raft `product_db` replicas | gRPC `50052`–`50056`; `RAFT_PEERS` uses `127.0.0.1` between peers |
| `seller-frontend` | Four `seller_server` replicas | `8082`, `8088`, `8089`, `8090` |
| `buyer-frontend` | One `financial_transactions` + four `buyer_server` | SOAP `8085`; buyers `8083`, `8084`, `8086`, `8087` |
| `client-runner` | Evaluator / load clients (optional) | SSH and run `evaluator` or clients from this host |

**Option D (not used here):** one VM per replica (12+ instances) — higher IP quota required.

**Internal addressing:** Frontends use `CUSTOMER_DB_ADDRS` = five ports on **`customer-stack`**’s internal IP (default `50051`–`50055`) and `PRODUCT_DB_PEERS` = **`product-stack` internal IP** with ports `50052`–`50056`. `PRODUCT_DB_ADDR` should track the Raft leader (defaults to `product-stack:50052`).

**Clients / evaluator:** `SELLER_SERVER_ADDRS` / `BUYER_SERVER_ADDRS` list four URLs; they differ by port on two external IPs (`seller-frontend`, `buyer-frontend`).
