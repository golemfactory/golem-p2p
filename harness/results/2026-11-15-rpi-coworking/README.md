# Raspberry Pi on coworking Wi-Fi, closed testnet (2026-11-15)

First testnet node outside the two test hosts: a Raspberry Pi 5 (8 GB), Debian
13, on the Wi-Fi of a shared office (coworking), golem-p2p `3bd7961` built on
the Pi (Rust 1.100.0). Node ID `0x039e559b9eba490585cbb4c5aa2c938b98e20766`,
registered under account master. Run by an agent on the Pi from copy-paste
instructions with public access only; results as reported by it.

| test | result |
|---|---|
| local check (`bench`, no `--to`) | HEALTHY: UDP works (IPv4 only), behind NAT, port changed by NAT, mapping type unknown (one relay answered) |
| `probe` master (firewalled, ufw) | relay only, never direct; RTT p50 71.4 ms, 30/30 |
| `probe` puppet's testnet node | direct after 112 ms; RTT p50 28.1 ms, 30/30 |
| `bench` master `--compare-relay` | main connection direct after 276 ms (98% direct), RTT 50 ms, up 56.4 / down 42.7 Mbps, 0% loss; small requests 330 ms during transfers; relay comparison failed (see below), so DEGRADED |
| `bench` puppet | not run yet |

## Findings

1. **`--compare-relay` could not work on a closed network:** its second
   endpoint used a fresh temporary identity, which has no certificate. Fixed
   with `--compare-relay-key` (a second registered key); without it the
   comparison is now skipped with a reason instead of failing.
2. **The office's public IP drifts.** The operator reports that the
   network's public address sometimes changes, probably because it goes out
   through several providers (multi-WAN). That fits the mixed results: to
   master hole punching failed in `probe` but succeeded in `bench` a few
   minutes later. If different flows or moments leave through different
   public addresses, the address the peer learns (via the relay's QAD) may
   not be the one the next packets come from, so hole punching becomes
   hit-or-miss, and an established direct path can break when the address
   changes. This is a special case for us to handle: a NAT that looks
   symmetric across, or over time within, one connection. To do: measure it
   (repeated QAD / reflector checks from the Pi over hours, path changes in
   a long `soak`), and check that connections recover through the relay.
