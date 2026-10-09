# Build Stage
FROM rust:1.85-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime Stage
FROM alpine:3.20
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/rudra-proxy /usr/local/bin/rudra-proxy
EXPOSE 11434
ENV RUDRA_HOST=0.0.0.0
ENV RUDRA_PORT=11434
ENTRYPOINT ["rudra-proxy", "serve"]
