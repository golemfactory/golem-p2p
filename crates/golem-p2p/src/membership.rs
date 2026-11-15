//! Closed networks: membership certificates issued by a registry.
//!
//! In an open network any node with a valid [record](crate::record) may
//! connect. A closed network (for example a private testnet) additionally
//! requires every node to hold a **membership certificate**: a statement,
//! signed by the network's registry, that a given iroh endpoint with a given
//! set of Ethereum identities is a member of the network until some time.
//!
//! * The registry ([`crate::registry`], feature `registry-server`) keeps a
//!   list of registered node IDs, managed by accounts. A node proves that it
//!   owns its identities and its endpoint key by sending its normal signed
//!   record; the registry answers with a certificate if every identity is
//!   registered. No password or token is stored on the node.
//! * The node carries the certificate next to the signatures of its record
//!   ([`VerifiedRecord::membership`]), so it travels wherever the record
//!   goes: to the directory, to peers in the handshake and in record updates.
//! * Peers (and optionally the directory) check certificates offline against
//!   the registry's public key ([`MembershipPolicy`]). They need no
//!   connection to the registry.
//! * Relays are not involved and stay public: they only forward traffic
//!   that is end-to-end encrypted between nodes, and a node that is not a
//!   member is refused by every member it tries to talk to, over a relay or
//!   directly.
//!
//! Certificates are short-lived (one hour by default) and renewed by the node
//! halfway through their lifetime. Removing a node from the registry stops
//! the renewal: the node loses access when its certificate expires, and peers
//! close existing connections to it then.

use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use iroh::{EndpointId, PublicKey, SecretKey};

use serde::{Deserialize, Serialize};

use crate::{NodeId, VerifiedRecord, record::MAX_CLOCK_SKEW, signer::BoxFuture};

/// Format version of [`CertPayload`].
pub const CERT_VERSION: u8 = 1;

/// Domain separation for the registry's signature.
pub const CERT_DOMAIN: &[u8] = b"golem-p2p/membership/v1\0";

/// Longest lifetime a certificate may claim.
pub const MAX_CERT_LIFETIME: Duration = Duration::from_secs(7 * 24 * 3600);

/// Largest encoded certificate accepted.
pub const MAX_CERT_SIZE: usize = 1024;

/// Longest network name.
pub const MAX_NETWORK_LEN: usize = 64;

/// The signed content of a membership certificate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertPayload {
    /// Format version, currently [`CERT_VERSION`].
    pub version: u8,
    /// Name of the network, e.g. `golem-testnet`. A certificate is only
    /// valid in the network it names.
    pub network: String,
    /// The iroh endpoint the certificate is bound to.
    pub endpoint_id: [u8; 32],
    /// The identities the endpoint may use in this network.
    pub identities: Vec<NodeId>,
    /// Issue time, seconds since the Unix epoch.
    pub issued_at: u64,
    /// Expiry, seconds since the Unix epoch.
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize)]
struct CertWire {
    payload: Vec<u8>,
    /// ed25519 public key of the registry that signed it.
    issuer: [u8; 32],
    signature: Vec<u8>,
}

/// Why a certificate or a record's membership was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MembershipError {
    /// The record carries no certificate.
    #[error("no membership certificate")]
    Missing,
    /// The encoding is invalid or too large.
    #[error("malformed membership certificate: {0}")]
    Malformed(&'static str),
    /// Unsupported format version.
    #[error("unsupported membership certificate version {0}")]
    Version(u8),
    /// Signed by a key that is not a trusted registry key of this network.
    #[error("membership certificate signed by an untrusted key")]
    UntrustedIssuer,
    /// The registry's signature does not verify.
    #[error("invalid membership certificate signature")]
    Signature,
    /// The certificate names another network.
    #[error("membership certificate is for network {0:?}")]
    WrongNetwork(String),
    /// The certificate is bound to another endpoint.
    #[error("membership certificate is for another endpoint")]
    WrongEndpoint,
    /// The record lists an identity the certificate does not cover.
    #[error("identity {0} is not covered by the membership certificate")]
    IdentityNotCovered(NodeId),
    /// Expired, not yet valid, or claims too long a lifetime.
    #[error("membership certificate expired or not yet valid")]
    Expired,
}

/// A decoded and signature-checked membership certificate.
#[derive(Clone, PartialEq, Eq)]
pub struct MembershipCert {
    payload: CertPayload,
    issuer: PublicKey,
    bytes: Vec<u8>,
}

impl fmt::Debug for MembershipCert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MembershipCert")
            .field("network", &self.payload.network)
            .field("endpoint_id", &hex::encode(&self.payload.endpoint_id[..5]))
            .field("identities", &self.payload.identities)
            .field("expires_at", &self.payload.expires_at)
            .finish()
    }
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// `secs` after the Unix epoch. Saturates at about 4,000 years, where no
/// platform's `SystemTime` overflows: the times of a decoded certificate are
/// unchecked input, and `UNIX_EPOCH + Duration::from_secs(u64::MAX)` panics.
fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs.min(1 << 37))
}

fn digest(payload: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(CERT_DOMAIN.len() + payload.len());
    m.extend_from_slice(CERT_DOMAIN);
    m.extend_from_slice(payload);
    m
}

impl MembershipCert {
    /// Signs a certificate. Used by the registry.
    pub fn issue(payload: CertPayload, key: &SecretKey) -> Self {
        let bytes_payload = postcard::to_allocvec(&payload).expect("encode certificate");
        let signature = key.sign(&digest(&bytes_payload)).to_bytes().to_vec();
        let issuer = key.public();
        let wire = CertWire {
            payload: bytes_payload,
            issuer: *issuer.as_bytes(),
            signature,
        };
        let bytes = postcard::to_allocvec(&wire).expect("encode certificate");
        Self {
            payload,
            issuer,
            bytes,
        }
    }

    /// Decodes a certificate and checks its signature against the key it
    /// names. Whether that key is trusted, and whether the certificate is
    /// valid now, is checked by [`MembershipPolicy`].
    pub fn decode(bytes: &[u8]) -> Result<Self, MembershipError> {
        if bytes.len() > MAX_CERT_SIZE {
            return Err(MembershipError::Malformed("too large"));
        }
        // Trailing bytes are refused, as in records: one certificate, one
        // encoding.
        let wire: CertWire = match postcard::take_from_bytes(bytes) {
            Ok((wire, [])) => wire,
            _ => return Err(MembershipError::Malformed("encoding")),
        };
        let payload: CertPayload = match postcard::take_from_bytes(&wire.payload) {
            Ok((payload, [])) => payload,
            _ => return Err(MembershipError::Malformed("payload encoding")),
        };
        if payload.version != CERT_VERSION {
            return Err(MembershipError::Version(payload.version));
        }
        let issuer = PublicKey::from_bytes(&wire.issuer)
            .map_err(|_| MembershipError::Malformed("issuer key"))?;
        let sig: [u8; 64] = wire
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| MembershipError::Signature)?;
        issuer
            .verify(&digest(&wire.payload), &iroh::Signature::from_bytes(&sig))
            .map_err(|_| MembershipError::Signature)?;
        Ok(Self {
            payload,
            issuer,
            bytes: bytes.to_vec(),
        })
    }

    /// The signed content.
    pub fn payload(&self) -> &CertPayload {
        &self.payload
    }

    /// The registry key that signed it.
    pub fn issuer(&self) -> PublicKey {
        self.issuer
    }

    /// The encoded certificate.
    pub fn to_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Expiry time (saturating some thousand years from now; a certificate
    /// is only checked by [`MembershipPolicy::check_cert`]).
    pub fn expires_at(&self) -> SystemTime {
        at(self.payload.expires_at)
    }

    /// Issue time (saturating like [`Self::expires_at`]).
    pub fn issued_at(&self) -> SystemTime {
        at(self.payload.issued_at)
    }

    /// Returns `true` if the certificate covers every identity in `ids`.
    pub fn covers(&self, ids: &[NodeId]) -> bool {
        ids.iter().all(|id| self.payload.identities.contains(id))
    }
}

/// Which certificates a node, relay or directory accepts: the network's name
/// and the public keys of its registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipPolicy {
    network: String,
    registry_keys: Vec<PublicKey>,
}

impl MembershipPolicy {
    /// A policy for `network`, trusting the registry key `key`. More keys
    /// (for a key rotation) can be added with [`Self::with_key`].
    pub fn new(network: impl Into<String>, key: PublicKey) -> Self {
        Self {
            network: network.into(),
            registry_keys: vec![key],
        }
    }

    /// Also trusts `key`.
    pub fn with_key(mut self, key: PublicKey) -> Self {
        if !self.registry_keys.contains(&key) {
            self.registry_keys.push(key);
        }
        self
    }

    /// The network name.
    pub fn network(&self) -> &str {
        &self.network
    }

    /// The trusted registry keys.
    pub fn registry_keys(&self) -> &[PublicKey] {
        &self.registry_keys
    }

    /// Checks that `cert` was issued by a trusted key for this network and
    /// endpoint and is valid at `now` (with [`MAX_CLOCK_SKEW`] tolerance).
    /// Does not look at identities; see [`Self::check_record`].
    pub fn check_cert(
        &self,
        cert: &MembershipCert,
        endpoint: EndpointId,
        now: SystemTime,
    ) -> Result<(), MembershipError> {
        if !self.registry_keys.contains(&cert.issuer) {
            return Err(MembershipError::UntrustedIssuer);
        }
        let p = &cert.payload;
        if p.network != self.network {
            return Err(MembershipError::WrongNetwork(p.network.clone()));
        }
        if p.endpoint_id != *endpoint.as_bytes() {
            return Err(MembershipError::WrongEndpoint);
        }
        let now = secs(now);
        let skew = MAX_CLOCK_SKEW.as_secs();
        if p.expires_at.saturating_add(skew) <= now
            || p.issued_at > now.saturating_add(skew)
            || p.expires_at < p.issued_at
            || p.expires_at - p.issued_at > MAX_CERT_LIFETIME.as_secs()
        {
            return Err(MembershipError::Expired);
        }
        Ok(())
    }

    /// Checks the certificate a verified record carries: issued by a trusted
    /// key for this network, bound to the record's endpoint, covering every
    /// identity the record lists, and valid at `now`. Returns the
    /// certificate.
    pub fn check_record(
        &self,
        rec: &VerifiedRecord,
        now: SystemTime,
    ) -> Result<MembershipCert, MembershipError> {
        let bytes = rec.membership().ok_or(MembershipError::Missing)?;
        let cert = MembershipCert::decode(bytes)?;
        self.check_cert(&cert, rec.endpoint_id(), now)?;
        if let Some(id) = rec
            .identities()
            .iter()
            .find(|id| !cert.payload.identities.contains(id))
        {
            return Err(MembershipError::IdentityNotCovered(*id));
        }
        Ok(cert)
    }
}

/// Error of a [`CertIssuer`].
#[derive(Debug, Clone, thiserror::Error)]
pub enum IssueError {
    /// The registry refused: an identity is not registered.
    #[error("registry refused the certificate: {0}")]
    Refused(String),
    /// The registry could not be reached or answered badly.
    #[error("registry unavailable: {0}")]
    Unavailable(String),
    /// The registry's certificate does not pass our own policy: the node is
    /// configured with another network name or registry key than the
    /// registry it asked.
    #[error(
        "the registry's certificate does not match this node's network settings (network name, registry key): {0}"
    )]
    Mismatch(MembershipError),
}

/// Something that issues membership certificates for this node: the
/// registry, over HTTP ([`RegistryClient`]), or in-process in tests.
pub trait CertIssuer: fmt::Debug + Send + Sync + 'static {
    /// Asks for a certificate for the endpoint and identities of `proof`,
    /// a record signed by the endpoint key and every identity.
    fn certify<'a>(
        &'a self,
        proof: &'a VerifiedRecord,
    ) -> BoxFuture<'a, Result<Vec<u8>, IssueError>>;
}

/// Closed-network settings of an endpoint: which certificates it accepts
/// from peers, and where it gets its own.
#[derive(Debug, Clone)]
pub struct Membership {
    pub(crate) policy: MembershipPolicy,
    pub(crate) issuer: std::sync::Arc<dyn CertIssuer>,
}

impl Membership {
    /// Accept peers by `policy`; get our certificates from `issuer`.
    pub fn new(policy: MembershipPolicy, issuer: impl CertIssuer) -> Self {
        Self {
            policy,
            issuer: std::sync::Arc::new(issuer),
        }
    }

    /// The policy.
    pub fn policy(&self) -> &MembershipPolicy {
        &self.policy
    }
}

/// HTTP client of the registry's certification endpoint:
/// `POST {base}/v1/certify` with the encoded signed record as body; the
/// answer is the certificate in hex (`200`), or `403` with a reason.
#[cfg(feature = "http-directory")]
#[derive(Debug, Clone)]
pub struct RegistryClient {
    base: url::Url,
    client: reqwest::Client,
}

#[cfg(feature = "http-directory")]
impl RegistryClient {
    /// A client for the registry at `base` (e.g. `https://registry.example/`),
    /// trusting the standard web PKI roots.
    pub fn new(base: url::Url) -> Result<Self, IssueError> {
        Self::with_tls(base, iroh::tls::CaTlsConfig::default())
    }

    /// Like [`Self::new`], with explicit TLS roots.
    pub fn with_tls(base: url::Url, tls: iroh::tls::CaTlsConfig) -> Result<Self, IssueError> {
        let tls = tls
            .client_config(iroh::tls::default_provider())
            .map_err(|e| IssueError::Unavailable(e.to_string()))?;
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| IssueError::Unavailable(e.to_string()))?;
        Ok(Self { base, client })
    }
}

/// Largest answer of the registry's management API that a client reads.
#[cfg(feature = "http-directory")]
const MAX_MANAGEMENT_ANSWER: usize = 4 << 20;

/// Reads a response body of at most `limit` bytes.
#[cfg(feature = "http-directory")]
async fn read_body(mut resp: reqwest::Response, limit: usize) -> Result<Vec<u8>, IssueError> {
    let mut body = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| IssueError::Unavailable(e.to_string()))?
    {
        if body.len() + chunk.len() > limit {
            return Err(IssueError::Unavailable("answer too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(feature = "http-directory")]
impl RegistryClient {
    /// One call of the registry's management API (see [`crate::registry`]):
    /// `method` (`GET`, `POST`, `PUT`, `DELETE`) on `path` (e.g.
    /// `v1/nodes/0x..`), with an optional bearer token. Returns the HTTP
    /// status and the body text (at most 4 MiB).
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
    ) -> Result<(u16, String), IssueError> {
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| IssueError::Unavailable(e.to_string()))?;
        let url = self
            .base
            .join(path.trim_start_matches('/'))
            .map_err(|e| IssueError::Unavailable(e.to_string()))?;
        let mut req = self.client.request(method, url);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| IssueError::Unavailable(e.to_string()))?;
        let status = resp.status().as_u16();
        let body = read_body(resp, MAX_MANAGEMENT_ANSWER).await?;
        Ok((status, String::from_utf8_lossy(&body).into_owned()))
    }
}

/// An error answer of the registry, or of the proxy in front of it, for a
/// message: `HTTP <status>` and the first line of the body, unless the body
/// is an HTML page (a proxy's error page, e.g. a `502` while the registry is
/// down).
pub fn error_text(status: u16, body: &str) -> String {
    let body = body.trim();
    let line: String = body
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    if line.is_empty() || line.starts_with('<') {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {line}")
    }
}

#[cfg(feature = "http-directory")]
impl CertIssuer for RegistryClient {
    fn certify<'a>(
        &'a self,
        proof: &'a VerifiedRecord,
    ) -> BoxFuture<'a, Result<Vec<u8>, IssueError>> {
        Box::pin(async move {
            let url = self
                .base
                .join("v1/certify")
                .map_err(|e| IssueError::Unavailable(e.to_string()))?;
            let resp = self
                .client
                .post(url)
                .header("content-type", "application/octet-stream")
                .body(proof.to_bytes().to_vec())
                .send()
                .await
                .map_err(|e| IssueError::Unavailable(e.to_string()))?;
            let status = resp.status();
            let body = read_body(resp, 2 * MAX_CERT_SIZE + 1024).await?;
            let text = String::from_utf8_lossy(&body).trim().to_string();
            // Too many requests (a rate limit in front of the registry) and
            // request timeouts are temporary, not a refusal.
            let temporary = matches!(status.as_u16(), 408 | 429);
            if status.is_client_error() && !temporary {
                if text.starts_with('<') {
                    return Err(IssueError::Refused(error_text(status.as_u16(), &text)));
                }
                return Err(IssueError::Refused(text));
            }
            if !status.is_success() {
                return Err(IssueError::Unavailable(error_text(status.as_u16(), &text)));
            }
            hex::decode(&text).map_err(|_| IssueError::Unavailable("bad certificate hex".into()))
        })
    }
}
