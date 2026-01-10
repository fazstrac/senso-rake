# --- Builder stage ---------------------------------------------------------
FROM rust:1.92-trixie AS builder

ARG BUILD_DATE
LABEL org.opencontainers.image.created=$BUILD_DATE

# Install common system build dependencies (adjust if your deps require others)
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates curl wget && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/app

# Copy manifest first and build a dummy binary to populate the cargo registry/cache
# This speeds up rebuilds when only your application code changes.
COPY Cargo.toml Cargo.lock ./

# Create a dummy main to allow dependency-only build
RUN mkdir -p src && echo 'fn main() { println!("placeholder"); }' > src/main.rs
RUN cargo build --release || true

# Replace with real source and build. Update the timestamp of main.rs to ensure recompilation.
WORKDIR /usr/src/app
COPY . .
RUN touch src/main.rs && cargo build --release

# --- Runtime stage ---------------------------------------------------------
FROM debian:trixie-slim

# Create an unprivileged user to run the service
RUN useradd -m -u 1000 appuser || true

# Copy the compiled binary from the builder stage
# Adjust the binary name if your crate outputs a different filename.
COPY --from=builder /usr/src/app/target/release/senso-rake /usr/local/bin/senso-rake

USER appuser
WORKDIR /data

# Minimal environment defaults; override with `--env` or `--env-file` at runtime
ENV RUST_LOG=info

CMD ["/usr/local/bin/senso-rake"]
