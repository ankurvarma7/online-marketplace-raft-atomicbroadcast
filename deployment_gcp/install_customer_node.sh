#!/usr/bin/env bash
# Manual install: run on customer-stack (or your customer VM) after copying
# python_customer_db under ~/marketplace/. Not invoked by deploy_to_vms.sh.
# Required env: INSTALL_NODE_ID (0-4), PEERS (full five-node map).
#
# Installs Docker MySQL, Python venv, applies schema, starts customer_db_service.py.

set -euo pipefail

INSTALL_NODE_ID="${INSTALL_NODE_ID:?set INSTALL_NODE_ID 0-4}"
PEERS="${PEERS:?set PEERS}"
REPO_HOME="${REPO_HOME:-$HOME/marketplace}"
MYSQL_ROOT_PW="${MYSQL_ROOT_PW:-my-secret-pw}"
MYSQL_DB="${MYSQL_DB:-customer_db}"

if [ ! -f "$REPO_HOME/python_customer_db/customer_db_service.py" ]; then
    echo "Missing $REPO_HOME/python_customer_db — copy python_customer_db first."
    exit 1
fi

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update -qq
sudo apt-get install -y -qq docker.io python3 python3-venv python3-pip

sudo systemctl enable --now docker 2>/dev/null || sudo service docker start

if ! sudo docker ps -a --format '{{.Names}}' | grep -q '^customer-mysql$'; then
    sudo docker run -d --name customer-mysql --restart unless-stopped \
        -e MYSQL_ROOT_PASSWORD="$MYSQL_ROOT_PW" \
        -e MYSQL_DATABASE="$MYSQL_DB" \
        -p 3306:3306 \
        mysql:8.0
    echo "Waiting for MySQL to accept connections..."
    for _ in $(seq 1 45); do
        if sudo docker exec customer-mysql mysql -uroot -p"$MYSQL_ROOT_PW" -e "SELECT 1" &>/dev/null; then
            break
        fi
        sleep 2
    done
fi

SCHEMA="${REPO_HOME}/create_table.sql"
if [ -f "$SCHEMA" ]; then
    sudo docker exec -i customer-mysql mysql -uroot -p"$MYSQL_ROOT_PW" < "$SCHEMA"
fi

VENV="${REPO_HOME}/venv"
if [ ! -d "$VENV" ]; then
    python3 -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "${VENV}/bin/activate"
pip install -q -r "${REPO_HOME}/python_customer_requirements.txt"

mkdir -p "$REPO_HOME/logs"
pkill -f 'customer_db_service.py' 2>/dev/null || true
sleep 1

export NUM_NODES=5
export PEERS
export GRPC_BIND_ADDR="${GRPC_BIND_ADDR:-0.0.0.0:50051}"
export UDP_BIND_ADDR="${UDP_BIND_ADDR:-0.0.0.0:50000}"
export CUSTOMER_DB_HOST="${CUSTOMER_DB_HOST:-127.0.0.1}"
export CUSTOMER_DB_PORT="${CUSTOMER_DB_PORT:-3306}"
export CUSTOMER_DB_USER="${CUSTOMER_DB_USER:-root}"
export CUSTOMER_DB_PASSWORD="$MYSQL_ROOT_PW"
export CUSTOMER_DB_NAME="$MYSQL_DB"

nohup "${VENV}/bin/python" "$REPO_HOME/python_customer_db/customer_db_service.py" \
    --id "$INSTALL_NODE_ID" \
    >>"$REPO_HOME/logs/customer_db_${INSTALL_NODE_ID}.log" 2>&1 &

echo $! >"$REPO_HOME/logs/customer_db_${INSTALL_NODE_ID}.pid"
echo "customer_db node ${INSTALL_NODE_ID} started (PID $(cat "$REPO_HOME/logs/customer_db_${INSTALL_NODE_ID}.pid"))"
