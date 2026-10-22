# Security Policy

## Scope

In scope: the `map-ip-to-asn` CLI and Rust library, the Python bindings under `python/`,
the Dockerfile, the Compose file and the GitHub Actions workflows. The data comes from
CAIDA's public RouteViews prefix-to-AS dataset; problems with the data itself belong to
[CAIDA](https://www.caida.org/catalog/datasets/routeviews-prefix2as/).

## Supported versions

The latest release on `main`. There are no maintained older branches.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:
https://github.com/estcarisimo/map-ip-to-asn/security/advisories/new. Please include
what is affected, how to reproduce it, the impact, and whether it is already public. You
will get an acknowledgment within 7 days and an assessment within 14; fixes ship as a
release. Please allow up to 90 days before public disclosure.

If private reporting is unavailable, contact the maintainer listed in
`.github/CODEOWNERS` through GitHub with the subject `[map-ip-to-asn security]`.

## What the tool does with the network and the file system

- It downloads only from `https://publicdata.caida.org` (HTTPS, rustls), with an
  identifying `User-Agent`, and only when the needed index or snapshot is not cached.
  `--offline` disables downloads entirely.
- Downloads go to a temporary file in the cache directory and are renamed into place, so
  an interrupted download never leaves a truncated file behind.
- The cache directory is the platform cache dir (`~/.cache/map-ip-to-asn` on Linux,
  `~/Library/Caches/map-ip-to-asn` on macOS), or `--cache-dir` / `MAP_IP_TO_ASN_CACHE_DIR`.
- Input files are read as text; gzip is detected by magic bytes and decompressed in
  memory in a streaming fashion. The tool never executes anything from its inputs.
- No telemetry, no other endpoints.

## What is in place

- `unsafe_code = "forbid"` in every crate; clippy `pedantic` with `-D warnings`.
- Supply chain: `Cargo.lock` and `python/uv.lock` committed and enforced (`--locked`),
  `cargo deny` (advisories, license allow-list, registry allow-list) on every PR and
  weekly, `pip-audit` on the bindings' dev tools, Dependabot for Cargo, uv, pip, Actions
  and Docker base images, actions pinned to commit SHAs, `zizmor` on the workflows,
  `permissions: contents: read` by default, CodeQL for Rust, Python and Actions.
- Releases: tag must match the versions and sit on `main`; binaries with SHA-256
  checksums; registries only through OIDC Trusted Publishing, no stored tokens.
- Container: multi-stage build, Debian slim runtime with only `ca-certificates`, non-root
  user, no shell entrypoint.
- Process: no direct pushes to `main` (`protect-main` ruleset), independent review of
  every PR (`.github/REVIEW.md`), CI green before merge.

## Out of scope

Resource exhaustion by feeding the tool enormous input files on purpose (it streams but
holds the whole result set in memory before writing), and findings in CAIDA's
infrastructure.
