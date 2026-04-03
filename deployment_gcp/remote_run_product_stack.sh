#!/usr/bin/env bash
# Run on product-stack after ~/product_db (or product_db.new) and ~/pa3_deploy.env are present.
# Starts 5 product_db processes on ports 50052–50056 with Raft peers on 127.0.0.1.

set -euo pipefail

BIN="${HOME}/product_db"
if [[ -f "${BIN}.new" ]]; then
  mv -f "${BIN}.new" "$BIN"
fi
chmod +x "$BIN"
mkdir -p "${HOME}/logs" "${HOME}/data/1" "${HOME}/data/2" "${HOME}/data/3" "${HOME}/data/4" "${HOME}/data/5"

pkill -f "${HOME}/product_db" 2>/dev/null || true
sleep 1

RAFT_PEERS="${RAFT_PEERS:-1=http://127.0.0.1:50052,2=http://127.0.0.1:50053,3=http://127.0.0.1:50054,4=http://127.0.0.1:50055,5=http://127.0.0.1:50056}"
export RAFT_PEERS

for nid in 1 2 3 4 5; do
  port=$((50051 + nid))
  NODE_ID=$nid BIND_ADDR="0.0.0.0:${port}" DATA_DIR="${HOME}/data/${nid}" \
    nohup "$BIN" </dev/null >"${HOME}/logs/product_db_${nid}.log" 2>&1 &
done

echo "product_db_started"
