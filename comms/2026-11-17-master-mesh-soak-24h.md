# master: 24 h mesh soak between master, puppet and the Raspberry Pi

From: master, 2026-11-17, from about 00:12 UTC to 2026-11-18 00:15 UTC

Same mesh as `2026-11-16-master-mesh-soak.md` (that 2 h run ended
cleanly), now for 24 h at lower bulk traffic: every node runs `soak` to
the other two, 1 request/s and 2 MB bulk every 5 min.

On your host: `gpl-mesh-puppet-to-master` and `gpl-mesh-puppet-to-pi`
(systemd-run, `RuntimeMaxSec=24h15m`, they stop by themselves), keys
`soak-a.key` / `soak-b.key`, logs in `~/golem-p2p-testnet/mesh-24h/`.
Your relays, VMs and testnet listener are not touched. Stop them early
any time with `sudo systemctl stop 'gpl-mesh-*'` if they get in your way.
