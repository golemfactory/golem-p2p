//! Telemetry reports of closed networks: who may send them.
//!
//! A node of a closed network can send a small report about itself (version,
//! uptime, connections, counters; see `docs/telemetry.md`) to the network's
//! telemetry collector every few seconds. Only members may report: the body
//! is signed by the node's default identity, and the node's membership
//! certificate goes with it. The collector checks both offline, against the
//! registry key, as peers check certificates in the handshake
//! ([`MembershipPolicy`]).
//!
//! On the wire a report is `POST <collector>/v1/report` with the report as the
//! body (at most [`MAX_REPORT_SIZE`] bytes) and two headers:
//!
//! * [`SIGNATURE_HEADER`]: the identity's signature over [`digest`] of the
//!   body, 65 bytes (yagna format) in hex;
//! * [`CERT_HEADER`]: the node's current membership certificate in hex.
//!
//! The content of the body, and freshness (timestamps, replays), are the
//! collector's business: this module only says who signed it and whether
//! that identity is a member now.

use std::time::SystemTime;

use iroh::EndpointId;
use sha2::{Digest, Sha256};

use crate::{
    NodeId, Signature, Signer,
    membership::{MembershipCert, MembershipError, MembershipPolicy},
    signer::SignError,
};

/// Domain separation for report signatures.
pub const DOMAIN: &[u8] = b"golem-p2p/telemetry/v1\0";

/// Largest report body accepted.
pub const MAX_REPORT_SIZE: usize = 64 * 1024;

/// HTTP header with the signature (hex).
pub const SIGNATURE_HEADER: &str = "x-golem-p2p-signature";

/// HTTP header with the membership certificate (hex).
pub const CERT_HEADER: &str = "x-golem-p2p-membership";

/// The digest an identity signs for a report body.
pub fn digest(body: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(body);
    h.finalize().into()
}

/// Signs a report body with identity `id`. The signature is checked before
/// it is returned, as for records.
pub async fn sign(signer: &dyn Signer, id: NodeId, body: &[u8]) -> Result<Signature, SignError> {
    let d = digest(body);
    let sig = signer.sign(id, d).await?;
    if sig.recover_node_id(&d).ok() != Some(id) {
        return Err(SignError::Backend(
            "signer returned a signature of another key".into(),
        ));
    }
    Ok(sig)
}

/// Why a report was not accepted as coming from a member.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    /// The body is larger than [`MAX_REPORT_SIZE`].
    #[error("report larger than {MAX_REPORT_SIZE} bytes")]
    TooLarge,
    /// The signature is malformed.
    #[error("invalid report signature")]
    Signature,
    /// The certificate is invalid, not trusted, for another network or
    /// expired.
    #[error(transparent)]
    Membership(#[from] MembershipError),
    /// The identity that signed is not one the certificate covers.
    #[error("{0} is not covered by the membership certificate")]
    NotCovered(NodeId),
}

/// The member that sent a report.
#[derive(Debug, Clone)]
pub struct ReportSender {
    /// The identity that signed the body.
    pub node_id: NodeId,
    /// Its membership certificate, checked against the policy.
    pub cert: MembershipCert,
}

/// Checks that `body` was signed (`signature`, raw bytes) by an identity that
/// the membership certificate `cert` (encoded) covers, and that the
/// certificate passes `policy` at `now`.
pub fn verify(
    policy: &MembershipPolicy,
    body: &[u8],
    signature: &[u8],
    cert: &[u8],
    now: SystemTime,
) -> Result<ReportSender, ReportError> {
    if body.len() > MAX_REPORT_SIZE {
        return Err(ReportError::TooLarge);
    }
    let sig = Signature::from_slice(signature).map_err(|_| ReportError::Signature)?;
    let node_id = sig
        .recover_node_id(&digest(body))
        .map_err(|_| ReportError::Signature)?;
    let cert = MembershipCert::decode(cert)?;
    let endpoint = EndpointId::from_bytes(&cert.payload().endpoint_id)
        .map_err(|_| MembershipError::Malformed("endpoint id"))?;
    policy.check_cert(&cert, endpoint, now)?;
    if !cert.covers(&[node_id]) {
        return Err(ReportError::NotCovered(node_id));
    }
    Ok(ReportSender { node_id, cert })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use iroh::SecretKey;

    use super::*;
    use crate::{
        InMemorySigner,
        membership::{CERT_VERSION, CertPayload},
    };

    fn cert(
        key: &SecretKey,
        network: &str,
        ids: Vec<NodeId>,
        issued_at: u64,
        lifetime: u64,
    ) -> Vec<u8> {
        MembershipCert::issue(
            CertPayload {
                version: CERT_VERSION,
                network: network.into(),
                endpoint_id: *SecretKey::generate().public().as_bytes(),
                identities: ids,
                issued_at,
                expires_at: issued_at + lifetime,
            },
            key,
        )
        .to_bytes()
        .to_vec()
    }

    #[tokio::test]
    async fn reports_of_members_only() {
        let registry = SecretKey::generate();
        let policy = MembershipPolicy::new("net", registry.public());
        let signer = InMemorySigner::new();
        let me = signer.generate();
        let other = signer.generate();
        let now = SystemTime::now();
        let t = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let body = br#"{"v":1}"#;
        let sig = sign(&signer, me, body).await.unwrap();
        let sig = sig.as_bytes().as_slice();

        // Valid.
        let good = cert(&registry, "net", vec![me], t - 10, 3600);
        let s = verify(&policy, body, sig, &good, now).unwrap();
        assert_eq!(s.node_id, me);

        // A changed body: the signature recovers another identity.
        let err = verify(&policy, br#"{"v":2}"#, sig, &good, now).unwrap_err();
        assert!(
            matches!(err, ReportError::NotCovered(id) if id != me),
            "{err}"
        );
        // Garbage signatures.
        assert_eq!(
            verify(&policy, body, &sig[..64], &good, now).unwrap_err(),
            ReportError::Signature
        );
        assert_eq!(
            verify(&policy, body, &[7u8; 65], &good, now).unwrap_err(),
            ReportError::Signature
        );
        // Another network, another registry, expired, someone else's
        // certificate, a broken certificate.
        let wrong_net = cert(&registry, "other", vec![me], t - 10, 3600);
        assert!(matches!(
            verify(&policy, body, sig, &wrong_net, now).unwrap_err(),
            ReportError::Membership(MembershipError::WrongNetwork(_))
        ));
        let untrusted = cert(&SecretKey::generate(), "net", vec![me], t - 10, 3600);
        assert_eq!(
            verify(&policy, body, sig, &untrusted, now).unwrap_err(),
            ReportError::Membership(MembershipError::UntrustedIssuer)
        );
        let expired = cert(&registry, "net", vec![me], t - 7200, 600);
        assert_eq!(
            verify(&policy, body, sig, &expired, now).unwrap_err(),
            ReportError::Membership(MembershipError::Expired)
        );
        let others = cert(&registry, "net", vec![other], t - 10, 3600);
        assert_eq!(
            verify(&policy, body, sig, &others, now).unwrap_err(),
            ReportError::NotCovered(me)
        );
        assert!(matches!(
            verify(&policy, body, sig, b"junk", now).unwrap_err(),
            ReportError::Membership(MembershipError::Malformed(_))
        ));
        // Too large, before any crypto.
        let big = vec![b' '; MAX_REPORT_SIZE + 1];
        assert_eq!(
            verify(&policy, &big, sig, &good, now).unwrap_err(),
            ReportError::TooLarge
        );
        // Within the clock tolerance of an expiry, still valid.
        let later = now + Duration::from_secs(3600 + 60);
        assert!(verify(&policy, body, sig, &good, later).is_ok());
    }
}
