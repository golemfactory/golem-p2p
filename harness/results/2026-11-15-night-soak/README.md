# Night soak: one connection for 6 hours (2026-11-15)

**Result: PASS on both paths.** Each client kept a single connection to
puppet for the full 6 hours: no reconnect, no disconnect, no failed request
or upload.

## Setup

* Window: 01:41 to 07:41 UTC (`--duration 6h`), both ended normally
  (`final: true`, not interrupted, exit status 0).
* Client: `golem-p2p soak` on master (binary `9e7afee`), in the closed
  testnet `golem-testnet` (registry certificates, members-only directory).
* Target: puppet's testnet listener `0x03b31e0e661ef73f64c8710b8486518198e81c18`.
* Traffic: a 64-byte echo every 5 s and a 1 MB upload every 10 min,
  `soak_stats` every 5 min.

| run | client node ID | path |
|---|---|---|
| `night-direct` | `0x962865b3f423127ada55192512c63dc25ce73813` | direct (IPv6) |
| `night-relay` | `0xc3567c62d207f75c17bc178726a030511cd1662e` | `--relay-only`, puppet's public relay |

## Results

| | direct | relay-only |
|---|---|---|
| connections (`new_conns`) | 1 | 1 |
| `disconnect` events | 0 | 0 |
| echo requests ok | 4320 / 4320 | 4320 / 4320 |
| 1 MB uploads ok | 35 / 35 | 35 / 35 |
| RTT p50 (range of 5-min windows) | 33.7 to 34.0 ms | 34.3 to 34.5 ms |
| worst RTT (max of 6 h) | 34.4 ms | 39.8 ms |
| 1 MB upload throughput | 29 to 131 Mbit/s | 28 to 33 Mbit/s |
| first reply after start | 5.5 s | 2.2 s |

1 MB uploads are short, so their throughput mostly reflects QUIC's slow
start, not the link's capacity.

**Certificate renewals.** Certificates last 1 h and renew at half their
lifetime, so each client renewed about 11 times, with the connection staying
up. Each renewal publishes a new record; together with the 20-minute
republish that gives the 28 (direct) and 29 (relay) `record` events.

**Path changes.** One on the direct run: at 01:45:00 the connection's
selected path moved from puppet's IPv6 address to its IPv4 address
(`213.163.75.157:53715`) and 34 ms later back to IPv6. Same connection, no
failed request. None on the relay run.

**Warnings.** Three of the same kind, at 05:40 and 06:40:
`publishing record failed: discovery service holds a newer record`. Every
60 minutes the 20-minute republish and the 30-minute certificate renewal
publish two records 65 ms apart; when the older one reaches the directory
second, it is refused. The directory keeps the newer record, so this is
harmless log noise. Fix later: serialize publishes, or don't warn when the
directory holds our own newer record.

## Files

* `direct.jsonl`, `relay.jsonl`: all `--json` events (see `docs/cli-events.md`).
* `direct.err`, `relay.err`: stderr (info and warnings).
* `check.py`: the pass/fail summary used above (`./check.py` in this folder).
