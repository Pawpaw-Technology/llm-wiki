FROM rust:1.92-slim AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release -p lw-cli \
    && strip target/release/lw

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/lw /usr/local/bin/lw

RUN useradd --create-home lw
USER lw
WORKDIR /home/lw

ENTRYPOINT ["lw"]
