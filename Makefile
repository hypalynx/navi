.PHONY: run

run:
	cargo run -- $(ARGS)

debug:
	http_proxy=http://127.0.0.1:8080 cargo run -- $(ARGS)


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

listen: # to capture LLM traffic and debug
	mitmproxy --mode reverse:http://127.0.0.1:7777 \
		--listen-host 127.0.0.1 \
		--listen-port 8080
