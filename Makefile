.PHONY: build release test fmt clippy lint check clean docker

build:
	cargo build --workspace

release:
	cargo build --release -p lw-cli

test:
	cargo test --workspace

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

lint: fmt-check clippy

check: lint test

clean:
	cargo clean

docker:
	docker build -t lw:latest .

docker-run:
	docker run --rm -v "$(PWD):/wiki" lw:latest --help

install:
	cargo install --path crates/lw-cli
