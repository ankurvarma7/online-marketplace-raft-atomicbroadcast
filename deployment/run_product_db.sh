#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run_product_db.sh — Launch / stop product_db Raft nodes
#
# MODES
#   local   (default) – build (if needed) and start all NUM_NODES on this machine
#   single  – start exactly one node (identified by NODE_ID) on this machine
#   stop    – stop the local node identified by NODE_ID (reads PID from PID_DIR)
#
# USAGE
#   ./deployment/run_product_db.sh [local|single|stop]
#
# All settings below can be overridden by environment variables.
#
# LOCAL EXAMPLE (5 nodes, all on one machine — default)
#   ./deployment/run_product_db.sh local
#
# MULTI-MACHINE EXAMPLE
#   On each machine, set NODE_ID and RAFT_PEERS to the real IPs, then:
#
#   NODE_ID=1 \
#   RAFT_PEERS="1=http://10.0.0.1:50052,2=http://10.0.0.2:50052,3=http://10.0.0.3:50052,4=http://10.0.0.4:50052,5=http://10.0.0.5:50052" \
#   BIND_ADDR="0.0.0.0:50052" \
#   DATA_DIR="/var/lib/product_db/node_1" \
#   ./deployment/run_product_db.sh single
#
#   To stop that node later:
#   NODE_ID=1 ./deployment/run_product_db.sh stop
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# ── Cluster topology ──────────────────────────────────────────────────────────
# Node IDs are 1-indexed. Each node's default gRPC port = GRPC_BASE_PORT + NODE_ID.
NUM_NODES="${NUM_NODES:-5}"
GRPC_BASE_PORT="${GRPC_BASE_PORT:-50051}"   # node N binds on BASE + N  → 50052…50056

# RAFT_PEERS: comma-separated  <node_id>=http://<ip>:<port>
# Defaults to a cluster of NUM_NODES nodes all on localhost.
_default_peers=""
for i in $(seq 1 "$NUM_NODES"); do
    _default_peers="${_default_peers:+$_default_peers,}${i}=http://0.0.0.0:$((GRPC_BASE_PORT + i))"
done
RAFT_PEERS="${RAFT_PEERS:-$_default_peers}"

# Seconds to wait before attempting Raft cluster initialisation.
RAFT_INIT_DELAY_SECS="${RAFT_INIT_DELAY_SECS:-2}"

# ── Paths ─────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BINARY_DIR="${BINARY_DIR:-${REPO_ROOT}/target/release}"
DATA_BASE_DIR="${DATA_BASE_DIR:-${REPO_ROOT}/logs/product_db_data}"
LOG_DIR="${LOG_DIR:-${REPO_ROOT}/logs/product_db}"
PID_DIR="${PID_DIR:-${REPO_ROOT}/logs/marketplace_pids}"

# ── Log rotation ──────────────────────────────────────────────────────────────
LOG_MAX_BYTES="${LOG_MAX_BYTES:-52428800}"   # 50 MB
LOG_TAIL_LINES="${LOG_TAIL_LINES:-1000}"
LOG_CHECK_INTERVAL="${LOG_CHECK_INTERVAL:-30}"

_start_log_watchdog() {
    local log_file="$1"
    local pid_file="$2"
    (
        while [ -f "$pid_file" ]; do
            sleep "$LOG_CHECK_INTERVAL"
            if [ -f "$log_file" ]; then
                size=$(wc -c < "$log_file" 2>/dev/null || echo 0)
                if [ "$size" -gt "$LOG_MAX_BYTES" ]; then
                    tail -n "$LOG_TAIL_LINES" "$log_file" > "${log_file}.tmp" \
                        && mv "${log_file}.tmp" "$log_file"
                fi
            fi
        done
    ) &
}

# ── Helpers ───────────────────────────────────────────────────────────────────

stop_node() {
    local node_id="$1"
    local pid_file="${PID_DIR}/product_db_${node_id}.pid"
    if [ ! -f "$pid_file" ]; then
        echo "No PID file found for node ${node_id} (${pid_file}). Already stopped?"
        return
    fi
    local pid
    pid=$(cat "$pid_file")
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" && echo "Stopped node ${node_id} (PID $pid)"
    else
        echo "Node ${node_id} (PID $pid) is not running."
    fi
    rm -f "$pid_file"
}

start_node() {
    local node_id="$1"
    local bind_addr="$2"
    local data_dir="$3"

    local log_file="${LOG_DIR}/node_${node_id}.log"
    local pid_file="${PID_DIR}/product_db_${node_id}.pid"

    mkdir -p "$data_dir"

    echo "Starting product_db node ${node_id}..."
    echo "  Bind  : ${bind_addr}"
    echo "  Data  : ${data_dir}"
    echo "  Log   : ${log_file}"

    NODE_ID="$node_id" \
    BIND_ADDR="$bind_addr" \
    DATA_DIR="$data_dir" \
    RAFT_PEERS="$RAFT_PEERS" \
    RAFT_INIT_DELAY_SECS="$RAFT_INIT_DELAY_SECS" \
        "${BINARY_DIR}/product_db" > "$log_file" 2>&1 &

    echo $! > "$pid_file"
    echo "  PID $!"
    _start_log_watchdog "$log_file" "$pid_file"
}

# ── Mode dispatch ─────────────────────────────────────────────────────────────

MODE="${1:-local}"

case "$MODE" in

  stop)
    NODE_ID="${NODE_ID:?NODE_ID must be set to identify which node to stop}"
    stop_node "$NODE_ID"
    exit 0
    ;;

  single)
    NODE_ID="${NODE_ID:?NODE_ID must be set in single mode}"
    BIND_ADDR="${BIND_ADDR:-0.0.0.0:$((GRPC_BASE_PORT + NODE_ID))}"
    DATA_DIR="${DATA_DIR:-${DATA_BASE_DIR}/${NODE_ID}}"

    mkdir -p "$LOG_DIR" "$PID_DIR"
    start_node "$NODE_ID" "$BIND_ADDR" "$DATA_DIR"

    echo ""
    echo "Node ${NODE_ID} started."
    echo "To stop: NODE_ID=${NODE_ID} $0 stop"
    ;;

  local)
    echo "Building product_db release binary..."
    cargo build --release -p product_db --manifest-path "${REPO_ROOT}/Cargo.toml"

    mkdir -p "$LOG_DIR" "$PID_DIR"

    for node_id in $(seq 1 "$NUM_NODES"); do
        bind_addr="0.0.0.0:$((GRPC_BASE_PORT + node_id))"
        data_dir="${DATA_BASE_DIR}/${node_id}"
        start_node "$node_id" "$bind_addr" "$data_dir"
        echo ""
    done

    echo "All ${NUM_NODES} product_db nodes started."
    echo ""
    echo "Raft peers string for buyer_server / seller_server:"
    echo "  export RAFT_PEERS=${RAFT_PEERS}"
    echo ""
    echo "Connect clients to any node (they will redirect to the leader):"
    for node_id in $(seq 1 "$NUM_NODES"); do
        echo "  Node ${node_id}: http://0.0.0.0:$((GRPC_BASE_PORT + node_id))  (logs: ${LOG_DIR}/node_${node_id}.log)"
    done
    echo ""
    echo "To stop a node: NODE_ID=<n> $0 stop"
    ;;

  *)
    echo "Usage: $0 [local|single|stop]"
    exit 1
    ;;
esac
