#!/usr/bin/env bash
# Builds Catervas from this folder and starts `catervas serve` in the
# background, in a session of its own, so that scripts/catervas-stop.sh can
# stop it together with everything it started.
#
# It assumes nothing was ever built: it installs what it can for you (Rust,
# Node, pnpm, Claude Code, all in your home folder, no sudo), says how to get
# what it cannot (git, curl, Docker), then builds the web app and Catervas and
# starts it. Later runs only rebuild what changed.
#
# Usage: scripts/catervas-start.sh [--no-build] [--port <port>] [--no-open]
#   --no-build   start what was built last time
# Any other option is passed to `catervas serve`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$root/target/release/catervas"
state="${XDG_STATE_HOME:-$HOME/.local/state}/catervas"
pid_file="$state/serve.pid"
log_file="$state/serve.log"
build_log="$state/build.log"
node_version="$(cat "$root/.node-version" 2>/dev/null || echo 24.14.0)"
tools="$HOME/.local/share/catervas-tools"

build=1
serve_args=()
for arg in "$@"; do
  case "$arg" in
    --no-build) build=0 ;;
    -h | --help) sed -n '2,14p' "$0"; exit 0 ;;
    *) serve_args+=("$arg") ;;
  esac
done

say() { printf '%s\n' "$*"; }
fail() { printf '%s\n' "$*" >&2; exit 1; }
mkdir -p "$state"

if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
  say "Catervas is already running (process $(cat "$pid_file"))."
  say "Its log: $log_file"
  say "To start it again with the newest code: scripts/catervas-stop.sh, then this script."
  exit 0
fi

# --- What Catervas needs ---------------------------------------------------

os="$(uname -s)"
case "$(uname -m)" in
  x86_64 | amd64) node_arch=x64 ;;
  aarch64 | arm64) node_arch=arm64 ;;
  *) node_arch="" ;;
esac

need_system() { # name, how to install it
  command -v "$1" >/dev/null 2>&1 && return 0
  say "Catervas needs $1, which is not installed. $2"
  return 1
}

missing=0
need_system git "Install it with your package manager, e.g. 'sudo apt install git' (Linux) or 'xcode-select --install' (Mac)." || missing=1
need_system curl "Install it with your package manager, e.g. 'sudo apt install curl'." || missing=1
need_system docker "Install Docker Desktop from https://docs.docker.com/get-docker/ (or Docker Engine on Linux)." || missing=1
((missing)) && fail "Install what is missing above, then run this script again."
docker info >/dev/null 2>&1 ||
  fail "Docker is installed but not running. Start Docker (Docker Desktop, or 'sudo service docker start'), then run this script again."

# Rust, at the version rust-toolchain.toml pins; rustup fetches it on first use.
if ! command -v cargo >/dev/null 2>&1; then
  if [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
    say "Installing Rust (rustup) in your home folder…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --profile minimal --default-toolchain none --no-modify-path >>"$build_log" 2>&1 ||
      fail "Installing Rust failed. See $build_log"
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# Node, at the major version .node-version pins, unless one is installed.
node_major() { node --version 2>/dev/null | sed 's/^v//; s/\..*//'; }
if [[ "$(node_major)" != "${node_version%%.*}" ]]; then
  local_node="$tools/node-v$node_version"
  if [[ ! -x "$local_node/bin/node" ]]; then
    case "$os" in
      Linux) platform="linux-$node_arch" ;;
      Darwin) platform="darwin-$node_arch" ;;
      *) platform="" ;;
    esac
    [[ -n "$node_arch" && -n "$platform" ]] ||
      fail "Catervas needs Node $node_version. Install it from https://nodejs.org, then run this script again."
    say "Installing Node $node_version in your home folder…"
    mkdir -p "$tools"
    curl -fsSL "https://nodejs.org/dist/v$node_version/node-v$node_version-$platform.tar.gz" |
      tar -xz -C "$tools" || fail "Downloading Node failed."
    mv "$tools/node-v$node_version-$platform" "$local_node"
  fi
  export PATH="$local_node/bin:$PATH"
fi

# pnpm, at the version package.json names, through Node's own corepack.
export COREPACK_ENABLE_DOWNLOAD_PROMPT=0
pnpm() { corepack pnpm "$@"; }

# Claude Code, which the agents run in.
export PATH="$HOME/.local/bin:$PATH"
if ! command -v claude >/dev/null 2>&1; then
  say "Installing Claude Code in your home folder…"
  curl -fsSL https://claude.ai/install.sh | bash >>"$build_log" 2>&1 ||
    say "Installing Claude Code failed; Catervas's setup will say how to install it."
fi

# --- Build -----------------------------------------------------------------

if ((build)); then
  : >"$build_log"
  step() { # what, command…
    local what=$1
    shift
    say "$what…"
    if ! (cd "$root" && "$@") >>"$build_log" 2>&1; then
      say "That failed. The last lines of $build_log:" >&2
      tail -n 25 "$build_log" >&2
      exit 1
    fi
  }
  step "Getting the web app's packages" pnpm install --frozen-lockfile
  step "Generating the web app's types" pnpm -r --if-present generate
  step "Building the web app" pnpm --filter @catervas/web build
  step "Building Catervas (the first build takes several minutes)" cargo build --release -p catervas
elif [[ ! -x "$binary" ]]; then
  fail "Catervas is not built yet. Run this script without --no-build."
fi

# --- Start -----------------------------------------------------------------

: >"$log_file"
cd "$root"
# setsid makes catervas the leader of a new session: every process it starts
# joins that session, which is what the stop script ends. macOS has no setsid.
if command -v setsid >/dev/null 2>&1; then
  setsid "$binary" serve "${serve_args[@]}" >>"$log_file" 2>&1 </dev/null &
else
  nohup "$binary" serve "${serve_args[@]}" >>"$log_file" 2>&1 </dev/null &
fi
pid=$!
echo "$pid" >"$pid_file"

# Wait for the connection link, or for catervas to stop early.
for _ in $(seq 1 60); do
  if ! kill -0 "$pid" 2>/dev/null; then
    rm -f "$pid_file"
    say "Catervas stopped while starting. Its log:" >&2
    tail -n 20 "$log_file" >&2
    exit 1
  fi
  link="$(grep -o 'http://[^ ]*/connect#[^ ]*' "$log_file" | head -n 1 || true)"
  if [[ -n "$link" ]]; then
    say "Catervas is running (process $pid)."
    say "Open: $link"
    say "Its log: $log_file"
    say "Stop it with: scripts/catervas-stop.sh"
    exit 0
  fi
  sleep 0.5
done

say "Catervas is running (process $pid), but printed no link yet. Its log: $log_file"
