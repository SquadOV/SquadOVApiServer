use crate::{
    SquadOvError,
    discord::{
        DiscordConfig,
        DiscordUser,
        oauth::{
            self,
            DiscordOAuthToken,
        },
        self,
    },
};
use reqwest::{StatusCode, header};
use std::sync::Arc;
use async_std::sync::RwLock;
use sqlx::PgPool;

pub struct DiscordApiClient {
    config: DiscordConfig,
    token: RwLock<DiscordOAuthToken>,
    pool: Arc<PgPool>,
    user_id: i64,
}

const DISCORD_API_URL : &'static str = "https://discord.com/api/v8";

impl DiscordApiClient {
    pub fn new(config: DiscordConfig, token: DiscordOAuthToken, pool: Arc<PgPool>, user_id: i64) -> Self {
        Self {
            config,
            token: RwLock::new(token),
            pool,
            user_id,
        }
    }

    async fn create_http_client(&self) -> Result<reqwest::Client, SquadOvError> {
        let mut headers = header::HeaderMap::new();
        let access_token = format!("Bearer {}", {
            let tk = self.token.read().await;
            tk.access_token.clone()
        });
        headers.insert(header::AUTHORIZATION, header::HeaderValue::from_str(&access_token)?);
        Ok(reqwest::ClientBuilder::new()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?)
    }

    pub async fn refresh_user_access_token(&self) -> Result<(), SquadOvError> {
        let new_token = oauth::refresh_oauth_token(&self.config.client_id, &self.config.client_secret, &{
            let tk = self.token.read().await;
            tk.refresh_token.clone()
        }).await?;

        let mut tk = self.token.write().await;
        let old_access_key = tk.access_token.clone();
        tk.copy_from(&new_token);

        discord::db::update_access_refresh_tokens(&*self.pool, self.user_id, &old_access_key, &tk).await?;
        Ok(())
    }

    async fn execute(&self, f: Arc<dyn Fn(&reqwest::Client) -> reqwest::RequestBuilder + Send + Sync>) -> Result<reqwest::Response, SquadOvError> {
        let mut client = self.create_http_client().await?;
        let mut resp = f(&client).send().await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            log::info!("Unauthorized Discord Access [User] - Refreshing {}", resp.text().await?);
            match self.refresh_user_access_token().await {
                Ok(_) => (),
                Err(err) => {
                    log::warn!("Failed to refresh Discord access token: {:?}", err);
                    return Err(SquadOvError::Unauthorized);
                }
            }
            
            client = self.create_http_client().await?;
            resp = f(&client).send().await?;
        }
        Ok(resp)
    }

    pub async fn get_current_user(&self) -> Result<DiscordUser, SquadOvError> {
        let resp = {
            self.execute(Arc::new(move |client: &reqwest::Client| {
                client.get(
                    &format!(
                        "{}/users/@me",
                        DISCORD_API_URL,
                    )
                )
            })).await?
        };

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Get Current User Error Discord: {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<DiscordUser>().await?)
    }
}