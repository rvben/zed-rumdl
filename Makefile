.PHONY: all toolchain fmt-check lint test build-wasm check-assets

# The target Zed compiles Rust extensions for.
WASM_TARGET := wasm32-wasip2

# CI runs these targets; every step is runnable locally.
all: fmt-check lint test build-wasm

# Install the toolchain pinned in rust-toolchain.toml, then the wasm target.
# The target is added explicitly because a RUSTUP_TOOLCHAIN environment
# override makes rustup skip the targets a toolchain file lists.
toolchain:
	rustup toolchain install
	rustup target add $(WASM_TARGET)

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

build-wasm:
	cargo build --release --target $(WASM_TARGET)

# Ground-truth drift check: verify the asset names this extension computes still
# exist in rumdl's latest release. Needs the network and an authenticated `gh`.
# Runs weekly in CI (release-drift workflow) and on demand locally.
check-assets:
	cargo test -- --ignored computed_asset_names_exist_in_latest_release
