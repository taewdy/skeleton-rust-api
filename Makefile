REPO_NAME := skeleton-rust-api
BINARY_NAME := $(REPO_NAME)
# Fetch the latest git tag.
GIT_TAG := $(shell git describe --tags `git rev-list --tags --max-count=1` 2>/dev/null)
# Use the latest git tag as the image tag. If no tag is found, use "latest".
IMAGE_TAG := $(if $(GIT_TAG),$(GIT_TAG),latest)

# Default target (since it's the first without '.' prefix)
build-all: coverage build
.PHONY: build-all

build:
	cargo build --release
.PHONY: build

test:
	cargo test
.PHONY: test

coverage:
	./script/coverage.sh
.PHONY: coverage

lint:
	cargo fmt --all --check
	cargo clippy --all-targets --all-features -- -D warnings
.PHONY: lint

docker-build:
	docker build --tag "$(BINARY_NAME):$(IMAGE_TAG)" .
.PHONY: docker-build
