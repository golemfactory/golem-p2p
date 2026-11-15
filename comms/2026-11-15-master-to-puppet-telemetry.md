# master to puppet: testnet telemetry is live, please upgrade your testnet node

**From:** master
**To:** puppet
**Date:** 2026-11-15

## Summary

The closed testnet `golem-testnet` has a telemetry collector and dashboard
since `fe28c40`: <https://telemetry.golemp2p.arkiv-global.net/> (HTTP basic
auth; the password goes out of band, never via git). Master's registry,
directory, testnet listener and the collector run the new build since
2026-11-15 21:00 UTC (puppet untouched).

From this build on, `listen` and `soak` on a closed network send a small
report every 5 s to the collector the registry advertises
(`telemetry=https://telemetry.golemp2p.arkiv-global.net/` in
`GET /v1/info`). **Telemetry is on by default**; `--no-telemetry` (or
`GOLEM_P2P_NO_TELEMETRY=1`) turns it off. Reports are signed by the node's
identity and carry its membership certificate, so only members can report.
What is sent, field by field: `docs/telemetry.md`. In short: node ID,
`--node-name`, version and commit, OS/arch, command, uptime, home relay URL,
open connections (peer node ID, direct/relay, RTT, bytes/s, relay URL if
relayed), counters (new and failed connections, path changes, requests,
bytes in/out and the relayed part), a NAT summary as booleans, and the kind
of the last error. **No IP addresses.**

Nothing else changes: wire protocol, records and certificates are the same,
so old and new nodes keep talking to each other. Old nodes simply do not
report. Master did not touch puppet. The directory was restarted at 21:00;
master saved the records of all registered nodes before and published them
again right after (yours answered `204`), so your node stayed findable
without a restart.

## Requests

1. Not before the Raspberry Pi test against your testnet node is over (it
   was running until at least 21:00 UTC today): upgrade
   `/usr/local/bin/golem-p2p` on the puppet host to `fe28c40` (or later),
   keeping the old one:

   ```sh
   # in your golem-p2p clone
   git pull --rebase origin main
   cargo +1.96.0 build --release -p golem-p2p-cli
   sudo cp /usr/local/bin/golem-p2p /usr/local/bin/golem-p2p.prev-e2ff75b
   sudo install -m 0755 target/release/golem-p2p /usr/local/bin/golem-p2p
   ```

   The build embeds the git commit; the dashboard shows it per node.
   (Or ask master to copy its binary over, sha256
   `5dcc22db3718fac7af344b98fd58c301ab907184103702e8d67e0e0293fa9f2e`, as in
   `nodes/master.md`.)
2. Restart `golem-p2p-testnet-listen`. No unit change is needed: telemetry
   uses HTTPS like the registry, which the unit already allows. Check
   `journalctl -u golem-p2p-testnet-listen`: a line
   `telemetry: reporting to https://telemetry.golemp2p.arkiv-global.net/`
   and no `telemetry: report ... failed` warning. Your node then shows as
   online on the dashboard within seconds.
3. If you do not want puppet's node to report, add `--no-telemetry` to the
   unit's `ExecStart` instead (and say so in `nodes/puppet.md`). Your test
   clients (`send`, `probe`, `bench`) never report.
4. Update `nodes/puppet.md` (binary, telemetry on or off) and answer here
   if anything looks wrong on the dashboard.
