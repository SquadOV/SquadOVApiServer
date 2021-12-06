use crate::{
    SquadOvError
};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Duration};

#[derive(Deserialize, Clone)]
pub struct DiscordOAuthToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    pub expires_in: i32,
}

impl DiscordOAuthToken {
    pub fn copy_from(&mut self, other: &DiscordOAuthToken) {
        self.access_token = other.access_token.clone();
        self.refresh_token = other.refresh_token.clone();
        self.expires_in = other.expires_in;
    }

    pub fn expiration_time(&self) -> DateTime<Utc> {
        // Subtract a couple minutes to give us a buffer to ensure there isn't some weird timing issue
        // where we technically used more than 0 seconds between when the token was issued to now causing
        // there technically to be less than 3600 (for example) seconds left. 
        Utc::now() + Duration::seconds(self.expires_in as i64 - 120)
    }
}

const TOKEN_URL: &'static str = "https://discord.com/api/v8/oauth2/token";

pub async fn exchange_authorization_code_for_access_token(client_id: &str, client_secret: &str, redirect_url: &str, code: &str) -> Result<DiscordOAuthToken, SquadOvError> {
    #[derive(Serialize)]
    pub struct Body<'a> {
        grant_type: &'a str,
        code: &'a str,
        client_id: &'a str,
        client_secret: &'a str,
        redirect_uri: &'a str,
    }
    
    let client = reqwest::ClientBuilder::new().build()?;
    let result = client
        .post(TOKEN_URL)
        .form(&Body{
            grant_type: "authorization_code",
            code,
            client_id,
            client_secret,
            redirect_uri: redirect_url,
        })
        .send()
        .await?;
    
    let status = result.status().as_u16();
    if status != 200 {
        return Err(SquadOvError::InternalError(format!("Failed to exchange auth code Discord [{}]: {}", status, result.text().await?)));
    }

    Ok(result.json::<DiscordOAuthToken>().await?)
}

pub async fn refresh_oauth_token(client_id: &str, client_secret: &str, refresh_token: &str) -> Result<DiscordOAuthToken, SquadOvError> {        
    #[derive(Serialize)]
    pub struct Body<'a> {
        grant_type: &'a str,
        refresh_token: &'a str,
        client_id: &'a str,
        client_secret: &'a str,
    }

    let client = reqwest::ClientBuilder::new().build()?;
    let result = client
        .post(TOKEN_URL)
        .form(&Body{
            grant_type: "refresh_token",
            refresh_token,
            client_id,
            client_secret
        })
        .send()
        .await?;
    
    let status = result.status().as_u16();
    if status != 200 {
        return Err(SquadOvError::InternalError(format!("Failed to exchange refresh oauth token Discord [{}]: {}", status, result.text().await?)));
    }

    Ok(result.json::<DiscordOAuthToken>().await?)
}
