#!/usr/bin/env bash
# Hard reset: deletes everything Catervas has stored on this computer, so the
# next `catervas serve` starts from first-run setup, as on a new machine.
#
# Usage: scripts/catervas-reset.sh [--dry-run] [--yes] [--keep-key] [--keep-images]
#   --dry-run      list what would be deleted, delete nothing
#   --yes          do not ask before deleting
#   --keep-key     keep the AI account's key (credential.json, keychain entry)
#   --keep-images  keep Catervas's Docker images (the sandbox), which take minutes to rebuild
#
# It deletes: Catervas's settings folder, every project's .catervas/ folder
# (team, log, contracts, worktrees, sandboxes), the local branches Catervas
# made for its tasks, its containers and images, and the start script's log.
# It never touches GitHub, nor files committed in your repositories.
set -uo pipefail

dry=0 yes=0 keep_key=0 keep_images=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry=1 ;;
    --yes) yes=1 ;;
    --keep-key) keep_key=1 ;;
    --keep-images) keep_images=1 ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
config="${XDG_CONFIG_HOME:-$HOME/.config}/catervas"
state="${XDG_STATE_HOME:-$HOME/.local/state}/catervas"

# Every project Catervas has worked in: a .catervas/ folder holding a team,
# found under the home folder, plus the last project Catervas remembers.
projects=()
while IFS= read -r team; do
  projects+=("$(dirname "$(dirname "$team")")")
done < <(find "$HOME" -maxdepth 4 \( -name node_modules -o -name .git -o -name target -o -path '*/.claude/worktrees' \) -prune -o -path '*/.catervas/team.yaml' -print 2>/dev/null)
if [[ -f "$config/last-project" ]]; then
  last="$(cat "$config/last-project")"
  [[ -d "$last/.catervas" ]] && projects+=("$last")
fi
mapfile -t projects < <(printf '%s\n' "${projects[@]}" | grep -v '^$' | sort -u)

# The local branches Catervas made in a project: each contract's recorded
# branch, and the names older versions used (feature|fix|docs/CTV-n, docs/folder-n).
branches_of() {
  local project=$1
  {
    grep -h '^branch:' "$project"/.catervas/contracts/*.yaml 2>/dev/null | sed 's/^branch:[[:space:]]*//; s/^"//; s/"$//'
    git -C "$project" for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null |
      grep -E '^((feature|fix|docs)/CTV-[0-9]+|docs/folder-[0-9]+)$'
  } | sort -u
}

echo "Catervas hard reset$([[ $dry == 1 ]] && echo ' (dry run: nothing is deleted)')"
echo
echo "Settings and accounts:"
if [[ -d "$config" ]]; then
  for entry in "$config"/* "$config"/.[!.]*; do
    [[ -e "$entry" ]] || continue
    [[ $keep_key == 1 && "$(basename "$entry")" == credential.json ]] && continue
    echo "  $entry"
  done
else
  echo "  (none)"
fi
[[ -d "$state" ]] && echo "  $state"
[[ $keep_key == 1 ]] && echo "  (keeping the AI account's key)"
echo
echo "Projects (their .catervas/ folder and Catervas's local branches):"
((${#projects[@]})) || echo "  (none)"
for project in "${projects[@]}"; do
  tracked="$(git -C "$project" ls-files .catervas 2>/dev/null | wc -l)"
  echo "  $project/.catervas$([[ $tracked -gt 0 ]] && echo "  ($tracked files in it are committed; deleting them changes the repository)")"
  while IFS= read -r branch; do
    [[ -n "$branch" ]] && echo "    branch $branch"
  done < <(branches_of "$project")
done
echo
echo "Docker:"
have_docker=0
command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 && have_docker=1
if ((have_docker)); then
  containers="$(docker ps -aq --filter label=catervas.project | wc -l)"
  echo "  $containers container(s) labelled catervas.project"
  if [[ $keep_images == 0 ]]; then
    docker images --format '{{.Repository}}:{{.Tag}}' | grep -E '^catervas[/-]' | sed 's/^/  image /'
  else
    echo "  (keeping the images)"
  fi
else
  echo "  (Docker is not running: containers and images are left)"
fi
echo

if [[ $dry == 1 ]]; then
  echo "Dry run: nothing was deleted."
  exit 0
fi
if [[ $yes == 0 ]]; then
  read -r -p "Type delete to delete all of this: " answer
  [[ "$answer" == delete ]] || { echo "Nothing was deleted."; exit 1; }
fi

# Catervas first, with everything it started.
[[ -x "$here/catervas-stop.sh" ]] && "$here/catervas-stop.sh" >/dev/null

# Files a sandbox wrote can belong to another user; Docker removes those.
remove() {
  rm -rf -- "$1" 2>/dev/null
  if [[ -e "$1" ]] && ((have_docker)); then
    docker run --rm -v "$(dirname "$1")":/p alpine:3.22 rm -rf -- "/p/$(basename "$1")" >/dev/null 2>&1 ||
      docker run --rm -v "$(dirname "$1")":/p busybox rm -rf -- "/p/$(basename "$1")" >/dev/null 2>&1
  fi
  [[ -e "$1" ]] && echo "  could not delete $1" >&2
}

for project in "${projects[@]}"; do
  mapfile -t branches < <(branches_of "$project")
  remove "$project/.catervas"
  git -C "$project" worktree prune 2>/dev/null
  current="$(git -C "$project" branch --show-current 2>/dev/null)"
  for branch in "${branches[@]}"; do
    [[ -z "$branch" || "$branch" == "$current" ]] && continue
    git -C "$project" branch -D "$branch" >/dev/null 2>&1
  done
  echo "Reset $project"
done

if [[ -d "$config" ]]; then
  for entry in "$config"/* "$config"/.[!.]*; do
    [[ -e "$entry" ]] || continue
    [[ $keep_key == 1 && "$(basename "$entry")" == credential.json ]] && continue
    remove "$entry"
  done
  [[ $keep_key == 0 ]] && rmdir "$config" 2>/dev/null
fi
remove "$state"
if [[ $keep_key == 0 ]] && command -v secret-tool >/dev/null 2>&1; then
  secret-tool clear service catervas 2>/dev/null
fi

if ((have_docker)); then
  docker ps -aq --filter label=catervas.project | xargs -r docker rm -f >/dev/null
  if [[ $keep_images == 0 ]]; then
    docker images --format '{{.Repository}}:{{.Tag}}' | grep -E '^catervas[/-]' | xargs -r docker rmi -f >/dev/null
  fi
fi

echo "Catervas is reset. The next catervas serve starts with first-run setup."
