#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

if [[ "$(git branch --show-current)" != "custom" ]]; then
  echo "Run fork-sync.sh from the custom branch." >&2
  exit 1
fi

git fetch upstream --tags
stable_tag="${1:-$(git tag --list 'v[0-9]*' | grep -E '^v[0-9]+(\.[0-9]+)*$' | sort -V | tail -n 1 || true)}"
if [[ -z "$stable_tag" ]]; then
  echo "No stable upstream tag was found." >&2
  exit 1
fi

git merge --no-edit "$stable_tag"
cargo check -p zed
