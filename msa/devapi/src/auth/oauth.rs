use std::pin::Pin;
use std::task::{Context, Poll};
use std::rc::Rc;
use std::cell::RefCell;
use std::sync::Arc;
use serde::{Serialize, Deserialize};

use actix_service::{Service, Transform};
use actix_web::{dev::ServiceRequest, dev::ServiceResponse, Error, HttpResponse, http, body::EitherBody, Result, web, cookie::{SameSite, Cookie}};
use futures::future::{ok, Ready};
use futures::Future;
use crate::shared::{DevApiConfig, SharedApp};

pub struct OAuth {
    pub config: DevApiConfig,
}

// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S, B> Transform<S, ServiceRequest> for OAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = OAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(OAuthMiddleware {
            service: Rc::new(RefCell::new(service)),
            config: self.config.clone(),
        })
    }
}

pub struct OAuthMiddleware<S> {
    service: Rc<RefCell<S>>,
    config: DevApiConfig,
}

impl<S, B> Service<ServiceRequest> for OAuthMiddleware<S>
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
        let config = self.config.clone();

        Box::pin(async move {
            // Check for the JWT cookie. If it exists, validate it.
            // If it doesn't exist or if the JWT is invalid, force the user to login again.
            let is_authenticated = if let Some(cookie) = req.cookie("squadovDevApiJwt") {
                let jwt_value = cookie.value();

                // Validate the JWT with FusionAuth at the /api/jwt/validate endpoint.
                let client = reqwest::ClientBuilder::new().build().unwrap();
                let endpoint = format!("{}/api/jwt/validate", &config.fa_url);
                
                match client.get(&endpoint).header("Authorization", format!("Bearer {}", jwt_value)).send().await {
                    Ok(resp) => resp.status() == reqwest::StatusCode::OK,
                    Err(err) => {
                        log::warn!("Failed to validate with FusionAuth: {:?}", err);
                        false
                    }
                }
            } else {
                false
            };
            
            if is_authenticated {
                let res = srv.call(req).await?;
                Ok(res.map_into_left_body())
            } else {
                Ok(req.into_response(
                    HttpResponse::TemporaryRedirect()
                        .append_header((
                            http::header::LOCATION,
                            format!(
                                "{fa}/oauth2/authorize?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&tenantId={tenant_id}&scope=openid",
                                fa=&config.fa_url,
                                client_id=&config.fa_client_id,
                                redirect_uri=format!("{}/oauth", &config.self_url()),
                                tenant_id=&config.fa_tenant_id,
                            ).as_str(),
                        ))
                        .finish()
                        .map_into_right_body()
                ))
            }
        })
    }
}

#[derive(Deserialize)]
pub struct OauthRequest {
    code: String,
}

pub async fn oauth_handler(query: web::Query<OauthRequest>, app: web::Data<Arc<SharedApp>>) -> Result<HttpResponse> {
    // Exchange the auth code to get a JWT token and then store that token in a cookie.
    let client = reqwest::ClientBuilder::new().build().unwrap();
    let endpoint = format!("{}/oauth2/token", &app.config.fa_url);

    #[derive(Serialize)]
    pub struct TokenExchangeRequest {
        client_id: String,
        client_secret: String,
        code: String,
        grant_type: String,
        redirect_uri: String,
    }

    let resp = client.post(&endpoint)
        .form(&TokenExchangeRequest{
            client_id: app.config.fa_client_id.clone(),
            client_secret: app.config.fa_client_secret.clone(),
            code: query.code.clone(),
            grant_type: String::from("authorization_code"),
            redirect_uri: format!("{}/oauth", &app.config.self_url()),
        })
        .send()
        .await.unwrap();

    #[derive(Deserialize)]
    pub struct ExchangeResponse {
        access_token: String
    }

    Ok(
        if resp.status() == reqwest::StatusCode::OK {
            let data = resp.json::<ExchangeResponse>().await.unwrap();
            HttpResponse::TemporaryRedirect()
                .cookie(
                    Cookie::build("squadovDevApiJwt", &data.access_token)
                        .secure(app.config.secure())
                        .http_only(app.config.secure())
                        .same_site(SameSite::Strict)
                        .finish()
                )
                .append_header((
                    http::header::LOCATION,
                    format!(
                        "{}/dashboard/",
                        &app.config.self_url(),
                    ).as_str(),
                ))
                .finish()
        } else {
            HttpResponse::Forbidden().finish()
        }
    )
}