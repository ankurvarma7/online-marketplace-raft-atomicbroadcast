#!/usr/bin/env bash
# Run on buyer-frontend after binaries and ~/pa3_deploy.env are present.

set -euo pipefail

# shellcheck source=/dev/null
source "${HOME}/pa3_deploy.env"

FT="${HOME}/financial_transactions"
BS="${HOME}/buyer_server"
if [[ -f "${FT}.new" ]]; then mv -f "${FT}.new" "$FT"; fi
if [[ -f "${BS}.new" ]]; then mv -f "${BS}.new" "$BS"; fi
chmod +x "$FT" "$BS"
mkdir -p "${HOME}/logs"

pkill -f "${HOME}/buyer_server" 2>/dev/null || true
pkill -f "${HOME}/financial_transactions" 2>/dev/null || true
sleep 1

nohup "$FT" </dev/null >"${HOME}/logs/financial_transactions.log" 2>&1 &
sleep 2

for port in 8083 8084 8086 8087; do
  BUYER_SERVER_BIND_ADDR="0.0.0.0:${port}" \
  CUSTOMER_DB_ADDRS="${CUSTOMER_DB_ADDRS}" \
  PRODUCT_DB_ADDR="${PRODUCT_DB_ADDR}" \
  PRODUCT_DB_PEERS="${PRODUCT_DB_PEERS}" \
  FINANCIAL_TX_ADDR="${FINANCIAL_TX_ADDR:-127.0.0.1:8085}" \
    nohup "$BS" </dev/null >"${HOME}/logs/buyer_${port}.log" 2>&1 &
done

echo "buyers_started"
