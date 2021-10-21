use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Serialize, Deserialize};
use crate::api;
use crate::api::fusionauth;
use squadov_common::{
    SquadOvError,
    profile,
};
use crate::api::auth::SquadOVSession;
use crate::logged_error;
use uuid::Uuid;
use std::sync::Arc;
use chrono::{DateTime, Utc, NaiveDateTime};
use convert_case::{Case, Casing};

#[derive(Deserialize)]
pub struct LoginData {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct MfaLoginData {
    id: String,
    code: String,
}

#[derive(Serialize)]
struct LoginResponse {
    #[serde(rename = "userId")]
    user_id: i64,
    #[serde(rename = "sessionId")]
    session_id: String,
    verified: bool,
    #[serde(rename = "twoFactor")]
    two_factor: Option<String>,
}

impl api::ApiApplication {
    async fn generic_login_from_fusionauth(&self, result: fusionauth::FusionAuthLoginResult) -> Result<super::SquadOVSession, SquadOvError> {
        let reg = self.clients.fusionauth.find_auth_registration(&result.user);
        let mut session = match reg {
            Some(x) => super::SquadOVSession{
                session_id: Uuid::new_v4().to_string(),
                user: super::SquadOVUser{
                    id: -1, // Invalid ID is fine here - we'll grab it later.
                    username: match &x.username {
                        Some(y) => y.clone(),
                        None => String::from(""),
                    },
                    email: result.user.email.clone(),
                    verified: result.user.verified,
                    uuid: Uuid::nil(), // We'll pull this later along with the id.
                    is_test: false,
                    is_admin: false,
                    welcome_sent: false,
                    registration_time: Some(DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(x.insert_instant / 1000, 0), Utc)),
                },
                access_token: result.token,
                refresh_token: result.refresh_token,
                is_temp: false,
                share_token: None,
                sqv_access_token: None,
            },
            None => return Err(SquadOvError::InternalError(String::from("Could not find user auth registration with the current app."))),
        };

        // Ensure that the user is also being tracked by our own database.
        // If not, create a new user.
        let mut did_create_user = false;
        let stored_user = match self.users.get_stored_user_from_email(&session.user.email, &self.pool).await {
            Ok(x) => match x {
                Some(y) => y,
                None => {
                    did_create_user = true;
                    let user = match self.users.create_user(&session.user, &self.pool).await {
                        Ok(z) => z,
                        Err(err) => return Err(SquadOvError::InternalError(format!("Create User {}", err))),
                    };

                    // Check for any pending squad invites and apply them.
                    self.associate_pending_invites_to_user(&user.email, user.id).await?;

                    // Create a default squad for this user and create their default profile using a slug created from their username.
                    let mut tx = self.pool.begin().await?;
                    self.create_default_squad(&mut tx, &user).await?;
                    profile::create_user_profile_for_user_id(
                        &mut tx,
                        user.id,
                        &format!(
                            "{}-{}",
                            petname::Petnames::large().generate_one(3, "-").to_case(Case::Pascal),
                            user.uuid.to_hyphenated().to_string().split("-").collect::<Vec<&str>>()[0],
                        ),
                    ).await?;
                    tx.commit().await?;

                    user
                },
            },
            Err(err) => return Err(SquadOvError::InternalError(format!("Get User {}", err))),
        };

        // Need to do a preliminary identify on login. Empty IP/Anonymous ID so that we only
        // fill out some basic information (email, primarily, for Vero).
        self.analytics_identify_user(&stored_user, "", "").await?;

        // If we just created the user, then we also need to mark the user as just having done the "register" event.
        if did_create_user {
            self.segment.track(&stored_user.uuid.to_string(), "registered").await?;
        }

        session.user = stored_user;
        // Store this session in our database and ensure the user is made aware of which session they should
        // be echoing back to us so we can verify their session. It's the client's responsibility to store
        // the session ID and echo it back to us (since we're kinda assuming the lack of cookies because of Electron).
        self.session.store_session(&*self.pool, &session).await?;
        Ok(session)
    }
}

/// Authenticates the user with our backend and returns a session.
async fn login(fa: &fusionauth::FusionAuthClient, data: LoginData, ip: Option<&str>) -> Result<fusionauth::FusionAuthLoginResult, SquadOvError> {
    let res = fa.login(fa.build_login_input(
        data.username,
        data.password,
        ip,
    )).await;
    match res {
        Ok(result) => Ok(result),
        // TODO: Handle two factor errors/change password errors/email verification errors.
        Err(err) => match err {
            fusionauth::FusionAuthLoginError::Auth => Err(SquadOvError::Credentials),
            fusionauth::FusionAuthLoginError::Generic{code, message} => Err(SquadOvError::InternalError(format!("Code: {} Message: {}", code, message))),
            fusionauth::FusionAuthLoginError::TwoFactor(two_factor_id) => Err(SquadOvError::TwoFactor(two_factor_id)),
            _ => Err(SquadOvError::InternalError(String::from("Unhandled error."))),
        }
    }
}

/// Handles taking the user's login request, passing it to FusionAuth and returning a response.
/// 
/// We expect only two parameters to be passed via the POST body: 
/// * Username
/// * Password
/// This function will login the user with FusionAuth. If that's successful, it'll also login the user
/// with SquadOV for session tracking.
///
/// Possible Responses:
/// * 200 - Login succeeded.
/// * 400 - If a user is already logged in.
/// * 401 - Login failed due to bad credentials.
/// * 500 - Login failed due to other reasons.
pub async fn login_handler(data : web::Json<LoginData>, app : web::Data<Arc<api::ApiApplication>>, req : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    if app.is_logged_in(&req).await? {
        return logged_error!(SquadOvError::BadRequest);
    }

    // First authenticate with our backend and obtain a valid session.
    let conn = req.connection_info();
    let login_result = match login(&app.clients.fusionauth, data.into_inner(), conn.realip_remote_addr()).await {
        Ok(x) => x,
        Err(err) => match err {
            SquadOvError::TwoFactor(two_factor_id) => return Ok(HttpResponse::Ok().json(LoginResponse{
                user_id: 0,
                session_id: String::new(),
                verified: false,
                two_factor: Some(two_factor_id),
            })),
            _ => return Err(err),
        }
    };
    let session = app.generic_login_from_fusionauth(login_result).await?;
    Ok(HttpResponse::Ok().json(LoginResponse{
        user_id: session.user.id,
        session_id: session.session_id,
        verified: session.user.verified,
        two_factor: None,
    }))
}

pub async fn mfa_login_handler(data : web::Json<MfaLoginData>, app : web::Data<Arc<api::ApiApplication>>, req : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    if app.is_logged_in(&req).await? {
        return Err(SquadOvError::BadRequest);
    }

    let login_result = app.clients.fusionauth.mfa_login(&data.id, &data.code).await?;
    let session = app.generic_login_from_fusionauth(login_result).await?;
    Ok(HttpResponse::Ok().json(LoginResponse{
        user_id: session.user.id,
        session_id: session.session_id,
        verified: session.user.verified,
        two_factor: None,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct VerifyPasswordData {
    password: String
}

pub async fn verify_pw_handler(app : web::Data<Arc<api::ApiApplication>>, data : web::Json<VerifyPasswordData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let conn = req.connection_info();
    let _ = login(&app.clients.fusionauth, LoginData{
        username: session.user.email.clone(),
        password: data.password.clone(),
    }, conn.realip_remote_addr()).await?;

    Ok(HttpResponse::NoContent().finish())
}