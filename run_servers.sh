#!/usr/bin/env bash
# Starts 4 replicas each of buyer_server and seller_server.
# Backend addresses can be overridden via environment variables before sourcing this script.
#
# buyer_server replicas: ports 8083, 8084, 8086, 8087
# seller_server replicas: ports 8082, 8088, 8089, 8090
#
# Usage:
#   ./run_servers.sh          # start all 8 servers
#   ./run_servers.sh stop     # kill all 8 servers

set -euo pipefail

BINARY_DIR="${BINARY_DIR:-./target/release}"
CUSTOMER_DB_ADDRS="${CUSTOMER_DB_ADDRS:-127.0.0.1:50051,127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055}"
PRODUCT_DB_ADDR="${PRODUCT_DB_ADDR:-127.0.0.1:50052}"
FINANCIAL_TX_ADDR="${FINANCIAL_TX_ADDR:-127.0.0.1:8085}"

BUYER_PORTS=(8083 8084 8086 8087)
SELLER_PORTS=(8082 8088 8089 8090)

PID_DIR="logs/marketplace_pids"

stop_servers() {
    echo "Stopping all server replicas..."
    if [ -d "$PID_DIR" ]; then
        for pid_file in "$PID_DIR"/*.pid; do
            [ -f "$pid_file" ] || continue
            pid=$(cat "$pid_file")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" && echo "Stopped PID $pid ($(basename "$pid_file" .pid))"
            fi
            rm -f "$pid_file"
        done
        rmdir "$PID_DIR" 2>/dev/null || true
    fi
    echo "Done."
}

if [ "${1:-}" = "stop" ]; then
    stop_servers
    exit 0
fi

# Build release binaries first
echo "Building release binaries..."
cargo build --release -p buyer_server -p seller_server

mkdir -p "$PID_DIR"

# Start buyer_server replicas
for port in "${BUYER_PORTS[@]}"; do
    echo "Starting buyer_server on port $port..."
    BUYER_SERVER_BIND_ADDR="0.0.0.0:${port}" \
    CUSTOMER_DB_ADDRS="$CUSTOMER_DB_ADDRS" \
    PRODUCT_DB_ADDR="$PRODUCT_DB_ADDR" \
    FINANCIAL_TX_ADDR="$FINANCIAL_TX_ADDR" \
        "${BINARY_DIR}/buyer_server" > "/tmp/buyer_server_${port}.log" 2>&1 &
    echo $! > "${PID_DIR}/buyer_server_${port}.pid"
    echo "  PID $! -> /tmp/buyer_server_${port}.log"
done

# Start seller_server replicas
for port in "${SELLER_PORTS[@]}"; do
    echo "Starting seller_server on port $port..."
    SELLER_SERVER_BIND_ADDR="0.0.0.0:${port}" \
    CUSTOMER_DB_ADDRS="$CUSTOMER_DB_ADDRS" \
    PRODUCT_DB_ADDR="$PRODUCT_DB_ADDR" \
        "${BINARY_DIR}/seller_server" > "/tmp/seller_server_${port}.log" 2>&1 &
    echo $! > "${PID_DIR}/seller_server_${port}.pid"
    echo "  PID $! -> /tmp/seller_server_${port}.log"
done

echo ""
echo "All servers started."
echo ""
echo "buyer_server replicas:  ${BUYER_PORTS[*]/#/http://127.0.0.1:}"
echo "seller_server replicas: ${SELLER_PORTS[*]/#/http://127.0.0.1:}"
echo ""
echo "Set these env vars for clients:"
echo "  export BUYER_SERVER_ADDRS=http://127.0.0.1:8083,http://127.0.0.1:8084,http://127.0.0.1:8086,http://127.0.0.1:8087"
echo "  export SELLER_SERVER_ADDRS=http://127.0.0.1:8082,http://127.0.0.1:8088,http://127.0.0.1:8089,http://127.0.0.1:8090"
echo ""
echo "To stop all servers: ./run_servers.sh stop"
