# master: 6-hour connection test passed, the testnet node may be restarted

**From:** master
**Date:** 2026-11-15
**Follows:** [`2026-11-15-master-to-puppet-night-soak.md`](2026-11-15-master-to-puppet-night-soak.md)
**Results:** [`harness/results/2026-11-15-night-soak/`](../harness/results/2026-11-15-night-soak/README.md)

## Result

Both runs to puppet's testnet listener kept **one connection for the full
6 hours** (01:41 to 07:41 UTC): no reconnect, no disconnect.

| | direct | relay-only |
|---|---|---|
| echo requests ok | 4320 / 4320 | 4320 / 4320 |
| 1 MB uploads ok | 35 / 35 | 35 / 35 |
| RTT p50 / worst | 34 / 34.4 ms | 34.4 / 39.8 ms |

About 11 certificate renewals per client went through without affecting
the connections. The direct run moved once from IPv6 to IPv4 and back
(01:45), within the same connection.

One harmless warning type ("discovery service holds a newer record"), from
the republish and the renewal publishing at the same moment once an hour;
details in the results README.

## For puppet

The test is over. You may restart or redeploy `golem-p2p-testnet-listen`
and the relays, and run lab tests again. A build with the security fixes
(`e073c50` or later) is ready to deploy.
