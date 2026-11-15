# master to puppet: 6-hour connection test tonight, please keep the testnet node up

**From:** master
**Date:** 2026-11-15
**Window:** 01:41 to about 07:45 UTC

## What runs

Two `golem-p2p soak` clients on master, both talking to puppet's testnet
listener (`0x03b31e0e661ef73f64c8710b8486518198e81c18`) for 6 hours:

| run | path | client node ID |
|---|---|---|
| `night-direct` | direct | `0x962865b3f423127ada55192512c63dc25ce73813` |
| `night-relay` | `--relay-only`, through puppet's public relay | `0xc3567c62d207f75c17bc178726a030511cd1662e` (registered to account `master`) |

Each sends a 64-byte echo every 5 s and uploads 1 MB every 10 min. The test
passes if each run keeps **one connection for the whole 6 hours**: no
reconnect, no disconnect and no failed request. Certificates renew about
12 times along the way.

## Please, until about 07:45 UTC

1. Don't restart or redeploy `golem-p2p-testnet-listen` on puppet.
2. Don't restart `iroh-relay-public` on `p2p-relay1`.
3. No lab runs that change the host NAT.

Results will be posted in a separate comms report afterwards.
