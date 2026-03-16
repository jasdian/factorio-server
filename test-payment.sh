#!/usr/bin/env bash
# End-to-end payment test: anvil + server + register + send ETH + verify confirmation
set -euo pipefail

# Anvil default account 0
SENDER_KEY="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
SENDER_ADDR="0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
DEPOSIT_ADDR="0x0000000000000000000000000000000000000000"
SERVER="http://127.0.0.1:3000"
RPC="http://127.0.0.1:8545"
TEST_DIR="/tmp/factorio-seasons-test"

cleanup() {
    echo ""
    echo "--- Cleaning up ---"
    [[ -n "${ANVIL_PID:-}" ]] && kill "$ANVIL_PID" 2>/dev/null && echo "Stopped anvil ($ANVIL_PID)"
    [[ -n "${SERVER_PID:-}" ]] && kill "$SERVER_PID" 2>/dev/null && echo "Stopped server ($SERVER_PID)"
    wait 2>/dev/null
}
trap cleanup EXIT

for cmd in anvil cast cargo curl jq; do
    command -v "$cmd" >/dev/null || { echo "Missing: $cmd"; exit 1; }
done

echo "=== Payment Integration Test ==="

# 0. Kill stale processes, fresh DB
pkill -f "factorio-seasons test-config" 2>/dev/null || true
lsof -ti:3000 2>/dev/null | xargs -r kill 2>/dev/null || true
lsof -ti:8545 2>/dev/null | xargs -r kill 2>/dev/null || true
sleep 1
rm -f "$TEST_DIR/seasons.db"*
mkdir -p "$TEST_DIR"/{static,data,archive}
cp -r static/* "$TEST_DIR/static/" 2>/dev/null || true

# 1. Start anvil (auto-mine — one block per tx)
echo "Starting anvil..."
anvil --port 8545 --balance 10000 --silent &
ANVIL_PID=$!
sleep 1
cast block-number --rpc-url "$RPC" >/dev/null || { echo "FAIL: anvil not responding"; exit 1; }
echo "Anvil running (PID $ANVIL_PID), block $(cast block-number --rpc-url "$RPC")"

# 2. Build and start server
echo "Building server..."
cargo build --quiet 2>&1
echo "Starting server..."
./target/debug/factorio-seasons test-config.toml 2>&1 &
SERVER_PID=$!
sleep 2
curl -sf "$SERVER/health" >/dev/null || { echo "FAIL: server not responding"; exit 1; }
echo "Server running (PID $SERVER_PID)"

# 3. Register (safe to do immediately — registration is DB-only)
echo ""
echo "--- Registering player ---"
REG_RESPONSE=$(curl -sf "$SERVER/api/register" \
    -H 'Content-Type: application/json' \
    -d "{\"factorio_name\": \"TestPlayer\", \"eth_address\": \"$SENDER_ADDR\"}")
echo "$REG_RESPONSE" | jq .

REG_ID=$(echo "$REG_RESPONSE" | jq -r '.registration_id')
AMOUNT_WEI=$(echo "$REG_RESPONSE" | jq -r '.amount_wei')
REG_STATUS=$(echo "$REG_RESPONSE" | jq -r '.status')

echo "Registration ID: $REG_ID"
echo "Required amount: $AMOUNT_WEI wei (~\$1 at \$3000/ETH)"
echo "Initial status:  $REG_STATUS"

[[ "$REG_STATUS" == "awaiting_payment" ]] || { echo "FAIL: expected awaiting_payment"; exit 1; }
echo "PASS: Registration created"

# 4. Wait for poller to capture initial block (poller sleeps 15s on first loop)
echo ""
echo "--- Waiting for poller to capture initial block (18s) ---"
sleep 18
echo "Poller should have captured initial block by now"

# 5. NOW send the payment — it will land in a new block after the initial snapshot
echo ""
echo "--- Sending $AMOUNT_WEI wei -> $DEPOSIT_ADDR ---"
TX_HASH=$(cast send \
    --rpc-url "$RPC" \
    --private-key "$SENDER_KEY" \
    --value "$AMOUNT_WEI" \
    "$DEPOSIT_ADDR" \
    --json | jq -r '.transactionHash')
echo "TX hash: $TX_HASH"

# Mine extra block to be safe
cast rpc evm_mine --rpc-url "$RPC" >/dev/null 2>&1 || true
echo "Block: $(cast block-number --rpc-url "$RPC")"

# 6. Wait for next poll cycle to scan new blocks (15s + buffer)
echo ""
echo "--- Waiting for poller to scan new blocks (20s) ---"
sleep 20

# 7. Check result
echo ""
echo "--- Checking registration status ---"
STATUS_RESPONSE=$(curl -sf "$SERVER/api/register/$REG_ID")
echo "$STATUS_RESPONSE" | jq .

FINAL_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.status')
FINAL_TX=$(echo "$STATUS_RESPONSE" | jq -r '.tx_hash // empty')

echo ""
echo "Final status: $FINAL_STATUS"
echo "TX hash:      ${FINAL_TX:-none}"

if [[ "$FINAL_STATUS" == "confirmed" ]]; then
    echo ""
    echo "============================="
    echo "  PASS: Payment confirmed!"
    echo "============================="
    exit 0
else
    echo ""
    echo "============================="
    echo "  FAIL: Status is $FINAL_STATUS (expected confirmed)"
    echo "============================="
    exit 1
fi
