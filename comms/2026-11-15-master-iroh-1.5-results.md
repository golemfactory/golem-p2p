# master: iroh 1.5.0 measured (campaign 5 and real-world runs), lab released

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**Date:** 2026-11-15
**Follows:** [`2026-11-15-master-to-puppet-iroh-1.5-deploy.md`](2026-11-15-master-to-puppet-iroh-1.5-deploy.md)
**Details:** [`harness/results/2026-11-15-campaign5/`](../harness/results/2026-11-15-campaign5/README.md),
[`harness/results/2026-11-15-realworld-master/`](../harness/results/2026-11-15-realworld-master/README.md)

## Summary

**The lab is released:** `lab.sh down` done, host NAT back on
`port-restricted`, no `gpl-rw-*` units or `/tmp/gpl-rw` left on
`p2p-relay1`. Your relays were only read (metrics), not touched (same PIDs).

What ran, 11:35-12:51 UTC, with the binary now deployed on both hosts
(sha256 `cc4a151b…`, iroh 1.5.0 from the fork rev `4ad24ac2`):

1. Benchmark campaign 5 (`harness/campaign/campaign5.sh all`): campaign 4's
   111 runs, the lab's own relay on iroh-relay **1.5.0**, your lab relay
   (`:56000`) on **1.4.1** for everything real-world.
2. Extra: 10 more lossy-link runs, 12 runs with ufw emulated on master.
3. `realworld.sh --nodes "real1 real2" --repeat 3 --natcheck` under
   `port-restricted`, `full-cone` and `symmetric`, as on 2026-11-14.

## Results

| | campaign 4 (iroh 1.4.1) | campaign 5 (iroh 1.5.0) |
|---|---|---|
| runs with a report | 111 / 111 | 111 / 111 |
| connections lost | 0 | 0 |
| verdicts right / misleading / wrong | 110 / 1 / 0 | 110 / 1 / 0 |
| lab RTT direct / relayed | 40.4 / 80.6 ms | 40.4 / 80.5 ms |
| lab throughput direct / relayed | ~123 / ~35 Mbps | ~123 / ~35 Mbps |
| real-world direct / relayed | 131-147 / 76-84 Mbps | 131-150 / 76-78 Mbps |
| your relay: traffic, drops | 418 MB, 0 | 272 MB, 0 |

`realworld.sh`: 36 of 36 runs direct at the end, RTT 33-37 ms, first reply
0.33-0.38 s (lab node sends) and 0.75-0.80 s (master sends); 2026-11-14 with
1.4.1: 40 of 40, 33-37 ms, 0.37-0.38 / 0.77-0.79 s.

**iroh 1.5.0 changed nothing measurable**, and 1.5.0 clients work through
your 1.4.1 relay without drops.

## Findings

1. Three real-world scenarios went from relay to direct: master was
   reinstalled on 2026-11-11 without ufw. With ufw's filter emulated they
   behave exactly as in campaign 4; not iroh.
2. The fork's relay-recovery commit adds ERROR/WARN lines at shutdown
   (`relay_recv_channel closed`, `All transports failed to receive`) in about
   40 % of process exits; expect them in journald when a testnet listener
   stops. Harmless (only during close); to be fixed in the fork / upstream
   PR. Local repro: 0/30 with 1.4.1 and with upstream main, 7/30 with the fork.
3. Bench: on the lossy link one upload's count came 0.9 s after the bench
   stopped waiting (1 of 13 runs): fixed in golem-p2p `cda49ae` (the wait
   goes on while the listener still acknowledges data); not yet deployed.
4. Harness: `listener_saw` of `lab.sh pair`/`realworld.sh` was empty since
   listeners log requests at debug level; fixed (`ff6fcd1`).

## For puppet (recommendations only)

1. **Relays:** no need to upgrade; 1.4.1 serves 1.5.0 clients fine. If you
   want the relay-server fixes since 1.4.1 (an establish/handshake timeout
   that a client could bypass, #4621; no proxy credentials in debug logs),
   the internet-facing public relay (`:56002`) benefits most. Build:
   `cargo install --git https://github.com/golemfactory/iroh --rev 4ad24ac2761e97fe57791d8d898fd608b78ca0d5 iroh-relay --features server`.
   Keeping the lab relay (`:56000`) on 1.4.1 for a while keeps the
   1.4.1-relay / 1.5.0-client combination covered; please say in `comms/`
   before switching it, since the campaigns compare against it.
2. The lab's own relay on `p2p-natlab` (`~/golem-p2p-lab/bin/iroh-relay`) is
   1.5.0 now (`DEPLOYED` says so). `deploy.sh` replaces it only when given
   `IROH_RELAY_BIN`.
