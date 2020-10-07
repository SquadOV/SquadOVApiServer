use actix_web::{web, HttpRequest, HttpResponse};
use serde::{Serialize, Deserialize};
use crate::api;
use crate::api::fusionauth;
use crate::logged_error;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct LoginData {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    session_id: String
}

/// Authenticates the user with our backend and returns a session.
async fn login(fa: &fusionauth::FusionAuthClient, data: LoginData, ip: Option<&str>) -> Result<super::SquadOVSession, super::AuthError> {
    let res = fa.login(fa.build_login_input(
        data.username,
        data.password,
        ip,
    )).await;
    match res {
        Ok(result) => {
            let reg = fa.find_auth_registration(&result.user);
            match reg {
                Some(x) => Ok(super::SquadOVSession{
                    session_id: Uuid::new_v4().to_string(),
                    user: super::SquadOVUser{
                        id: -1, // Invalid ID is fine here - we'll grab it later.
                        username: match &x.username {
                            Some(y) => y.clone(),
                            None => String::from(""),
                        },
                        email: result.user.email,
                    },
                    access_token: result.token,
                    refresh_token: result.refresh_token,
                }),
                None => Err(super::AuthError::System{
                    message: String::from("Could not find user auth registration with the current app.")
                }),
            }
        },
        // TODO: Handle two factor errors/change password errors/email verification errors.
        Err(err) => match err {
            fusionauth::FusionAuthLoginError::Auth => Err(super::AuthError::Credentials),
            fusionauth::FusionAuthLoginError::Generic{code, message} => Err(super::AuthError::System {
                message: format!("Code: {} Message: {}", code, message)
            }),
            _ => Err(super::AuthError::System{ 
                message: String::from("Unhandled error.")
            })
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
pub async fn login_handler(data : web::Json<LoginData>, app : web::Data<api::ApiApplication>, req : HttpRequest) -> Result<HttpResponse, super::AuthError> {
    if super::is_logged_in(&req) {
        return logged_error!(super::AuthError::BadRequest);
    }

    // First authenticate with our backend and obtain a valid session.
    let conn = req.connection_info();
    let res = login(&app.clients.fusionauth, data.into_inner(), conn.realip_remote_addr()).await;
    if res.is_err() {
        let err = res.unwrap_err();
        return logged_error!(err);
    }

    let mut session = res.unwrap();

    // Ensure that the user is also being tracked by our own database.
    // If not, create a new user.
    let stored_user = match app.users.get_stored_user_from_email(&session.user.email, &app.pool).await {
        Ok(x) => match x {
            Some(y) => y,
            None => match app.users.create_user(&session.user, &app.pool).await {
                Ok(z) => z,
                Err(err) => return logged_error!(super::AuthError::System{
                    message: format!("Create User {}", err)
                })
            },
        },
        Err(err) => return logged_error!(super::AuthError::System{
            message: format!("Get User {}", err)
        })
    };
    session.user = stored_user;

    // Store this session in our database and ensure the user is made aware of which session they should
    // be echoing back to us so we can verify their session. It's the client's responsibility to store
    // the session ID securely.
    match app.session.store_session(&session, &app.pool).await {
        Ok(_) => Ok(HttpResponse::Ok().json(LoginResponse{session_id: session.session_id})),
        Err(err) =>  logged_error!(super::AuthError::System{
            message: format!("Store Session {}", err),
        })
    }
}