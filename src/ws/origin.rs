//! WebSocket Origin allowlist.
//!
//! Configured via the `ALLOWED_ORIGINS` env var (comma-separated). When the
//! allowlist is empty, all origins are permitted (dev-safe default so
//! `npm run dev` / Playwright work without extra setup).

/// Name of the env var holding the comma-separated allowed Origin values.
pub const ALLOWED_ORIGINS_ENV: &str = "ALLOWED_ORIGINS";

/// Parse the `ALLOWED_ORIGINS` env var into a list of allowed Origin values.
/// Splits on commas, trims whitespace, and drops empty entries. An unset or
/// blank var yields an empty list (meaning "allow all").
#[must_use]
pub fn allowed_origins_from_env() -> Vec<String> {
    std::env::var(ALLOWED_ORIGINS_ENV)
        .ok()
        .map(|raw| parse_allowed(&raw))
        .unwrap_or_default()
}

/// Parse a raw comma-separated origin list into trimmed, non-empty entries.
#[must_use]
pub fn parse_allowed(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Decide whether a request's `Origin` header is acceptable.
///
/// - Empty `allowed` list ⇒ allow everything (dev default).
/// - Non-empty list ⇒ allow only when `origin` is present AND exactly matches
///   one of the allowed values; a missing Origin header is rejected.
#[must_use]
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    origin.is_some_and(|o| allowed.iter().any(|a| a == o))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allowlist_permits_everything() {
        assert!(origin_allowed(None, &[]));
        assert!(origin_allowed(Some("https://anything.example"), &[]));
    }

    #[test]
    fn missing_origin_rejected_when_allowlist_set() {
        let allowed = vec!["https://app.example".to_owned()];
        assert!(!origin_allowed(None, &allowed));
    }

    #[test]
    fn exact_match_allowed() {
        let allowed = vec![
            "https://app.example".to_owned(),
            "http://localhost:5173".to_owned(),
        ];
        assert!(origin_allowed(Some("https://app.example"), &allowed));
        assert!(origin_allowed(Some("http://localhost:5173"), &allowed));
    }

    #[test]
    fn non_match_rejected_when_allowlist_set() {
        let allowed = vec!["https://app.example".to_owned()];
        assert!(!origin_allowed(Some("https://evil.example"), &allowed));
        // Substring / prefix must NOT match.
        assert!(!origin_allowed(
            Some("https://app.example.evil.com"),
            &allowed
        ));
        assert!(!origin_allowed(Some("https://app.exampl"), &allowed));
    }

    #[test]
    fn match_is_case_sensitive_and_exact() {
        let allowed = vec!["https://App.Example".to_owned()];
        // Origin comparison is byte-exact; differing case does not match.
        assert!(!origin_allowed(Some("https://app.example"), &allowed));
        assert!(origin_allowed(Some("https://App.Example"), &allowed));
    }

    #[test]
    fn parse_allowed_trims_and_drops_empties() {
        let parsed = parse_allowed(" https://a.example , ,http://b.example, ");
        assert_eq!(parsed, vec!["https://a.example", "http://b.example"]);
    }

    #[test]
    fn parse_allowed_blank_yields_empty() {
        assert!(parse_allowed("").is_empty());
        assert!(parse_allowed("   ").is_empty());
        assert!(parse_allowed(",,").is_empty());
    }
}
