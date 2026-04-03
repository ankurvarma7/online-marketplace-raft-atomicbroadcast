#!/usr/bin/env bash
# Print export lines for evaluator_distributed (external HTTP URLs, PA3 four replicas each).
# Usage from repo root:
#   eval "$(./deployment_gcp/export_evaluator_env.sh)"
#
# Uses instances seller-frontend and buyer-frontend (see PA3_TOPOLOGY.md).

set -euo pipefail

ZONE="${ZONE:-us-east5-a}"

external_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].accessConfigs[0].natIP)' 2>/dev/null || true
}

internal_ip() {
    gcloud compute instances describe "$1" --zone="$ZONE" \
        --format='get(networkInterfaces[0].networkIP)' 2>/dev/null || true
}

SELLER_EXT="$(external_ip seller-frontend)"
BUYER_EXT="$(external_ip buyer-frontend)"

if [ -z "$SELLER_EXT" ] || [ -z "$BUYER_EXT" ]; then
    echo "Error: could not read external IPs for seller-frontend and/or buyer-frontend in zone $ZONE." >&2
    echo "Ensure PA3 VMs exist and have external access." >&2
    exit 1
fi

SELLER_LIST="http://${SELLER_EXT}:8082,http://${SELLER_EXT}:8088,http://${SELLER_EXT}:8089,http://${SELLER_EXT}:8090"
BUYER_LIST="http://${BUYER_EXT}:8083,http://${BUYER_EXT}:8084,http://${BUYER_EXT}:8086,http://${BUYER_EXT}:8087"

echo "# Zone: $ZONE (PA3 Option C: four seller / four buyer ports on seller-frontend + buyer-frontend)"
echo "# seller-frontend external: $SELLER_EXT  internal: $(internal_ip seller-frontend)"
echo "# buyer-frontend  external: $BUYER_EXT  internal: $(internal_ip buyer-frontend)"
echo ""
echo "export SELLER_SERVER_ADDRS='${SELLER_LIST}'"
echo "export BUYER_SERVER_ADDRS='${BUYER_LIST}'"
