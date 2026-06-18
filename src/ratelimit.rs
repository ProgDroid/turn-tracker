//! Per-client rate limiting for abuse-prone endpoints (currently room creation).
//!
//! The app runs behind a TLS-terminating reverse proxy (Caddy/nginx), so the
//! socket peer is always the proxy. Keying on the peer IP would bucket the whole
//! internet together, so we key on the real client IP that the proxy forwards in
//! `Forwarded` / `X-Forwarded-For` / `X-Real-IP` (resolved by actix's
//! `realip_remote_addr`).
//!
//! TRUST BOUNDARY: `X-Forwarded-For` is client-spoofable. This keying is only
//! sound because the container is reachable *only* through the proxy (it is not
//! published directly to the internet) and the proxy overwrites the header. If
//! the app is ever exposed directly, switch to [`actix_governor::PeerIpKeyExtractor`].

use actix_governor::governor::middleware::NoOpMiddleware;
use actix_governor::{
    GovernorConfig, GovernorConfigBuilder, KeyExtractor, SimpleKeyExtractionError,
};
use actix_web::dev::ServiceRequest;

/// Room creations allowed per minute per client once the burst is spent.
const ROOM_CREATE_PER_MINUTE: u64 = 20;
/// Burst of room creations a client may make back-to-back before throttling.
const ROOM_CREATE_BURST: u32 = 10;

/// A [`KeyExtractor`] that keys on the real client IP behind a reverse proxy.
///
/// Falls back to a single shared `"unknown"` bucket when no address can be
/// determined, so an un-attributable request is still rate-limited rather than
/// erroring or bypassing the limiter.
#[derive(Debug, Clone, Copy)]
pub struct RealIpKeyExtractor;

impl KeyExtractor for RealIpKeyExtractor {
    type Key = String;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        let info = req.connection_info();
        Ok(info.realip_remote_addr().unwrap_or("unknown").to_owned())
    }
}

/// Build the shared governor config for `POST /api/rooms`.
///
/// The returned config holds an `Arc`'d limiter; clone it into each Actix worker
/// (via `Governor::new(&config)`) so all workers share one set of buckets.
///
/// # Panics
/// Never in practice: the builder only returns `None` for a zero period/burst,
/// and both constants here are non-zero.
pub fn room_create_config() -> GovernorConfig<RealIpKeyExtractor, NoOpMiddleware> {
    GovernorConfigBuilder::default()
        .requests_per_minute(ROOM_CREATE_PER_MINUTE)
        .burst_size(ROOM_CREATE_BURST)
        .key_extractor(RealIpKeyExtractor)
        .finish()
        .expect("non-zero rate-limit params yield a valid governor config")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn config_builds() {
        // Smoke test: the configured params produce a valid limiter.
        let _ = room_create_config();
    }

    #[test]
    fn extractor_prefers_forwarded_client_ip_over_peer() {
        let req = TestRequest::default()
            .insert_header(("x-forwarded-for", "203.0.113.7, 70.41.3.18"))
            .to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(
            key, "203.0.113.7",
            "must key on the leftmost (client) XFF entry"
        );
    }

    #[test]
    fn extractor_falls_back_to_shared_key_when_unattributable() {
        let req = TestRequest::default().to_srv_request();
        let key = RealIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "unknown");
    }
}
