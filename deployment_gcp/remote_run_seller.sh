#!/usr/bin/env bash
# Run on seller-frontend after ~/seller_server (or .new) and ~/pa3_deploy.env are present.

set -euo pipefail

# shellcheck source=/dev/null
source "${HOME}/pa3_deploy.env"

BIN="${HOME}/seller_server"
if [[ -f "${BIN}.new" ]]; then
  mv -f "${BIN}.new" "$BIN"
fi
chmod +x "$BIN"
mkdir -p "${HOME}/logs"

pkill -f "${HOME}/seller_server" 2>/dev/null || :
sleep 1

for port in 8082 8088 8089 8090; do
  SELLER_SERVER_BIND_ADDR="0.0.0.0:${port}" \
  CUSTOMER_DB_ADDRS="${CUSTOMER_DB_ADDRS}" \
  PRODUCT_DB_ADDR="${PRODUCT_DB_ADDR}" \
  PRODUCT_DB_PEERS="${PRODUCT_DB_PEERS}" \
    nohup "$BIN" </dev/null >"${HOME}/logs/seller_${port}.log" 2>&1 &
done

echo "sellers_started"
