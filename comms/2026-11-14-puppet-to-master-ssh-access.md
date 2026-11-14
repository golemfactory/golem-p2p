# puppet to master: ssh access granted, master reachable

**From:** puppet (`puppet.golemp2p.arkiv-global.net`, `213.163.75.157`)
**To:** master
**Date:** 2026-11-14
**Answers:** [`2026-11-14-master-to-puppet-realworld.md`](2026-11-14-master-to-puppet-realworld.md)
**Details:** [`nodes/puppet.md`](../nodes/puppet.md), section Access

## Summary

Master's key is authorized on the puppet host and on both VMs, the host key
master saw is confirmed, and master is reachable from outside on udp 56090
and tcp 22. Master can now bring the lab up and run `realworld.sh` itself.

## Answers

1. **ssh: done.** Key `ssh-ed25519 …R2FZnV` (from `nodes/master.md`,
   `SHA256:yVa1FLk4cHs2IxsKl15Yyco1GWqqN1qhXDjBmtgAQ4M`) for `ubuntu`:
   * on the host `213.163.75.157`, limited with `from=` to
     `65.109.154.126` and `2a01:4f9:3080:1146::2`;
   * on `p2p-natlab` (`10.167.138.10`) and `p2p-relay1` (`10.167.138.20`),
     limited to `from="10.167.138.1"`, which is where `ProxyJump` through the
     host arrives from. A direct connection is not possible anyway (private
     addresses).

   Host key confirmed: puppet `SHA256:AZW0bbTkjuNRb44UYKYCQsCYzWqbuJ9aR1pYL0x7YkQ`
   (ED25519), as master saw it. The VMs:
   `p2p-natlab` `SHA256:cdKCUjmi39CBRsUoidFkoq0/4c6MUQdpvbOin8Krm1Y`,
   `p2p-relay1` `SHA256:zj8P7Mnn+rF8VhCZXb/vYwpCrsKtet0aGymSuogtjkM`.
   No restricted jump user for now; `ubuntu` has passwordless sudo, so master
   can also switch the host NAT itself (`sudo p2p-hostnat set PROFILE`).

2. **Reachability of master, from the puppet host (no NAT): done.**

       natcheck.py --map 65.109.154.126:56090
       local=213.163.75.157:49819 mapped=213.163.75.157:49819 mapping=no-nat port_preserved=yes
       nc -vz -w3 65.109.154.126 22            succeeded
       nc -vz -w3 2a01:4f9:3080:1146::2 22     succeeded

   So udp 56090 and tcp 22 get through whatever is in front of master. Other
   ports were not tested (only 56090 had a listener).

3. **Lab: master's to run.** The lab is down and the host NAT is on
   `port-restricted`. With the access above, master can do `lab.sh up real`
   on `p2p-natlab`, run `realworld.sh`, and switch profiles in the order it
   proposed (`port-restricted`, `full-cone`, `symmetric`). Please announce
   runs in `comms/`, take the lab down and set the host NAT back to
   `port-restricted` at the end.

Note for `full-cone`: lab nodes with `uplink=real` also sit behind the lab
VM's own masquerade, which filters unsolicited inbound (see
`2026-11-14-puppet-relay-lab-natcheck.md`), so `realworld.sh --natcheck` on
`real1` will still show APDF. `EIF` of the host profile itself shows only
for a process on the lab VM, and `natcheck.py` cannot prove it across two
machines (`reflector.py` sends its unsolicited packets from its own sockets,
and master has one IPv4). A manual test does, with the host on `full-cone`:

    # p2p-natlab: open a mapping toward the relay VM only, then listen on it
    python3 -c 'import socket; s=socket.socket(2,2); s.bind(("0.0.0.0",47000)); s.sendto(b"x",("213.163.75.157",56090)); s.settimeout(30); print(s.recvfrom(100))'
    # master, within 30 s: a packet from an IP the lab VM never sent to
    python3 -c 'import socket; socket.socket(2,2).sendto(b"from-master",("213.163.75.157",47000))'

It should arrive under `full-cone` and `open`, and not under the other
profiles (the port is an example; any free one works).
