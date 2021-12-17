use crate::{
    SquadOvError,
    subscriptions,
    squad,
    profile::UserProfileBasicRaw,
};
use sqlx::{
    Executor,
    Postgres,
    postgres::PgPool,
};
use serde::Deserialize;

pub const USER_PROFILE_ACCESS_SELF: i32 = 0;
pub const USER_PROFILE_ACCESS_PRIVATE_SQUADS: i32 = 1;
pub const USER_PROFILE_ACCESS_PRIVATE_TWITCH_SUB: i32 = 2;
//pub const USER_PROFILE_ACCESS_PRIVATE_SQUADOV_SUB: i32 = 4;
pub const USER_PROFILE_ACCESS_PUBLIC: i32 = 8;

pub struct ProfileAccess {
    pub misc: bool,
    pub achievements: bool,
    pub matches: bool,
}

struct ProfileAccessRequest {
    is_self: bool,
    is_same_squad: bool,
    is_twitch_sub: bool,
}

impl ProfileAccessRequest {
    fn check_access(&self, access_level: i32) -> bool {
        match access_level {
            USER_PROFILE_ACCESS_SELF => self.is_self,
            USER_PROFILE_ACCESS_PUBLIC => true,
            _ => {
                (access_level & USER_PROFILE_ACCESS_PRIVATE_SQUADS > 0) && self.is_same_squad ||
                (access_level & USER_PROFILE_ACCESS_PRIVATE_TWITCH_SUB > 0) && self.is_twitch_sub
            }
        }
    }
}

pub async fn get_user_profile_access(ex: &PgPool, profile: &UserProfileBasicRaw, requester: Option<i64>) -> Result<ProfileAccess, SquadOvError> {
    let mut req = ProfileAccessRequest {
        is_self: false,
        is_same_squad: false,
        is_twitch_sub: false,
    };
    
    if let Some(requester_id) = requester {
        if requester_id == profile.user_id {
            req.is_self = true;
            req.is_same_squad = true;
            req.is_twitch_sub = true;
        } else {
            req.is_same_squad = squad::check_users_same_squad(&*ex, requester_id, profile.user_id).await?;
            req.is_twitch_sub = subscriptions::get_u2u_subscription_from_user_ids(&*ex, requester_id, profile.user_id).await?.iter().any(|x| x.is_twitch);
        }
    }

    Ok(ProfileAccess{
        misc: req.check_access(profile.misc_access),
        achievements: req.check_access(profile.achievement_access),
        matches: req.check_access(profile.match_access),
    })
}

#[derive(Deserialize)]
pub struct UserProfileBasicUpdateAccess {
    slug: String,
    misc: i32,
    achievements: i32,
    matches: i32,
}

pub async fn update_user_profile_basic_access<'a, T>(ex: T, user_id: i64, data: &UserProfileBasicUpdateAccess) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.user_profiles
        SET link_slug = $2,
            match_access = $3,
            achievement_access = $4,
            misc_access = $5
        WHERE user_id = $1
        ",
        user_id,
        &data.slug.to_lowercase(),
        data.matches,
        data.achievements,
        data.misc,
    )
        .execute(ex)
        .await?;
    Ok(())
}