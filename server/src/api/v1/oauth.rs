use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::{
    v1::UserResourcePath,
    auth::SquadOVSession,
};
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    accounts::twitch,
    twitch::{
        api::{
            EventSubCondition,
            EventSubTransport,
            TwitchTokenType,
        },
        eventsub::{
            TWITCH_CHANNEL_SUBSCRIBE,
            TWITCH_CHANNEL_UNSUB,
        },
    },
    discord::{
        api::{
            DiscordApiClient,
        },
        self,
    }
};
use uuid::Uuid;
use serde::Deserialize;
use chrono::{Utc, Duration};
use url::Url;

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct OAuthCallbackData {
    state: String,
    code: String,
    #[serde(default)]
    redirect_url: String,
}

pub async fn get_user_rso_auth_url_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<UserResourcePath>) -> Result<HttpResponse, SquadOvError> {
    // Generate a new session just for OAuth since the web app will need to know how to fire off a valid
    // API request even if the user isn't logged in in the browser.
    let session = SquadOVSession{
        session_id: Uuid::new_v4().to_string(),
        user: app.users.get_stored_user_from_id(path.user_id, &*app.pool).await?.ok_or(SquadOvError::NotFound)?,
        access_token: String::new(),
        refresh_token: String::new(),
        is_temp: true,
        share_token: None,
        sqv_access_token: None,
    };
    app.session.store_session(&*app.pool, &session).await?;

    let state = squadov_common::generate_csrf_user_oauth_state(&*app.pool, &session.user.uuid, &session.session_id).await?;
    let rso_url = format!(
        "{base}&state={state}",
        base=app.config.riot.rso_url,
        state=&state,
    );
    Ok(HttpResponse::Ok().json(&rso_url))
}

pub async fn get_twitch_login_url_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    
    let session = SquadOVSession{
        session_id: Uuid::new_v4().to_string(),
        user: session.user.clone(),
        access_token: String::new(),
        refresh_token: String::new(),
        is_temp: true,
        share_token: None,
        sqv_access_token: None,
    };
    app.session.store_session(&*app.pool, &session).await?;

    let state = squadov_common::generate_csrf_user_oauth_state(&*app.pool, &session.user.uuid, &session.session_id).await?;
    let url = format!(
        "{base}&state={state}",
        base=app.config.twitch.base_url,
        state=&state,
    );
    Ok(HttpResponse::Ok().json(&url))
}

pub async fn handle_riot_oauth_callback_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<OAuthCallbackData>) -> Result<HttpResponse, SquadOvError> {
    let session_id = squadov_common::check_csrf_user_oauth_state(&*app.pool, &data.state).await?;
    let session = app.session.get_session_from_id(&session_id, &*app.pool).await?.ok_or(SquadOvError::Unauthorized)?;

    // Now we can exchange the auth code for an access token from Riot.
    // We can then use this access token to get account information.
    // TODO: When we have production key access for LoL/TFT we need to use this account information to summoner name info too.
    let access_token = squadov_common::riot::rso::exchange_authorization_code_for_access_token(&app.config.riot.rso_client_id, &app.config.riot.rso_client_secret, &data.code).await?;
    let account = app.rso_itf.obtain_riot_account_from_access_token(&access_token.access_token, &access_token.refresh_token, &(Utc::now() + Duration::seconds(access_token.expires_in.into())), session.user.id).await?;

    // Need to request a Valorant backfill off the bat because we need to assume that the user's backfill won't come in on time just in
    // case they don't try to link their account until they're already in-game.
    app.valorant_itf.request_backfill_user_valorant_matches(&account.puuid).await?;

    app.session.delete_session(&session_id, &*app.pool).await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn handle_twitch_oauth_callback_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<OAuthCallbackData>) -> Result<HttpResponse, SquadOvError> {
    let session_id = squadov_common::check_csrf_user_oauth_state(&*app.pool, &data.state).await?;
    let session = app.session.get_session_from_id(&session_id, &*app.pool).await?.ok_or(SquadOvError::Unauthorized)?;

    // I don't think Twitch's API has rate limits? I COULD BE VERY WRONG.
    // So it should be safe to just handle all the Twitch API calls we need in this HTTP request
    // without dumping it onto RabbitMQ.
    let mut redirect_url = Url::parse(&data.redirect_url)?;
    redirect_url.set_query(None);

    let token = squadov_common::twitch::oauth::exchange_authorization_code_for_access_token(
        &app.config.twitch.client_id,
        &app.config.twitch.client_secret,
        &redirect_url.as_str(),
        &data.code
    ).await?;
    let api = squadov_common::twitch::api::TwitchApiClient::new(app.config.twitch.clone(), token.clone(), TwitchTokenType::User, app.pool.clone());

    // Parse the ID token to get the user ID.
    if token.id_token.is_none() {
        return Err(SquadOvError::BadRequest);
    }

    let id = token.id_token.clone().unwrap();
    let twitch_user_id = squadov_common::twitch::oauth::extract_twitch_user_id_from_id_token(&id)?;

    let account = api.get_basic_account_info(&twitch_user_id).await?;

    // If the account already exists then we shouldn't create an EventSub subscription.
    if twitch::find_twitch_account_id(&*app.pool, &account.twitch_user_id).await?.is_none() {
        // We do this immediately and not in the transaction because we're passing off to RabbitMQ
        // and we don't want there to be some weird thing where we haven't stored the Twitch account
        // into our database yet.
        twitch::create_twitch_account(&*app.pool, &account, &token).await?;

        let condition = EventSubCondition{
            broadcaster_user_id: account.twitch_user_id.clone(),
        };

        let transport = EventSubTransport{
            method: String::from("webhook"),
            callback: format!("{}/twitch/eventsub", &app.config.twitch.eventsub_hostname),
            // because i'm too lazy to add another secret into the app atm lmao.
            secret: app.config.squadov.hashid_salt.clone(),
        };

        // These EventSub lets us detect changes automatically to who's actually subscribed to the Twitch channel (one on sub and one on unsub).
        app.twitch_api.register_eventsub_subscription(TWITCH_CHANNEL_SUBSCRIBE, condition.clone(), transport.clone()).await?;
        app.twitch_api.register_eventsub_subscription(TWITCH_CHANNEL_UNSUB, condition.clone(), transport.clone()).await?;
        
        // We also need to do an initial subscriber sync. This should be fairly quick as long as Pokimane's not joining
        // SquadOV. To be safe we can just throw it onto RabbitMQ to make sure this process is reliable as well.
        app.twitch_itf.request_sync_subscriber(&account.twitch_user_id, None).await?;
    }

    twitch::link_twitch_account_to_user(&*app.pool, session.user.id, &account.twitch_user_id).await?;
    
    app.session.delete_session(&session_id, &*app.pool).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_discord_login_url_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    
    let session = SquadOVSession{
        session_id: Uuid::new_v4().to_string(),
        user: session.user.clone(),
        access_token: String::new(),
        refresh_token: String::new(),
        is_temp: true,
        share_token: None,
        sqv_access_token: None,
    };
    app.session.store_session(&*app.pool, &session).await?;

    let state = squadov_common::generate_csrf_user_oauth_state(&*app.pool, &session.user.uuid, &session.session_id).await?;
    let url = format!(
        "{base}&state={state}",
        base=app.config.discord.base_url,
        state=&state,
    );
    Ok(HttpResponse::Ok().json(&url))
}

pub async fn handle_discord_oauth_callback_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<OAuthCallbackData>) -> Result<HttpResponse, SquadOvError> {
    let session_id = squadov_common::check_csrf_user_oauth_state(&*app.pool, &data.state).await?;
    let session = app.session.get_session_from_id(&session_id, &*app.pool).await?.ok_or(SquadOvError::Unauthorized)?;

    let mut redirect_url = Url::parse(&data.redirect_url)?;
    redirect_url.set_query(None);

    let token = squadov_common::discord::oauth::exchange_authorization_code_for_access_token(
        &app.config.discord.client_id,
        &app.config.discord.client_secret,
        &redirect_url.as_str(),
        &data.code
    ).await?;

    let api = DiscordApiClient::new(app.config.discord.clone(), token.clone(), app.pool.clone(), session.user.id);
    let discord_user = api.get_current_user().await?;

    let mut tx = app.pool.begin().await?;
    discord::db::store_discord_user(&mut tx, &discord_user).await?;
    discord::db::link_discord_user_to_squadv(&mut tx, session.user.id, discord_user.id.parse::<i64>()?, &token).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}