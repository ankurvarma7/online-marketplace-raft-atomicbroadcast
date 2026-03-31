#!/bin/bash
# Deploy script for building all components
# Run: ./deploy.sh [--linux]
#
# Options:
#   --linux    Cross-compile for Linux (x86_64-unknown-linux-gnu)
#              Required when deploying to Linux VMs from macOS.
#              Prerequisites: rustup target add x86_64-unknown-linux-gnu
#                             brew install zig && cargo install cargo-zigbuild

set -e

LINUX_TARGET="x86_64-unknown-linux-gnu"
BUILD_FOR_LINUX=false

for arg in "$@"; do
    case "$arg" in
        --linux) BUILD_FOR_LINUX=true ;;
    esac
done

if [ "$BUILD_FOR_LINUX" = true ]; then
    echo "Building all components for Linux ($LINUX_TARGET)..."
    if [[ "$(uname -s)" == "Darwin" ]]; then
        echo "Detected macOS — using cargo-zigbuild for cross-compilation..."
        cargo zigbuild --release --target "$LINUX_TARGET"
    else
        cargo build --release --target "$LINUX_TARGET"
    fi
    echo ""
    echo "Tip: to build only product_db (faster), add -p product_db to the cargo command above."
    BINARY_DIR="target/${LINUX_TARGET}/release"
else
    echo "Building all components for host platform..."
    cargo build --release
    BINARY_DIR="target/release"
fi

echo ""
echo "Build complete! Binaries are in $BINARY_DIR/"
echo ""
echo "Components:"
echo "  $BINARY_DIR/customer_db           - Customer Database (gRPC, port 50051)"
echo "  $BINARY_DIR/product_db            - Product Database (gRPC, port 50052)"
echo "  $BINARY_DIR/seller_server         - Seller Frontend (REST, port 8082)"
echo "  $BINARY_DIR/buyer_server          - Buyer Frontend (REST, port 8083)"
echo "  $BINARY_DIR/financial_transactions - Financial Tx (SOAP, port 8085)"
echo "  $BINARY_DIR/seller_client         - Seller CLI"
echo "  $BINARY_DIR/buyer_client          - Buyer CLI"
echo "  $BINARY_DIR/evaluator             - Performance Evaluator"
echo ""
echo "To run locally (host build only):"
echo "  1. ./$BINARY_DIR/customer_db"
echo "  2. ./$BINARY_DIR/product_db"
echo "  3. ./$BINARY_DIR/seller_server"
echo "  4. ./$BINARY_DIR/buyer_server"
echo "  5. ./$BINARY_DIR/financial_transactions"
