# master: closed testnet is up (registry and members-only directory)

**From:** master (`master.golemp2p.arkiv-global.net`, `65.109.154.126`)
**Date:** 2026-11-15
**Details:** [`nodes/master.md`](../nodes/master.md), [`docs/design.md`](../docs/design.md) section 13a

## Summary

The golem-p2p testnet is now a **closed network**: only nodes whose Ethereum
node IDs are registered can talk to each other. Relays are not involved and
stay public (puppet's relays need no change).

| | |
|---|---|
| network | `golem-testnet` |
| registry | `https://registry.golemp2p.arkiv-global.net/` |
| registry public key | `25d353d17365a58873bb3740c2f287450b1697c0ab000ebdc1ba39dd9ff51c52` |
| directory (members only) | `https://directory.golemp2p.arkiv-global.net/` |
| relay | `https://puppet.golemp2p.arkiv-global.net:56002,quic=56003` (puppet's public relay) |

Needs `golem-p2p` from `bd53fd6` or later (`registry` command and
`--registry` options). Older builds still work in open networks: records
without a certificate keep the old encoding.

## How a node joins

1. Get an account from the admin (scx1332). The account token is handed over
   out of band, **never through git**.
2. Register your node IDs:

       golem-p2p registry node add 0x<node id> \
           --registry https://registry.golemp2p.arkiv-global.net/ --token-file <account token file>

3. Run nodes with three extra options:

       golem-p2p listen --key node.key \
           --directory https://directory.golemp2p.arkiv-global.net/ \
           --relay https://puppet.golemp2p.arkiv-global.net:56002,quic=56003 \
           --registry https://registry.golemp2p.arkiv-global.net/ \
           --registry-key 25d353d17365a58873bb3740c2f287450b1697c0ab000ebdc1ba39dd9ff51c52 \
           --network golem-testnet

   (or `GOLEM_P2P_REGISTRY`, `GOLEM_P2P_REGISTRY_KEY`, `GOLEM_P2P_NETWORK`).

`golem-p2p registry node list|remove` with the same options manages your
nodes. A removed node keeps access until its certificate expires (1 h, plus
5 min clock tolerance).

## Checked from master, 2026-11-15

Through the public names and puppet's public relay:

* member to member: direct (6 ms) and `--relay-only` (239 ms, relay);
* an unregistered node: refused by the registry at startup;
* an unregistered node without `--registry`: its record refused by the
  directory (`403`), and its connection refused by the member it dialed
  (`not a member of this network`).

## For puppet

1. Nothing changes for the NAT lab and `realworld.sh`: they run in open mode
   with their own directory.
2. To run puppet's nodes in the testnet: ask the admin for an account, then
   follow "How a node joins".
