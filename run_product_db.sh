#!/usr/bin/env bash
# Starts a 5-node product_db Raft cluster.
#
# Node IDs : 1 – 5
# gRPC ports: 50052 – 50056  (node N listens on 50051 + N)
# Data dirs : logs/product_db_data/{node_id}
#
# Usage:
#   ./run_product_db.sh          # build (if needed) and start all 5 nodes
#   ./run_product_db.sh stop     # gracefully stop all 5 nodes

set -euo pipefail

BINARY_DIR="${BINARY_DIR:-./target/release}"
LOG_DIR="logs/product_db"
PID_DIR="logs/marketplace_pids"
DATA_BASE_DIR="logs/product_db_data"

RAFT_PEERS="1=http://127.0.0.1:50052,2=http://127.0.0.1:50053,3=http://127.0.0.1:50054,4=http://127.0.0.1:50055,5=http://127.0.0.1:50056"
RAFT_INIT_DELAY_SECS="${RAFT_INIT_DELAY_SECS:-2}"

stop_servers() {
    echo "Stopping all product_db nodes..."
    if [ -d "$PID_DIR" ]; then
        for pid_file in "$PID_DIR"/product_db_*.pid; do
            [ -f "$pid_file" ] || continue
            pid=$(cat "$pid_file")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" && echo "  Stopped PID $pid ($(basename "$pid_file" .pid))"
            fi
            rm -f "$pid_file"
        done
    fi
    echo "Done."
}

if [ "${1:-}" = "stop" ]; then
    stop_servers
    exit 0
fi

echo "Building product_db release binary..."
cargo build --release -p product_db

mkdir -p "$LOG_DIR" "$PID_DIR"

for node_id in 1 2 3 4 5; do
    port=$((50051 + node_id))
    data_dir="${DATA_BASE_DIR}/${node_id}"
    log_file="${LOG_DIR}/node_${node_id}.log"
    pid_file="${PID_DIR}/product_db_${node_id}.pid"

    mkdir -p "$data_dir"

    echo "Starting product_db node ${node_id} on port ${port}..."
    NODE_ID="$node_id" \
    BIND_ADDR="127.0.0.1:${port}" \
    DATA_DIR="$data_dir" \
    RAFT_PEERS="$RAFT_PEERS" \
    RAFT_INIT_DELAY_SECS="$RAFT_INIT_DELAY_SECS" \
        "${BINARY_DIR}/product_db" > "$log_file" 2>&1 &
    echo $! > "$pid_file"
    echo "  PID $! -> $log_file"
done

echo ""
echo "All product_db nodes started."
echo ""
echo "  Node 1: http://127.0.0.1:50052  (logs: ${LOG_DIR}/node_1.log)"
echo "  Node 2: http://127.0.0.1:50053  (logs: ${LOG_DIR}/node_2.log)"
echo "  Node 3: http://127.0.0.1:50054  (logs: ${LOG_DIR}/node_3.log)"
echo "  Node 4: http://127.0.0.1:50055  (logs: ${LOG_DIR}/node_4.log)"
echo "  Node 5: http://127.0.0.1:50056  (logs: ${LOG_DIR}/node_5.log)"
echo ""
echo "Set this env var for buyer_server / seller_server:"
echo "  export PRODUCT_DB_ADDR=http://127.0.0.1:50052"
echo ""
echo "To stop all nodes: ./run_product_db.sh stop"
