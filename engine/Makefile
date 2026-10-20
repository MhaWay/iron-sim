# CI-friendly entry points. Run from engine/.
SHELL := /bin/bash
CARGO = cargo

.PHONY: all build test fmt clippy lint check clean

all: check

build:
	source /root/.cargo/env && $(CARGO) build --workspace

test:
	source /root/.cargo/env && $(CARGO) test --workspace

fmt:
	source /root/.cargo/env && $(CARGO) fmt --all

clippy:
	source /root/.cargo/env && $(CARGO) clippy --workspace --all-targets -- -D warnings

lint:
	source /root/.cargo/env && $(CARGO) fmt --all -- --check
	source /root/.cargo/env && $(CARGO) clippy --workspace --all-targets -- -D warnings

check: fmt clippy test

clean:
	source /root/.cargo/env && $(CARGO) clean
