use crate::{
    SquadOvError,
    riot::{RiotSummoner, RiotSummonerDto}
};
use reqwest::{StatusCode};

impl super::RiotApiHandler {
    pub async fn get_tft_summoner_from_puuid(&self, puuid: &str, platform: &str) -> Result<RiotSummoner, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(platform, &format!("tft/summoner/v1/summoners/by-puuid/{}", puuid));
        self.tick_thresholds().await?;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain TFT summoner acount by PUUID {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        let summoner = resp.json::<RiotSummonerDto>().await?;
        Ok(RiotSummoner{
            puuid: summoner.puuid,
            account_id: Some(summoner.account_id),
            summoner_id: Some(summoner.id),
            summoner_name: Some(summoner.name),
            last_backfill_lol_time: None,
            last_backfill_tft_time: None,
        })
    }
}