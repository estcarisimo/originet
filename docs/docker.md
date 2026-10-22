# Docker

The image builds the CLI from source in a `rust` stage and ships only the binary on
Debian slim with `ca-certificates`, as a non-root user. Downloads persist in the `/cache`
volume (`ORIGINET_CACHE_DIR`); `/data` is the working directory for input and output
files.

```bash
docker build -t originet .
docker run --rm -v originet-cache:/cache originet 8.8.8.8 2001:4860::8888
docker run --rm -v originet-cache:/cache -v "$PWD/data:/data" originet \
  --file /data/ips.txt --format csv --output /data/results.csv
```

With Compose (`./data` on the host is `/data` in the container):

```bash
docker compose run --rm originet --file /data/ips.txt --date 2024-01-01 -F jsonl
```

Tagged releases are also published as `ghcr.io/estcarisimo/originet:<version>`
(linux/amd64).
