use crate::api::{
    ApiApplication,
    auth::SquadOVSession,
};
use squadov_common::SquadOvError;
use std::sync::Arc;
use actix_web::{
    dev::{Path, Url},
    HttpRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

pub struct ShareTokenMetadata {
    path: Path<Url>,
    full_path: String,
    method: String,
}

pub struct ShareTokenAccessRestricter {}

#[async_trait]
impl super::AccessChecker<ShareTokenMetadata> for ShareTokenAccessRestricter {
    fn generate_aux_metadata(&self, req: &HttpRequest) -> Result<ShareTokenMetadata, SquadOvError> {
        Ok(ShareTokenMetadata{
            path: req.match_info().clone(),
            full_path: String::from(req.path()),
            method: String::from(req.method().as_str()),
        })
    }

    async fn check(&self, _app: Arc<ApiApplication>, session: Option<&SquadOVSession>, data: ShareTokenMetadata) -> Result<bool, SquadOvError> {
        let session = session.unwrap();
        // If the session doesn't have a share token then this checker isn't relevant.
        if session.share_token.is_none() {
            return Ok(true);
        }

        // Override permissions for certain URLs.
        if data.full_path.contains("wow/characters/armory") {
            return Ok(true);
        }

        // First off, reject any HTTP methods that aren't a GET because a share token shouldn't be used for making changes.
        if data.method != "GET" {
            return Ok(false);
        }

        Ok(true)
    }

    async fn post_check(&self, _app: Arc<ApiApplication>, session: Option<&SquadOVSession>, data: ShareTokenMetadata) -> Result<bool, SquadOvError> {
        let session = session.unwrap();

        // If the session doesn't have a share token then this checker isn't relevant.
        if session.share_token.is_none() {
            return Ok(true);
        }
        
        let share_token = session.share_token.as_ref().unwrap();

        // There's three accesses we want to check:
        //  1) Match
        //  2) VOD
        //  3) User
        // In certain cases, there may not be a user so we can ignore it. If it does exist though, it must pass.
        let mut granted_access = false;

        // Next, check for the presence of either a match_uuid or a video_uuid in the path because those are currently the
        // only things that can be shared right now. Then check that this access token grants access to that particular
        // match or video. Note that a specific route can also disable access to a shared token if
        // need be with a separate middleware object.
        if let Some(match_uuid) = data.path.get("match_uuid") {
            if share_token.match_uuid.is_none() || &match_uuid.parse::<Uuid>()? != share_token.match_uuid.as_ref().unwrap()  {
                return Ok(false);
            }
            granted_access = true;
        }
        
        if let Some(video_uuid) = data.path.get("video_uuid") {
            // The video UUID must be in either the singular video uuid or the bulk video uuid (LEGACY BABYYY).
            // In the case where it's singular, then we assume that's the ONLY one set and it must match exactly.
            // Otherwise, we use the bulk option and it just must be in the array.
            let path_video_uuid = video_uuid.parse::<Uuid>()?;
            if let Some(share_uuid) = share_token.video_uuid.as_ref() {
                if &path_video_uuid == share_uuid {
                    granted_access = true;                    
                }
            } 
            
            if !granted_access {
                for share_uuid in &share_token.bulk_video_uuids {
                    if &path_video_uuid == share_uuid {
                        granted_access = true;
                        break;
                    }
                }
            }

            if !granted_access {
                // At least one of the two checks above need to pass.
                return Ok(false);
            }
        }

        if let Some(clip_uuid) = data.path.get("clip_uuid") {
            if share_token.clip_uuid.is_none() || &clip_uuid.parse::<Uuid>()? != share_token.clip_uuid.as_ref().unwrap() {
                return Ok(false);
            }
            granted_access = true;
        }

        Ok(granted_access)
    }
}

pub struct DenyShareTokenAccess {}

#[async_trait]
impl super::AccessChecker<()> for DenyShareTokenAccess {
    fn generate_aux_metadata(&self, _req: &HttpRequest) -> Result<(), SquadOvError> {
        Ok(())
    }

    async fn check(&self, _app: Arc<ApiApplication>, session: Option<&SquadOVSession>, _data: ()) -> Result<bool, SquadOvError> {
        let session = session.unwrap();
        Ok(session.share_token.is_none() && session.sqv_access_token.is_none())
    }

    async fn post_check(&self, _app: Arc<ApiApplication>, _session: Option<&SquadOVSession>, _data: ()) -> Result<bool, SquadOvError> {
        Ok(true)
    }
}