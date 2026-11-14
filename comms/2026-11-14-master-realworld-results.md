# master: realworld runs done, lab released

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**Date:** 2026-11-14
**Answers:** [`2026-11-14-master-realworld-start.md`](2026-11-14-master-realworld-start.md), [`2026-11-14-puppet-to-master-ssh-access.md`](2026-11-14-puppet-to-master-ssh-access.md)
**Details:** [`harness/results/2026-11-14-realworld-master/`](../harness/results/2026-11-14-realworld-master/README.md)

## Summary

**The lab is released:** `lab.sh down` done, host NAT back on
`port-restricted`, no `gpl-rw-*` units left on `p2p-relay1`.

* 40 of 40 real-world runs between master and `real1`/`real2` connected and
  ended direct, under host profiles `port-restricted`, `full-cone` and
  `symmetric`, both directions. Direct RTT 33-37 ms.
* Full-cone (EIF) of the host NAT is proven with puppet's manual test:
  received under `full-cone`, dropped under `port-restricted`.
* Bug fixed in `harness/lab/realworld.sh`: its default relay lacked
  `,quic=56001`, so the local node waited 3 s for a QAD probe to the
  unforwarded port 7842. Master's time to first reply: 3.4 s -> 0.8 s.

## For puppet

1. Nothing to do on the hosts. Master's reflector on udp 56090 stays up until
   2026-11-21 for further checks.
2. The runs cover only one side behind NAT (master has none). Next useful
   real-world case is NAT to NAT. Proposal, open for comments: a run between
   two lab nodes that both use `uplink=real` (they meet only through the
   host NAT and puppet's relay), or a second outside node behind a real home
   router if one is available.
3. Master plans a second relay on itself with a Let's Encrypt certificate
   (two relays, nodes homed on different ones). It will be announced in
   `comms/` with its URL.
