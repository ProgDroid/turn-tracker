//! Strongly-typed identifiers for rooms and players.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

/// Opaque, server-generated identifier for a player within a room.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub String);

/// Short, human-shareable room code (e.g. "K7M-Q2P").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomCode(pub String);

/// Unambiguous alphabet: no 0/O, 1/I/L to ease verbal sharing.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const CODE_LEN: usize = 6;

impl RoomCode {
    /// Generate a random 6-character code from the unambiguous alphabet.
    #[must_use]
    pub fn generate() -> Self {
        // rand 0.10 API: standalone `random_range` over the thread-local RNG.
        let code: String = (0..CODE_LEN)
            .map(|_| {
                let idx = rand::random_range(0..CODE_ALPHABET.len());
                CODE_ALPHABET[idx] as char
            })
            .collect();
        Self(code)
    }
}

/// Generate a 256-bit opaque token, hex-encoded (used for player/host auth).
#[must_use]
pub fn generate_token() -> String {
    // rand 0.10 API: standalone `random()` fills the array from StandardUniform.
    // fold+write! avoids per-byte allocations (clippy::format_collect).
    let bytes: [u8; 32] = rand::random();
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_id_equality_compares_inner_string() {
        assert_eq!(PlayerId("a".into()), PlayerId("a".into()));
        assert_ne!(PlayerId("a".into()), PlayerId("b".into()));
    }

    #[test]
    fn test_generate_code_has_correct_length() {
        assert_eq!(RoomCode::generate().0.len(), CODE_LEN);
    }

    #[test]
    fn test_generate_code_uses_only_unambiguous_alphabet() {
        for _ in 0..1000 {
            let code = RoomCode::generate();
            for ch in code.0.bytes() {
                assert!(
                    CODE_ALPHABET.contains(&ch),
                    "char {} not in alphabet",
                    ch as char
                );
            }
        }
    }

    #[test]
    fn test_generate_code_excludes_ambiguous_chars() {
        for _ in 0..1000 {
            let code = RoomCode::generate();
            for bad in ['0', 'O', '1', 'I', 'L'] {
                assert!(!code.0.contains(bad), "code {} contains {}", code.0, bad);
            }
        }
    }

    #[test]
    fn test_generate_token_is_64_hex_chars() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token_is_unique_across_calls() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }
}
