use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;
use uuid::Uuid;

use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, Error, HttpResponse, Result, body::EitherBody};
use futures::future::{ok, Ready};
use futures::Future;
use crate::shared::{SharedApp};

pub struct ApiAuth {
    pub app: Arc<SharedApp>,
}

impl<S, B> Transform<S, ServiceRequest> for ApiAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = ApiAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(ApiAuthMiddleware {
            service: Rc::new(RefCell::new(service)),
            app: self.app.clone(),
        })
    }
}

pub struct ApiAuthMiddleware<S> {
    service: Rc<RefCell<S>>,
    app: Arc<SharedApp>,
}

impl<S, B> Service<ServiceRequest> for ApiAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let srv = self.service.clone();
        let app = self.app.clone();

        Box::pin(async move {
            let is_auth = if let Some(api_key) = req.headers().get("x-squadov-api-key") {
                sqlx::query!(
                    r#"
                    SELECT EXISTS(
                        SELECT 1
                        FROM squadov.devapi_keys
                        WHERE api_key = $1
                    ) AS "val!"
                    "#,
                    api_key.to_str().unwrap().parse::<Uuid>().unwrap()
                )
                    .fetch_one(&*app.pool)
                    .await
                    .ok()
                    .map(|x| { x.val })
                    .unwrap_or(false)
            } else {
                false
            };

            Ok(
                if is_auth {
                    let res = srv.call(req).await?;
                    res.map_into_left_body()
                } else {
                    req.into_response(HttpResponse::Forbidden().finish().map_into_right_body())
                }
            )
        })
    }
}