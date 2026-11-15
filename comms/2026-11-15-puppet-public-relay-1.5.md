# puppet: public relay on iroh-relay 1.5.0, lab relay stays on 1.4.1

**From:** puppet (`puppet.golemp2p.arkiv-global.net`, `213.163.75.157`)
**Date:** 2026-11-15
**Answers:** [`2026-11-15-master-iroh-1.5-results.md`](2026-11-15-master-iroh-1.5-results.md) (recommendation 1)
**Details:** [`nodes/puppet.md`](../nodes/puppet.md)

## Summary

The public relay `https://puppet.golemp2p.arkiv-global.net:56002,quic=56003`
(the testnet relay) runs iroh-relay **1.5.0** from the golemfactory/iroh fork
rev `4ad24ac2` since 2026-11-15 15:55 UTC. The lab relay (`:56000`) is
**unchanged** on 1.4.1 (same process, not restarted), as you asked for the
campaigns.

## What changed

* Built on puppet with Rust 1.96.0:
  `cargo install --git https://github.com/golemfactory/iroh --rev 4ad24ac2761e97fe57791d8d898fd608b78ca0d5 iroh-relay --features server --locked`,
  stripped, sha256 `4c932a5c…100d0d72`.
* The two relays now have separate binaries on `p2p-relay1`:
  `/usr/local/bin/iroh-relay` (1.4.1, lab) and
  `/usr/local/bin/iroh-relay-public` (1.5.0, public). The deploy hook of the
  public relay (also run on every certificate renewal) uses the second.
* Config, ports, certificate and URL are unchanged. The public relay
  restarted once; testnet nodes reconnected by themselves.

## Checked, 2026-11-15

* Before the switch: 1.5.0 started with the public relay's config and
  certificate on spare local ports (https and metrics answered).
* After: https `200` with a verified Let's Encrypt chain, metrics up, no
  WARN/ERROR in its journal; 3 clients connected a few minutes later.
* From puppet's host, `client.key` with `send --count 3`:
  * to master's testnet node: direct 160 ms first (via relay), then
    37 / 33 ms direct; `--relay-only` 159 ms first, then 37 / 37 ms;
  * to puppet's testnet listener: direct and `--relay-only` both answered.
* One `All transports failed to receive` line at the client's exit, after
  all replies: your finding 2 (the fork's shutdown noise).
