# Changelog

All notable changes to this project are documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning: [SemVer](https://semver.org).

## [Unreleased]

## [1.0.0] - 2026-10-22

A rewrite in Rust. The tool no longer depends on CAIDA's libipmeta/pyipmeta C stack: it
reads the `pfx2as` snapshots itself, so it installs as a single binary and runs anywhere.

### Added
- Single-binary CLI: addresses from arguments, a file or standard input; IPv4 and IPv6
  in one run; JSON, JSON Lines, CSV, TSV and Parquet output; `--strict`, `--quiet`, `-v`.
- Snapshots are resolved through CAIDA's `pfx2as-creation.log` index for recent days and
  the monthly directory listings for older ones: `--date` picks the newest snapshot on or
  before any day since the dataset began (2005), and the default is the newest one.
- Local cache of indexes and snapshots (`--cache-dir`, `MAP_IP_TO_ASN_CACHE_DIR`), atomic
  downloads, `--offline` and `--refresh`.
- `--pfx2as` / `--pfx2as6` to use local snapshot files (plain or gzip) instead of downloading.
- Every answer carries the longest matching prefix and all origins in CAIDA's notation
  (`A`, `A_B` multi-origin, `A,B` AS set), plus the snapshot date; the JSON report also
  records the snapshot files, their source and the counts.
- Rust library (`map_ip_to_asn`) exposing the parser, the prefix table and the fetcher.
- Python bindings (`python/`, PyO3): `Table.from_files`, `Table.from_caida`, `lookup`,
  `lookup_many` and `lookup_asns` (bulk, GIL released), typed stubs.
- Multi-stage Dockerfile (non-root, cache volume) and Compose file.
- CI on Linux, macOS and Windows; MSRV check; clippy pedantic; rustdoc; cargo-deny;
  CodeQL (Rust, Python, Actions); zizmor; pip-audit; Dependabot for Cargo, uv, pip,
  Actions and Docker; release workflow (binaries, wheels, GHCR image, optional PyPI and
  crates.io Trusted Publishing); MkDocs site on GitHub Pages.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CITATION.cff`, `AGENTS.md`,
  issue forms, PR template, `CODEOWNERS`, review brief.

### Changed
- `--format` is now `-F/--format` (`-f` is `--file`); `--ip` is gone, addresses are
  positional. `--date` no longer fails for dates without a snapshot in the previous six
  months; it uses the newest earlier snapshot and says so.
- The `asn` field is `null` (empty in CSV/TSV) when no prefix covers an address, instead
  of `0`. Invalid inputs are reported per row with an `error` field instead of aborting.
- Output records are flat (`ip`, `prefix`, `asn`, `origins`, `snapshot`, `error`) across
  every format; the per-row `timestamp` and `provider` fields are gone.

### Removed
- The Python implementation (pydantic models, pandas serializers, pyipmeta provider,
  BeautifulSoup scraping), `run-docker.sh`, the Docker Hub publishing job and the
  multi-service Compose setup.

## [0.1.0] - 2025-08-05

### Added
- Initial Python CLI on CAIDA's pyipmeta with JSON, CSV and Parquet output and a Docker image.
