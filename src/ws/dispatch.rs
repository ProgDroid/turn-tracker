//! Translate an authenticated `ClientMessage` into a room mutation plus the
//! resulting outbound messages. Pure orchestration over the domain layer.

use std::time::Instant;

use crate::domain::ids::PlayerId;
use crate::domain::room::{Room, TurnError};
use crate::registry::Outbound;
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};

/// Apply `msg` from `actor` to `room`, returning `(broadcasts, dirty)`.
///
/// `dirty` is true iff persisted state changed (so the registry should snapshot).
/// `Join` is handled at the connection layer (it may create a player), so it is
/// treated as a no-op refresh here. Nudges and any rejected action are NOT dirty:
/// nudge cooldowns aren't persisted, and a rejected action mutated nothing.
pub fn dispatch(
    room: &mut Room,
    actor: &PlayerId,
    msg: ClientMessage,
    now: Instant,
) -> (Vec<Outbound>, bool) {
    // `mutates` is the action's intent; it only counts when the result is Ok
    // (a rejected action changed nothing). Nudge/Join refresh don't persist.
    let (result, mutates): (Result<Vec<Outbound>, TurnError>, bool) = match msg {
        ClientMessage::StartGame => (
            room.start_game(actor, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::EndTurn => (
            room.end_turn(actor, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::ClaimTurn => (
            room.claim_turn(actor, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::UndoTurn => (
            room.undo_turn(actor, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::SkipPlayer { player_id } => (
            room.skip_player(actor, &player_id, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::RemovePlayer { player_id } => (
            room.remove_player(actor, &player_id, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::SetOrder { player_ids } => (
            room.set_order(actor, &player_ids, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::SetLocked { locked } => (
            room.set_locked(actor, locked, now)
                .map(|()| state_broadcast(room, now)),
            true,
        ),
        ClientMessage::Nudge => (
            room.nudge(actor, now)
                .map(|target| vec![Outbound::Player(target, ServerMessage::Nudged)]),
            false,
        ),
        ClientMessage::Join { .. } => (Ok(state_broadcast(room, now)), false),
    };

    match result {
        Ok(out) => (out, mutates),
        Err(e) => (
            vec![Outbound::Player(actor.clone(), error_message(&e))],
            false,
        ),
    }
}

/// Build a full room-state broadcast to all connections, as of `now` — or,
/// when nobody is left, the closure announcement instead.
///
/// Centralised here so every mutation that can empty a room gets this for free.
/// A player-less `RoomState` must never reach a client: there is no turn to
/// show, and the UI would render an empty one. Dropping the room itself is the
/// connection layer's job, once this broadcast is out.
fn state_broadcast(room: &Room, now: Instant) -> Vec<Outbound> {
    if room.players.is_empty() {
        return vec![Outbound::All(ServerMessage::RoomClosed)];
    }
    vec![Outbound::All(ServerMessage::RoomState {
        room: PublicRoom::new(room, now),
    })]
}

fn error_message(e: &TurnError) -> ServerMessage {
    let (code, message) = match e {
        TurnError::NotAuthorized => ("not_authorized", "You are not allowed to do that"),
        TurnError::NotFound => ("not_found", "Target not found"),
        TurnError::WrongState => ("wrong_state", "Action not valid in the current state"),
        TurnError::NotYourTurn => ("not_your_turn", "It is not your turn to claim"),
        TurnError::NudgeOnCooldown => ("nudge_cooldown", "Nudge is on cooldown"),
        TurnError::RoomFull => ("room_full", "Room is full"),
        TurnError::RoomLocked => ("room_locked", "Room is locked"),
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
        let (out, dirty) = dispatch(&mut room, &host, ClientMessage::EndTurn, Instant::now());
        assert!(matches!(
            out.as_slice(),
            [Outbound::All(ServerMessage::RoomState { .. })]
        ));
        assert!(dirty, "a successful turn advance must mark the room dirty");
    }

    #[test]
    fn test_dispatch_unauthorized_produces_targeted_error() {
        let (mut room, _host, bob) = active_room();
        let (out, dirty) = dispatch(&mut room, &bob, ClientMessage::EndTurn, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Error { code, .. })] => {
                assert_eq!(target, &bob);
                assert_eq!(code, "not_authorized");
            }
            _ => panic!("expected targeted error, got {out:?}"),
        }
        assert!(!dirty, "a rejected action must NOT mark the room dirty");
    }

    #[test]
    fn test_dispatch_set_locked_by_host_broadcasts_and_is_dirty() {
        let (mut room, host, _bob) = active_room();
        let (out, dirty) = dispatch(
            &mut room,
            &host,
            ClientMessage::SetLocked { locked: true },
            Instant::now(),
        );
        assert!(matches!(
            out.as_slice(),
            [Outbound::All(ServerMessage::RoomState { .. })]
        ));
        assert!(dirty, "toggling the lock changes persisted state");
        assert!(room.locked);
    }

    #[test]
    fn test_dispatch_set_locked_by_non_host_is_rejected() {
        let (mut room, _host, bob) = active_room();
        let (out, dirty) = dispatch(
            &mut room,
            &bob,
            ClientMessage::SetLocked { locked: true },
            Instant::now(),
        );
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Error { code, .. })] => {
                assert_eq!(target, &bob);
                assert_eq!(code, "not_authorized");
            }
            _ => panic!("expected targeted error, got {out:?}"),
        }
        assert!(!dirty);
        assert!(!room.locked, "a rejected toggle must not lock the room");
    }

    #[test]
    fn test_dispatch_nudge_targets_current_player() {
        let (mut room, host, bob) = active_room(); // host is current
        let (out, dirty) = dispatch(&mut room, &bob, ClientMessage::Nudge, Instant::now());
        match out.as_slice() {
            [Outbound::Player(target, ServerMessage::Nudged)] => assert_eq!(target, &host),
            _ => panic!("expected nudge to host, got {out:?}"),
        }
        assert!(
            !dirty,
            "nudge cooldowns are not persisted; must NOT mark dirty"
        );
    }

    #[test]
    fn test_removing_the_last_player_closes_the_room_instead_of_broadcasting_an_empty_one() {
        let now = Instant::now();
        let (mut room, host, _t) = Room::create(RoomCode("ABC123".into()), "Host".into(), now);
        room.attach_connection(&host);
        room.start_game(&host, now).unwrap();

        let (out, dirty) = dispatch(
            &mut room,
            &host,
            ClientMessage::RemovePlayer {
                player_id: host.clone(),
            },
            now,
        );

        assert!(room.players.is_empty());
        match out.as_slice() {
            [Outbound::All(ServerMessage::RoomClosed)] => {}
            other => panic!(
                "a room with nobody in it must announce that it is closed, never \
                 broadcast a player-less RoomState for clients to render: {other:?}"
            ),
        }
        assert!(dirty, "the room is gone; the snapshot must not keep it");
    }

    #[test]
    fn test_removing_a_player_who_is_not_the_last_still_broadcasts_state() {
        let (mut room, host, bob) = active_room();
        let (out, _dirty) = dispatch(
            &mut room,
            &host,
            ClientMessage::RemovePlayer { player_id: bob },
            Instant::now(),
        );
        assert!(matches!(
            out.as_slice(),
            [Outbound::All(ServerMessage::RoomState { .. })]
        ));
    }
}
