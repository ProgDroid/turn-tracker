//! On-the-wire JSON contract. `PublicRoom`/`PublicPlayer` deliberately omit
//! secret tokens so broadcasts never leak credentials.

use serde::{Deserialize, Serialize};

use crate::domain::ids::PlayerId;
use crate::domain::room::{Room, RoomState};

/// Messages a client sends to the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    Join {
        player_token: Option<String>,
        player_name: Option<String>,
    },
    StartGame,
    SetOrder {
        player_ids: Vec<PlayerId>,
    },
    EndTurn,
    ClaimTurn,
    UndoTurn,
    SkipPlayer {
        player_id: PlayerId,
    },
    RemovePlayer {
        player_id: PlayerId,
    },
    SetLocked {
        locked: bool,
    },
    Nudge,
}

/// Messages the server sends to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome { player_id: PlayerId, token: String },
    RoomState { room: PublicRoom },
    Nudged,
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicState {
    Lobby,
    Active,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicPlayer {
    pub id: PlayerId,
    pub name: String,
    pub is_host: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicRoom {
    pub code: String,
    pub state: PublicState,
    pub locked: bool,
    pub players: Vec<PublicPlayer>,
    pub current_player_id: Option<PlayerId>,
}

impl From<&Room> for PublicRoom {
    fn from(room: &Room) -> Self {
        Self {
            code: room.code.0.clone(),
            state: match room.state {
                RoomState::Lobby => PublicState::Lobby,
                RoomState::Active => PublicState::Active,
            },
            locked: room.locked,
            players: room
                .players
                .iter()
                .map(|p| PublicPlayer {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    is_host: p.is_host,
                    connected: p.connected,
                })
                .collect(),
            current_player_id: room.current_player_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use crate::domain::ids::RoomCode;

    #[test]
    fn test_client_message_deserializes_tagged_join() {
        let json = r#"{"type":"join","player_name":"Alice"}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Join {
                player_name,
                player_token,
            } => {
                assert_eq!(player_name, Some("Alice".into()));
                assert_eq!(player_token, None);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_public_room_omits_player_tokens() {
        let (room, _h, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), Instant::now());
        let public = PublicRoom::from(&room);
        let json = serde_json::to_string(&public).unwrap();
        assert!(
            !json.contains("token"),
            "PublicRoom leaked a token field: {json}"
        );
    }
}
