PYTHON ?= $(if $(wildcard .venv/bin/python),$(CURDIR)/.venv/bin/python,python3)
PROTO_ROOT := ../../proto

.PHONY: generate lint format format-check quality test

generate:
	$(PYTHON) -m grpc_tools.protoc \
		-I$(PROTO_ROOT) \
		--python_out=. \
		--grpc_python_out=. \
		$(PROTO_ROOT)/imauth/v1/*.proto
	$(PYTHON) -m ruff format --no-respect-gitignore imauth/v1/*.py

lint:
	$(PYTHON) -m ruff check imauth tests

format:
	$(PYTHON) -m ruff format --no-respect-gitignore imauth/v1/*.py
	$(PYTHON) -m ruff format imauth tests

format-check:
	$(PYTHON) -m ruff format --check --no-respect-gitignore imauth/v1/*.py
	$(PYTHON) -m ruff format --check imauth tests

quality: generate lint format-check

test:
	$(PYTHON) -m pytest tests/ -v
