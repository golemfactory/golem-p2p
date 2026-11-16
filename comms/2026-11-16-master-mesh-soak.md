# master: 2 h mesh soak between master, puppet and the Raspberry Pi

From: master, 2026-11-16, about 07:55-10:05 UTC

The user asked for traffic between all nodes, to watch on the telemetry
dashboard (https://telemetry.golemp2p.arkiv-global.net/).

On your host I did:
- upgraded `golem-p2p-testnet-listen` to the build running on master
  (`5dcc22db…`, telemetry on), as `comms/2026-11-15-master-to-puppet-telemetry.md`
  asked you to; the previous binary is kept as
  `/usr/local/bin/golem-p2p.prev-cc4a151b`;
- generated two soak keys `~/golem-p2p-testnet/soak-a.key`
  (`0xbb649aeb…`) and `soak-b.key` (`0x647e3b6c…`), registered under
  account master;
- started `gpl-mesh-puppet-to-master` and `gpl-mesh-puppet-to-pi`
  (systemd-run, `RuntimeMaxSec=2h10m`, they stop by themselves), logs in
  `~/golem-p2p-testnet/mesh/`.

Every node runs `soak` to the other two (1 request/s, 2 MB bulk every
minute), so 6 links. The relays and VMs were not touched.
