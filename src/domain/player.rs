//! A participant in a room.

use crate::domain::ids::PlayerId;

/// Maximum accepted length (in characters) for a player/host name after trim.
pub const MAX_NAME_LEN: usize = 40;

/// Trim and validate a candidate player/host name.
///
/// Returns the trimmed name on success.
///
/// # Errors
/// `NameError::Empty` if the trimmed name is empty; `NameError::TooLong` if it
/// exceeds [`MAX_NAME_LEN`] characters.
pub fn validate_name(raw: &str) -> Result<String, NameError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(NameError::Empty);
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(NameError::TooLong);
    }
    Ok(trimmed.to_owned())
}

/// Reasons a candidate name is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
}

/// Identifies WHICH live connection currently owns a player.
///
/// A player can be targeted by more than one socket at a time: a phone whose
/// flow was silently dropped leaves a half-open connection behind while the
/// same player re-attaches by token on a fresh one. Each attach is handed a
/// distinct epoch, so a departing connection can tell whether it is still the
/// owner before it clears `connected`.
///
/// [`ConnEpoch::NONE`] is the value a player carries before any socket has
/// attached (and after a snapshot reload). It is never issued, so it can never
/// match a live connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnEpoch(pub u64);

impl ConnEpoch {
    /// The epoch of a player no connection has ever attached to.
    pub const NONE: Self = Self(0);
}

/// A single player in a room. `token` is a secret session credential and is
/// NEVER serialized to other clients (see `wire::PublicPlayer`).
#[derive(Debug, Clone)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub token: String,
    pub is_host: bool,
    pub connected: bool,
    /// When true, this player is skipped exactly once, then the flag clears.
    pub skip_next: bool,
    /// Which connection last attached to this player. Process-local liveness
    /// bookkeeping, deliberately NOT persisted (see `persist::snapshot`).
    pub conn_epoch: ConnEpoch,
}

impl Player {
    #[must_use]
    pub const fn new(id: PlayerId, name: String, token: String, is_host: bool) -> Self {
        Self {
            id,
            name,
            token,
            is_host,
            connected: true,
            skip_next: false,
            conn_epoch: ConnEpoch::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_new_defaults_connected_true_skip_false() {
        let p = Player::new(PlayerId("p1".into()), "Alice".into(), "tok".into(), true);
        assert!(p.connected);
        assert!(!p.skip_next);
        assert!(p.is_host);
        assert_eq!(p.name, "Alice");
        assert_eq!(
            p.conn_epoch,
            ConnEpoch::NONE,
            "a player owns no connection until one attaches"
        );
    }

    #[test]
    fn test_validate_name_trims_and_accepts() {
        assert_eq!(validate_name("  Alice  ").unwrap(), "Alice");
    }

    #[test]
    fn test_validate_name_rejects_empty_and_whitespace_only() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("   "), Err(NameError::Empty));
    }

    #[test]
    fn test_validate_name_accepts_exactly_max_len() {
        let name = "a".repeat(MAX_NAME_LEN);
        assert_eq!(validate_name(&name).unwrap(), name);
    }

    #[test]
    fn test_validate_name_rejects_one_over_max_len() {
        let name = "a".repeat(MAX_NAME_LEN + 1);
        assert_eq!(validate_name(&name), Err(NameError::TooLong));
    }

    #[test]
    fn test_validate_name_counts_chars_not_bytes() {
        // 40 multi-byte chars: accepted (char count, not byte length).
        let name = "é".repeat(MAX_NAME_LEN);
        assert_eq!(validate_name(&name).unwrap(), name);
        let too_long = "é".repeat(MAX_NAME_LEN + 1);
        assert_eq!(validate_name(&too_long), Err(NameError::TooLong));
    }
}
