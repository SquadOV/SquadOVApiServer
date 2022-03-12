use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::{HashSet, HashMap};
use actix_web_actors::ws;
use actix::prelude::*;
use crate::{
    games::{FullSupportedGame},
    session::SessionVerifier,
    SquadOvError,
    redis::RedisConfig,
};
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};
use std::sync::Arc;
use async_std::sync::RwLock;

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
    pub states: HashMap<i64, UserActivityState>,
}

// Request message for when the user wants to subscribe to the status of another user (can be denied).
#[derive(Message)]
#[rtype(result="()")]
struct UserActivitySubscribeRequest {
    pub user_id: Vec<i64>,
}

#[derive(MessageResponse, Serialize)]
struct UserActivitySubscribeResponse {
    status: HashMap<i64, UserActivityState>
}

// TODO: This probably needs to be stored externally if we ever want to
// sync across multiple servers.
pub struct UserActivityStatusTracker {
    rconfig: RedisConfig,
    redis: Arc<deadpool_redis::Pool>,
    // Session ID of the Websocket to the address of the recipient to send to. 
    sessions: RwLock<HashMap<Uuid, Recipient<UserActivityChange>>>,
    // For each user, sessions that are listening to the user.
    per_user_sessions: RwLock<HashMap<i64, HashSet<Uuid>>>,
}

impl UserActivityStatusTracker {
    pub async fn new(redis_config: &RedisConfig, redis: Arc<deadpool_redis::Pool>) -> Arc<Self> {
        let tracker = Arc::new(UserActivityStatusTracker{
            rconfig: redis_config.clone(),
            redis,
            sessions: RwLock::new(HashMap::new()),
            per_user_sessions: RwLock::new(HashMap::new()),
        });

        {
            let ps_tracker = tracker.clone();
            tokio::task::spawn(async move {
                let outer_tracker = ps_tracker.clone();
                loop {
                    let inner_tracker = outer_tracker.clone();
                    let t1 = tokio::task::spawn(async move {
                        let client_tracker = inner_tracker.clone();
                        let client = redis::Client::open(client_tracker.rconfig.url.as_str())?;
                        let mut conn = client.get_connection()?;
                        let mut pubsub = conn.as_pubsub();
                        pubsub.subscribe("user-status")?;

                        loop {
                            let msg = pubsub.get_message()?;
                            let user_id: i64 = msg.get_payload::<String>()?.parse::<i64>()?;
                            inner_tracker.notify_user_state(user_id, None).await?;
                        }

                        #[allow(unreachable_code)]
                        Ok::<(), SquadOvError>(())
                    });

                    match t1.await {
                        Ok(_) => (),
                        Err(err) => log::warn!("Redis Pubsub Thread failed...restarting {:?}", err),
                    };

                    async_std::task::sleep(std::time::Duration::from_millis(16)).await;
                }
            });
        }

        tracker
    }

    async fn get_connection(&self) -> Result<deadpool_redis::Connection, SquadOvError> {
        let conn = self.redis.get().await?;
        Ok(conn)
    }

    fn get_user_cache_key(&self, user_id: i64) -> String {
        format!("state-cache-{}", user_id)
    }

    async fn get_user_state(&self, user_id: i64) -> Result<UserActivityState, SquadOvError> {
        let mut conn = self.get_connection().await?;
        let raw: Option<String> = deadpool_redis::redis::cmd("GET")
            .arg(&[&self.get_user_cache_key(user_id)])
            .query_async(&mut conn)
            .await?;

        Ok(if let Some(r) = raw {
            serde_json::from_str::<UserActivityState>(&r)?
        } else {
            UserActivityState::default()
        })
    }

    async fn batch_get_multiple_user_states(&self, user_ids: &[i64]) -> Result<Vec<UserActivityState>, SquadOvError> {
        // Break the input user ids into batches. Running a ton of keys on MGET at a single time is a YIKES.
        let mut result: Vec<UserActivityState> = vec![];
        for ch in user_ids.chunks(10) {
            let inner = self.get_multiple_user_states(ch).await?;
            result.extend(inner.into_iter());
        }
        Ok(result)
    }

    async fn get_multiple_user_states(&self, user_ids: &[i64]) -> Result<Vec<UserActivityState>, SquadOvError> {
        let mut conn = self.get_connection().await?;
        let raw: Vec<Option<String>> = deadpool_redis::redis::cmd("MGET")
            .arg(user_ids.iter().map(|x| { self.get_user_cache_key(*x)}).collect::<Vec<_>>().as_slice())
            .query_async(&mut conn)
            .await?;

        Ok(raw.into_iter().map(|x| {
            Ok::<UserActivityState, SquadOvError>(if let Some(r) = x {
                serde_json::from_str::<UserActivityState>(&r)?
            } else {
                UserActivityState::default()
            })
        }).collect::<Result<Vec<UserActivityState>, SquadOvError>>()?)
    }

    async fn notify_single_session_bulk_user_state(&self, session_id: &Uuid, states: HashMap<i64, UserActivityState>) -> Result<(), SquadOvError> {
        let sessions = self.sessions.read().await;
        if let Some(addr) = sessions.get(session_id) {
            addr.do_send(UserActivityChange{
                states,
            }).map_err(|x| {
                SquadOvError::InternalError(format!("Failed to notify user of bulk state change: {:?}", x))
            })?;
        }
        Ok(())
    }

    async fn notify_single_session_user_state(&self, session_id: &Uuid, user_id: i64, state: UserActivityState) -> Result<(), SquadOvError> {
        let mut states = HashMap::new();
        states.insert(user_id, state);
        self.notify_single_session_bulk_user_state(session_id, states).await
    }

    async fn notify_user_state(&self, user_id: i64, state: Option<UserActivityState>) -> Result<(), SquadOvError> {
        let final_state: UserActivityState = if let Some(st) = state {
            st
        } else {
            self.get_user_state(user_id).await?
        };

        let per_user_sessions = self.per_user_sessions.read().await;

        if let Some(pu_sessions) = per_user_sessions.get(&user_id) {
            for session_id in pu_sessions {
                self.notify_single_session_user_state(session_id, user_id, final_state.clone()).await?;
            }
        }

        Ok(())
    }

    async fn update_user_state(&self, user_id: i64, state: UserActivityState) -> Result<(), SquadOvError> {
        let mut conn = self.get_connection().await?;

        // Update user state in Redis.
        if state.activity == Activity::Offline {
            deadpool_redis::redis::cmd("DEL")
                .arg(&[&self.get_user_cache_key(user_id)])
                .query_async(&mut conn)
                .await?;
        } else {
            deadpool_redis::redis::cmd("SET")
                // If the user hasn't changed their status in a day they're fucking offline.
                .arg(&[&self.get_user_cache_key(user_id), &serde_json::to_string(&state)?, "EX", "86400"])
                .query_async(&mut conn)
                .await?;
        }

        // Notify everyone else of the user's state change via pub/sub using Redis.
        deadpool_redis::redis::cmd("PUBLISH")
            .arg(&["user-status", &format!("{}", user_id)])
            .query_async(&mut conn)
            .await?;
        
        // Send updates to listening users as necessary.
        self.notify_user_state(user_id, Some(state)).await?;

        Ok(())
    }

    async fn add_subscriptions(&self, id: &Uuid, user_ids: &[i64]) -> Result<(), SquadOvError> {
        let mut subs = self.per_user_sessions.write().await;
        let user_states = self.batch_get_multiple_user_states(user_ids).await?;
        let mut user_state_map: HashMap<i64, UserActivityState> = HashMap::new();
        for (idx, uid) in user_ids.iter().enumerate() {
            if !subs.contains_key(uid) {
                subs.insert(*uid, HashSet::new());
            }

            if let Some(listeners) = subs.get_mut(uid) {
                listeners.insert(id.clone());
            }

            user_state_map.insert(*uid, user_states[idx].clone());
        }
        self.notify_single_session_bulk_user_state(id, user_state_map).await?;

        Ok(())
    }

    async fn remove_subscriptions(&self, id: &Uuid, user_ids: &[i64]) {
        let mut subs = self.per_user_sessions.write().await;
        for uid in user_ids {
            if let Some(listeners) = subs.get_mut(uid) {
                listeners.remove(id);
            }
        }
    }

    async fn add_session(&self, id: &Uuid, addr: Recipient<UserActivityChange>) {
        let mut sess = self.sessions.write().await;
        sess.insert(id.clone(), addr);
    }

    async fn remove_session(&self, id: &Uuid) {
        let mut sess = self.sessions.write().await;
        sess.remove(id);
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
    pub tracker: Arc<UserActivityStatusTracker>,
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
    pub fn new(user_id: i64, tracker: Arc<UserActivityStatusTracker>, verifier: Arc<T>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            last_heartbeat: Utc::now(),
            tracker,
            verifier,
            authenticated: false,
        }
    }

    fn start_session_heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            let now = Utc::now();
            if now.signed_duration_since(act.last_heartbeat) > chrono::Duration::seconds(HEARTBEAT_TIMEOUT_SECONDS) {
                let tracker = act.tracker.clone();
                let user_id = act.user_id;
                tokio::task::spawn(async move {
                    match tracker.update_user_state(user_id, UserActivityState{
                        activity: Activity::Offline,
                        ..UserActivityState::default()
                    }).await {
                        Ok(_) => (),
                        Err(err) => log::warn!("Failed to update user state on session fail: {:?}", err),
                    };
                });
                
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

        let id = self.id.clone();
        let rec = ctx.address().recipient();
        let user_id = self.user_id;
        let tracker = self.tracker.clone();
        tokio::task::spawn(async move {
            tracker.add_session(&id, rec).await;
            match tracker.update_user_state(user_id, UserActivityState{
                activity: Activity::Online,
                ..UserActivityState::default()
            }).await {
                Ok(_) => (),
                Err(err) => log::warn!("Fail to connect user to tracker: {:?}", err),
            };
        });        
    }

    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        log::info!("User Activity Disconnected: {}", self.user_id);
        
        let id = self.id.clone();
        let user_id = self.user_id;
        let tracker = self.tracker.clone();

        tokio::task::spawn(async move {
            tracker.remove_session(&id).await;
            match tracker.update_user_state(user_id, UserActivityState{
                activity: Activity::Offline,
                ..UserActivityState::default()
            }).await {
                Ok(_) => (),
                Err(err) => log::warn!("Fail to remove user from tracker: {:?}", err),
            };
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
                    UserActivityMessage::Unsubscribe{users} => {
                        let id = self.id.clone();
                        let tracker = self.tracker.clone();
                        tokio::task::spawn(async move {
                            tracker.remove_subscriptions(&id, &users).await;
                        });
                    },
                    UserActivityMessage::StatusChange{activity, game} => {
                        let tracker = self.tracker.clone();
                        let user_id = self.user_id;

                        tokio::task::spawn(async move {
                            match tracker.update_user_state(
                                user_id,
                                UserActivityState{activity, game},
                            ).await {
                                Ok(_) => (),
                                Err(err) => log::warn!("Fail to handle status change: {:?}", err),
                            };
                        });
                    },
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
        
        let user_ids = msg.user_id.clone();
        future
            .into_actor(self)
            .then(move |res, act, _ctx| {
                match res {
                    Ok(v) => {
                        if v {
                            let tracker = act.tracker.clone();
                            let id = act.id.clone();
                            tokio::task::spawn(async move {
                                match tracker.add_subscriptions(&id, &user_ids).await {
                                    Ok(_) => (),
                                    Err(err) => log::warn!("Failed to add subs {:?}", err),
                                }
                            });
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
        
        let resp = UserActivitySubscribeResponse{
            status: msg.states,
        };
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
