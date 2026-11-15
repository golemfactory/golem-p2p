# Telemetry of closed networks

Nodes of a closed network (one with a registry, see
[design.md](design.md), section 13a) send a small report about themselves
to the network's telemetry collector every few seconds, so the health of
the network can be watched in one place: which nodes are up, how they are
connected (direct or through a relay), round-trip times, traffic and
failures. The collector shows it on a dashboard, as JSON and as Prometheus
metrics.

Testnet (`golem-testnet`): dashboard at
<https://telemetry.golemp2p.arkiv-global.net/> (HTTP basic auth; ask the
master node's operator for the credentials). Setup in
[nodes/master.md](../nodes/master.md).

## On the node

**Who reports.** `listen` and `soak` on a closed network (`--registry`),
unless turned off. `send`, `probe` and `bench` never report, and nothing
reports on an open network (there is no certificate to authenticate with).

**Opting out.** `--no-telemetry`, or `GOLEM_P2P_NO_TELEMETRY=1` in the
environment (`1`, `true`, `yes`, `on`; `0`, `false`, `no`, `off` leave it
on). The `telemetry` event (`--json`) and the log say whether reports are
on.

**Where to.** The registry advertises the collector: `GET /v1/info` has a
line `telemetry=<url>` (`registry serve --telemetry-url URL`). The node
asks the registry once at start (again every 10 minutes while it knows no
URL), so nothing is hardcoded. `--telemetry URL` (or `GOLEM_P2P_TELEMETRY`)
overrides it. Reports go to `<url>/v1/report`.

**How often.** Every `--telemetry-interval` (default 5 s, at least 1 s;
`GOLEM_P2P_TELEMETRY_INTERVAL`), the first one right after start, and a
last one (`stopping: true`) on a clean exit (SIGINT, SIGTERM,
`--duration`), so the dashboard shows the node offline at once.

**Never in the way.** One report at a time, each bounded by a timeout of
the interval (at most 10 s; connect at most 5 s), no retries: a failed
report is simply replaced by the next one. The first failure is logged at
`warn`, later ones at `debug`, and recovery at `info`. Reports do not
touch the node's connections (they are plain HTTPS to the collector).

## What is sent

One JSON object per report, at most 64 KiB (a few hundred bytes plus about
200 bytes per open connection). **No IP addresses** and no other location
data: the remote address of a direct path is never included, the NAT
summary has booleans only, and errors are sent as a kind (`timeout`,
`refused`, ...), never as a message (messages can contain addresses).
Relay URLs are included: relays are public servers.

| field | meaning |
|---|---|
| `v` | format version, 1 |
| `node_id` | the node's default identity (it signs the report) |
| `ts_ms` | time of the report, Unix ms (the node's clock) |
| `seq` | report number of this process, from 1 |
| `instance` | random ID of this process, new on every start (hex) |
| `name` | `--node-name`, if given (at most 64 characters) |
| `version`, `commit` | golem-p2p version and the git commit of the build (if known) |
| `os`, `arch` | e.g. `linux`, `x86_64` |
| `command` | `listen` or `soak` |
| `uptime_s` | seconds since start |
| `interval_s` | the report interval |
| `transport` | `direct_and_relay`, `relay_only` or `direct_only` |
| `home_relay`, `home_relay_connected` | the home relay's URL and whether it is connected |
| `nat` | iroh's net report, booleans only: `udp_v4`, `udp_v6` (a relay answered QUIC address discovery), `behind_nat_v4`, `port_preserved_v4`, `mapping_varies` (hard NAT; needs two relays), `captive_portal`; null before the first net report |
| `connections` | open connections (at most 256): `peer` (its default identity), `direction` (`in`/`out`), `path` (`direct`/`relay`/`unknown`), `relay` (the relay URL on a relayed path), `rtt_ms` (QUIC's smoothed RTT of the selected path), `path_s` (seconds on the current kind of path), `rate_in`, `rate_out` (bytes/s of the connection since the last report) |
| `connections_open` | number of open connections, also beyond the list |
| `totals` | counters since start: `conns_new` (connections that completed the handshake, both directions), `conns_failed` (failed connects and incoming handshakes), `path_changes` (between direct and relay), `requests_ok`, `requests_failed` (served by `listen`, answered for `soak`), `bytes_in`, `bytes_out` (UDP, all paths), `bytes_in_relay`, `bytes_out_relay` (the relayed part) |
| `delta`, `delta_s` | the same counters since the last accepted report, and the time that covers |
| `last_error` | `kind` and time (`at_ms`) of the last failed request, or null |
| `stopping` | true in the last report of a node that stops |

The collector necessarily sees the address the HTTPS request comes from (as
any web server does); it does not store it. The nginx in front of the
testnet collector keeps its usual access log.

## Authentication

Only members of the network can report. Each report is signed by the
node's default identity (secp256k1, as records are) over
`SHA-256("golem-p2p/telemetry/v1\0" || body)`, and carries the node's
current membership certificate:

```
POST /v1/report
content-type: application/json
x-golem-p2p-signature: <65-byte signature, hex>
x-golem-p2p-membership: <membership certificate, hex>

{"v":1,"node_id":"0x...",...}
```

The collector (library: `golem_p2p::telemetry::verify`) checks, offline,
with the registry's public key and the network name, as peers do in the
handshake: the certificate is signed by the registry, for this network,
valid now (5 min clock tolerance) and covers the identity that signed the
body; and that identity is the report's `node_id`. Then: the report parses
and its fields are within bounds; `ts_ms` is within 5 min of the
collector's clock and later than the last accepted report of that node (a
replayed or reordered report is refused, also after a collector restart);
and the node has not sent more than one report per `--min-report-interval`
(default 4 s) on average, with bursts of 3.

Answers: `204` accepted; `400` malformed or stale timestamp; `401` unsigned
or bad signature; `403` not a member (or signed by another identity than
`node_id`); `409` replay; `413` larger than 64 KiB; `429` rate limited.
`/metrics` counts the answers by reason.

## The collector

```sh
golem-p2p telemetry serve --listen 127.0.0.1:8091 \
    --registry-key <registry public key hex> --network golem-testnet \
    --store /var/lib/golem-p2p-telemetry
golem-p2p registry serve ... --telemetry-url https://telemetry.example/
```

It serves plain HTTP: put a TLS proxy in front, open `POST /v1/report` to
the nodes and protect everything else (dashboard, JSON API, metrics) with
authentication, e.g. nginx basic auth (as on master). The dashboard uses
relative URLs only, so it also works under a path prefix.

**Node status.** Online: a report within 3 report intervals (+2 s);
stale: up to 60 s (at least 6 intervals) since the last one; offline after
that, or at once after a `stopping` report. Peers that do not report (opted
out, or not `listen`/`soak`) appear while a reporting node is connected to
them, without details.

**Links.** A connection is reported by both ends; the collector merges the
two views into one link per pair of nodes: the path of the newer report,
the smaller RTT estimate, the larger traffic. A link reported by only one
end is shown too.

**History.** Every 5 s the collector takes a sample: nodes online, links by
path, traffic (bytes/s, also over relays) and the counts of the interval
(new and failed connections, path changes, requests), for the network, per
node and per link. 5 s samples are kept for an hour, per-minute aggregates
(gauges averaged, counts summed) for `--retention` (default 7 days). Both
are kept in memory and in the `--store` directory, which stays bounded:

| file | content |
|---|---|
| `fine.jsonl` | the 5 s samples (rewritten hourly to hold about the last hour) |
| `minutes-YYYY-MM-DD.jsonl` | the per-minute aggregates of a day (UTC); files older than the retention are deleted |
| `nodes.json` | the last report of every node seen within the retention (for restarts and replay protection), rewritten every minute |

A sample is a few hundred bytes plus about 100 per online node and link: a
10-node network uses a few MB a day.

## Dashboard and API

`GET /` is the dashboard, one self-contained page (no external resources):
a live map of the network (nodes, relays, links coloured by path, width by
traffic, through the relay when relayed; click a node or link for details
and its history), summary tiles, a node table and charts of the last 15
minutes to 7 days. It refreshes every 5 s.

| method and path | answer |
|---|---|
| `GET /v1/state` | the live view: `nodes` (status, version, uptime, connections, rates, totals, last error), `relays`, merged `links`, `summary` |
| `GET /v1/nodes` | the `nodes` of `/v1/state` |
| `GET /v1/history?range=S[&node=ID][&link=A,B]` | time series of the last `S` seconds (60 to the retention): 5 s steps up to an hour, per-minute aggregates merged to at most 720 points beyond; `network`, and `node` or `link` if asked for |
| `GET /metrics` | Prometheus text format |
| `GET /health` | `ok` |

Prometheus metrics (labels `node_id`, `name` per node):
`golem_p2p_nodes_online`, `golem_p2p_nodes_known`,
`golem_p2p_links{path}`, `golem_p2p_node_up`,
`golem_p2p_node_info{version,commit,os,arch,command}`,
`golem_p2p_node_last_report_age_seconds`,
`golem_p2p_node_uptime_seconds`, `golem_p2p_node_connections{path}`,
the counters `golem_p2p_node_connections_new_total`,
`golem_p2p_node_connections_failed_total`,
`golem_p2p_node_path_changes_total`,
`golem_p2p_node_requests_total{result}`,
`golem_p2p_node_bytes_total{direction,path}` (from the nodes' totals, so
they restart at 0 when a node restarts, as Prometheus expects of
counters), `golem_p2p_link_rtt_seconds{a,b,path}` and
`golem_p2p_telemetry_reports_total{result}`. A scrape configuration for a
later Grafana:

```yaml
scrape_configs:
  - job_name: golem-p2p-telemetry
    scheme: https
    metrics_path: /metrics
    basic_auth: {username: golem, password_file: /etc/prometheus/golem-telemetry.pass}
    static_configs: [{targets: [telemetry.golemp2p.arkiv-global.net]}]
```
