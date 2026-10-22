from os import PathLike
from typing import Any

__version__: str

class Match:
    """The answer for one address: its longest matching prefix and origin ASN(s)."""

    ip: str
    prefix: str
    asn: int
    origins: str
    @property
    def is_multi_origin(self) -> bool: ...
    @property
    def is_as_set(self) -> bool: ...
    def asns(self) -> list[int]: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

class Table:
    """A prefix-to-origin table for IPv4 and IPv6 with longest-prefix-match lookups."""

    @staticmethod
    def from_files(
        ipv4: str | PathLike[str] | None = None,
        ipv6: str | PathLike[str] | None = None,
    ) -> Table: ...
    @staticmethod
    def from_caida(
        date: str | None = None,
        *,
        cache_dir: str | PathLike[str] | None = None,
        ipv4: bool = True,
        ipv6: bool = True,
        offline: bool = False,
        refresh: bool = False,
    ) -> Table: ...
    def lookup(self, ip: str) -> Match | None: ...
    def lookup_many(self, ips: list[str], *, strict: bool = True) -> list[Match | None]: ...
    def lookup_asns(self, ips: list[str], *, strict: bool = True) -> list[int | None]: ...
    @property
    def len_v4(self) -> int: ...
    @property
    def len_v6(self) -> int: ...
    @property
    def snapshots(self) -> list[dict[str, Any]]: ...
    def __len__(self) -> int: ...
