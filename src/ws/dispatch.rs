//! Translate an authenticated `ClientMessage` into a room mutation plus the
//! resulting outbound messages. Pure orchestration over the domain layer.

use std::time::Instant;

use crate::domain::ids::PlayerId;
use crate::domain::room::{Room, TurnError};
use crate::registry::Outbound;
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};

/// Apply `msg` from `actor` to `room`, returning the messages to broadcast.
/// `Join` is handled at the connection layer (it may create a player), so it is
/// treated as a no-op refresh here.
pub fn dispatch(
    room: &mut Room,
    actor: &PlayerId,
    msg: ClientMessage,
    now: Instant,
) -> Vec<Outbound> {
    let result: Result<Vec<Outbound>, TurnError> = match msg {
        ClientMessage::StartGame => room.start_game(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::EndTurn => room.end_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::ClaimTurn => room.claim_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::UndoTurn => room.undo_turn(actor, now).map(|()| state_broadcast(room)),
        ClientMessage::SkipPlayer { player_id } => room
            .skip_player(actor, &player_id, now)
            .map(|()| state_broadcast(room)),
        ClientMessage::RemovePlayer { player_id } => room
            .remove_player(actor, &player_id, now)
            .map(|()| state_broadcast(room)),
        ClientMessage::SetOrder { player_ids } => room
            .set_order(actor, &player_ids, now)
            .map(|()| state_broadcast(room)),
        ClientMessage::Nudge => room
            .nudge(actor, now)
            .map(|target| vec![Outbound::Player(target, ServerMessage::Nudged)]),
        ClientMessage::Join { .. } => Ok(state_broadcast(room)),
    };

    match result {
        Ok(out) => out,
        Err(e) => vec![Outbound::Player(actor.clone(), error_message(&e))],
    }
}

/// Build a full room-state broadcast to all connections.
fn state_broadcast(room: &Room) -> Vec<Outbound> {
    vec![Outbound::All(ServerMessage::RoomState {
        room: PublicRoom::from(room),
    })]
}

fn error_message(e: &TurnError) -> ServerMessage {
    let (code, message) = match e {
        TurnError::NotAuthorized => ("not_authorized", "You are not allowed to do that"),
        TurnError::NotFound => ("not_found", "Target not found"),
        TurnError::WrongState => ("wrong_state", "Action not valid in the current state"),
        TurnError::NotYourTurn => ("not_your_turn", "It is not your turn to claim"),
        TurnError::NudgeOnCooldown => ("nudge_cooldown", "Nudge is on cooldown"),
    };
    ServerMessage::Error {
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::RoomCode;

    fn active_room() -> (Room, PlayerId, PlayerId) {
        let now = Instant::now();
        let (mut room, host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        let (bob, _bt) = room.add_player("Bob".into(), now);
        room.start_game(&host, now).unwrap();
        (room, host, bob)
    }

    #[test]
    fn test_dispatch_end_turn_produces_room_state_broadcast() {
        let (mut room, host, _bob) = active_room();
        let out = dispatch(&mut room, &host, ClientMessage::EndTurn, Instant::now());
        assert!(matches!(
            out.as_slice(),
            [Outbound::All(ServerMessage::RoomState { .. })]
        ));
    }

    #[test]
    fn test_dispatch_unauthorized_produces_targeted_error() {
        let (mut room, _host, bob) = active_room();
        let out = dispatch(&mut room, &bob, ClientMessage::EndTurn, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Error { code, .. })] => {
                assert_eq!(target, &bob);
                assert_eq!(code, "not_authorized");
            }
            _ => panic!("expected targeted error, got {out:?}"),
        }
    }

    #[test]
    fn test_dispatch_nudge_targets_current_player() {
        let (mut room, host, bob) = active_room(); // host is current
        let out = dispatch(&mut room, &bob, ClientMessage::Nudge, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Nudged)] => assert_eq!(target, &host),
            _ => panic!("expected nudge to host, got {out:?}"),
        }
    }
}
