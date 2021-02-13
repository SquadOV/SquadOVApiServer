use squadov_common::{SquadOvError, SerializedUserSession, SessionJwtClaims};
use actix_web::{web, HttpResponse, HttpRequest};
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

pub async fn refresh_user_session_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<RefreshSessionInput>) -> Result<HttpResponse, SquadOvError> {
    let session = app.session.get_session_from_id(&data.session_id, &app.pool).await?;
    if session.is_none() {
        return Err(SquadOvError::Unauthorized);
    }

    // We do need to force the refresh here because the client will pre-emptively
    // request a new session to make sure it doesn't ever have an invalid session.
    let session = app.refresh_session_if_necessary(session.unwrap(), true).await?;

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