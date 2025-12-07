#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${BLUE}=== TaskRun Demo (with TLS) ===${NC}"
echo ""

# Generate certificates if needed
echo -e "${GREEN}Checking certificates...${NC}"
"$SCRIPT_DIR/gen-dev-certs.sh"
echo ""

# Build
echo -e "${GREEN}Building...${NC}"
cargo build --workspace --quiet

# Cleanup function
cleanup() {
    echo ""
    echo -e "${GREEN}Cleaning up...${NC}"
    pkill -f taskrun-control-plane 2>/dev/null || true
    pkill -f taskrun-worker 2>/dev/null || true
}
trap cleanup EXIT

# Start control plane
echo -e "${GREEN}Starting control plane...${NC}"
cargo run -p taskrun-control-plane --quiet &
sleep 2

# Start worker
echo -e "${GREEN}Starting worker...${NC}"
cargo run -p taskrun-worker --quiet &
sleep 2

# List workers
echo ""
echo -e "${BLUE}Connected workers:${NC}"
cargo run -p taskrun-cli --quiet -- list-workers
echo ""

# Create task
echo -e "${BLUE}Creating task...${NC}"
TASK_OUTPUT=$(cargo run -p taskrun-cli --quiet -- create-task \
    --agent support_triage \
    --input '{"query": "Hello from demo!"}')
echo "$TASK_OUTPUT"

# Extract task ID
TASK_ID=$(echo "$TASK_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')
echo ""

# Wait for completion
echo -e "${GREEN}Waiting for task to complete...${NC}"
sleep 3

# Get final status
echo ""
echo -e "${BLUE}Final task status:${NC}"
cargo run -p taskrun-cli --quiet -- get-task "$TASK_ID"

echo ""
echo -e "${GREEN}Demo complete!${NC}"
