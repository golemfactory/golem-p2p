//! Telemetry of closed networks: the reports `listen` and `soak` send to the
//! network's collector, and the collector itself ([`collector`]).
//!
//! On a closed network (`--registry`), a long-running node sends a small JSON
//! [`Report`] every `--telemetry-interval` (default 5 s) to the collector the
//! registry advertises (`telemetry=<url>` in `GET /v1/info`), or to
//! `--telemetry <URL>`. `--no-telemetry` (or `GOLEM_P2P_NO_TELEMETRY=1`)
//! turns it off. Reports are signed by the node's default identity and carry
//! its membership certificate ([`golem_p2p::telemetry`]). They contain no IP
//! addresses: see `docs/telemetry.md` for every field.
//!
//! Telemetry never affects the node: one report at a time, each bounded by a
//! timeout shorter than the interval, no retries (the next report replaces a
//! failed one), and failures are logged once at `warn`, then at `debug`.

pub mod collector;

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clap::Args;
use golem_p2p::{
    Direction, Endpoint, InMemorySigner, PathKind, RegistryClient, TransportMode,
    iroh::Watcher,
    telemetry::{CERT_HEADER, SIGNATURE_HEADER},
};
use serde::{Deserialize, Serialize};
use tokio::{sync::oneshot, task::JoinHandle, time::MissedTickBehavior};
use tracing::{debug, info, warn};

use crate::{
    NodeArgs,
    observe::SelectedPath,
    out::{Out, path_name, round3},
    parse,
};

/// Format version of [`Report`].
pub const REPORT_VERSION: u32 = 1;

/// Most connections listed in one report (the count covers all).
pub const MAX_REPORT_CONNECTIONS: usize = 256;

/// Longest node name sent (characters).
pub const MAX_NAME_LEN: usize = 64;

/// How often a node asks the registry for the collector URL while it knows
/// none.
const INFO_RETRY: Duration = Duration::from_secs(600);

/// The git commit the binary was built from, if known at build time.
pub const COMMIT: Option<&str> = option_env!("GOLEM_P2P_COMMIT");

/// Telemetry options of `listen` and `soak`.
#[derive(Args, Clone, Debug)]
pub struct TelemetryArgs {
    /// Do not send telemetry reports. On a closed network (`--registry`) a
    /// node reports its version, uptime, connections and counters (no IP
    /// addresses, see docs/telemetry.md) to the network's collector every
    /// `--telemetry-interval`.
    #[arg(
        long,
        env = "GOLEM_P2P_NO_TELEMETRY",
        value_parser = clap::builder::FalseyValueParser::new()
    )]
    pub no_telemetry: bool,
    /// Telemetry collector URL. Default: the URL the registry advertises.
    #[arg(long, value_name = "URL", env = "GOLEM_P2P_TELEMETRY")]
    pub telemetry: Option<url::Url>,
    /// Time between two telemetry reports (at least 1 s).
    #[arg(
        long,
        default_value = "5s",
        value_parser = parse::duration,
        env = "GOLEM_P2P_TELEMETRY_INTERVAL"
    )]
    pub telemetry_interval: Duration,
}

/// Counters of this process that the endpoint does not keep: requests,
/// path changes, the last error. Updated by `listen`, `soak` and the
/// connection observer.
pub struct Activity {
    requests_ok: AtomicU64,
    requests_failed: AtomicU64,
    path_changes: AtomicU64,
    last_error: Mutex<Option<LastError>>,
}

/// The process' [`Activity`].
pub static ACTIVITY: Activity = Activity {
    requests_ok: AtomicU64::new(0),
    requests_failed: AtomicU64::new(0),
    path_changes: AtomicU64::new(0),
    last_error: Mutex::new(None),
};

impl Activity {
    /// A request served (`listen`) or answered (`soak`).
    pub fn request_ok(&self) {
        self.requests_ok.fetch_add(1, Ordering::Relaxed);
    }

    /// A failed request, with its error kind (`timeout`, `refused`, ...).
    pub fn request_failed(&self, kind: &str) {
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
        self.error(kind);
    }

    /// Remembers an error kind as the last error (never a message: messages
    /// may contain addresses).
    pub fn error(&self, kind: &str) {
        *self.last_error.lock().expect("poisoned") = Some(LastError {
            kind: kind.chars().take(32).collect(),
            at_ms: now_ms(),
        });
    }
}

/// The paths of open connections: since when each one is on its current kind
/// of path (direct or relay). Kept by the connection observer.
static PATHS: Mutex<BTreeMap<usize, PathSince>> = Mutex::new(BTreeMap::new());

#[derive(Debug, Clone, Copy)]
struct PathSince {
    kind: PathKind,
    since_ms: u64,
}

/// The observer saw the selected path of connection `id` (its stable ID):
/// when it opened and on every change. Counts a change between direct and
/// relay as a path change.
pub fn path_seen(id: usize, kind: PathKind) {
    if kind == PathKind::Unknown {
        return;
    }
    let mut paths = PATHS.lock().expect("poisoned");
    match paths.get_mut(&id) {
        Some(p) if p.kind == kind => {}
        Some(p) => {
            ACTIVITY.path_changes.fetch_add(1, Ordering::Relaxed);
            *p = PathSince {
                kind,
                since_ms: now_ms(),
            };
        }
        None => {
            paths.insert(
                id,
                PathSince {
                    kind,
                    since_ms: now_ms(),
                },
            );
        }
    }
}

/// Connection `id` closed.
pub fn path_closed(id: usize) {
    PATHS.lock().expect("poisoned").remove(&id);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// A telemetry report: what a node says about itself. Field by field in
/// `docs/telemetry.md`. No IP addresses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    /// [`REPORT_VERSION`].
    pub v: u32,
    /// The node's default identity; it signs the report.
    pub node_id: String,
    /// When the report was made (sender's clock), Unix ms.
    pub ts_ms: u64,
    /// Report number of this process, from 1.
    pub seq: u64,
    /// Random ID of this process (new on every start), hex.
    pub instance: String,
    /// `--node-name`, if given.
    #[serde(default)]
    pub name: Option<String>,
    /// golem-p2p version.
    pub version: String,
    /// Git commit of the build, if known.
    #[serde(default)]
    pub commit: Option<String>,
    /// Operating system and CPU architecture (`linux`, `x86_64`).
    pub os: String,
    pub arch: String,
    /// The command that runs: `listen` or `soak`.
    pub command: String,
    /// Seconds since the node started.
    pub uptime_s: u64,
    /// The report interval, seconds.
    pub interval_s: f64,
    /// `direct_and_relay`, `relay_only` or `direct_only`.
    pub transport: String,
    /// The home relay's URL (a public server) and whether it is connected.
    #[serde(default)]
    pub home_relay: Option<String>,
    #[serde(default)]
    pub home_relay_connected: bool,
    /// What iroh's net report found, without addresses.
    #[serde(default)]
    pub nat: Option<Nat>,
    /// Open connections (at most [`MAX_REPORT_CONNECTIONS`]).
    #[serde(default)]
    pub connections: Vec<Conn>,
    /// Number of open connections, also beyond the list.
    #[serde(default)]
    pub connections_open: usize,
    /// Counters since the node started.
    pub totals: Counters,
    /// Counters since the last accepted report, which was `delta_s` ago.
    pub delta: Counters,
    pub delta_s: f64,
    /// Kind and time (Unix ms) of the last error, if any.
    #[serde(default)]
    pub last_error: Option<LastError>,
    /// The last report of a node that stops.
    #[serde(default)]
    pub stopping: bool,
}

/// The NAT summary of a report: booleans only, no addresses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Nat {
    /// A relay answered QUIC address discovery over IPv4 / IPv6.
    pub udp_v4: bool,
    pub udp_v6: bool,
    /// The public IPv4 address is not on this machine.
    pub behind_nat_v4: Option<bool>,
    /// The NAT kept the local port.
    pub port_preserved_v4: Option<bool>,
    /// Relays saw different public ports (hard NAT); needs two relays.
    pub mapping_varies: Option<bool>,
    pub captive_portal: Option<bool>,
}

/// One open connection in a report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conn {
    /// The peer's default identity.
    pub peer: String,
    /// `in` (the peer dialed) or `out`.
    pub direction: String,
    /// `direct`, `relay` or `unknown`.
    pub path: String,
    /// The relay's URL when the path is `relay`.
    #[serde(default)]
    pub relay: Option<String>,
    /// QUIC's smoothed round-trip time of the selected path.
    #[serde(default)]
    pub rtt_ms: Option<f64>,
    /// Seconds on the current kind of path (direct or relay).
    #[serde(default)]
    pub path_s: Option<u64>,
    /// Bytes per second received and sent since the last report (UDP
    /// payload of the connection, all paths).
    #[serde(default)]
    pub rate_in: f64,
    #[serde(default)]
    pub rate_out: f64,
}

/// Counters of a report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    /// Connections that completed the handshake (both directions).
    pub conns_new: u64,
    /// Failed connects (outgoing) and failed handshakes (incoming).
    pub conns_failed: u64,
    /// Changes between a direct and a relayed path.
    pub path_changes: u64,
    /// Requests served (`listen`) or answered (`soak`), and failed ones.
    pub requests_ok: u64,
    pub requests_failed: u64,
    /// Bytes received and sent over UDP, all paths, and the part of them
    /// that went over relays.
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub bytes_in_relay: u64,
    pub bytes_out_relay: u64,
}

impl Counters {
    /// `self - before`, per counter (0 where a counter went back).
    pub fn since(&self, before: &Counters) -> Counters {
        Counters {
            conns_new: self.conns_new.saturating_sub(before.conns_new),
            conns_failed: self.conns_failed.saturating_sub(before.conns_failed),
            path_changes: self.path_changes.saturating_sub(before.path_changes),
            requests_ok: self.requests_ok.saturating_sub(before.requests_ok),
            requests_failed: self.requests_failed.saturating_sub(before.requests_failed),
            bytes_in: self.bytes_in.saturating_sub(before.bytes_in),
            bytes_out: self.bytes_out.saturating_sub(before.bytes_out),
            bytes_in_relay: self.bytes_in_relay.saturating_sub(before.bytes_in_relay),
            bytes_out_relay: self.bytes_out_relay.saturating_sub(before.bytes_out_relay),
        }
    }
}

/// The last error of a report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastError {
    pub kind: String,
    pub at_ms: u64,
}

/// The counters of `ep` and of this process.
fn totals(ep: &Endpoint) -> Counters {
    let s = ep.stats();
    let m = &ep.iroh().metrics().socket;
    let relay_in = m.recv_data_relay.get();
    let relay_out = m.send_relay.get();
    Counters {
        conns_new: s.connections_outgoing + s.connections_incoming,
        conns_failed: s.connects_failed + s.handshakes_failed,
        path_changes: ACTIVITY.path_changes.load(Ordering::Relaxed),
        requests_ok: ACTIVITY.requests_ok.load(Ordering::Relaxed),
        requests_failed: ACTIVITY.requests_failed.load(Ordering::Relaxed),
        bytes_in: m.recv_data_ipv4.get() + m.recv_data_ipv6.get() + relay_in,
        bytes_out: m.send_ipv4.get() + m.send_ipv6.get() + relay_out,
        bytes_in_relay: relay_in,
        bytes_out_relay: relay_out,
    }
}

/// What the node is, for every report.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub command: &'static str,
    pub name: Option<String>,
    pub transport: TransportMode,
    pub interval: Duration,
}

/// Builds reports; keeps what is needed for deltas and rates.
struct Reporter {
    ep: Endpoint,
    info: NodeInfo,
    started: Instant,
    seq: u64,
    /// Totals and time of the last accepted report.
    last_totals: Counters,
    last_sent: Instant,
    /// Bytes received and sent per connection at the last report.
    conn_bytes: HashMap<usize, (u64, u64)>,
    last_report: Instant,
}

impl Reporter {
    fn new(ep: Endpoint, info: NodeInfo) -> Self {
        let now = Instant::now();
        Self {
            ep,
            info,
            started: now,
            seq: 0,
            last_totals: Counters::default(),
            last_sent: now,
            conn_bytes: HashMap::new(),
            last_report: now,
        }
    }

    fn build(&mut self, stopping: bool) -> Report {
        let ep = &self.ep;
        self.seq += 1;
        let now = Instant::now();
        let window = now.duration_since(self.last_report).as_secs_f64();
        self.last_report = now;
        let paths = PATHS.lock().expect("poisoned").clone();
        let now_ms = now_ms();
        let mut conn_bytes = HashMap::new();
        let mut connections = Vec::new();
        let open = ep.connections();
        for c in &open {
            let stats = c.iroh_connection().stats();
            let (rx, tx) = (stats.udp_rx.bytes, stats.udp_tx.bytes);
            let id = c.stable_id();
            let (rx0, tx0) = self.conn_bytes.get(&id).copied().unwrap_or((0, 0));
            conn_bytes.insert(id, (rx, tx));
            if connections.len() >= MAX_REPORT_CONNECTIONS {
                continue;
            }
            let sel = SelectedPath::of(c);
            let rtt = c
                .iroh_connection()
                .paths()
                .iter()
                .find(|p| p.is_selected())
                .map(|p| round3(p.rtt().as_secs_f64() * 1000.0));
            let rate = |now: u64, before: u64| {
                if window > 0.0 {
                    round3(now.saturating_sub(before) as f64 / window)
                } else {
                    0.0
                }
            };
            connections.push(Conn {
                peer: c.peer().default_id().to_string(),
                direction: match c.direction() {
                    Direction::Outgoing => "out",
                    Direction::Incoming => "in",
                }
                .into(),
                path: path_name(sel.kind).into(),
                // Only a relay URL: the remote address of a direct path is
                // never sent.
                relay: (sel.kind == PathKind::Relay)
                    .then_some(sel.remote)
                    .flatten(),
                rtt_ms: rtt,
                path_s: paths
                    .get(&id)
                    .filter(|p| p.kind == sel.kind)
                    .map(|p| now_ms.saturating_sub(p.since_ms) / 1000),
                rate_in: rate(rx, rx0),
                rate_out: rate(tx, tx0),
            });
        }
        self.conn_bytes = conn_bytes;
        let totals = totals(ep);
        let home = ep.iroh().home_relay_status().get();
        let home = home.first();
        let nat = ep
            .iroh()
            .net_report()
            .get()
            .as_ref()
            .map(|r| crate::bench::local::convert(r, ep))
            .map(|n| Nat {
                udp_v4: n.udp_v4,
                udp_v6: n.udp_v6,
                behind_nat_v4: n.behind_nat_v4,
                port_preserved_v4: n.port_preserved_v4,
                mapping_varies: n.mapping_varies_by_dest,
                captive_portal: n.captive_portal,
            });
        Report {
            v: REPORT_VERSION,
            node_id: ep.node_id().to_string(),
            ts_ms: now_ms,
            seq: self.seq,
            instance: hex::encode(ep.instance_id()),
            name: self
                .info
                .name
                .as_ref()
                .map(|n| n.chars().take(MAX_NAME_LEN).collect()),
            version: env!("CARGO_PKG_VERSION").into(),
            commit: COMMIT.map(Into::into),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            command: self.info.command.into(),
            uptime_s: self.started.elapsed().as_secs(),
            interval_s: round3(self.info.interval.as_secs_f64()),
            transport: match self.info.transport {
                TransportMode::DirectAndRelay => "direct_and_relay",
                TransportMode::RelayOnly => "relay_only",
                TransportMode::DirectOnly => "direct_only",
            }
            .into(),
            home_relay: home.map(|h| h.url().to_string()),
            home_relay_connected: home.is_some_and(|h| h.is_connected()),
            nat,
            connections_open: open.len(),
            connections,
            delta: totals.since(&self.last_totals),
            delta_s: round3(now.duration_since(self.last_sent).as_secs_f64()),
            totals,
            last_error: ACTIVITY.last_error.lock().expect("poisoned").clone(),
            stopping,
        }
    }

    /// The report was accepted: the next delta starts here.
    fn accepted(&mut self, r: &Report) {
        self.last_totals = r.totals;
        self.last_sent = self.last_report;
    }
}

/// Where reports go.
enum Target {
    /// `--telemetry URL`.
    Fixed(url::Url),
    /// The URL the registry advertises, looked up when needed.
    Registry {
        client: RegistryClient,
        url: Option<url::Url>,
        next_lookup: Instant,
    },
}

/// The collector URL in a registry's `GET /v1/info` answer.
pub fn advertised_url(info: &str) -> Option<url::Url> {
    info.lines()
        .find_map(|l| l.trim().strip_prefix("telemetry="))
        .and_then(|u| u.trim().parse::<url::Url>().ok())
        .filter(|u| matches!(u.scheme(), "https" | "http"))
}

/// `base/v1/report`.
pub fn report_url(base: &url::Url) -> url::Url {
    let mut b = base.clone();
    if !b.path().ends_with('/') {
        b.set_path(&format!("{}/", b.path()));
    }
    b.join("v1/report").expect("relative path")
}

impl Target {
    async fn url(&mut self) -> Option<url::Url> {
        match self {
            Target::Fixed(u) => Some(u.clone()),
            Target::Registry {
                client,
                url,
                next_lookup,
            } => {
                if url.is_none() && Instant::now() >= *next_lookup {
                    *next_lookup = Instant::now() + INFO_RETRY;
                    match client.call("GET", "v1/info", None).await {
                        Ok((200, body)) => match advertised_url(&body) {
                            Some(u) => {
                                info!("telemetry: reporting to {u} (opt out: --no-telemetry)");
                                *url = Some(u);
                            }
                            None => debug!("telemetry: the registry advertises no collector"),
                        },
                        Ok((status, body)) => debug!(
                            "telemetry: registry info: {}",
                            golem_p2p::membership::error_text(status, &body)
                        ),
                        Err(e) => debug!("telemetry: registry info: {e}"),
                    }
                }
                url.clone()
            }
        }
    }
}

/// The reporting task of a node. Dropping it stops reports without a last
/// one; [`Telemetry::finish`] sends a last report first.
pub struct Telemetry {
    task: JoinHandle<()>,
    stop: Option<oneshot::Sender<()>>,
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Telemetry {
    /// Starts reporting if telemetry is on for this node, else says why not
    /// (once, at `info`) and returns `None`.
    pub fn start(
        args: &TelemetryArgs,
        node: &NodeArgs,
        ep: &Endpoint,
        signer: InMemorySigner,
        info: NodeInfo,
        out: &Out,
    ) -> anyhow::Result<Option<Self>> {
        if args.telemetry_interval < Duration::from_secs(1) {
            anyhow::bail!("--telemetry-interval must be at least 1s");
        }
        let target = if args.no_telemetry {
            info!("telemetry: off (--no-telemetry)");
            None
        } else if node.registry.is_none() {
            if args.telemetry.is_some() {
                warn!("telemetry: --telemetry needs a closed network (--registry); not reporting");
            }
            None
        } else if let Some(u) = &args.telemetry {
            info!("telemetry: reporting to {u} (opt out: --no-telemetry)");
            Some(Target::Fixed(u.clone()))
        } else {
            let url = node.registry.clone().expect("checked");
            Some(Target::Registry {
                client: RegistryClient::new(url)?,
                url: None,
                next_lookup: Instant::now(),
            })
        };
        out.event("telemetry")
            .field("enabled", target.is_some())
            .field(
                "url",
                match &target {
                    Some(Target::Fixed(u)) => Some(u.to_string()),
                    _ => None,
                },
            )
            .field("interval_s", round3(args.telemetry_interval.as_secs_f64()))
            .emit();
        let Some(target) = target else {
            return Ok(None);
        };
        let tls = golem_p2p::iroh::tls::CaTlsConfig::default()
            .client_config(golem_p2p::iroh::tls::default_provider())?;
        // A report never takes longer than the interval (at most 10 s), so
        // reports never pile up.
        let timeout = args.telemetry_interval.min(Duration::from_secs(10));
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .timeout(timeout)
            .connect_timeout(timeout.min(Duration::from_secs(5)))
            .build()?;
        let (stop_tx, stop_rx) = oneshot::channel();
        let reporter = Reporter::new(ep.clone(), info);
        let task = tokio::spawn(run(
            reporter,
            target,
            http,
            signer,
            args.telemetry_interval,
            stop_rx,
        ));
        Ok(Some(Self {
            task,
            stop: Some(stop_tx),
        }))
    }

    /// Sends a last report (`stopping`), waiting at most 3 s, and stops.
    pub async fn finish(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
            let _ = tokio::time::timeout(Duration::from_secs(3), &mut self.task).await;
        }
    }
}

/// Why a report was not accepted.
#[derive(Debug, PartialEq, Eq)]
enum SendError {
    /// The collector answered with an error status.
    Status(u16, String),
    /// Not sent or no answer.
    Transport(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Status(s, body) => {
                write!(f, "{}", golem_p2p::membership::error_text(*s, body))
            }
            SendError::Transport(e) => write!(f, "{e}"),
        }
    }
}

async fn send(
    http: &reqwest::Client,
    url: &url::Url,
    signer: &InMemorySigner,
    report: &Report,
    cert: &[u8],
) -> Result<(), SendError> {
    let body = serde_json::to_vec(report).map_err(|e| SendError::Transport(e.to_string()))?;
    let id = report
        .node_id
        .parse()
        .map_err(|_| SendError::Transport("bad node id".into()))?;
    let sig = golem_p2p::telemetry::sign(signer, id, &body)
        .await
        .map_err(|e| SendError::Transport(e.to_string()))?;
    let resp = http
        .post(report_url(url))
        .header("content-type", "application/json")
        .header(SIGNATURE_HEADER, hex::encode(sig.as_bytes()))
        .header(CERT_HEADER, hex::encode(cert))
        .body(body)
        .send()
        .await
        .map_err(|e| SendError::Transport(e.to_string()))?;
    let status = resp.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(());
    }
    // The first line of the answer is enough for a log line.
    let text = resp.text().await.unwrap_or_default();
    Err(SendError::Status(
        status,
        text.chars().take(200).collect::<String>(),
    ))
}

async fn run(
    mut reporter: Reporter,
    mut target: Target,
    http: reqwest::Client,
    signer: InMemorySigner,
    interval: Duration,
    mut stop: oneshot::Receiver<()>,
) {
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut failing = false;
    loop {
        let stopping = tokio::select! {
            _ = tick.tick() => false,
            _ = &mut stop => true,
        };
        let Some(url) = target.url().await else {
            if stopping {
                break;
            }
            continue;
        };
        let rec = reporter.ep.local_record();
        let Some(cert) = rec.membership() else {
            debug!("telemetry: no membership certificate yet");
            if stopping {
                break;
            }
            continue;
        };
        let report = reporter.build(stopping);
        match send(&http, &url, &signer, &report, cert).await {
            Ok(()) => {
                reporter.accepted(&report);
                if failing {
                    info!("telemetry: reports accepted again");
                    failing = false;
                }
            }
            Err(e) if !failing => {
                warn!(
                    "telemetry: report to {url} failed: {e} (retried every interval, logged at debug)"
                );
                failing = true;
            }
            Err(e) => debug!("telemetry: report failed: {e}"),
        }
        if stopping {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_url_from_registry_info() {
        let info = "network=n\npublic_key=ab\ncert_lifetime_s=3600\ntelemetry=https://t.example/\n";
        assert_eq!(advertised_url(info).unwrap().as_str(), "https://t.example/");
        assert_eq!(advertised_url("network=n\n"), None);
        assert_eq!(advertised_url("telemetry=ftp://x/\n"), None);
        let u: url::Url = "https://t.example/telemetry".parse().unwrap();
        assert_eq!(
            report_url(&u).as_str(),
            "https://t.example/telemetry/v1/report"
        );
        let u: url::Url = "https://t.example/".parse().unwrap();
        assert_eq!(report_url(&u).as_str(), "https://t.example/v1/report");
    }

    #[test]
    fn counter_deltas() {
        let a = Counters {
            conns_new: 5,
            bytes_in: 100,
            ..Default::default()
        };
        let b = Counters {
            conns_new: 7,
            bytes_in: 50,
            ..Default::default()
        };
        let d = b.since(&a);
        assert_eq!(d.conns_new, 2);
        assert_eq!(d.bytes_in, 0, "a counter that went back is not negative");
    }

    #[test]
    fn path_kind_changes_are_counted() {
        let before = ACTIVITY.path_changes.load(Ordering::Relaxed);
        let id = usize::MAX - 7;
        path_seen(id, PathKind::Relay);
        path_seen(id, PathKind::Unknown);
        path_seen(id, PathKind::Relay);
        path_seen(id, PathKind::Direct);
        assert_eq!(PATHS.lock().unwrap()[&id].kind, PathKind::Direct);
        path_closed(id);
        assert!(!PATHS.lock().unwrap().contains_key(&id));
        // Other tests may count changes concurrently.
        assert!(ACTIVITY.path_changes.load(Ordering::Relaxed) > before);
    }
}
