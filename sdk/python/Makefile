PYTHON ?= $(if $(wildcard .venv/bin/python),$(CURDIR)/.venv/bin/python,python3)
PROTO_ROOT := ../../proto
PROTOBUF_GENCODE_VERSION := 6.31.1
GRPCIO_GENCODE_VERSION := 1.76.0

.PHONY: generate verify-generated-runtime lint format format-check quality test typecheck

generate:
	$(PYTHON) -m grpc_tools.protoc \
		-I$(PROTO_ROOT) \
		--python_out=. \
		--pyi_out=. \
		--grpc_python_out=. \
		$(PROTO_ROOT)/imauth/v1/*.proto
	$(PYTHON) -m ruff format --no-respect-gitignore imauth/v1/*.py

verify-generated-runtime:
	@for generated in imauth/v1/*_pb2.py; do \
		if ! grep -q "Protobuf Python Version: $(PROTOBUF_GENCODE_VERSION)" "$$generated"; then \
			echo "$$generated: expected protobuf gencode $(PROTOBUF_GENCODE_VERSION)" >&2; \
			exit 1; \
		fi; \
	done
	@for generated in imauth/v1/*_pb2_grpc.py; do \
		if ! grep -q "GRPC_GENERATED_VERSION = \"$(GRPCIO_GENCODE_VERSION)\"" "$$generated"; then \
			echo "$$generated: expected gRPC gencode $(GRPCIO_GENCODE_VERSION)" >&2; \
			exit 1; \
		fi; \
	done

lint:
	$(PYTHON) -m ruff check imauth tests

format:
	$(PYTHON) -m ruff format --no-respect-gitignore imauth/v1/*.py
	$(PYTHON) -m ruff format imauth tests

format-check:
	$(PYTHON) -m ruff format --check --no-respect-gitignore imauth/v1/*.py
	$(PYTHON) -m ruff format --check imauth tests

typecheck:
	$(PYTHON) -m basedpyright --level error imauth tests

quality: generate verify-generated-runtime lint format-check typecheck

test:
	$(PYTHON) -m pytest tests/ -v
