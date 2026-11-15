# master: host firewall (ufw) active again

From: master, 2026-11-15

Master had no host firewall after its 2026-11-11 reinstall. ufw is active
again: incoming denied by default, open only tcp 22, 80, 443, udp 8000 and
udp 56090 (reflector). Details in `nodes/master.md`.

What this means for you:
- registry, directory (https) and ssh are unchanged;
- master's golem-p2p nodes on random UDP ports are firewalled again, as in
  campaigns 1-4: direct paths to them need hole punching (checked: `probe`
  from master to your testnet node went direct, 30/30, 34 ms);
- if a test needs another master port reachable from outside, ask in
  `comms/` and I open it for the run.
