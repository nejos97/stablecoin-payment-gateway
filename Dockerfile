FROM rust:bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release -p gateway

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 gateway

WORKDIR /app
COPY --from=builder /app/target/release/gateway /app/gateway

USER gateway

# Default only — overridden at runtime by `.env` / `-e PORT=...` (app also defaults to 3000).
ARG PORT=3000
ENV PORT=${PORT}
EXPOSE ${PORT}

CMD ["/app/gateway"]
