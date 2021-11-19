mod login;
mod register;
mod user;
mod jwt;
mod mfa;

use serde::{Deserialize};
use reqwest::header;
use squadov_common::config::CommonConfig;

#[derive(Deserialize,Debug,Clone)]
pub struct FusionAuthConfig {
    host: String,
    port: u16,
    api_key: String,
    tenant_id: String,
    application_id: String,   
}

impl CommonConfig for FusionAuthConfig {
    fn read_from_env(&mut self) {
        if let Ok(host) = std::env::var("SQUADOV_FUSIONAUTH_HOST") {
            self.host = host;
        }

        if let Ok(port) = std::env::var("SQUADOV_FUSIONAUTH_PORT") {
            self.port = port.parse::<u16>().unwrap_or(self.port);
        }
    }
}

pub struct FusionAuthClient {
    cfg: FusionAuthConfig,
    client: reqwest::Client,
}

impl FusionAuthClient {
    pub fn new(cfg : FusionAuthConfig) -> FusionAuthClient {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, header::HeaderValue::from_str(&cfg.api_key).unwrap());
        // Unsure if this is actually used across all APIs but probably safe to set anyway.
        headers.insert("X-FusionAuth-TenantId", header::HeaderValue::from_str(&cfg.tenant_id).unwrap());

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        return FusionAuthClient{
            cfg: cfg,
            client: client,
        }
    }

    fn build_url(&self, endpoint : &str) -> String {
        return format!("{}:{}{}", self.cfg.host, self.cfg.port, endpoint);
    }
}

pub use login::*;
pub use register::*;
pub use user::*;
pub use jwt::*;
pub use mfa::*;