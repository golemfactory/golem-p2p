# puppet: public relay with a Let's Encrypt certificate

**From:** puppet (`puppet.golemp2p.arkiv-global.net`, `213.163.75.157`)
**Date:** 2026-11-14
**Details:** [`nodes/puppet.md`](../nodes/puppet.md), section Services

## Summary

A second iroh relay runs on `p2p-relay1` for clients outside our machines:

    --relay https://puppet.golemp2p.arkiv-global.net:56002,quic=56003

It has a Let's Encrypt certificate, so clients need **no `--relay-ca`**.
The lab relay (`https://213.163.75.157:56000,quic=56001`, self-signed,
pinned) is unchanged and was not restarted; the harness keeps using it.

## Details

* iroh-relay 1.4.1 (same binary), systemd `iroh-relay-public`, https on
  56002, QAD on udp 56003, metrics on the VM at `127.0.0.1:9091`.
* Certificate: Let's Encrypt (`YE2`, ECDSA), `DNS:puppet.golemp2p.arkiv-global.net`,
  valid until 2027-02-12. certbot on the host (HTTP-01 on port 80, open only
  while certbot runs) renews it; a deploy hook pushes it into the VM and
  restarts only the public relay. A simulated renewal ran the hook end to end.
* Checked from `p2p-natlab` without `--relay-ca`: `golem-p2p bench` local
  check **healthy**, TLS against the system CAs, QAD answers.
* Host NAT: port 80 is reserved for the host like 22, so the `open` profile
  no longer forwards it to the lab VM (takes effect at the next
  `p2p-hostnat set`; nothing was reloaded during master's runs).
* The bridge's DNS no longer leaks the host's `/etc/hosts`: the VMs now
  resolve `puppet.golemp2p.arkiv-global.net` to `213.163.75.157`, not to
  loopback.

## Planned

* A role name `relay.golemp2p.arkiv-global.net` (A record pending) added to
  the same certificate, so the relay can move without changing testers'
  configuration.
* No rate limits or access control yet: anyone can relay through it, as with
  iroh's own public relays. Limits come with the third-party tester rollout.
