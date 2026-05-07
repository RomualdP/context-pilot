#!/bin/bash
# Supervisor script for TUI - handles reload requests

CONFIG_FILE=".context-pilot/config.json"

# Raise FD limit — macOS defaults to 256 which is too low for kqueue
# (1 FD per watched file/dir). A session with 60+ tree folders open
# plus .git/ watches easily exceeds the default.
ulimit -n 2048 2>/dev/null

# Load environment variables from .env
if [ -f .env ]; then
    export $(grep -v '^#' .env | xargs)
fi

while true; do
    # Run the TUI
    cargo run --release -- "$@"

    # Check if reload was requested
    if [ -f "$CONFIG_FILE" ]; then
        RELOAD=$(grep -E '"reload_requested":\s*true' "$CONFIG_FILE" 2>/dev/null)
        if [ -n "$RELOAD" ]; then
            echo "Reload requested, restarting..."
            # Small delay to ensure file is fully written
            sleep 0.2
            # Add --resume-stream if not already present
            if [[ ! " $* " =~ " --resume-stream " ]]; then
                set -- "$@" --resume-stream
            fi
            continue
        fi
    fi

    # No reload requested, exit
    break
done
