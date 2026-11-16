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

Host firewall: ufw, active since 2026-11-15 (the host had none after the
2026-11-11 reinstall). Incoming is denied by default, outgoing allowed, and
only these are open: tcp 22 (ssh), tcp 80 and 443 (nginx), udp 8000 (the
harness's "open" listener port) and udp 56090 (reflector). Every other UDP
port drops unsolicited packets, so a golem-p2p node here on a random port is
a firewalled node: direct paths need hole punching, as in the `*-ufw`
scenarios of campaigns 1-4. Checked from p2p-relay1 after enabling: ssh and
https reachable, other ports blocked; `probe` from master to puppet's testnet
node went direct (30/30, 34 ms). Docker's published ports are on 127.0.0.1
only, and container egress still works.
Whether the provider (Hetzner) filters in front of the host is not known yet:
it can only be tested from outside (see `comms/2026-11-14-master-to-puppet-realworld.md`).

Ports in use: 22 (sshd), 80 and 443 (nginx, another service). For golem-p2p
tests master uses **56000-56099** tcp and udp, the same range as puppet:

| port | use | state |
|---|---|---|
| udp 56090 | reflector (`harness/lab/tools/reflector.py`) for `realworld.sh --natcheck --natcheck-local-port 56090` and puppet's reachability check | up, systemd `gpl-master-reflector`, stops by itself on 2026-11-21 |
| tcp 56080, 56081, 56082 (127.0.0.1 only) | registry, directory, telemetry collector, behind nginx | up |
| other 56000-56099 | listeners, a second relay later | free; closed in ufw (open one with `sudo ufw allow <port>/udp` when a test needs it reachable) |

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
| golem-p2p **telemetry** collector and dashboard | `https://telemetry.golemp2p.arkiv-global.net/` | up since 2026-11-15, systemd `golem-p2p-telemetry` on `127.0.0.1:56082`, nginx + Let's Encrypt in front; see below |
| golem-p2p **testnet node** (`listen`, account `master`) | node ID `0x16d62b72e71dd73ccdd121f26b11152a5ed4b877` | up since 2026-11-15, systemd `golem-p2p-testnet-listen`, through puppet's public relay; key `~/golem-p2p-testnet/listen.key` |
| iroh relay | | none; testnet nodes use puppet's public relay |
| `golem-p2p listen` (other) | | per run |

`registry.`, `directory.` and `telemetry.` are CNAMEs of `master.golemp2p.arkiv-global.net`
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

**Telemetry** ([docs/telemetry.md](../docs/telemetry.md)). The registry
runs with `--telemetry-url https://telemetry.golemp2p.arkiv-global.net/`, so
every testnet `listen` and `soak` reports there every 5 s unless started
with `--no-telemetry`. The collector (`golem-p2p telemetry serve`, unit
`golem-p2p-telemetry`, hardened like the others, writes only its store)
keeps 5 s samples for an hour and per-minute samples for 7 days in
`~/golem-p2p-testnet/telemetry/` (0700). nginx (site `golem-p2p-testnet`,
certificate `golem-p2p-telemetry`): `POST /v1/report` is open (reports are
signed by members; at most 10/s per address, bursts of 40). The dashboard
and the JSON API need a Google login (since 2026-11-16): nginx
`auth_request` to oauth2-proxy v7.15.5 (`/usr/local/bin/oauth2-proxy`,
systemd `oauth2-proxy-telemetry`, user `oauth2-telemetry`, 127.0.0.1:4180,
callback `/oauth2/callback`). Its config with the OAuth client secret and
cookie secret, and the list of allowed Google accounts (one email per line,
reloaded on change), are in `/etc/oauth2-proxy-telemetry/` (root:oauth2-telemetry
0640, not in git). `/metrics` (for Prometheus) keeps HTTP basic auth, user
`golem`, password in `~/golem-p2p-testnet/telemetry-dashboard.credentials`
(0600, not in git; nginx has only the hash,
`/etc/nginx/golem-p2p-telemetry.htpasswd`). A
restart of the collector keeps the history (it reloads its store); a node
missing for more than 60 s shows as offline. Checked on 2026-11-15 after the
deploy: master's testnet node reports (found through the registry); a
temporary `soak` with `client.key` against it appeared online with a direct
link reported by both ends, and was offline (stopped) at once after it
exited; without credentials the dashboard and `/metrics` answer `401`, an
unsigned report `401`.

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
* `/usr/local/bin/golem-p2p` built from `fe28c40` (telemetry) with Rust
  1.96.0, sha256 `5dcc22db…3fa9f2e`: iroh **1.5.0** from the golemfactory/iroh
  fork (rev `4ad24ac2`, branch `relay-recovery`), netwatch 0.20. Deployed
  2026-11-15 21:00 UTC (registry, directory, testnet listener, telemetry
  collector). The directory's records of registered nodes were saved before
  its restart and published again right after it, so puppet's node stayed
  findable. The previous build (`e2ff75b`, sha256 `cc4a151b…6bb6544`, the one
  measured in benchmark campaign 5, `harness/results/2026-11-15-campaign5/`,
  and still on puppet's host) is kept as
  `/usr/local/bin/golem-p2p.prev-e2ff75b`.
* iroh-relay 1.5.0 from the same fork rev, built here
  (`~/golemfactory/iroh/target/release/iroh-relay`), not run as a service on
  master; a debug-stripped copy is the lab's own relay on `p2p-natlab` since
  2026-11-15 (`harness/lab/deploy.sh` with `IROH_RELAY_BIN`).
* Docker runs here; its bridge addresses are not reachable from outside.
