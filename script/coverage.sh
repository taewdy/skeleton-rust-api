#!/usr/bin/env bash
set -euo pipefail

# Fails when line coverage drops below the expected percentage.
# The binary entry point and the CLI wiring are excluded, matching the Go
# skeleton's exclusion of ./cmd (mockall mocks only exist inside #[cfg(test)]
# and are never counted).
#
# Requires cargo-llvm-cov: cargo install cargo-llvm-cov

EXPECTED_COVERAGE=${EXPECTED_COVERAGE:-80}

cargo llvm-cov \
  --ignore-filename-regex '(src/main\.rs|src/commands/)' \
  --fail-under-lines "$EXPECTED_COVERAGE" \
  --summary-only

echo "SUCCESS: Coverage meets the minimum expected ${EXPECTED_COVERAGE}%"
