# PA3 evaluation on GCP

Maps [`PA3.md`](../PA3.md) to [`evaluator_distributed`](../evaluator_distributed/src/main.rs) after a full [`deploy_to_vms.sh`](../deploy_to_vms.sh) (Option C: 5 VMs, four seller / four buyer URLs on two frontend instances).

## Metrics

| PA3 requirement | How to measure |
|-----------------|---------------|
| Avg response time (10 runs) | Evaluator per-scenario **Average Response Time** |
| Avg throughput (10 runs, 1000 ops/client) | `--runs 10 --ops 1000` |
| Scenarios 1–3 | Built-in evaluator scenarios |
| Failure: one seller + one buyer replica | Stop **one** seller port and **one** buyer port on the two frontend VMs (distinct processes) |
| Failure: product_db follower / leader | SSH to **`product-stack`** and stop one `product_db` process (by port or systemd unit) |

## Run evaluator

From repo root:

```bash
eval "$(./deployment_gcp/export_evaluator_env.sh)"
cargo run --release -p evaluator_distributed -- --runs 10 --ops 1000
```

[`export_evaluator_env.sh`](export_evaluator_env.sh) sets **four distinct seller URLs** and **four distinct buyer URLs** (ports per [`PA3_TOPOLOGY.md`](PA3_TOPOLOGY.md)).

## Manual failure examples

Replace `ZONE` (default in scripts is `us-central1-b`; match `create_vms.sh`).

### One seller replica + one buyer replica stopped

If you use **systemd** from [`setup_services.sh`](../setup_services.sh), stop one seller unit and one buyer unit (example ports `8088` and `8086`):

```bash
gcloud compute ssh seller-frontend --zone=ZONE --command='sudo systemctl stop seller_server_8088'
gcloud compute ssh buyer-frontend --zone=ZONE --command='sudo systemctl stop buyer_server_8086'
```

Re-run the evaluator, then `sudo systemctl start …` the same units. With **nohup-only** deploy, use `pkill` against the specific process or restart from [`deploy_to_vms.sh`](../deploy_to_vms.sh) after edits.

### product_db non-leader

Example: stop Raft node 2 (port `50053`, systemd unit `product_db_2`):

```bash
gcloud compute ssh product-stack --zone=ZONE --command='sudo systemctl stop product_db_2'
```

If you use **nohup-only** deploy (no systemd), find the PID for `NODE_ID=2` (e.g. `ps aux | grep product_db`) or stop the listener on `50053`, then restart that process after the test.

### product_db leader

Stop the leader’s `product_db` process on **`product-stack`**; after election, update `PRODUCT_DB_ADDR` on **seller-frontend** and **buyer-frontend** if frontends still point at the old leader, then restart seller/buyer processes.

## Reporting

Save evaluator stdout for each scenario and your failure runs; align the narrative with [`README.md`](../README.md) and [`deployment_gcp/README.md`](README.md).
