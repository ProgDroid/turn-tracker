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
#[derive(Debug, Clone)]
pub struct RealIpKeyExtractor {
    /// Name of a header the platform guarantees is unforgeable, e.g.
    /// `Fly-Client-IP`. `None` means fall back to `realip_remote_addr()`.
    trusted_header: Option<String>,
}

impl RealIpKeyExtractor {
    /// Build an extractor.
    ///
    /// Pass `Some(name)` ONLY where the platform sets that header from the real
    /// connection and strips any client-supplied value. Passing a header the
    /// platform does not control hands clients the keying value — behind a
    /// generic reverse proxy, an unknown header is forwarded verbatim.
    #[must_use]
    pub const fn new(trusted_header: Option<String>) -> Self {
        Self { trusted_header }
    }

    /// Read the trusted-header name from `TT_CLIENT_IP_HEADER`, treating unset
    /// or empty as "not configured" — the same degrade-to-default shape as
    /// `ALLOWED_ORIGINS` and `TT_CREATE_TOKEN`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("TT_CLIENT_IP_HEADER")
                .ok()
                .filter(|h| !h.is_empty()),
        )
    }
}

impl KeyExtractor for RealIpKeyExtractor {
    type Key = String;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        // When a trusted header is configured it is the ONLY source. Falling
        // back to XFF when it is absent would reopen the spoofing hole this
        // exists to close, so an absent value fails closed into the shared
        // "unknown" bucket instead.
        if let Some(name) = &self.trusted_header {
            return Ok(req
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map_or_else(|| "unknown".to_owned(), str::to_owned));
        }
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
        .key_extractor(RealIpKeyExtractor::from_env())
        .finish()
        .expect("non-zero rate-limit params yield a valid governor config")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test]
    fn trusted_header_is_preferred_when_configured() {
        // On Fly the platform sets Fly-Client-IP from the real TCP connection
        // and strips any client-supplied value, so it is authoritative there.
        let req = TestRequest::default()
            .insert_header(("fly-client-ip", "203.0.113.7"))
            .insert_header(("x-forwarded-for", "198.51.100.1"))
            .to_srv_request();
        let key = RealIpKeyExtractor::new(Some("fly-client-ip".into()))
            .extract(&req)
            .unwrap();
        assert_eq!(
            key, "203.0.113.7",
            "the configured trusted header must win over XFF"
        );
    }

    #[test]
    fn a_spoofed_xff_cannot_override_the_trusted_header() {
        // The whole point: on Fly, XFF is client-spoofable. A client rotating
        // it must not be able to pick its own rate-limit bucket.
        let req = TestRequest::default()
            .insert_header(("fly-client-ip", "203.0.113.7"))
            .insert_header(("x-forwarded-for", "1.2.3.4, 5.6.7.8"))
            .to_srv_request();
        let key = RealIpKeyExtractor::new(Some("fly-client-ip".into()))
            .extract(&req)
            .unwrap();
        assert_eq!(key, "203.0.113.7");
    }

    #[test]
    fn a_configured_header_that_is_absent_falls_back_to_the_shared_bucket() {
        // Fail CLOSED. If the trusted header is configured but missing, the
        // request is un-attributable — bucket it with every other
        // un-attributable request rather than trusting a spoofable XFF.
        let req = TestRequest::default()
            .insert_header(("x-forwarded-for", "1.2.3.4"))
            .to_srv_request();
        let key = RealIpKeyExtractor::new(Some("fly-client-ip".into()))
            .extract(&req)
            .unwrap();
        assert_eq!(
            key, "unknown",
            "a missing trusted header must NOT silently fall back to spoofable XFF"
        );
    }

    #[test]
    fn unconfigured_extractor_keeps_the_reverse_proxy_xff_behaviour() {
        // The VPS path: Caddy is configured to OVERWRITE XFF, so the leftmost
        // entry is trustworthy there. Unset = today's behaviour, unchanged.
        let req = TestRequest::default()
            .insert_header(("x-forwarded-for", "203.0.113.7, 70.41.3.18"))
            .to_srv_request();
        let key = RealIpKeyExtractor::new(None).extract(&req).unwrap();
        assert_eq!(key, "203.0.113.7");
    }

    #[test]
    fn unconfigured_extractor_ignores_a_client_supplied_platform_header() {
        // Guards the trap this design exists to avoid: behind Caddy, a client
        // can send Fly-Client-IP and the proxy passes it straight through.
        // Without explicit configuration it must be ignored entirely.
        let req = TestRequest::default()
            .insert_header(("fly-client-ip", "6.6.6.6"))
            .insert_header(("x-forwarded-for", "203.0.113.7"))
            .to_srv_request();
        let key = RealIpKeyExtractor::new(None).extract(&req).unwrap();
        assert_eq!(
            key, "203.0.113.7",
            "an unconfigured platform header must never be trusted"
        );
    }

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
        let key = RealIpKeyExtractor::new(None).extract(&req).unwrap();
        assert_eq!(
            key, "203.0.113.7",
            "must key on the leftmost (client) XFF entry"
        );
    }

    #[test]
    fn extractor_falls_back_to_shared_key_when_unattributable() {
        let req = TestRequest::default().to_srv_request();
        let key = RealIpKeyExtractor::new(None).extract(&req).unwrap();
        assert_eq!(key, "unknown");
    }
}
