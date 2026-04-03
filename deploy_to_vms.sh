#!/bin/bash
# Deploy PA3-aligned stack to GCP (see deployment_gcp/PA3_TOPOLOGY.md)
# Usage: ./deploy_to_vms.sh [--skip-build]
#
# Uses scp for binaries + small remote runner scripts (avoids long ssh --command / exit 255).
# Set ZONE to match your VMs (default us-east5-a). If ssh fails, try: GCP_IAP_SSH=1 ./deploy_to_vms.sh
#
# Prerequisites:
#   - gcloud CLI; VMs from create_vms.sh (Option C)
#   - rustup target x86_64-unknown-linux-gnu; macOS: zig + cargo-zigbuild
#   - Python customer_db on customer-stack is your responsibility; optional:
#       export CUSTOMER_DB_ADDRS='ip:port,...' before running

set -e

SKIP_BUILD=false
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=true ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_GCP="${SCRIPT_DIR}/deployment_gcp"
ZONE="${ZONE:-us-east5-a}"
LINUX_TARGET="x86_64-unknown-linux-gnu"

# Optional: GCP_IAP_SSH=1 if direct SSH is blocked (uses Identity-Aware Proxy tunnel)
GCP_FLAGS=()
if [[ "${GCP_IAP_SSH:-}" == 1 ]]; then
    GCP_FLAGS+=(--tunnel-through-iap)
fi

resolve_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].networkIP)'
}

echo "Resolving VM internal IPs (ZONE=${ZONE})..."
CUSTOMER_IP=$(resolve_ip customer-stack)
PRODUCT_IP=$(resolve_ip product-stack)
SELLER_FE_IP=$(resolve_ip seller-frontend)
BUYER_FE_IP=$(resolve_ip buyer-frontend)

echo "  customer-stack  -> $CUSTOMER_IP"
echo "  product-stack   -> $PRODUCT_IP"
echo "  seller-frontend -> $SELLER_FE_IP"
echo "  buyer-frontend  -> $BUYER_FE_IP"

if [ -n "${CUSTOMER_DB_ADDRS:-}" ]; then
    :
else
    CUSTOMER_DB_ADDRS="${CUSTOMER_IP}:50051,${CUSTOMER_IP}:50052,${CUSTOMER_IP}:50053,${CUSTOMER_IP}:50054,${CUSTOMER_IP}:50055"
fi

RAFT_PEERS="1=http://127.0.0.1:50052,2=http://127.0.0.1:50053,3=http://127.0.0.1:50054,4=http://127.0.0.1:50055,5=http://127.0.0.1:50056"
PRODUCT_DB_PEERS="${PRODUCT_IP}:50052,${PRODUCT_IP}:50053,${PRODUCT_IP}:50054,${PRODUCT_IP}:50055,${PRODUCT_IP}:50056"
PRODUCT_DB_ADDR_FOR_FRONTENDS="${PRODUCT_IP}:50052"

ENV_FILE="$(mktemp)"
trap 'rm -f "$ENV_FILE"' EXIT
{
    echo "export CUSTOMER_DB_ADDRS='${CUSTOMER_DB_ADDRS}'"
    echo "export PRODUCT_DB_ADDR='${PRODUCT_DB_ADDR_FOR_FRONTENDS}'"
    echo "export PRODUCT_DB_PEERS='${PRODUCT_DB_PEERS}'"
    echo "export FINANCIAL_TX_ADDR='127.0.0.1:8085'"
} >"$ENV_FILE"

if [ "$SKIP_BUILD" = false ]; then
    echo "Building Rust release binaries for Linux ($LINUX_TARGET)..."
    if [[ "$(uname -s)" == "Darwin" ]]; then
        cargo zigbuild --release --target "$LINUX_TARGET" -p product_db -p seller_server -p buyer_server -p financial_transactions
    else
        cargo build --release --target "$LINUX_TARGET" -p product_db -p seller_server -p buyer_server -p financial_transactions
    fi
else
    echo "Skipping build (--skip-build)"
fi

BINARY_DIR="target/${LINUX_TARGET}/release"

# ── product_db x5 on product-stack (ports 50052–50056) ───────────────────────
echo ""
echo "=== product-stack: scp product_db + remote_run_product_stack.sh ==="
gcloud compute scp "${GCP_FLAGS[@]}" "$BINARY_DIR/product_db" "product-stack:~/product_db.new" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "${DEPLOY_GCP}/remote_run_product_stack.sh" "product-stack:~/remote_run_product_stack.sh" --zone="$ZONE"
gcloud compute ssh "${GCP_FLAGS[@]}" product-stack --zone="$ZONE" \
    --ssh-flag="-o" --ssh-flag="StrictHostKeyChecking=accept-new" \
    --command="chmod +x ~/remote_run_product_stack.sh && bash ~/remote_run_product_stack.sh"

# ── seller-frontend x4 ───────────────────────────────────────────────────────
echo ""
echo "=== seller-frontend: scp seller_server + env + remote_run_seller.sh ==="
gcloud compute scp "${GCP_FLAGS[@]}" "$ENV_FILE" "seller-frontend:~/pa3_deploy.env" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "$BINARY_DIR/seller_server" "seller-frontend:~/seller_server.new" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "${DEPLOY_GCP}/remote_run_seller.sh" "seller-frontend:~/remote_run_seller.sh" --zone="$ZONE"
gcloud compute ssh "${GCP_FLAGS[@]}" seller-frontend --zone="$ZONE" \
    --ssh-flag="-o" --ssh-flag="StrictHostKeyChecking=accept-new" \
    --command="chmod +x ~/remote_run_seller.sh && bash ~/remote_run_seller.sh"

# ── buyer-frontend: financial + buyers x4 ───────────────────────────────────
echo ""
echo "=== buyer-frontend: scp binaries + env + remote_run_buyer.sh ==="
gcloud compute scp "${GCP_FLAGS[@]}" "$ENV_FILE" "buyer-frontend:~/pa3_deploy.env" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "$BINARY_DIR/financial_transactions" "buyer-frontend:~/financial_transactions.new" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "$BINARY_DIR/buyer_server" "buyer-frontend:~/buyer_server.new" --zone="$ZONE"
gcloud compute scp "${GCP_FLAGS[@]}" "${DEPLOY_GCP}/remote_run_buyer.sh" "buyer-frontend:~/remote_run_buyer.sh" --zone="$ZONE"
gcloud compute ssh "${GCP_FLAGS[@]}" buyer-frontend --zone="$ZONE" \
    --ssh-flag="-o" --ssh-flag="StrictHostKeyChecking=accept-new" \
    --command="chmod +x ~/remote_run_buyer.sh && bash ~/remote_run_buyer.sh"

echo ""
echo "Deployment complete."
echo ""
echo "Internal service env (for documentation):"
echo "  CUSTOMER_DB_ADDRS=${CUSTOMER_DB_ADDRS}"
echo "  PRODUCT_DB_PEERS=${PRODUCT_DB_PEERS}"
echo "  PRODUCT_DB_ADDR=${PRODUCT_DB_ADDR_FOR_FRONTENDS}"
echo ""
echo "For clients / evaluator (HTTP), set external IPs and ports, e.g.:"
echo "  SELLER_SERVER_ADDRS=http://SELLER_EXT:8082,http://SELLER_EXT:8088,http://SELLER_EXT:8089,http://SELLER_EXT:8090"
echo "  BUYER_SERVER_ADDRS=http://BUYER_EXT:8083,http://BUYER_EXT:8084,http://BUYER_EXT:8086,http://BUYER_EXT:8087"
echo ""
echo "If the Raft leader is not on :50052, set PRODUCT_DB_ADDR on both frontends to the leader (see product-stack ~/logs/product_db_*.log)."
