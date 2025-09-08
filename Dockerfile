# ---- Stage 1: Build ----
FROM rust:1.89-trixie as builder

WORKDIR /usr/src/app

ARG USE_RSPROXY=""

RUN set -eux; \
    if [ -z "$USE_RSPROXY" ]; then \
      country=$(curl -s https://ipapi.co/country/ || echo ""); \
      if [ "$country" = "CN" ]; then USE_RSPROXY=1; fi; \
    fi; \
    if [ "$USE_RSPROXY" = "1" ]; then \
      echo "Using rsproxy.cn crates source"; \
      mkdir -p /usr/src/app/.cargo; \
      printf '%s\n' \
        '[source.crates-io]' \
        "replace-with = 'rsproxy-sparse'" \
        '[source.rsproxy]' \
        'registry = "https://rsproxy.cn/crates.io-index"' \
        '[source.rsproxy-sparse]' \
        'registry = "sparse+https://rsproxy.cn/index/"' \
        '[registries.rsproxy]' \
        'index = "https://rsproxy.cn/crates.io-index"' \
        '[net]' \
        'git-fetch-with-cli = true' \
        > /usr/src/app/.cargo/config.toml; \
    else \
      echo "Using default crates.io"; \
    fi

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked
RUN rm -rf src

COPY . .
ENV SQLX_OFFLINE true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# ---- Stage 2: Runtime ----

FROM debian:trixie-slim as runtime

ARG USER_ID=1000
ARG GROUP_ID=1000
RUN groupadd --system --gid ${GROUP_ID} appuser && \
    useradd --system --create-home --uid ${USER_ID} --gid ${GROUP_ID} appuser

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/hilo /home/appuser/hilo
COPY *.json /home/appuser/
COPY entrypoint.sh /home/appuser/entrypoint.sh
RUN chmod +x /home/appuser/hilo /home/appuser/entrypoint.sh

USER appuser

WORKDIR /home/appuser

EXPOSE 8090 8091

ENTRYPOINT ["./entrypoint.sh"]
CMD ["./hilo"]
