//! Optional shared-secret gate on room creation, used during a closed beta.
//!
//! Mirrors the `ALLOWED_ORIGINS` pattern: unset means the gate is OFF, so local
//! development and the Playwright E2E suite need no configuration. When set,
//! `POST /api/rooms` requires a matching `X-Create-Token` header.
//!
//! Joining a room is deliberately NOT gated — guests arrive by QR deep link and
//! must see no extra friction.
//!
//! TRUST BOUNDARY: this is a shared secret distributed to a handful of hosts,
//! not an authentication system. It is transmitted over TLS and guarded by the
//! per-IP rate limiter on the same endpoint, which runs first. A constant-time
//! comparison is not warranted at this scale.

/// Read the configured creation token, treating unset or empty as "no gate".
#[must_use]
pub fn create_token_from_env() -> Option<String> {
    std::env::var("TT_CREATE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// Whether a room-creation request may proceed.
///
/// `expected` of `None` or `Some("")` means the gate is disabled and everything
/// is allowed.
#[must_use]
pub fn create_allowed(provided: Option<&str>, expected: Option<&str>) -> bool {
    match expected {
        None | Some("") => true,
        Some(want) => provided == Some(want),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_is_open_when_no_token_is_configured() {
        assert!(create_allowed(None, None));
        assert!(
            create_allowed(Some("anything"), None),
            "a stray header must not break an ungated server"
        );
    }

    #[test]
    fn matching_token_is_allowed() {
        assert!(create_allowed(Some("s3cret"), Some("s3cret")));
    }

    #[test]
    fn missing_token_is_rejected_when_gated() {
        assert!(!create_allowed(None, Some("s3cret")));
    }

    #[test]
    fn wrong_token_is_rejected() {
        assert!(!create_allowed(Some("nope"), Some("s3cret")));
    }

    #[test]
    fn empty_configured_token_means_the_gate_is_off() {
        // Treat `TT_CREATE_TOKEN=` the same as unset, matching how
        // ALLOWED_ORIGINS handles an empty value.
        assert!(create_allowed(None, Some("")));
    }
}
