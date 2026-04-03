#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# run_buyer_server.sh — Launch / stop buyer_server replicas
#
# MODES
#   local   (default) – build (if needed) and start REPLICA_COUNT replicas on
#                       this machine, one per port in BUYER_PORTS
#   single  – start exactly one replica at BIND_ADDR on this machine
#   stop    – stop the replica whose port matches the one in BIND_ADDR
#
# USAGE
#   ./deployment/run_buyer_server.sh [local|single|stop]
#
# LOCAL EXAMPLE (4 replicas on one machine — default)
#   ./deployment/run_buyer_server.sh local
#
# MULTI-MACHINE EXAMPLE
#   On each machine, set the addresses for the backing services, then:
#
#   BIND_ADDR="0.0.0.0:8083" \
#   CUSTOMER_DB_ADDRS="10.0.1.1:50051,10.0.1.2:50051,10.0.1.3:50051,10.0.1.4:50051,10.0.1.5:50051" \
#   PRODUCT_DB_PEERS="http://10.0.2.1:50052,http://10.0.2.2:50052,http://10.0.2.3:50052,http://10.0.2.4:50052,http://10.0.2.5:50052" \
#   FINANCIAL_TX_ADDR="10.0.3.1:8085" \
#   ./deployment/run_buyer_server.sh single
#
#   To stop that replica:
#   BIND_ADDR="0.0.0.0:8083" ./deployment/run_buyer_server.sh stop
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# ── Replica ports (local mode) ────────────────────────────────────────────────
# Space-separated list of ports to bind in local mode.
BUYER_PORTS="${BUYER_PORTS:-8083 8084 8086 8087}"

# ── Backing service addresses ─────────────────────────────────────────────────
CUSTOMER_DB_ADDRS="${CUSTOMER_DB_ADDRS:-127.0.0.1:50051,127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055}"
# Initial product_db leader hint (any node is fine — the server will discover the real leader).
PRODUCT_DB_ADDR="${PRODUCT_DB_ADDR:-127.0.0.1:50052}"
PRODUCT_DB_PEERS="${PRODUCT_DB_PEERS:-127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056}"
FINANCIAL_TX_ADDR="${FINANCIAL_TX_ADDR:-127.0.0.1:8085}"

# ── Paths ─────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BINARY_DIR="${BINARY_DIR:-${REPO_ROOT}/target/release}"
LOG_DIR="${LOG_DIR:-${REPO_ROOT}/logs/buyer_server}"
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

# Extract just the port number from an addr like "0.0.0.0:8083" or "8083".
_port_from_addr() {
    echo "${1##*:}"
}

stop_replica() {
    local bind_addr="$1"
    local port
    port=$(_port_from_addr "$bind_addr")
    local pid_file="${PID_DIR}/buyer_server_${port}.pid"
    if [ ! -f "$pid_file" ]; then
        echo "No PID file found for buyer_server port ${port} (${pid_file}). Already stopped?"
        return
    fi
    local pid
    pid=$(cat "$pid_file")
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" && echo "Stopped buyer_server port ${port} (PID $pid)"
    else
        echo "buyer_server port ${port} (PID $pid) is not running."
    fi
    rm -f "$pid_file"
}

start_replica() {
    local bind_addr="$1"
    local port
    port=$(_port_from_addr "$bind_addr")
    local log_file="${LOG_DIR}/buyer_server_${port}.log"
    local pid_file="${PID_DIR}/buyer_server_${port}.pid"

    echo "Starting buyer_server on ${bind_addr}..."
    echo "  CustomerDB : ${CUSTOMER_DB_ADDRS}"
    echo "  ProductDB  : ${PRODUCT_DB_PEERS}"
    echo "  FinancialTx: ${FINANCIAL_TX_ADDR}"
    echo "  Log        : ${log_file}"

    BUYER_SERVER_BIND_ADDR="$bind_addr" \
    CUSTOMER_DB_ADDRS="$CUSTOMER_DB_ADDRS" \
    PRODUCT_DB_ADDR="$PRODUCT_DB_ADDR" \
    PRODUCT_DB_PEERS="$PRODUCT_DB_PEERS" \
    FINANCIAL_TX_ADDR="$FINANCIAL_TX_ADDR" \
        "${BINARY_DIR}/buyer_server" > "$log_file" 2>&1 &

    echo $! > "$pid_file"
    echo "  PID $!"
    _start_log_watchdog "$log_file" "$pid_file"
}

# ── Mode dispatch ─────────────────────────────────────────────────────────────

MODE="${1:-local}"

case "$MODE" in

  stop)
    BIND_ADDR="${BIND_ADDR:?BIND_ADDR must be set (e.g. 0.0.0.0:8083) to identify which replica to stop}"
    stop_replica "$BIND_ADDR"
    exit 0
    ;;

  single)
    BIND_ADDR="${BIND_ADDR:?BIND_ADDR must be set in single mode (e.g. 0.0.0.0:8083)}"

    mkdir -p "$LOG_DIR" "$PID_DIR"
    start_replica "$BIND_ADDR"

    echo ""
    echo "buyer_server started on ${BIND_ADDR}."
    echo "To stop: BIND_ADDR=${BIND_ADDR} $0 stop"
    ;;

  local)
    echo "Building buyer_server release binary..."
    cargo build --release -p buyer_server --manifest-path "${REPO_ROOT}/Cargo.toml"

    mkdir -p "$LOG_DIR" "$PID_DIR"

    for port in $BUYER_PORTS; do
        start_replica "0.0.0.0:${port}"
        echo ""
    done

    echo "All buyer_server replicas started."
    echo ""
    addrs=""
    for port in $BUYER_PORTS; do
        addrs="${addrs:+$addrs,}http://127.0.0.1:${port}"
    done
    echo "Set this env var for buyer_client:"
    echo "  export BUYER_SERVER_ADDRS=${addrs}"
    echo ""
    echo "To stop a replica: BIND_ADDR=0.0.0.0:<port> $0 stop"
    ;;

  *)
    echo "Usage: $0 [local|single|stop]"
    exit 1
    ;;
esac
