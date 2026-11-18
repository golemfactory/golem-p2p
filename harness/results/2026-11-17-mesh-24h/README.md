# 24 h mesh soak: master, puppet, Raspberry Pi (2026-11-17)

2026-11-17 00:11 UTC to 2026-11-18 00:12 UTC. Three testnet nodes, each
running `listen` plus a `soak` client to each of the other two: 6 links,
every soak with its own registered key. `soak --duration 24h --interval 1s
--stats-interval 5m --bulk-every 5m --bulk-size 2M`, relay
`https://puppet.golemp2p.arkiv-global.net:56002,quic=56003`, closed network
`golem-testnet`, telemetry on (dashboard
https://telemetry.golemp2p.arkiv-global.net/). Build `5dcc22db…` on master
and puppet (commit `1eae659`), the Pi built `300361a` itself (same code).

- master: public server, host firewall ufw (random UDP ports closed);
- puppet: host behind its own NAT (port-restricted), see `nodes/puppet.md`;
- Pi: Raspberry Pi 5 on coworking Wi-Fi behind a dual-ISP NAT (flows
  spread over two ISPs, see `../2026-11-15-rpi-coworking/`).

Preceded by a 2 h run with the same mesh (2026-11-16, 2 MB bulk every
minute): master's two soaks had 0 failures and 1 connection each.

## Results per link

| link | requests ok / sent | failed | connections | bulk 2 MB ok / failed | disconnects | path | RTT p50 (end) |
|---|---|---|---|---|---|---|---|
| master → puppet | 86 400 / 86 400 | 0 | 1 | 287 / 0 | 0 | direct 100% | 37.1 ms |
| master → Pi | 86 395 / 86 395 | 0 | 1 | 287 / 0 | 0 | relay → direct after <1 s, then direct 100% | 34.8 ms |
| puppet → master | 86 400 / 86 400 | 0 | 1 | 287 / 0 | 0 | direct 100% | 37.6 ms |
| puppet → Pi | 86 395 / 86 395 | 0 | 1 | 287 / 0 | 0 | direct 100% | 24.4 ms |
| Pi → master | 86 344 / 86 347 | 3 (timeouts) | 2 | 287 / 0 | 1 | direct 99.9% | 49.6 ms |
| Pi → puppet | 86 367 / 86 368 | 1 (timeout) | 2 | 287 / 0 | 1 | direct 100% | 32.1 ms |

Direct fraction per 5 min window: 1.00 in every window, except the Pi's
two links around 23:10 (minimum 0.85 and 0.90).

Totals: 518 304 requests, 4 failed (0.0008%), 1 722 bulk transfers of
2 MB, 0 failed; 4 of 6 links held one connection for 24 h.

## The one incident: 23:10 UTC, the Pi's network

At about 23:09:50 UTC every flow of the Pi broke at once: both golem-p2p
connections closed (`disconnect`, reason `closed`, 23:10:02), its
telemetry, directory and registry requests failed (23:09:51 to 23:10:07),
and its reverse SSH tunnel to master got "connection reset by peer"
(23:10:35). The Pi's NetworkManager logged no Wi-Fi or DHCP event, so it
was most likely the office network (NAT or ISP failover dropping its flow
state).

golem-p2p recovered without help: new connections through the relay after
5 s (Pi → master) and 8 s (Pi → puppet), direct again after 40 s and 26 s.
Pi → master switched direct → relay → direct once more at 23:14:42 (within
0.4 s). Requests lost: 1 per link at the break, plus 2 timeouts to master
at 23:14. The soaks on master and puppet towards the Pi's listener kept
their connections (0 failures, 1 connection).

Side finding (not golem-p2p): master kept the dead SSH tunnel session
(no keepalive), so the Pi's reconnects failed with "remote port forwarding
failed for listen port 2222" until master's stale session was killed on
2026-11-18 00:25 UTC. Master now has `ClientAliveInterval 30` /
`ClientAliveCountMax 3` for user `pi-tunnel` only
(`/etc/ssh/sshd_config.d/50-pi-tunnel.conf`).

## Process health

- Soak clients: RSS grew from 18-23 MB to 25-38 MB in the first hours and
  then stayed flat (master: 23 → 33 and 22 → 34 MB; puppet: 23 → 38 and
  22 → 36 MB; Pi: 18 → 25 MB each).
- Listeners: master's `golem-p2p-testnet-listen` (running since
  2026-11-15 21:00 UTC) at 21 MB, puppet's at 24 MB; no WARN/ERROR on
  master's; puppet's one `WARN noq_proto::connection: failed closing path
  err=LastOpenPath` at 23:10:26, when the Pi's connection broke.
- WARN/ERROR in the soak logs: none on master and puppet apart from the
  known shutdown line `relay_recv_channel closed`; on the Pi only the
  23:10 incident (publish, lookup, certificate renewal and telemetry
  retries, all recovered) and 4 single telemetry or publish retries
  during the day (10:31, 14:11, 14:31, 15:31, 22:50 UTC).
- All mesh processes ended on time (`final: true`, `interrupted: false`);
  the Pi's listener ends by itself at about 00:47 UTC.

Logs stay on the hosts: master and puppet `~/golem-p2p-testnet/mesh-24h/`,
the Pi `~/golem-p2p-results/mesh-24h/`.
