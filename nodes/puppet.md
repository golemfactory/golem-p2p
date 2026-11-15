# puppet

**Updated:** 2026-11-15

## Role

Lab host: runs the VMs of the NAT tests, with the NAT in front of them under
our control. This is the i3D host the harness calls "the i3D host"
(`213.163.75.157`): `p2p-natlab` (the emulated-NAT lab of `harness/lab/`) and
`p2p-relay1` (the real relay of `uplink=real` and the real-world runs) run here
as VMs.

## Reach

| | |
|---|---|
| DNS name | `puppet.golemp2p.arkiv-global.net` |
| IPv4 | `213.163.75.157` (A record) |
| IPv6 | `2a00:1630:a128:17::` in `2a00:1630:a128::/48`, on-link (no AAAA record yet) |
| NAT | host: none, the public addresses are on the interface (`bond0`); VMs: behind the host's NAT (below) |

No host firewall yet (only sshd on 22 listens publicly; certbot on 80 for a
minute when it issues or renews a certificate).

## Access

ssh as `ubuntu` (passwordless sudo on the host and both VMs), keys only, no
passwords, no root login.

| key | host `213.163.75.157` | `p2p-natlab`, `p2p-relay1` |
|---|---|---|
| master (`…R2FZnV`, see `nodes/master.md`) | from `65.109.154.126` and `2a01:4f9:3080:1146::2` | from `10.167.138.1` (the host's bridge address, i.e. `ProxyJump` through the host) |

Host keys (ED25519):

| host | fingerprint |
|---|---|
| puppet `213.163.75.157` | `SHA256:AZW0bbTkjuNRb44UYKYCQsCYzWqbuJ9aR1pYL0x7YkQ` |
| `p2p-natlab` `10.167.138.10` | `SHA256:cdKCUjmi39CBRsUoidFkoq0/4c6MUQdpvbOin8Krm1Y` |
| `p2p-relay1` `10.167.138.20` | `SHA256:zj8P7Mnn+rF8VhCZXb/vYwpCrsKtet0aGymSuogtjkM` |

With that access a node may run the lab (`lab.sh`, `realworld.sh`) and switch
the host NAT (`sudo p2p-hostnat set PROFILE`). One lab run at a time: say in
`comms/` before you start one, and leave the host NAT on `port-restricted`
afterwards.

## VMs

Incus 6.0.5, QEMU/KVM, bridge `incusbr0` `10.167.138.0/24`, ZFS pool on the
second NVMe. IPv4 only.

| VM | address | size | purpose |
|---|---|---|---|
| `p2p-natlab` | `10.167.138.10` | 8 vCPU, 16 GiB, 60 GiB | `harness/lab/lab.sh` (network namespaces, `sch_netem` loaded) |
| `p2p-relay1` | `10.167.138.20` | 4 vCPU, 8 GiB, 30 GiB | iroh relay, directory and reflectors for real-world runs |

Both: Ubuntu 26.04, kernel 7.0.0-43, user `ubuntu` with passwordless sudo,
`nftables conntrack tcpdump python3` installed. ssh from this host only (host
names as in `harness/`: `ssh -F ~/.ssh/p2p/ssh_config p2p-natlab`).

## Host NAT

`p2p-hostnat` (`/usr/local/sbin`, systemd unit `p2p-hostnat`, config
`/etc/p2p-hostnat.conf`) owns the nft table `ip p2p_host`:

* `p2p-relay1`: `213.163.75.157:56000-56099` tcp and udp forwarded 1:1, with
  hairpin (lab nodes and the host reach it at `213.163.75.157`, and it sees
  them as `213.163.75.157:<port>`, as before).
* `p2p-natlab` uplink, switchable with `p2p-hostnat set PROFILE [UDP_TIMEOUT_S]`:
  `port-restricted` (EIM + APDF, plain masquerade, current), `restricted`
  (EIM + ADF), `full-cone` (EIM + EIF), `symmetric` (APDM + APDF) and `open`
  (every port except 22, 80 and 56000-56099 forwarded to the VM).

The profile also applies on the hairpin (lab VM <-> `213.163.75.157` <->
relay VM), so reflectors on the relay VM measure the same NAT as the internet.

What reaches whom: processes on the lab VM itself get exactly the profile.
Lab nodes with `uplink=real` cross their emulated NAT, then the lab VM's own
masquerade (`lab.sh`, port-restricted: it drops unsolicited inbound), then
this NAT. So for them the host profile can make the mapping stricter
(`symmetric`), but `restricted`, `full-cone` and `open` do not open their
filtering.

Measured 2026-11-14 with `tools/natcheck.py` (reflectors on `p2p-relay1`
`:56090`, `:56091`, all through the hairpin, so one remote IP only: `EIF`
shows as `ADF`):

| host profile | on the lab VM | lab node `real1` (easy) | lab node `real2` (symmetric) |
|---|---|---|---|
| `port-restricted` | EIM APDF, port kept | EIM APDF, port kept | APDM APDF |
| `restricted` | EIM ADF, port kept | | |
| `full-cone` | EIM ADF (EIF needs a second remote IP) | | |
| `symmetric` | APDM APDF | APDM APDF | APDM APDF |
| `open` | EIM ADF, unmapped ports open | EIM APDF (2 of 3; 1 APDM) | APDM APDF |

## Services

| service | URL | state |
|---|---|---|
| iroh relay 1.4.1, lab (`p2p-relay1`) | `https://213.163.75.157:56000,quic=56001` + `--relay-ca` (self-signed, below) | up since 2026-11-14, systemd `iroh-relay`, metrics on the VM at `127.0.0.1:9090`; for `harness/` |
| iroh relay **1.5.0**, public (`p2p-relay1`) | `https://puppet.golemp2p.arkiv-global.net:56002,quic=56003`, no `--relay-ca` | up since 2026-11-14, systemd `iroh-relay-public`, metrics `127.0.0.1:9091`; Let's Encrypt certificate (HTTP-01 on the host, renewed by certbot, a deploy hook pushes it into the VM and restarts only this relay); for third-party testers |
| golem-p2p directory (`p2p-relay1`) | `http://213.163.75.157:56050/` | per run (`realworld.sh` starts and stops it) |
| NAT lab (`p2p-natlab`) | `~/golem-p2p-lab` | deployed by master: `ff6fcd1` with the lab's own iroh-relay 1.5.0 (2026-11-15); `8e4c3cb` passed `selftest` 8 of 8 here; lab down between runs |

Relay binaries on `p2p-relay1`, separate so the two relays can run different
versions:

* lab relay (`:56000`): `/usr/local/bin/iroh-relay`, 1.4.1 musl release
  (sha256 `9b9804c7…ac71b3`, the same as in `harness/lab/RESULTS.md`). Kept on
  1.4.1 on purpose: the campaigns compare against it (1.4.1 relay, 1.5.0
  clients). Switching it is announced in `comms/` first.
* public relay (`:56002`): `/usr/local/bin/iroh-relay-public`, 1.5.0 since
  2026-11-15, built here with Rust 1.96.0 from the golemfactory/iroh fork rev
  `4ad24ac2` (`cargo install --git ... --rev ... iroh-relay --features server
  --locked`), stripped, sha256 `4c932a5c…100d0d72`. Same version and
  source as the lab's own relay on `p2p-natlab`. The lab relay's certificate is new (older copies of
`p2p-relay1.crt` no longer verify), self-signed, valid until 2029-11-13; use
it as `--relay-ca` and as `~/.ssh/p2p/p2p-relay1.crt` for the harness scripts. SHA-256 fingerprint
`0B:F5:F0:40:02:DD:4E:47:37:26:E3:27:D0:05:C4:30:32:65:BC:FF:09:91:69:E9:C1:0D:60:50:9E:BB:9C:D1`:

```
-----BEGIN CERTIFICATE-----
MIIBxTCCAWygAwIBAgIUdzxPwflWBk6a/GS/ixNux5DzobowCgYIKoZIzj0EAwIw
FTETMBEGA1UEAwwKcDJwLXJlbGF5MTAeFw0yNjExMTQyMTI3MTNaFw0yOTExMTMy
MTI3MTNaMBUxEzARBgNVBAMMCnAycC1yZWxheTEwWTATBgcqhkjOPQIBBggqhkjO
PQMBBwNCAAQcuIoB25B2956cMs+plw/f+sxLM06F4VGiOX7FWPKlHS4CPULEl3Wy
FempCsR51kVlefByWqyo5bBGKaOGxXjjo4GZMIGWMB0GA1UdDgQWBBRzGs7trNu0
THrEJayQzf9oPW77yDAfBgNVHSMEGDAWgBRzGs7trNu0THrEJayQzf9oPW77yDAx
BgNVHREEKjAohwTVo0udgiBwdXBwZXQuZ29sZW1wMnAuYXJraXYtZ2xvYmFsLm5l
dDAMBgNVHRMBAf8EAjAAMBMGA1UdJQQMMAoGCCsGAQUFBwMBMAoGCCqGSM49BAMC
A0cAMEQCIBYfyt+sZYVkvhuAEN0QvbVv1tRZ657RH3wh1enEleejAiBKlLUhn/lo
GF8PKf8KcWehbRjQc36XnARjep0g1wZtgw==
-----END CERTIFICATE-----
```

DNS for the VMs: the bridge's dnsmasq has `no-hosts` and sends
`golemp2p.arkiv-global.net` to the host's upstream resolvers, so the VMs
resolve `puppet.golemp2p.arkiv-global.net` to `213.163.75.157` (before, the
host's `/etc/hosts` leaked through and gave them loopback addresses).

## Testnet

Installed by master on 2026-11-15 for the closed testnet `golem-testnet`
(registry and directory on master, see `nodes/master.md`). Nothing else on
the host, its relays, `p2p-hostnat` or the VMs was changed.

| | |
|---|---|
| service | systemd `golem-p2p-testnet-listen` on the host (not in a VM): `golem-p2p listen` (echo and bulk) in network `golem-testnet`, relay `https://puppet.golemp2p.arkiv-global.net:56002,quic=56003`, hardened unit (`ProtectSystem=strict`, `ProtectHome=read-only`, no capabilities) |
| binary | `/usr/local/bin/golem-p2p`, deployed by master on 2026-11-15: iroh 1.5.0 build, sha256 `cc4a151b…6bb6544` (as in `nodes/master.md`) |
| node ID | `0x03b31e0e661ef73f64c8710b8486518198e81c18` (key `~/golem-p2p-testnet/listen.key`, mode 0600) |
| account | `puppet`; token in `~/golem-p2p-testnet/puppet.token` (mode 0600, directory 0700). Registered: the listener and `0xdf54c6809fa776a3a0d11668d61ccd4330fddf76` (`client.key`, for `send` tests) |
| outsider | `~/golem-p2p-testnet/outsider.key` (`0x0fc81f1d…`), deliberately not registered, for refusal tests |

Since 2026-11-15 the host's `/etc/hosts` maps
`puppet.golemp2p.arkiv-global.net` (and `puppet`) to `213.163.75.157`, not
to loopback, so golem-p2p on the host reaches both relays by name. The
service's private hosts file (`BindReadOnlyPaths`) is no longer used, and
manual runs need no workaround.

## Machine

* Bare metal at i3D: AMD EPYC 4464P (12 cores, 24 threads, AMD-V), 122 GiB
  RAM, 2 x 1.7 TB NVMe (system; ZFS pool `vmpool` for the VMs).
* Ubuntu 26.04.1 LTS, kernel 7.0.0-43.
* Rust 1.100 (default), 1.96.0 (the toolchain CI pins) and 1.91.0 (MSRV).
