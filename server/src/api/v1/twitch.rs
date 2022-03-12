use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    twitch::{
        api::TwitchSubscriptionAlt,
        eventsub::{
            self,
            TWITCH_CHANNEL_SUBSCRIBE,
            TWITCH_CHANNEL_UNSUB,
        },
    },
};
use crate::api::ApiApplication;
use std::sync::Arc;
use serde::Deserialize;
use sha2::Sha256;
use hmac::{Hmac, Mac, NewMac};

#[cfg(feature = "eventloop")]
use squadov_common::{
    twitch::{
        oauth,
    },
    accounts::twitch,
};

#[derive(Deserialize)]
pub struct TwitchEventSubSubscription {
    id: String,
    #[serde(rename="type")]
    sub_type: String,
}

#[derive(Deserialize)]
pub struct TwitchEventSubNotification {
    challenge: Option<String>,
    subscription: TwitchEventSubSubscription,
    // Type depends on the subscription in question
    event: Option<serde_json::Value>,
}

async fn handle_twitch_eventsub_challenge(app : web::Data<Arc<ApiApplication>>, challenge: String, raw_data: web::Bytes, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let headers = req.headers();
    let message_id = headers.get("Twitch-Eventsub-Message-Id").ok_or(SquadOvError::Forbidden)?.to_str()?;
    let message_timestamp =  headers.get("Twitch-Eventsub-Message-Timestamp").ok_or(SquadOvError::Forbidden)?.to_str()?;

    let mut mac = Hmac::<Sha256>::new_from_slice(app.config.squadov.hashid_salt.as_bytes())?;
    mac.update(message_id.as_bytes());
    mac.update(message_timestamp.as_bytes());
    mac.update(&raw_data);
    let ref_signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let test_signature = headers.get("Twitch-Eventsub-Message-Signature").ok_or(SquadOvError::Forbidden)?.to_str()?;

    if test_signature != ref_signature {
        return Err(SquadOvError::Forbidden);
    }
    
    // Keep track of subscription in the database just in case there's a day where we want to mass delete them all.
    let parsed: TwitchEventSubNotification = serde_json::from_slice(&raw_data)?;
    let raw: serde_json::Value = serde_json::from_slice(&raw_data)?;
    eventsub::insert_twitch_eventsub(&*app.pool, &parsed.subscription.id, &parsed.subscription.sub_type, raw).await?;

    Ok(HttpResponse::Ok().body(challenge))
}

impl ApiApplication {
    async fn handle_twitch_subscribe(&self, sub: TwitchSubscriptionAlt) -> Result<(), SquadOvError> {
        eventsub::store_twitch_subs(&*self.pool, &[sub.to_sub()]).await?;
        Ok(())
    }

    async fn handle_twitch_unsub(&self, sub: TwitchSubscriptionAlt) -> Result<(), SquadOvError> {
        eventsub::delete_twitch_sub(&*self.pool, &sub.to_sub()).await?;
        Ok(())
    }

    #[cfg(feature = "eventloop")]
    pub async fn reverify_twitch_account_access_tokens(&self) -> Result<(), SquadOvError> {
        let accounts = twitch::get_twitch_accounts_need_validation(&*self.pool).await?;
        for a in accounts {
            if oauth::validate_access_token(&a.access_token).await? {
                twitch::update_twitch_account_last_update(&*self.pool, &a.access_token).await?;
            } else {
                twitch::delete_twitch_account(&*self.pool, &a.access_token).await?;
            }
        }
        Ok(())
    }
}

pub async fn on_twitch_eventsub_handler(app : web::Data<Arc<ApiApplication>>, data: web::Bytes, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let parsed: TwitchEventSubNotification = serde_json::from_slice(&data)?;
    if let Some(challenge) = parsed.challenge {
        handle_twitch_eventsub_challenge(app, challenge.clone(), data, req).await
    } else {
        let message_type = req.headers().get("Twitch-Eventsub-Message-Type").ok_or(SquadOvError::BadRequest)?.to_str()?;
        if message_type != "notification" {
            return Err(SquadOvError::BadRequest);
        }

        if let Some(event) = parsed.event {
            match parsed.subscription.sub_type.as_str() {
                TWITCH_CHANNEL_SUBSCRIBE => {
                    app.handle_twitch_subscribe(serde_json::from_value::<TwitchSubscriptionAlt>(event)?).await?;
                    Ok(HttpResponse::NoContent().finish())
                },
                TWITCH_CHANNEL_UNSUB => {
                    app.handle_twitch_unsub(serde_json::from_value::<TwitchSubscriptionAlt>(event)?).await?;
                    Ok(HttpResponse::NoContent().finish())
                },
                _ => Err(SquadOvError::BadRequest)
            }
        } else {
            Err(SquadOvError::BadRequest)
        }
    }
}