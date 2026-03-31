#!/bin/bash
# Deploy to GCP VMs
# Usage: ./deploy_to_vms.sh [--skip-build]
#
# Prerequisites:
#   - gcloud CLI installed and configured
#   - 8 VMs created (see create_vms.sh): customer-db, product-db-1..5, seller-server, buyer-server
#   - SSH keys configured
#   - For cross-compilation from macOS:
#       rustup target add x86_64-unknown-linux-gnu
#       brew install zig && cargo install cargo-zigbuild

set -e

SKIP_BUILD=false
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=true ;;
    esac
done

ZONE="${ZONE:-us-central1-a}"

# VM instance names (must match create_vms.sh)
CUSTOMER_DB_INSTANCE="customer-db"
SELLER_SERVER_INSTANCE="seller-server"
BUYER_SERVER_INSTANCE="buyer-server"

LINUX_TARGET="x86_64-unknown-linux-gnu"

# Resolve internal IPs from instance names for inter-service communication
resolve_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].networkIP)'
}

echo "Resolving VM internal IPs..."
CUSTOMER_DB_IP=$(resolve_ip "$CUSTOMER_DB_INSTANCE")
PRODUCT_DB_IP1=$(resolve_ip "product-db-1")
PRODUCT_DB_IP2=$(resolve_ip "product-db-2")
PRODUCT_DB_IP3=$(resolve_ip "product-db-3")
PRODUCT_DB_IP4=$(resolve_ip "product-db-4")
PRODUCT_DB_IP5=$(resolve_ip "product-db-5")
SELLER_SERVER_IP=$(resolve_ip "$SELLER_SERVER_INSTANCE")
BUYER_SERVER_IP=$(resolve_ip "$BUYER_SERVER_INSTANCE")
echo "  $CUSTOMER_DB_INSTANCE  -> $CUSTOMER_DB_IP"
echo "  product-db-1 -> $PRODUCT_DB_IP1"
echo "  product-db-2 -> $PRODUCT_DB_IP2"
echo "  product-db-3 -> $PRODUCT_DB_IP3"
echo "  product-db-4 -> $PRODUCT_DB_IP4"
echo "  product-db-5 -> $PRODUCT_DB_IP5"
echo "  $SELLER_SERVER_INSTANCE -> $SELLER_SERVER_IP"
echo "  $BUYER_SERVER_INSTANCE  -> $BUYER_SERVER_IP"

# Identical on every Raft node; peer URLs use internal IPs and port 50052 on each VM
RAFT_PEERS="1=http://${PRODUCT_DB_IP1}:50052,2=http://${PRODUCT_DB_IP2}:50052,3=http://${PRODUCT_DB_IP3}:50052,4=http://${PRODUCT_DB_IP4}:50052,5=http://${PRODUCT_DB_IP5}:50052"

# Frontends use one replica; point at the current Raft leader for correct reads/writes (often node 1 after bootstrap)
PRODUCT_DB_ADDR_FOR_FRONTENDS="${PRODUCT_DB_IP1}:50052"

if [ "$SKIP_BUILD" = false ]; then
    echo "Building release binaries for Linux ($LINUX_TARGET)..."
    if [[ "$(uname -s)" == "Darwin" ]]; then
        echo "Detected macOS — cross-compiling for Linux via cargo-zigbuild..."
        cargo zigbuild --release --target "$LINUX_TARGET"
    else
        cargo build --release --target "$LINUX_TARGET"
    fi
else
    echo "Skipping build (--skip-build flag set)"
fi

BINARY_DIR="target/${LINUX_TARGET}/release"

deploy_binary() {
    local instance="$1" binary_name="$2" env_vars="$3"
    echo "Deploying $binary_name to $instance..."
    gcloud compute scp "$BINARY_DIR/$binary_name" "$instance:~/${binary_name}.new" --zone="$ZONE"
    sleep 3
    gcloud compute ssh "$instance" --zone="$ZONE" \
        --ssh-flag="-o" --ssh-flag="StrictHostKeyChecking=accept-new" \
        --command="pkill -f './$binary_name' 2>/dev/null || true; sleep 1; mv -f ~/${binary_name}.new ~/$binary_name; chmod +x ~/$binary_name; $env_vars nohup ~/$binary_name </dev/null > ~/${binary_name}.log 2>&1 & echo '$binary_name started'"
}

deploy_product_db() {
    local instance="$1" node_id="$2"
    echo "Deploying product_db (NODE_ID=$node_id) to $instance..."
    gcloud compute scp "$BINARY_DIR/product_db" "$instance:~/product_db.new" --zone="$ZONE"
    sleep 3
    gcloud compute ssh "$instance" --zone="$ZONE" \
        --ssh-flag="-o" --ssh-flag="StrictHostKeyChecking=accept-new" \
        --command="pkill -f './product_db' 2>/dev/null || true; sleep 1; mv -f ~/product_db.new ~/product_db; chmod +x ~/product_db; NODE_ID=${node_id} BIND_ADDR=0.0.0.0:50052 RAFT_PEERS='${RAFT_PEERS}' DATA_DIR=\$HOME/data/${node_id} nohup ~/product_db </dev/null > ~/product_db.log 2>&1 & echo 'product_db started'"
}

deploy_binary "$CUSTOMER_DB_INSTANCE" "customer_db" \
    "CUSTOMER_DB_BIND_ADDR=0.0.0.0:50051"

deploy_product_db "product-db-1" 1
deploy_product_db "product-db-2" 2
deploy_product_db "product-db-3" 3
deploy_product_db "product-db-4" 4
deploy_product_db "product-db-5" 5

deploy_binary "$SELLER_SERVER_INSTANCE" "seller_server" \
    "SELLER_SERVER_BIND_ADDR=0.0.0.0:8082 CUSTOMER_DB_ADDR=${CUSTOMER_DB_IP}:50051 PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}"

deploy_binary "$BUYER_SERVER_INSTANCE" "buyer_server" \
    "BUYER_SERVER_BIND_ADDR=0.0.0.0:8083 CUSTOMER_DB_ADDR=${CUSTOMER_DB_IP}:50051 PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS} FINANCIAL_TX_ADDR=127.0.0.1:8085"

echo ""
echo "Deployment complete!"
echo "Set client environment variables:"
echo "  export SELLER_SERVER_ADDR=http://${SELLER_SERVER_IP}:8082"
echo "  export BUYER_SERVER_ADDR=http://${BUYER_SERVER_IP}:8083"
echo ""
echo "Raft product_db: PRODUCT_DB_ADDR for frontends is ${PRODUCT_DB_ADDR_FOR_FRONTENDS} (node 1)."
echo "If that node is not the leader, set PRODUCT_DB_ADDR to the leader IP (see product_db logs / gRPC metadata x-raft-leader-addr)."
