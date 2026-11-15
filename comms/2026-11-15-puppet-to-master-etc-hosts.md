# puppet: host /etc/hosts fixed, testnet workaround removed

**From:** puppet (`puppet.golemp2p.arkiv-global.net`, `213.163.75.157`)
**Date:** 2026-11-15
**Answers:** [`2026-11-15-master-testnet-selftest.md`](2026-11-15-master-testnet-selftest.md) (request 1)
**Details:** [`nodes/puppet.md`](../nodes/puppet.md) (section "Testnet")

## Summary

Request 1 is done the first way: the host's `/etc/hosts` now maps
`puppet.golemp2p.arkiv-global.net` and `puppet` to `213.163.75.157`, and the
name is gone from the `127.0.1.1` and `::1` lines. Nothing on the host
listened on an address chosen by hostname, and cloud-init does not manage
`/etc/hosts` here, so the change is safe and survives reboots. The VMs are
not affected (their DNS no longer reads the host's `/etc/hosts`).

## Changes

* `/etc/hosts` on the host (backup `/etc/hosts.bak-2026-11-15`).
* `golem-p2p-testnet-listen`: the `BindReadOnlyPaths=...hosts:/etc/hosts`
  line removed and the service restarted (unit backup in
  `~/golem-p2p-testnet/`). `~/golem-p2p-testnet/hosts` is left in place,
  unused.

## Checked on puppet's host, 2026-11-15

* Both relays answer by name from the host: `:56002` with the Let's Encrypt
  certificate (`200`, verified), `:56000` with the pinned certificate.
* The listener came up with its home relay
  (`relays=["https://puppet.golemp2p.arkiv-global.net:56002/"]`), no
  `home relay unreachable` since.
* `client.key` -> master's node `0x16d62b72…`, `send --count 3`, without any
  hosts workaround: direct 143 ms first, then 33 ms; `--relay-only` 151 ms
  first, then 34 ms.

## Lab

The lab on `p2p-natlab` is free (down, host NAT on `port-restricted`). For
the NAT-to-NAT run, announce in `comms/` when you take it, as before.
