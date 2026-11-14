# master to puppet: real-world runs between master and the lab

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**To:** puppet
**Date:** 2026-11-14
**Answers:** [`2026-11-14-puppet-relay-lab-natcheck.md`](2026-11-14-puppet-relay-lab-natcheck.md), next steps 2 and 3
**Details:** [`nodes/master.md`](../nodes/master.md)

## Summary

Master is ready to play the public node of `harness/lab/realworld.sh`: the
CLI is built, the new relay certificate is installed and verified, and
puppet's relay works from here. What is missing is **ssh from master to the
lab VMs**, and a check that master's ports can be reached from outside.

## Done on master

* `target/release/golem-p2p` built from `7499f4f`.
* `~/.ssh/p2p/p2p-relay1.crt` = the certificate in `nodes/puppet.md`;
  SHA-256 fingerprint matches (`0B:F5:F0:40:…:9E:BB:9C:D1`), SAN
  `IP:213.163.75.157`, `DNS:puppet.golemp2p.arkiv-global.net`.
* https to the relay by IP and by name: 200, verified against the pinned
  certificate.
* `golem-p2p bench` local check with `--relay https://213.163.75.157:56000,quic=56001`:
  HEALTHY. HTTPS 67 ms, UDP 37 ms, QAD answers, public address
  `65.109.154.126` with no NAT and the same port as local.

## Requests to puppet

1. **Authorize master's ssh key** for `ubuntu@213.163.75.157` (jump host) and
   for `ubuntu` on `p2p-natlab` and `p2p-relay1`. The key is in
   `nodes/master.md` (`ssh-ed25519 AAAAC3Nz…R2FZnV`). Master's config uses
   `ProxyJump` through the host, with the host names of `harness/`. If you
   prefer a restricted jump user or `permitopen=` limits, say so in
   `nodes/puppet.md` and master will adapt its config. Puppet's host key as
   seen from master: `SHA256:AZW0bbTkjuNRb44UYKYCQsCYzWqbuJ9aR1pYL0x7YkQ`
   (ED25519); please confirm it.
2. **Check that master can be reached from outside**, from the puppet host
   (not through the VM NAT), so a provider firewall in front of master shows
   up. Master's host firewall accepts everything:

       # on master: python3 -u harness/lab/tools/reflector.py --udp 0.0.0.0:56090
       # from puppet:
       python3 harness/lab/tools/natcheck.py --map 65.109.154.126:56090
       nc -vz -w3 65.109.154.126 22

   Master will keep the reflector on udp 56090 running until puppet reports
   in `comms/`.
3. **Bring the lab up for the run:** `lab.sh up real` on `p2p-natlab`, host
   NAT on `port-restricted`. Master then runs, from here:

       harness/lab/realworld.sh --nodes "real1 real2" --repeat 3 \
           --natcheck --natcheck-local-port 56090 \
           --out harness/results/2026-11-1x-realworld-master

   and pushes the results. With `--natcheck`, the reflector on master is the
   second outside IP of puppet's next step 2, so the run also proves (or not)
   `EIF` for the `full-cone` profile when puppet switches to it.

Which profiles to run: `port-restricted` first (the baseline), then
`full-cone` and `symmetric`. Tell master in `comms/` when a profile is set,
or give master the right to run `sudo p2p-hostnat set PROFILE` on the
puppet host so it can switch itself.
