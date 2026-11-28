#!/usr/bin/env bash
# Prints the Homebrew formula for a release.
# Usage: homebrew-formula.sh <version> <directory with the archives>
set -euo pipefail

version=$1
dir=$2
base="https://github.com/udondan/transitous/releases/download/transitous-v$version"

# Prints the url and sha256 lines of the archive for a target.
archive() {
  local file="transitous-$version-$1.tar.gz"
  local sum
  sum=$(sha256sum "$dir/$file" | cut -d' ' -f1)
  printf '      url "%s/%s"\n      sha256 "%s"\n' "$base" "$file" "$sum"
}

cat <<EOF
# typed: false
# frozen_string_literal: true

class Transitous < Formula
  desc "CLI and MCP server for public transport on Transitous, with realtime status"
  homepage "https://github.com/udondan/transitous"
  version "$version"
  license "Apache-2.0"

  on_macos do
    on_arm do
$(archive aarch64-apple-darwin)
    end
    on_intel do
$(archive x86_64-apple-darwin)
    end
  end

  on_linux do
    on_arm do
$(archive aarch64-unknown-linux-musl)
    end
    on_intel do
$(archive x86_64-unknown-linux-musl)
    end
  end

  def install
    bin.install "transitous"
    bash_completion.install "completions/transitous.bash" => "transitous"
    zsh_completion.install "completions/_transitous"
    fish_completion.install "completions/transitous.fish"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/transitous --version")
  end
end
EOF
