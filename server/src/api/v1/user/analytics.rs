use squadov_common::{
    SquadOvError,
    SquadOvGames,
    hardware::{
        Hardware,
        store_hardware_for_user,
        get_hardware_for_user,
    },
    ipstack::{
        LocationData,
    },
    segment::{
        ServerUserIdentifyTraits,
        ServerUserAddressTraits,
    },
};
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api::{
    ApiApplication,
    auth::{
        SquadOVSession,
        SquadOVUser,
    }
};
use std::collections::HashMap;
use std::sync::Arc;
use std::net::{IpAddr};
use std::str::FromStr;
use std::convert::TryFrom;
use serde::{Deserialize};
use ipnetwork::IpNetwork;

impl ApiApplication {
    async fn get_cached_ip_location_data(&self, ip: &IpAddr) -> Result<Option<LocationData>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                LocationData,
                "
                SELECT city, country, timezone, cache_tm
                FROM squadov.geo_ip_cache
                WHERE ip_addr >>= $1
                ",
                IpNetwork::new(
                    ip.clone(),
                    match ip {
                        &IpAddr::V4(_) => 24,
                        &IpAddr::V6(_) => 48,
                    }
                )?,
            )
                .fetch_optional(&*self.pool)
                .await?
        )
    }

    async fn store_cached_ip_location_data(&self, ip: &IpAddr, data: Option<LocationData>, update: bool) -> Result<(), SquadOvError> {
        let ip = IpNetwork::new(
            ip.clone(),
            match ip {
                &IpAddr::V4(_) => 24,
                &IpAddr::V6(_) => 48,
            }
        )?;

        if let Some(d) = data {
            if update {
                sqlx::query!(
                    "
                    UPDATE squadov.geo_ip_cache
                    SET cache_tm = NOW(),
                        city = $2,
                        country = $3,
                        timezone = $4
                    WHERE ip_addr >>= $1
                    ",
                    ip,
                    d.city,
                    d.country,
                    d.timezone,
                )
                    .execute(&*self.pool)
                    .await?;
            } else {
                sqlx::query!(
                    "
                    INSERT INTO squadov.geo_ip_cache (
                        ip_addr,
                        city,
                        country,
                        timezone,
                        cache_tm
                    )
                    VALUES (
                        $1,
                        $2,
                        $3,
                        $4,
                        NOW()
                    )
                    ",
                    ip,
                    d.city,
                    d.country,
                    d.timezone,
                )
                    .execute(&*self.pool)
                    .await?;
            }
        } else if !update {
            sqlx::query!(
                "
                INSERT INTO squadov.geo_ip_cache (
                    ip_addr,
                    cache_tm
                )
                VALUES (
                    $1,
                    NOW()
                )
                ",
                ip,
            )
                .execute(&*self.pool)
                .await?;
        } else {
            sqlx::query!(
                "
                UPDATE squadov.geo_ip_cache
                SET cache_tm = NOW()
                WHERE ip_addr >>= $1
                ",
                ip
            )
                .execute(&*self.pool)
                .await?;
        }
        
        Ok(())
    }

    async fn retrieve_ip_location_data(&self, ip: &IpAddr, update: bool) -> Result<Option<LocationData>, SquadOvError> {
        // Otherwise query the web API to get city/country and timezone.
        // Then store an anonymized version of the IP address in the database.
        let ret = match self.ip.get_location_data(ip).await {
            Ok(x) => Some(x),
            Err(err) => {
                // Don't actually want this to be a failure.
                log::warn!("Failed to get IP address info: {:?}", err);
                None
            }
        };

        self.store_cached_ip_location_data(ip, ret.clone(), update).await?;
        Ok(ret)
    }

    pub async fn analytics_identify_user(&self, user: &SquadOVUser, ip_addr: &str, anon_id: &str) -> Result<(), SquadOvError> {
        let loc_data: Option<LocationData> = if !ip_addr.is_empty() {
            let parsed_ip = IpAddr::from_str(ip_addr)?;

            if parsed_ip.is_loopback() {
                None
            } else {
                // Go from IP address to city, country, timezone.
                if let Some(data) = self.get_cached_ip_location_data(&parsed_ip).await? {
                    if data.expired() {
                        self.retrieve_ip_location_data(&parsed_ip, true).await?
                    } else {
                        // Check database cache for the IP address to get city/country and timezone.
                        Some(data)
                    }
                } else { 
                    self.retrieve_ip_location_data(&parsed_ip, false).await?
                }
            }
        } else {
            None
        };

        // Get user hardware information.
        let hardware = get_hardware_for_user(&*self.pool, user.id).await?;

        // Get traits about squads (# of squads, # of friends in squads with them).
        let squad_traits = sqlx::query!(
            r#"
            SELECT
                COUNT(DISTINCT sra.squad_id) AS "squads!",
                COUNT(DISTINCT ora.user_id) AS "friends!"
            FROM squadov.squad_role_assignments AS sra
            LEFT JOIN squadov.squad_role_assignments AS ora
                ON ora.squad_id = sra.squad_id
            WHERE sra.user_id = $1
            "#,
            user.id,
        )
            .fetch_one(&*self.pool)
            .await?;

        // Get squad invite usage (# of manual invites, # of times invite link has been used).
        let squad_invites = sqlx::query!(
            r#"
            SELECT (
                SELECT COUNT(invite_uuid)
                FROM squadov.squad_membership_invites
                WHERE inviter_user_id = $1
            ) AS "invites!", (
                SELECT COUNT(slu.user_id)
                FROM squadov.squad_invite_links AS sil
                LEFT JOIN squadov.squad_invite_link_usage AS slu
                    ON slu.link_id = sil.id
                WHERE sil.user_id = $1
            ) AS "links!"
            "#,
            user.id,
        )
            .fetch_one(&*self.pool)
            .await?;

        // Get traits about referrals (# of times the referral has been used).
        let referral_count = sqlx::query!(
            r#"
            SELECT COUNT(DISTINCT email) AS "count!"
            FROM squadov.referral_codes AS rc
            INNER JOIN squadov.user_referral_code_usage AS ucu
                ON ucu.code_id = rc.id
            WHERE rc.user_id = $1
            "#,
            user.id
        )
            .fetch_one(&*self.pool)
            .await?
            .count;

        // Get social traits about user (# of likes they've done, # of comments they given).
        let clip_reacts = sqlx::query!(
            r#"
            SELECT (
                SELECT COUNT(*)
                FROM squadov.clip_reacts
                WHERE user_id = $1
            ) AS "likes!", (
                SELECT COUNT(*)
                FROM squadov.clip_comments
                WHERE user_id = $1
            ) AS "comments!"
            "#,
            user.id
        )
            .fetch_one(&*self.pool)
            .await?;

        // Get traits about number of games [with VODs] user played (# aimlab, # wow, etc.)
        let mut per_game_vods = sqlx::query!(
            r#"
            SELECT m.game AS "game!", COUNT(DISTINCT v.video_uuid) AS "count!"
            FROM squadov.vods AS v
            INNER JOIN squadov.users AS u
                ON u.uuid = v.user_uuid
            INNER JOIN squadov.matches AS m
                ON m.uuid = v.match_uuid
            WHERE v.is_clip = FALSE
                AND v.end_time IS NOT NULL
                AND u.id = $1
                AND m.game IS NOT NULL
            GROUP BY m.game
            "#,
            user.id,
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| {
                (SquadOvGames::try_from(x.game).unwrap(), x.count)
            })
            .collect::<HashMap<SquadOvGames, i64>>();

        let gpu_0 = if let Some(hw) = &hardware {
            hw.display.gpus.get(0).clone()
        } else {
            None
        };

        let gpu_1 = if let Some(hw) = &hardware {
            hw.display.gpus.get(1).clone()
        } else {
            None
        };

        // Segment API call to identify user.
        match self.segment.identify(&user.uuid.to_string(), anon_id, ip_addr, &ServerUserIdentifyTraits{
            email: user.email.clone(),
            username: user.username.clone(),
            referral_code: sqlx::query!(
                r#"
                SELECT rc.code AS "code!"
                FROM squadov.user_referral_code_usage AS rcu
                INNER JOIN squadov.referral_codes AS rc
                    ON rc.id = rcu.code_id
                INNER JOIN squadov.users AS u
                    ON u.email = rcu.email
                WHERE u.id = $1
                "#,
                user.id,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| { x.code }),
            address: if let Some(loc) = &loc_data {
                Some(ServerUserAddressTraits{
                    city: loc.city.clone(),
                    country: loc.country.clone(),
                })
            } else {
                None
            },
            timezone: if let Some(d) = &loc_data { d.timezone } else { None },
            city: if let Some(d) = &loc_data { d.city.clone() } else { None },
            country: if let Some(d) = &loc_data { d.country.clone() } else { None },
            cpu_vendor: hardware.as_ref().map(|x| { x.cpu.vendor.clone() }),
            cpu_brand: hardware.as_ref().map(|x| { x.cpu.brand.clone() }),
            cpu_clock: hardware.as_ref().map(|x| { x.cpu.clock }),
            cpu_cores: hardware.as_ref().map(|x| { x.cpu.cores }),
            os_name: hardware.as_ref().map(|x| { x.os.name.clone() }),
            os_major: hardware.as_ref().map(|x| { x.os.major_version.clone() }),
            os_minor: hardware.as_ref().map(|x| { x.os.minor_version.clone() }),
            os_edition: hardware.as_ref().map(|x| { x.os.edition.clone() }),
            gpu_name_0: gpu_0.as_ref().map(|x| { x.name.clone() }),
            gpu_memory_0: gpu_0.as_ref().map(|x| { x.memory_bytes }),
            gpu_name_1: gpu_1.as_ref().map(|x| { x.name.clone() }),
            gpu_memory_1: gpu_1.as_ref().map(|x| { x.memory_bytes }),
            ram_kb: hardware.as_ref().map(|x| { x.ram_kb }),
            squads: squad_traits.squads,
            squad_friends: squad_traits.friends,
            squad_invites_sent: squad_invites.invites,
            squad_link_used: squad_invites.links,
            referrals: referral_count,
            clip_likes: clip_reacts.likes,
            clip_comments: clip_reacts.comments,
            aimlab_vods: per_game_vods.remove(&SquadOvGames::AimLab).unwrap_or(0),
            csgo_vods: per_game_vods.remove(&SquadOvGames::Csgo).unwrap_or(0),
            hearthstone_vods: per_game_vods.remove(&SquadOvGames::Hearthstone).unwrap_or(0),
            lol_vods: per_game_vods.remove(&SquadOvGames::LeagueOfLegends).unwrap_or(0),
            tft_vods: per_game_vods.remove(&SquadOvGames::TeamfightTactics).unwrap_or(0),
            valorant_vods: per_game_vods.remove(&SquadOvGames::Valorant).unwrap_or(0),
            wow_vods: per_game_vods.remove(&SquadOvGames::WorldOfWarcraft).unwrap_or(0),
        }).await {
            Ok(()) => (),
            Err(err) => {
                log::warn!("Failed to send analytics to segment: {:?}", err);
            }
        };

        Ok(())
    }
}

pub async fn sync_user_hardware_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<Hardware>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    store_hardware_for_user(&*app.pool, session.user.id, data.into_inner()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct IdentifyInput {
    ip: String,
    anon_id: String,
}

pub async fn perform_user_analytics_identify_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<IdentifyInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.analytics_identify_user(&session.user, &data.ip, &data.anon_id).await?;
    Ok(HttpResponse::NoContent().finish())
}