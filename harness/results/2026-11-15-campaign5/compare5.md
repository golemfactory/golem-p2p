# Campaign series: 2026-11-07-campaign1 → 2026-11-07-campaign2 → 2026-11-08-campaign3 → 2026-11-08-campaign4 → 2026-11-15-campaign5

All campaigns recomputed from their raw runs with `harness/campaign/summarize.py` (`compare.py --series`), each judged by the rules of its own bench; generated, except the highlights, which come from the last campaign's `notes.json` (`comparison_series`).

* 2026-11-07-campaign1: commit 9c37664
* 2026-11-07-campaign2: commit 711a316 (harness on top of 3fb1e9c; the golem-p2p crates are those of 3fb1e9c)
* 2026-11-08-campaign3: commit d1bc1cd (harness on top of c270dde; the golem-p2p crates are those of c270dde)
* 2026-11-08-campaign4: commit 1484e3a (harness on top of 5c5396a; the golem-p2p crates are those of 5c5396a)
* 2026-11-15-campaign5: commit 656d229 (harness on top of e2ff75b; the golem-p2p crates are those of 31c954c)

## Highlights

* **Verdict runs right / misleading / wrong: 81 / 28 / 2 -> 88 / 23 / 0 -> 110 / 1 / 0 -> 110 / 1 / 0 -> 110 / 1 / 0.** Checks 229 / 41 / 2 -> 252 / 24 / 0 -> 254 / 1 / 0 -> 254 / 1 / 0 -> 251 / 1 / 0 (three relayed real-world scenarios are direct now and have fewer checks). What remains since campaign 3 is one sampling miss of 1 % loss in 3 runs.
* **Outcomes: final direct / relay / failed 49 / 42 / 2 -> 51 / 42 / 0 -> 51 / 42 / 0 -> 51 / 42 / 0 -> 60 / 33 / 0; lost connections 2 -> 0 -> 0 -> 0 -> 0.** The direct/relay outcome of every scenario is the same in all five campaigns except the three *-ufw real-world scenarios in campaign 5, whose firewall is gone (confirmed by debug/ufw-emul); campaign 1's failures were the lossy link's connections closed after request timeouts.
* **Lossy link across the campaigns:** campaign 1: connection closed in 2/3, loss hint 0/3; campaign 2: survives 3/3, relay_fallback_failed 2/3; campaign 3: survives, relay fallback ok 13/13, loss hint 11/13, one upload failed by a race; campaign 4: healthy 13/13, loss hint 9/13, concurrency 13/13 (2 restarts); campaign 5: loss hint 9/13, concurrency 13/13 (1 restart, cut-off requests cancelled), healthy 12/13 (one upload count given up too early, fixed afterwards).
* **Measurements stable across iroh 1.4.1 -> 1.5.0:** lab RTT 40.4 / 80.6 ms and throughput ~123 / ~35 Mbps direct / relayed in all five campaigns; real-world RTT and throughput vary with the ~34 / ~37.5 ms route clusters; slow Hetzner -> lab downloads in 1 / 0 / 2 / 3 / 0 runs; no drops at the real relay.

## Totals

| | 2026-11-07-campaign1 | 2026-11-07-campaign2 | 2026-11-08-campaign3 | 2026-11-08-campaign4 | 2026-11-15-campaign5 |
|---|---|---|---|---|---|
| scenarios / runs / runs with a report | 35 / 111 / 111 | 35 / 111 / 111 | 35 / 111 / 111 | 35 / 111 / 111 | 35 / 111 / 111 |
| verdict runs right / misleading / wrong (by worst finding) | 81 / 28 / 2 | 88 / 23 / 0 | 110 / 1 / 0 | 110 / 1 / 0 | 110 / 1 / 0 |
| verdict checks right / misleading / wrong | 229 / 41 / 2 | 252 / 24 / 0 | 254 / 1 / 0 | 254 / 1 / 0 | 251 / 1 / 0 |
| peer runs: final direct / relay / failed | 49 / 42 / 2 | 51 / 42 / 0 | 51 / 42 / 0 | 51 / 42 / 0 | 60 / 33 / 0 |
| runs whose connection closed during the run | 2 | 0 | 0 | 0 | 0 |
| peer runs with QUIC counters at the end | 91 | 93 | 93 | 93 | 93 |

## Per scenario

Each cell lists the campaigns in order (2026-11-07-campaign1 → 2026-11-07-campaign2 → 2026-11-08-campaign3 → 2026-11-08-campaign4 → 2026-11-15-campaign5). Verdicts = runs right/misleading/wrong by their worst finding; outcome = final path direct/relay/failed per run (failed includes connections gone at the end); RTT = latency-phase p50 pooled over runs; up/down/conc = median throughput (conc: concurrency phase, - = no run measured it); run = bench run duration p50; lost conn = runs with a `disconnect` event.

| scenario | verdicts r/m/w | outcome d/r/f | RTT ms | up Mbps | down Mbps | conc Mbps | run s | lost conn |
|---|---|---|---|---|---|---|---|---|
| lab-public-easy | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.5 → 123.5 → 123.0 → 123.0 → 122.9 | 122.4 → 123.6 → 122.9 → 123.9 → 123.1 | 156.0 → 156.0 → 155.8 → 209.4 → 238.2 | 20.3 → 20.3 → 22.3 → 22.2 → 22.1 | 0 → 0 → 0 → 0 → 0 |
| lab-easy-easy | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.0 → 122.7 → 122.5 → 123.3 → 123.4 | 123.7 → 123.6 → 123.7 → 122.4 → 122.9 | 156.4 → 156.1 → 156.0 → 226.8 → 235.9 | 20.3 → 20.3 → 22.3 → 22.2 → 22.1 | 0 → 0 → 0 → 0 → 0 |
| lab-sym-easy | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.1 → 122.8 → 122.6 → 121.9 → 123.8 | 123.6 → 123.3 → 122.7 → 123.0 → 122.4 | 156.2 → 156.2 → 155.9 → 219.3 → 233.9 | 20.4 → 20.4 → 22.4 → 22.3 → 22.3 | 0 → 0 → 0 → 0 → 0 |
| lab-easy-sym | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.4 → 123.5 → 123.3 → 123.5 → 123.2 | 122.7 → 124.3 → 123.5 → 123.1 → 123.0 | 156.0 → 156.1 → 156.2 → 207.4 → 232.7 | 12.6 → 12.6 → 14.6 → 14.5 → 14.5 | 0 → 0 → 0 → 0 → 0 |
| lab-pr-pr | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.5 → 40.4 → 40.4 → 40.3 | 122.2 → 123.0 → 121.8 → 122.4 → 122.8 | 123.6 → 123.4 → 123.6 → 123.7 → 123.8 | 156.1 → 156.1 → 156.0 → 189.8 → 229.6 | 20.4 → 20.4 → 22.4 → 22.3 → 22.3 | 0 → 0 → 0 → 0 → 0 |
| lab-sym-sym | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 80.6 → 80.6 → 80.6 → 80.6 → 80.6 | 35.4 → 35.6 → 35.4 → 35.4 → 35.4 | 35.1 → 35.4 → 34.9 → 35.1 → 35.3 | 78.0 → 78.0 → 78.1 → 78.1 → 77.2 | 13.2 → 13.2 → 15.2 → 15.2 → 15.2 | 0 → 0 → 0 → 0 → 0 |
| lab-cgnat-easy | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.5 → 123.2 → 123.6 → 122.8 → 122.0 | 123.7 → 123.5 → 123.3 → 123.7 → 121.8 | 156.1 → 156.1 → 156.0 → 227.5 → 226.1 | 12.7 → 12.7 → 14.7 → 14.6 → 14.6 | 0 → 0 → 0 → 0 → 0 |
| lab-double-easy | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 123.0 → 124.0 → 124.2 → 122.2 → 122.2 | 123.6 → 123.9 → 124.4 → 123.9 → 122.8 | 155.8 → 156.0 → 155.9 → 223.2 → 250.1 | 12.6 → 12.6 → 14.6 → 14.5 → 14.5 | 0 → 0 → 0 → 0 → 0 |
| lab-easy-udpb | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 80.6 → 80.6 → 80.6 → 80.6 → 80.5 | 35.6 → 35.6 → 35.7 → 35.6 → 35.6 | 35.2 → 35.2 → 35.4 → 35.4 → 35.3 | 78.2 → 74.3 → 74.2 → 75.9 → 76.6 | 15.8 → 15.8 → 17.8 → 17.8 → 17.8 | 0 → 0 → 0 → 0 → 0 |
| lab-udpb-easy | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 80.6 → 80.6 → 80.7 → 80.6 → 80.5 | 35.6 → 35.6 → 35.4 → 35.7 → 35.5 | 35.2 → 35.3 → 35.4 → 35.3 → 35.0 | 78.1 → 74.2 → 74.2 → 76.2 → 77.1 | 13.2 → 13.2 → 15.2 → 15.2 → 15.2 | 0 → 0 → 0 → 0 → 0 |
| lab-pr-sym | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 80.7 → 80.6 → 80.6 → 80.6 → 80.5 | 35.6 → 35.3 → 35.4 → 35.7 → 35.3 | 35.4 → 35.1 → 35.2 → 35.3 → 35.6 | 78.2 → 74.3 → 74.2 → 80.6 → 77.5 | 13.2 → 13.2 → 15.2 → 15.2 → 15.2 | 0 → 0 → 0 → 0 → 0 |
| lab-rc-sym | 3/0/0 → 1/2/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 122.7 → 122.8 → 123.2 → 122.6 → 122.2 | 123.3 → 122.7 → 122.9 → 123.3 → 122.0 | 156.1 → 156.1 → 155.9 → 201.6 → 257.9 | 12.7 → 12.7 → 14.7 → 14.6 → 14.6 | 0 → 0 → 0 → 0 → 0 |
| lab-netem-pr | 0/1/2 → 1/2/0 → 2/1/0 → 2/1/0 → 2/1/0 | 1/0/2 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 120.4 → 120.4 → 120.4 → 120.4 → 120.3 | 21.8 → 18.8 → 1.5 → 2.1 → 15.9 | 3.9 → 1.2 → 2.6 → 4.3 → 10.7 | 2.4 → - → 1.0 → 0.9 → 2.4 | 71.9 → 63.6 → 69.0 → 61.6 → 66.6 | 2 → 0 → 0 → 0 → 0 |
| lab-idle30-pr | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.5 → 40.4 → 40.4 → 40.4 | 122.2 → 123.1 → 122.3 → 123.3 → 122.1 | 123.0 → 122.7 → 123.8 → 123.9 → 124.0 | 156.3 → 156.0 → 156.1 → 223.3 → 249.0 | 72.7 → 72.7 → 74.7 → 74.6 → 74.6 | 0 → 0 → 0 → 0 → 0 |
| lab-idle5-pr | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 122.7 → 122.8 → 121.9 → 122.9 → 121.4 | 122.8 → 123.4 → 123.6 → 123.4 → 121.6 | 156.2 → 156.3 → 156.2 → 219.1 → 237.2 | 72.8 → 72.7 → 74.7 → 74.6 → 74.6 | 0 → 0 → 0 → 0 → 0 |
| lab-natflush-pr | 3/0/0 → 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 40.4 → 40.4 → 40.4 → 40.4 → 40.4 | 121.8 → 121.4 → 122.7 → 122.0 → 122.8 | 124.3 → 123.4 → 122.9 → 123.6 → 123.4 | 156.2 → 156.1 → 156.0 → 240.8 → 233.2 | 72.8 → 72.8 → 74.8 → 74.6 → 74.6 | 0 → 0 → 0 → 0 → 0 |
| rw-easy-to-ufw | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 33.9 → 37.1 → 34.5 → 34.3 → 34.3 | 145.4 → 134.2 → 140.4 → 144.1 → 146.3 | 146.9 → 133.3 → 134.5 → 15.6 → 146.4 | 280.4 → 261.3 → 279.6 → 15.2 → 307.2 | 11.6 → 11.8 → 13.8 → 22.3 → 13.6 | 0 → 0 → 0 → 0 → 0 |
| rw-easy-to-open | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 33.7 → 37.1 → 37.2 → 37.2 → 37.2 | 147.6 → 133.9 → 132.8 → 134.1 → 134.3 | 147.7 → 132.5 → 132.9 → 133.2 → 133.2 | 308.2 → 262.5 → 155.9 → 295.2 → 305.2 | 17.7 → 17.9 → 20.0 → 20.0 → 19.9 | 0 → 0 → 0 → 0 → 0 |
| rw-symmetric-to-ufw | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 3/0/0 | 37.7 → 37.3 → 35.1 → 37.2 → 34.2 | 77.5 → 78.6 → 82.1 → 77.3 → 147.8 | 76.7 → 76.8 → 80.3 → 76.8 → 146.6 | 155.3 → 155.3 → 154.5 → 177.0 → 326.6 | 11.4 → 11.4 → 13.4 → 13.3 → 13.6 | 0 → 0 → 0 → 0 → 0 |
| rw-symmetric-to-open | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 34.1 → 34.1 → 36.9 → 34.2 → 34.6 | 145.9 → 147.3 → 134.4 → 141.1 → 146.3 | 146.2 → 147.5 → 134.8 → 131.9 → 147.8 | 280.0 → 280.0 → 279.9 → 276.6 → 304.6 | 11.6 → 11.6 → 13.7 → 13.8 → 13.6 | 0 → 0 → 0 → 0 → 0 |
| rw-udp-blocked-to-ufw | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 37.8 → 34.2 → 37.4 → 34.7 → 38.0 | 77.3 → 85.5 → 77.3 → 83.2 → 76.8 | 75.8 → 85.8 → 76.4 → 82.4 → 76.7 | 155.1 → 155.2 → 154.8 → 168.9 → 164.5 | 14.1 → 14.0 → 16.1 → 16.1 → 16.1 | 0 → 0 → 0 → 0 → 0 |
| rw-udp-blocked-to-open | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 37.4 → 37.5 → 37.7 → 34.2 → 37.6 | 77.4 → 77.0 → 76.5 → 84.3 → 78.1 | 77.7 → 77.0 → 76.9 → 84.1 → 78.1 | 155.0 → 155.1 → 131.1 → 155.1 → 168.9 | 14.1 → 14.1 → 16.2 → 16.0 → 16.1 | 0 → 0 → 0 → 0 → 0 |
| rw-ufw-to-easy | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 3/0/0 | 37.3 → 34.5 → 37.1 → 34.3 → 37.3 | 77.7 → 83.9 → 75.8 → 82.4 → 134.5 | 78.1 → 85.0 → 79.2 → 83.9 → 134.9 | 155.1 → 154.1 → 154.6 → 188.5 → 259.7 | 11.9 → 11.8 → 13.9 → 13.8 → 14.3 | 0 → 0 → 0 → 0 → 0 |
| rw-open-to-easy | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 33.9 → 37.2 → 34.0 → 33.8 → 37.3 | 146.1 → 134.9 → 145.6 → 144.2 → 132.9 | 145.8 → 134.5 → 146.5 → 147.1 → 134.6 | 264.3 → 156.2 → 280.0 → 300.4 → 270.6 | 12.2 → 12.5 → 14.3 → 14.2 → 14.3 | 0 → 0 → 0 → 0 → 0 |
| rw-ufw-to-symmetric | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 3/0/0 | 34.8 → 37.9 → 37.1 → 37.5 → 34.2 | 83.6 → 75.9 → 77.7 → 76.8 → 146.4 | 85.1 → 77.6 → 78.8 → 77.0 → 147.0 | 153.4 → 152.9 → 154.6 → 160.1 → 294.2 | 11.8 → 11.9 → 13.9 → 13.9 → 14.2 | 0 → 0 → 0 → 0 → 0 |
| rw-open-to-symmetric | 0/3/0 → 2/1/0 → 3/0/0 → 3/0/0 → 3/0/0 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 36.9 → 36.9 → 37.0 → 34.5 → 37.4 | 135.4 → 134.9 → 136.5 → 142.8 → 134.3 | 135.2 → 134.2 → 134.8 → 145.2 → 134.0 | 278.5 → 156.3 → 156.2 → 295.4 → 278.9 | 12.3 → 12.4 → 14.4 → 14.3 → 14.3 | 0 → 0 → 0 → 0 → 0 |
| startup-hetzner-qad-56001 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 0.7 → 0.7 → 0.7 → 0.8 → 0.7 | - → - → - → - → - |
| startup-lab-easy-qad-56001 | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 0.2 → 0.3 → 0.2 → 0.2 → 0.3 | - → - → - → - → - |
| startup-hetzner-qad-7842 | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 3.3 → 3.4 → 3.4 → 3.4 → 3.3 | - → - → - → - → - |
| startup-lab-easy-qad-7842 | 0/3/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 3.0 → 3.0 → 3.0 → 3.0 → 3.0 | - → - → - → - → - |
| startup-hetzner-qad-off | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 0.7 → 0.7 → 0.8 → 0.8 → 0.7 | - → - → - → - → - |
| startup-lab-easy-qad-off | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | - → - → - → - → - | 0.2 → 0.2 → 0.3 → 0.2 → 0.3 | - → - → - → - → - |
| relay-single | 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 → 3/0/0 | 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 → 0/3/0 | 74.5 → 67.6 → 71.1 → 74.1 → 71.7 | 38.7 → 42.4 → 40.5 → 38.2 → 40.1 | 38.2 → 42.1 → 40.3 → 38.3 → 40.5 | 70.5 → 73.9 → 78.0 → 70.5 → 86.2 | 13.0 → 12.8 → 14.8 → 15.0 → 14.8 | 0 → 0 → 0 → 0 → 0 |
| relay-parallel2 | 4/0/0 → 4/0/0 → 4/0/0 → 4/0/0 → 4/0/0 | 0/4/0 → 0/4/0 → 0/4/0 → 0/4/0 → 0/4/0 | 71.1 → 67.4 → 72.2 → 71.6 → 71.4 | 40.8 → 42.8 → 39.4 → 40.5 → 40.6 | 40.2 → 41.3 → 38.8 → 40.0 → 40.4 | - → - → - → - → - | 12.4 → 12.3 → 14.5 → 14.4 → 14.4 | 0 → 0 → 0 → 0 → 0 |
| relay-parallel4 | 8/0/0 → 8/0/0 → 8/0/0 → 8/0/0 → 8/0/0 | 0/8/0 → 0/8/0 → 0/8/0 → 0/8/0 → 0/8/0 | 68.8 → 72.3 → 72.1 → 71.5 → 72.0 | 41.2 → 39.6 → 39.5 → 40.5 → 39.9 | 40.8 → 39.1 → 39.7 → 39.7 → 39.6 | - → - → - → - → - | 12.3 → 12.4 → 14.5 → 14.4 → 14.4 | 0 → 0 → 0 → 0 → 0 |

## Misleading and wrong verdict checks per campaign

* `lab-sym-easy`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `lab-pr-pr`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `lab-sym-sym`: misleading:hard_nat_missed x3, misleading:relay_reason_vague x3 → none → none → none → none
* `lab-cgnat-easy`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `lab-pr-sym`: misleading:hard_nat_missed x3, misleading:relay_reason_vague x3 → none → none → none → none
* `lab-rc-sym`: none → misleading:packet_loss_unshaped x2 → none → none → none
* `lab-netem-pr`: misleading:connection_closed_unexplained x2, misleading:packet_loss_missed x3, wrong:says_relay x2 → misleading:packet_loss_missed, misleading:relay_fallback_failed_but_works x2 → misleading:packet_loss_missed → misleading:packet_loss_missed → misleading:packet_loss_missed
* `lab-idle30-pr`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `lab-idle5-pr`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `lab-natflush-pr`: none → misleading:packet_loss_unshaped x3 → none → none → none
* `rw-symmetric-to-ufw`: misleading:hard_nat_missed x3, misleading:relay_reason_vague x3 → none → none → none → none
* `rw-ufw-to-easy`: misleading:two_nats_without_nat x3 → none → none → none → none
* `rw-open-to-easy`: misleading:packet_loss_congestion x3 → none → none → none → none
* `rw-ufw-to-symmetric`: misleading:two_nats_without_nat x3 → none → none → none → none
* `rw-open-to-symmetric`: misleading:packet_loss_congestion x3 → misleading:packet_loss_unshaped → none → none → none
* `startup-hetzner-qad-7842`: misleading:udp_blocked_wrong_qad_port x3 → none → none → none → none
* `startup-lab-easy-qad-7842`: misleading:udp_blocked_wrong_qad_port x3 → none → none → none → none
