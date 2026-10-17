#!/usr/bin/env bash
# Stops `catervas serve` and everything it started: the agents' sessions, the
# connector servers, the sandbox and preview containers.
#
# Usage: scripts/catervas-stop.sh
set -uo pipefail

state="${XDG_STATE_HOME:-$HOME/.local/state}/catervas"
pid_file="$state/serve.pid"
grace_seconds=15

# Every running `catervas serve`: the one the start script recorded, and any
# started by hand.
leaders=()
if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
  leaders+=("$(cat "$pid_file")")
fi
while read -r pid; do
  # The program is catervas and its first argument is serve.
  command="$(tr '\0' '\n' <"/proc/$pid/cmdline" 2>/dev/null | sed -n 2p)"
  [[ "$command" == serve ]] || continue
  [[ " ${leaders[*]} " == *" $pid "* ]] || leaders+=("$pid")
done < <(pgrep -x catervas || true)

# A process and all its descendants, found through their parents, so that a
# child that left the session is still found.
descendants() {
  local parent=$1 child
  echo "$parent"
  for child in $(pgrep -P "$parent" || true); do
    descendants "$child"
  done
}

targets=()
for leader in "${leaders[@]}"; do
  targets+=($(descendants "$leader"))
  # The members of its session too, in case a parent exited and left orphans.
  sid="$(ps -o sid= -p "$leader" 2>/dev/null | tr -d ' ')"
  [[ -n "$sid" ]] && targets+=($(pgrep -s "$sid" || true))
done
mapfile -t targets < <(printf '%s\n' "${targets[@]}" | grep -v '^$' | sort -un)

if ((${#leaders[@]} == 0)); then
  echo "Catervas is not running."
else
  # Ctrl-C first, which catervas handles by stopping its team cleanly.
  kill -INT "${leaders[@]}" 2>/dev/null
  for _ in $(seq 1 $((grace_seconds * 2))); do
    alive=0
    for pid in "${leaders[@]}"; do kill -0 "$pid" 2>/dev/null && alive=1; done
    ((alive)) || break
    sleep 0.5
  done
  # Then everything left, gently and then by force.
  kill -TERM "${targets[@]}" 2>/dev/null
  sleep 2
  left=()
  for pid in "${targets[@]}"; do kill -0 "$pid" 2>/dev/null && left+=("$pid"); done
  ((${#left[@]})) && kill -KILL "${left[@]}" 2>/dev/null
  echo "Stopped Catervas and ${#targets[@]} of its processes."
fi
rm -f "$pid_file"

# The containers Catervas started carry its labels; nothing else is touched.
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  containers="$(docker ps -aq --filter label=catervas.project)"
  if [[ -n "$containers" ]]; then
    docker rm -f $containers >/dev/null
    echo "Removed $(wc -w <<<"$containers") Catervas container(s)."
  fi
fi
