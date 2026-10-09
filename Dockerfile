# Build Stage
FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# Runtime Stage
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
WORKDIR /app
COPY --from=builder /app/target/release/rudra /usr/local/bin/rudra
COPY --from=builder /app/target/release/rudra-proxy /usr/local/bin/rudra-proxy
EXPOSE 11434
ENV RUDRA_HOST=0.0.0.0
ENV RUDRA_PORT=11434
ENTRYPOINT ["rudra"]
CMD ["serve"]
