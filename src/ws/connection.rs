//! actix-ws connection handler. Each connection runs a task that selects
//! between the incoming client stream and the room's broadcast receiver.

use std::sync::Arc;
use std::time::Instant;

use actix_web::{HttpRequest, HttpResponse, web};
use futures_util::{Stream, StreamExt as _};
use tokio::sync::broadcast;

use crate::domain::ids::PlayerId;
use crate::domain::player::{self, ConnEpoch, NameError};
use crate::registry::{Outbound, Registry};
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};
use crate::ws::dispatch::dispatch;
use crate::ws::heartbeat::{CLIENT_TIMEOUT, HEARTBEAT_INTERVAL, Heartbeat};
use crate::ws::origin::{allowed_origins_from_env, origin_allowed};

/// GET /ws/{code} — upgrade to a WebSocket bound to that room.
///
/// # Errors
/// Returns 404 if the room does not exist; otherwise upgrades the connection.
#[allow(clippy::future_not_send)]
// actix-web types (HttpRequest, Payload) are not Send; handler runs on actix's single-threaded runtime
pub async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    registry: web::Data<Arc<Registry>>,
) -> Result<HttpResponse, actix_web::Error> {
    let code = path.into_inner();

    // Origin allowlist (dev-safe: empty list allows all).
    let allowed = allowed_origins_from_env();
    let origin = req
        .headers()
        .get(actix_web::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok());
    if !origin_allowed(origin, &allowed) {
        log::warn!("ws upgrade rejected: origin not allowed (room={code})");
        return Ok(
            HttpResponse::Forbidden().json(serde_json::json!({ "error": "Origin not allowed" }))
        );
    }

    if !registry.contains(&code) {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({ "error": "Room not found" })));
    }
    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;
    let registry = registry.get_ref().clone();

    actix_web::rt::spawn(run_connection(registry, code, session, msg_stream));

    Ok(response)
}

/// Run a WebSocket connection: handle liveness, dispatch, and cleanup.
///
/// Manages the select loop over incoming messages, broadcast events, and heartbeat ticks.
async fn run_connection<S>(
    registry: Arc<Registry>,
    code: String,
    mut session: actix_ws::Session,
    mut msg_stream: S,
) where
    S: Stream<Item = Result<actix_ws::Message, actix_ws::ProtocolError>> + Unpin,
{
    let Ok(mut rx) = registry.subscribe(&code) else {
        return;
    };
    log::info!("ws connected: room={code}");
    // The player this socket speaks for, plus the epoch proving it is still
    // the connection that owns them (see `Room::attach_connection`).
    let mut me: Option<(PlayerId, ConnEpoch)> = None;
    let mut heartbeat = Heartbeat::new(CLIENT_TIMEOUT, Instant::now());
    let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // `interval` yields immediately on its first tick; consume it so the
    // first real ping happens one full interval from now.
    ticker.tick().await;

    loop {
        tokio::select! {
            incoming = msg_stream.next() => {
                match incoming {
                    // text is ByteString; .as_ref() gives &str
                    Some(Ok(actix_ws::Message::Text(text)))
                        if handle_text(&registry, &code, &mut me, &mut session, text.as_ref()).await.is_err() =>
                    {
                        break;
                    }
                    Some(Ok(actix_ws::Message::Close(_))) | None => break,
                    Some(Ok(actix_ws::Message::Ping(bytes))) => {
                        let _ = session.pong(&bytes).await;
                    }
                    Some(Ok(actix_ws::Message::Pong(_))) => {
                        heartbeat.record_pong(Instant::now());
                    }
                    _ => {}
                }
            }
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(Outbound::All(msg)) => {
                        if forward(&mut session, &msg).await.is_err() {
                            break;
                        }
                    }
                    Ok(Outbound::Player(target, msg)) => {
                        if me.as_ref().is_some_and(|(id, _)| id == &target)
                            && forward(&mut session, &msg).await.is_err()
                        {
                            break;
                        }
                    }
                    // The receiver fell behind and the channel dropped messages.
                    // Don't tear down the socket — resync by pushing the current
                    // authoritative room state so the client recovers.
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!("ws lagged: room={code} dropped {skipped} msg(s); resyncing");
                        if let Some(room) = registry.public_room(&code)
                            && forward(&mut session, &ServerMessage::RoomState { room })
                                .await
                                .is_err()
                        {
                            break;
                        }
                    }
                    // Channel closed (room swept/removed) — end the connection.
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = ticker.tick() => {
                if heartbeat.is_stale(Instant::now()) {
                    log::info!("ws heartbeat timeout: room={code}");
                    break;
                }
                if session.ping(b"").await.is_err() {
                    break;
                }
            }
        }
    }

    if let Some((id, epoch)) = me {
        log::info!("ws disconnected: room={code} player={}", id.0);
        let _ = registry.with_room_mut(&code, |room| {
            // Only the connection that still OWNS the player may mark them
            // disconnected. A socket superseded by a reconnect (the phone came
            // back before this one's heartbeat expired) is a ghost: it changes
            // nothing and broadcasts nothing.
            let out = if room.detach_connection(&id, epoch) {
                vec![Outbound::All(ServerMessage::RoomState {
                    room: PublicRoom::from(&*room),
                })]
            } else {
                log::debug!("ws stale cleanup ignored: room={code} player={}", id.0);
                Vec::new()
            };
            (
                (),
                out,
                // `connected` is not part of the snapshot, so a disconnect
                // changes no persisted state.
                false,
            )
        });
    }
    let _ = session.close(None).await;
}

/// Handle one inbound text frame. Returns `Err(())` to terminate the connection.
#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
async fn handle_text(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<(PlayerId, ConnEpoch)>,
    session: &mut actix_ws::Session,
    text: &str,
) -> Result<(), ()> {
    let Ok(msg) = serde_json::from_str::<ClientMessage>(text) else {
        log::warn!("ws bad_message: room={code} (unparseable frame)");
        let _ = forward(
            session,
            &ServerMessage::Error {
                code: "bad_message".into(),
                message: "Could not parse message".into(),
            },
        )
        .await;
        return Ok(());
    };

    if let ClientMessage::Join {
        player_token,
        player_name,
    } = &msg
    {
        return handle_join(
            registry,
            code,
            me,
            session,
            player_token.clone(),
            player_name.clone(),
        )
        .await;
    }

    let Some(actor) = me.as_ref().map(|(id, _)| id.clone()) else {
        let _ = forward(
            session,
            &ServerMessage::Error {
                code: "not_joined".into(),
                message: "Send join first".into(),
            },
        )
        .await;
        return Ok(());
    };

    let _ = registry.with_room_mut(code, |room| {
        let (out, dirty) = dispatch(room, &actor, msg, Instant::now());
        ((), out, dirty)
    });
    Ok(())
}

/// Resolve a join: re-attach via token, or create a new player.
#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
#[allow(clippy::too_many_lines)]
async fn handle_join(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<(PlayerId, ConnEpoch)>,
    session: &mut actix_ws::Session,
    player_token: Option<String>,
    player_name: Option<String>,
) -> Result<(), ()> {
    // One connection owns at most one player, for its whole life. A second
    // join would overwrite `me`, and the end-of-loop cleanup can only ever
    // detach whichever player it holds last — so the earlier one would be
    // stranded `connected: true` with no owner, sitting in the turn rotation
    // where nobody can claim it. Repeating it fills the room to MAX_PLAYERS
    // with phantoms.
    //
    // No legitimate client does this: the SPA's auto-rejoin fires on a fresh
    // socket, where `me` is None. A join that was REFUSED leaves `me` None
    // too, so a genuine retry still works.
    if me.is_some() {
        log::warn!("ws duplicate join refused: room={code} (connection already joined)");
        let _ = forward(
            session,
            &ServerMessage::Error {
                code: "wrong_state".into(),
                message: "Already joined on this connection".into(),
            },
        )
        .await;
        return Ok(());
    }

    // Validate a fresh player's name up front (rejoin-by-token reuses the
    // stored name, so it bypasses this). Absent/blank names keep the legacy
    // "Player" default; an over-long name is rejected.
    let is_rejoin = player_token.as_deref().is_some_and(|tok| !tok.is_empty());
    let new_name: Result<String, NameError> = if is_rejoin {
        Ok(String::new()) // unused for rejoins
    } else {
        match player_name.as_deref() {
            Some(raw) if !raw.trim().is_empty() => player::validate_name(raw),
            _ => Ok("Player".into()),
        }
    };
    let Ok(new_name) = new_name else {
        let _ = forward(
            session,
            &ServerMessage::Error {
                code: "bad_name".into(),
                message: "Name must be between 1 and 40 characters".into(),
            },
        )
        .await;
        return Ok(());
    };

    let now = Instant::now();
    let resolved = registry.with_room_mut(code, |room| {
        // Re-attach by token if provided, otherwise None → new player.
        let existing: Option<(PlayerId, String)> = player_token.and_then(|tok| {
            room.player_by_token(&tok)
                .map(|p| (p.id.clone(), p.token.clone()))
        });

        let outcome: JoinOutcome = if let Some((id, token)) = existing {
            // Taking ownership here supersedes any older socket for this
            // player, so its eventual cleanup cannot disconnect them.
            let epoch = room.attach_connection(&id);
            // Always send Welcome on (re-)join so the client can set `me`.
            // Reconnection by token bypasses the lock — a dropped player returns.
            JoinOutcome::Joined {
                id,
                token,
                epoch,
                rejoined: true,
            }
        } else if is_rejoin {
            // A token was supplied but matched nothing. Minting a player here
            // would create an empty-named ghost (the name is blank for rejoins),
            // so reject and let the client clear its stale token.
            JoinOutcome::UnknownToken
        } else if room.locked {
            JoinOutcome::RoomLocked
        } else if room.is_full() {
            JoinOutcome::RoomFull
        } else {
            let (id, token) = room.add_player(new_name, now);
            let epoch = room.attach_connection(&id);
            JoinOutcome::Joined {
                id,
                token,
                epoch,
                rejoined: false,
            }
        };

        let broadcast = match &outcome {
            JoinOutcome::Joined { .. } => vec![Outbound::All(ServerMessage::RoomState {
                room: PublicRoom::from(&*room),
            })],
            JoinOutcome::RoomFull | JoinOutcome::RoomLocked | JoinOutcome::UnknownToken => {
                Vec::new()
            }
        };
        // Only a brand-new player changes persisted state. A rejoin just flips
        // `connected` (not snapshotted); a full room changes nothing.
        let dirty = matches!(
            outcome,
            JoinOutcome::Joined {
                rejoined: false,
                ..
            }
        );
        (outcome, broadcast, dirty)
    });

    match resolved {
        Ok(JoinOutcome::Joined {
            id,
            token,
            epoch,
            rejoined,
        }) => {
            if rejoined {
                log::debug!("ws rejoin-by-token: room={code} player={}", id.0);
            } else {
                log::info!("ws join: room={code} player={}", id.0);
            }
            *me = Some((id.clone(), epoch));
            let _ = forward(
                session,
                &ServerMessage::Welcome {
                    player_id: id,
                    token,
                },
            )
            .await;
            Ok(())
        }
        Ok(JoinOutcome::RoomLocked) => {
            log::info!("ws join rejected: room={code} is locked");
            let _ = forward(
                session,
                &ServerMessage::Error {
                    code: "room_locked".into(),
                    message: "Room is locked".into(),
                },
            )
            .await;
            Ok(())
        }
        Ok(JoinOutcome::RoomFull) => {
            log::info!("ws join rejected: room={code} is full");
            let _ = forward(
                session,
                &ServerMessage::Error {
                    code: "room_full".into(),
                    message: "Room is full".into(),
                },
            )
            .await;
            Ok(())
        }
        Ok(JoinOutcome::UnknownToken) => {
            log::info!("ws join rejected: room={code} unknown player token");
            let _ = forward(
                session,
                &ServerMessage::Error {
                    code: "not_found".into(),
                    message: "Room not found".into(),
                },
            )
            .await;
            Ok(())
        }
        Err(_) => Err(()),
    }
}

/// Result of resolving a join inside the room lock.
enum JoinOutcome {
    Joined {
        id: PlayerId,
        token: String,
        /// Proof that this connection owns `id` until a later one attaches.
        epoch: ConnEpoch,
        rejoined: bool,
    },
    RoomFull,
    RoomLocked,
    /// A non-empty token was supplied but matched no player in the room.
    UnknownToken,
}

#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
async fn forward(session: &mut actix_ws::Session, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    session.text(json).await.map_err(|_| ())
}
