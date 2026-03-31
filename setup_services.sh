#!/bin/bash
# Set up systemd services on VMs so they auto-start on boot.
# Usage: ./setup_services.sh
#
# Run this ONCE after deploying binaries. After this, services will
# survive VM stop/start cycles automatically.

set -e

ZONE="${ZONE:-us-central1-a}"

resolve_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].networkIP)'
}

echo "Resolving VM internal IPs..."
CUSTOMER_DB_IP=$(resolve_ip "customer-db")
PRODUCT_DB_IP1=$(resolve_ip "product-db-1")
PRODUCT_DB_IP2=$(resolve_ip "product-db-2")
PRODUCT_DB_IP3=$(resolve_ip "product-db-3")
PRODUCT_DB_IP4=$(resolve_ip "product-db-4")
PRODUCT_DB_IP5=$(resolve_ip "product-db-5")
SELLER_SERVER_IP=$(resolve_ip "seller-server")
BUYER_SERVER_IP=$(resolve_ip "buyer-server")
echo "  customer-db   -> $CUSTOMER_DB_IP"
echo "  product-db-1  -> $PRODUCT_DB_IP1"
echo "  product-db-2  -> $PRODUCT_DB_IP2"
echo "  product-db-3  -> $PRODUCT_DB_IP3"
echo "  product-db-4  -> $PRODUCT_DB_IP4"
echo "  product-db-5  -> $PRODUCT_DB_IP5"
echo "  seller-server -> $SELLER_SERVER_IP"
echo "  buyer-server  -> $BUYER_SERVER_IP"

RAFT_PEERS="1=http://${PRODUCT_DB_IP1}:50052,2=http://${PRODUCT_DB_IP2}:50052,3=http://${PRODUCT_DB_IP3}:50052,4=http://${PRODUCT_DB_IP4}:50052,5=http://${PRODUCT_DB_IP5}:50052"
PRODUCT_DB_ADDR_FOR_FRONTENDS="${PRODUCT_DB_IP1}:50052"

create_service() {
    local instance="$1" binary="$2" env_lines="$3"
    echo "Setting up $binary service on $instance..."
    gcloud compute ssh "$instance" --zone="$ZONE" --command="
        USER=\$(whoami)
        sudo tee /etc/systemd/system/${binary}.service > /dev/null << UNIT
[Unit]
Description=${binary}
After=network.target

[Service]
ExecStart=/home/\${USER}/${binary}
${env_lines}
Restart=always
RestartSec=3
User=\${USER}

[Install]
WantedBy=multi-user.target
UNIT
        sudo systemctl daemon-reload
        sudo systemctl enable ${binary}
        sudo systemctl restart ${binary}
        echo '$binary service enabled and started'
    "
}

create_service "customer-db" "customer_db" \
    "Environment=CUSTOMER_DB_BIND_ADDR=0.0.0.0:50051"

create_service "product-db-1" "product_db" \
    "Environment=NODE_ID=1
Environment=BIND_ADDR=0.0.0.0:50052
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/1"

create_service "product-db-2" "product_db" \
    "Environment=NODE_ID=2
Environment=BIND_ADDR=0.0.0.0:50052
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/2"

create_service "product-db-3" "product_db" \
    "Environment=NODE_ID=3
Environment=BIND_ADDR=0.0.0.0:50052
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/3"

create_service "product-db-4" "product_db" \
    "Environment=NODE_ID=4
Environment=BIND_ADDR=0.0.0.0:50052
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/4"

create_service "product-db-5" "product_db" \
    "Environment=NODE_ID=5
Environment=BIND_ADDR=0.0.0.0:50052
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/5"

create_service "seller-server" "seller_server" \
    "Environment=SELLER_SERVER_BIND_ADDR=0.0.0.0:8082
Environment=CUSTOMER_DB_ADDR=${CUSTOMER_DB_IP}:50051
Environment=PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}"

create_service "buyer-server" "buyer_server" \
    "Environment=BUYER_SERVER_BIND_ADDR=0.0.0.0:8083
Environment=CUSTOMER_DB_ADDR=${CUSTOMER_DB_IP}:50051
Environment=PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}
Environment=FINANCIAL_TX_ADDR=127.0.0.1:8085"

echo ""
echo "All services configured and started!"
echo "Services will auto-start on VM boot."
echo ""
echo "Useful commands:"
echo "  gcloud compute ssh <instance> --zone=$ZONE --command='sudo systemctl status <service>'"
echo "  gcloud compute ssh <instance> --zone=$ZONE --command='sudo journalctl -u <service> -n 50'"
