# Build stage
FROM rust:1.75 as builder

WORKDIR /certus

# Copy manifests
COPY Cargo.toml .
COPY common/Cargo.toml common/
COPY executor/Cargo.toml executor/
COPY verifier/Cargo.toml verifier/

# Build dependencies
RUN mkdir -p common/src executor/src verifier/src && \
    echo "fn main() {}" > executor/src/main.rs && \
    echo "fn main() {}" > verifier/src/main.rs && \
    echo "" > common/src/lib.rs && \
    cargo build --release && \
    rm -rf common/src executor/src verifier/src target/release/deps/certus*

# Copy source
COPY . .

# Build release
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /certus/target/release/executor /usr/local/bin/
COPY --from=builder /certus/target/release/verifier /usr/local/bin/

CMD ["executor"]