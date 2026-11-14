# Real-world runs: master <-> lab on puppet, 2026-11-14

`harness/lab/realworld.sh` run **on master** (`65.109.154.126`, Hetzner, no
NAT), against lab nodes on `p2p-natlab` behind puppet's host NAT
(`213.163.75.157`), through puppet's relay `https://213.163.75.157:56000`
and a per-run directory on `p2p-relay1`. Lab topology `real`: `real1` (easy
emulated NAT) and `real2` (symmetric emulated NAT), both `uplink=real`.

    harness/lab/realworld.sh --nodes "real1 real2" --repeat 3 \
        --natcheck --natcheck-local-port 56090 --out <profile>

Private node keys of the runs were deleted before committing (`keys/` keeps
only the node IDs).

## Results

Every run connected and ended on a direct path (`direct_pct=100`), for every
host profile and both directions. Direct RTT 33-37 ms.

| host profile | runs ok | lab node mapping (natcheck, 3 tries each) | first path | last path |
|---|---|---|---|---|
| `port-restricted` | 12 / 12 | `real1` EIM APDF port kept; `real2` APDM APDF | relay or direct | direct |
| `full-cone` | 12 / 12 | same as `port-restricted` | relay or direct | direct |
| `symmetric` | 12 / 12 | `real1` and `real2` APDM APDF | relay or direct | direct |
| `port-restricted`, after the QAD fix | 4 / 4 (1 repeat) | not measured | relay or direct | direct |

Time to the first reply (`t_first`):

| direction | before the fix | after the fix |
|---|---|---|
| lab node sends, master listens | 0.37-0.38 s | 0.37-0.38 s |
| master sends, lab node listens | 3.36-3.41 s | 0.77-0.79 s |

## Findings

1. **Master's side started 3 s late: fixed in `realworld.sh`.** The script gave
   the local node `--relay https://213.163.75.157:56000` without
   `,quic=56001` (`lab.sh` has it). iroh then sent its QUIC address discovery
   probe to its default UDP port 7842, which puppet does not forward; the
   probe timed out after 3 s, the first net report said `udp_v4: false`, and
   `bind` waited for it. Direct paths still came up afterwards. With
   `,quic=56001` the default is fixed; `GPL_REAL_RELAY` still overrides it.
   Side note for the library: a node whose relay's QAD port is blocked pays
   3 s on every start (see `docs/design.md`, relays).
2. **Full-cone (EIF) of the host NAT proven** (`eif-manual.txt`): `p2p-natlab`
   opened a mapping on port 47000 toward the relay VM only; master, which the
   VM never sent to, then sent to `213.163.75.157:47000`. Received under
   `full-cone` (twice), dropped under `port-restricted`.
3. **The host profile does not reach lab nodes' filtering**, as puppet
   predicted: under `full-cone` the lab nodes still show APDF, because the lab
   VM's own masquerade filters first. `symmetric` does reach their mapping
   (`real1` turns APDM).
4. **These runs only cover "one side behind NAT".** Master has public
   addresses and no NAT, so a direct path is always possible as soon as the
   lab node sends to master's address; even symmetric/APDF nodes go direct.
   NAT-to-NAT (both sides behind NAT, e.g. two lab nodes with
   `uplink=real`, or a second NATed outside node) is the harder case and is
   not tested here.

## Files

    <profile>/results.txt     one line per run (summarize.py keys)
    <profile>/natcheck.txt    natcheck of real1/real2 against reflectors on
                              p2p-relay1 (:56090, :56091) and master (:56090)
    <profile>/run<N>-<listener>-<sender>/   listen/send logs of each run
    <profile>.log             the script's own output
    eif-manual.txt            the manual full-cone test
