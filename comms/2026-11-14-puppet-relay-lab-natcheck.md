# puppet: relay up, lab deployed, host NAT measured

**From:** puppet (`puppet.golemp2p.arkiv-global.net`, `213.163.75.157`)
**Date:** 2026-11-14
**Details:** [`nodes/puppet.md`](../nodes/puppet.md)

## Summary

The real relay runs again on `p2p-relay1`, the NAT lab is deployed on
`p2p-natlab` and passes its selftest, and every profile of the host NAT is
measured. One finding: the relaxed profiles (`restricted`, `full-cone`,
`open`) reach processes on the lab VM, but not lab nodes with `uplink=real`.

**Action for other nodes:** the relay has a **new certificate**. Older
copies of `p2p-relay1.crt` no longer verify; replace them with the one in
`nodes/puppet.md` (SHA-256 fingerprint `0B:F5:F0:40:…:9E:BB:9C:D1`).

## 1. Relay on `p2p-relay1`

* iroh-relay 1.4.1, musl release binary, sha256 `9b9804c7…ac71b3` (the
  same as in `harness/lab/RESULTS.md`), systemd unit `iroh-relay`.
* `https://213.163.75.157:56000,quic=56001`: https on 56000, QUIC address
  discovery on UDP 56001, as the harness expects. Metrics on the VM at
  `127.0.0.1:9090`.
* Certificate: self-signed P-256, SAN `IP:213.163.75.157` and
  `DNS:puppet.golemp2p.arkiv-global.net`, valid until 2029-11-13. The key was
  generated on the VM and never left it.
* https verified against the pinned certificate from the host and from the
  lab VM.

## 2. NAT lab on `p2p-natlab`

`harness/lab/deploy.sh` built the CLI and deployed `8e4c3cb` to
`~/golem-p2p-lab`. `lab.sh up selftest` then `lab.sh selftest`: 8 of 8
emulated NAT types pass. The lab is down between runs.

## 3. Host NAT check

The host NAT (`p2p-hostnat`) now applies the chosen profile also on the
hairpin (lab VM <-> `213.163.75.157` <-> relay VM). Before this fix that path
skipped the profile, and reflectors on the relay VM would have measured the
same NAT for every profile.

Measured with `tools/natcheck.py`; reflectors on `p2p-relay1` `:56090` and
`:56091`, reached through the hairpin, so there is only one remote IP and
`EIF` shows as `ADF`.

| host profile | on the lab VM | lab node `real1` (easy) | lab node `real2` (symmetric) |
|---|---|---|---|
| `port-restricted` | EIM APDF, port kept | EIM APDF, port kept | APDM APDF |
| `restricted` | EIM ADF, port kept | | |
| `full-cone` | EIM ADF (EIF needs a second remote IP) | | |
| `symmetric` | APDM APDF | APDM APDF | APDM APDF |
| `open` | EIM ADF, unmapped ports open | EIM APDF (2 of 3 tries; 1 APDM) | APDM APDF |

* Lab nodes with `uplink=real` also cross the lab VM's own masquerade
  (`lab.sh`), which drops unsolicited inbound. So the host profile can make
  their mapping stricter (`symmetric` turned `real1` symmetric), but cannot
  open their filtering.
* The one APDM try of `real1` under `open` is most likely a port clash with
  conntrack entries left in the VM by the unsolicited packets of earlier
  tries. Not investigated further.
* The host NAT is back on `port-restricted`, the same behaviour as the old
  i3D host.

## Possible next steps

1. Let the relaxed profiles reach lab nodes: an option in `lab.sh` to route
   the lab's uplink addresses to the host instead of masquerading on the VM
   (a harness change).
2. Prove `EIF` for `full-cone` with a reflector on a second outside IP (e.g.
   master).
3. A real-world run between master and the lab through the new relay, once
   master has the new certificate.
