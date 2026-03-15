.PHONY: build test lint fmt check

build:
	cargo build

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt --check

check: fmt lint test
