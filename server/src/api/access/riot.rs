mod valorant;
mod lol;
mod tft;

pub use valorant::*;
pub use lol::*;
pub use tft::*;

use actix_web::{HttpRequest};
use squadov_common::SquadOvError;
use crate::api::auth::SquadOVSession;
use crate::api::ApiApplication;
use std::sync::Arc;
use async_trait::async_trait;
use squadov_common::riot::{
    db,
};

pub struct RiotValorantAccountAccessBasicData {
    pub user_id: i64,
    pub puuid: String
}

pub struct RiotValorantAccountPathObtainer {
    pub user_id_key: &'static str,
    pub puuid_key: &'static str
}

pub struct RiotValorantAccountAccessChecker {
    pub obtainer: RiotValorantAccountPathObtainer
}

#[async_trait]
impl super::AccessChecker<RiotValorantAccountAccessBasicData> for RiotValorantAccountAccessChecker {
    fn generate_aux_metadata(&self, req: &HttpRequest) -> Result<RiotValorantAccountAccessBasicData, SquadOvError> {
        Ok(RiotValorantAccountAccessBasicData{
            user_id: match req.match_info().get(self.obtainer.user_id_key) {
                Some(x) => x.parse::<i64>()?,
                None => return Err(squadov_common::SquadOvError::BadRequest),
            },
            puuid: match req.match_info().get(self.obtainer.puuid_key) {
                Some(x) => String::from(x),
                None => return Err(squadov_common::SquadOvError::BadRequest),
            },  
        })
    }

    async fn check(&self, app: Arc<ApiApplication>, _session: Option<&SquadOVSession>, data: RiotValorantAccountAccessBasicData) -> Result<bool, SquadOvError> {
        Ok(db::is_riot_puuid_linked_to_user(&*app.pool, data.user_id, &data.puuid).await?)
    }

    async fn post_check(&self, _app: Arc<ApiApplication>, _session: Option<&SquadOVSession>, _data: RiotValorantAccountAccessBasicData) -> Result<bool, SquadOvError> {
        Ok(true)
    }
}