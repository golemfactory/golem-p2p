//! `golem-p2p telemetry serve`: the collector of a closed network's
//! telemetry, with a dashboard, a JSON API and Prometheus metrics.
//!
//! * `POST /v1/report`: a node's signed report ([`golem_p2p::telemetry`]).
//!   Members only, at most [`MAX_REPORT_SIZE`] bytes, timestamps within 5 min
//!   of the collector's clock and increasing per node (no replays), and per
//!   node at most one report per `--min-report-interval` on average (bursts
//!   of [`RATE_BURST`]).
//! * `GET /`: the dashboard; `GET /v1/state`, `/v1/nodes`, `/v1/history`:
//!   JSON; `GET /metrics`: Prometheus text format; `GET /health`.
//!
//! History: every [`TICK`] the collector takes a sample of the network (nodes
//! online, links by path, traffic, failures; per node and per link). The
//! 5 s samples are kept for [`FINE_KEEP`], per-minute aggregates for
//! `--retention` (default 7 days), both in memory and as JSON lines in the
//! `--store` directory, so storage stays bounded.
//!
//! Only the report route is meant to be public; put the others behind
//! authentication (nginx basic auth on master, see `nodes/master.md`).

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as _,
    io::Write as _,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::{Args, Subcommand};
use golem_p2p::{
    MembershipPolicy, NodeId,
    record::MAX_CLOCK_SKEW,
    telemetry::{CERT_HEADER, MAX_REPORT_SIZE, ReportError, SIGNATURE_HEADER},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{debug, info, warn};

use super::{MAX_NAME_LEN, MAX_REPORT_CONNECTIONS, REPORT_VERSION, Report};
use crate::{out::Out, parse};

/// Time between two samples.
pub const TICK: Duration = Duration::from_secs(5);

/// How long the 5 s samples are kept.
pub const FINE_KEEP: Duration = Duration::from_secs(3600);

/// Reports a node may send at once before the rate limit applies.
pub const RATE_BURST: f64 = 3.0;

/// Most nodes the collector keeps (members only, so far more than needed).
const MAX_NODES: usize = 10_000;

/// Most points in one history answer (larger ranges use larger steps).
const MAX_POINTS: u64 = 720;

/// The dashboard page.
const DASHBOARD: &str = include_str!("dashboard.html");

#[derive(Subcommand)]
pub enum TelemetryCmd {
    /// Run the telemetry collector: accepts members' reports, keeps a
    /// bounded history, serves a dashboard, a JSON API and Prometheus
    /// metrics. Put it behind a TLS proxy; protect everything except
    /// `POST /v1/report` (which members sign) with authentication.
    Serve(ServeArgs),
}

#[derive(Args)]
pub struct ServeArgs {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:8091")]
    listen: SocketAddr,
    /// The registry's public key (hex): reports must carry a valid
    /// membership certificate of this key.
    #[arg(long, value_name = "HEX")]
    registry_key: String,
    /// Network name.
    #[arg(long, default_value = "golem-testnet")]
    network: String,
    /// Directory for the history and the last state of every node (created
    /// if missing).
    #[arg(long)]
    store: PathBuf,
    /// How long the per-minute history is kept.
    #[arg(long, default_value = "7d", value_parser = parse::duration)]
    retention: Duration,
    /// Per node, at most one report per this time on average (with bursts
    /// of 3); more are refused with 429.
    #[arg(long, default_value = "4s", value_parser = parse::duration)]
    min_report_interval: Duration,
}

pub async fn run(cmd: TelemetryCmd, out: &Out) -> anyhow::Result<()> {
    let TelemetryCmd::Serve(args) = cmd;
    let policy = MembershipPolicy::new(
        args.network.clone(),
        crate::parse_registry_key(&args.registry_key)?,
    );
    if args.min_report_interval.is_zero() || args.retention < Duration::from_secs(3600) {
        anyhow::bail!("--min-report-interval must be positive and --retention at least 1h");
    }
    let config = Config {
        policy,
        store: Some(args.store.clone()),
        retention: args.retention,
        min_interval: args.min_report_interval,
    };
    let shared = Collector::open(config)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    let url = format!("http://{}/", listener.local_addr()?);
    if out.json() {
        out.event("telemetry_collector")
            .field("url", url)
            .field("network", args.network)
            .emit();
    } else {
        println!(
            "telemetry collector of network {} listening on {url}, store {}",
            args.network,
            args.store.display()
        );
    }
    let ticker = tokio::spawn(tick_loop(shared.clone()));
    let res = tokio::select! {
        r = axum::serve(listener, router(shared.clone())) => r.map_err(Into::into),
        _ = crate::shutdown_signal() => Ok(()),
    };
    ticker.abort();
    shared.lock().expect("poisoned").save_nodes();
    res
}

/// Settings of a collector.
#[derive(Debug, Clone)]
pub struct Config {
    pub policy: MembershipPolicy,
    /// `None`: memory only (tests).
    pub store: Option<PathBuf>,
    pub retention: Duration,
    pub min_interval: Duration,
}

/// A collector shared between the HTTP handlers and the sampler.
pub type Shared = Arc<Mutex<Collector>>;

/// What the collector knows about a node that reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeState {
    report: Report,
    /// When the last report arrived (collector clock), Unix ms.
    received_ms: u64,
    /// Highest report timestamp accepted (replay protection).
    last_ts_ms: u64,
    /// Bytes per second over the last report's delta.
    rate_in: f64,
    rate_out: f64,
    rate_relay: f64,
    #[serde(skip)]
    bucket: Option<Bucket>,
}

/// Token bucket of the per-node rate limit.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    at_ms: u64,
}

/// Counts received since the last sample.
#[derive(Debug, Clone, Default)]
struct Pending {
    net: Counts,
    nodes: BTreeMap<String, Counts>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
struct Counts {
    #[serde(default)]
    conns_new: u64,
    #[serde(default)]
    conns_failed: u64,
    #[serde(default)]
    path_changes: u64,
    #[serde(default)]
    requests_ok: u64,
    #[serde(default)]
    requests_failed: u64,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.conns_new += o.conns_new;
        self.conns_failed += o.conns_failed;
        self.path_changes += o.path_changes;
        self.requests_ok += o.requests_ok;
        self.requests_failed += o.requests_failed;
    }
}

/// One sample of the history: 5 s, or an aggregate (1 min on disk, larger
/// in answers). Gauges are averages over the period, counts are sums.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Start, Unix ms.
    t: u64,
    /// Length, seconds.
    s: u64,
    net: NetSample,
    #[serde(default)]
    nodes: BTreeMap<String, NodeSample>,
    /// By `a,b` (a < b).
    #[serde(default)]
    links: BTreeMap<String, LinkSample>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct NetSample {
    nodes: f64,
    direct: f64,
    relay: f64,
    rate_in: f64,
    rate_out: f64,
    rate_relay: f64,
    #[serde(flatten)]
    counts: Counts,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct NodeSample {
    online: f64,
    direct: f64,
    relay: f64,
    rate_in: f64,
    rate_out: f64,
    #[serde(flatten)]
    counts: Counts,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct LinkSample {
    path: Option<String>,
    /// Fraction of the period on a direct path, of the time the link existed.
    direct_frac: f64,
    rtt_ms: Option<f64>,
    rate: f64,
    /// Samples the link existed in (for averaging).
    #[serde(default = "one")]
    n: u64,
}

fn one() -> u64 {
    1
}

/// A link between two nodes, merged from what both ends report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    pub a: String,
    pub b: String,
    pub path: String,
    pub relay: Option<String>,
    pub rtt_ms: Option<f64>,
    pub rate: f64,
    pub path_since: Option<u64>,
    pub reported_by: Vec<String>,
    pub path_changes: u64,
}

/// Path changes seen per link by the collector.
#[derive(Debug, Clone)]
struct LinkTrack {
    path: String,
    changes: u64,
}

/// Results of report handling, for `/metrics`.
#[derive(Debug, Default)]
struct ReportStats {
    by_result: BTreeMap<&'static str, u64>,
}

/// The collector's state.
pub struct Collector {
    config: Config,
    nodes: BTreeMap<String, NodeState>,
    pending: Pending,
    links: BTreeMap<String, LinkTrack>,
    fine: VecDeque<Sample>,
    minutes: VecDeque<Sample>,
    stats: ReportStats,
    /// Start of the last fine-file compaction, Unix ms.
    fine_compacted_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// A refused report: status and reason (also the `/metrics` label).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    pub status: StatusCode,
    pub reason: &'static str,
}

const fn refuse(status: StatusCode, reason: &'static str) -> Refusal {
    Refusal { status, reason }
}

/// Characters a node name or version may contain.
fn printable(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// Checks the content of a report: version, sizes, names, node IDs.
fn validate(r: &Report) -> Result<(), &'static str> {
    if r.v != REPORT_VERSION {
        return Err("unsupported report version");
    }
    let short = |s: &str, n: usize| s.chars().count() <= n && printable(s);
    if !r.name.as_deref().is_none_or(|n| short(n, MAX_NAME_LEN))
        || !short(&r.version, 32)
        || !r.commit.as_deref().is_none_or(|c| short(c, 64))
        || !short(&r.os, 32)
        || !short(&r.arch, 32)
        || !short(&r.command, 16)
        || !short(&r.transport, 32)
        || !short(&r.instance, 64)
        || !r.home_relay.as_deref().is_none_or(|u| short(u, 256))
        || !r.last_error.as_ref().is_none_or(|e| short(&e.kind, 32))
    {
        return Err("a field is too long or has control characters");
    }
    if r.connections.len() > MAX_REPORT_CONNECTIONS {
        return Err("too many connections");
    }
    for c in &r.connections {
        if c.peer.parse::<NodeId>().is_err()
            || !matches!(c.path.as_str(), "direct" | "relay" | "unknown")
            || !matches!(c.direction.as_str(), "in" | "out")
            || !c.relay.as_deref().is_none_or(|u| short(u, 256))
            || !c.rate_in.is_finite()
            || !c.rate_out.is_finite()
            || !c.rtt_ms.is_none_or(f64::is_finite)
        {
            return Err("invalid connection entry");
        }
    }
    if !r.interval_s.is_finite() || !r.delta_s.is_finite() {
        return Err("invalid number");
    }
    Ok(())
}

fn hex_header(h: &HeaderMap, name: &str) -> Option<Vec<u8>> {
    hex::decode(h.get(name)?.to_str().ok()?.trim()).ok()
}

impl Collector {
    /// A collector with an empty history (memory only if `config.store` is
    /// `None`).
    pub fn new(config: Config) -> Self {
        Self {
            config,
            nodes: BTreeMap::new(),
            pending: Pending::default(),
            links: BTreeMap::new(),
            fine: VecDeque::new(),
            minutes: VecDeque::new(),
            stats: ReportStats::default(),
            fine_compacted_ms: now_ms(),
        }
    }

    /// Opens the store (creating the directory) and loads what it holds.
    pub fn open(config: Config) -> anyhow::Result<Shared> {
        let mut c = Self::new(config);
        if let Some(dir) = c.config.store.clone() {
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            c.load(&dir, now_ms());
        }
        Ok(Arc::new(Mutex::new(c)))
    }

    fn load(&mut self, dir: &Path, now: u64) {
        if let Ok(text) = std::fs::read_to_string(dir.join("nodes.json")) {
            match serde_json::from_str::<BTreeMap<String, NodeState>>(&text) {
                Ok(nodes) => self.nodes = nodes,
                Err(e) => warn!("ignoring nodes.json: {e}"),
            }
        }
        let since = now.saturating_sub(self.config.retention.as_millis() as u64);
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| minute_file_day(p).is_some())
            .collect();
        files.sort();
        for f in files {
            for s in read_samples(&f) {
                if s.t >= since {
                    self.minutes.push_back(s);
                }
            }
        }
        let fine_since = now.saturating_sub(FINE_KEEP.as_millis() as u64);
        self.fine = read_samples(&dir.join("fine.jsonl"))
            .into_iter()
            .filter(|s| s.t >= fine_since)
            .collect();
        self.prune_files(now);
        info!(
            nodes = self.nodes.len(),
            minutes = self.minutes.len(),
            fine = self.fine.len(),
            "telemetry store loaded"
        );
    }

    fn count(&mut self, result: &'static str) {
        *self.stats.by_result.entry(result).or_default() += 1;
    }

    /// Handles a report: authentication, checks, rate limit, then stores it.
    pub fn accept(
        &mut self,
        body: &[u8],
        headers: &HeaderMap,
        now: u64,
    ) -> Result<String, Refusal> {
        let r = self.accept_inner(body, headers, now);
        self.count(match &r {
            Ok(_) => "accepted",
            Err(e) => e.reason,
        });
        r
    }

    fn accept_inner(
        &mut self,
        body: &[u8],
        headers: &HeaderMap,
        now: u64,
    ) -> Result<String, Refusal> {
        if body.len() > MAX_REPORT_SIZE {
            return Err(refuse(StatusCode::PAYLOAD_TOO_LARGE, "too_large"));
        }
        let (Some(sig), Some(cert)) = (
            hex_header(headers, SIGNATURE_HEADER),
            hex_header(headers, CERT_HEADER),
        ) else {
            return Err(refuse(StatusCode::UNAUTHORIZED, "unsigned"));
        };
        let sender = golem_p2p::telemetry::verify(
            &self.config.policy,
            body,
            &sig,
            &cert,
            UNIX_EPOCH + Duration::from_millis(now),
        )
        .map_err(|e| match e {
            ReportError::TooLarge => refuse(StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            ReportError::Signature => refuse(StatusCode::UNAUTHORIZED, "bad_signature"),
            ReportError::Membership(_) | ReportError::NotCovered(_) => {
                debug!("telemetry report refused: {e}");
                refuse(StatusCode::FORBIDDEN, "not_member")
            }
        })?;
        let report: Report = serde_json::from_slice(body)
            .map_err(|_| refuse(StatusCode::BAD_REQUEST, "malformed"))?;
        let id = sender.node_id.to_string();
        if report.node_id != id {
            return Err(refuse(StatusCode::FORBIDDEN, "node_mismatch"));
        }
        validate(&report).map_err(|e| {
            debug!(node = %id, "telemetry report refused: {e}");
            refuse(StatusCode::BAD_REQUEST, "malformed")
        })?;
        let skew = MAX_CLOCK_SKEW.as_millis() as u64;
        if report.ts_ms.abs_diff(now) > skew {
            return Err(refuse(StatusCode::BAD_REQUEST, "stale"));
        }
        if !self.nodes.contains_key(&id) && self.nodes.len() >= MAX_NODES {
            return Err(refuse(StatusCode::SERVICE_UNAVAILABLE, "full"));
        }
        let min_ms = self.config.min_interval.as_millis() as f64;
        let prev = self.nodes.get(&id);
        if prev.is_some_and(|p| report.ts_ms <= p.last_ts_ms) {
            return Err(refuse(StatusCode::CONFLICT, "replay"));
        }
        // Token bucket: RATE_BURST tokens, one more per min_interval.
        let mut bucket = prev.and_then(|p| p.bucket).unwrap_or(Bucket {
            tokens: RATE_BURST,
            at_ms: now,
        });
        bucket.tokens =
            (bucket.tokens + now.saturating_sub(bucket.at_ms) as f64 / min_ms).min(RATE_BURST);
        bucket.at_ms = now;
        if bucket.tokens < 1.0 {
            if let Some(p) = self.nodes.get_mut(&id) {
                p.bucket = Some(bucket);
            }
            return Err(refuse(StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
        }
        bucket.tokens -= 1.0;

        let d = &report.delta;
        let secs = if report.delta_s > 0.0 {
            report.delta_s
        } else {
            report.interval_s.max(1.0)
        };
        let counts = Counts {
            conns_new: d.conns_new,
            conns_failed: d.conns_failed,
            path_changes: d.path_changes,
            requests_ok: d.requests_ok,
            requests_failed: d.requests_failed,
        };
        self.pending.net.add(&counts);
        self.pending
            .nodes
            .entry(id.clone())
            .or_default()
            .add(&counts);
        if prev.is_none() {
            info!(node = %id, name = ?report.name, version = %report.version, "telemetry: new node");
        }
        let state = NodeState {
            received_ms: now,
            last_ts_ms: report.ts_ms,
            rate_in: round(d.bytes_in as f64 / secs),
            rate_out: round(d.bytes_out as f64 / secs),
            rate_relay: round((d.bytes_in_relay + d.bytes_out_relay) as f64 / secs),
            bucket: Some(bucket),
            report,
        };
        self.nodes.insert(id.clone(), state);
        Ok(id)
    }

    /// `online`, `stale` or `offline`.
    fn status(n: &NodeState, now: u64) -> &'static str {
        if n.report.stopping {
            return "offline";
        }
        let age = now.saturating_sub(n.received_ms);
        let interval = (n.report.interval_s.max(1.0) * 1000.0) as u64;
        if age <= 3 * interval + 2000 {
            "online"
        } else if age <= (6 * interval).max(60_000) {
            "stale"
        } else {
            "offline"
        }
    }

    /// The links reported by nodes that are online or stale, both views of
    /// a link merged.
    fn merged_links(&self, now: u64) -> Vec<Link> {
        struct View<'a> {
            by: &'a str,
            ts: u64,
            c: &'a super::Conn,
        }
        let mut views: BTreeMap<(String, String), Vec<View>> = BTreeMap::new();
        for (id, n) in &self.nodes {
            if Self::status(n, now) == "offline" {
                continue;
            }
            for c in &n.report.connections {
                if c.peer == *id {
                    continue;
                }
                let key = if *id < c.peer {
                    (id.clone(), c.peer.clone())
                } else {
                    (c.peer.clone(), id.clone())
                };
                views.entry(key).or_default().push(View {
                    by: id,
                    ts: n.report.ts_ms,
                    c,
                });
            }
        }
        let mut out = Vec::new();
        for ((a, b), vs) in views {
            // A node may hold two connections to a peer for a moment; the
            // newest report's busiest one stands for the side.
            let latest = vs
                .iter()
                .max_by(|x, y| {
                    (x.ts, x.c.path != "unknown")
                        .cmp(&(y.ts, y.c.path != "unknown"))
                        .then(rate(x.c).total_cmp(&rate(y.c)))
                })
                .expect("not empty");
            let mut by: Vec<String> = vs.iter().map(|v| v.by.to_string()).collect();
            by.dedup();
            let rtt = vs.iter().filter_map(|v| v.c.rtt_ms).min_by(f64::total_cmp);
            let key = format!("{a},{b}");
            out.push(Link {
                path: latest.c.path.clone(),
                relay: latest
                    .c
                    .relay
                    .clone()
                    .or_else(|| vs.iter().find_map(|v| v.c.relay.clone()))
                    .filter(|_| latest.c.path == "relay"),
                rtt_ms: rtt,
                rate: vs.iter().map(|v| rate(v.c)).fold(0.0, f64::max),
                path_since: latest.c.path_s.map(|s| latest.ts.saturating_sub(s * 1000)),
                reported_by: by,
                path_changes: self.links.get(&key).map_or(0, |t| t.changes),
                a,
                b,
            });
        }
        out
    }

    /// Takes a sample at `now`: called every [`TICK`].
    pub fn tick(&mut self, now: u64) {
        let t = now - now % TICK.as_millis() as u64;
        let pending = std::mem::take(&mut self.pending);
        let links = self.merged_links(now);
        // Path changes per link, as the collector sees them.
        let mut seen = BTreeSet::new();
        for l in &links {
            let key = format!("{},{}", l.a, l.b);
            if l.path != "unknown" {
                let track = self.links.entry(key.clone()).or_insert(LinkTrack {
                    path: l.path.clone(),
                    changes: 0,
                });
                if track.path != l.path {
                    track.changes += 1;
                    track.path = l.path.clone();
                }
            }
            seen.insert(key);
        }
        self.links.retain(|k, _| seen.contains(k));

        let mut s = Sample {
            t,
            s: TICK.as_secs(),
            ..Default::default()
        };
        for (id, n) in &self.nodes {
            if Self::status(n, now) != "online" {
                continue;
            }
            let (d, r) = conn_paths(&n.report);
            s.net.nodes += 1.0;
            s.net.rate_in += n.rate_in;
            s.net.rate_out += n.rate_out;
            s.net.rate_relay += n.rate_relay;
            s.nodes.insert(
                id.clone(),
                NodeSample {
                    online: 1.0,
                    direct: d as f64,
                    relay: r as f64,
                    rate_in: n.rate_in,
                    rate_out: n.rate_out,
                    counts: Counts::default(),
                },
            );
        }
        for (id, c) in pending.nodes {
            s.nodes.entry(id).or_default().counts = c;
        }
        s.net.counts = pending.net;
        for l in &links {
            match l.path.as_str() {
                "direct" => s.net.direct += 1.0,
                "relay" => s.net.relay += 1.0,
                _ => {}
            }
            s.links.insert(
                format!("{},{}", l.a, l.b),
                LinkSample {
                    path: Some(l.path.clone()),
                    direct_frac: if l.path == "direct" { 1.0 } else { 0.0 },
                    rtt_ms: l.rtt_ms,
                    rate: l.rate,
                    n: 1,
                },
            );
        }
        self.push_fine(s, now);
    }

    fn push_fine(&mut self, s: Sample, now: u64) {
        let minute = |t: u64| t - t % 60_000;
        // A new minute: aggregate the last one.
        if let Some(last) = self.fine.back()
            && minute(last.t) != minute(s.t)
        {
            let m = minute(last.t);
            let of_minute: Vec<&Sample> = self.fine.iter().filter(|x| minute(x.t) == m).collect();
            let agg = aggregate(&of_minute, m, 60);
            self.push_minute(agg, now);
        }
        self.append("fine.jsonl", &s);
        self.fine.push_back(s);
        let keep = now.saturating_sub(FINE_KEEP.as_millis() as u64);
        while self.fine.front().is_some_and(|x| x.t < keep) {
            self.fine.pop_front();
        }
        // Rewrite the fine file once an hour, so it holds about an hour.
        if now.saturating_sub(self.fine_compacted_ms) > FINE_KEEP.as_millis() as u64 {
            self.fine_compacted_ms = now;
            self.rewrite_fine();
        }
    }

    fn push_minute(&mut self, m: Sample, now: u64) {
        let day = day_of(m.t);
        self.append(&format!("minutes-{day}.jsonl"), &m);
        self.minutes.push_back(m);
        let keep = now.saturating_sub(self.config.retention.as_millis() as u64);
        while self.minutes.front().is_some_and(|x| x.t < keep) {
            self.minutes.pop_front();
        }
        // Nodes not seen within the retention are forgotten, and so are
        // files of days before it (checked hourly).
        let retention = self.config.retention.as_millis() as u64;
        self.nodes
            .retain(|_, n| now.saturating_sub(n.received_ms) <= retention);
        if self.minutes.len().is_multiple_of(60) {
            self.prune_files(now);
        }
        self.save_nodes();
    }

    fn append(&self, file: &str, s: &Sample) {
        let Some(dir) = &self.config.store else {
            return;
        };
        let path = dir.join(file);
        let line = serde_json::to_string(s).expect("encode sample");
        let r = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"));
        if let Err(e) = r {
            warn!("writing {}: {e}", path.display());
        }
    }

    fn rewrite_fine(&self) {
        let Some(dir) = &self.config.store else {
            return;
        };
        let mut text = String::new();
        for s in &self.fine {
            text.push_str(&serde_json::to_string(s).expect("encode sample"));
            text.push('\n');
        }
        if let Err(e) = write_atomic(&dir.join("fine.jsonl"), text.as_bytes()) {
            warn!("rewriting fine.jsonl: {e}");
        }
    }

    /// Saves the last state of every node (for restarts).
    pub fn save_nodes(&self) {
        let Some(dir) = &self.config.store else {
            return;
        };
        let text = serde_json::to_string(&self.nodes).expect("encode nodes");
        if let Err(e) = write_atomic(&dir.join("nodes.json"), text.as_bytes()) {
            warn!("writing nodes.json: {e}");
        }
    }

    fn prune_files(&self, now: u64) {
        let Some(dir) = &self.config.store else {
            return;
        };
        let oldest = day_of(now.saturating_sub(self.config.retention.as_millis() as u64));
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if let Some(day) = minute_file_day(&p)
                && day < oldest
            {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    /// The live view: nodes, relays, links, summary (`GET /v1/state`).
    pub fn state(&self, now: u64) -> Value {
        let links = self.merged_links(now);
        let mut nodes = Vec::new();
        let mut known: BTreeSet<&str> = BTreeSet::new();
        let mut relays: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let (mut online, mut rate_in, mut rate_out) = (0, 0.0, 0.0);
        for (id, n) in &self.nodes {
            known.insert(id);
            let status = Self::status(n, now);
            let r = &n.report;
            let (d, rl) = conn_paths(r);
            if status == "online" {
                online += 1;
                rate_in += n.rate_in;
                rate_out += n.rate_out;
            }
            if status != "offline"
                && let Some(h) = &r.home_relay
            {
                relays.entry(h.clone()).or_default().push(id.clone());
            }
            let fresh = status != "offline";
            nodes.push(json!({
                "id": id,
                "name": r.name,
                "status": status,
                "reporting": true,
                "stopped": r.stopping,
                "last_seen": n.received_ms,
                "version": r.version,
                "commit": r.commit,
                "os": r.os,
                "arch": r.arch,
                "command": r.command,
                "uptime_s": r.uptime_s,
                "interval_s": r.interval_s,
                "transport": r.transport,
                "home_relay": r.home_relay,
                "home_relay_connected": r.home_relay_connected && fresh,
                "nat": r.nat,
                "conns_direct": if fresh { d } else { 0 },
                "conns_relay": if fresh { rl } else { 0 },
                "rate_in": if fresh { n.rate_in } else { 0.0 },
                "rate_out": if fresh { n.rate_out } else { 0.0 },
                "totals": r.totals,
                "last_error": r.last_error.as_ref().map(|e| json!({"kind": e.kind, "at": e.at_ms})),
            }));
        }
        // Peers that do not report, seen in links of nodes that do.
        let mut peers = BTreeSet::new();
        for l in &links {
            for p in [&l.a, &l.b] {
                if !known.contains(p.as_str()) {
                    peers.insert(p.clone());
                }
            }
            if let Some(r) = &l.relay {
                relays.entry(r.clone()).or_default();
            }
        }
        for p in peers {
            nodes.push(json!({
                "id": p, "name": null, "status": "online", "reporting": false, "stopped": false,
                "last_seen": null, "version": null, "commit": null, "os": null, "arch": null,
                "command": null, "uptime_s": null, "interval_s": null, "transport": null,
                "home_relay": null, "home_relay_connected": false, "nat": null,
                "conns_direct": null, "conns_relay": null, "rate_in": null, "rate_out": null,
                "totals": null, "last_error": null,
            }));
        }
        let relays: Vec<Value> = relays
            .into_iter()
            .map(|(url, nodes)| json!({"url": url, "nodes": nodes}))
            .collect();
        let mut hour = self.pending.net;
        for s in &self.fine {
            hour.add(&s.net.counts);
        }
        json!({
            "now": now,
            "network": self.config.policy.network(),
            "nodes": nodes,
            "relays": relays,
            "links": links,
            "summary": {
                "nodes_online": online,
                "nodes_known": self.nodes.len(),
                "links_direct": links.iter().filter(|l| l.path == "direct").count(),
                "links_relay": links.iter().filter(|l| l.path == "relay").count(),
                "rate_in": round(rate_in),
                "rate_out": round(rate_out),
                "conns_failed_1h": hour.conns_failed,
                "requests_1h": hour.requests_ok + hour.requests_failed,
                "requests_failed_1h": hour.requests_failed,
                "path_changes_1h": hour.path_changes,
            },
        })
    }

    /// The history of the last `range` (`GET /v1/history`): the network,
    /// and one node or link if asked for.
    pub fn history(
        &self,
        now: u64,
        range: Duration,
        node: Option<&str>,
        link: Option<(&str, &str)>,
    ) -> Value {
        let range_ms = range.as_millis() as u64;
        let since = now.saturating_sub(range_ms);
        let samples: Vec<Sample> = if range <= FINE_KEEP {
            // 5 s samples; minutes before the first one (after a restart
            // without the fine file).
            let first_fine = self.fine.front().map_or(u64::MAX, |s| s.t);
            self.minutes
                .iter()
                .filter(|s| s.t >= since && s.t + 60_000 <= first_fine)
                .chain(self.fine.iter().filter(|s| s.t >= since))
                .cloned()
                .collect()
        } else {
            let step = (range.as_secs().div_ceil(MAX_POINTS)).div_ceil(60).max(1) * 60;
            let step_ms = step * 1000;
            let mut out: Vec<Sample> = Vec::new();
            let mut group: Vec<&Sample> = Vec::new();
            let mut group_t = None;
            for s in self.minutes.iter().filter(|s| s.t >= since) {
                let t = s.t - s.t % step_ms;
                if group_t.is_some_and(|g| g != t) {
                    out.push(aggregate(&group, group_t.expect("set"), step));
                    group.clear();
                }
                group_t = Some(t);
                group.push(s);
            }
            if let Some(t) = group_t {
                out.push(aggregate(&group, t, step));
            }
            out
        };
        let step_s = samples.last().map_or(TICK.as_secs(), |s| s.s);
        let network: Vec<Value> = samples
            .iter()
            .map(|s| {
                let mut v = serde_json::to_value(&s.net).expect("encode");
                v["t"] = s.t.into();
                v
            })
            .collect();
        let mut out = json!({
            "now": now,
            "range_s": range.as_secs(),
            "step_s": step_s,
            "network": network,
        });
        if let Some(id) = node {
            let id = id.to_lowercase();
            let series: Vec<Value> = samples
                .iter()
                .map(|s| {
                    let n = s.nodes.get(&id).cloned().unwrap_or_default();
                    let mut v = serde_json::to_value(&n).expect("encode");
                    v["t"] = s.t.into();
                    v
                })
                .collect();
            out["node"] = series.into();
        }
        if let Some((a, b)) = link {
            let (a, b) = (a.to_lowercase(), b.to_lowercase());
            let key = if a < b {
                format!("{a},{b}")
            } else {
                format!("{b},{a}")
            };
            let series: Vec<Value> = samples
                .iter()
                .map(|s| match s.links.get(&key) {
                    Some(l) => json!({"t": s.t, "path": l.path, "direct_frac": l.direct_frac,
                        "rtt_ms": l.rtt_ms, "rate": l.rate}),
                    None => json!({"t": s.t, "path": null, "direct_frac": null,
                        "rtt_ms": null, "rate": 0.0}),
                })
                .collect();
            out["link"] = series.into();
        }
        out
    }

    /// Prometheus text format (`GET /metrics`).
    pub fn metrics(&self, now: u64) -> String {
        let mut m = String::new();
        let links = self.merged_links(now);
        let mut gauge = |name: &str, help: &str, kind: &str, rows: Vec<(String, f64)>| {
            let _ = writeln!(m, "# HELP {name} {help}\n# TYPE {name} {kind}");
            for (labels, v) in rows {
                let _ = writeln!(m, "{name}{labels} {v}");
            }
        };
        let online = self
            .nodes
            .values()
            .filter(|n| Self::status(n, now) == "online")
            .count();
        gauge(
            "golem_p2p_nodes_online",
            "Nodes that reported recently.",
            "gauge",
            vec![(String::new(), online as f64)],
        );
        gauge(
            "golem_p2p_nodes_known",
            "Nodes that reported within the retention time.",
            "gauge",
            vec![(String::new(), self.nodes.len() as f64)],
        );
        gauge(
            "golem_p2p_links",
            "Connections between nodes, both ends merged, by path.",
            "gauge",
            ["direct", "relay", "unknown"]
                .iter()
                .map(|p| {
                    (
                        format!("{{path=\"{p}\"}}"),
                        links.iter().filter(|l| l.path == *p).count() as f64,
                    )
                })
                .collect(),
        );
        let label = |id: &str, n: &NodeState| {
            format!(
                "node_id=\"{}\",name=\"{}\"",
                prom_escape(id),
                prom_escape(n.report.name.as_deref().unwrap_or(""))
            )
        };
        let nodes: Vec<(&String, &NodeState)> = self.nodes.iter().collect();
        let per_node = |f: &dyn Fn(&NodeState) -> Vec<(String, f64)>| -> Vec<(String, f64)> {
            nodes
                .iter()
                .flat_map(|(id, n)| {
                    f(n).into_iter().map(move |(extra, v)| {
                        let sep = if extra.is_empty() { "" } else { "," };
                        (format!("{{{}{sep}{extra}}}", label(id, n)), v)
                    })
                })
                .collect()
        };
        gauge(
            "golem_p2p_node_up",
            "1 if the node reported recently, else 0.",
            "gauge",
            per_node(&|n| vec![(String::new(), f64::from(Self::status(n, now) == "online"))]),
        );
        gauge(
            "golem_p2p_node_info",
            "Version, commit, platform and command of the node.",
            "gauge",
            per_node(&|n| {
                let r = &n.report;
                vec![(
                    format!(
                        "version=\"{}\",commit=\"{}\",os=\"{}\",arch=\"{}\",command=\"{}\"",
                        prom_escape(&r.version),
                        prom_escape(r.commit.as_deref().unwrap_or("")),
                        prom_escape(&r.os),
                        prom_escape(&r.arch),
                        prom_escape(&r.command)
                    ),
                    1.0,
                )]
            }),
        );
        gauge(
            "golem_p2p_node_last_report_age_seconds",
            "Seconds since the node's last report.",
            "gauge",
            per_node(&|n| {
                vec![(
                    String::new(),
                    now.saturating_sub(n.received_ms) as f64 / 1000.0,
                )]
            }),
        );
        gauge(
            "golem_p2p_node_uptime_seconds",
            "Uptime the node reported last.",
            "gauge",
            per_node(&|n| vec![(String::new(), n.report.uptime_s as f64)]),
        );
        gauge(
            "golem_p2p_node_connections",
            "Open connections of the node by path (last report).",
            "gauge",
            per_node(&|n| {
                let (d, r) = conn_paths(&n.report);
                vec![
                    ("path=\"direct\"".into(), d as f64),
                    ("path=\"relay\"".into(), r as f64),
                ]
            }),
        );
        let t = |f: fn(&super::Counters) -> u64| {
            per_node(&|n| vec![(String::new(), f(&n.report.totals) as f64)])
        };
        gauge(
            "golem_p2p_node_connections_new_total",
            "Connections that completed the handshake since the node started.",
            "counter",
            t(|c| c.conns_new),
        );
        gauge(
            "golem_p2p_node_connections_failed_total",
            "Failed connects and handshakes since the node started.",
            "counter",
            t(|c| c.conns_failed),
        );
        gauge(
            "golem_p2p_node_path_changes_total",
            "Changes between direct and relayed paths since the node started.",
            "counter",
            t(|c| c.path_changes),
        );
        gauge(
            "golem_p2p_node_requests_total",
            "Requests served (listen) or answered (soak) since the node started.",
            "counter",
            per_node(&|n| {
                vec![
                    ("result=\"ok\"".into(), n.report.totals.requests_ok as f64),
                    (
                        "result=\"failed\"".into(),
                        n.report.totals.requests_failed as f64,
                    ),
                ]
            }),
        );
        gauge(
            "golem_p2p_node_bytes_total",
            "UDP bytes since the node started, by direction and path.",
            "counter",
            per_node(&|n| {
                let c = &n.report.totals;
                vec![
                    (
                        "direction=\"in\",path=\"direct\"".into(),
                        c.bytes_in.saturating_sub(c.bytes_in_relay) as f64,
                    ),
                    (
                        "direction=\"in\",path=\"relay\"".into(),
                        c.bytes_in_relay as f64,
                    ),
                    (
                        "direction=\"out\",path=\"direct\"".into(),
                        c.bytes_out.saturating_sub(c.bytes_out_relay) as f64,
                    ),
                    (
                        "direction=\"out\",path=\"relay\"".into(),
                        c.bytes_out_relay as f64,
                    ),
                ]
            }),
        );
        gauge(
            "golem_p2p_link_rtt_seconds",
            "Round-trip time of each link (smaller of the two ends' estimates).",
            "gauge",
            links
                .iter()
                .filter_map(|l| {
                    Some((
                        format!(
                            "{{a=\"{}\",b=\"{}\",path=\"{}\"}}",
                            l.a,
                            l.b,
                            prom_escape(&l.path)
                        ),
                        l.rtt_ms? / 1000.0,
                    ))
                })
                .collect(),
        );
        gauge(
            "golem_p2p_telemetry_reports_total",
            "Reports the collector received, by result.",
            "counter",
            self.stats
                .by_result
                .iter()
                .map(|(r, v)| (format!("{{result=\"{r}\"}}"), *v as f64))
                .collect(),
        );
        m
    }
}

/// Bytes per second of a connection, both directions.
fn rate(c: &super::Conn) -> f64 {
    c.rate_in + c.rate_out
}

/// Open connections of a report by path: (direct, relay).
fn conn_paths(r: &Report) -> (usize, usize) {
    let d = r.connections.iter().filter(|c| c.path == "direct").count();
    let rl = r.connections.iter().filter(|c| c.path == "relay").count();
    (d, rl)
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Aggregates samples into one of `step` seconds starting at `t`.
fn aggregate(samples: &[&Sample], t: u64, step: u64) -> Sample {
    let n = samples.len().max(1) as f64;
    let mut out = Sample {
        t,
        s: step,
        ..Default::default()
    };
    let mut links: BTreeMap<String, (LinkSample, f64, u64, u64)> = BTreeMap::new();
    for s in samples {
        let a = &mut out.net;
        a.nodes += s.net.nodes / n;
        a.direct += s.net.direct / n;
        a.relay += s.net.relay / n;
        a.rate_in += s.net.rate_in / n;
        a.rate_out += s.net.rate_out / n;
        a.rate_relay += s.net.rate_relay / n;
        a.counts.add(&s.net.counts);
        for (id, ns) in &s.nodes {
            let o = out.nodes.entry(id.clone()).or_default();
            o.online += ns.online / n;
            o.direct += ns.direct / n;
            o.relay += ns.relay / n;
            o.rate_in += ns.rate_in / n;
            o.rate_out += ns.rate_out / n;
            o.counts.add(&ns.counts);
        }
        for (k, l) in &s.links {
            // (last sample, direct time, rtt sum, rtt count) over the
            // samples the link existed in.
            let e = links.entry(k.clone()).or_insert_with(|| {
                (
                    LinkSample {
                        n: 0,
                        ..Default::default()
                    },
                    0.0,
                    0,
                    0,
                )
            });
            e.0.path = l.path.clone().or(e.0.path.take());
            e.0.n += l.n;
            e.1 += l.direct_frac * l.n as f64;
            if let Some(r) = l.rtt_ms {
                e.2 += (r * 1000.0) as u64 * l.n;
                e.3 += l.n;
            }
            e.0.rate += l.rate / n;
        }
    }
    for (k, (mut l, direct, rtt_sum, rtt_n)) in links {
        l.direct_frac = round(direct / l.n.max(1) as f64);
        l.rtt_ms = (rtt_n > 0).then(|| round(rtt_sum as f64 / rtt_n as f64 / 1000.0));
        l.rate = round(l.rate);
        out.links.insert(k, l);
    }
    let r = |v: &mut f64| *v = round(*v);
    r(&mut out.net.nodes);
    r(&mut out.net.direct);
    r(&mut out.net.relay);
    r(&mut out.net.rate_in);
    r(&mut out.net.rate_out);
    r(&mut out.net.rate_relay);
    for o in out.nodes.values_mut() {
        r(&mut o.online);
        r(&mut o.direct);
        r(&mut o.relay);
        r(&mut o.rate_in);
        r(&mut o.rate_out);
    }
    out
}

/// A label value in Prometheus' text format.
fn prom_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// `YYYY-MM-DD` (UTC) of a Unix ms time.
fn day_of(ms: u64) -> String {
    let days = (ms / 86_400_000) as i64;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The day of a `minutes-YYYY-MM-DD.jsonl` file.
fn minute_file_day(p: &Path) -> Option<String> {
    let name = p.file_name()?.to_str()?;
    let day = name.strip_prefix("minutes-")?.strip_suffix(".jsonl")?;
    (day.len() == 10).then(|| day.to_string())
}

fn read_samples(p: &Path) -> Vec<Sample> {
    let Ok(text) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

async fn tick_loop(shared: Shared) {
    // Samples at multiples of 5 s of the wall clock.
    loop {
        let now = now_ms();
        let tick = TICK.as_millis() as u64;
        let wait = tick - now % tick;
        tokio::time::sleep(Duration::from_millis(wait)).await;
        shared.lock().expect("poisoned").tick(now_ms());
    }
}

/// The collector's HTTP routes.
pub fn router(shared: Shared) -> Router {
    Router::new()
        .route(
            "/v1/report",
            post(report).layer(DefaultBodyLimit::max(MAX_REPORT_SIZE)),
        )
        .route("/", get(dashboard))
        .route("/v1/state", get(state))
        .route("/v1/nodes", get(nodes))
        .route("/v1/history", get(history))
        .route("/metrics", get(metrics))
        .route("/health", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(MAX_REPORT_SIZE))
        .with_state(shared)
}

fn text(status: StatusCode, body: impl Into<String>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body.into(),
    )
        .into_response()
}

fn json_response(v: &Value) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        v.to_string(),
    )
        .into_response()
}

async fn report(State(s): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    let r = s
        .lock()
        .expect("poisoned")
        .accept(&body, &headers, now_ms());
    match r {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => text(e.status, format!("{}\n", e.reason)),
    }
}

async fn dashboard() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
                 connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        DASHBOARD,
    )
        .into_response()
}

async fn state(State(s): State<Shared>) -> Response {
    let v = s.lock().expect("poisoned").state(now_ms());
    json_response(&v)
}

async fn nodes(State(s): State<Shared>) -> Response {
    let v = s.lock().expect("poisoned").state(now_ms());
    json_response(&v["nodes"])
}

/// `key=value` pairs of a query string (no percent-decoding needed: the
/// values are numbers and hex node IDs; `%2C` is accepted for the comma).
fn query(q: Option<&str>) -> BTreeMap<String, String> {
    q.unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.replace("%2C", ",").replace("%2c", ",")))
        .collect()
}

async fn history(State(s): State<Shared>, RawQuery(q): RawQuery) -> Response {
    let q = query(q.as_deref());
    let range = q
        .get("range")
        .and_then(|r| r.parse::<u64>().ok())
        .unwrap_or(3600);
    let c = s.lock().expect("poisoned");
    let max = c.config.retention.as_secs();
    let range = Duration::from_secs(range.clamp(60, max));
    let node = q.get("node").map(String::as_str);
    let link = q.get("link").and_then(|l| l.split_once(','));
    if node.is_some_and(|n| n.parse::<NodeId>().is_err())
        || link.is_some_and(|(a, b)| a.parse::<NodeId>().is_err() || b.parse::<NodeId>().is_err())
    {
        return text(StatusCode::BAD_REQUEST, "invalid node id\n");
    }
    let v = c.history(now_ms(), range, node, link);
    json_response(&v)
}

async fn metrics(State(s): State<Shared>) -> Response {
    let m = s.lock().expect("poisoned").metrics(now_ms());
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        m,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use golem_p2p::{
        InMemorySigner,
        iroh::SecretKey,
        membership::{CERT_VERSION, CertPayload, MembershipCert},
    };

    use super::*;
    use crate::telemetry::{Conn, Counters};

    const T0: u64 = 1_800_000_000_000;

    struct Net {
        registry: SecretKey,
        signer: InMemorySigner,
    }

    impl Net {
        fn new() -> Self {
            Self {
                registry: SecretKey::generate(),
                signer: InMemorySigner::new(),
            }
        }

        fn collector(&self) -> Collector {
            Collector::new(Config {
                policy: MembershipPolicy::new("net", self.registry.public()),
                store: None,
                retention: Duration::from_secs(7 * 86400),
                min_interval: Duration::from_secs(4),
            })
        }

        fn cert(&self, network: &str, id: NodeId, issued_s: u64, lifetime: u64) -> Vec<u8> {
            MembershipCert::issue(
                CertPayload {
                    version: CERT_VERSION,
                    network: network.into(),
                    endpoint_id: *SecretKey::generate().public().as_bytes(),
                    identities: vec![id],
                    issued_at: issued_s,
                    expires_at: issued_s + lifetime,
                },
                &self.registry,
            )
            .to_bytes()
            .to_vec()
        }

        /// A signed request: body and headers.
        fn signed(&self, id: NodeId, body: &[u8], cert: &[u8]) -> HeaderMap {
            let d = golem_p2p::telemetry::digest(body);
            let sig = self.signer.sign_blocking(id, &d).unwrap();
            let mut h = HeaderMap::new();
            h.insert(
                SIGNATURE_HEADER,
                hex::encode(sig.as_bytes()).parse().unwrap(),
            );
            h.insert(CERT_HEADER, hex::encode(cert).parse().unwrap());
            h
        }
    }

    fn report(id: NodeId, ts_ms: u64, conns: Vec<Conn>) -> Report {
        Report {
            v: REPORT_VERSION,
            node_id: id.to_string(),
            ts_ms,
            seq: 1,
            instance: "00".repeat(16),
            name: Some("node <b>\"x\"</b>".into()),
            version: "0.1.0".into(),
            commit: Some("abc".into()),
            os: "linux".into(),
            arch: "x86_64".into(),
            command: "listen".into(),
            uptime_s: 10,
            interval_s: 5.0,
            transport: "direct_and_relay".into(),
            home_relay: Some("https://relay.example/".into()),
            home_relay_connected: true,
            connections_open: conns.len(),
            connections: conns,
            totals: Counters {
                requests_ok: 7,
                bytes_in: 5000,
                ..Default::default()
            },
            delta: Counters {
                requests_ok: 2,
                conns_failed: 1,
                bytes_in: 5000,
                bytes_out: 2500,
                ..Default::default()
            },
            delta_s: 5.0,
            ..Default::default()
        }
    }

    fn conn(peer: NodeId, path: &str) -> Conn {
        Conn {
            peer: peer.to_string(),
            direction: "out".into(),
            path: path.into(),
            relay: (path == "relay").then(|| "https://relay.example/".into()),
            rtt_ms: Some(if path == "relay" { 80.0 } else { 30.0 }),
            path_s: Some(20),
            rate_in: 100.0,
            rate_out: 50.0,
        }
    }

    /// Sends `r` signed by its node with a valid certificate.
    fn send(net: &Net, c: &mut Collector, r: &Report, now: u64) -> Result<String, Refusal> {
        let id: NodeId = r.node_id.parse().unwrap();
        let cert = net.cert("net", id, now / 1000 - 60, 3600);
        let body = serde_json::to_vec(r).unwrap();
        c.accept(&body, &net.signed(id, &body, &cert), now)
    }

    #[test]
    fn only_fresh_reports_of_members() {
        let net = Net::new();
        let mut c = net.collector();
        let a = net.signer.generate();
        let b = net.signer.generate();
        let now = T0;
        let body = serde_json::to_vec(&report(a, now, vec![])).unwrap();
        let good = net.cert("net", a, now / 1000 - 60, 3600);
        let reason = |r: Result<String, Refusal>| r.map_err(|e| e.reason);

        // Wrong network, expired, someone else's certificate.
        let h = net.signed(a, &body, &net.cert("other", a, now / 1000 - 60, 3600));
        assert_eq!(reason(c.accept(&body, &h, now)), Err("not_member"));
        let h = net.signed(a, &body, &net.cert("net", a, now / 1000 - 7200, 600));
        assert_eq!(reason(c.accept(&body, &h, now)), Err("not_member"));
        let h = net.signed(a, &body, &net.cert("net", b, now / 1000 - 60, 3600));
        assert_eq!(reason(c.accept(&body, &h, now)), Err("not_member"));
        // A tampered body: the signature is of another identity.
        let h = net.signed(a, &body, &good);
        let mut tampered = body.clone();
        tampered[10] ^= 1;
        assert_eq!(reason(c.accept(&tampered, &h, now)), Err("not_member"));
        // No or garbage signature.
        assert_eq!(
            reason(c.accept(&body, &HeaderMap::new(), now)),
            Err("unsigned")
        );
        let mut h = net.signed(a, &body, &good);
        h.insert(SIGNATURE_HEADER, "00".repeat(65).parse().unwrap());
        assert_eq!(reason(c.accept(&body, &h, now)), Err("bad_signature"));
        // Oversized.
        let big = vec![b' '; MAX_REPORT_SIZE + 1];
        let h = net.signed(a, &big, &good);
        let e = c.accept(&big, &h, now).unwrap_err();
        assert_eq!(e.status, StatusCode::PAYLOAD_TOO_LARGE);
        // Signed by a, claiming b.
        let other = serde_json::to_vec(&report(b, now, vec![])).unwrap();
        let h = net.signed(a, &other, &good);
        assert_eq!(reason(c.accept(&other, &h, now)), Err("node_mismatch"));
        // A timestamp far from the collector's clock.
        let old = serde_json::to_vec(&report(a, now - 10 * 60_000, vec![])).unwrap();
        let h = net.signed(a, &old, &good);
        assert_eq!(reason(c.accept(&old, &h, now)), Err("stale"));
        // Garbage JSON, a report with an invalid connection.
        let junk = b"{\"v\":1}".to_vec();
        let h = net.signed(a, &junk, &good);
        assert_eq!(reason(c.accept(&junk, &h, now)), Err("malformed"));
        let mut bad = report(a, now, vec![conn(b, "direct")]);
        bad.connections[0].path = "teleport".into();
        assert_eq!(reason(send(&net, &mut c, &bad, now)), Err("malformed"));
        assert!(c.nodes.is_empty());

        // Valid, then the same report again (replay), then an older one.
        let h = net.signed(a, &body, &good);
        assert_eq!(reason(c.accept(&body, &h, now)), Ok(a.to_string()));
        assert_eq!(reason(c.accept(&body, &h, now + 5000)), Err("replay"));
        let older = report(a, now - 1000, vec![]);
        assert_eq!(
            reason(send(&net, &mut c, &older, now + 5000)),
            Err("replay")
        );
        let m = c.metrics(now);
        assert!(
            m.contains("golem_p2p_telemetry_reports_total{result=\"replay\"} 2"),
            "{m}"
        );
        assert!(
            m.contains("golem_p2p_telemetry_reports_total{result=\"accepted\"} 1"),
            "{m}"
        );
    }

    #[test]
    fn rate_limit_per_node() {
        let net = Net::new();
        let mut c = net.collector();
        let a = net.signer.generate();
        let b = net.signer.generate();
        // A burst of 3 passes, the 4th does not; another node is not
        // affected.
        for i in 0..3 {
            send(&net, &mut c, &report(a, T0 + i, vec![]), T0 + i).unwrap();
        }
        let e = send(&net, &mut c, &report(a, T0 + 3, vec![]), T0 + 3).unwrap_err();
        assert_eq!(e.status, StatusCode::TOO_MANY_REQUESTS);
        send(&net, &mut c, &report(b, T0 + 3, vec![]), T0 + 3).unwrap();
        // One more every 4 s; a report every 5 s always passes.
        send(&net, &mut c, &report(a, T0 + 4100, vec![]), T0 + 4100).unwrap();
        for i in 1..50 {
            let t = T0 + 4100 + i * 5000;
            send(&net, &mut c, &report(a, t, vec![]), t).unwrap();
        }
    }

    #[test]
    fn links_status_and_history() {
        let net = Net::new();
        let mut c = net.collector();
        let a = net.signer.generate();
        let b = net.signer.generate();
        let peer = net.signer.generate(); // never reports
        let mut now = T0 - T0 % 60_000;
        // a sees b direct and the silent peer over the relay; b sees a over
        // the relay, in an older report: a's newer view wins.
        send(&net, &mut c, &report(b, now, vec![conn(a, "relay")]), now).unwrap();
        now += 1000;
        let ra = report(a, now, vec![conn(b, "direct"), conn(peer, "relay")]);
        send(&net, &mut c, &ra, now).unwrap();
        c.tick(now);

        let st = c.state(now);
        assert_eq!(st["summary"]["nodes_online"], 2);
        assert_eq!(st["summary"]["links_direct"], 1);
        assert_eq!(st["summary"]["links_relay"], 1);
        let links = st["links"].as_array().unwrap();
        let ab = links
            .iter()
            .find(|l| l["reported_by"].as_array().unwrap().len() == 2)
            .unwrap();
        assert_eq!(ab["path"], "direct");
        assert_eq!(ab["rtt_ms"], 30.0, "the smaller estimate");
        assert_eq!(ab["path_since"], now - 20_000);
        let silent = links.iter().find(|l| l["path"] == "relay").unwrap();
        assert_eq!(silent["relay"], "https://relay.example/");
        let peer_node = st["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == peer.to_string())
            .unwrap();
        assert_eq!(peer_node["reporting"], false);
        assert_eq!(st["relays"][0]["url"], "https://relay.example/");
        assert_eq!(st["relays"][0]["nodes"].as_array().unwrap().len(), 2);

        // Path change relay -> direct on the silent peer's link.
        let mut moved = ra.clone();
        moved.ts_ms = now + 5000;
        moved.connections[1] = conn(peer, "direct");
        now += 5000;
        send(&net, &mut c, &moved, now).unwrap();
        c.tick(now);
        let st = c.state(now);
        assert_eq!(st["summary"]["links_direct"], 2);
        let l = st["links"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["reported_by"].as_array().unwrap().len() == 1)
            .unwrap();
        assert_eq!(l["path_changes"], 1);

        // b stops cleanly: offline at once. a goes silent: stale, then
        // offline; its links disappear with it.
        let mut stop = report(b, now + 1, vec![]);
        stop.stopping = true;
        send(&net, &mut c, &stop, now + 1).unwrap();
        let status = |c: &Collector, id: NodeId, now: u64| {
            c.state(now)["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == id.to_string())
                .map(|n| n["status"].as_str().unwrap().to_string())
        };
        assert_eq!(status(&c, b, now + 2).as_deref(), Some("offline"));
        assert_eq!(status(&c, a, now + 17_000).as_deref(), Some("online"));
        assert_eq!(status(&c, a, now + 30_000).as_deref(), Some("stale"));
        assert_eq!(status(&c, a, now + 61_000).as_deref(), Some("offline"));
        assert!(
            c.state(now + 61_000)["links"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(status(&c, peer, now + 61_000), None);

        // Fine history, then minute aggregates after the minute ends.
        for i in 1..=24 {
            c.tick(now + i * 5000);
        }
        let h = c.history(
            now + 24 * 5000,
            Duration::from_secs(900),
            Some(&a.to_string()),
            None,
        );
        assert_eq!(h["step_s"], 5);
        let net_series = h["network"].as_array().unwrap();
        assert_eq!(net_series.len(), 26);
        assert_eq!(net_series[0]["nodes"], 2.0);
        assert_eq!(net_series[0]["requests_ok"], 4);
        assert_eq!(net_series[0]["conns_failed"], 2);
        assert_eq!(net_series.last().unwrap()["nodes"], 0.0);
        assert_eq!(h["node"].as_array().unwrap().len(), 26);
        assert_eq!(h["node"][0]["online"], 1.0);
        assert!(!c.minutes.is_empty());
        let h = c.history(
            now + 24 * 5000,
            Duration::from_secs(6 * 3600),
            None,
            Some((&peer.to_string(), &a.to_string())),
        );
        assert_eq!(h["step_s"], 60);
        let link = h["link"].as_array().unwrap();
        // 1 of 12 samples of the first minute over the relay.
        assert_eq!(link[0]["direct_frac"], 0.917, "{link:?}");
        assert_eq!(link[0]["path"], "direct");
        assert_eq!(link[0]["rtt_ms"], 34.167);
    }

    #[test]
    fn metrics_text() {
        let net = Net::new();
        let mut c = net.collector();
        let a = net.signer.generate();
        let b = net.signer.generate();
        send(&net, &mut c, &report(a, T0, vec![conn(b, "direct")]), T0).unwrap();
        let m = c.metrics(T0 + 1000);
        let id = a.to_string();
        for line in [
            "golem_p2p_nodes_online 1".to_string(),
            "golem_p2p_links{path=\"direct\"} 1".into(),
            format!("golem_p2p_node_up{{node_id=\"{id}\",name=\"node <b>\\\"x\\\"</b>\"}} 1"),
            format!(
                "golem_p2p_node_requests_total{{node_id=\"{id}\",name=\"node <b>\\\"x\\\"</b>\",result=\"ok\"}} 7"
            ),
            format!(
                "golem_p2p_node_bytes_total{{node_id=\"{id}\",name=\"node <b>\\\"x\\\"</b>\",direction=\"in\",path=\"direct\"}} 5000"
            ),
            "# TYPE golem_p2p_node_bytes_total counter".into(),
        ] {
            assert!(m.lines().any(|l| l == line), "missing {line}\n{m}");
        }
        // Every sample line is `name{labels} value` or `name value`.
        for l in m.lines().filter(|l| !l.starts_with('#')) {
            let v = l.rsplit(' ').next().unwrap();
            assert!(v.parse::<f64>().is_ok(), "{l}");
        }
    }

    #[test]
    fn store_survives_restarts() {
        let net = Net::new();
        let dir = std::env::temp_dir().join(format!(
            "golem-p2p-telemetry-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let config = Config {
            policy: MembershipPolicy::new("net", net.registry.public()),
            store: Some(dir.clone()),
            retention: Duration::from_secs(7 * 86400),
            min_interval: Duration::from_secs(4),
        };
        let a = net.signer.generate();
        let now = now_ms();
        let start = now - now % 60_000 - 3 * 60_000;
        {
            let shared = Collector::open(config.clone()).unwrap();
            let mut c = shared.lock().unwrap();
            for i in 0..(3 * 12) {
                c.tick(start + i * 5000);
            }
            send(&net, &mut c, &report(a, now, vec![]), now).unwrap();
            c.save_nodes();
        }
        let shared = Collector::open(config).unwrap();
        let mut c = shared.lock().unwrap();
        assert_eq!(c.minutes.len(), 2);
        assert_eq!(c.fine.len(), 36);
        assert!(c.nodes.contains_key(&a.to_string()));
        // Replays are refused after a restart too.
        let e = send(&net, &mut c, &report(a, now, vec![]), now).unwrap_err();
        assert_eq!(e.reason, "replay");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn days() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_763_164_800_000), "2025-11-15");
        assert_eq!(day_of(951_782_400_000), "2000-02-29");
        assert_eq!(
            minute_file_day(Path::new("/x/minutes-2026-11-15.jsonl")).as_deref(),
            Some("2026-11-15")
        );
        assert_eq!(minute_file_day(Path::new("/x/fine.jsonl")), None);
    }

    #[tokio::test]
    async fn http_api() {
        let net = Net::new();
        let shared = Arc::new(Mutex::new(net.collector()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(axum::serve(listener, router(shared.clone())).into_future());
        let tls = golem_p2p::iroh::tls::CaTlsConfig::default()
            .client_config(golem_p2p::iroh::tls::default_provider())
            .unwrap();
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .build()
            .unwrap();

        let a = net.signer.generate();
        let now = now_ms();
        let body = serde_json::to_vec(&report(a, now, vec![])).unwrap();
        let cert = net.cert("net", a, now / 1000 - 60, 3600);
        let h = net.signed(a, &body, &cert);
        let post = |body: Vec<u8>| {
            let mut req = http
                .post(format!("{base}v1/report"))
                .header("content-type", "application/json");
            for (k, v) in &h {
                req = req.header(k, v);
            }
            req.body(body).send()
        };
        assert_eq!(post(body.clone()).await.unwrap().status(), 204);
        let r = post(body.clone()).await.unwrap();
        assert_eq!(r.status(), 409);
        assert_eq!(r.text().await.unwrap(), "replay\n");
        let r = post(vec![b' '; MAX_REPORT_SIZE + 10]).await.unwrap();
        assert_eq!(r.status(), 413);

        shared.lock().unwrap().tick(now_ms());
        let get = |path: &str| http.get(format!("{base}{path}")).send();
        let st: Value = get("v1/state").await.unwrap().json().await.unwrap();
        assert_eq!(st["network"], "net");
        assert_eq!(st["nodes"][0]["id"], a.to_string());
        assert_eq!(st["nodes"][0]["status"], "online");
        let nodes: Value = get("v1/nodes").await.unwrap().json().await.unwrap();
        assert_eq!(nodes, st["nodes"]);
        let h: Value = get(&format!("v1/history?range=600&node={a}&link={a}%2C{a}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(h["network"].as_array().unwrap().len(), 1);
        assert_eq!(h["node"][0]["online"], 1.0);
        assert_eq!(get("v1/history?node=0xnope").await.unwrap().status(), 400);
        let m = get("metrics").await.unwrap();
        assert!(
            m.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/plain; version=0.0.4")
        );
        assert!(m.text().await.unwrap().contains("golem_p2p_nodes_online 1"));
        let page = get("").await.unwrap();
        assert_eq!(page.headers()["content-type"], "text/html; charset=utf-8");
        assert!(page.headers().contains_key("content-security-policy"));
        let html = page.text().await.unwrap();
        assert!(html.contains("<title>"), "dashboard served");
        // The page works under a path prefix: no absolute API URLs.
        assert!(!html.contains("\"/v1/") && !html.contains("'/v1/"));
        server.abort();
    }
}
