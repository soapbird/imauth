PROJECT   := imauth
REGISTRY  ?= docker.io/soapbird
IMAGE     := $(REGISTRY)/$(PROJECT)
CHROME_IMAGE ?= $(REGISTRY)/$(PROJECT)-chrome
VERSION   := $(shell grep '^version' crates/imauth-server/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
GIT_HASH  := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
PLATFORMS := linux/amd64,linux/arm64

.PHONY: all build test lint fmt check clean proto docker docker-chrome docker-push docker-chrome-push docker-buildx docker-chrome-buildx deploy deploy-chrome deploy-all up down

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
	cd crates/imauth-proto && cargo build

docker:
	docker build -t $(IMAGE):$(VERSION) -t $(IMAGE):latest -t $(IMAGE):$(GIT_HASH) .

docker-chrome:
	docker build -f Dockerfile.chrome -t $(CHROME_IMAGE):$(VERSION) -t $(CHROME_IMAGE):latest -t $(CHROME_IMAGE):$(GIT_HASH) .

docker-push:
	docker push $(IMAGE):$(VERSION)
	docker push $(IMAGE):latest
	docker push $(IMAGE):$(GIT_HASH)

docker-chrome-push:
	docker push $(CHROME_IMAGE):$(VERSION)
	docker push $(CHROME_IMAGE):latest
	docker push $(CHROME_IMAGE):$(GIT_HASH)

docker-buildx:
	docker buildx build \
		--platform $(PLATFORMS) \
		-t $(IMAGE):$(VERSION) \
		-t $(IMAGE):latest \
		-t $(IMAGE):$(GIT_HASH) \
		.

docker-chrome-buildx:
	docker buildx build \
		--platform $(PLATFORMS) \
		-f Dockerfile.chrome \
		-t $(CHROME_IMAGE):$(VERSION) \
		-t $(CHROME_IMAGE):latest \
		-t $(CHROME_IMAGE):$(GIT_HASH) \
		.

deploy:
	docker buildx build \
		--platform $(PLATFORMS) \
		--push \
		-t $(IMAGE):$(VERSION) \
		-t $(IMAGE):latest \
		-t $(IMAGE):$(GIT_HASH) \
		.

deploy-chrome:
	docker buildx build \
		--platform $(PLATFORMS) \
		--push \
		-f Dockerfile.chrome \
		-t $(CHROME_IMAGE):$(VERSION) \
		-t $(CHROME_IMAGE):latest \
		-t $(CHROME_IMAGE):$(GIT_HASH) \
		.

deploy-all: deploy deploy-chrome

up:
	docker compose up -d

down:
	docker compose down

run-server:
	cargo run --release -p imauth-server -- serve

run-cli-login:
	cargo run --release -p imauth-cli -- login --platform instagram

start-local:
	@./scripts/start-local.sh
