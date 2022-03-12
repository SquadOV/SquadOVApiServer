use serde::Deserialize;
use crate::{
    SquadOvError,
    http::RateLimiter,
};
use url::Url;
use reqwest::{StatusCode};

#[derive(Clone, Deserialize, Debug)]
pub struct SteamApiConfig {
    pub api_key: String,
    pub requests: usize,
    pub seconds: u64,
}

pub struct SteamApiClient {
    config: SteamApiConfig,
    limiter: RateLimiter,
}

#[derive(Deserialize)]
pub struct GenericSteamApiResponse<T> {
    response: T,
}

#[derive(Deserialize)]
pub struct SteamPlayerSummaryResponse {
    players: Vec<SteamPlayerSummary>,
}

#[derive(Deserialize)]
pub struct SteamPlayerSummary {
    pub steamid: String,
    pub personaname: String,
    pub avatarfull: String
}

impl SteamApiClient {
    pub fn new(config: &SteamApiConfig) -> SteamApiClient {
        Self {
            config: config.clone(),
            limiter: RateLimiter::new(config.requests, config.seconds),
        }
    }

    fn build_url(&self, base: &str) -> Result<Url, SquadOvError> {
        let mut ret = Url::parse(base)?;
        ret.query_pairs_mut().append_pair("key", &self.config.api_key);
        Ok(ret)
    }

    fn create_http_client(&self) -> Result<reqwest::Client, SquadOvError> {
        Ok(reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?)
    }

    pub async fn get_player_summaries(&self, steam_ids: &[i64]) -> Result<Vec<SteamPlayerSummary>, SquadOvError> {
        self.limiter.consume().await?;

        let mut url = self.build_url("http://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002")?;
        url.query_pairs_mut().append_pair("steamids", &steam_ids.iter().map(|x| {
            format!("{}", x)
        }).collect::<Vec<String>>().join(","));

        let client = self.create_http_client()?;
        let resp = client.get(url.as_str())
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get Steam player summaries {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        let data = resp.json::<GenericSteamApiResponse<SteamPlayerSummaryResponse>>().await?;
        Ok(data.response.players)
    }
}