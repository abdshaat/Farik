#!/usr/bin/env bash
# Starts `catervas serve` in the background, in a session of its own, so that
# scripts/catervas-stop.sh can stop it together with everything it started.
#
# Usage: scripts/catervas-start.sh [--port <port>] [--no-open]
# The options are passed to `catervas serve`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/release/catervas"
state="${XDG_STATE_HOME:-$HOME/.local/state}/catervas"
pid_file="$state/serve.pid"
log_file="$state/serve.log"

if [[ ! -x "$binary" ]]; then
  echo "Catervas is not built yet. Run: cargo build --release" >&2
  exit 1
fi

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
  echo "Catervas is already running (process $(cat "$pid_file"))."
  echo "Its log: $log_file"
  exit 0
fi

mkdir -p "$state"
: >"$log_file"
cd "$root"
# setsid makes catervas the leader of a new session: every process it starts
# joins that session, which is what the stop script ends.
setsid "$binary" serve "$@" >>"$log_file" 2>&1 </dev/null &
pid=$!
echo "$pid" >"$pid_file"

# Wait for the connection link, or for catervas to stop early.
for _ in $(seq 1 60); do
  if ! kill -0 "$pid" 2>/dev/null; then
    rm -f "$pid_file"
    echo "Catervas stopped while starting. Its log:" >&2
    tail -n 20 "$log_file" >&2
    exit 1
  fi
  link="$(grep -o 'http://[^ ]*/connect#[^ ]*' "$log_file" | head -n 1 || true)"
  if [[ -n "$link" ]]; then
    echo "Catervas is running (process $pid)."
    echo "Open: $link"
    echo "Its log: $log_file"
    echo "Stop it with: scripts/catervas-stop.sh"
    exit 0
  fi
  sleep 0.5
done

echo "Catervas is running (process $pid), but printed no link yet. Its log: $log_file"
