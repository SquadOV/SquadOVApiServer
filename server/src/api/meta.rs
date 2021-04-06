use crate::api;
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    encrypt::{
        squadov_decrypt,
    },
    access::AccessTokenRequest,
    VodSegmentId,
};
use actix_web::{web, HttpResponse};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct MetaParams {
    share: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct MetaData {
    pub meta_title: String,
    pub meta_has_video: bool,
    pub meta_video: Option<String>,
    pub meta_video_type: Option<String>,
    pub meta_video_width: Option<i32>,
    pub meta_video_height: Option<i32>,
    pub meta_username: String,
    pub meta_has_thumbnail: bool,
    pub meta_thumbnail: Option<String>,
    pub meta_thumbnail_width: Option<i32>,
    pub meta_thumbnail_height: Option<i32>,
    pub meta_player: Option<String>,
    // Twitter card type
    pub twitter_card: String,
    // OpenGraph type
    pub og_type: String,
    // OEmbed type,
    pub oembed_type: String,
}

impl Default for MetaData  {
    fn default() -> Self {
        Self {
            meta_title: String::from("Awesome SquadOV Content!"),
            meta_has_video: false,
            meta_video: None,
            meta_video_type: None,
            meta_video_width: None,
            meta_video_height: None,
            meta_username: String::new(),
            meta_has_thumbnail: false,
            meta_thumbnail: None,
            meta_thumbnail_width: None,
            meta_thumbnail_height: None,
            meta_player: None,
            twitter_card: String::from("summary"),
            og_type: String::from("website"),
            oembed_type: String::from("link"),
        }
    }
}

impl api::ApiApplication {
    pub async fn get_share_meta_data(&self, share_token: &Uuid, max_width: Option<i32>, max_height: Option<i32>) -> Result<MetaData, SquadOvError> {
        let mut metadata = MetaData::default();

        let token = squadov_common::access::find_encrypted_access_token_from_id(&*self.pool, &share_token).await?;
        let req = squadov_decrypt(token, &self.config.squadov.share_key)?;

        let access = serde_json::from_slice::<AccessTokenRequest>(&req.data)?;
        let user = self.users.get_stored_user_from_uuid(&access.user_uuid, &*self.pool).await?.ok_or(SquadOvError::NotFound)?;

        // Only two possible objects can be shared at the moment: clips and matches.
        // In both cases, we only care to return video when the VOD/Clip in question
        // has been fastified. Otherwise it's just a fancy link.

        if let Some(clip_uuid) = &access.clip_uuid {
            let clips = self.get_vod_clip_from_clip_uuids(&[clip_uuid.clone()], user.id).await?;
            let clip = clips.first().ok_or(SquadOvError::NotFound)?;
            metadata.meta_title = clip.title.clone();
            metadata.meta_username = clip.clipper.clone();
        } else if let Some(match_uuid) = &access.match_uuid {
            let base_matches = self.get_recent_base_matches(&[match_uuid.clone()], user.id).await?;
            let recent_matches = self.get_recent_matches_from_uuids(&base_matches).await?;
            let m = recent_matches.first().ok_or(SquadOvError::NotFound)?;

            metadata.meta_username = m.base.username.clone();
            if let Some(aimlab) = &m.aimlab_task {
                metadata.meta_title = format!(
                    "Aim Lab :: {task} :: {score} [{tm}]",
                    task=&aimlab.task_name,
                    score=aimlab.score,
                    tm=aimlab.create_date.format("%Y%m%d %H:%M:%S").to_string(),
                );
            } else if let Some(lol) = &m.lol_match {
                metadata.meta_title = format!(
                    "League of Legends :: {mode} {win} [{tm}]",
                    mode=&lol.game_mode,
                    win=&lol.win_loss(),
                    tm=lol.game_creation.format("%Y%m%d %H:%M:%S").to_string()
                );
            } else if let Some(tft) = &m.tft_match {
                metadata.meta_title = format!(
                    "Teamfight Tactics :: {place} [{tm}]",
                    place=tft.placement,
                    tm=tft.game_datetime.format("%Y%m%d %H:%M:%S").to_string()
                );
            } else if let Some(val) = &m.valorant_match {
                metadata.meta_title = format!(
                    "Valorant ({or}-{tr}, {win}) :: {k}/{d}/{a} [{tm}]",
                    or=val.rounds_won,
                    tr=val.rounds_lost,
                    win=if val.won { String::from("Win") } else { String::from("Loss") },
                    k=val.kills,
                    d=val.deaths,
                    a=val.assists,
                    tm=val.server_start_time_utc.map(|x| {
                        x.format("%Y%m%d %H:%M:%S").to_string()
                    }).unwrap_or(String::from("Unknown"))
                );
            } else if let Some(wow_challenge) = &m.wow_challenge {
                metadata.meta_title = format!(
                    "WoW Keystone {instance}, +{lvl} {win} :: [{tm}]",
                    instance=&wow_challenge.challenge_name,
                    lvl=wow_challenge.keystone_level,
                    win=if wow_challenge.success { String::from("Success") } else { String::from("Failure") },
                    tm=wow_challenge.finish_time.map(|x| {
                        x.format("%Y%m%d %H:%M:%S").to_string()
                    }).unwrap_or(String::from("Unknown"))
                );
            } else if let Some(wow_encounter) = &m.wow_encounter {
                metadata.meta_title = format!(
                    "WoW Encounter {nm} {win} :: [{tm}]",
                    nm=&wow_encounter.encounter_name,
                    win=if wow_encounter.success { String::from("Success") } else { String::from("Failure") },
                    tm=wow_encounter.finish_time.map(|x| {
                        x.format("%Y%m%d %H:%M:%S").to_string()
                    }).unwrap_or(String::from("Unknown"))
                );
            } else if let Some(wow_arena) = &m.wow_arena {
                metadata.meta_title = format!(
                    "WoW Arena {typ} {win} :: [{tm}]",
                    typ=&wow_arena.arena_type,
                    win=if wow_arena.success { String::from("Success") } else { String::from("Failure") },
                    tm=wow_arena.finish_time.map(|x| {
                        x.format("%Y%m%d %H:%M:%S").to_string()
                    }).unwrap_or(String::from("Unknown"))
                );
            } else if m.base.game == SquadOvGames::Hearthstone {
                metadata.meta_title = String::from("Hearthstone Match");
            } else {
                return Err(SquadOvError::NotFound);
            }
        }

        if let Some(video_uuid) = &access.video_uuid {
            let vod_metadata_map = self.get_vod_quality_options(&[video_uuid.clone()]).await?;
            let vod_metadata = vod_metadata_map.get(&video_uuid).ok_or(SquadOvError::NotFound)?.first().ok_or(SquadOvError::NotFound)?;

            if vod_metadata.has_fastify {
                metadata.meta_has_video = true;
                metadata.oembed_type = String::from("video");
                metadata.twitter_card = String::from("player");
                metadata.og_type = String::from("video.other");

                let aspect_ratio = (vod_metadata.res_x as f32) / (vod_metadata.res_y as f32);
                let mut video_height: i32 = max_height.unwrap_or(300);
                let mut video_width: i32 = (video_height as f32 * aspect_ratio).floor() as i32;

                if let Some(maxwidth) = max_width {
                    if video_width > maxwidth {
                        video_width = maxwidth;
                        video_height = (video_width as f32 / aspect_ratio).floor() as i32;
                    }
                }
                
                metadata.meta_video_width = Some(video_width);
                metadata.meta_video_height = Some(video_height);
                metadata.meta_video = Some(self.vod.get_segment_redirect_uri(&VodSegmentId{
                    video_uuid: video_uuid.clone(),
                    quality: String::from("source"),
                    segment_name: String::from("fastify.mp4"),
                }).await?);
                metadata.meta_player = Some(
                    format!(
                        "{host}/player/{video_uuid}?share={share_token}",
                        host=&self.config.squadov.app_url,
                        video_uuid=&video_uuid,
                        share_token=&share_token,
                    ),
                );
                metadata.meta_video_type = Some(String::from("video/mp4"));
            }
        }
        Ok(metadata)
    }
}

pub async fn meta_handler(app : web::Data<Arc<api::ApiApplication>>, params: web::Query<MetaParams>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(
        app.get_share_meta_data(&params.share, Some(640), Some(480)).await?
    ))
}