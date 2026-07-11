FROM rust:1.94-alpine AS build

RUN apk add --no-cache bash~=5 make~=4 musl-dev

WORKDIR /app

COPY . ./
RUN make test
RUN cargo build --release

FROM alpine:3.21 AS release
RUN apk add --no-cache tzdata

COPY --from=build /app/target/release/skeleton-rust-api /usr/local/bin/skeleton-rust-api
ENTRYPOINT ["/usr/local/bin/skeleton-rust-api"]
