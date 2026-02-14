FROM rust:1.93-slim-trixie as builder

RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  libpq-dev \
  g++ \
  build-essential \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY .sqlx ./.sqlx

COPY src ./src
COPY migrations ./migrations
COPY models ./models

# Enable offline mode
ENV SQLX_OFFLINE=true

# Build the release binary
RUN cargo build --release

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y \
  ca-certificates \
  libssl3t64 \
  libpq5 \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/api /usr/local/bin/api
COPY --from=builder /app/migrations ./migrations
COPY --from=builder /app/models ./models 

# Render standard envs
ENV RUST_LOG=info
ENV HOST=0.0.0.0
ENV PORT=3000
EXPOSE 3000

CMD ["/usr/local/bin/api"]
