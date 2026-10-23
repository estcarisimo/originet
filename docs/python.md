# Python

The `originet` package wraps the Rust table with [PyO3](https://pyo3.rs). It has
no Python dependencies and ships type stubs (`py.typed`).

## Install

Not on PyPI yet. From the repository (the extension is compiled, so a Rust toolchain
1.88+ is needed):

```bash
uv pip install "originet @ git+https://github.com/estcarisimo/originet#subdirectory=python"
```

Wheels for Linux (x86_64, aarch64, manylinux), macOS (Intel, Apple Silicon) and Windows
are built by the release workflow and attached to each release.

## Loading a table

```python
from originet import Table

table = Table.from_caida()                                   # newest IPv4 + IPv6 snapshots
table = Table.from_caida("2019-06-01", ipv6=False)           # newest on or before that day, IPv4 only
table = Table.from_caida(cache_dir="/data/pfx2as", offline=True)   # only what is cached
table = Table.from_files(ipv4="routeviews-rv2-20261021-0800.pfx2as.gz",
                         ipv6="routeviews-rv6-20261021-1200.pfx2as.gz")
```

`from_caida` takes `date` (`"YYYY-MM-DD"` or `None`), and keyword-only `cache_dir`,
`ipv4`, `ipv6`, `offline`, `refresh`. It raises `ValueError` for a bad date or a date
before the dataset, `OSError` when offline and something is not cached or a file cannot
be read, and `RuntimeError` for a failed download. Loading the IPv4 snapshot (1.1 M
prefixes) takes about a second; the table is immutable afterwards and safe to share
between threads.

`table.snapshots` lists what was loaded:

```python
>>> table.snapshots
[{'family': 'ipv4', 'source': 'caida', 'date': '2026-10-21',
  'file': 'routeviews-rv2-20261021-0800.pfx2as.gz',
  'location': 'https://publicdata.caida.org/datasets/routing/routeviews-prefix2as/2026/10/routeviews-rv2-20261021-0800.pfx2as.gz',
  'prefixes': 1129523}, ...]
>>> table.len_v4, table.len_v6, len(table)
(1129523, 265790, 1395313)
```

## Looking up

```python
>>> m = table.lookup("1.1.8.9")
>>> m
Match(ip='1.1.8.9', prefix='1.1.8.0/24', asn=149511, origins='149511_138421')
>>> m.asn, m.prefix, m.origins, m.asns(), m.is_multi_origin, m.is_as_set
(149511, '1.1.8.0/24', '149511_138421', [149511, 138421], True, False)
>>> table.lookup("10.0.0.1") is None
True
>>> table.lookup("not an ip")
ValueError: "not an ip": invalid IP address syntax
```

For many addresses use the bulk methods; they parse and look up in Rust with the GIL
released, so other threads keep running:

```python
hits = table.lookup_many(ips)                  # list[Match | None], same order as ips
asns = table.lookup_asns(ips)                  # list[int | None], cheaper when only the ASN matters
asns = table.lookup_asns(ips, strict=False)    # invalid inputs become None instead of raising
```

`Match` objects are immutable, hashable and compare by `ip`, `prefix` and `origins`.

### With pandas

```python
import pandas as pd
from originet import Table

df = pd.read_csv("flows.csv")
table = Table.from_caida("2024-01-01")
hits = table.lookup_many(df["dst_ip"].astype(str).tolist(), strict=False)
df["asn"] = [h.asn if h else None for h in hits]
df["prefix"] = [h.prefix if h else None for h in hits]
```

## Development

```bash
cd python
uv sync --locked          # builds the extension in editable mode
uv run pytest -q
uv run ruff check . && uv run ruff format --check . && uv run mypy
```

The tests use the fixtures in `../tests/fixtures/` and never touch the network. On an
Apple Silicon Mac the Rust toolchain must be the arm64 one (rustup); an x86_64 Homebrew
toolchain cannot build the extension for an arm64 Python.
