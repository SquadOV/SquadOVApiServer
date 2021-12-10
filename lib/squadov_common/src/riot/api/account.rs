use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY},
    riot::{RiotAccount, RiotSummonerDto, RiotSummoner, RiotUserInfo},
};
use super::RiotApiTask;
use reqwest::{StatusCode};
use crate::riot::db;
use serde::Deserialize;
use chrono::{DateTime, Utc};

const RIOT_MAX_AGE_SECONDS: i64 = 86400; // 1 day

impl super::RiotApiHandler {
    pub async fn get_account_by_puuid(&self, puuid: &str) -> Result<RiotAccount, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint("americas", &format!("riot/account/v1/accounts/by-puuid/{}", puuid));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain Riot acount by PUUID {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<RiotAccount>().await?)
    }

    pub async fn get_active_shard_by_game_for_puuid(&self, game: &str, puuid: &str) -> Result<String, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint("americas", &format!("riot/account/v1/active-shards/by-game/{game}/by-puuid/{puuid}", game=game, puuid=puuid));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get active shard for game by puuid {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        #[derive(Deserialize)]
        struct ShardInfo {
            #[serde(rename="activeShard")]
            active_shard: String
        }

        let shard = resp.json::<ShardInfo>().await?;
        Ok(shard.active_shard)
    }

    pub async fn get_account_me(&self, access_token: &str) -> Result<RiotAccount, SquadOvError> {
        let client = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?;
        let endpoint = Self::build_api_endpoint("americas", "riot/account/v1/accounts/me");
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .bearer_auth(access_token)
            .send()
            .await?;

        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(SquadOvError::RateLimit);
        } else if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get Riot account using RSO {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        let mut account = resp.json::<RiotAccount>().await?;

        // There's a possibility that's the account name that we get back might have whitespace near the end?
        // Honestly it barely happens to maybe not an issue but it happened once and that's all that matters...
        account.game_name = account.game_name.map(|x| {
            x.trim().to_string()
        });

        Ok(account)
    }

    pub async fn get_summoner_me(&self, access_token: &str, region: &str) -> Result<Option<RiotSummoner>, SquadOvError> {
        let client = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?;
        let endpoint = Self::build_api_endpoint(region, "lol/summoner/v4/summoners/me");
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .bearer_auth(access_token)
            .send()
            .await?;

        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(SquadOvError::RateLimit);
        } else if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        } else if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get Riot summoner using RSO {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        let dto = resp.json::<RiotSummonerDto>().await?;
        Ok(Some(
            RiotSummoner{
                puuid: dto.puuid,
                account_id: Some(dto.account_id),
                summoner_id: Some(dto.id),
                summoner_name: Some(dto.name),
                last_backfill_lol_time: None,
                last_backfill_tft_time: None,
            }
        ))
    }

    pub async fn get_user_info(&self, access_token: &str) -> Result<RiotUserInfo, SquadOvError> {
        let client = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?;
        let endpoint = String::from("https://auth.riotgames.com/userinfo");
        let resp = client.get(&endpoint)
            .bearer_auth(access_token)
            .send()
            .await?;

        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(SquadOvError::RateLimit);
        } else if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get Riot account userinfo {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<RiotUserInfo>().await?)
    }
}

impl super::RiotApiApplicationInterface {
    pub async fn obtain_riot_account_from_puuid(&self, puuid: &str) -> Result<(), SquadOvError> {
        let account = self.api.get_account_by_puuid(puuid).await?;
        let mut tx = self.db.begin().await?;
        db::store_riot_account(&mut tx, &account).await?;
        tx.commit().await?;
        Ok(())
    }
    
    pub async fn request_riot_account_from_puuid(&self, puuid: &str) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.valorant_queue, serde_json::to_vec(&RiotApiTask::Account{puuid: String::from(puuid)})?, RABBITMQ_DEFAULT_PRIORITY, RIOT_MAX_AGE_SECONDS).await;
        Ok(())
    }

    pub async fn obtain_riot_account_from_access_token(&self, access_token: &str, refresh_token: &str, expiration: &DateTime<Utc>, user_id: i64) -> Result<RiotAccount, SquadOvError> {
        log::info!("Obtain Riot Account from Access Token for User: {}", user_id);
        // Check for the expiration of the access token using the passed in expiration date. If it is expired, use the refresh token to obtain a new access token.
        // Note that we use a 1 minute buffer here to guard against potential cases where the access token is valid when we check but no longer valid when we send the request.
        let (access_token, refresh_token, expiration) = if &(Utc::now() + chrono::Duration::minutes(1)) > expiration {
            let new_token = crate::riot::rso::refresh_authorization_code(&self.config.rso_client_id, &self.config.rso_client_secret, refresh_token).await?;
            (new_token.access_token.clone(), new_token.refresh_token.clone(), Utc::now() + chrono::Duration::seconds(new_token.expires_in.into()))
        } else {
            (access_token.to_string(), refresh_token.to_string(), expiration.clone())
        };

        let user_info = self.api.get_user_info(&access_token).await?;
        let account = self.api.get_account_me(&access_token).await?;
        let summoner = if let Some(cpid) = &user_info.cpid {
            match self.api.get_summoner_me(&access_token, cpid).await {
                Ok(x) => x,
                Err(err) => {
                    log::warn!("Failed to get my summoner: {:?}", err);
                    None
                }
            }
        } else {
            None
        };

        let mut tx = self.db.begin().await?;

        log::info!("\t...Storing account: {:?}#{:?} for {}", &account.game_name, &account.tag_line, user_id);
        db::store_riot_account(&mut tx, &account).await?;
        
        if let Some(s) = summoner {
            log::info!("\t...Storing summoner: {:?} for {}", &s.summoner_name, user_id);
            db::store_riot_summoner(&mut tx, &s).await?;
        }

        db::link_riot_account_to_user(&mut tx, &account.puuid, user_id).await?;
        db::store_rso_for_riot_account(&mut tx, &account.puuid, user_id, &access_token, &refresh_token, &expiration).await?;
        tx.commit().await?;

        // Now that we've linked our account we need to check if we have Valorant games for the user already stored for the given PUUID.
        // And then we need to cache the data for that so we can search things for the user's POV if necessary.
        let valorant_match_uuids = db::get_match_uuids_contains_puuid(&*self.db, &account.puuid).await?;
        for match_uuid in valorant_match_uuids {
            self.request_valorant_match_player_cache_data(&match_uuid, user_id).await?;
        }

        Ok(account)
    }
    
    pub async fn request_riot_account_from_access_token(&self, access_token: &str, refresh_token: &str, expiration: DateTime<Utc>, user_id: i64) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.rso_queue, serde_json::to_vec(&RiotApiTask::AccountMe{
            access_token: access_token.to_string(),
            refresh_token: refresh_token.to_string(),
            expiration,
            user_id,
        })?, RABBITMQ_DEFAULT_PRIORITY, RIOT_MAX_AGE_SECONDS).await;
        Ok(())
    }
}