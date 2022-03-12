use actix_web::{HttpResponse, web, HttpRequest, HttpMessage};
use serde::Deserialize;
use crate::logged_error;
use crate::api;
use crate::api::fusionauth;
use crate::api::auth::SquadOVSession;
use squadov_common::SquadOvError;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ForgotPasswordInputs {
    #[serde(rename = "loginId")]
    login_id: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordInputs {
    #[serde(rename = "changePasswordId")]
    change_password_id: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct NewPasswordInputs {
    current_pw: String,
    new_pw: String,
}

async fn forgot_pw(fa: &fusionauth::FusionAuthClient, login_id: &str) -> Result<(), SquadOvError> {
    match fa.start_forgot_password_workflow(login_id).await {
        Ok(_) => Ok(()),
        Err(err) => match err {
            // Handle this case especially since we don't want to present to the caller data about whether or not that particular
            // user exists.
            fusionauth::FusionAuthResendVerificationEmailError::DoesNotExist => Ok(()),
            _ => Err(SquadOvError::InternalError(format!("Start Forgot Password Workflow: {}", err)))
        }
    }
}

async fn change_pw(fa: &fusionauth::FusionAuthClient, change_id: &str, password: &str) -> Result<(), SquadOvError> {
    match fa.change_user_password(change_id, password).await {
        Ok(_) => Ok(()),
        Err(err) => Err(SquadOvError::InternalError(format!("Change Password: {}", err))),
    }
}

/// Starts the password reset flow. Note that no error is given if
/// the specified user doesn't exist.
/// 
/// Possible Responses:
/// * 200 - Success.
/// * 500 - Internal error (no email sent).
pub async fn forgot_pw_handler(data : web::Query<ForgotPasswordInputs>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    match forgot_pw(&app.clients.fusionauth, &data.login_id).await {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(err) => logged_error!(err),
    }
}

/// Changes the user password given that they started the forgot password flow.
/// 
/// Possible Responses:
/// * 200 - Success.
/// * 500 - Internal error (password was not  changed).
pub async fn forgot_pw_change_handler(app : web::Data<Arc<api::ApiApplication>>, data : web::Json<ChangePasswordInputs>) -> Result<HttpResponse, SquadOvError> {
    match change_pw(&app.clients.fusionauth, &data.change_password_id, &data.password).await {
        Ok(_) => Ok(HttpResponse::NoContent().finish()),
        Err(err) => logged_error!(err),
    }
}

pub async fn change_pw_handler(app : web::Data<Arc<api::ApiApplication>>, data : web::Json<NewPasswordInputs>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    app.clients.fusionauth.change_user_password_with_id(&data.current_pw, &data.new_pw, &session.user.email).await?;
    Ok(HttpResponse::NoContent().finish())
}