#!/bin/bash
# Optional: systemd units on GCP VMs (run after deploy_to_vms.sh).
# Python customer_db on customer-stack is not installed by repo scripts; see deployment_gcp/install_customer_node.sh if you install manually.

set -e

ZONE="${ZONE:-us-east5-a}"

resolve_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].networkIP)'
}

echo "Resolving VM internal IPs..."
CUSTOMER_IP=$(resolve_ip customer-stack)
PRODUCT_IP=$(resolve_ip product-stack)

if [ -n "${CUSTOMER_DB_ADDRS:-}" ]; then
    :
else
    CUSTOMER_DB_ADDRS="${CUSTOMER_IP}:50051,${CUSTOMER_IP}:50052,${CUSTOMER_IP}:50053,${CUSTOMER_IP}:50054,${CUSTOMER_IP}:50055"
fi

RAFT_PEERS="1=http://127.0.0.1:50052,2=http://127.0.0.1:50053,3=http://127.0.0.1:50054,4=http://127.0.0.1:50055,5=http://127.0.0.1:50056"
PRODUCT_DB_PEERS="${PRODUCT_IP}:50052,${PRODUCT_IP}:50053,${PRODUCT_IP}:50054,${PRODUCT_IP}:50055,${PRODUCT_IP}:50056"
PRODUCT_DB_ADDR_FOR_FRONTENDS="${PRODUCT_IP}:50052"

# Args: instance, systemd_unit_name, executable basename in \$HOME, env_lines
create_service() {
    local instance="$1" unit="$2" exe="$3" env_lines="$4"
    echo "Setting up ${unit}.service on ${instance}..."
    gcloud compute ssh "$instance" --zone="$ZONE" --command="
        USER=\$(whoami)
        sudo tee /etc/systemd/system/${unit}.service > /dev/null << UNIT
[Unit]
Description=${unit}
After=network.target

[Service]
ExecStart=/home/\${USER}/${exe}
${env_lines}
Restart=always
RestartSec=3
User=\${USER}

[Install]
WantedBy=multi-user.target
UNIT
        sudo systemctl daemon-reload
        sudo systemctl enable ${unit}
        sudo systemctl restart ${unit}
        echo '${unit} service enabled'
    "
}

create_buyer_service() {
    local port="$1"
    local name="buyer_server_${port}"
    echo "Setting up ${name}.service on buyer-frontend..."
    gcloud compute ssh buyer-frontend --zone="$ZONE" --command="
        USER=\$(whoami)
        sudo tee /etc/systemd/system/${name}.service > /dev/null << UNIT
[Unit]
Description=${name}
After=network.target financial_transactions.service

[Service]
ExecStart=/home/\${USER}/buyer_server
Environment=BUYER_SERVER_BIND_ADDR=0.0.0.0:${port}
Environment=CUSTOMER_DB_ADDRS=${CUSTOMER_DB_ADDRS}
Environment=PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}
Environment=PRODUCT_DB_PEERS=${PRODUCT_DB_PEERS}
Environment=FINANCIAL_TX_ADDR=127.0.0.1:8085
Restart=always
RestartSec=3
User=\${USER}

[Install]
WantedBy=multi-user.target
UNIT
        sudo systemctl daemon-reload
        sudo systemctl enable ${name}
        sudo systemctl restart ${name}
        echo '${name} enabled'
    "
}

for nid in 1 2 3 4 5; do
    create_service "product-stack" "product_db_${nid}" "product_db" \
        "Environment=NODE_ID=${nid}
Environment=BIND_ADDR=0.0.0.0:$((50051 + nid))
Environment=RAFT_PEERS=${RAFT_PEERS}
Environment=DATA_DIR=%h/data/${nid}"
done

for port in 8082 8088 8089 8090; do
    create_service seller-frontend "seller_server_${port}" "seller_server" \
        "Environment=SELLER_SERVER_BIND_ADDR=0.0.0.0:${port}
Environment=CUSTOMER_DB_ADDRS=${CUSTOMER_DB_ADDRS}
Environment=PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}
Environment=PRODUCT_DB_PEERS=${PRODUCT_DB_PEERS}"
done

create_service buyer-frontend "financial_transactions" "financial_transactions" \
    "Environment=FINANCIAL_TX_BIND_ADDR=0.0.0.0:8085"

for port in 8083 8084 8086 8087; do
    create_buyer_service "$port"
done

echo ""
echo "Done. Python customer_db on customer-stack is not managed by systemd here (see deployment_gcp/install_customer_node.sh)."
