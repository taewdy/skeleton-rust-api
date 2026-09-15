# skeleton-rust-api

This is my take of a skeleton project for a Rust API application. It is intended to be used as a starting point for new API applications, and mirrors the structure of `skeleton-go-api`.

## Usage

To use this project as a starting point for a new application, follow these steps:
1. Clone this repository.
2. Rename the `skeleton-rust-api` directory to the name of your new application.
3. Update the `name` in `Cargo.toml` to reflect the new application name.
4. Replace `skeleton-rust-api` with the new application name in all files.
5. Implement the desired functionality for the new application.

You can basically replace `skeleton-rust-api` with your new application name and start building your application.

## What's Included

- A basic API application structure (axum + tokio).
- A simple CLI (clap) with a subcommand.
- Layered configuration (defaults < YAML < environment < flags) via the `config` crate.
- Structured logging via `tracing`, with a runtime-changeable log level.
- A Postgres connection pool (sqlx) with pool settings and a ping-on-startup check.
- A `Makefile` with common tasks.
- A `Dockerfile` for building a Docker image.
- A GitHub Actions workflow for versioning.
- A GitHub Actions workflow for PR checks: linting (rustfmt + clippy), test coverage, and build.
- A coverage gate (`script/coverage.sh`, requires [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov)).

## Go → Rust mapping

| Concern | skeleton-go-api | skeleton-rust-api |
|---|---|---|
| CLI | cobra | clap (derive) |
| Config | viper (flags/env/yaml) | `config` crate + clap overrides |
| Logging | zap | `tracing` / `tracing-subscriber` |
| HTTP server | gin | axum (tokio team's framework) |
| HTTP client | net/http wrapper | reqwest wrapper (`src/client.rs`) |
| DB pool | sqlx (Go) + lib/pq | sqlx (Rust) `PgPool` |
| Mocks | mockery | mockall (`#[automock]`, test-only) |
| Interfaces at the consumer | implicit interfaces | traits declared in the consuming module |
| Goroutines + channels | `go` + `chan` | `tokio::spawn` + `mpsc` |
| Lint | golangci-lint | clippy (pedantic) + rustfmt |
| Coverage gate | `go tool cover` script | `cargo llvm-cov --fail-under-lines` |

Layout: `src/main.rs` is the entry point (Go's `cmd/`), `src/commands/` is the CLI wiring, and the flat modules `config`, `logger`, `server`, `db`, `client`, `photos`, `api` mirror `internal/*`.

## Sample Service, Get Photos from jsonplaceholder.typicode.com

This project includes a sample service that fetches photos from jsonplaceholder.typicode.com. The service is implemented in the `photos` module and exposed through the `/photos/{id}` endpoint.

The server pings Postgres on startup (full parity with the Go skeleton), so it needs a database to boot:

```bash
docker run --rm -d --name skeleton-db -e POSTGRES_USER=db -e POSTGRES_PASSWORD=db -e POSTGRES_DB=db -p 5432:5432 postgres:16-alpine

make build
./target/release/skeleton-rust-api
```

Now run `curl http://localhost:8080/photos/1` and it will return

```json
{"albumId":1,"id":1,"title":"accusamus beatae ad facilis cum similique qui sunt","url":"https://via.placeholder.com/600/92c952","thumbnailUrl":"https://via.placeholder.com/150/92c952"}
```

## Rust Implementation Guidelines

### TL;DR: Enhance flexibility and maintainability by:
- Testability: Use dependency injection through traits and constructor functions for easier mocking and isolated testing. Aim for test coverage above 80%.
- Adhering to SOLID Principles: Extend functionalities without altering existing code to preserve functionality and ensure stability.
- Embracing DRY: Centralize logic to avoid duplications and inconsistencies.
- Separating Concerns: Keep modules focused on their intended functionality to simplify maintenance and reduce conflicts.
- Maintaining Code Hygiene: Standardize coding practices (rustfmt, clippy pedantic), keep functions concise, and reduce conditional complexity.

### Project layout and where new code goes
- `src/main.rs` is the entry point (Go's `cmd/`), `src/commands/` is the CLI wiring, and the flat modules `config`, `logger`, `server`, `db`, `client`, `photos`, `api` mirror Go's `internal/*`. A new concern is a new flat module in `src/`, declared in `lib.rs`.
- Handlers go in `api` (one router factory per resource), business logic in a service module (`photos`), I/O clients in their own module (`client`, `db`). Nothing else creates `Router`s or opens connections.
- Migrations and schema changes belong to the deploying application; this skeleton only pings the pool on startup.

### Testability through Dependency Injection
- Services accept their dependencies in `new()` constructors, generic over traits (`Service<C: HttpGet>`), so tests inject `mockall` mocks with zero runtime cost.
- Define traits at the consumer side, not at the provider side: `photos::HttpGet` abstracts the HTTP client for the photos service; `api::PhotosGetter` abstracts the photos service for the handlers. The concrete types implement those traits next to the trait definition, which keeps providers unaware of their consumers (Rust's explicit-impl version of Go's implicit interfaces).
- Handlers get their dependencies baked in via router factories (`api::photos_router`), the equivalent of Go's closure-returning handler factories.
- The server exposes `router()` for in-process tests (`tower::ServiceExt::oneshot`) and `start_with_shutdown` so tests control the shutdown future instead of OS signals.
- "Accept interfaces, return structs" becomes "accept `impl Trait`/generics, return concrete types".
- Mocks are generated with `#[cfg_attr(test, mockall::automock)]` on the trait, so they exist only in test builds.
- Coverage is enforced at 80% by `make coverage` (`cargo-llvm-cov`).

### Preserving Existing Functionality (Open/Closed Principle)
- New endpoints are added as new router factories merged into the server (`Vec<Router>`), without modifying existing handlers or the server module.
- New commands are added as new variants of the `Command` enum with their own module, without touching existing commands.

### DRY
- Response buffering, status checking, and JSON decoding live in one place each (`client`, `photos`).
- Errors are typed per module with `thiserror` and converted with `#[from]` / `#[error(transparent)]`, so error context is added once, where it originates. `anyhow` is used only in the binary (`main.rs`) to render the final error; library modules return typed errors.
- Configuration is parsed once (`config`) and passed down; modules never read the environment themselves.

### Separation of Concerns
- Each module does only what its name says: `config` loads config, `logger` sets up tracing, `server` owns routing/middleware/shutdown, `api` maps HTTP to services, `photos` owns business logic, `client` talks HTTP, `db` owns the pool.
- Handlers never contain business logic; services never build HTTP responses.

### Errors
- One `Error` enum per module, `#[derive(Debug, thiserror::Error)]`, every variant documented.
- Wrap third-party errors with context at the boundary where they occur; internal details are logged, never sent to clients or printed as command output.
- Mappings to HTTP status codes / exit codes are exhaustive (`match` without a wildcard arm) so adding a variant fails to compile until it is mapped.

### Logging
- `tracing` macros everywhere; `tracing-subscriber` is initialised once in `main` from the configured level. Log at the point of failure with context, then return the typed error — never both log and swallow.

### Configuration
- Layered: defaults < `config.yaml` < environment variables < CLI flags, via the `config` crate and clap. Parsing is a pure function tested with the files in `testdata/`. Secrets come from the environment, never from the checked-in YAML.

### Code hygiene
- `cargo fmt` and `cargo clippy` with the `pedantic` group run in `make lint` and CI with warnings denied.
- Every public item has a doc comment (`missing_docs = "warn"`); every module starts with a `//!` header explaining its role.
- Keep functions/methods small — if a function grows past a screenful (~30 lines) or contains distinctly different logic, split it.
- Minimize branching; prefer early returns (`let ... else`), `?`, and exhaustive `match` over nested `if` chains.
- Pin the toolchain in `rust-toolchain.toml` (compiler and components); CI uses the same file, so local and CI results match.

### Pull request checklist
- `make lint` and `make coverage` pass locally.
- New logic has unit tests; anything that talks to the network or a database is behind a trait with a mock.
- Config keys added to `config.yaml`, `testdata/`, and documented in this README.
- PR title starts with `major:`, `minor:` or `patch:` so the semver workflow tags the merge.
