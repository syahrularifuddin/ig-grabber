# ---- build stage: BoringSSL needs cmake; the toolchain never ships to prod ----
FROM rust:1-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release

# ---- runtime stage: slim debian, non-root, TLS roots for talking to Instagram ----
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/ig-backend /usr/local/bin/ig-backend
WORKDIR /app
ENV PORT=8080
EXPOSE 8080
USER 65534:65534
ENTRYPOINT ["/usr/local/bin/ig-backend"]
