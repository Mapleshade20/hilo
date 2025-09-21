# ---- Stage 1: Build ----
FROM rust:1.89-trixie AS builder

WORKDIR /usr/src/app

ARG USE_MIRROR=""

RUN set -eux; \
    if [ -z "$USE_MIRROR" ]; then \
      country=$(curl -s https://ipapi.co/country/ || echo ""); \
      if [ "$country" = "CN" ]; then USE_MIRROR=1; else USE_MIRROR=0; fi; \
    fi; \
    echo "USE_MIRROR=$USE_MIRROR" > /mirror.env; \
    if [ "$USE_MIRROR" = "1" ]; then \
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
ENV SQLX_OFFLINE=true
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release --locked

# ---- Stage 2: Runtime ----

FROM debian:trixie-slim AS runtime

ARG USER_ID=1000
ARG GROUP_ID=1000
RUN groupadd --system --gid ${GROUP_ID} appuser && \
    useradd --system --create-home --uid ${USER_ID} --gid ${GROUP_ID} appuser

COPY --from=builder /mirror.env /mirror.env
RUN set -euxa && . /mirror.env && set +a; \
    if [ "$USE_MIRROR" = "1" ]; then \
      echo "Using mirror.tuna.tsinghua.edu.cn apt source"; \
      mkdir -p /etc/apt/; \
      printf '%s\n' \
        'deb http://mirrors.tuna.tsinghua.edu.cn/debian/ trixie main contrib non-free' \
        'deb http://mirrors.tuna.tsinghua.edu.cn/debian-security trixie-security main contrib non-free' \
        'deb http://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-updates main contrib non-free' \
        'deb http://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-backports main contrib non-free' \
        > /etc/apt/sources.list; \
      rm -rf /var/lib/apt/lists/*; \
      rm -rf /etc/apt/sources.list.d/*; \
    else \
      echo "Using default debian.org source"; \
    fi

RUN apt-get update -y && \
    fetch_deps='gcc libc-dev' && \
    apt-get install -y --no-install-recommends curl ca-certificates $fetch_deps && \
    curl -o /usr/local/bin/su-exec.c https://raw.githubusercontent.com/ncopa/su-exec/master/su-exec.c && \
    gcc -Wall /usr/local/bin/su-exec.c -o /usr/local/bin/su-exec && \
    chown root:root /usr/local/bin/su-exec && \
    chmod 0755 /usr/local/bin/su-exec && \
    rm /usr/local/bin/su-exec.c && \
    apt-get purge -y --auto-remove $fetch_deps && \
    apt-get autoremove -y && \
    apt-get clean -y && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/hilo /home/appuser/hilo
COPY *.json /home/appuser/
COPY scripts/entrypoint.sh /home/appuser/entrypoint.sh
RUN chmod +x /home/appuser/hilo /home/appuser/entrypoint.sh

WORKDIR /home/appuser

EXPOSE 8090 8091

ENTRYPOINT ["/home/appuser/entrypoint.sh"]
CMD ["./hilo"]
