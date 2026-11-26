#!/bin/sh
# Measures the coverage of the workspace inside the Linux container of the
# pre-push hook, with the current stable toolchain, like CI. Writes
# target/lcov.info.
set -eu

rustup update stable >/dev/null 2>&1
rustup default stable >/dev/null
rustup component add llvm-tools-preview >/dev/null 2>&1
if ! cargo llvm-cov --version >/dev/null 2>&1; then
    cargo install cargo-llvm-cov --locked --quiet
fi
# Test binaries of earlier builds would add their stale line counts to the
# report, so measure from a clean build.
cargo llvm-cov clean --workspace
cargo llvm-cov --workspace --all-features --lcov --output-path target/lcov.info --quiet
