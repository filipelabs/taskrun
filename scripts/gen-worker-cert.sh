#!/usr/bin/env bash
# Generate a worker certificate for development/testing.
# This bypasses the enrollment endpoint for quick local testing.
#
# Usage: ./scripts/gen-worker-cert.sh [worker-id]
# Example: ./scripts/gen-worker-cert.sh worker-dev

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CERTS_DIR="$PROJECT_DIR/certs"

# Worker ID (default: worker-dev)
WORKER_ID="${1:-worker-dev}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}=== Generating Worker Certificate ===${NC}"
echo -e "Worker ID: ${GREEN}${WORKER_ID}${NC}"

# Check that CA exists
if [[ ! -f "$CERTS_DIR/ca.crt" || ! -f "$CERTS_DIR/ca.key" ]]; then
    echo -e "${RED}Error: CA certificate/key not found in $CERTS_DIR${NC}"
    echo "Run scripts/gen-dev-certs.sh first to generate the CA."
    exit 1
fi

cd "$CERTS_DIR"

# Generate worker key (PKCS#8 format required by rustls/rcgen)
echo -e "${GREEN}Generating worker private key...${NC}"
openssl ecparam -genkey -name prime256v1 -out worker.key.ec 2>/dev/null
openssl pkcs8 -topk8 -nocrypt -in worker.key.ec -out worker.key
rm worker.key.ec

# Generate CSR with CN=worker:<id>
echo -e "${GREEN}Generating certificate signing request...${NC}"
openssl req -new -key worker.key -out worker.csr \
    -subj "/CN=worker:$WORKER_ID/O=TaskRun Worker" 2>/dev/null

# Sign with CA (valid for 7 days)
echo -e "${GREEN}Signing certificate with CA...${NC}"
openssl x509 -req -days 7 -in worker.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out worker.crt 2>/dev/null

# Clean up
rm -f worker.csr ca.srl

# Set permissions
chmod 600 worker.key
chmod 644 worker.crt

echo ""
echo -e "${GREEN}Worker certificate generated successfully:${NC}"
echo "  Certificate: $CERTS_DIR/worker.crt"
echo "  Private Key: $CERTS_DIR/worker.key"
echo "  Worker ID:   $WORKER_ID"
echo "  Valid for:   7 days"
echo ""
echo "The worker will present this certificate when connecting to the control plane."
