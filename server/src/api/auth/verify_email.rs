use actix_web::{HttpResponse, HttpRequest, web, HttpMessage};
use serde::{Serialize,Deserialize};
use crate::api;
use crate::api::fusionauth;
use crate::logged_error;
use squadov_common;
use super::SquadOVSession;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct VerifyEmailData {
    #[serde(rename = "verificationId")]
    verification_id: String
}

#[derive(Serialize)]
pub struct CheckEmailVerifiedResponse {
    verified: bool
}

async fn verify_email(fa: &fusionauth::FusionAuthClient, verification_id: &str) -> Result<(), squadov_common::SquadOvError> {
    match fa.verify_email(verification_id).await {
        Ok(_) => Ok(()),
        Err(err) => Err(squadov_common::SquadOvError::InternalError(format!("Failed to verify email: {}", err))),
    }
}

async fn resend_verify_email(fa: &fusionauth::FusionAuthClient, email: &str) -> Result<(), squadov_common::SquadOvError> {
    match fa.resend_verify_email(email).await {
        Ok(_) => Ok(()),
        Err(err) => Err(squadov_common::SquadOvError::InternalError(format!("Failed to resend verification email: {}", err))),
    }
}

/// Verifies the user's email. This needs to
///  1) Communicate with the backend FusionAuth service to verify the email there.
///  2) Mark the user's email as being verified in the database.
/// 
/// Possible Responses:
/// * 200 - Email verification succeded.
/// * 500 - Email verification failed.
pub async fn verify_email_handler(data : web::Json<VerifyEmailData>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, squadov_common::SquadOvError> {
    // Get the user for this verification ID.
    // Note that we can't assume that the user is logged in here.
    let user = match app.clients.fusionauth.find_user_from_email_verification_id(&data.verification_id).await {
        Ok(u) => u,
        Err(err) => return logged_error!(squadov_common::SquadOvError::InternalError(format!("Failed to get user from verification ID: {}", err))),
    };

    verify_email(&app.clients.fusionauth, &data.verification_id).await?;

    // If we get to this point it means the verification was successful!
    // Make the user with the given email as being verified.
    match app.users.mark_user_email_verified_from_email(&user.email, &app.pool).await {
        Ok(_) => Ok(HttpResponse::Ok().finish()),
        Err(err) => logged_error!(squadov_common::SquadOvError::InternalError(format!("Mark User Email Verified {}", err))),
    }
}

/// Checks whether or not the user actually verified their email for the currently logged in user.
/// 
/// Possible Responses:
/// * 200 - Returns true/false depending on whether the user has verified their email.
/// * 401 - User not logged in.
/// * 500 - Could not check verification.
pub async fn check_verify_email_handler(req: HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return logged_error!(squadov_common::SquadOvError::Unauthorized),
    };

    Ok(HttpResponse::Ok().json(CheckEmailVerifiedResponse{
        verified: session.user.verified,
    }))
}

/// Resends the user verification email for the currently logged in user.
/// 
/// Possible Responses:
/// * 200 - Email sent.
/// * 401 - User not logged in.
/// * 500 - Email was not sent.
pub async fn resend_verify_email_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return logged_error!(squadov_common::SquadOvError::Unauthorized),
    };

    match resend_verify_email(&app.clients.fusionauth, &session.user.email).await {
        Ok(_) => Ok(HttpResponse::Ok().finish()),
        Err(err) => logged_error!(err),
    }
}