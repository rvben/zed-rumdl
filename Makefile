.PHONY: all fmt-check lint test build-wasm check-assets

# CI runs these targets; every step is runnable locally.
all: fmt-check lint test build-wasm

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

build-wasm:
	cargo build --release --target wasm32-wasip1

# Ground-truth drift check: verify the asset names this extension computes still
# exist in rumdl's latest release. Needs the network and an authenticated `gh`.
# Runs weekly in CI (release-drift workflow) and on demand locally.
check-assets:
	cargo test -- --ignored computed_asset_names_exist_in_latest_release
