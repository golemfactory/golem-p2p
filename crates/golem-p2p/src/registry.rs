//! The registry of a closed network (feature `registry-server`; its HTTP
//! client, [`RegistryClient`](crate::membership::RegistryClient), needs
//! `http-directory`).
//!
//! The registry decides who is a member. It keeps:
//!
//! * **accounts**, created by the administrator; each gets a random token;
//! * **registered nodes**: Ethereum node IDs, each owned by one account;
//! * the network's **signing key**, which signs membership certificates
//!   ([`crate::membership`]).
//!
//! A node asks for a certificate with its normal signed record (which proves
//! it owns the endpoint key and every identity it lists); the registry
//! answers with a certificate if every identity is registered. Nodes never
//! need an account token.
//!
//! HTTP API (plain text, plain HTTP: run it behind a TLS-terminating proxy):
//!
//! | method and path | auth | answer |
//! |---|---|---|
//! | `GET /v1/info` | none | `network=..`, `public_key=..`, `cert_lifetime_s=..` lines |
//! | `POST /v1/certify` (body: record) | none (the record is the proof) | certificate in hex; `403` not registered |
//! | `POST /v1/accounts/{name}` | admin | the new account's token; `409` exists |
//! | `DELETE /v1/accounts/{name}` | admin | `204`; the account's nodes are removed too |
//! | `GET /v1/accounts` | admin | `<name> <node count>` lines |
//! | `PUT /v1/nodes/{node id}` | account | `204`; `409` registered by another account |
//! | `DELETE /v1/nodes/{node id}` | account | `204`; `404` not one of the account's nodes |
//! | `GET /v1/nodes` | account (admin: all, as `<node id> <account>`) | node ID lines |
//!
//! Tokens are sent as `Authorization: Bearer <token>` and stored only as
//! SHA-256 hashes.

#[cfg(feature = "registry-server")]
pub use server::*;

#[cfg(feature = "registry-server")]
mod server {
    use std::{
        collections::BTreeMap,
        path::{Path as FsPath, PathBuf},
        sync::{Arc, Mutex},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use axum::{
        Router,
        body::Bytes,
        extract::{DefaultBodyLimit, Path, State},
        http::{HeaderMap, StatusCode, header},
        response::{IntoResponse, Response},
        routing::{get, post, put},
    };
    use iroh::SecretKey;
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};

    use crate::{
        NodeId, VerifiedRecord,
        membership::{
            CERT_VERSION, CertPayload, MAX_CERT_LIFETIME, MAX_NETWORK_LEN, MembershipCert,
            MembershipPolicy,
        },
        record::MAX_RECORD_SIZE,
    };

    /// Default lifetime of issued certificates.
    pub const DEFAULT_CERT_LIFETIME: Duration = Duration::from_secs(3600);

    /// Longest account name.
    pub const MAX_ACCOUNT_NAME: usize = 64;

    /// The registry's persistent state (one JSON file, mode 0600).
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RegistryState {
        /// Network name, part of every certificate.
        pub network: String,
        /// The signing key, hex.
        secret_key: String,
        /// Lifetime of issued certificates, seconds.
        pub cert_lifetime_s: u64,
        /// SHA-256 of the admin token, hex.
        admin_token_sha256: String,
        /// Accounts by name.
        pub accounts: BTreeMap<String, Account>,
        /// Registered node IDs (`0x..`, lowercase) and the account that owns
        /// each.
        pub nodes: BTreeMap<String, String>,
    }

    /// An account.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Account {
        /// SHA-256 of the account token, hex.
        token_sha256: String,
        /// Creation time, seconds since the Unix epoch.
        pub created_at: u64,
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn new_token() -> String {
        hex::encode(rand::random::<[u8; 32]>())
    }

    fn hash(token: &str) -> String {
        hex::encode(Sha256::digest(token.as_bytes()))
    }

    fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= MAX_ACCOUNT_NAME
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }

    impl RegistryState {
        /// A new registry: a fresh signing key and admin token. Returns the
        /// state and the admin token (shown once; only its hash is kept).
        pub fn init(network: &str, cert_lifetime: Duration) -> anyhow_lite::Result<(Self, String)> {
            if network.is_empty() || network.len() > MAX_NETWORK_LEN {
                return Err(anyhow_lite::msg("network name must be 1 to 64 characters"));
            }
            if cert_lifetime < Duration::from_secs(60) || cert_lifetime > MAX_CERT_LIFETIME {
                return Err(anyhow_lite::msg(
                    "certificate lifetime must be 60 s to 7 days",
                ));
            }
            let admin = new_token();
            let key = SecretKey::generate();
            Ok((
                Self {
                    network: network.to_string(),
                    secret_key: hex::encode(key.to_bytes()),
                    cert_lifetime_s: cert_lifetime.as_secs(),
                    admin_token_sha256: hash(&admin),
                    accounts: BTreeMap::new(),
                    nodes: BTreeMap::new(),
                },
                admin,
            ))
        }

        /// Loads the state file.
        pub fn load(path: &FsPath) -> anyhow_lite::Result<Self> {
            let text = std::fs::read_to_string(path)
                .map_err(|e| anyhow_lite::msg(format!("reading {}: {e}", path.display())))?;
            let st: Self = serde_json::from_str(&text)
                .map_err(|e| anyhow_lite::msg(format!("parsing {}: {e}", path.display())))?;
            st.key()?;
            Ok(st)
        }

        /// Writes the state file atomically (temporary file, then rename),
        /// readable by the owner only.
        pub fn save(&self, path: &FsPath) -> anyhow_lite::Result<()> {
            let tmp = path.with_extension("tmp");
            let text = serde_json::to_string_pretty(self).expect("encode state");
            write_private(&tmp, text.as_bytes())
                .map_err(|e| anyhow_lite::msg(format!("writing {}: {e}", tmp.display())))?;
            std::fs::rename(&tmp, path)
                .map_err(|e| anyhow_lite::msg(format!("writing {}: {e}", path.display())))?;
            Ok(())
        }

        fn key(&self) -> anyhow_lite::Result<SecretKey> {
            let mut b = [0u8; 32];
            hex::decode_to_slice(&self.secret_key, &mut b)
                .map_err(|_| anyhow_lite::msg("bad secret_key in state"))?;
            Ok(SecretKey::from_bytes(&b))
        }

        /// The policy nodes use to check certificates of this registry.
        pub fn policy(&self) -> MembershipPolicy {
            MembershipPolicy::new(self.network.clone(), self.key().expect("checked").public())
        }

        /// Issues a certificate for the endpoint and identities of `proof`
        /// if every identity is registered.
        pub fn certify(&self, proof: &VerifiedRecord) -> Result<MembershipCert, String> {
            if let Some(id) = proof
                .identities()
                .iter()
                .find(|id| !self.nodes.contains_key(&id.to_string()))
            {
                return Err(format!(
                    "{id} is not registered in network {}",
                    self.network
                ));
            }
            let now = now_secs();
            let payload = CertPayload {
                version: CERT_VERSION,
                network: self.network.clone(),
                endpoint_id: *proof.endpoint_id().as_bytes(),
                identities: proof.identities().to_vec(),
                issued_at: now,
                expires_at: now + self.cert_lifetime_s,
            };
            Ok(MembershipCert::issue(
                payload,
                &self.key().expect("checked"),
            ))
        }

        fn is_admin(&self, token: &str) -> bool {
            hash(token) == self.admin_token_sha256
        }

        fn account_of(&self, token: &str) -> Option<String> {
            let h = hash(token);
            self.accounts
                .iter()
                .find(|(_, a)| a.token_sha256 == h)
                .map(|(n, _)| n.clone())
        }
    }

    #[cfg(unix)]
    fn write_private(path: &FsPath, data: &[u8]) -> std::io::Result<()> {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)?;
        f.sync_all()
    }

    #[cfg(not(unix))]
    fn write_private(path: &FsPath, data: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, data)
    }

    /// Minimal error type, so the library does not depend on `anyhow`.
    pub mod anyhow_lite {
        /// An error message.
        #[derive(Debug, Clone, thiserror::Error)]
        #[error("{0}")]
        pub struct Error(pub String);
        /// Result with [`Error`].
        pub type Result<T> = std::result::Result<T, Error>;
        /// Builds an [`Error`].
        pub fn msg(s: impl Into<String>) -> Error {
            Error(s.into())
        }
    }

    struct Shared {
        state: Mutex<RegistryState>,
        path: PathBuf,
    }

    /// Builds the registry's HTTP routes on the state file at `path`.
    pub fn router(path: PathBuf) -> anyhow_lite::Result<Router> {
        let state = RegistryState::load(&path)?;
        let shared = Arc::new(Shared {
            state: Mutex::new(state),
            path,
        });
        Ok(Router::new()
            .route("/v1/info", get(info))
            .route("/v1/certify", post(certify))
            .route("/v1/accounts", get(list_accounts))
            .route(
                "/v1/accounts/{name}",
                post(add_account).delete(remove_account),
            )
            .route("/v1/nodes", get(list_nodes))
            .route("/v1/nodes/{node_id}", put(add_node).delete(remove_node))
            .route("/health", get(|| async { "ok" }))
            .layer(DefaultBodyLimit::max(MAX_RECORD_SIZE))
            .with_state(shared))
    }

    /// Serves the registry on `listener` until the future is dropped.
    pub async fn serve(listener: tokio::net::TcpListener, path: PathBuf) -> std::io::Result<()> {
        let app = router(path).map_err(|e| std::io::Error::other(e.0))?;
        axum::serve(listener, app).await
    }

    fn bearer(h: &HeaderMap) -> Option<&str> {
        let v = h.get(header::AUTHORIZATION)?.to_str().ok()?;
        let (scheme, token) = v.split_once(' ')?;
        scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
    }

    fn text(status: StatusCode, body: impl Into<String>) -> Response {
        (
            status,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            body.into(),
        )
            .into_response()
    }

    /// A refused request: status and message.
    type Fail = (StatusCode, &'static str);

    const UNAUTHORIZED: Fail = (StatusCode::UNAUTHORIZED, "missing or invalid token\n");

    fn unauthorized() -> Response {
        text(UNAUTHORIZED.0, UNAUTHORIZED.1)
    }

    /// Applies `f` to the state and saves it; on a save failure the change is
    /// rolled back.
    fn mutate<T>(
        s: &Shared,
        f: impl FnOnce(&mut RegistryState) -> Result<T, Fail>,
    ) -> Result<T, Fail> {
        let mut st = s.state.lock().expect("poisoned");
        let before = st.clone();
        let out = f(&mut st)?;
        if let Err(e) = st.save(&s.path) {
            *st = before;
            tracing::error!("saving registry state failed: {e}");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "saving state failed\n"));
        }
        Ok(out)
    }

    async fn info(State(s): State<Arc<Shared>>) -> Response {
        let st = s.state.lock().expect("poisoned");
        let key = st.key().expect("checked").public();
        text(
            StatusCode::OK,
            format!(
                "network={}\npublic_key={}\ncert_lifetime_s={}\n",
                st.network,
                hex::encode(key.as_bytes()),
                st.cert_lifetime_s
            ),
        )
    }

    async fn certify(State(s): State<Arc<Shared>>, body: Bytes) -> Response {
        let proof = match VerifiedRecord::verify(&body) {
            Ok(r) => r,
            Err(e) => return text(StatusCode::BAD_REQUEST, format!("invalid record: {e}\n")),
        };
        let st = s.state.lock().expect("poisoned");
        match st.certify(&proof) {
            Ok(cert) => {
                tracing::debug!(ids = ?proof.identities(), "certificate issued");
                text(StatusCode::OK, hex::encode(cert.to_bytes()))
            }
            Err(e) => {
                tracing::info!(ids = ?proof.identities(), "certificate refused: {e}");
                text(StatusCode::FORBIDDEN, format!("{e}\n"))
            }
        }
    }

    async fn add_account(
        State(s): State<Arc<Shared>>,
        Path(name): Path<String>,
        h: HeaderMap,
    ) -> Response {
        let token = bearer(&h).unwrap_or_default().to_string();
        let res = mutate(&s, |st| {
            if !st.is_admin(&token) {
                return Err(UNAUTHORIZED);
            }
            if !valid_name(&name) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "account names: 1-64 of a-z A-Z 0-9 - _ .\n",
                ));
            }
            if st.accounts.contains_key(&name) {
                return Err((StatusCode::CONFLICT, "account exists\n"));
            }
            let t = new_token();
            st.accounts.insert(
                name.clone(),
                Account {
                    token_sha256: hash(&t),
                    created_at: now_secs(),
                },
            );
            Ok(t)
        });
        match res {
            Ok(t) => {
                tracing::info!(account = %name, "account created");
                text(StatusCode::OK, format!("{t}\n"))
            }
            Err((status, msg)) => text(status, msg),
        }
    }

    async fn remove_account(
        State(s): State<Arc<Shared>>,
        Path(name): Path<String>,
        h: HeaderMap,
    ) -> Response {
        let token = bearer(&h).unwrap_or_default().to_string();
        let res = mutate(&s, |st| {
            if !st.is_admin(&token) {
                return Err(UNAUTHORIZED);
            }
            if st.accounts.remove(&name).is_none() {
                return Err((StatusCode::NOT_FOUND, "no such account\n"));
            }
            st.nodes.retain(|_, a| *a != name);
            Ok(())
        });
        match res {
            Ok(()) => {
                tracing::info!(account = %name, "account removed with its nodes");
                StatusCode::NO_CONTENT.into_response()
            }
            Err((status, msg)) => text(status, msg),
        }
    }

    async fn list_accounts(State(s): State<Arc<Shared>>, h: HeaderMap) -> Response {
        let st = s.state.lock().expect("poisoned");
        if !bearer(&h).is_some_and(|t| st.is_admin(t)) {
            return unauthorized();
        }
        let mut out = String::new();
        for name in st.accounts.keys() {
            let n = st.nodes.values().filter(|a| *a == name).count();
            out.push_str(&format!("{name} {n}\n"));
        }
        text(StatusCode::OK, out)
    }

    async fn add_node(
        State(s): State<Arc<Shared>>,
        Path(node_id): Path<String>,
        h: HeaderMap,
    ) -> Response {
        let Ok(id) = node_id.parse::<NodeId>() else {
            return text(StatusCode::BAD_REQUEST, "invalid node id\n");
        };
        let token = bearer(&h).unwrap_or_default().to_string();
        let res = mutate(&s, |st| {
            let Some(account) = st.account_of(&token) else {
                return Err(UNAUTHORIZED);
            };
            match st.nodes.get(&id.to_string()) {
                Some(owner) if *owner != account => {
                    Err((StatusCode::CONFLICT, "registered by another account\n"))
                }
                _ => {
                    st.nodes.insert(id.to_string(), account.clone());
                    Ok(account)
                }
            }
        });
        match res {
            Ok(account) => {
                tracing::info!(%id, %account, "node registered");
                StatusCode::NO_CONTENT.into_response()
            }
            Err((status, msg)) => text(status, msg),
        }
    }

    async fn remove_node(
        State(s): State<Arc<Shared>>,
        Path(node_id): Path<String>,
        h: HeaderMap,
    ) -> Response {
        let Ok(id) = node_id.parse::<NodeId>() else {
            return text(StatusCode::BAD_REQUEST, "invalid node id\n");
        };
        let token = bearer(&h).unwrap_or_default().to_string();
        let res = mutate(&s, |st| {
            let Some(account) = st.account_of(&token) else {
                return Err(UNAUTHORIZED);
            };
            if st.nodes.get(&id.to_string()) != Some(&account) {
                return Err((StatusCode::NOT_FOUND, "not one of your nodes\n"));
            }
            st.nodes.remove(&id.to_string());
            Ok(account)
        });
        match res {
            Ok(account) => {
                tracing::info!(%id, %account, "node removed");
                StatusCode::NO_CONTENT.into_response()
            }
            Err((status, msg)) => text(status, msg),
        }
    }

    async fn list_nodes(State(s): State<Arc<Shared>>, h: HeaderMap) -> Response {
        let st = s.state.lock().expect("poisoned");
        let Some(token) = bearer(&h) else {
            return unauthorized();
        };
        let mut out = String::new();
        if st.is_admin(token) {
            for (id, a) in &st.nodes {
                out.push_str(&format!("{id} {a}\n"));
            }
        } else if let Some(account) = st.account_of(token) {
            for (id, a) in &st.nodes {
                if *a == account {
                    out.push_str(&format!("{id}\n"));
                }
            }
        } else {
            return unauthorized();
        }
        text(StatusCode::OK, out)
    }
}
