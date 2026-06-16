//! A participant in a room.

use crate::domain::ids::PlayerId;

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
    }
}
