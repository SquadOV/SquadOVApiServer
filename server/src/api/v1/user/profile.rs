use squadov_common::SquadOvError;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::{
    SquadOVSession,
    SquadOVUser,
};
use std::sync::Arc;
use serde::Deserialize;
use sqlx::{Transaction, Postgres};

impl api::ApiApplication {
    async fn edit_user(&self, tx: &mut Transaction<'_, Postgres>, user: &SquadOVUser) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.users
            SET username = $2,
                email = $3,
                verified = $4
            WHERE id = $1
            ",
            user.id,
            user.username,
            user.email,
            user.verified,
        )
            .execute(tx)
            .await?;
        Ok(())
    }
}

pub async fn get_user_profile_handler(data : web::Path<super::UserResourcePath>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    match app.users.get_stored_user_from_id(data.user_id, &app.pool).await {
        Ok(x) => match x {
            Some(x) => Ok(HttpResponse::Ok().json(&x)),
            None => Err(squadov_common::SquadOvError::NotFound),
        },
        Err(err) => Err(squadov_common::SquadOvError::InternalError(format!("Get User Profile Handler {}", err))),
    }
}

pub async fn get_current_user_profile_handler(app : web::Data<Arc<api::ApiApplication>>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let user = app.users.get_stored_user_from_id(session.user.id, &app.pool).await?;
    return Ok(HttpResponse::Ok().json(&user));
}

#[derive(Deserialize)]
pub struct EditUsernameData {
    username: String
}

pub async fn edit_current_user_username_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<EditUsernameData>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    let user = SquadOVUser{
        username: data.username.clone(),
        ..session.user.clone()
    };

    let mut tx = app.pool.begin().await?;
    // Edit the user in the database first so that in the rare case that this fails, we don't
    // propagate the change to fusionauth.
    app.edit_user(&mut tx, &user).await?;

    // We need to also change the user's referral code since it's based off their username
    app.regenerate_user_referral_code(&mut tx, user.id).await?;

    // Finally change the user in fusionauth. If the fusionauth change fails none of the above
    // gets changed since we still haven't committed the transaction yet.
    let fa_user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
        SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
    })?;

    app.clients.fusionauth.update_user_id(&fa_user.id, &user.username, &session.user.email).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct EditEmailData {
    email: String
}

pub async fn edit_current_user_email_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<EditEmailData>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    let user = SquadOVUser{
        email: data.email.clone(),
        verified: false,
        ..session.user.clone()
    };

    let mut tx = app.pool.begin().await?;
    // Edit the user in the database first so that in the rare case that this fails, we don't
    // propagate the change to fusionauth.
    app.edit_user(&mut tx, &user).await?;

    // Finally change the user in fusionauth. If the fusionauth change fails none of the above
    // gets changed since we still haven't committed the transaction yet.
    let fa_user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
        SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
    })?;

    app.clients.fusionauth.update_user_id(&fa_user.id, &session.user.username, &user.email).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}