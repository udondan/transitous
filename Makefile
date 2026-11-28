# The latest stable Rust of rustup, like CI, where there is rustup. A
# packaged toolchain, like Homebrew's, can be older and miss new lints.
# `rustup run` does not put the toolchain first in PATH, so cargo would still
# run clippy and rustfmt of such a toolchain. Hence the PATH.
TOOLCHAIN := $(if $(shell command -v rustup),$(shell dirname "$$(rustup which --toolchain stable cargo)"))
ifneq ($(TOOLCHAIN),)
export PATH := $(TOOLCHAIN):$(PATH)
endif
CARGO ?= cargo

.PHONY: build release run test lint coverage live hooks

build:
	@$(CARGO) build --workspace

release:
	@$(CARGO) build --release -p transitous

run:
	@$(CARGO) run -p transitous

test:
	@$(CARGO) test --workspace --all-features

lint:
	@$(CARGO) fmt --all -- --check
	@$(CARGO) clippy --workspace --all-targets -- -D warnings
	@$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

# Checks that the tests cover every line changed against origin/main, like
# the patch status of Codecov.
coverage:
	@.githooks/pre-push

# A smoke test against the real API with four requests. Needs
# TRANSITOUS_CONTACT. By hand only, never in CI.
live:
	@bash scripts/live.sh

# Links the hooks into the repository's hooks directory, which all worktrees
# share. The links point to the main checkout's copies of the scripts.
hooks:
	@hooks="$$(git rev-parse --path-format=absolute --git-common-dir)/hooks" && \
		mkdir -p "$$hooks" && \
		for hook in pre-commit pre-push; do \
			ln -sf "../../.githooks/$$hook" "$$hooks/$$hook" && \
			echo "installed $$hooks/$$hook"; \
		done
