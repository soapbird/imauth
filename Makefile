PROJECT   := imauth
REGISTRY  ?= docker.io/soapbird
IMAGE     := $(REGISTRY)/$(PROJECT)
CHROME_IMAGE       ?= imyounjs/chrome
CHROME_PROXY_IMAGE ?= imyounjs/chrome-proxy
VERSION   := $(shell grep '^version' crates/imauth-server/Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
GIT_HASH  := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
PLATFORMS := linux/amd64,linux/arm64
BUILDER   ?= imauth-builder

.PHONY: all build test lint fmt check clean proto buildx-setup docker docker-chrome docker-chrome-proxy docker-push docker-chrome-push docker-chrome-proxy-push docker-buildx docker-chrome-buildx docker-chrome-proxy-buildx deploy deploy-chrome deploy\:chrome deploy-chrome-proxy deploy\:chrome-proxy deploy-chrome-amd64 deploy-chrome-arm64 deploy-chrome-fast deploy-all up down

# Create (once) a docker-container builder so multi-platform builds can
# export/import a registry layer cache. The default `docker` driver cannot,
# which is why every build re-pulls the multi-GB chromium base image.
buildx-setup:
	@docker buildx inspect $(BUILDER) >/dev/null 2>&1 || \
		docker buildx create --name $(BUILDER) --driver docker-container --bootstrap

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

docker-chrome-proxy:
	docker build -f Dockerfile.chrome-proxy -t $(CHROME_PROXY_IMAGE):$(VERSION) -t $(CHROME_PROXY_IMAGE):latest -t $(CHROME_PROXY_IMAGE):$(GIT_HASH) .

docker-push:
	docker push $(IMAGE):$(VERSION)
	docker push $(IMAGE):latest
	docker push $(IMAGE):$(GIT_HASH)

docker-chrome-push:
	docker push $(CHROME_IMAGE):$(VERSION)
	docker push $(CHROME_IMAGE):latest
	docker push $(CHROME_IMAGE):$(GIT_HASH)

docker-chrome-proxy-push:
	docker push $(CHROME_PROXY_IMAGE):$(VERSION)
	docker push $(CHROME_PROXY_IMAGE):latest
	docker push $(CHROME_PROXY_IMAGE):$(GIT_HASH)

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

docker-chrome-proxy-buildx:
	docker buildx build \
		--platform $(PLATFORMS) \
		-f Dockerfile.chrome-proxy \
		-t $(CHROME_PROXY_IMAGE):$(VERSION) \
		-t $(CHROME_PROXY_IMAGE):latest \
		-t $(CHROME_PROXY_IMAGE):$(GIT_HASH) \
		.

deploy: buildx-setup
	docker buildx build \
		--builder $(BUILDER) \
		--platform $(PLATFORMS) \
		--provenance=false \
		--cache-from type=registry,ref=$(IMAGE):buildcache \
		--cache-to type=registry,ref=$(IMAGE):buildcache,mode=max \
		--push \
		-t $(IMAGE):$(VERSION) \
		-t $(IMAGE):latest \
		-t $(IMAGE):$(GIT_HASH) \
		.

deploy-chrome: buildx-setup
	docker buildx build \
		--builder $(BUILDER) \
		--platform $(PLATFORMS) \
		--provenance=false \
		--cache-from type=registry,ref=$(CHROME_IMAGE):buildcache \
		--cache-to type=registry,ref=$(CHROME_IMAGE):buildcache,mode=max \
		--push \
		-f Dockerfile.chrome \
		-t $(CHROME_IMAGE):$(VERSION) \
		-t $(CHROME_IMAGE):latest \
		-t $(CHROME_IMAGE):$(GIT_HASH) \
		.

deploy\:chrome: deploy-chrome

deploy-chrome-amd64:
	docker buildx build \
		--platform linux/amd64 \
		--provenance=false \
		--push \
		-f Dockerfile.chrome \
		-t $(CHROME_IMAGE):$(VERSION)-amd64 \
		.

deploy-chrome-arm64:
	docker buildx build \
		--platform linux/arm64 \
		--provenance=false \
		--push \
		-f Dockerfile.chrome \
		-t $(CHROME_IMAGE):$(VERSION)-arm64 \
		.

deploy-chrome-fast: deploy-chrome-amd64 deploy-chrome-arm64
	docker buildx imagetools create \
		-t $(CHROME_IMAGE):$(VERSION) \
		-t $(CHROME_IMAGE):latest \
		-t $(CHROME_IMAGE):$(GIT_HASH) \
		$(CHROME_IMAGE):$(VERSION)-amd64 \
		$(CHROME_IMAGE):$(VERSION)-arm64

deploy-chrome-proxy: buildx-setup
	docker buildx build \
		--builder $(BUILDER) \
		--platform $(PLATFORMS) \
		--provenance=false \
		--cache-from type=registry,ref=$(CHROME_PROXY_IMAGE):buildcache \
		--cache-to type=registry,ref=$(CHROME_PROXY_IMAGE):buildcache,mode=max \
		--push \
		-f Dockerfile.chrome-proxy \
		-t $(CHROME_PROXY_IMAGE):$(VERSION) \
		-t $(CHROME_PROXY_IMAGE):latest \
		-t $(CHROME_PROXY_IMAGE):$(GIT_HASH) \
		.

deploy\:chrome-proxy: deploy-chrome-proxy

deploy-all: deploy deploy-chrome deploy-chrome-proxy

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
