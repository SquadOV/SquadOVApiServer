use squadov_common::{SquadOvError, SerializedUserSession, SessionJwtClaims};
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use serde::Deserialize;
use chrono::{DateTime, Utc, NaiveDateTime};

#[derive(Deserialize)]
pub struct RefreshSessionInput {
    #[serde(rename="sessionId")]
    session_id: String
}

async fn handle_session_user_backfill_tasks(app : web::Data<Arc<api::ApiApplication>>, session: &SquadOVSession) -> Result<(), SquadOvError> {
    if !session.user.welcome_sent {
        app.send_welcome_email_to_user(&session.user).await?;
    }

    if session.user.registration_time.is_none() {
        let fa_user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
            SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
        })?;
        let reg = app.clients.fusionauth.find_auth_registration(&fa_user);
        if reg.is_some() {
            let reg_time = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(reg.as_ref().unwrap().insert_instant / 1000, 0), Utc);
            app.update_user_registration_time(session.user.id, &reg_time).await?;
        } else {
            log::warn!("Failed to find FusionAuth registration for user: {}", session.user.id);
        }
    }
    Ok(())
}

pub async fn refresh_user_session_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<RefreshSessionInput>) -> Result<HttpResponse, SquadOvError> {
    let session = app.session.get_session_from_id(&data.session_id, &app.pool).await?;
    if session.is_none() {
        return Err(SquadOvError::Unauthorized);
    }
    
    // We do need to force the refresh here because the client will pre-emptively
    // request a new session to make sure it doesn't ever have an invalid session.
    let session = app.refresh_session_if_necessary(session.unwrap(), true).await?;

    // This block of code isn't crucial to the task of refreshing the session so we can ignore errors here.
    match handle_session_user_backfill_tasks(app.clone(), &session).await {
        Ok(_) => (),
        Err(err) => log::warn!("Failed to handle session user backfill tasks: {}", err),
    };

    // Extract expiration from the access token JWT.
    let token = jsonwebtoken::dangerous_insecure_decode::<SessionJwtClaims>(&session.access_token)?;
    let expiration = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(token.claims.exp, 0), Utc);

    let now = Utc::now();
    if expiration < now {
        return Err(SquadOvError::InternalError(String::from("Bad expiration")));
    }

    let time_to_expire = expiration - now;
    Ok(HttpResponse::Ok().json(SerializedUserSession{
        session_id: session.session_id.clone(),
        expiration,
        expires_in: time_to_expire.num_seconds(),
        server_time: Utc::now(),
    }))
}

pub async fn mark_user_active_endpoint_handler(app : web::Data<Arc<api::ApiApplication>>,  request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    squadov_common::analytics::mark_active_user_endpoint(&mut tx, session.user.id).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}