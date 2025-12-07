#!/usr/bin/env bash
# Generate self-signed certificates for development
# These are NOT suitable for production use!

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CERTS_DIR="$PROJECT_DIR/certs"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== Generating Development Certificates ===${NC}"

# Create certs directory
mkdir -p "$CERTS_DIR"
cd "$CERTS_DIR"

# Check if certs already exist
if [[ -f "ca.crt" && -f "server.crt" ]]; then
    echo -e "${GREEN}Certificates already exist in $CERTS_DIR${NC}"
    echo "To regenerate, delete the certs/ directory and run again."
    exit 0
fi

echo -e "${GREEN}Generating CA key and certificate...${NC}"
openssl ecparam -genkey -name prime256v1 -out ca.key 2>/dev/null
openssl req -new -x509 -days 365 -key ca.key -out ca.crt \
    -subj "/CN=TaskRun Dev CA" 2>/dev/null

echo -e "${GREEN}Generating server key and certificate...${NC}"
openssl ecparam -genkey -name prime256v1 -out server.key 2>/dev/null
openssl req -new -key server.key -out server.csr \
    -subj "/CN=localhost" 2>/dev/null

# Create extension file for SANs
cat > san.ext << EOF
subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1
EOF

openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt -extfile san.ext 2>/dev/null

# Clean up temporary files
rm -f server.csr san.ext ca.srl

# Set permissions
chmod 600 ca.key server.key
chmod 644 ca.crt server.crt

echo ""
echo -e "${GREEN}Certificates generated successfully:${NC}"
echo "  CA Certificate:     $CERTS_DIR/ca.crt"
echo "  CA Key:             $CERTS_DIR/ca.key"
echo "  Server Certificate: $CERTS_DIR/server.crt"
echo "  Server Key:         $CERTS_DIR/server.key"
echo ""
echo "Workers should trust ca.crt (CA pinning)"
echo "Control plane uses server.crt + server.key"
