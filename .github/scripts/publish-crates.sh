#!/usr/bin/env bash
# Publishes every crate of the workspace whose version is not on crates.io
# yet, dependencies first. A release can push several tags at once, and each
# runs this script, so it skips versions that are published already and
# accepts a version that another run published at the same time.
set -euo pipefail

CRATES=(motis motis-sim transitous-core transitous)

# Path of a crate in the sparse index, see
# https://doc.rust-lang.org/cargo/reference/registry-index.html#index-files
index_path() {
  local name=$1
  case ${#name} in
    1) echo "1/$name" ;;
    2) echo "2/$name" ;;
    3) echo "3/${name:0:1}/$name" ;;
    *) echo "${name:0:2}/${name:2:2}/$name" ;;
  esac
}

published() {
  curl -fsS "https://index.crates.io/$(index_path "$1")" 2>/dev/null |
    jq -e --arg v "$2" 'select(.vers == $v)' >/dev/null
}

metadata=$(cargo metadata --no-deps --format-version 1)
for crate in "${CRATES[@]}"; do
  version=$(jq -r --arg c "$crate" '.packages[] | select(.name == $c) | .version' <<<"$metadata")
  if published "$crate" "$version"; then
    echo "$crate $version is on crates.io already"
    continue
  fi
  if cargo publish --locked -p "$crate"; then
    continue
  fi
  # Another run may have published it a moment ago.
  for _ in $(seq 18); do
    if published "$crate" "$version"; then
      echo "$crate $version was published by another run"
      continue 2
    fi
    sleep 10
  done
  echo "::error::Could not publish $crate $version"
  exit 1
done
