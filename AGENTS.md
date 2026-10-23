# Instructions for coding agents

Read this before changing anything. `CLAUDE.md` includes this file.

## What this is

A Rust CLI and library (`src/`) that maps IP addresses to origin ASNs with CAIDA's
RouteViews prefix-to-AS (`pfx2as`) snapshots, plus PyO3 Python bindings (`python/`).
There is no Python at runtime; Python exists only as the bindings and the docs tooling.

## Layout

| Path | Purpose |
| --- | --- |
| `src/main.rs` | The CLI (clap): arguments, orchestration, stderr summary, exit codes |
| `src/pfx2as.rs` | Parser of the `pfx2as` format; `Origins` (`A`, `A_B`, `A,B`) |
| `src/table.rs` | Longest-prefix-match table (prefix-trie) for IPv4 and IPv6 |
| `src/caida.rs` | Dataset layout on `publicdata.caida.org`, index parsing, snapshot selection by date |
| `src/fetch.rs` | Cached downloads (index freshness, atomic snapshot download, offline mode) |
| `src/cache.rs` | Inventory of the cache (snapshots, sizes) and pruning by age or count |
| `src/load.rs` | Fills a table from a local file or from CAIDA and records the provenance |
| `src/input.rs` | Input parsing (addresses, `/32` and `/128`, IPv4-mapped IPv6, comments) |
| `src/output.rs` | Records, summary, report; table, JSON, JSON Lines, CSV, TSV, Parquet writers |
| `src/progress.rs` | The binary's progress bars (indicatif) behind `fetch::Progress` |
| `tests/cli.rs`, `tests/fixtures/` | End-to-end tests of the binary; no network |
| `python/src/lib.rs` | PyO3 module `originet._native`: `Table`, `Match` |
| `python/originet/` | Package, typed stub `_native.pyi`, `py.typed` |
| `python/tests/` | pytest suite of the bindings |
| `docs/`, `mkdocs.yml` | Documentation site (MkDocs Material, GitHub Pages) |
| `.github/` | CI, security, CodeQL, release and Pages workflows; Dependabot; review brief |

## Commands

```bash
cargo build && cargo test --workspace --all-features
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
cd python && uv sync --locked && uv run pytest -q && uv run ruff check . && uv run mypy
uvx --with-requirements docs/requirements.txt mkdocs build --strict
```

## Conventions

- American English (US spelling) in identifiers, comments, docs and the changelog.
- `unsafe_code = "forbid"`. Clippy `pedantic` is on; justify any `allow` in `Cargo.toml`.
- Fail loudly: no silent fallbacks. The one deliberate fallback (a stale cached index when
  the download fails) logs a warning. Validate at the edge (`input.rs`, `pfx2as.rs`), not
  inside loops.
- Tests never touch the network. Fixtures live in `tests/fixtures/`; the cache layout can
  be seeded in a temp dir (see `tests/cli.rs`).
- Output columns are a contract: `ip`, `prefix`, `asn`, `origins`, `snapshot`, `error` in
  every format. Changing them is a breaking change and a changelog entry.
- The Python API mirrors the Rust one; a change to `python/src/lib.rs` updates
  `_native.pyi` and the tests.
- Every documented command is run before it is written down; example output is a real
  capture, not an edited one.
- `CHANGELOG.md`: a bullet under `[Unreleased]` for every user-visible change.
- Never push to `main`; feature branch + PR; review per `.github/REVIEW.md`.
- Workflows: actions pinned to SHAs, `permissions: contents: read` by default, lockfiles
  enforced with `--locked`.
