pub mod access;
pub mod data;

use crate::{
    SquadOvError,
    blob::BlobManagementClient,
    access::{
        AccessToken,
    },
};
use serde::Serialize;
use sqlx::{
    Executor,
    Postgres,
    postgres::PgPool,
};
use chrono::{DateTime, Utc, Duration};
use uuid::Uuid;
use std::sync::Arc;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UserProfileSocialLinksSerialized {
    pub facebook: Option<String>,
    pub instagram: Option<String>,
    pub twitch: Option<String>,
    pub youtube: Option<String>,
    pub snapchat: Option<String>,
    pub twitter: Option<String>,
    pub tiktok: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UserProfileMiscDataSerialized {
    pub description: String,
    pub profile_picture_url: Option<String>,
    pub cover_picture_url: Option<String>,
    pub links: UserProfileSocialLinksSerialized,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UserProfileBasicSerialized {
    pub user_id: i64,
    pub link_slug: String,
    pub username: String,
    pub display_name: String,
    pub has_achievement_access: bool,
    pub has_match_access: bool,
    pub misc: Option<UserProfileMiscDataSerialized>,
    pub member_since: DateTime<Utc>,
    pub achievement_access: i32,
    pub match_access: i32,
    pub misc_access: i32,
    pub twitch_channel_for_sub: Option<String>,
    pub access_token: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UserProfileBasicRaw {
    pub user_id: i64,
    pub link_slug: String,
    pub display_name: String,
    pub description: String,
    pub achievement_access: i32,
    pub match_access: i32,
    pub profile_picture_blob: Option<Uuid>,
    pub cover_picture_blob: Option<Uuid>,
    pub facebook: Option<String>,
    pub instagram: Option<String>,
    pub twitch: Option<String>,
    pub youtube: Option<String>,
    pub snapchat: Option<String>,
    pub twitter: Option<String>,
    pub tiktok: Option<String>,
    pub misc_access: i32,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct UserProfileHandle {
    pub username: String,
    pub slug: String,
}

pub async fn create_user_profile_for_user_id<'a, T>(ex: T, id: i64, slug: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        SELECT squadov.create_user_profile($1, $2)
        ",
        id,
        slug.to_lowercase(),
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn get_user_profile_from_id<'a, T>(ex: T, id: i64) -> Result<Option<UserProfileBasicRaw>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            UserProfileBasicRaw,
            "
            SELECT *
            FROM squadov.user_profiles
            WHERE user_id = $1
            ",
            id,
        )
            .fetch_optional(ex)
            .await?
    )
}

pub async fn get_user_profile_from_slug<'a, T>(ex: T, slug: &str) -> Result<Option<UserProfileBasicRaw>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            UserProfileBasicRaw,
            "
            SELECT *
            FROM squadov.user_profiles
            WHERE link_slug = $1
            ",
            slug.to_lowercase(),
        )
            .fetch_optional(ex)
            .await?
    )
}

pub async fn get_user_profile_basic_serialized_with_requester(ex: &PgPool, profile: UserProfileBasicRaw, requester: Option<i64>, blob_client: Arc<BlobManagementClient>, token_key: &str) -> Result<UserProfileBasicSerialized, SquadOvError> {
    // The main thing here is to do some access checks to determine whether or not to let certain data get passed back to the caller.
    let profile_access = access::get_user_profile_access(&*ex, &profile, requester).await?;
    let user_data = sqlx::query!(
        r#"
        SELECT u.username, u.registration_time, ta.twitch_name AS "twitch_name?"
        FROM squadov.users AS u
        LEFT JOIN squadov.linked_twitch_accounts AS lta
            ON lta.user_id = u.id
        LEFT JOIN squadov.twitch_accounts AS ta
            ON ta.twitch_user_id = lta.twitch_user_id
        WHERE u.id = $1
        "#,
        profile.user_id,
    )
        .fetch_one(ex)
        .await?;

    Ok(
        UserProfileBasicSerialized {
            user_id: profile.user_id,
            link_slug: profile.link_slug,
            username: user_data.username,
            display_name: profile.display_name,
            has_achievement_access: profile_access.achievements,
            has_match_access: profile_access.matches,
            misc: if profile_access.misc {
                Some(UserProfileMiscDataSerialized {
                    description: profile.description,
                    profile_picture_url: if let Some(b) = profile.profile_picture_blob {
                        Some(blob_client.get_blob_url(&b).await?)
                    } else {
                        None
                    },
                    cover_picture_url: if let Some(b) = profile.cover_picture_blob {
                        Some(blob_client.get_blob_url(&b).await?)
                    } else {
                        None
                    },
                    links: UserProfileSocialLinksSerialized{
                        facebook: profile.facebook,
                        instagram: profile.instagram,
                        twitch: profile.twitch,
                        youtube: profile.youtube,
                        snapchat: profile.snapchat,
                        twitter: profile.twitter,
                        tiktok: profile.tiktok,
                    },
                })
            } else {
                None
            },
            member_since: user_data.registration_time.unwrap_or(Utc::now()),
            achievement_access: profile.achievement_access,
            match_access: profile.match_access,
            misc_access: profile.misc_access,
            twitch_channel_for_sub: user_data.twitch_name,
            // This access token is PURELY for the API calls that'll need to be performed on the profile page.
            access_token: Some(AccessToken{
                // Ideally we'd refresh this somehow instead of just granting access for such a large chunk of time.
                expires: Some(Utc::now() + Duration::hours(6)),
                methods: Some(vec![String::from("GET"), String::from("POST")]),
                paths: Some(vec![
                    format!("/profile/{}/matches", profile.user_id),
                    format!("/profile/{}/clips", profile.user_id),
                ]),
                user_id: Some(profile.user_id),
            }.encrypt(token_key)?),
        }
    )
}

pub async fn get_user_profile_handle_from_video_uuid<'a, T>(ex: T, video: &Uuid) -> Result<UserProfileHandle, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            UserProfileHandle,
            r#"
            SELECT
                u.username AS "username!",
                up.link_slug AS "slug!"
            FROM squadov.vods AS v
            INNER JOIN squadov.users AS u
                ON u.uuid = v.user_uuid
            INNER JOIN squadov.user_profiles AS up
                ON up.user_id = u.id
            WHERE v.video_uuid = $1
            "#,
            video,
        )
            .fetch_one(ex)
            .await?
    )
}