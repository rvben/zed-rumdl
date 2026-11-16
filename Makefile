.PHONY: all toolchain fmt-check lint test build-wasm check-assets check-tag

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

# Release gate: TAG (e.g. v0.0.5) must name the version in extension.toml, so
# the registry never receives a tag whose source declares a different version.
check-tag:
	@test -n "$(TAG)" || { echo "usage: make check-tag TAG=vX.Y.Z" >&2; exit 2; }
	@version=$$(sed -n 's/^version = "\(.*\)"$$/\1/p' extension.toml); \
	test -n "$$version" || { echo "extension.toml declares no version" >&2; exit 1; }; \
	test "$(TAG)" = "v$$version" || { echo "tag $(TAG) does not match extension.toml version $$version (expected v$$version)" >&2; exit 1; }; \
	echo "tag $(TAG) matches extension.toml version $$version"
