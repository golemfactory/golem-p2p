//! Telemetry end to end: a registry that advertises a collector, `listen`
//! and `soak` nodes that report to it, and nodes that opt out
//! (`--no-telemetry`, `GOLEM_P2P_NO_TELEMETRY`). A node that stops says so
//! in a last report and is offline at once.

mod common;

use std::time::Duration;

use common::*;
use golem_p2p::registry::{RegistryState, ServeOptions};
use serde_json::Value;

fn strs(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

async fn state(http: &reqwest::Client, collector: &str) -> Value {
    http.get(format!("{collector}v1/state"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn node<'a>(st: &'a Value, id: &str) -> Option<&'a Value> {
    st["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == id)
}

/// Polls the collector's state until `pred` holds.
async fn wait_state(
    http: &reqwest::Client,
    collector: &str,
    what: &str,
    pred: impl Fn(&Value) -> bool,
) -> Value {
    let mut last = Value::Null;
    for _ in 0..150 {
        last = state(http, collector).await;
        if pred(&last) {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("timed out waiting for {what}: {last:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn members_report_unless_they_opt_out() {
    let infra = Infra::start("telemetry").await;
    let (alice, alice_key) = infra.key("alice");
    let (bob, bob_key) = infra.key("bob");
    let (carol, carol_key) = infra.key("carol");
    let (dave, dave_key) = infra.key("dave");
    let (mut st, _admin) = RegistryState::init("golem-testnet", Duration::from_secs(3600)).unwrap();
    for id in [alice, bob, carol, dave] {
        st.nodes.insert(id.to_string(), "test".into());
    }
    let key = hex::encode(st.policy().registry_keys()[0].as_bytes());
    let state_file = infra.tmp.join("registry.json");
    st.save(&state_file).unwrap();

    let store = infra.tmp.join("telemetry");
    let mut collector = Cli::spawn(&strs(&[
        "--json",
        "telemetry",
        "serve",
        "--listen",
        "127.0.0.1:0",
        "--registry-key",
        &key,
        "--network",
        "golem-testnet",
        "--store",
        &store.display().to_string(),
        "--min-report-interval",
        "500ms",
    ]));
    let ev = collector.wait_event("telemetry_collector", 30).await;
    let collector_url = ev["url"].as_str().unwrap().to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let registry_url = format!("http://{}/", listener.local_addr().unwrap());
    let registry = tokio::spawn(golem_p2p::registry::serve_with(
        listener,
        state_file,
        ServeOptions {
            telemetry_url: Some(collector_url.clone()),
        },
    ));
    let registry_key = key.clone();
    let closed = |name: &str, cmd: &str, key: &std::path::Path, extra: &[&str]| {
        let mut a = strs(&["--json", "--node-name", name, cmd]);
        a.extend(infra.node_args(key));
        a.extend(strs(&[
            "--registry",
            &registry_url,
            "--registry-key",
            &registry_key,
            "--network",
            "golem-testnet",
            "--telemetry-interval",
            "1s",
        ]));
        a.extend(strs(extra));
        a
    };

    // alice reports (the registry advertises the collector); bob opts out.
    let mut a = Cli::spawn(&closed("alice", "listen", &alice_key, &[]));
    a.wait_event("ready", 30).await;
    let t = a.wait_event("telemetry", 10).await;
    assert_eq!(t["enabled"], true, "{t}");
    let mut b = Cli::spawn(&closed("bob", "listen", &bob_key, &["--no-telemetry"]));
    b.wait_event("ready", 30).await;
    let t = b.wait_event("telemetry", 10).await;
    assert_eq!(t["enabled"], false, "{t}");
    // carol soaks alice and reports; dave too, opted out by environment.
    let alice_s = alice.to_string();
    let soak = [
        "--to",
        alice_s.as_str(),
        "--interval",
        "200ms",
        "--stats-interval",
        "60s",
    ];
    let mut c = Cli::spawn(&closed("carol", "soak", &carol_key, &soak));
    c.wait_event("ready", 30).await;
    let mut d = Cli::spawn_env(
        &closed("dave", "soak", &dave_key, &soak),
        &[("GOLEM_P2P_NO_TELEMETRY", "1")],
    );
    let t = d.wait_event("telemetry", 30).await;
    assert_eq!(t["enabled"], false, "{t}");

    let tls = golem_p2p::iroh::tls::CaTlsConfig::default()
        .client_config(golem_p2p::iroh::tls::default_provider())
        .unwrap();
    let http = reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .build()
        .unwrap();
    let (carol_s, dave_s) = (carol.to_string(), dave.to_string());
    let st = wait_state(&http, &collector_url, "alice, carol and their link", |st| {
        let alice_ok = node(st, &alice_s).is_some_and(|n| {
            n["status"] == "online" && n["totals"]["requests_ok"].as_u64() > Some(3)
        });
        let carol_ok = node(st, &carol_s).is_some_and(|n| n["status"] == "online");
        let dave_peer = node(st, &dave_s).is_some_and(|n| n["reporting"] == false);
        let link = st["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["reported_by"].as_array().unwrap().len() == 2);
        alice_ok && carol_ok && dave_peer && link
    })
    .await;
    let a_node = node(&st, &alice_s).unwrap();
    assert_eq!(a_node["name"], "alice");
    assert_eq!(a_node["command"], "listen");
    assert_eq!(a_node["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(a_node["transport"], "direct_only");
    assert_eq!(node(&st, &carol_s).unwrap()["command"], "soak");
    // The opted-out nodes never reported.
    assert!(node(&st, &bob.to_string()).is_none(), "{st:#}");
    assert_eq!(node(&st, &dave_s).unwrap()["version"], Value::Null);
    let link = st["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["reported_by"].as_array().unwrap().len() == 2)
        .unwrap();
    assert_eq!(link["path"], "direct", "{link}");
    assert!(link["rtt_ms"].as_f64().is_some(), "{link}");
    // No IP addresses anywhere in what the nodes sent.
    assert!(!st.to_string().contains("127.0.0.1"), "{st:#}");

    // alice stops: her last report makes her offline at once.
    a.terminate();
    let (status, _) = a.finish(20).await;
    assert!(status.success(), "{status}");
    let st = wait_state(&http, &collector_url, "alice offline", |st| {
        node(st, &alice_s).is_some_and(|n| n["status"] == "offline" && n["stopped"] == true)
    })
    .await;
    assert!(node(&st, &carol_s).is_some());

    // Prometheus metrics of the same.
    let m = http
        .get(format!("{collector_url}metrics"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        m.contains(&format!(
            "golem_p2p_node_up{{node_id=\"{alice_s}\",name=\"alice\"}} 0"
        )),
        "{m}"
    );
    for cli in [&b, &c, &d, &collector] {
        cli.terminate();
    }
    registry.abort();
}
