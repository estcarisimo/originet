"""Fast IP-to-ASN lookups on CAIDA's RouteViews prefix-to-AS snapshots.

The work happens in Rust: a longest-prefix-match trie per address family, loaded
from a local ``pfx2as`` file or from a CAIDA snapshot picked by date and cached
locally. Bulk lookups release the GIL.

Examples
--------
>>> from map_ip_to_asn import Table
>>> table = Table.from_caida()                       # newest snapshot, cached
>>> table.lookup("8.8.8.8")                          # doctest: +SKIP
Match(ip='8.8.8.8', prefix='8.8.8.0/24', asn=15169, origins='15169')
>>> table.lookup_asns(["1.1.1.1", "2001:4860::8888"])  # doctest: +SKIP
[13335, 15169]
"""

from map_ip_to_asn._native import Match, Table, __version__

__all__ = ["Match", "Table", "__version__"]
