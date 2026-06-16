//! actix-ws connection handler. Each connection runs a task that selects
//! between the incoming client stream and the room's broadcast receiver.

use std::sync::Arc;
use std::time::Instant;

use actix_web::{HttpRequest, HttpResponse, web};
use futures_util::StreamExt as _;

use crate::domain::ids::PlayerId;
use crate::registry::{Outbound, Registry};
use crate::wire::{ClientMessage, PublicRoom, ServerMessage};
use crate::ws::dispatch::dispatch;

/// GET /ws/{code} — upgrade to a WebSocket bound to that room.
///
/// # Errors
/// Returns 404 if the room does not exist; otherwise upgrades the connection.
#[allow(clippy::future_not_send)] // actix-web types (HttpRequest, Payload) are not Send; handler runs on actix's single-threaded runtime
pub async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    registry: web::Data<Arc<Registry>>,
) -> Result<HttpResponse, actix_web::Error> {
    let code = path.into_inner();
    if !registry.contains(&code) {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({ "error": "Room not found" })));
    }
    let (response, session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let registry = registry.get_ref().clone();

    actix_web::rt::spawn(async move {
        let Ok(mut rx) = registry.subscribe(&code) else {
            return;
        };
        let mut session = session;
        let mut me: Option<PlayerId> = None;

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
                            if me.as_ref() == Some(&target)
                                && forward(&mut session, &msg).await.is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        if let Some(id) = me {
            let _ = registry.with_room_mut(&code, |room| {
                if let Some(p) = room.players.iter_mut().find(|p| p.id == id) {
                    p.connected = false;
                }
                (
                    (),
                    vec![Outbound::All(ServerMessage::RoomState {
                        room: PublicRoom::from(&*room),
                    })],
                )
            });
        }
        let _ = session.close(None).await;
    });

    Ok(response)
}

/// Handle one inbound text frame. Returns `Err(())` to terminate the connection.
#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
async fn handle_text(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<PlayerId>,
    session: &mut actix_ws::Session,
    text: &str,
) -> Result<(), ()> {
    let Ok(msg) = serde_json::from_str::<ClientMessage>(text) else {
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

    let Some(actor) = me.clone() else {
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
        let out = dispatch(room, &actor, msg, Instant::now());
        ((), out)
    });
    Ok(())
}

/// Resolve a join: re-attach via token, or create a new player.
#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
async fn handle_join(
    registry: &Arc<Registry>,
    code: &str,
    me: &mut Option<PlayerId>,
    session: &mut actix_ws::Session,
    player_token: Option<String>,
    player_name: Option<String>,
) -> Result<(), ()> {
    let now = Instant::now();
    let resolved = registry.with_room_mut(code, |room| {
        // Re-attach by token if provided, otherwise None → new player.
        let existing: Option<PlayerId> =
            player_token.and_then(|tok| room.player_by_token(&tok).map(|p| p.id.clone()));

        let (id, fresh_token) = if let Some(id) = existing {
            if let Some(p) = room.players.iter_mut().find(|p| p.id == id) {
                p.connected = true;
            }
            (id, None)
        } else {
            let name = player_name.unwrap_or_else(|| "Player".into());
            let (id, token) = room.add_player(name, now);
            (id, Some(token))
        };

        let broadcast = vec![Outbound::All(ServerMessage::RoomState {
            room: PublicRoom::from(&*room),
        })];
        ((id, fresh_token), broadcast)
    });

    match resolved {
        Ok((id, fresh_token)) => {
            *me = Some(id.clone());
            if let Some(token) = fresh_token {
                let _ = forward(
                    session,
                    &ServerMessage::Welcome {
                        player_id: id,
                        token,
                    },
                )
                .await;
            }
            Ok(())
        }
        Err(_) => Err(()),
    }
}

#[allow(clippy::result_unit_err)] // unit error is an intentional "just terminate" signal
async fn forward(session: &mut actix_ws::Session, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    session.text(json).await.map_err(|_| ())
}
