# Campaign comparison: 2026-11-08-campaign4 → 2026-11-15-campaign5

Base: commit 1484e3a (harness on top of 5c5396a; the golem-p2p crates are those of 5c5396a); new: commit 656d229 (harness on top of e2ff75b; the golem-p2p crates are those of 31c954c). Both recomputed from the raw runs with `harness/campaign/summarize.py` (`compare.py`); generated, except the highlights, which come from the new campaign's `notes.json` (`comparison`).

## Highlights

* **iroh 1.4.1 -> 1.5.0 (fork with relay recovery fixes): nothing measurable changed.** Same outcome in every scenario whose environment did not change, 0 connections lost, QUIC counters in 93/93; lab RTT 40.4 / 80.6 ms, throughput ~123 / ~35 Mbps, connect/hole-punching and bind times identical, also with the lab's relay on 1.5.0; real-world numbers within the route clusters; the 1.4.1 real relay carried the 1.5.0 clients without drops. Verdicts 110 right / 1 misleading / 0 wrong runs, as in campaigns 3 and 4.
* **Three outcome changes (relay -> direct) are this machine's firewall:** rw-symmetric-to-ufw, rw-ufw-to-easy and rw-ufw-to-symmetric go direct, and rw-easy-to-ufw is direct from the first packet, because the reinstalled build machine has no ufw; final paths 60 direct / 33 relay (was 51 / 42). With ufw's filtering emulated on the same build (debug/ufw-emul, 12 runs) campaign 4's outcomes come back exactly.
* **Faster binds and no slow downloads on this machine:** load < 0.4 (campaign 4: 24-49 from other sessions): startup-hetzner binds 667 -> 588 ms, relay-single 584 -> 507 ms; no slow Hetzner -> lab download (campaign 4: 3). The regression flag on rw-open-to-easy RTT (33.8 -> 37.3 ms) is the route cluster.
* **New: ERROR/WARN log lines at endpoint shutdown (iroh fork),** in 41 % of process exits (84 of 204; 'relay_recv_channel closed', 'All transports failed to receive'), only during close, no functional effect. Local repro: 0/30 with iroh 1.4.1 and with upstream main, 7/30 with the fork rev: from its relay-recovery commit (relay actors end before the QUIC endpoint stops polling). To fix in the fork/upstream PR.
* **Lossy link:** packet_loss 9/13 and relay_compare ok 13/13 as in campaign 4; concurrency measured 13/13, 1 restart (was 2), cut-off requests now cancelled on the wire (late requests at the listener: 2/13 runs, <= 0.1 s; campaign 4: 7/13, up to 3.9 s); one upload whose count came 0.9 s after the bench stopped waiting (exit 5, debug run): bench bug, fixed in cda49ae.

## Totals

| | base | new |
|---|---|---|
| scenarios / runs / runs with a report | 35 / 111 / 111 | 35 / 111 / 111 |
| peer runs: final direct / relay / failed | 51 / 42 / 0 | 60 / 33 / 0 |
| runs whose connection closed during the run | 0 | 0 |
| peer runs with QUIC counters at the end | 93 | 93 |
| verdict runs right (by worst finding) | 110 | 110 |
| verdict runs misleading (by worst finding) | 1 | 1 |
| verdict runs wrong (by worst finding) | 0 | 0 |
| verdict checks right | 254 | 251 |
| verdict checks misleading | 1 | 1 |
| verdict checks wrong | 0 | 0 |

## Per scenario (base → new)

Outcome = final path direct/relay/failed per run (failed includes connections gone at the end). RTT = latency-phase p50 pooled over runs; up/down/conc = median throughput (conc: concurrency phase); bind = `ready.bind_ms` p50; run = bench run duration p50; lost conn = runs with a `disconnect` event; verdicts = runs right/misleading/wrong by their worst finding.

| scenario | outcome d/r/f | RTT ms | up Mbps | down Mbps | conc Mbps | bind ms | run s | lost conn | verdicts r/m/w |
|---|---|---|---|---|---|---|---|---|---|
| lab-public-easy | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 123.0 → 122.9 | 123.9 → 123.1 | 209.4 → 238.2 | 609 → 608 | 22.2 → 22.1 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-easy-easy | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 123.3 → 123.4 | 122.4 → 122.9 | 226.8 → 235.9 | 609 → 608 | 22.2 → 22.1 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-sym-easy | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 121.9 → 123.8 | 123.0 → 122.4 | 219.3 → 233.9 | 609 → 609 | 22.3 → 22.3 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-easy-sym | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 123.5 → 123.2 | 123.1 → 123.0 | 207.4 → 232.7 | 609 → 610 | 14.5 → 14.5 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-pr-pr | 3/0/0 → 3/0/0 | 40.4 → 40.3 | 122.4 → 122.8 | 123.7 → 123.8 | 189.8 → 229.6 | 608 → 608 | 22.3 → 22.3 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-sym-sym | 0/3/0 → 0/3/0 | 80.6 → 80.6 | 35.4 → 35.4 | 35.1 → 35.3 | 78.1 → 77.2 | 609 → 609 | 15.2 → 15.2 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-cgnat-easy | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 122.8 → 122.0 | 123.7 → 121.8 | 227.5 → 226.1 | 609 → 608 | 14.6 → 14.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-double-easy | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 122.2 → 122.2 | 123.9 → 122.8 | 223.2 → 250.1 | 609 → 608 | 14.5 → 14.5 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-easy-udpb | 0/3/0 → 0/3/0 | 80.6 → 80.5 | 35.6 → 35.6 | 35.4 → 35.3 | 75.9 → 76.6 | 3207 → 3207 | 17.8 → 17.8 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-udpb-easy | 0/3/0 → 0/3/0 | 80.6 → 80.5 | 35.7 → 35.5 | 35.3 → 35.0 | 76.2 → 77.1 | 608 → 608 | 15.2 → 15.2 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-pr-sym | 0/3/0 → 0/3/0 | 80.6 → 80.5 | 35.7 → 35.3 | 35.3 → 35.6 | 80.6 → 77.5 | 608 → 608 | 15.2 → 15.2 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-rc-sym | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 122.6 → 122.2 | 123.3 → 122.0 | 201.6 → 257.9 | 608 → 609 | 14.6 → 14.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-netem-pr | 3/0/0 → 3/0/0 | 120.4 → 120.3 | 2.1 → 15.9 | 4.3 → 10.7 | 0.9 → 2.4 | 1329 → 1329 | 61.6 → 66.6 | 0 → 0 | 2/1/0 → 2/1/0 |
| lab-idle30-pr | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 123.3 → 122.1 | 123.9 → 124.0 | 223.3 → 249.0 | 608 → 609 | 74.6 → 74.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-idle5-pr | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 122.9 → 121.4 | 123.4 → 121.6 | 219.1 → 237.2 | 609 → 608 | 74.6 → 74.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| lab-natflush-pr | 3/0/0 → 3/0/0 | 40.4 → 40.4 | 122.0 → 122.8 | 123.6 → 123.4 | 240.8 → 233.2 | 609 → 608 | 74.6 → 74.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-easy-to-ufw | 3/0/0 → 3/0/0 | 34.3 → 34.3 | 144.1 → 146.3 | 15.6 → 146.4 | 15.2 → 307.2 | 249 → 249 | 22.3 → 13.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-easy-to-open | 3/0/0 → 3/0/0 | 37.2 → 37.2 | 134.1 → 134.3 | 133.2 → 133.2 | 295.2 → 305.2 | 248 → 249 | 20.0 → 19.9 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-symmetric-to-ufw | 0/3/0 → 3/0/0 | 37.2 → 34.2 | 77.3 → 147.8 | 76.8 → 146.6 | 177.0 → 326.6 | 250 → 249 | 13.3 → 13.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-symmetric-to-open | 3/0/0 → 3/0/0 | 34.2 → 34.6 | 141.1 → 146.3 | 131.9 → 147.8 | 276.6 → 304.6 | 249 → 249 | 13.8 → 13.6 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-udp-blocked-to-ufw | 0/3/0 → 0/3/0 | 34.7 → 38.0 | 83.2 → 76.8 | 82.4 → 76.7 | 168.9 → 164.5 | 3007 → 3007 | 16.1 → 16.1 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-udp-blocked-to-open | 0/3/0 → 0/3/0 | 34.2 → 37.6 | 84.3 → 78.1 | 84.1 → 78.1 | 155.1 → 168.9 | 3007 → 3007 | 16.0 → 16.1 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-ufw-to-easy | 0/3/0 → 3/0/0 | 34.3 → 37.3 | 82.4 → 134.5 | 83.9 → 134.9 | 188.5 → 259.7 | 602 → 592 | 13.8 → 14.3 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-open-to-easy | 3/0/0 → 3/0/0 | 33.8 → 37.3 | 144.2 → 132.9 | 147.1 → 134.6 | 300.4 → 270.6 | 611 → 591 | 14.2 → 14.3 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-ufw-to-symmetric | 0/3/0 → 3/0/0 | 37.5 → 34.2 | 76.8 → 146.4 | 77.0 → 147.0 | 160.1 → 294.2 | 624 → 581 | 13.9 → 14.2 | 0 → 0 | 3/0/0 → 3/0/0 |
| rw-open-to-symmetric | 3/0/0 → 3/0/0 | 34.5 → 37.4 | 142.8 → 134.3 | 145.2 → 134.0 | 295.4 → 278.9 | 611 → 592 | 14.3 → 14.3 | 0 → 0 | 3/0/0 → 3/0/0 |
| startup-hetzner-qad-56001 | - → - | - → - | - → - | - → - | - → - | 667 → 588 | 0.8 → 0.7 | - → - | 3/0/0 → 3/0/0 |
| startup-lab-easy-qad-56001 | - → - | - → - | - → - | - → - | - → - | 248 → 249 | 0.2 → 0.3 | - → - | 3/0/0 → 3/0/0 |
| startup-hetzner-qad-7842 | - → - | - → - | - → - | - → - | - → - | 3248 → 3197 | 3.4 → 3.3 | - → - | 3/0/0 → 3/0/0 |
| startup-lab-easy-qad-7842 | - → - | - → - | - → - | - → - | - → - | 3008 → 3007 | 3.0 → 3.0 | - → - | 3/0/0 → 3/0/0 |
| startup-hetzner-qad-off | - → - | - → - | - → - | - → - | - → - | 666 → 596 | 0.8 → 0.7 | - → - | 3/0/0 → 3/0/0 |
| startup-lab-easy-qad-off | - → - | - → - | - → - | - → - | - → - | 248 → 249 | 0.2 → 0.3 | - → - | 3/0/0 → 3/0/0 |
| relay-single | 0/3/0 → 0/3/0 | 74.1 → 71.7 | 38.2 → 40.1 | 38.3 → 40.5 | 70.5 → 86.2 | 584 → 507 | 15.0 → 14.8 | 0 → 0 | 3/0/0 → 3/0/0 |
| relay-parallel2 | 0/4/0 → 0/4/0 | 71.6 → 71.4 | 40.5 → 40.6 | 40.0 → 40.4 | - → - | 532 → 496 | 14.4 → 14.4 | 0 → 0 | 4/0/0 → 4/0/0 |
| relay-parallel4 | 0/8/0 → 0/8/0 | 71.5 → 72.0 | 40.5 → 39.9 | 39.7 → 39.6 | - → - | 525 → 503 | 14.4 → 14.4 | 0 → 0 | 8/0/0 → 8/0/0 |

## Flags

Changes beyond the noise thresholds (relative and absolute: RTT 10 % and 3 ms, throughput (also concurrency) 25 % and 5 Mbps, bind 25 % and 100 ms, run duration 25 % and 3 s), outcome and verdict changes.

* `lab-rc-sym`: improvement: concurrency_mbps 201.58 -> 257.904 Mbps
* `lab-netem-pr`: improvement: upload_mbps 2.072 -> 15.945 Mbps; improvement: download_mbps 4.273 -> 10.748 Mbps
* `rw-easy-to-ufw`: improvement: download_mbps 15.611 -> 146.351 Mbps; improvement: concurrency_mbps 15.237 -> 307.234 Mbps; improvement: run_duration_s_p50 22.3 -> 13.6 s
* `rw-symmetric-to-ufw`: improvement: outcome d/r/f 0/3/0 -> 3/0/0; improvement: upload_mbps 77.269 -> 147.759 Mbps; improvement: download_mbps 76.761 -> 146.562 Mbps; improvement: concurrency_mbps 177.011 -> 326.629 Mbps
* `rw-ufw-to-easy`: improvement: outcome d/r/f 0/3/0 -> 3/0/0; improvement: upload_mbps 82.384 -> 134.477 Mbps; improvement: download_mbps 83.856 -> 134.885 Mbps; improvement: concurrency_mbps 188.502 -> 259.691 Mbps
* `rw-open-to-easy`: regression: rtt_ms_p50 33.841 -> 37.28 ms
* `rw-ufw-to-symmetric`: improvement: outcome d/r/f 0/3/0 -> 3/0/0; improvement: upload_mbps 76.794 -> 146.386 Mbps; improvement: download_mbps 76.978 -> 146.982 Mbps; improvement: concurrency_mbps 160.08 -> 294.178 Mbps

## Misleading and wrong verdict checks (base → new)

* `lab-netem-pr`: misleading:packet_loss_missed → misleading:packet_loss_missed
