FROM rust:1.99.1-slim-trixie AS builder

RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  curl \
  ca-certificates \
  g++ \
  build-essential \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

# Enable offline mode
ENV SQLX_OFFLINE=true

# Cache build artifacts without including them in an image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin api --example onnx_smoke \
    && install -D target/release/api /out/api \
    && install -D target/release/examples/onnx_smoke /out/onnx-smoke \
    && bash scripts/install-onnx-runtime.sh /opt/onnxruntime

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
  ca-certificates \
  libssl3t64 \
  libstdc++6 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /out/ /usr/local/bin/
COPY --from=builder /opt/onnxruntime /opt/onnxruntime
COPY --from=builder /app/migrations ./migrations
COPY --from=builder /app/models ./models 

# Render standard envs
ENV RUST_LOG=info
ENV HOST=0.0.0.0
ENV PORT=3000
ENV ORT_DYLIB_PATH=/opt/onnxruntime/lib/libonnxruntime.so.1.23.2
EXPOSE 3000

CMD ["/usr/local/bin/api"]
