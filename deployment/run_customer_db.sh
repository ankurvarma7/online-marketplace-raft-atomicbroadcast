#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run_customer_db.sh — Launch / stop customer_db replicas
#
# MODES
#   local   (default) – start all NUM_NODES replicas on this machine
#   single  – start exactly one replica (identified by NODE_ID) on this machine
#   stop    – stop the local node identified by NODE_ID (reads PID from PID_DIR)
#
# USAGE
#   ./deployment/run_customer_db.sh [local|single|stop]
#
# All settings below can be overridden by environment variables before calling
# this script:
#
#   export NUM_NODES=5
#   export PEERS="0=192.168.1.10:50000,1=192.168.1.11:50000,..."
#   ./deployment/run_customer_db.sh local
#
# MULTI-MACHINE EXAMPLE
#   On machine A (node 0):
#     NODE_ID=0 \
#     PEERS="0=192.168.1.10:50000,1=192.168.1.11:50000,2=192.168.1.12:50000,3=192.168.1.13:50000,4=192.168.1.14:50000" \
#     GRPC_BIND_ADDR="0.0.0.0:50051" \
#     UDP_BIND_ADDR="0.0.0.0:50000" \
#     CUSTOMER_DB_HOST="127.0.0.1" \
#     CUSTOMER_DB_PORT="8000" \
#     ./deployment/run_customer_db.sh single
#
#   On machine B (node 1):
#     NODE_ID=1 \
#     PEERS="0=192.168.1.10:50000,1=192.168.1.11:50000,..." \
#     GRPC_BIND_ADDR="0.0.0.0:50051" \
#     UDP_BIND_ADDR="0.0.0.0:50000" \
#     CUSTOMER_DB_HOST="127.0.0.1" \
#     CUSTOMER_DB_PORT="8000" \
#     ./deployment/run_customer_db.sh single
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# ── Cluster topology ──────────────────────────────────────────────────────────
NUM_NODES="${NUM_NODES:-5}"

# PEERS: comma-separated  <node_id>=<ip>:<udp_port>
# Defaults to a 5-node cluster all on localhost, UDP ports 50000-50004.
_default_peers=""
for i in $(seq 0 $((NUM_NODES - 1))); do
    _default_peers="${_default_peers:+$_default_peers,}${i}=127.0.0.1:$((50000 + i))"
done
PEERS="${PEERS:-$_default_peers}"

# gRPC base port (node N binds on GRPC_BASE_PORT + N) — used in local mode only.
# In single mode, set GRPC_BIND_ADDR directly instead.
GRPC_BASE_PORT="${GRPC_BASE_PORT:-50051}"

# ── Database ──────────────────────────────────────────────────────────────────
# CUSTOMER_DB_HOST     — MySQL host (same for all nodes in local mode, or per-machine)
# CUSTOMER_DB_PORT     — exact MySQL port for *this* node's DB instance
# CUSTOMER_DB_BASE_PORT — used in local mode: node N connects to BASE_PORT + N
# CUSTOMER_DB_USER     — MySQL user
# CUSTOMER_DB_PASSWORD — MySQL password
# CUSTOMER_DB_NAME     — MySQL database name
CUSTOMER_DB_HOST="${CUSTOMER_DB_HOST:-127.0.0.1}"
CUSTOMER_DB_BASE_PORT="${CUSTOMER_DB_BASE_PORT:-8000}"   # local mode only
CUSTOMER_DB_USER="${CUSTOMER_DB_USER:-root}"
CUSTOMER_DB_PASSWORD="${CUSTOMER_DB_PASSWORD:-my-secret-pw}"
CUSTOMER_DB_NAME="${CUSTOMER_DB_NAME:-customer_db}"

# ── Protocol tuning ───────────────────────────────────────────────────────────
DELIVER_TIMEOUT="${DELIVER_TIMEOUT:-30.0}"
SESSION_TTL="${SESSION_TTL:-300}"

# ── Paths ─────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SERVICE_SCRIPT="${SERVICE_SCRIPT:-${REPO_ROOT}/python_customer_db/customer_db_service.py}"
PYTHON="${PYTHON:-python3}"
LOG_DIR="${LOG_DIR:-${REPO_ROOT}/logs/customer_db}"
PID_DIR="${PID_DIR:-${REPO_ROOT}/logs/marketplace_pids}"

# ── Helpers ───────────────────────────────────────────────────────────────────

stop_node() {
    # Stop a single node by ID.
    local node_id="$1"
    local pid_file="${PID_DIR}/customer_db_${node_id}.pid"
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
    local grpc_bind="$2"
    local udp_bind="$3"
    local db_port="$4"

    local log_file="${LOG_DIR}/node_${node_id}.log"
    local pid_file="${PID_DIR}/customer_db_${node_id}.pid"

    echo "Starting customer_db node ${node_id}..."
    echo "  gRPC  : ${grpc_bind}"
    echo "  UDP   : ${udp_bind}"
    echo "  DB    : ${CUSTOMER_DB_HOST}:${db_port}/${CUSTOMER_DB_NAME}"
    echo "  Log   : ${log_file}"

    NODE_ID="$node_id" \
    NUM_NODES="$NUM_NODES" \
    PEERS="$PEERS" \
    GRPC_BIND_ADDR="$grpc_bind" \
    UDP_BIND_ADDR="$udp_bind" \
    CUSTOMER_DB_HOST="$CUSTOMER_DB_HOST" \
    CUSTOMER_DB_PORT="$db_port" \
    CUSTOMER_DB_USER="$CUSTOMER_DB_USER" \
    CUSTOMER_DB_PASSWORD="$CUSTOMER_DB_PASSWORD" \
    CUSTOMER_DB_NAME="$CUSTOMER_DB_NAME" \
    DELIVER_TIMEOUT="$DELIVER_TIMEOUT" \
    SESSION_TTL="$SESSION_TTL" \
        "$PYTHON" "$SERVICE_SCRIPT" --id "$node_id" > "$log_file" 2>&1 &

    echo $! > "$pid_file"
    echo "  PID $!"
}

# ── Mode dispatch ─────────────────────────────────────────────────────────────

MODE="${1:-local}"

case "$MODE" in

  stop)
    # Stop only the local node identified by NODE_ID.
    NODE_ID="${NODE_ID:?NODE_ID must be set to identify which node to stop}"
    stop_node "$NODE_ID"
    exit 0
    ;;

  single)
    # Single-replica mode — intended for multi-machine deployments.
    # The caller is expected to set NODE_ID, GRPC_BIND_ADDR, UDP_BIND_ADDR,
    # and CUSTOMER_DB_PORT (or rely on the defaults below).
    NODE_ID="${NODE_ID:?NODE_ID must be set in single mode}"
    GRPC_BIND="${GRPC_BIND_ADDR:-0.0.0.0:$((GRPC_BASE_PORT + NODE_ID))}"
    UDP_BIND="${UDP_BIND_ADDR:-0.0.0.0:$((50000 + NODE_ID))}"
    DB_PORT="${CUSTOMER_DB_PORT:-$((CUSTOMER_DB_BASE_PORT + NODE_ID))}"

    mkdir -p "$LOG_DIR" "$PID_DIR"
    start_node "$NODE_ID" "$GRPC_BIND" "$UDP_BIND" "$DB_PORT"

    echo ""
    echo "Node ${NODE_ID} started."
    echo "To stop: NODE_ID=${NODE_ID} $0 stop"
    ;;

  local)
    # Local mode — starts all NUM_NODES replicas on this machine.
    # Each node gets:
    #   gRPC port = GRPC_BASE_PORT + node_id   (default 50051, 50052, …)
    #   UDP  port = 50000 + node_id
    #   DB   port = CUSTOMER_DB_BASE_PORT + node_id  (default 8000, 8001, …)

    mkdir -p "$LOG_DIR" "$PID_DIR"

    for node_id in $(seq 0 $((NUM_NODES - 1))); do
        grpc_bind="0.0.0.0:$((GRPC_BASE_PORT + node_id))"
        udp_bind="0.0.0.0:$((50000 + node_id))"
        db_port="$((CUSTOMER_DB_BASE_PORT + node_id))"
        start_node "$node_id" "$grpc_bind" "$udp_bind" "$db_port"
        echo ""
    done

    echo "All ${NUM_NODES} customer_db nodes started."
    echo ""
    echo "gRPC addresses for CUSTOMER_DB_ADDRS:"
    addrs=""
    for node_id in $(seq 0 $((NUM_NODES - 1))); do
        port=$((GRPC_BASE_PORT + node_id))
        addrs="${addrs:+$addrs,}127.0.0.1:${port}"
    done
    echo "  export CUSTOMER_DB_ADDRS=${addrs}"
    echo ""
    echo "To stop a node:    NODE_ID=<n> $0 stop"
    ;;

  *)
    echo "Usage: $0 [local|single|stop]"
    exit 1
    ;;
esac
