use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use crate::api::v1::GenericMatchPathInput;
use squadov_common::{
    SquadOvError,
    csgo::{
        db,
        summary::CsgoPlayerMatchSummary,
        schema::{
            CsgoView,
            CsgoCommonEventContainer,
        },
    },
    matches::MatchPlayerPair,
    vod::VodAssociation,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use uuid::Uuid;
use std::iter::FromIterator;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct CsgoUserMatchInput {
    user_id: i64,
    match_uuid: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct CsgoMatchResponse {
    summary: CsgoPlayerMatchSummary,
    view: CsgoView,
    container: CsgoCommonEventContainer,
    // This is primarily relevant in the case where a demo exists.
    // The exact UTC time of things happening is going to be not quite the same
    // as the time given by the user thus we need to tell the user the proper
    // offset to allow them to be able to accurately go to the correct time
    // in the VOD for any given event.
    clock_offset_ms: i64,
}

#[derive(Serialize)]
struct CsgoUserAccessibleVodOutput {
    pub vods: Vec<VodAssociation>,
    #[serde(rename="userMapping")]
    pub user_mapping: HashMap<Uuid, i64>
}

pub async fn get_csgo_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CsgoUserMatchInput>) -> Result<HttpResponse, SquadOvError> {
    let user = app.users.get_stored_user_from_id(path.user_id, &*app.pool).await?.ok_or(SquadOvError::NotFound)?;
    let view = db::find_csgo_view_from_match_user(&*app.pool, &path.match_uuid, path.user_id).await?;
    let container = db::get_csgo_event_container_from_view(&*app.pool, &view.view_uuid).await?;
    Ok(HttpResponse::Ok().json(CsgoMatchResponse{
        clock_offset_ms: db::compute_csgo_timing_offset(&*app.pool, &view.view_uuid).await?,
        summary: db::list_csgo_match_summaries_for_uuids(&*app.pool, &[MatchPlayerPair{
            match_uuid: path.match_uuid.clone(),
            player_uuid: user.uuid.clone(),
        }]).await?.pop().ok_or(SquadOvError::NotFound)?,
        view: view,
        container: container,
    }))
}

pub async fn get_csgo_match_accessible_vods_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };
    let vods = app.find_accessible_vods_in_match_for_user(&path.match_uuid, session.user.id).await?;

    // Note that for each VOD we also need to figure out the mapping from user uuid to steam ID.
    let user_uuids: Vec<Uuid> = vods.iter()
        .filter(|x| { x.user_uuid.is_some() })
        .map(|x| { x.user_uuid.unwrap().clone() })
        .collect();

    Ok(HttpResponse::Ok().json(CsgoUserAccessibleVodOutput{
        vods,
        user_mapping: HashMap::from_iter(db::get_steam_ids_in_match_for_user_uuids(&*app.pool, &path.match_uuid, &user_uuids).await?)
    }))
}