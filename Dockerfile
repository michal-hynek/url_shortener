# Stage 1: Build the Rust binary
FROM rust:1.96-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

# Stage 2: Runtime image
FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/url_shortener /app/url_shortener

# Bind strictly to localhost so only the Tailscale sidecar can reach it
ENV HOST=127.0.0.1
ENV PORT=3000

EXPOSE 3000
CMD ["./url_shortener"]