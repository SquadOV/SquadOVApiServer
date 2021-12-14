pub mod status;
pub mod links;

use crate::{SquadOvError, SquadOvGames, SquadOvWowRelease};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use unicode_segmentation::UnicodeSegmentation;
use sqlx::{Executor, Postgres};

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SquadOvSquad {
    pub id: i64,
    pub squad_name: String,
    pub creation_time: DateTime<Utc>,
    pub member_count: i64,
    pub pending_invite_count: i64,
    pub is_public: bool,
    pub is_discoverable: bool,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SquadOvSquadMembership {
    pub squad: SquadOvSquad,
    pub role: SquadRole,
    pub username: String,
    pub user_id: i64,
    pub can_share: bool,
}

#[derive(Serialize, sqlx::Type, PartialEq, Debug)]
#[sqlx(rename="squad_role")]
pub enum SquadRole {
    Owner,
    Member
}

#[derive(Serialize)]
pub struct SquadInvite {
    #[serde(rename="squadId")]
    pub squad_id: i64,
    #[serde(rename="userId")]
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub email: String,
    pub joined: bool,
    #[serde(rename="responseTime")]
    pub response_time: Option<DateTime<Utc>>,
    #[serde(rename="inviteTime")]
    pub invite_time: Option<DateTime<Utc>>,
    #[serde(rename="inviteUuid")]
    pub invite_uuid: uuid::Uuid,
    #[serde(rename="inviterUsername")]
    pub inviter_username: String
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all="camelCase")]
pub struct SquadWowSharingSettings {
    #[serde(default)]
    pub disabled_releases: Vec<SquadOvWowRelease>,
    pub disable_encounters: bool,
    pub disable_dungeons: bool,
    pub disable_keystones: bool,
    pub disable_arenas: bool,
    pub disable_bgs: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SquadSharingSettings {
    pub disabled_games: Vec<SquadOvGames>,
    pub wow: SquadWowSharingSettings,
}

const EMAIL_HIDE_TOKEN: &'static str = "*******";

impl SquadInvite {
    pub fn hide_email(mut self) -> Self {
        // Modify the email address so that it should be
        // fairly obvious who the user sent the email to without
        // revealing the entire email address.
        let email_tokens: Vec<&str> = self.email.split("@").collect();
        if email_tokens.len() >= 2 {
            let username: String = email_tokens[0..email_tokens.len()-1].join("@");
            let username: Vec<&str> = username.graphemes(true).collect();
            let hidden_username = if username.len() > 1 {
                format!(
                    "{}{}{}",
                    username[0],
                    username[1],
                    EMAIL_HIDE_TOKEN
                )
            } else {
                String::from(EMAIL_HIDE_TOKEN)
            };

            self.email = format!(
                "{}@{}",
                hidden_username,
                email_tokens.last().unwrap()
            );
        }
        self
    }
}

pub async fn check_users_same_squad<'a, T>(ex: T, user_1: i64, user_2: i64) -> Result<bool, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM squadov.squad_role_assignments AS sra
                INNER JOIN squadov.squad_role_assignments AS ora
                    ON ora.squad_id = sra.squad_id
                WHERE sra.user_id = $1
                    AND ora.user_id = $2
            ) as "exists!"
            "#,
            user_1,
            user_2,
        )
            .fetch_one(ex)
            .await?
            .exists
    )
}