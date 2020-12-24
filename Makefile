# Targets available via Rustup that are supported.
TARGETS ?= "x86_64-apple-darwin" "x86_64-unknown-freebsd" "x86_64-unknown-linux-gnu" "x86_64-pc-windows-gnu"

test:
	cargo test --all-features

# Test everythubg for the current OS/architecture and check for all targets in
# $TARGETS.
test_all: check_all_targets
	cargo hack test --feature-powerset --skip guide,extra-docs,tcp,udp,uds,pipe,os-util
	cargo hack test --feature-powerset --skip guide,extra-docs,tcp,udp,uds,pipe,os-util --release

# Check all targets using all features.
check_all_targets: $(TARGETS)
$(TARGETS):
	cargo hack check --target $@ --feature-powerset --skip guide,extra-docs,tcp,udp,uds,pipe,os-util

install_targets:
	rustup target add $(TARGETS)

# NOTE: when using this command you might want to change the `test` target to
# only run a subset of the tests you're actively working on.
dev:
	find src/ tests/ Makefile Cargo.toml | entr -d -c $(MAKE) test

clean:
	cargo clean

.PHONY: test test_all check_all_targets $(TARGETS) dev clean
