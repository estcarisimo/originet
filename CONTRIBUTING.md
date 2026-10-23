# Contributing

Thanks for helping with originet. Bugs and proposals go through
[issues](https://github.com/estcarisimo/originet/issues); code through pull requests.

## Setup

```bash
git clone git@github.com:estcarisimo/originet.git && cd originet
cargo build                       # the CLI: target/debug/originet
cargo test --workspace --all-features
cd python && uv sync --locked     # builds the extension in editable mode (needs Rust)
uv run pytest
```

Requirements: Rust 1.88 or newer (`rust-version` in `Cargo.toml`), and for the Python
bindings [uv](https://docs.astral.sh/uv/) and Python 3.10 or newer. Build the extension
with `uv sync` (maturin), not with `cargo build --workspace`: a pyo3 `extension-module`
cannot be linked by cargo alone (`cargo clippy --workspace` and `cargo test --workspace`
are fine, they do not link it). On an Apple Silicon
Mac the Rust toolchain must be the arm64 one (rustup); an x86_64 Homebrew toolchain
cannot build the extension for an arm64 Python.

Checks that CI runs, in the same order:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy -p originet --all-targets --no-default-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p originet --no-deps --all-features
cargo deny check                  # cargo install cargo-deny
cargo test --workspace --all-features --locked
cargo test -p originet --no-default-features --locked
(cd python && uv run ruff check . && uv run ruff format --check . && uv run mypy && uv run pytest -q)
docker build -t originet .
uvx --with-requirements docs/requirements.txt mkdocs build --strict
```

Tests never touch the network: they use the fixtures in `tests/fixtures/` and a seeded
cache directory. Keep it that way; a test that needs CAIDA is an `#[ignore]` test.

## Pull requests

- Branch from `main`; never push to `main` directly (the `protect-main` ruleset enforces
  a pull request with green checks).
- Every PR gets an independent review following `.github/REVIEW.md` and is merged only
  after `VERDICT: APPROVE` on its final commit with CI green.
- Add a bullet under `[Unreleased]` in `CHANGELOG.md` for user-visible changes.
- American English in identifiers, comments, docs and the changelog.
- Every documented command is run before it is written down; example output is a real
  capture.
- A change to the Python API updates `python/originet/_native.pyi` and
  `python/tests/` too.
- Dependencies: pinned to SHAs in workflows; new crates must pass `cargo deny check`
  (license allow-list in `deny.toml`).

## Releases

1. Move `[Unreleased]` in `CHANGELOG.md` to `## [X.Y.Z] - YYYY-MM-DD`.
2. Set `version` to `X.Y.Z` in `Cargo.toml`, `python/Cargo.toml`, `python/pyproject.toml`
   and `CITATION.cff` (with `date-released`); run `cargo update --workspace` and
   `(cd python && uv lock)` so the lockfiles follow.
3. Merge that PR, then tag `vX.Y.Z` on `main` and push the tag.

The release workflow checks that the tag, the four versions and the changelog agree and
that the tag is on `main`; then it creates the GitHub release with the changelog section,
attaches binaries (Linux x86_64/aarch64 musl, macOS x86_64/arm64, Windows x86_64), builds
the wheels and the sdist, and pushes `ghcr.io/estcarisimo/originet`.

Publishing to PyPI and crates.io is gated by the repository variables `PYPI_PUBLISH` and
`CRATES_IO_PUBLISH` (`true` to publish) and uses OIDC Trusted Publishing, no stored
tokens. One-time setup per registry:

- PyPI: a *pending publisher* for `originet` (owner `estcarisimo`, repository
  `originet`, workflow `release.yml`, environment `pypi`) can be created before the first
  upload, so the workflow can publish the first version.
- crates.io has no pending publisher: the first version is uploaded by hand
  (`cargo login` with a token scoped to `publish-new` for `originet`, then
  `cargo publish -p originet --locked` from `main`), the trusted publisher is added in the
  crate's settings (owner `estcarisimo`, repository `originet`, workflow `release.yml`,
  environment `crates-io`), and the token is revoked. The workflow skips a version that
  is already on the registry, so tagging that same version afterwards is safe.
