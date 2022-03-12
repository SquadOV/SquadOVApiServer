mod user_specific;
mod squad;
mod squad_invite;
mod riot;
mod vod;
mod wow;
mod share;
mod matches;
mod null;
mod aimlab;
mod hearthstone;
mod csgo;
mod community;
mod token;

pub use user_specific::*;
pub use squad::*;
pub use squad_invite::*;
pub use riot::*;
pub use vod::*;
pub use wow::*;
pub use share::*;
pub use matches::*;
pub use null::*;
pub use aimlab::*;
pub use hearthstone::*;
pub use csgo::*;
pub use community::*;
pub use token::*;

use squadov_common;
use actix_web::{web, HttpRequest};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::RefCell;
use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, Error, HttpMessage};
use futures::future::{ok, Ready};
use futures::Future;
use super::auth::SquadOVSession;
use crate::api::ApiApplication;
use std::sync::Arc;
use std::boxed::Box;
use async_trait::async_trait;
use std::collections::HashMap;

type TChecker<T> = Rc<RefCell<Box<dyn AccessChecker<T>>>>;

/// This trait is used by the access middleware to check to see
/// whether the current user has access to whatever the checker is
/// protecting.
#[async_trait]
pub trait AccessChecker<T: Send + Sync> {
    /// Checks whether or not the current request should have
    /// access to whatever path is being requested. This needs to
    /// be an instance method instead of a static method so that the
    /// instance can be made by the user to hold parameters specific
    /// to that path (e.g. checking whether the user has access to some
    /// resource specifically).
    async fn check(&self, app: Arc<ApiApplication>, session: Option<&SquadOVSession>, data: T) -> Result<bool, squadov_common::SquadOvError>;
    async fn post_check(&self, app: Arc<ApiApplication>, session: Option<&SquadOVSession>, data: T) -> Result<bool, squadov_common::SquadOvError>;
    fn generate_aux_metadata(&self, req: &HttpRequest) -> Result<T, squadov_common::SquadOvError>;
}

pub struct ApiAccess<T : Send + Sync> {
    // Default checker when no other matches exist.
    pub checker: TChecker<T>,
    // Checker to use for specific HTTP verbs
    pub verb_checkers: HashMap<String, TChecker<T>>,
    // Whether or not the session is mandatory
    pub mandatory_session: bool,
}

impl<T: Send + Sync> ApiAccess<T> {
    pub fn new(input: Box<dyn AccessChecker<T>>) -> Self {
        Self {
            checker: Rc::new(RefCell::new(input)),
            verb_checkers: HashMap::new(),
            mandatory_session: true,
        }
    }

    pub fn verb_override(mut self, v: &str, c: Box<dyn AccessChecker<T>>) -> Self {
        self.verb_checkers.insert(String::from(v), Rc::new(RefCell::new(c)));
        self
    }
}

impl<S, T> Transform<S, ServiceRequest> for ApiAccess<T>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
    T: Send + Sync + 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = ApiAccessMiddleware<S, T>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiAccessMiddleware { 
            rc_service: Rc::new(RefCell::new(service)),
            checker: self.checker.clone(),
            verb_checkers: self.verb_checkers.clone(),
            mandatory_session: self.mandatory_session,
        })
    }
}

pub struct ApiAccessMiddleware<S, T : Send + Sync> {
    rc_service: Rc<RefCell<S>>,
    checker: TChecker<T>,
    verb_checkers: HashMap<String, TChecker<T>>,
    mandatory_session: bool,
}

impl<S, T> Service<ServiceRequest> for ApiAccessMiddleware<S, T>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
    T: Send + Sync + 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.rc_service.borrow_mut().poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.rc_service.clone();
        let method = String::from(req.method().as_str());
        let checker = if self.verb_checkers.contains_key(&method) {
            self.verb_checkers.get(&method).unwrap().clone()
        } else {
            self.checker.clone()
        };
        let mandatory_session = self.mandatory_session;

        Box::pin(async move {
            let (request, payload) = req.into_parts();

            {
                // We assume that this middleware is used in conjunction with the api::auth::ApiSessionValidatorMiddleware
                // middleware so given that they're logged in, we can obtain their session.
                let extensions = request.extensions();
                let session = extensions.get::<SquadOVSession>();

                if mandatory_session && session.is_none() {
                    return Err(actix_web::error::ErrorUnauthorized("No session"));
                };

                let app = request.app_data::<web::Data<Arc<ApiApplication>>>();
                if app.is_none() {
                    return Err(actix_web::error::ErrorInternalServerError("No app data."));
                }

                let borrowed_checker = checker.borrow();

                // Obtain aux data from the request necessary for the checker to perform an access check.
                // This is necessary because HttpRequest is not send/sync so we can't pass it to an async call.
                let aux_data = borrowed_checker.generate_aux_metadata(&request)?;
                match checker.borrow().check(app.unwrap().get_ref().clone(), session, aux_data).await {
                    Ok(x) => if x { () } else {  return Err(actix_web::error::ErrorUnauthorized("Access check fail")) },
                    Err(_) => return Err(actix_web::error::ErrorInternalServerError("Failed to perform access check")),
                };
            }

            let resp = srv.call(ServiceRequest::from_parts(request, payload)).await?;

            // This is *NOT IDEAL*; however, for checkers that rely on the path, it has to go here since actix web doesn't
            // parse the path parameters beforehand.
            {
                // We assume that this middleware is used in conjunction with the api::auth::ApiSessionValidatorMiddleware
                // middleware so given that they're logged in, we can obtain their session.
                let extensions = resp.request().extensions();
                let session = extensions.get::<SquadOVSession>();

                let app = resp.request().app_data::<web::Data<Arc<ApiApplication>>>();
                if app.is_none() {
                    return Err(actix_web::error::ErrorInternalServerError("No app data."));
                }

                let borrowed_checker = checker.borrow();

                // Obtain aux data from the request necessary for the checker to perform an access check.
                // This is necessary because HttpRequest is not send/sync so we can't pass it to an async call.
                let aux_data = borrowed_checker.generate_aux_metadata(&resp.request())?;
                match checker.borrow().post_check(app.unwrap().get_ref().clone(), session, aux_data).await {
                    Ok(x) => if x { () } else {  return Err(actix_web::error::ErrorUnauthorized("Access [post] check fail")) },
                    Err(_) => return Err(actix_web::error::ErrorInternalServerError("Failed to perform [post] access check")),
                };
            }

            Ok(resp)
        })
    }
}