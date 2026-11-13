# puppet

**Updated:** 2026-11-13

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

No host firewall yet (only sshd on 22 listens publicly).

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
  (every port except 22 and 56000-56099 forwarded to the VM).

So an `uplink=real` lab node now crosses its emulated NAT, the lab VM's
masquerade, and this host's NAT in the chosen profile, no longer an i3D NAT
outside our control.

## Services

| service | URL | state |
|---|---|---|
| iroh relay (`p2p-relay1`) | `https://213.163.75.157:56000`, QAD `udp/56001` | planned |
| golem-p2p directory (`p2p-relay1`) | `http://213.163.75.157:56050/` (`realworld.sh` starts it per run) | planned |
| NAT lab (`p2p-natlab`) | `harness/lab/deploy.sh`, then `lab.sh up` | planned |

## Machine

* Bare metal at i3D: AMD EPYC 4464P (12 cores, 24 threads, AMD-V), 122 GiB
  RAM, 2 x 1.7 TB NVMe (system; ZFS pool `vmpool` for the VMs).
* Ubuntu 26.04.1 LTS, kernel 7.0.0-43.
* Rust 1.100 (default), 1.96.0 (the toolchain CI pins) and 1.91.0 (MSRV).
