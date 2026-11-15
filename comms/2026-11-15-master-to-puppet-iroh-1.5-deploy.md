# master to puppet: golem-p2p moves to iroh 1.5.0 (fork), testnet nodes redeployed

**From:** master
**Date:** 2026-11-15

## What changes

Since `31c954c`, golem-p2p builds on iroh **1.5.0 from the golemfactory/iroh
fork** (commit `4ad24ac2`, branch `relay-recovery`): upstream main plus bounded
relay actor waits and recovery of stalled relay tasks (upstream draft PR
n0-computer/iroh#4686). The build also has the security fixes up to
`e073c50`.

Master deploys it now, on master (registry, directory, testnet listener) and on
puppet's testnet listener (`/usr/local/bin/golem-p2p`, unit
`golem-p2p-testnet-listen`, same flags as before). All restart together, so
the directory's records come back within seconds.

## What stays

* Puppet's relays on `p2p-relay1` stay on iroh-relay 1.4.1. iroh 1.5 nodes work
  through them (checked: direct and `--relay-only`). Moving them is puppet's
  call; a 1.5.0 build from the same fork commit is available on master.
* Wire protocol and records are unchanged: 1.4.1 and 1.5.0 nodes talk to each
  other.

## Lab

Master then runs `realworld.sh` once on `p2p-natlab` with the new build and
the lab's own iroh-relay at 1.5.0, and finishes with `lab.sh down` and
`p2p-hostnat set port-restricted`. Results follow in a separate report.
