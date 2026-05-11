#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "== imauth local launcher =="

# Check Rust build
if [ ! -f "$ROOT_DIR/target/release/imauth-server" ] || [ ! -f "$ROOT_DIR/target/release/imauth" ]; then
    echo "Building imauth-server and imauth CLI..."
    cd "$ROOT_DIR"
    cargo build --release -p imauth-server -p imauth-cli
fi

# Start NATS if not running
if ! lsof -i :4222 > /dev/null 2>&1; then
    echo "Starting NATS on :4222 ..."
    if command -v nats-server > /dev/null 2>&1; then
        nohup nats-server > /tmp/nats.log 2>&1 &
    else
        echo "NATS not found. Install with: brew install nats-server"
        exit 1
    fi
    sleep 1
else
    echo "NATS already running on :4222"
fi

# Start Chrome CDP if not running
if ! curl -s http://localhost:9222/json/version > /dev/null 2>&1; then
    echo "Starting Chrome CDP on :9222 ..."
    CHROME_PATH=""
    if [ -f "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" ]; then
        CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    elif command -v google-chrome > /dev/null 2>&1; then
        CHROME_PATH="google-chrome"
    elif command -v chromium > /dev/null 2>&1; then
        CHROME_PATH="chromium"
    else
        echo "Chrome/Chromium not found. Please install Google Chrome."
        exit 1
    fi
    nohup "$CHROME_PATH" --remote-debugging-port=9222 --headless=new --disable-gpu --no-sandbox > /tmp/chrome_cdp.log 2>&1 &
echo "Waiting for Chrome CDP..."
    for i in {1..10}; do
        if curl -s http://localhost:9222/json/version > /dev/null 2>&1; then
            break
        fi
        sleep 1
    done
else
    echo "Chrome CDP already running on :9222"
fi

# Start imauth-server
if lsof -i :50051 > /dev/null 2>&1; then
    echo "imauth-server already running on :50051"
else
    echo "Starting imauth-server on :50051 ..."
    cd "$ROOT_DIR"
    nohup ./target/release/imauth-server serve > /tmp/imauth-server.log 2>&1 &
echo "Waiting for imauth-server..."
    for i in {1..10}; do
        if lsof -i :50051 > /dev/null 2>&1; then
            break
        fi
        sleep 1
    done
fi

echo ""
echo "All services are up!"
echo "  - NATS:     nats://localhost:4222"
echo "  - Chrome:   http://localhost:9222"
echo "  - imauth:   http://localhost:50051"
echo ""
echo "Quick start:"
echo "  $ ./target/release/imauth credentials save -p instagram -u USER -w PASS"
echo "  $ ./target/release/imauth credentials get -p instagram"
echo ""
echo "Stop server:  pkill -f imauth-server"
echo "Logs:         tail -f /tmp/imauth-server.log"
