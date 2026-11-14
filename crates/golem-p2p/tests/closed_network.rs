//! Closed networks: a registry over HTTP, members that connect, outsiders
//! that are refused in both directions, accounts and runtime identity
//! changes.

mod common;

use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use common::*;
use golem_p2p::{
    CertIssuer, Discovery, Endpoint, Error, Identities, InMemorySigner, IssueError, Membership,
    MembershipCert, MembershipError, MembershipPolicy, MemoryDiscovery, NodeId, RegistryClient,
    TransportMode,
    membership::{CERT_VERSION, CertPayload},
    registry::RegistryState,
    wire::RejectReason,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Registry {
    addr: SocketAddr,
    admin: String,
    policy: MembershipPolicy,
    _dir: TempDir,
    task: tokio::task::JoinHandle<()>,
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Registry {
    async fn spawn() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "golem-p2p-registry-{}",
            hex::encode(rand::random::<[u8; 8]>())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("registry.json");
        let (state, admin) = RegistryState::init("test-net", Duration::from_secs(3600)).unwrap();
        state.save(&path).unwrap();
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            golem_p2p::registry::serve(listener, path).await.unwrap();
        });
        Self {
            addr,
            admin,
            policy: state.policy(),
            _dir: TempDir(dir),
            task,
        }
    }

    fn client(&self) -> RegistryClient {
        RegistryClient::new(format!("http://{}/", self.addr).parse().unwrap()).unwrap()
    }

    fn membership(&self) -> Membership {
        Membership::new(self.policy.clone(), self.client())
    }

    /// A raw HTTP/1.1 request; returns the status and body.
    async fn http(&self, method: &str, path: &str, token: Option<&str>) -> (u16, String) {
        let mut s = tokio::net::TcpStream::connect(self.addr).await.unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: registry\r\n{auth}Content-Length: 0\r\nConnection: close\r\n\r\n"
        );
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf).to_string();
        let status = text[9..12].parse().unwrap();
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.trim().to_string())
            .unwrap_or_default();
        (status, body)
    }

    async fn account(&self, name: &str) -> String {
        let (status, token) = self
            .http("POST", &format!("/v1/accounts/{name}"), Some(&self.admin))
            .await;
        assert_eq!(status, 200, "create account {name}: {token}");
        token
    }

    async fn register(&self, token: &str, id: NodeId) -> u16 {
        self.http("PUT", &format!("/v1/nodes/{id}"), Some(token))
            .await
            .0
    }
}

async fn node(
    discovery: &Arc<dyn Discovery>,
    signer: &InMemorySigner,
    ids: Identities,
    membership: Option<Membership>,
) -> golem_p2p::Result<Endpoint> {
    let mut b = Endpoint::builder(signer.clone(), ids)
        .discovery(discovery.clone())
        .transport_mode(TransportMode::DirectOnly)
        .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .connect_timeout(Duration::from_secs(5))
        .request_timeout(Duration::from_secs(5));
    if let Some(m) = membership {
        b = b.membership(m);
    }
    b.bind().await
}

#[tokio::test(flavor = "multi_thread")]
async fn members_connect_outsiders_are_refused() {
    init_tracing();
    with_timeout(60, async {
        let reg = Registry::spawn().await;
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let (a, b, c) = (signer.generate(), signer.generate(), signer.generate());

        let alice = reg.account("alice").await;
        assert_eq!(reg.register(&alice, a).await, 204);
        assert_eq!(reg.register(&alice, b).await, 204);

        let na = node(
            &discovery,
            &signer,
            Identities::new(a),
            Some(reg.membership()),
        )
        .await
        .unwrap();
        let nb = node(
            &discovery,
            &signer,
            Identities::new(b),
            Some(reg.membership()),
        )
        .await
        .unwrap();
        spawn_echo(&na);
        spawn_echo(&nb);
        assert!(na.local_record().membership().is_some());
        reg.policy
            .check_record(&na.local_record(), SystemTime::now())
            .unwrap();

        // Members talk to each other, both ways.
        assert_eq!(echo(&na, b, "hi").await.unwrap().from, a);
        assert_eq!(echo(&nb, a, "hi").await.unwrap().from, b);

        // An outsider cannot get a certificate...
        let err = node(
            &discovery,
            &signer,
            Identities::new(c),
            Some(reg.membership()),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, Error::Certify(IssueError::Refused(_))),
            "{err}"
        );

        // ...and without one, members refuse it in both directions.
        let nc = node(&discovery, &signer, Identities::new(c), None)
            .await
            .unwrap();
        spawn_echo(&nc);
        let err = echo(&nc, a, "let me in").await.unwrap_err();
        assert!(
            matches!(err, Error::Rejected(RejectReason::NotMember)),
            "{err}"
        );
        let err = echo(&na, c, "hello?").await.unwrap_err();
        assert!(matches!(err, Error::NotFound(id) if id == c), "{err}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn accounts_own_their_nodes() {
    init_tracing();
    with_timeout(30, async {
        let reg = Registry::spawn().await;
        let signer = InMemorySigner::new();
        let (a, b) = (signer.generate(), signer.generate());

        // Only the admin creates accounts.
        assert_eq!(reg.http("POST", "/v1/accounts/x", None).await.0, 401);
        assert_eq!(
            reg.http("POST", "/v1/accounts/x", Some("nope")).await.0,
            401
        );
        assert_eq!(
            reg.http("POST", "/v1/accounts/bad%20name", Some(&reg.admin))
                .await
                .0,
            400
        );
        let alice = reg.account("alice").await;
        let bob = reg.account("bob").await;
        assert_eq!(
            reg.http("POST", "/v1/accounts/alice", Some(&reg.admin))
                .await
                .0,
            409
        );

        // An account cannot take over or remove another account's node.
        assert_eq!(reg.register(&alice, a).await, 204);
        assert_eq!(reg.register(&alice, a).await, 204, "idempotent");
        assert_eq!(reg.register(&bob, a).await, 409);
        assert_eq!(
            reg.http("DELETE", &format!("/v1/nodes/{a}"), Some(&bob))
                .await
                .0,
            404
        );
        assert_eq!(reg.register(&bob, b).await, 204);
        assert_eq!(
            reg.http("GET", "/v1/nodes", Some(&alice)).await,
            (200, a.to_string())
        );
        let (_, all) = reg.http("GET", "/v1/nodes", Some(&reg.admin)).await;
        assert!(all.contains(&format!("{a} alice")) && all.contains(&format!("{b} bob")));
        let (_, accounts) = reg.http("GET", "/v1/accounts", Some(&reg.admin)).await;
        assert_eq!(accounts, "alice 1\nbob 1");

        // The certificate endpoint follows the registrations.
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let na = node(
            &discovery,
            &signer,
            Identities::new(a),
            Some(reg.membership()),
        )
        .await
        .unwrap();
        assert!(reg.client().certify(&na.local_record()).await.is_ok());
        assert_eq!(
            reg.http("DELETE", &format!("/v1/nodes/{a}"), Some(&alice))
                .await
                .0,
            204
        );
        let err = reg.client().certify(&na.local_record()).await.unwrap_err();
        assert!(matches!(err, IssueError::Refused(_)), "{err}");

        // Removing an account removes its nodes.
        assert_eq!(
            reg.http("DELETE", "/v1/accounts/bob", Some(&reg.admin))
                .await
                .0,
            204
        );
        let (_, all) = reg.http("GET", "/v1/nodes", Some(&reg.admin)).await;
        assert!(all.is_empty(), "{all}");
        assert_eq!(
            reg.register(&bob, b).await,
            401,
            "token of a removed account"
        );

        let (status, info) = reg.http("GET", "/v1/info", None).await;
        assert_eq!(status, 200);
        assert!(info.contains("network=test-net"), "{info}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn new_alias_needs_registration() {
    init_tracing();
    with_timeout(30, async {
        let reg = Registry::spawn().await;
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let (a, b, alias) = (signer.generate(), signer.generate(), signer.generate());
        let alice = reg.account("alice").await;
        reg.register(&alice, a).await;
        reg.register(&alice, b).await;
        let na = node(
            &discovery,
            &signer,
            Identities::new(a),
            Some(reg.membership()),
        )
        .await
        .unwrap();
        let nb = node(
            &discovery,
            &signer,
            Identities::new(b),
            Some(reg.membership()),
        )
        .await
        .unwrap();
        spawn_echo(&na);

        // Not registered: the record keeps its old identities.
        let err = na.add_alias(alias).await.unwrap_err();
        assert!(
            matches!(err, Error::Certify(IssueError::Refused(_))),
            "{err}"
        );
        assert_eq!(na.local_record().identities(), &[a]);

        // Registered: the new certificate covers it and peers can use it.
        assert_eq!(reg.register(&alice, alias).await, 204);
        let rec = na.add_alias(alias).await.unwrap();
        reg.policy.check_record(&rec, SystemTime::now()).unwrap();
        assert_eq!(echo_eventually(&nb, alias, "hi", 10).await.to, alias);
    })
    .await;
}

/// Offline certificate checks: untrusted keys, other networks, other
/// endpoints, uncovered identities and expiry.
#[tokio::test]
async fn certificate_checks() {
    let key = golem_p2p::iroh::SecretKey::generate();
    let other_key = golem_p2p::iroh::SecretKey::generate();
    let endpoint = golem_p2p::iroh::SecretKey::generate().public();
    let signer = InMemorySigner::new();
    let id = signer.generate();
    let now = SystemTime::now();
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload = |network: &str, ep: [u8; 32], issued: u64, expires: u64| CertPayload {
        version: CERT_VERSION,
        network: network.into(),
        endpoint_id: ep,
        identities: vec![id],
        issued_at: issued,
        expires_at: expires,
    };
    let policy = MembershipPolicy::new("net", key.public());
    let ok = MembershipCert::issue(
        payload("net", *endpoint.as_bytes(), secs, secs + 3600),
        &key,
    );
    let decoded = MembershipCert::decode(ok.to_bytes()).unwrap();
    policy.check_cert(&decoded, endpoint, now).unwrap();
    assert!(decoded.covers(&[id]));

    let untrusted = MembershipCert::issue(
        payload("net", *endpoint.as_bytes(), secs, secs + 3600),
        &other_key,
    );
    assert_eq!(
        policy.check_cert(&untrusted, endpoint, now),
        Err(MembershipError::UntrustedIssuer)
    );
    // A second trusted key (key rotation).
    let rotated = policy.clone().with_key(other_key.public());
    rotated.check_cert(&untrusted, endpoint, now).unwrap();

    let wrong_net = MembershipCert::issue(
        payload("other", *endpoint.as_bytes(), secs, secs + 3600),
        &key,
    );
    assert!(matches!(
        policy.check_cert(&wrong_net, endpoint, now),
        Err(MembershipError::WrongNetwork(_))
    ));
    let other_ep = golem_p2p::iroh::SecretKey::generate().public();
    assert_eq!(
        policy.check_cert(&ok, other_ep, now),
        Err(MembershipError::WrongEndpoint)
    );
    // Expired: valid until expiry plus the clock skew allowance.
    let later = now + Duration::from_secs(3600 + 301);
    assert_eq!(
        policy.check_cert(&ok, endpoint, later),
        Err(MembershipError::Expired)
    );
    // Too long a lifetime.
    let forever = MembershipCert::issue(
        payload("net", *endpoint.as_bytes(), secs, secs + 365 * 86400),
        &key,
    );
    assert_eq!(
        policy.check_cert(&forever, endpoint, now),
        Err(MembershipError::Expired)
    );
    // A tampered certificate does not decode.
    let mut bytes = ok.to_bytes().to_vec();
    let n = bytes.len();
    bytes[n - 1] ^= 1;
    assert!(MembershipCert::decode(&bytes).is_err());
}

/// An in-process registry for timing tests: issues certificates for any
/// identity, `lifetime_s` long, with the registry's clock `behind_s` behind
/// ours, until `refuse` is set (the node was removed). Counts its calls.
#[derive(Debug, Clone)]
struct ScriptedIssuer(Arc<IssuerState>);

#[derive(Debug)]
struct IssuerState {
    key: golem_p2p::iroh::SecretKey,
    network: String,
    lifetime_s: u64,
    behind_s: u64,
    refuse: std::sync::atomic::AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
}

impl ScriptedIssuer {
    fn new(key: &golem_p2p::iroh::SecretKey, lifetime_s: u64, behind_s: u64) -> Self {
        Self(Arc::new(IssuerState {
            key: key.clone(),
            network: "test-net".into(),
            lifetime_s,
            behind_s,
            refuse: false.into(),
            calls: 0.into(),
        }))
    }

    fn calls(&self) -> usize {
        self.0.calls.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn refuse(&self) {
        self.0
            .refuse
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl CertIssuer for ScriptedIssuer {
    fn certify<'a>(
        &'a self,
        proof: &'a golem_p2p::VerifiedRecord,
    ) -> golem_p2p::signer::BoxFuture<'a, Result<Vec<u8>, IssueError>> {
        Box::pin(async move {
            use std::sync::atomic::Ordering::SeqCst;
            let s = &self.0;
            s.calls.fetch_add(1, SeqCst);
            if s.refuse.load(SeqCst) {
                return Err(IssueError::Refused("removed".into()));
            }
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - s.behind_s;
            let cert = MembershipCert::issue(
                CertPayload {
                    version: CERT_VERSION,
                    network: s.network.clone(),
                    endpoint_id: *proof.endpoint_id().as_bytes(),
                    identities: proof.identities().to_vec(),
                    issued_at: now,
                    expires_at: now + s.lifetime_s,
                },
                &s.key,
            );
            Ok(cert.to_bytes().to_vec())
        })
    }
}

/// Certificates are renewed on live connections: a node with 2 s
/// certificates renews about every second, its peer gets every renewed
/// record over the connection, and the connection stays up.
#[tokio::test(flavor = "multi_thread")]
async fn certificates_renew_on_live_connections() {
    init_tracing();
    with_timeout(60, async {
        let key = golem_p2p::iroh::SecretKey::generate();
        let policy = MembershipPolicy::new("test-net", key.public());
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let (a, b) = (signer.generate(), signer.generate());
        let stable = ScriptedIssuer::new(&key, 3600, 0);
        let short = ScriptedIssuer::new(&key, 2, 0);
        let na = node(
            &discovery,
            &signer,
            Identities::new(a),
            Some(Membership::new(policy.clone(), stable.clone())),
        )
        .await
        .unwrap();
        let nb = node(
            &discovery,
            &signer,
            Identities::new(b),
            Some(Membership::new(policy.clone(), short.clone())),
        )
        .await
        .unwrap();
        spawn_echo(&na);
        let mut events = na.subscribe();
        echo(&nb, a, "first").await.unwrap();
        let conn = nb.connections()[0].stable_id();

        let mut renewed = 0;
        let mut last_expiry = 0;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
        while let Ok(ev) = tokio::time::timeout_at(deadline, events.recv()).await {
            if let golem_p2p::Event::PeerUpdated { current, .. } = ev.unwrap() {
                let cert = policy.check_record(&current, SystemTime::now()).unwrap();
                assert!(cert.payload().expires_at >= last_expiry);
                last_expiry = cert.payload().expires_at;
                renewed += 1;
            }
        }
        assert!(renewed >= 3, "only {renewed} renewed records in 6 s");
        assert!(short.calls() >= 4, "{} certify calls", short.calls());
        // The same connection carries on.
        echo(&nb, a, "still there").await.unwrap();
        assert_eq!(nb.connections().len(), 1);
        assert_eq!(nb.connections()[0].stable_id(), conn);
        // The node with 1 h certificates did not renew.
        assert_eq!(stable.calls(), 1);
    })
    .await;
}

/// The renewal is timed on the node's clock: a registry whose clock is far
/// behind (here 40 s, with 60 s certificates, so every certificate looks
/// past its half-life on arrival) does not make the node renew in a loop.
#[tokio::test(flavor = "multi_thread")]
async fn renewal_does_not_loop_on_registry_clock_offset() {
    init_tracing();
    with_timeout(30, async {
        let key = golem_p2p::iroh::SecretKey::generate();
        let policy = MembershipPolicy::new("test-net", key.public());
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let issuer = ScriptedIssuer::new(&key, 60, 40);
        let _n = node(
            &discovery,
            &signer,
            Identities::new(signer.generate()),
            Some(Membership::new(policy, issuer.clone())),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(issuer.calls(), 1, "renewed before half the lifetime");
    })
    .await;
}

/// A node whose certificate expires (it was removed from the registry, so
/// renewal is refused) loses its live connection: its peer closes it once
/// the certificate is past expiry plus the clock tolerance, and refuses new
/// connections. The certificate is issued so that this point is 4 s away;
/// waiting for the real 5 min tolerance is left to manual tests.
#[tokio::test(flavor = "multi_thread")]
async fn expired_member_is_disconnected() {
    init_tracing();
    with_timeout(60, async {
        let key = golem_p2p::iroh::SecretKey::generate();
        let policy = MembershipPolicy::new("test-net", key.public());
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let (a, b) = (signer.generate(), signer.generate());
        let stable = ScriptedIssuer::new(&key, 3600, 0);
        // Expires (with the 300 s tolerance) 4 s from now; renewal due in 30 s.
        let expiring = ScriptedIssuer::new(&key, 60, 60 + 300 - 4);
        let na = node(
            &discovery,
            &signer,
            Identities::new(a),
            Some(Membership::new(policy.clone(), stable.clone())),
        )
        .await
        .unwrap();
        let nb = node(
            &discovery,
            &signer,
            Identities::new(b),
            Some(Membership::new(policy.clone(), expiring.clone())),
        )
        .await
        .unwrap();
        expiring.refuse();
        spawn_echo(&na);
        let mut events_a = na.subscribe();
        let mut events_b = nb.subscribe();
        echo(&nb, a, "while valid").await.unwrap();
        assert_eq!(na.connections().len(), 1);

        async fn disconnected(
            events: &mut tokio::sync::broadcast::Receiver<golem_p2p::Event>,
        ) -> (golem_p2p::VerifiedRecord, String) {
            let wait = async {
                loop {
                    if let golem_p2p::Event::Disconnected { peer, reason } =
                        events.recv().await.unwrap()
                    {
                        break (peer, reason);
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(15), wait)
                .await
                .expect("connection not closed after expiry")
        }
        // The peer closes the connection...
        let (peer, _) = disconnected(&mut events_a).await;
        assert_eq!(peer.default_id(), b);
        assert!(policy.check_record(&peer, SystemTime::now()).is_err());
        // ...and the expired node learns why.
        let (peer, reason) = disconnected(&mut events_b).await;
        assert_eq!(peer.default_id(), a);
        assert!(reason.contains("membership expired"), "{reason}");
        // A new connection is refused.
        let err = echo(&nb, a, "after expiry").await.unwrap_err();
        assert!(
            matches!(err, Error::Rejected(RejectReason::NotMember)),
            "{err}"
        );
    })
    .await;
}

/// Reads one HTTP/1.1 request with its `Content-Length` body, so that the
/// answer is not lost to a reset for unread data.
async fn read_http_request(s: &mut tokio::net::TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = s.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if buf.len() >= end + 4 + len {
                return;
            }
        }
    }
}

/// A rate limit in front of the registry (`429`) is a temporary failure,
/// not a refusal; other `4xx` answers are refusals.
#[tokio::test(flavor = "multi_thread")]
async fn rate_limited_registry_is_unavailable_not_refusing() {
    init_tracing();
    with_timeout(30, async {
        for (status, temporary) in [("429 Too Many Requests", true), ("403 Forbidden", false)] {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                loop {
                    let (mut s, _) = listener.accept().await.unwrap();
                    read_http_request(&mut s).await;
                    let resp = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow"
                    );
                    let _ = s.write_all(resp.as_bytes()).await;
                }
            });
            let key = golem_p2p::iroh::SecretKey::generate();
            let client = RegistryClient::new(format!("http://{addr}/").parse().unwrap()).unwrap();
            let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
            let signer = InMemorySigner::new();
            let err = node(
                &discovery,
                &signer,
                Identities::new(signer.generate()),
                Some(Membership::new(
                    MembershipPolicy::new("test-net", key.public()),
                    client,
                )),
            )
            .await
            .unwrap_err();
            if temporary {
                assert!(
                    matches!(err, Error::Certify(IssueError::Unavailable(_))),
                    "{status}: {err}"
                );
            } else {
                assert!(
                    matches!(err, Error::Certify(IssueError::Refused(_))),
                    "{status}: {err}"
                );
            }
            server.abort();
        }
    })
    .await;
}

/// A node configured with another network name or registry key than its
/// registry's fails at bind with an error that says so (not as if a peer
/// were refused).
#[tokio::test(flavor = "multi_thread")]
async fn wrong_network_settings_fail_clearly() {
    init_tracing();
    with_timeout(30, async {
        let reg = Registry::spawn().await;
        let discovery: Arc<dyn Discovery> = Arc::new(MemoryDiscovery::new());
        let signer = InMemorySigner::new();
        let a = signer.generate();
        let alice = reg.account("alice").await;
        assert_eq!(reg.register(&alice, a).await, 204);
        let key = reg.policy.registry_keys()[0];
        let other_key = golem_p2p::iroh::SecretKey::generate().public();
        for (policy, want) in [
            (
                MembershipPolicy::new("other-net", key),
                MembershipError::WrongNetwork("test-net".into()),
            ),
            (
                MembershipPolicy::new("test-net", other_key),
                MembershipError::UntrustedIssuer,
            ),
        ] {
            let err = node(
                &discovery,
                &signer,
                Identities::new(a),
                Some(Membership::new(policy, reg.client())),
            )
            .await
            .unwrap_err();
            match &err {
                Error::Certify(IssueError::Mismatch(e)) => assert_eq!(e, &want),
                _ => panic!("unexpected error: {err}"),
            }
            assert!(err.to_string().contains("network settings"), "{err}");
        }
    })
    .await;
}
