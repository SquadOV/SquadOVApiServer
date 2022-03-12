mod user;
mod login;
mod register;
mod forgot_password;
mod verify_email;
mod logout;
mod session;
mod mfa;

pub use user::*;
pub use login::*;
pub use register::*;
pub use forgot_password::*;
pub use verify_email::*;
pub use logout::*;
pub use session::*;
pub use mfa::*;

use squadov_common::SquadOvError;
use std::sync::Arc;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::RefCell;
use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, Error, web, HttpMessage};
use futures::future::{ok, Ready};
use futures::Future;
use chrono::Utc;

/// Session validation middleware.
/// 
/// This middleware will automatically process the request, determine the
/// session, and refresh the session access token if necessary. This middleware
/// will abort the request with an unauthorized response if no session is found
/// or if an invalid session is passed in.
pub struct ApiSessionValidator {
    pub required: bool,
}

impl<S> Transform<S, ServiceRequest> for ApiSessionValidator
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = ApiSessionValidatorMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiSessionValidatorMiddleware { 
            rc_service: Rc::new(RefCell::new(service)),
            required: self.required,
        })
    }
}

pub struct ApiSessionValidatorMiddleware<S> {
    rc_service: Rc<RefCell<S>>,
    required: bool,
}

impl<S> Service<ServiceRequest> for ApiSessionValidatorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.rc_service.borrow_mut().poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.rc_service.clone();
        let required = self.required;

        Box::pin(async move {
            let (request, payload) = req.into_parts();

            // The docs say to use app::data::<web::Data<T>> when we store the data using
            // App::data...but we're using App::app_data but if we don't retrieve a web::Data the
            // Option return none. /shrug.
            let app = match request.app_data::<web::Data<Arc<crate::api::ApiApplication>>>() {
                Some(x) => x,
                None => return Err(actix_web::error::ErrorInternalServerError("Bad App Data")),
            };

            // We need to check if there's an access token also attached to the request.
            // In the case that there is one and it's not expired (!) then we want to augment
            // the use the access token as the session. If no access token exists, then we use the
            // share token, otherwise it's the logged in user session.
            let share_token = app.session.get_share_token_from_request(&request, &app.config.squadov.share_key).await?;
            let access_token = app.session.get_access_token_from_request(&request, &app.config.squadov.access_key).await?;
            let some_session = if let Some(access_token) = access_token {
                let is_expired = if let Some(expires) = access_token.expires {
                    Utc::now() > expires
                } else {
                    false
                };

                if !is_expired {
                    Some(SquadOVSession{
                        session_id: String::new(),
                        // We do want this to fail if the access token's user id is not set. It's legacy behavior what can you do.
                        user: app.users.get_stored_user_from_id(access_token.user_id.unwrap_or(-1), &*app.pool).await?.ok_or(SquadOvError::NotFound)?,
                        access_token: String::new(),
                        refresh_token: String::new(),
                        is_temp: true,
                        share_token: None,
                        sqv_access_token: Some(access_token),
                    })
                } else {
                    return Err(actix_web::error::ErrorUnauthorized("Expired access token"));
                }
            } else if let Some(share_token) = share_token {
                Some(SquadOVSession{
                    session_id: String::new(),
                    user: app.users.get_stored_user_from_uuid(&share_token.user_uuid, &*app.pool).await?.ok_or(SquadOvError::NotFound)?,
                    access_token: String::new(),
                    refresh_token: String::new(),
                    is_temp: true,
                    share_token: Some(share_token),
                    sqv_access_token: None,
                })
            } else {
                match app.session.get_session_from_request(&request, &app.pool).await {
                    Ok(x) => x,
                    Err(_) => return Err(actix_web::error::ErrorInternalServerError("Could not retrieve session")),
                }
            };

            if let Some(session) = some_session {
                match app.is_session_valid(&session).await {
                    Ok(b) => {
                        if !b {
                            return Err(actix_web::error::ErrorUnauthorized("Invalid session"))
                        }
                    }
                    Err(_) => return Err(actix_web::error::ErrorInternalServerError("Could not check valid session")),
                };

                {
                    let mut extensions = request.extensions_mut();
                    extensions.insert(session);
                }
            } else if required {
                return Err(actix_web::error::ErrorUnauthorized("No session found when required."));
            }

            Ok(srv.call(ServiceRequest::from_parts(request, payload)).await?)
        })
    }
}