# 🌐 originet

Map IP addresses to the Autonomous System that originates them, using CAIDA's daily
RouteViews prefix-to-AS snapshots. (Formerly `map-ip-to-asn`, a Python tool on pyipmeta.) One binary, no C library: it picks the snapshot for the day
you ask for, downloads it once, builds a longest-prefix-match trie and answers a million
addresses in about two seconds. A Python package exposes the same table for bulk lookups.

[![CI](https://github.com/estcarisimo/originet/actions/workflows/ci.yml/badge.svg)](https://github.com/estcarisimo/originet/actions/workflows/ci.yml)
[![Security](https://github.com/estcarisimo/originet/actions/workflows/security.yml/badge.svg)](https://github.com/estcarisimo/originet/actions/workflows/security.yml)
[![CodeQL](https://github.com/estcarisimo/originet/actions/workflows/codeql.yml/badge.svg)](https://github.com/estcarisimo/originet/actions/workflows/codeql.yml)
[![Docs](https://img.shields.io/badge/docs-estcarisimo.github.io-blue)](https://estcarisimo.github.io/originet/)
[![crates.io](https://img.shields.io/crates/v/originet.svg)](https://crates.io/crates/originet)
[![docs.rs](https://img.shields.io/docsrs/originet)](https://docs.rs/originet)
![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)
![Python](https://img.shields.io/badge/python-3.10%2B-blue.svg)
![License](https://img.shields.io/badge/license-MIT-green.svg)

[llms.txt](https://estcarisimo.github.io/originet/llms.txt) is the documentation index for AI agents.

## ✨ Features

- 🔍 **Longest-prefix match**: every answer carries the most specific prefix that covers the address, its origin ASN and, for multi-origin prefixes and AS sets, all origins in CAIDA's notation
- 📅 **Any day since 2005**: `--date` resolves the newest snapshot on or before that day through CAIDA's index and monthly listings; the default is the newest snapshot
- 🌍 **IPv4 and IPv6 in one run**: each family's snapshot is loaded only when the input needs it
- 📦 **Batch by default**: addresses from arguments, a file or standard input; a colored table on the terminal (`--color never` for plain), JSON, JSON Lines, CSV, TSV or Parquet out
- 💾 **Cached and offline-capable**: snapshots are downloaded once (atomically, with a progress bar) into a cache; `--fetch` warms it, `--cache` lists and prunes it, `--offline` never touches the network; `--pfx2as` uses your own file
- 🐍 **Python bindings**: `Table.from_caida()` / `Table.from_files()` and `lookup_many()` with the GIL released
- 🦀 **Single binary, no C stack**: no libipmeta, no pyipmeta, no scraping; `unsafe_code = "forbid"`
- 🐳 **Docker image**: multi-stage build, non-root, cache volume

## 🚀 Quick Start

### CLI

From [crates.io](https://crates.io/crates/originet), with a Rust toolchain (1.88 or newer):

```bash
cargo install originet --locked
originet 8.8.8.8 2001:4860::8888
```

The same from the repository's `main` instead of the latest release:

```bash
cargo install --git https://github.com/estcarisimo/originet --locked
```

Or from source:

```bash
git clone https://github.com/estcarisimo/originet.git && cd originet
cargo build --release --locked          # target/release/originet
```

Prebuilt binaries for Linux (x86_64, aarch64, static musl), macOS (Intel, Apple Silicon)
and Windows are attached to every
[tagged release](https://github.com/estcarisimo/originet/releases), with SHA-256 checksums.

### Python

The package is not on PyPI yet; install it from the repository (builds the extension,
so it needs Rust):

```bash
uv pip install "originet @ git+https://github.com/estcarisimo/originet#subdirectory=python"
```

### Docker

```bash
docker build -t originet .
docker run --rm -v originet-cache:/cache originet 8.8.8.8
```

## 📖 Usage

```bash
# One or more addresses: a table on the terminal (the JSON report when piped), summary on stderr
originet 8.8.8.8 1.1.1.1 2606:4700::1111
originet --format json 8.8.8.8

# A file (one address per line, # comments allowed) to CSV
originet --file ips.txt --format csv --output results.csv

# Standard input, JSON Lines out: the shape for pipelines
cat ips.txt | originet -f - -F jsonl > results.jsonl

# The routing table of a given day (newest snapshot on or before it)
originet --date 2015-01-01 --file ips.txt -F parquet -o ips-2015.parquet

# Your own snapshot files (plain or gzip), no download
originet --pfx2as routeviews-rv2-20261021-0800.pfx2as.gz --pfx2as6 routeviews-rv6-20261021-1200.pfx2as.gz -f ips.txt

# Never touch the network; fail unless everything needed is cached
originet --offline --date 2024-02-29 8.8.8.8

# Fail (exit 1) if any input is not an IP address; show what is fetched
originet --strict -v -f ips.txt
```

Every output format has the same six columns: `ip`, `prefix`, `asn`, `origins`,
`snapshot`, `error`. `asn` is the first ASN of `origins`; `origins` is `15169`,
`149511_138421` (several ASes announce the prefix) or `64496,64497` (an AS set). An
address no prefix covers gets null `prefix` and `asn`; an input that is not an address
gets an `error`. The JSON report also lists the snapshot files used, their source and
how many prefixes each had. Details: [output formats](https://estcarisimo.github.io/originet/output/),
[the data](https://estcarisimo.github.io/originet/data/).

### 🐍 Python

```python
from originet import Table

table = Table.from_caida("2024-01-01")              # newest snapshot on or before that day, cached
table = Table.from_files(ipv4="routeviews-rv2-20261021-0800.pfx2as.gz")   # or your own file

table.lookup("8.8.8.8")
# Match(ip='8.8.8.8', prefix='8.8.8.0/24', asn=15169, origins='15169')

table.lookup_many(["1.1.1.1", "10.0.0.1", "2001:4860::8888"])   # GIL released
# [Match(ip='1.1.1.1', prefix='1.1.1.0/24', asn=13335, origins='13335'), None, Match(...)]

table.lookup_asns(["1.1.1.1", "bogus"], strict=False)            # just the ASNs
# [13335, None]

table.snapshots      # [{'family': 'ipv4', 'source': 'caida', 'date': '2024-01-01', ...}]
```

## 🔧 Configuration

| Setting | Effect |
| --- | --- |
| `--cache-dir DIR` / `ORIGINET_CACHE_DIR` | Where indexes, listings and snapshots are kept. Default: the platform cache directory (`~/.cache/originet` on Linux, `~/Library/Caches/originet` on macOS, `/cache` in the image) |
| `--offline` | Never download; error if the index, listing or snapshot is not cached |
| `--refresh` | Fetch the snapshot index again even if the cached copy is less than six hours old |
| `--cache list\|size\|prune` | Show what the cache holds and its size, or prune it: `--keep-days N`, `--keep-last N`, `--dry-run` |
| `-v`, `-vv`, `RUST_LOG` | Log what is fetched or read from the cache |

The index and the current month's listing are refreshed after six hours; snapshot files
and past months never change and are cached forever.

## 🏗️ Architecture

```text
src/
├── main.rs      # CLI (clap): arguments, orchestration, stderr summary, exit codes
├── pfx2as.rs    # Parser of CAIDA's pfx2as format; Origins (A, A_B, A,B); gzip by magic bytes
├── table.rs     # Longest-prefix-match table (one prefix-trie per family)
├── caida.rs     # Dataset layout, rolling index and monthly listings, date selection
├── fetch.rs     # Cached, atomic downloads; index freshness; offline mode
├── cache.rs     # Inventory of the cache; pruning by age or count
├── load.rs      # Local file or CAIDA snapshot into a table, with provenance
├── input.rs     # Address parsing (/32, /128, IPv4-mapped IPv6, comments)
└── output.rs    # Records, summary, report; JSON, JSON Lines, CSV, TSV, Parquet
python/
├── src/lib.rs            # PyO3 module originet._native: Table, Match
├── originet/        # Package, typed stub, py.typed
└── tests/                # pytest suite
```

The crate is also a library (`originet`); see
[Rust library](https://estcarisimo.github.io/originet/library/).

## 🧪 Development

```bash
cargo build && cargo test --workspace --all-features
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check                                  # cargo install cargo-deny
cd python && uv sync --locked && uv run pytest -q && uv run ruff check . && uv run mypy
uvx --with-requirements docs/requirements.txt mkdocs build --strict
```

Tests never touch the network: they run against `tests/fixtures/` and a seeded cache.
`CONTRIBUTING.md` has the full checklist and the release procedure.

## 📊 Example Output

```text
$ originet -F csv 8.8.8.8 1.1.1.1 1.1.8.9 10.0.0.1
ip,prefix,asn,origins,snapshot,error
8.8.8.8,8.8.8.0/24,15169,15169,2026-10-21,
1.1.1.1,1.1.1.0/24,13335,13335,2026-10-21,
1.1.8.9,1.1.8.0/24,149511,149511_138421,2026-10-21,
10.0.0.1,,,,2026-10-21,
4 addresses: 3 found, 1 not found, 0 invalid (ipv4 2026-10-21; 407.8ms)

$ originet --date 2005-06-01 -F csv 8.8.8.8
ip,prefix,asn,origins,snapshot,error
8.8.8.8,8.0.0.0/8,3356,3356,2005-06-01,
1 addresses: 1 found, 0 not found, 0 invalid (ipv4 2005-06-01; 2.0s)

$ originet -f ips-1M.txt -F jsonl -o out.jsonl
1000000 addresses: 836003 found, 163997 not found, 0 invalid (ipv4 2026-10-21; 1.7s)
```

The first run for a day downloads the snapshot (about 4 MB for IPv4, 1 MB for IPv6);
after that the time is loading 1.1 M prefixes into the trie plus the lookups. The 1 M
figure is one million random IPv4 addresses on a 2023 laptop, snapshot already cached.

## 🤝 Contributing

Contributions are welcome. Read [CONTRIBUTING.md](https://github.com/estcarisimo/originet/blob/main/CONTRIBUTING.md) first, then:

1. Fork the repository
2. Create a branch (`git checkout -b feat/my-change`)
3. Commit with tests and a `CHANGELOG.md` entry
4. Push the branch
5. Open a pull request (every PR is reviewed per `.github/REVIEW.md`)

## 📄 License

MIT, see [LICENSE](https://github.com/estcarisimo/originet/blob/main/LICENSE). The data is CAIDA's and keeps
[its own terms](https://www.caida.org/catalog/datasets/routeviews-prefix2as/).

## 🔗 Related Resources

- [Documentation site](https://estcarisimo.github.io/originet/)
- [llms.txt](https://estcarisimo.github.io/originet/llms.txt) for AI agents ([full text](https://estcarisimo.github.io/originet/llms-full.txt))
- [CAIDA Routeviews Prefix to AS mappings Dataset (pfx2as)](https://www.caida.org/catalog/datasets/routeviews-prefix2as/)
- [RouteViews](https://www.routeviews.org/)
- [CAIDA libipmeta](https://github.com/CAIDA/libipmeta), the C library the 0.x Python version wrapped

## 🙏 Acknowledgements

This tool reads **The CAIDA UCSD Routeviews Prefix to AS mappings Dataset (pfx2as) for
IPv4 and IPv6**, https://www.caida.org/catalog/datasets/routeviews-prefix2as/, derived
from RouteViews BGP data. Please cite the dataset in work that uses this tool;
`CITATION.cff` has the entries for both.
