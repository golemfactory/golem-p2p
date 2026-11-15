# Real-world runs: master <-> lab on puppet, iroh 1.5.0, 2026-11-15

The runs of [2026-11-14-realworld-master](../2026-11-14-realworld-master/README.md)
again, with golem-p2p on **iroh 1.5.0 from the golemfactory/iroh fork** (rev
`4ad24ac2`), right after benchmark campaign 5
([2026-11-15-campaign5](../2026-11-15-campaign5/README.md)).
`harness/lab/realworld.sh` ran **on master** (`65.109.154.126`, Hetzner, no
NAT, no firewall), against lab nodes on `p2p-natlab` behind puppet's host NAT
(`213.163.75.157`), through puppet's relay `https://213.163.75.157:56000`
(iroh-relay **1.4.1**, unchanged) and a per-run directory on `p2p-relay1`.
Lab topology `real`: `real1` (easy emulated NAT) and `real2` (symmetric
emulated NAT), both `uplink=real`. Host profile set with `sudo p2p-hostnat
set <profile>` on puppet's host before each run, `port-restricted` again at
the end (`hostnat-<profile>.txt`).

    harness/lab/realworld.sh --nodes "real1 real2" --repeat 3 \
        --natcheck --natcheck-local-port 56090 --out <profile>

| | |
|---|---|
| golem-p2p | `target/release/golem-p2p` sha256 `cc4a151b…6bb6544` (campaign 5's binary, built from `656d229`; the same bytes as on master's services and puppet's testnet listener), on master and in the lab (`DEPLOYED`: `ff6fcd1`, scripts only) |
| time | 12:30:45-12:51:20 UTC |
| lab's own relay | 1.5.0 (not used here: `uplink=real` nodes use puppet's relay) |

Why also these runs and not only campaign 5's `realworld` step: the
campaign's step runs `bench` under the host's `port-restricted` profile only;
these runs add `natcheck` (the mapping and filtering of the real path), the
`full-cone` and `symmetric` host profiles, and `send`'s time to the first reply
in both directions, which is what the 2026-11-14 runs measured.

Private node keys were deleted before committing (`keys/` keeps only the node
IDs).

## Results

**36 of 36 runs connected and ended on a direct path** (`direct_pct=100`), for
every host profile and both directions; direct RTT 33-37 ms. Same as on
2026-11-14 with iroh 1.4.1 (40 of 40, 33-37 ms).

| host profile | runs ok | lab node mapping (natcheck, 3 tries each) | first path | last path |
|---|---|---|---|---|
| `port-restricted` | 12 / 12 | `real1` EIM APDF port kept; `real2` APDM APDF | relay or direct | direct |
| `full-cone` | 12 / 12 | same as `port-restricted` | relay or direct | direct |
| `symmetric` | 12 / 12 | `real1` and `real2` APDM APDF | relay or direct | direct |

Time to the first reply (`t_first`):

| direction | 2026-11-14 (iroh 1.4.1, after the QAD fix) | 2026-11-15 (iroh 1.5.0) |
|---|---|---|
| lab node sends, master listens | 0.37-0.38 s | 0.33-0.38 s |
| master sends, lab node listens | 0.77-0.79 s | 0.75-0.80 s |

(On 2026-11-14 only one repeat ran after the QAD fix in `realworld.sh`; its
three profile runs before the fix had 3.4 s for master's side.)

## Notes

1. The natcheck results are identical to 2026-11-14 for every profile: the
   host NAT and the lab are unchanged.
2. `listener_saw` (paths of the requests the lab listener served) is back:
   since `9e7afee` the CLI logs requests at debug level only, and
   `lab.sh`/`realworld.sh` now start listeners without `--json` with
   `RUST_LOG=...,golem_p2p::listen=debug` (`ff6fcd1`). The relayed requests
   at the start are 2-3 per run, as before.
3. 9 of master's 18 listener logs (`run*-local-*/listen.log`) end with
   `relay_recv_channel closed` / `All transports failed to receive` at
   shutdown: the iroh fork's new shutdown log lines, harmless (see campaign
   5's `debug/shutdown-logs/`).
4. master's reflector on udp 56090 (`gpl-master-reflector`, until
   2026-11-21) answered the natcheck probes to master; `realworld.sh`'s own
   reflector started next to it (`local-reflector.log`).

## Files

    <profile>/results.txt     one line per run (summarize.py keys)
    <profile>/natcheck.txt    natcheck of real1/real2 against reflectors on
                              p2p-relay1 (:56090, :56091) and master (:56090)
    <profile>/run<N>-<listener>-<sender>/   listen/send logs of each run
    <profile>.log             the script's own output
    hostnat-<profile>.txt     p2p-hostnat output when the profile was set
