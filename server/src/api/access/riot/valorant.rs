use actix_web::{HttpRequest};
use squadov_common::{
    SquadOvError,
    access
};
use crate::api::auth::SquadOVSession;
use crate::api::ApiApplication;
use crate::api::access::AccessChecker;
use std::sync::Arc;
use async_trait::async_trait;
use std::collections::HashSet;
use std::iter::FromIterator;
use uuid::Uuid;

pub struct ValorantMatchUuidData {
    pub match_uuid: Uuid
}

pub struct ValorantMatchUuidPathObtainer {
    pub match_uuid_key: &'static str
}

pub struct ValorantMatchAccessChecker {
    pub obtainer: ValorantMatchUuidPathObtainer
}

#[async_trait]
impl AccessChecker<ValorantMatchUuidData> for ValorantMatchAccessChecker {
    fn generate_aux_metadata(&self, req: &HttpRequest) -> Result<ValorantMatchUuidData, SquadOvError> {
        Ok(ValorantMatchUuidData{
            match_uuid: match req.match_info().get(self.obtainer.match_uuid_key) {
                Some(x) => x.parse()?,
                None => return Err(squadov_common::SquadOvError::BadRequest),
            },
        })
    }

    async fn check(&self, app: Arc<ApiApplication>, session: &SquadOVSession, data: ValorantMatchUuidData) -> Result<bool, SquadOvError> {
        // The user must be either in the match or a squad member of a user in that match.        
        let base_access_set: HashSet<i64> = HashSet::from_iter(app.list_squadov_accounts_in_valorant_match(&data.match_uuid).await?);
        if base_access_set.contains(&session.user.id) {
            return Ok(true);
        } else {
            return Ok(access::check_user_has_access_to_match_vod_from_user(&*app.pool, session.user.id, None, Some(data.match_uuid.clone()), None).await?);
        }
    }

    async fn post_check(&self, _app: Arc<ApiApplication>, _session: &SquadOVSession, _data: ValorantMatchUuidData) -> Result<bool, SquadOvError> {
        Ok(true)
    }
}