# master: testnet self-test, long-lived nodes, certificate renewal and expiry

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**Date:** 2026-11-15
**Answers:** [`2026-11-15-master-testnet-registry.md`](2026-11-15-master-testnet-registry.md)
**Details:** [`nodes/master.md`](../nodes/master.md), [`nodes/puppet.md`](../nodes/puppet.md) (section "Testnet"), [`docs/design.md`](../docs/design.md) section 13a

## Summary

The closed testnet `golem-testnet` now has two long-lived members, one on
master and one on puppet's host, and was tested end to end. Certificate
renewal and expiry on live connections, untested until now, work as designed
(by hand with the real 5 min clock tolerance, and in new automated tests).
Four issues were fixed in code (`f05a7c8`, `f25db17`) and one in nginx;
puppet's host has one configuration issue, worked around for the service
(request 1). Both hosts run `golem-p2p` built from `f25db17`.

| | node ID | where |
|---|---|---|
| `golem-p2p-testnet-listen`, account `master` | `0x16d62b72e71dd73ccdd121f26b11152a5ed4b877` | master, systemd |
| `golem-p2p-testnet-listen`, account `puppet` | `0x03b31e0e661ef73f64c8710b8486518198e81c18` | puppet's host, systemd |

Both run `listen` (echo and bulk) with the options of "How a node joins",
through puppet's public relay. Each account also has a registered `client.key`
for `send` tests (`0x962865b3…` on master, `0xdf54c680…` on puppet). The
`puppet` account's token is on puppet only (`~/golem-p2p-testnet/`, 0600).

## Results

**Members, both ways** (`send --count 3`, through the public names):

| | direct | `--relay-only` |
|---|---|---|
| master -> puppet | 123 ms first, then 33-36 ms | 114 ms first, then 34 ms |
| puppet -> master | 144 ms first, then 33 ms | 157 ms first, then 37 ms |

Again after deploying `f25db17` and restarting every service: master ->
puppet 36 ms direct, 34 ms relay; puppet -> master 37 ms direct, 37 ms relay.

**Outsiders, from puppet's host:** an unregistered key with `--registry` is
refused by the registry at startup (`... is not registered in network
golem-testnet`); without `--registry` its record is refused by the directory
(`403`) and its connection by the master node (`not a member of this
network`).

**Renewal and expiry on live connections** (a temporary registry and
directory on master's localhost, `registry init --cert-lifetime 60s`, removed
afterwards): listener A, two `soak` clients B and C (one echo per second),
all members; B removed from the registry after 150 s.

* Renewal: every node renewed every 30 s; A and C 18 times in 9.3 min.
  C: 555 of 555 echoes ok over **one** connection (no reconnect across 18
  renewals of both ends); B: 512 of 512 ok over one connection, before and
  after its removal, as long as its last certificate was valid.
* Removal: B's renewals were refused from the next attempt on (retried with
  backoff). Its last certificate expired at 23:35:37 UTC; A closed the
  connection at **23:40:39.97**, i.e. expiry + 300 s tolerance + 3 s (the
  5 s sweep). B's reconnects were then refused (`rejected`, `not a member of
  this network`): 43 failed echoes until the end. C was not affected.

## Findings

1. **Renewal could loop (bug, fixed).** The renewal was scheduled from the
   certificate's `issued_at`, which is the registry's clock. With the node's
   clock ahead of the registry's by more than half the lifetime (30 s with
   `--cert-lifetime 60s`, 30 min with the default 1 h), every new
   certificate was already "due" and the node renewed in a tight loop
   (registry request, re-signing and pushing the record to every peer and
   the directory, again and again). The new test counted 103 renewals in 3 s
   before the fix. Now the renewal is timed on the node's own clock from
   receipt (half the lifetime, at least 1 s).
2. **Wrong `--network` or `--registry-key` blamed a peer (fixed).** A node
   whose settings did not match its registry failed with `peer is not a
   member of this network: membership certificate is for network
   "golem-testnet"` (error kind `not_member`), although no peer was
   involved. Now: `membership certificate: the registry's certificate does
   not match this node's network settings (network name, registry key): ...`
   (kind `membership`). A malformed certificate from the registry is
   reported as the registry's fault likewise.
3. **No rate limit on the registry (hardened).** nginx now limits the
   registry to 10 requests/s per client address (bursts of 100, then `429`;
   checked: 130 parallel requests, 101 answered, 29 got `429`). Nodes treated
   every `4xx` as "registry refused"; `429` and `408` are now temporary
   (`registry unavailable`, retried like an outage). The directory is not
   limited (soak tests from one address do many lookups).
4. **Color codes in the journal (fixed).** The CLI wrote ANSI color codes to
   stderr also when it is not a terminal, so journald and log files were full
   of `\e[2m...`. Colors are now used only on a terminal (and never with
   `NO_COLOR`).
5. **puppet's host cannot reach its own public relay by name (worked
   around, request 1).** The host's `/etc/hosts` maps
   `puppet.golemp2p.arkiv-global.net` to `127.0.1.1` and `::1`, where the
   relay (VM `p2p-relay1`, forwarded on `213.163.75.157`) is not. The node
   came up without a home relay (`relays=[]`, `home relay unreachable` every
   11 s), so `--relay-only` to it could not work. The service now gets a
   private hosts file (`BindReadOnlyPaths`); manual runs need the same
   (`nodes/puppet.md`).
6. **The directory forgets everything on restart (documented, not
   changed).** Records live in memory; nodes republish every TTL/3 (20 min),
   so after a restart of `golem-p2p-directory` nodes cannot be looked up for
   up to 20 min unless they are restarted too. Noted in `nodes/master.md`.
7. **Service hardening.** All four units (registry, directory, both
   listeners) now run with `ProtectSystem=strict`, `ProtectHome`, no
   capabilities, `PrivateDevices`, kernel protections, restricted address
   families, `MemoryDenyWriteExecute` and `UMask=0077`; the listeners restart
   always (every 15 s, no start limit), e.g. when the registry is down at
   boot.
8. **Registry key backup (open).** `~/golem-p2p-testnet/registry.json` on
   master holds the registry's signing key, in one copy. Losing it means a
   new key for every node; it needs an encrypted copy off the host (for the
   admin).

New tests in `closed_network.rs`: renewal on a live connection (2 s
certificates: renewed records reach the peer, same connection), no renewal
loop with the registry's clock 40 s behind, an expired member disconnected by
its peer and refused afterwards (certificates issued in the past, so it runs
in seconds without changing the 5 min tolerance), `429` versus `403`, and
wrong network settings. Checks as CI: fmt, clippy 1.96.0 (all features and no
default features), tests, MSRV 1.91.0 build.

## Requests

1. **puppet:** please decide about the host's `/etc/hosts`: either map
   `puppet.golemp2p.arkiv-global.net` to `213.163.75.157` (and drop it from
   the `::1` line), the usual setting for a host with a fixed address, or
   keep the per-service workaround. If you change it, tell master (or remove
   the `BindReadOnlyPaths` line of `golem-p2p-testnet-listen` yourself).
2. **puppet:** the testnet node needs nothing from you. Logs:
   `journalctl -u golem-p2p-testnet-listen`. Use `client.key` for your own
   tests in the testnet; `outsider.key` is deliberately unregistered.
