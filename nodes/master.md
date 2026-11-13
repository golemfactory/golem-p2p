# master

**Updated:** 2026-11-13

## Role

Master node of the golem-p2p tests: the public host that other nodes connect
to. It is meant to run the iroh relay and the golem-p2p directory for tests
across machines.

## Reach

| | |
|---|---|
| DNS name | `master.golemp2p.arkiv-global.net` |
| IPv4 | `65.109.154.126` (A record, TTL 3600) |
| IPv6 | `2a01:4f9:3080:1146::2` (no AAAA record yet) |
| NAT | none: the public addresses are on the interface |

The domain `arkiv-global.net` (OVH) expires on 2026-11-18 unless it is renewed.

## Services

| service | URL | state |
|---|---|---|
| iroh relay | `https://master.golemp2p.arkiv-global.net` | planned |
| golem-p2p directory | | planned |
| `golem-p2p listen` | | planned |

## Machine

* Ubuntu 26.04 LTS, kernel 7.0, 32 CPUs, 62 GB RAM, 3.5 TB disk.
* Rust 1.100 (default) and 1.96.0 (the toolchain CI pins).
* Docker runs here; its bridge addresses are not reachable from outside.
