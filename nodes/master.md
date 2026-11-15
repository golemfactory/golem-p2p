# master

**Updated:** 2026-11-15

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

State: authorized (ssh and sudo on the host and both VMs, since 2026-11-14).

## Services

| service | URL | state |
|---|---|---|
| golem-p2p **registry**, network `golem-testnet` | `https://registry.golemp2p.arkiv-global.net/` | up since 2026-11-15, systemd `golem-p2p-registry` on `127.0.0.1:56080`, nginx + Let's Encrypt in front |
| golem-p2p **directory**, members only | `https://directory.golemp2p.arkiv-global.net/` | up since 2026-11-15, systemd `golem-p2p-directory` on `127.0.0.1:56081`, nginx + Let's Encrypt in front |
| golem-p2p **testnet node** (`listen`, account `master`) | node ID `0x16d62b72e71dd73ccdd121f26b11152a5ed4b877` | up since 2026-11-15, systemd `golem-p2p-testnet-listen`, through puppet's public relay; key `~/golem-p2p-testnet/listen.key` |
| iroh relay | | none; testnet nodes use puppet's public relay |
| `golem-p2p listen` (other) | | per run |

`registry.` and `directory.` are CNAMEs of `master.golemp2p.arkiv-global.net`
(role names: they can move to another host without changing node
configuration). Registry public key (pass as `--registry-key`):

    25d353d17365a58873bb3740c2f287450b1697c0ab000ebdc1ba39dd9ff51c52

Certificates are valid 1 h and renewed by the node. The registry state and
admin token are in `~/golem-p2p-testnet/` on master (mode 0600, not in git).
The state holds the registry's signing key: losing it means a new key for
every node, so it needs a backup off the host (not done yet).

| account | registered node IDs |
|---|---|
| `master` | `0x16d62b72e71dd73ccdd121f26b11152a5ed4b877` (testnet node), `0x962865b3f423127ada55192512c63dc25ce73813` (`~/golem-p2p-testnet/client.key`, for `send` tests) |
| `puppet` | `0x03b31e0e661ef73f64c8710b8486518198e81c18` (puppet's testnet node, see `nodes/puppet.md`), `0xdf54c6809fa776a3a0d11668d61ccd4330fddf76` (its `client.key`) |

All units are hardened (`ProtectSystem=strict`, `ProtectHome`, no
capabilities, `UMask=0077`; `systemd-analyze security`: 3.0-3.1 "OK"). The
registry logs every issued certificate (`journalctl -u golem-p2p-registry`). nginx limits the registry to 10 requests/s per
client address (bursts of 100, then `429`, which nodes retry). The directory
keeps records in memory only: after a restart of `golem-p2p-directory`,
restart the nodes too (or wait up to 20 min until they republish).

## Checks from here

2026-11-15, testnet (details in `comms/2026-11-15-master-testnet-selftest.md`):
members master <-> puppet both ways, direct and `--relay-only` (33-37 ms);
outsiders refused; `probe --bulk 50M` master -> puppet 216 Mbit/s direct;
a 28 s registry outage did not disturb a live connection (50 of 50 echoes),
while a node started during it failed with `registry unavailable`;
renewal and expiry with 60 s certificates as designed; both long-lived
nodes renewed their 1 h certificates 30 min after start (00:12:47 and
00:12:53 UTC).

2026-11-14, `golem-p2p bench` (local check) against puppet's relay
`https://213.163.75.157:56000,quic=56001` with the certificate from
`nodes/puppet.md` (fingerprint `0B:F5:F0:40:…:9E:BB:9C:D1`, verified):
**HEALTHY**. Relay TCP 33 ms, TLS 34 ms, HTTPS 67 ms, UDP 37 ms; public
address `65.109.154.126`, no NAT, same port as local; direct addresses IPv4
and IPv6.

## Machine

* Ubuntu 26.04 LTS, kernel 7.0, 32 CPUs, 62 GB RAM, 3.5 TB disk.
* Rust 1.100 (default) and 1.96.0 (the toolchain CI pins).
* `/usr/local/bin/golem-p2p` built from `577df97` (the same binary on puppet's host).
* Docker runs here; its bridge addresses are not reachable from outside.
