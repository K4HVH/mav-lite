ARG TARGETARCH

FROM rust:1.83-slim AS builder
WORKDIR /app
ARG TARGETARCH

RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev git && \
    rm -rf /var/lib/apt/lists/*

RUN if [ "$TARGETARCH" = "arm64" ]; then \
    dpkg --add-architecture arm64 && \
    apt-get update && \
    apt-get install -y gcc-aarch64-linux-gnu libc6-dev-arm64-cross && \
    rustup target add aarch64-unknown-linux-gnu && \
    rm -rf /var/lib/apt/lists/*; \
    fi

COPY . .

RUN if [ "$TARGETARCH" = "arm64" ]; then \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    cargo build --release --target aarch64-unknown-linux-gnu && \
    cp target/aarch64-unknown-linux-gnu/release/mav-lite /app/mav-lite; \
    else \
    cargo build --release && \
    cp target/release/mav-lite /app/mav-lite; \
    fi

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/mav-lite /usr/local/bin/mav-lite
COPY config.toml /app/config.toml

RUN useradd -m -u 1000 mavlite && \
    chown -R mavlite:mavlite /app

USER mavlite

EXPOSE 5761/tcp

CMD ["mav-lite", "/app/config.toml"]
