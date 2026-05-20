#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "== imauth local launcher (user-driven login) =="

# Check Rust build
if [ ! -f "$ROOT_DIR/target/release/imauth-server" ] || [ ! -f "$ROOT_DIR/target/release/imauth" ]; then
    echo "Building imauth-server and imauth CLI..."
    cd "$ROOT_DIR"
    cargo build --release -p imauth-server -p imauth-cli
fi

# Start Chrome in headed mode with CDP if not running
if ! curl -s http://localhost:9222/json/version > /dev/null 2>&1; then
    echo "Starting Chrome (headed) with CDP on :9222 ..."
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
    # Headed mode — user can see and interact with the browser window
    nohup "$CHROME_PATH" --remote-debugging-port=9222 --disable-gpu --no-sandbox --user-data-dir=/tmp/imauth-chrome > /tmp/chrome_cdp.log 2>&1 &
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
echo "  - Chrome:   http://localhost:9222 (headed — you can see the window)"
echo "  - imauth:   http://localhost:50051"
echo ""
echo "Quick start (user-driven login):"
echo "  $ ./target/release/imauth login -p instagram"
echo "  $ ./target/release/imauth login -p naver"
echo ""
echo "The Chrome window will open the login page."
echo "Log in manually — imauth detects the session cookie automatically."
echo ""
echo "Stop server:  pkill -f imauth-server"
echo "Logs:         tail -f /tmp/imauth-server.log"
