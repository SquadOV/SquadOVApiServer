use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::{HashSet, HashMap};
use actix_web_actors::ws;
use actix::prelude::*;
use crate::{
    games::{FullSupportedGame},
    session::SessionVerifier,
};
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};
use std::sync::Arc;

const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
const HEARTBEAT_TIMEOUT_SECONDS: i64 = 30;

#[derive(Clone,Debug,Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(i32)]
pub enum Activity {
    Online,
    InGame,
    Recording,
    Offline
}

#[derive(Clone,Serialize,Deserialize,Debug)]
struct UserActivityState {
    activity: Activity,
    game: Vec<FullSupportedGame>,
}

impl Default for UserActivityState {
    fn default() -> Self {
        UserActivityState{
            activity: Activity::Offline,
            game: Vec::new(),
        }
    }
}

// Message for when the server receives a change in the user's activity
#[derive(Message)]
#[rtype(result="()")]
struct UserActivityChange {
    pub user_id: i64,
    pub state: UserActivityState,
}

// Message for when the user first registers that they want to listen for activity messages.
#[derive(Message)]
#[rtype(result="()")]
struct UserActivityConnect {
    pub id: Uuid,
    pub addr: Recipient<UserActivityChange>,
}

// Message for when the user no longer wishes to listen for activity messages (all of them).
#[derive(Message)]
#[rtype(result="()")]
struct UserActivityDisconnect {
    pub id: Uuid,
}

// Request message for when the user wants to subscribe to the status of another user (can be denied).
#[derive(Message)]
#[rtype(result="()")]
struct UserActivitySubscribeRequest {
    pub user_id: Vec<i64>,
}

// Authoritative message for when the user wants to subscribe to the status of another user.
#[derive(Message)]
#[rtype(result="UserActivitySubscribeResponse")]
struct UserActivitySubscribe {
    pub session_id: Uuid,
    pub user_id: Vec<i64>,
}

#[derive(MessageResponse, Serialize)]
struct UserActivitySubscribeResponse {
    status: HashMap<i64, UserActivityState>
}

// Message for when the user wants to unsubscribe from the status of another user.
#[derive(Message)]
#[rtype(result="()")]
struct UserActivityUnsubscribe {
    pub session_id: Uuid,
    pub user_id: Vec<i64>,
}

// TODO: This probably needs to be stored externally if we ever want to
// sync across multiple servers.
pub struct UserActivityStatusTracker {
    // Session ID of the Websocket to the address of the recipient to send to. 
    sessions: HashMap<Uuid, Recipient<UserActivityChange>>,
    // For each user, sessions that are listening to the user.
    per_user_sessions: HashMap<i64, HashSet<Uuid>>,
    // For each user, their current activity state.
    user_states: HashMap<i64, UserActivityState>,
}

impl UserActivityStatusTracker {
    pub fn new() -> Self {
        UserActivityStatusTracker {
            sessions: HashMap::new(),
            per_user_sessions: HashMap::new(),
            user_states: HashMap::new(),
        }
    }

    fn update_user_state(&mut self, user_id: i64, state: UserActivityState) {
        if state.activity == Activity::Offline {
            self.user_states.remove(&user_id);
        } else {
            self.user_states.insert(user_id, state.clone());
        }

        if let Some(sessions) = self.per_user_sessions.get(&user_id) {
            for session_id in sessions {
                if let Some(addr) = self.sessions.get(session_id) {
                    match addr.do_send(UserActivityChange{
                        user_id,
                        state: state.clone(),
                    }) {
                        Ok(_) => (),
                        Err(err) => log::warn!("Failed to send status update of {} to {} - {:?}", user_id, session_id, err),
                    };
                }
            }
        }
    }
}

impl actix::Actor for UserActivityStatusTracker {
    type Context = Context<Self>;
}

impl actix::Handler<UserActivityConnect> for UserActivityStatusTracker {
    type Result = ();

    fn handle(&mut self, msg: UserActivityConnect, _ctx: &mut Self::Context) {
        log::info!("User Activity Connect: {}", &msg.id);
        self.sessions.insert(msg.id, msg.addr);
    }
}

impl actix::Handler<UserActivityDisconnect> for UserActivityStatusTracker {
    type Result = ();

    fn handle(&mut self, msg: UserActivityDisconnect, _ctx: &mut Self::Context) {
        log::info!("User Activity Disconnect: {}", &msg.id);
        self.sessions.remove(&msg.id);
        for (_, user_sessions) in &mut self.per_user_sessions {
            user_sessions.remove(&msg.id);
        }
    }
}

impl actix::Handler<UserActivityChange> for UserActivityStatusTracker {
    type Result = ();

    fn handle(&mut self, msg: UserActivityChange, _ctx: &mut Self::Context) {
        log::info!("User Activity Change: {} - {:?}", msg.user_id, msg.state);
        self.update_user_state(msg.user_id, msg.state);
    }
}

impl actix::Handler<UserActivitySubscribe> for UserActivityStatusTracker {
    type Result = UserActivitySubscribeResponse;

    fn handle(&mut self, msg: UserActivitySubscribe, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("User Activity Subscribe to {:?} by {}", &msg.user_id, &msg.session_id);
        let mut status = HashMap::new();
        for user in &msg.user_id {
            if !self.per_user_sessions.contains_key(user) {
                self.per_user_sessions.insert(user.clone(), HashSet::new());
            }
            self.per_user_sessions.get_mut(user).unwrap().insert(msg.session_id.clone());
            status.insert(user.clone(), self.user_states.get(user).cloned().unwrap_or_default());
        }

        UserActivitySubscribeResponse{status}
    }
}

impl actix::Handler<UserActivityUnsubscribe> for UserActivityStatusTracker {
    type Result = ();

    fn handle(&mut self, msg: UserActivityUnsubscribe, _ctx: &mut Self::Context) {
        log::info!("User Activity Unsubscribe from {:?} by {}", &msg.user_id, &msg.session_id);
        for user in &msg.user_id {
            if !self.per_user_sessions.contains_key(user) {
                continue;
            }
            self.per_user_sessions.get_mut(user).unwrap().remove(&msg.session_id);
        }
    }
}

pub struct UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    // session id
    pub id: Uuid,
    pub user_id: i64,
    pub last_heartbeat: DateTime<Utc>,
    pub watched_users: HashSet<Uuid>,
    pub tracker: actix::Addr<UserActivityStatusTracker>,
    pub verifier: Arc<T>,
    pub authenticated: bool,
}

#[derive(Message)]
#[rtype(result="()")]
pub struct WebsocketAuthenticationRequest {
    pub session_id: String,
}

#[derive(Serialize)]
pub struct WebsocketAuthenticationResponse {
    pub success: bool,
}

impl<T> UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    pub fn new(user_id: i64, tracker: actix::Addr<UserActivityStatusTracker>, verifier: Arc<T>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            last_heartbeat: Utc::now(),
            watched_users: HashSet::new(),
            tracker,
            verifier,
            authenticated: false,
        }
    }

    fn start_session_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            let now = Utc::now();
            if now.signed_duration_since(act.last_heartbeat) > chrono::Duration::seconds(HEARTBEAT_TIMEOUT_SECONDS) {
                act.tracker.do_send(UserActivityDisconnect{ id: act.id.clone() });
                ctx.stop();
            } else {
                ctx.ping(b"");
            }
        });
    }
}

impl<T> actix::Actor for UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("User Activity Connected: {}", self.user_id);
        self.start_session_heartbeat(ctx);

        self.tracker
            .send(UserActivityConnect{
                id: self.id.clone(),
                addr: ctx.address().recipient(),
            })
            .into_actor(self)
            .then(|res, _act, ctx| {
                match res {
                    Ok(_) => (),
                    _ => ctx.stop(),
                }
                fut::ready(())
            })
            .wait(ctx);
        self.tracker.do_send(UserActivityChange{
            user_id: self.user_id.clone(),
            state: UserActivityState{
                activity: Activity::Online,
                game: Vec::new(),
            }
        });
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        log::info!("User Activity Disconnected: {}", self.user_id);
        self.tracker.do_send(UserActivityDisconnect{ id: self.id.clone() });
        self.tracker.do_send(UserActivityChange{
            user_id: self.user_id.clone(),
            state: UserActivityState{
                activity: Activity::Offline,
                game: Vec::new(),
            }
        });
        Running::Stop
    }
}

#[derive(Deserialize)]
#[serde(tag="type")]
pub enum UserActivityMessage {
    Authenticate{
        #[serde(rename="sessionId")]
        session_id: String,
    },
    Subscribe{
        users: Vec<i64>,
    },
    Unsubscribe{
        users: Vec<i64>,
    },
    StatusChange{
        activity: Activity,
        game: Vec<FullSupportedGame>,
    }
}

impl<T> StreamHandler<Result<ws::Message, ws::ProtocolError>> for UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.last_heartbeat= Utc::now();
                ctx.pong(&msg);
            },
            Ok(ws::Message::Pong(_)) => {
                self.last_heartbeat = Utc::now();
            },
            Ok(ws::Message::Text(text)) => {
                log::info!("Receive status msg: {}", &text);
                let parsed_message = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(err) => {
                        log::warn!("Failed to parse status message: {:?} [{}]", err, &text);
                        return;
                    }
                };

                match parsed_message {
                    UserActivityMessage::Subscribe{users} => {
                        // Two step process here: 1) we need to ensure that the session has
                        // access to the requested users before we 2) actually subscribe them
                        // to the users' statuses.
                        ctx.notify(UserActivitySubscribeRequest{
                            user_id: users,
                        });
                    },
                    UserActivityMessage::Unsubscribe{users} => self.tracker.do_send(UserActivityUnsubscribe{
                        session_id: self.id.clone(),
                        user_id: users,
                    }),
                    UserActivityMessage::StatusChange{activity, game} => self.tracker.do_send(UserActivityChange{
                        user_id: self.user_id.clone(),
                        state: UserActivityState{activity, game},
                    }),
                    UserActivityMessage::Authenticate{session_id} => {
                        ctx.notify(WebsocketAuthenticationRequest{
                            session_id,
                        });
                    },
                };
            },
            Ok(ws::Message::Binary(_)) => log::info!("Ignoring unhandled binary message."),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            Ok(ws::Message::Continuation(_)) => {
                ctx.stop();
            }
            Ok(ws::Message::Nop) => (),
            Err(err) => {
                log::warn!("Error in user activity session handler: {:?}", err);
                ctx.stop();
            }
        }
    }
}

impl<T> actix::Handler<UserActivitySubscribeRequest> for UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    type Result = ();

    fn handle(&mut self, msg: UserActivitySubscribeRequest, ctx: &mut Self::Context) {
        if !self.authenticated {
            return;
        }

        let verifier = self.verifier.clone();
        let user_id = self.user_id;
        let user_ids = msg.user_id.clone();
        let future = async move {
            verifier.verify_user_access_to_users(user_id, &user_ids).await
        };
            
        future
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(v) => {
                        if v {
                            act.tracker.send(UserActivitySubscribe{
                                session_id: act.id.clone(),
                                user_id: msg.user_id,
                            })
                                .into_actor(act)
                                .then(|ires, _act, ictx| {
                                    match ires {
                                        Ok(x) => ictx.text(serde_json::to_string(&x).unwrap_or(String::from("ERROR"))),
                                        Err(err) => {
                                            log::warn!("Failed to subscribe to user status: {:?}", err);
                                            ictx.text(String::from("ERROR"));
                                        }
                                    }
                                    fut::ready(())
                                })
                                .wait(ctx);
                        } else {
                            log::warn!("Session does not have access to requested users.");
                        }
                    },
                    Err(err) => {
                        log::warn!("Failed to subscribe to user activity: {:?}", err);
                    }
                }
                fut::ready(())
            })
            .wait(ctx);
    }
}


impl<T> actix::Handler<UserActivityChange> for UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    type Result = ();

    fn handle(&mut self, msg: UserActivityChange, ctx: &mut Self::Context) {
        if !self.authenticated {
            return;
        }
        
        let mut resp = UserActivitySubscribeResponse{
            status: HashMap::new(),
        };
        resp.status.insert(msg.user_id, msg.state);
        ctx.text(serde_json::to_string(&resp).unwrap_or(String::from("ERROR")))
    }
}

impl<T> actix::Handler<WebsocketAuthenticationRequest> for UserActivitySession<T>
where
    T: SessionVerifier + 'static
{
    type Result = ();

    fn handle(&mut self, msg: WebsocketAuthenticationRequest, ctx: &mut Self::Context) {
        let verifier = self.verifier.clone();
        let user_id = self.user_id;

        let future = async move {
            verifier.verify_session_id_for_user(user_id, msg.session_id).await
        };
            
        future
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(v) => {
                        act.authenticated = v;
                        ctx.text(serde_json::to_string(&WebsocketAuthenticationResponse{
                            success: v
                        }).unwrap())
                    },
                    Err(err) => {
                        log::warn!("Failed to verify user session: {:?}", err);
                        ctx.text(serde_json::to_string(&WebsocketAuthenticationResponse{
                            success: false
                        }).unwrap())
                    }
                }
                fut::ready(())
            })
            .wait(ctx);
    }
}
