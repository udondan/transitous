#!/usr/bin/env bash
# Copies the versions of the library crates into the requirements of the root
# [workspace.dependencies] and refreshes Cargo.lock. Release Please bumps the
# versions in the crate manifests only, and a requirement that no longer
# matches the crate breaks the build.
set -euo pipefail

for crate in motis transitous-core; do
  version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "crates/$crate/Cargo.toml" | head -n1)
  if [[ -z $version ]]; then
    echo "::error::No version in crates/$crate/Cargo.toml"
    exit 1
  fi
  sed -i.bak -E "s|^($crate = \{ path = \"crates/$crate\", version = \")[^\"]*\"|\1$version\"|" Cargo.toml
  rm Cargo.toml.bak
  grep -q "^$crate = { path = \"crates/$crate\", version = \"$version\" }" Cargo.toml || {
    echo "::error::Could not set the requirement of $crate in Cargo.toml"
    exit 1
  }
done

cargo update --workspace
