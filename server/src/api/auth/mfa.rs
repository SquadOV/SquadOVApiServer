use actix_web::{HttpResponse, web, HttpRequest, HttpMessage};
use crate::api;
use serde::{Serialize, Deserialize};
use crate::api::{
    auth::SquadOVSession,
    fusionauth::FusionAuthMfaSecret,
};
use squadov_common::SquadOvError;
use std::sync::Arc;

pub async fn check_2fa_status_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    let user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
        SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
    })?;

    let has_2fa = if let Some(methods) = user.two_factor.methods.as_ref() {
        !methods.is_empty()
    } else {
        false
    };

    Ok(HttpResponse::Ok().json(has_2fa))
}

pub async fn get_2fa_qr_code_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let secret = app.clients.fusionauth.generate_mfa_secret().await?;
    // Generate a URI of the format: otpauth://TYPE/LABEL?PARAMETERS
    // TYPE: totp
    // LABEL: issuer:email (where issue is always SquadOV)
    // PARAMETERS: 
    //      - secret: Base32 encoded secret we get from FA.
    //      - issuer: Same issuer as the one in the label.
    let uri = format!(
        "otpauth://totp/SquadOV:{email}?secret={secret}&issuer=SquadOV",
        email=&session.user.email,
        secret=&secret.secret_base32_encoded,
    );

    #[derive(Serialize)]
    struct Response {
        secret: FusionAuthMfaSecret,
        uri: String
    }

    Ok(HttpResponse::Ok().json(Response{
        secret,
        uri
    }))
}

#[derive(Deserialize)]
pub struct DisableMfaQuery {
    code: String
}

pub async fn remove_2fa_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<DisableMfaQuery>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    // Find FA user to get their ID.
    let user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
        SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
    })?;

    // Disable MFA.
    if let Some(methods) = user.two_factor.methods.as_ref() {
        for m in methods {
            if m.method == "authenticator" {
                app.clients.fusionauth.disable_mfa(&user.id, &query.code, &m.id).await?;
            }
        }
    }

    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct EnableMfaInputs {
    code: String,
    secret: String,
}

pub async fn enable_2fa_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<EnableMfaInputs>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    // Find FA user to get their ID.
    let user = app.clients.fusionauth.find_user_from_email_address(&session.user.email).await.map_err(|x| {
        SquadOvError::InternalError(format!("Failed to find user from email address: {:?}", x))
    })?;

    // Enable MFA, obtain recovery codes and send back to the user.
    Ok(HttpResponse::Ok().json(
        app.clients.fusionauth.enable_mfa(&user.id, &data.code, &data.secret).await?
    ))
}