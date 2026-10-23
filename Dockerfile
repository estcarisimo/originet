# Build the CLI from source, then ship only the binary on a minimal Debian image.
# Usage:
#   docker build -t originet .
#   docker run --rm -v originet-cache:/cache originet 8.8.8.8
# Downloads are cached in /cache (ORIGINET_CACHE_DIR); mount /data for input and output files.

FROM rust:1.91-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY python/Cargo.toml ./python/Cargo.toml
COPY python/src ./python/src
RUN cargo build --release --locked --bin originet

FROM debian:bookworm-slim
ARG GIT_COMMIT=unknown
LABEL org.opencontainers.image.title="originet" \
      org.opencontainers.image.description="Map IP addresses to origin ASNs with CAIDA's RouteViews prefix-to-AS snapshots" \
      org.opencontainers.image.source="https://github.com/estcarisimo/originet" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.revision="${GIT_COMMIT}"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --create-home app \
    && mkdir -p /cache /data \
    && chown app:app /cache /data
COPY --from=build /src/target/release/originet /usr/local/bin/originet
USER app
WORKDIR /data
ENV ORIGINET_CACHE_DIR=/cache
VOLUME ["/cache", "/data"]
ENTRYPOINT ["originet"]
CMD ["--help"]
