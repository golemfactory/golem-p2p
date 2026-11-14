# master

**Updated:** 2026-11-14

## Role

Master node of the golem-p2p tests: the public host that other nodes connect
to. In `harness/lab/realworld.sh` it plays the public node (the machine the
script runs on), against the lab on puppet.

## Reach

| | |
|---|---|
| DNS name | `master.golemp2p.arkiv-global.net` |
| IPv4 | `65.109.154.126` (A record, TTL 3600) |
| IPv6 | `2a01:4f9:3080:1146::2` (AAAA record) |
| NAT | none: the public addresses are on the interface (`enp4s0`) |

The domain `arkiv-global.net` (OVH) expires on 2026-11-18 unless it is renewed.

Host firewall: none (INPUT policy ACCEPT, only Docker's forwarding rules).
Whether the provider (Hetzner) filters in front of the host is not known yet:
it can only be tested from outside (see `comms/2026-11-14-master-to-puppet-realworld.md`).

Ports in use: 22 (sshd), 80 and 443 (nginx, another service). For golem-p2p
tests master uses **56000-56099** tcp and udp, the same range as puppet:

| port | use | state |
|---|---|---|
| udp 56090 | reflector (`harness/lab/tools/reflector.py`) for `realworld.sh --natcheck --natcheck-local-port 56090` and puppet's reachability check | up, systemd `gpl-master-reflector`, stops by itself on 2026-11-21 |
| other 56000-56099 | listeners, a second relay later | free |

## Access to puppet

`realworld.sh` runs here and needs ssh to `p2p-natlab` and `p2p-relay1`
through puppet. Config `~/.ssh/p2p/ssh_config` (hosts `puppet`,
`p2p-natlab`, `p2p-relay1`, jump via `ubuntu@213.163.75.157`), key
`~/.ssh/p2p/id_ed25519`, public key:

```
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEsXVbk0CE5gnMVGU1EqiwSLH6FnAeXNNg1dHHR2FZnV golem-p2p master (master.golemp2p.arkiv-global.net)
```

State: **not authorized yet** (puppet answers `Permission denied (publickey)`).

## Services

| service | URL | state |
|---|---|---|
| iroh relay | `https://master.golemp2p.arkiv-global.net:<port>` (Let's Encrypt) | planned |
| golem-p2p directory | | none; realworld runs use puppet's per-run directory |
| `golem-p2p listen` | | per run (`realworld.sh`) |

## Checks from here

2026-11-14, `golem-p2p bench` (local check) against puppet's relay
`https://213.163.75.157:56000,quic=56001` with the certificate from
`nodes/puppet.md` (fingerprint `0B:F5:F0:40:…:9E:BB:9C:D1`, verified):
**HEALTHY**. Relay TCP 33 ms, TLS 34 ms, HTTPS 67 ms, UDP 37 ms; public
address `65.109.154.126`, no NAT, same port as local; direct addresses IPv4
and IPv6.

## Machine

* Ubuntu 26.04 LTS, kernel 7.0, 32 CPUs, 62 GB RAM, 3.5 TB disk.
* Rust 1.100 (default) and 1.96.0 (the toolchain CI pins).
* `target/release/golem-p2p` built from `7499f4f`.
* Docker runs here; its bridge addresses are not reachable from outside.
