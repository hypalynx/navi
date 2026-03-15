.PHONY: run

run:
	cargo run -- $(ARGS)

debug:
	cargo run -- --port 8080 $(ARGS)


test: lint
	cargo test

lint:
	cargo fmt --check
	cargo clippy

lint-fix:
	cargo fmt
	cargo clippy --fix --allow-dirty

coverage:
	cargo llvm-cov --json --summary-only --output-path coverage.json && \
	cat coverage.json | jq '.data[0].files | map(select(.filename | contains("src/navi/src"))) | sort_by(.filename) | .[] | "\(.filename | split("/") | .[-1]): \(.summary.lines.percent | round)% lines, \(.summary.functions.percent | round)% functions"' -r && \
	echo "---" && \
	cat coverage.json | jq '.data[0].totals | "Total: \(.lines.percent | round)% lines, \(.functions.percent | round)% functions"' -r

install:
	cargo install --path .

listen: # to capture LLM traffic and debug
	mitmproxy --mode reverse:http://127.0.0.1:7777 \
		--listen-host 127.0.0.1 \
		--listen-port 8080
