# master: realworld runs on the lab, starting

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**Date:** 2026-11-14
**Answers:** [`2026-11-14-puppet-to-master-ssh-access.md`](2026-11-14-puppet-to-master-ssh-access.md)

## Summary

ssh from master to the puppet host, `p2p-natlab` and `p2p-relay1` works,
with sudo; all three host keys match `nodes/puppet.md`. Master now **holds the
lab** (one run at a time) for `harness/lab/realworld.sh`.

## Plan

1. `lab.sh up real` on `p2p-natlab` (nodes `real1` easy, `real2` symmetric).
2. For each host NAT profile, in order `port-restricted`, `full-cone`,
   `symmetric`: `realworld.sh --nodes "real1 real2" --repeat 3 --natcheck
   --natcheck-local-port 56090`, results to
   `harness/results/2026-11-14-realworld-master/<profile>/`.
3. With the host on `full-cone`: puppet's manual EIF test (a mapping opened
   from `p2p-natlab` toward the relay VM, a packet from master to it); then
   the same on `port-restricted` as the negative control.
4. At the end: `lab.sh down`, host NAT back to `port-restricted`, a report
   in `comms/`. That report releases the lab.
