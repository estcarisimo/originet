"""Tests of the Python bindings against the fixture snapshots. No network."""

from pathlib import Path

import pytest

from originet import Match, Table, __version__

FIXTURES = Path(__file__).resolve().parents[2] / "tests" / "fixtures"


@pytest.fixture(scope="module")
def table() -> Table:
    return Table.from_files(
        ipv4=FIXTURES / "sample-v4.pfx2as.gz", ipv6=FIXTURES / "sample-v6.pfx2as"
    )


def test_version_is_the_crate_version() -> None:
    assert __version__.count(".") == 2


def test_table_reports_what_it_loaded(table: Table) -> None:
    assert table.len_v4 == 9
    assert table.len_v6 == 5
    assert len(table) == 14
    families = [snapshot["family"] for snapshot in table.snapshots]
    assert families == ["ipv4", "ipv6"]
    assert table.snapshots[0]["source"] == "local"
    assert table.snapshots[0]["prefixes"] == 9
    assert "sample-v4.pfx2as.gz" in repr(table)


@pytest.mark.parametrize(
    ("ip", "prefix", "asn", "origins"),
    [
        ("8.8.8.8", "8.8.8.0/24", 15169, "15169"),
        ("1.1.8.200", "1.1.8.0/24", 149511, "149511_138421"),
        ("64.9.9.9", "64.0.0.0/8", 64496, "64496,64497"),
        ("64.1.2.3", "64.1.0.0/16", 64498, "64498"),
        ("2001:4860::8888", "2001:4860::/32", 15169, "15169"),
        ("2001:4:112::53", "2001:4:112::/48", 112, "112"),
        ("::ffff:8.8.4.4", "8.8.4.0/24", 15169, "15169"),
    ],
)
def test_lookup_returns_the_longest_match(
    table: Table, ip: str, prefix: str, asn: int, origins: str
) -> None:
    match = table.lookup(ip)
    assert match is not None
    assert match.prefix == prefix
    assert match.asn == asn
    assert match.origins == origins


def test_multi_origin_and_as_set_flags(table: Table) -> None:
    moas = table.lookup("1.1.8.1")
    assert moas is not None
    assert moas.is_multi_origin and not moas.is_as_set
    assert moas.asns() == [149511, 138421]
    as_set = table.lookup("64.200.0.1")
    assert as_set is not None
    assert as_set.is_as_set and not as_set.is_multi_origin
    assert as_set.asns() == [64496, 64497]


def test_lookup_misses_and_invalid(table: Table) -> None:
    assert table.lookup("203.0.113.1") is None
    assert table.lookup("2a00::1") is None
    with pytest.raises(ValueError, match="not-an-ip"):
        table.lookup("not-an-ip")


def test_lookup_many_keeps_order(table: Table) -> None:
    results = table.lookup_many(["8.8.8.8", "203.0.113.1", "2606:4700::1111"])
    assert [r.asn if r else None for r in results] == [15169, None, 13335]
    assert isinstance(results[0], Match)
    assert results[0] == table.lookup("8.8.8.8")
    assert hash(results[0]) == hash(table.lookup("8.8.8.8"))


def test_lookup_many_strictness(table: Table) -> None:
    with pytest.raises(ValueError, match="bogus"):
        table.lookup_many(["8.8.8.8", "bogus"])
    assert table.lookup_asns(["8.8.8.8", "bogus", "9.9.9.9"], strict=False) == [15169, None, 19281]


def test_one_family_is_enough() -> None:
    v4_only = Table.from_files(ipv4=FIXTURES / "sample-v4.pfx2as")
    assert v4_only.len_v6 == 0
    assert v4_only.lookup("2001:4860::8888") is None
    assert v4_only.lookup("8.8.8.8") is not None


def test_from_files_validates_its_arguments(tmp_path: Path) -> None:
    with pytest.raises(ValueError):
        Table.from_files()
    with pytest.raises(OSError, match="expected an ipv4 prefix"):
        Table.from_files(ipv4=FIXTURES / "sample-v6.pfx2as")
    with pytest.raises(OSError):
        Table.from_files(ipv4=tmp_path / "missing.pfx2as")


def test_from_caida_offline_uses_the_cache(tmp_path: Path) -> None:
    dataset = tmp_path / "routeviews-prefix2as"
    (dataset / "2026" / "10").mkdir(parents=True)
    (dataset / "pfx2as-creation.log").write_bytes((FIXTURES / "pfx2as-creation.log").read_bytes())
    (dataset / "2026" / "10" / "routeviews-rv2-20261020-1000.pfx2as.gz").write_bytes(
        (FIXTURES / "sample-v4.pfx2as.gz").read_bytes()
    )
    table = Table.from_caida("2026-10-20", cache_dir=tmp_path, ipv6=False, offline=True)
    assert table.snapshots[0]["source"] == "caida"
    assert table.snapshots[0]["date"] == "2026-10-20"
    assert table.lookup_asns(["8.8.8.8"]) == [15169]

    with pytest.raises(OSError, match="not cached"):
        Table.from_caida(cache_dir=tmp_path, ipv6=False, offline=True)
    with pytest.raises(ValueError, match="YYYY-MM-DD"):
        Table.from_caida("yesterday", cache_dir=tmp_path, ipv6=False, offline=True)
    with pytest.raises(ValueError, match="at least one"):
        Table.from_caida(cache_dir=tmp_path, ipv4=False, ipv6=False, offline=True)
    with pytest.raises(ValueError, match="dataset starts in 2005"):
        Table.from_caida("2004-12-31", cache_dir=tmp_path, ipv6=False, offline=True)


def test_bulk_lookups_agree_across_threads(table: Table) -> None:
    # Checks that concurrent callers get consistent answers; it does not measure
    # whether the GIL is released (a property of the Rust code, not observable here).
    from concurrent.futures import ThreadPoolExecutor

    ips = ["8.8.8.8", "1.1.8.9", "203.0.113.1", "2606:4700::1111"] * 250
    expected = table.lookup_asns(ips)
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(lambda _: table.lookup_asns(ips), range(8)))
    assert all(result == expected for result in results)


def test_failed_download_is_a_runtime_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    # Route the download through a proxy that is not listening, with nothing cached.
    monkeypatch.setenv("https_proxy", "http://127.0.0.1:9")
    monkeypatch.setenv("HTTPS_PROXY", "http://127.0.0.1:9")
    with pytest.raises(RuntimeError, match="downloading"):
        Table.from_caida(cache_dir=tmp_path, ipv6=False)
