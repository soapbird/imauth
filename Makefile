.PHONY: all build test lint fmt check clean proto docker

all: build

build:
	cargo build --release -p imauth-server -p imauth-cli

test:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all

check:
	cargo check --workspace

clean:
	cargo clean

proto:
	cd crates/imauth-server && cargo build

docker:
	docker build -t imauth:latest .

run-server:
	cargo run --release -p imauth-server -- serve

run-cli-login:
	cargo run --release -p imauth-cli -- login --platform instagram --username user --password pass
