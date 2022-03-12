use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::RefCell;
use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, Error, HttpMessage};
use futures::future::{ok, Ready};
use futures::Future;
use std::boxed::Box;
use crate::api::{
    auth::SquadOVSession,
};

pub struct ApiAccessToken {
    pub is_optional: bool
}

impl ApiAccessToken {
    pub fn new() -> Self {
        Self {
            is_optional: false,
        }
    }

    pub fn make_optional(mut self) -> Self {
        self.is_optional = true;
        self
    }
}

impl<S> Transform<S, ServiceRequest>  for ApiAccessToken
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = ApiAccessTokenMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiAccessTokenMiddleware { 
            service: Rc::new(RefCell::new(service)),
            optional: self.is_optional,
        })
    }
}

pub struct ApiAccessTokenMiddleware<S> {
    service: Rc<RefCell<S>>,
    optional: bool,
}

impl<S> Service<ServiceRequest> for ApiAccessTokenMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.borrow_mut().poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();
        let is_optional = self.optional;

        Box::pin(async move {
            let (request, payload) = req.into_parts();
            
            {
                // We assume that this middleware is used in conjunction with the api::auth::ApiSessionValidatorMiddleware
                // middleware so given that they're logged in, we can obtain their session.
                let extensions = request.extensions();
                let session = extensions.get::<SquadOVSession>();

                if let Some(session) = session {
                    if let Some(access_token) = &session.sqv_access_token {
                        if !access_token.check_method(request.method().as_str()) {
                            return Err(actix_web::error::ErrorUnauthorized("Access token method failure."));
                        }

                        if !access_token.check_path(request.path()) {
                            return Err(actix_web::error::ErrorUnauthorized("Access token path failure."));
                        }
                    }
                } else if !is_optional {
                    return Err(actix_web::error::ErrorUnauthorized("No access token."));
                }
            }

            Ok(srv.call(ServiceRequest::from_parts(request, payload)).await?)
        })
    }
}