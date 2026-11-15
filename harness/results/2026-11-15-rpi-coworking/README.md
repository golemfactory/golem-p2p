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
| `bench` puppet | HEALTHY: direct from the start, RTT 35 ms, up 60.4 / down 39.5 Mbps, 0% loss |
| `bench` master `--compare-relay --compare-relay-key` (round 2) | RELAY ONLY: never direct; relay 71 ms, 32.7 / 23.7 Mbps; relay-only comparison the same (70 ms, 33.9 / 23.2 Mbps) |
| `bench` puppet `--compare-relay --compare-relay-key` (round 2) | HEALTHY: direct from the start, 28 ms, 61.8 / 43.4 Mbps; relay comparison 32 ms, 28.9 / 27.2 Mbps |
| 3 more `probe` master | relay only, all 3 (RTT p50 72 ms) |
| 30 min `soak` master | 360/360 ok, one connection, relay to direct after about 4 s (32 hole punching attempts), then direct for the whole 30 min, 0 disconnects, RTT p50 50 ms |
| public address, 30 local checks, one per minute | 2 public IPs (ISP A 17x, ISP B 13x), changed 15 times in 29 intervals, a new port every run |
| `natcheck.py`, 20 runs (reflectors: master :56090, p2p-relay1 :56090 and :56091) | mapping EIM 6, ADM 7, APDM 7; filtering APDF in all 20 |

Totals to master (firewalled by ufw): direct in 2 of 7 attempts (bench 04, soak);
to puppet's node: direct in 4 of 4.

## Findings

1. **`--compare-relay` could not work on a closed network:** its second
   endpoint used a fresh temporary identity, which has no certificate. Fixed
   with `--compare-relay-key` (a second registered key); without it the
   comparison is now skipped with a reason instead of failing.
2. **The office balances flows over two ISPs, and that breaks hole punching
   to firewalled peers about half the time.** The operator reported that the
   public IP drifts; the measurements show how. Every `natcheck` run uses one
   fresh socket towards three reflector addresses. In 6 of 20 runs all three
   saw the same ISP and port; in the others the ISP differed between
   destinations, even between two ports of the same IP (so the load balancer
   picks the ISP per flow, by destination address and port, about 50/50).
   Within one ISP the NAT is endpoint-independent: whenever two destinations
   went through the same ISP they saw the same port. Filtering is
   address-and-port dependent (APDF).
   Consequence: the Pi learns its public address from the relay (QAD), which
   is the mapping of the flow to the relay. The flow to a peer leaves through
   the same ISP only about half the time. If it uses the other ISP, the peer
   punches towards the wrong address. Master's ufw then drops the Pi's packets,
   because they come from an address master never sent to, and the Pi's NAT
   drops master's. That fits the results: to master direct only 2 of 7 times,
   to puppet's node 4 of 4. Puppet's side accepts the Pi's packets from either
   address, and iroh learns the real address from them. A connection that
   became direct stayed direct for 30 minutes, because one flow keeps its ISP.
   Before 2026-11-15 master had no firewall and would have accepted the Pi's
   packets, as puppet's side does.
3. **Possible fixes, to discuss:**
   - Learn and publish both public addresses. Each ISP's NAT keeps one port per
     socket, so QAD answers from several relays (different IPs and ports)
     reveal both mappings. Peers would then punch towards both. Today the
     node uses one relay and iroh reports one public IPv4. This needs more
     relays in the node config and possibly an iroh change.
   - A second relay on another IP also lets `bench` classify the NAT
     (now "unknown, needs 2 relays").
   - Keep ufw on master: firewalled peers are the realistic case, and the
     relay fallback works (0 failures in all runs). Relayed RTT to master is
     about 71 ms against 50 ms direct; direct throughput is about 56 / 43 Mbps
     against about 33 / 24 Mbps relayed.
