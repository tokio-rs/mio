# Targets available via Rustup that are supported.
TARGETS ?= aarch64-apple-ios aarch64-linux-android arm-linux-androideabi i686-unknown-linux-gnu x86_64-apple-darwin x86_64-apple-ios x86_64-pc-windows-msvc x86_64-unknown-freebsd x86_64-unknown-illumos x86_64-unknown-linux-gnu x86_64-unknown-netbsd x86_64-unknown-redox wasm32-wasi
# Example value: `nightly-x86_64-apple-darwin`.
RUSTUP_TOOLCHAIN ?= $(shell rustup show active-toolchain | cut -d' ' -f1)
# Architecture target. Example value: `x86_64-apple-darwin`.
RUSTUP_TARGET    ?= $(shell echo $(RUSTUP_TOOLCHAIN) | cut -d'-' -f2,3,4,5)

test:
	cargo test --all-features

# Test everything for the current OS/architecture and check all targets in
# $TARGETS.
test_all: check_all_targets
	cargo hack test --feature-powerset
	cargo hack test --feature-powerset --release

# NOTE: Requires a nightly compiler.
# NOTE: Keep `RUSTFLAGS` and `RUSTDOCFLAGS` in sync to ensure the doc tests
# compile correctly.
test_sanitizer:
	@if [ -z $${SAN+x} ]; then echo "Required '\$$SAN' variable is not set" 1>&2; exit 1; fi
	RUSTFLAGS="-Z sanitizer=$$SAN -Z sanitizer-memory-track-origins" \
	RUSTDOCFLAGS="-Z sanitizer=$$SAN -Z sanitizer-memory-track-origins" \
	cargo test -Z build-std --all-features --target $(RUSTUP_TARGET)

# Check all targets using all features.
check_all_targets: $(TARGETS)
$(TARGETS):
	cargo hack check --target $@ --feature-powerset

# Installs all required targets for `check_all_targets`.
install_targets:
	rustup target add $(TARGETS)

# NOTE: when using this command you might want to change the `test` target to
# only run a subset of the tests you're actively working on.
dev:
	find src/ tests/ Makefile Cargo.toml | entr -d -c $(MAKE) test

clean:
	cargo clean

.PHONY: test test_all check_all_targets $(TARGETS) dev clean
