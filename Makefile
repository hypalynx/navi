.PHONY: run

run:
	cargo run -- --debug

test: lint
	cargo test

lint:
	cargo fmt --check
	cargo clippy

lint-fix:
	cargo fmt
	cargo clippy --fix --allow-dirty

install:
	cargo install --path .
