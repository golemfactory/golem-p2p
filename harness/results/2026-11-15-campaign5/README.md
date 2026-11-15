# Benchmark campaign 5: iroh 1.5.0 (golemfactory/iroh fork), 2026-11-15

Campaign 4's scenarios, arguments and run counts again
(`harness/campaign/campaign5.sh all`), after the move from iroh 1.4.1
(crates.io) to **iroh 1.5.0 from the golemfactory/iroh fork** (rev
`4ad24ac2761e97fe57791d8d898fd608b78ca0d5`, branch `relay-recovery`: upstream
main plus bounded relay-actor waits and recovery of stalled relay tasks,
draft upstream PR n0-computer/iroh#4686) with netwatch 0.20. The CLI and
library also changed since campaign 4 (2026-11-08): closed networks, security
fixes, two bench changes (`notes.json`, `bench_settings`), so not every
difference is iroh's.

| | |
|---|---|
| binary | `golem-p2p` built from `656d229` (crates of `31c954c`) with Rust 1.96.0, sha256 `cc4a151ba7261544a2cb7b7d27f9adac1f2b69c651570ac9a2399e6c6bbb6544` (the binary deployed on master and puppet's host) |
| iroh | 1.5.0, fork rev `4ad24ac2`; noq 1.4.0 as in campaign 4 |
| lab relay (`p2p-natlab`) | iroh-relay **1.5.0**, same rev, sha256 `a5150e27…f75c2` (`deploy.sh` with `IROH_RELAY_BIN`) |
| real relay (`p2p-relay1`) | iroh-relay **1.4.1** (puppet's, unchanged, sha256 `9b9804c7…ac71b3`) |
| time | 11:35-12:14 UTC, debug runs until 12:30 |
| build machine load | 0.01-0.32 during the steps that run here (campaign 4: 24-49) |

Generated: `summary.json`, `table.md`, `compare.md` (against campaign 4),
`compare5.md` (campaigns 1-5). Hand-written: `notes.json` (environment,
per-scenario notes, observations, limitations, highlights) and this file.

## Results against campaign 4

| | campaign 4 (iroh 1.4.1) | campaign 5 (iroh 1.5.0) |
|---|---|---|
| runs with a report | 111 / 111 | 111 / 111 |
| final paths direct / relay / failed | 51 / 42 / 0 | 60 / 33 / 0 (firewall, see below) |
| connections lost during a run | 0 | 0 |
| verdict runs right / misleading / wrong | 110 / 1 / 0 | 110 / 1 / 0 |
| lab RTT direct / relayed (p50) | 40.4 / 80.6 ms | 40.4 / 80.5 ms |
| lab throughput direct / relayed (median of scenario medians) | 122.8 / 35.6 Mbps up | 122.8 / 35.4 Mbps up |
| lab concurrency direct (33 runs) | 185-252 Mbps (median 219) | 200-269 Mbps (median 236) |
| lab bind time | 608-609 ms (3.2 s UDP blocked) | 608-610 ms (3.2 s) |
| real world RTT | 34-38 ms (two route clusters) | 34-38 ms |
| real world direct throughput | 131-147 Mbps, 3 slow runs (14-16 Mbps down) | 131-150 Mbps, no slow run |
| real world relayed (via the 1.4.1 relay) | 76-84 Mbps | 76-78 Mbps |
| relay-only single / 4 parallel | 38 Mbps / 161-163 Mbps aggregate | 40 Mbps / 157-162 Mbps aggregate |
| bind on this machine (startup, QAD 56001) | 667 ms (load 27-35) | 588 ms |
| traffic through the real relay, drops | 418.3 MB, 0 | 271.6 MB, 0 |

**iroh 1.5.0 changed nothing measurable**: every scenario whose environment
did not change keeps its outcome, RTT, throughput, hole-punching and bind
times, and the 1.4.1 real relay carries the 1.5.0 clients without drops.

## Findings

1. **Three outcome changes (relay -> direct) are this machine's firewall, not
   iroh.** The build machine was reinstalled on 2026-11-11 without ufw, so
   `rw-symmetric-to-ufw`, `rw-ufw-to-easy` and `rw-ufw-to-symmetric` now go
   direct and `rw-easy-to-ufw` is direct from the first packet. Control
   (`debug/ufw-emul/`, 12 runs with the same binary and ufw's inbound filter
   emulated on one port with nftables): campaign 4's outcomes come back
   exactly.
2. **New ERROR/WARN lines at endpoint shutdown, from the fork's
   relay-recovery commit** (`relay_recv_channel closed`, `All transports
   failed to receive`): in 84 of 204 process exits, always during
   `Endpoint::close`, no functional effect. Local repro
   (`debug/shutdown-logs/`): 0/30 with iroh 1.4.1 and with upstream main,
   7/30 with the fork rev. To be fixed in the fork / upstream PR, not in
   golem-p2p.
3. **Bench bug on the lossy link, fixed afterwards (`cda49ae`):** in 1 of 13
   lossy runs (`debug/lossy-extra/run5`) the bench gave up waiting for an
   upload's count 0.9 s before the listener had all 10 MB, and reported
   `transfer_failed` (exit 5). The wait now goes on while the listener still
   acknowledges data (at most 30 s). Otherwise the lossy link behaves as in
   campaign 4 (loss hint 9/13, concurrency measured 13/13), and requests cut
   off in the concurrency phase are now cancelled on the wire (`a08d715`):
   late requests at the listener in 2/13 runs (<= 0.1 s), campaign 4: 7/13
   (up to 3.9 s).
4. **Harness bug, fixed in `ff6fcd1`:** `listener_saw` of `lab.sh`
   pair/matrix and `realworld.sh` was lost since the listener's per-request
   lines moved to debug level (`9e7afee`).
5. The build machine was idle this time: binds here are back at campaign 3's
   level, and none of campaign 4's slow Hetzner -> lab downloads came back.

Details, per scenario and per observation: `notes.json`.

## Real-world check (`realworld.sh`)

`harness/lab/realworld.sh --nodes "real1 real2" --repeat 3 --natcheck
--natcheck-local-port 56090` under the host profiles `port-restricted`,
`full-cone` and `symmetric`, right after the campaign, as on 2026-11-14:
[../2026-11-15-realworld-master/](../2026-11-15-realworld-master/README.md).
**36 of 36 runs direct at the end**, RTT 33-37 ms, first reply 0.33-0.38 s
(lab node sends) and 0.75-0.80 s (master sends); 2026-11-14 with iroh 1.4.1:
40 of 40, 33-37 ms, 0.37-0.38 / 0.77-0.79 s. The campaign's `realworld` step
does not replace these runs: it benches under `port-restricted` only, without
`natcheck` and without `send`'s time to the first reply.

## Files

    versions.txt              binaries and relay versions on every machine
    scenarios.jsonl           scenario definitions (identical to campaign 4's)
    <group>/<scenario>/run<r>/   raw runs (bench.jsonl, bench.err, listen.*, meta.json)
    hetzner-load.txt          this machine's load average during realworld/startup/relay
    relay-metrics.txt         the real relay's counters before and after each step
    debug/lossy-extra/        10 more lab-netem-pr runs (run.sh on the lab VM)
    debug/ufw-emul/           the *-ufw scenarios with ufw emulated (run.sh)
    debug/shutdown-logs/      local repro of the shutdown log lines (repro.sh)

Private node keys of the runs were not kept (`.keys/` is git-ignored).
