# Changelog

All notable changes to this project are documented in this file.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning: [SemVer](https://semver.org).

## [Unreleased]

## [1.0.1] - 2026-10-23

The crates.io 1.0.0 was uploaded by hand from a commit that predates the cache commands,
so `cargo install originet` lacked `--cache` while the release binaries and the Python
wheels had it. 1.0.1 brings every channel to the same code, and it is the first version
published to crates.io by the release workflow through Trusted Publishing (PyPI 1.0.0
already was).

### Added
- The documentation site serves `/llms.txt` and `/llms-full.txt` for AI agents.
  The README links to them.

### Changed
- README and library docs: install from crates.io (`cargo install originet`), link docs.rs.

### Fixed
- Release workflow: the PyPI publish and the release attach download only the wheel and
  sdist artifacts; the image job's `.dockerbuild` record made twine fail the upload.
- Release workflow: the linux-aarch64 wheel builds again (manylinux_2_28 cross image and
  `__ARM_ARCH` for ring), so the wheels get attached and published. That wheel now
  targets glibc 2.28 (manylinux_2_28); the x86_64 wheel stays on manylinux2014.

## [1.0.0] - 2026-10-23

A rewrite in Rust, renamed from `map-ip-to-asn` to **originet**. The tool no longer
depends on CAIDA's libipmeta/pyipmeta C stack: it reads the `pfx2as` snapshots itself and
ships as one binary with no C library to install.

### Added
- Single-binary CLI: addresses from arguments, a file or standard input; IPv4 and IPv6
  in one run; JSON, JSON Lines, CSV, TSV and Parquet output; `--strict`, `--quiet`, `-v`.
- Snapshots are resolved through CAIDA's `pfx2as-creation.log` index for recent days and
  the monthly directory listings for older ones: `--date` picks the newest snapshot on or
  before any day since the dataset began (2005), and the default is the newest one.
- Local cache of indexes and snapshots (`--cache-dir`, `ORIGINET_CACHE_DIR`), atomic
  downloads, `--offline` and `--refresh`.
- `--pfx2as` / `--pfx2as6` to use local snapshot files (plain or gzip) instead of downloading.
- Every answer carries the longest matching prefix and all origins in CAIDA's notation
  (`A`, `A_B` multi-origin, `A,B` AS set), plus the snapshot date; the JSON report also
  records the snapshot files, their source and the counts.
- Rust library (`originet`) exposing the parser, the prefix table and the fetcher.
- Python bindings (`python/`, PyO3): `Table.from_files`, `Table.from_caida`, `lookup`,
  `lookup_many` and `lookup_asns` (bulk, GIL released), typed stubs.
- Multi-stage Dockerfile (non-root, cache volume) and Compose file.
- CI on Linux, macOS and Windows; MSRV check; clippy pedantic; rustdoc; cargo-deny;
  CodeQL (Rust, Python, Actions); zizmor; pip-audit; Dependabot for Cargo, uv, pip,
  Actions and Docker; release workflow (binaries, wheels, GHCR image, optional PyPI and
  crates.io Trusted Publishing); MkDocs site on GitHub Pages.
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `CITATION.cff`, `AGENTS.md`,
  issue forms, PR template, `CODEOWNERS`, review brief.
- `--format table`: aligned columns for reading in a terminal, with `-` for missing
  values and one `asn` column that lists every origin (`149511, 138421` for a
  multi-origin prefix, `{8035, 13979}` for an AS set). It is the default when standard
  output is a terminal; `json` stays the default for pipes and `--output`.
- `--color auto|always|never`: on a terminal the table, the summary counts and the
  progress bar are colored (`NO_COLOR` honored); `never` gives plain text, `always`
  keeps the escapes in a pipe or a file. Library: `output::Theme` and `write_table`.
- A progress bar on standard error while a snapshot downloads (size, speed, time left)
  and a spinner while it loads, when standard error is a terminal; `--quiet` turns them
  off.
- `--fetch`: download the day's IPv4 and IPv6 snapshots into the cache and exit, printing
  each one's date, `downloaded`/`cached` and path; with `--offline` it checks the cache.
- Library: `fetch::Progress` and `Fetcher::progress` to observe downloads and loads;
  `load::fetch_snapshot` resolves and caches a snapshot without loading it.
- `--cache list|size|prune`: inspect and prune the cache without deleting by hand.
  `list` prints every cached snapshot with its family, date, size and path, `size` the
  count and size per family and in total, and `prune` removes the snapshots that
  `--keep-days N` (older than N days) and `--keep-last N` (all but the N newest per
  family) do not keep, then the month and year directories left empty; `--dry-run`
  only reports. Indexes and files originet did not create are never touched. Library:
  the `cache` module (`Inventory`, `Retention`, `prune`, `format_bytes`).

### Changed
- Project renamed to `originet` (repository `estcarisimo/originet`; the old URL
  redirects). The binary, the crate, the Python package and the cache directory follow
  the new name; the environment variable is `ORIGINET_CACHE_DIR`.
- `--format` is now `-F/--format` (`-f` is `--file`); `--ip` is gone, addresses are
  positional. `--date` no longer fails for dates without a snapshot in the previous six
  months; it uses the newest earlier snapshot and says so.
- The `asn` field is `null` (empty in CSV/TSV) when no prefix covers an address, instead
  of `0`. Invalid inputs are reported per row with an `error` field instead of aborting.
- Output records are flat (`ip`, `prefix`, `asn`, `origins`, `snapshot`, `error`) across
  every format; the per-row `timestamp` and `provider` fields are gone.
- At a terminal the default output is the table instead of the JSON report; pipes and
  files are unchanged.
- Release workflow: a version already on PyPI or crates.io is skipped instead of failing
  the run, so the first crates.io upload can be done by hand and the tag still works.
- Crate tarball no longer carries `AGENTS.md`, `CLAUDE.md` and `.dockerignore`.

### Removed
- The Python implementation (pydantic models, pandas serializers, pyipmeta provider,
  BeautifulSoup scraping), `run-docker.sh`, the Docker Hub publishing job and the
  multi-service Compose setup.

## [0.1.0] - 2025-08-05

### Added
- Initial Python CLI on CAIDA's pyipmeta with JSON, CSV and Parquet output and a Docker image.
