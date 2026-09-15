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

/// The resolved creation gate, injected as shared application state.
///
/// Resolving the token once at startup keeps `std::env::var` out of the
/// per-request path and, more importantly, makes the gate configurable in a
/// test without `std::env::set_var` — which is `unsafe` under edition 2024 and
/// races across concurrently running tests. `None` means the gate is off.
#[derive(Debug, Clone, Default)]
pub struct CreateToken(Option<String>);

impl CreateToken {
    /// Wrap an already-resolved token.
    #[must_use]
    pub const fn new(token: Option<String>) -> Self {
        Self(token)
    }

    /// Resolve the gate from the environment. Call this once, at startup.
    #[must_use]
    pub fn from_env() -> Self {
        Self(create_token_from_env())
    }

    /// The token a request must present, or `None` when the gate is off.
    #[must_use]
    pub fn expected(&self) -> Option<&str> {
        self.0.as_deref()
    }
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
    fn create_token_wraps_and_exposes_the_expected_value() {
        assert_eq!(CreateToken::new(None).expected(), None);
        assert_eq!(
            CreateToken::new(Some("s3cret".into())).expected(),
            Some("s3cret")
        );
        assert_eq!(
            CreateToken::default().expected(),
            None,
            "the default gate is off"
        );
    }

    #[test]
    fn empty_configured_token_means_the_gate_is_off() {
        // Treat `TT_CREATE_TOKEN=` the same as unset, matching how
        // ALLOWED_ORIGINS handles an empty value.
        assert!(create_allowed(None, Some("")));
    }
}
