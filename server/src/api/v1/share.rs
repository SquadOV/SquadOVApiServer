use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    share,
    share::MatchVideoShareConnection,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ShareConnectionPath {
    connection_id: i64,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ShareConnectionNewData {
    conn: MatchVideoShareConnection,
    game: Option<SquadOvGames>,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ShareConnectionEditData {
    can_share: bool,
    can_clip: bool,
}

impl api::ApiApplication {
    async fn find_shareable_parent_connection_for_match_video_for_user(&self, some_match_uuid: Option<&Uuid>, some_video_uuid: Option<&Uuid>, user_id: i64, user_uuid: Uuid, some_game: Option<&SquadOvGames>) -> Result<Option<i64>, SquadOvError> {
        let perms = share::get_match_vod_share_permissions_for_user(&*self.pool, some_match_uuid, some_video_uuid, user_id).await?;

        // Check to see if this connection should be based off another connection (i.e. if the user isn't the source of this sharing).
        // For matches, if the user is in the match then there is no need for a parent connection.
        //              if the user is not in the match, then they must have had the match shared with them.
        // For VODs, if the user is the owner of the VOD, then there is no need for a parent connection.
        //           if the user is not the owner of the VOD, then they must have had the VOD shared with them.
        // Technically there could be multiple parent connections, but in reality it's fine if we only find one of them.
        let parent_connection_id = if let Some(match_uuid) = some_match_uuid {
            if let Some(game) = some_game {
                if squadov_common::matches::is_user_in_match(&*self.pool, user_id, match_uuid, *game).await? {
                    None
                } else {
                    Some(perms.id)
                }
            } else {
                return Err(SquadOvError::BadRequest);
            }
        } else if let Some(video_uuid) = some_video_uuid {
            let assocs = self.find_vod_associations(&[video_uuid.clone()]).await?;
            if let Some(vod) = assocs.get(video_uuid) {
                let vod_user_uuid = vod.user_uuid.clone().ok_or(SquadOvError::BadRequest)?;
                if vod_user_uuid == user_uuid {
                    None
                } else {
                    Some(perms.id)
                }
            } else {
                return Err(SquadOvError::BadRequest);
            }
        } else {
            return Err(SquadOvError::BadRequest);
        };

        if let Some(parent) = parent_connection_id {
            // Double check permissions - user must be able to share if they aren't the source of this shared item.
            if !perms.can_share {
                return Err(SquadOvError::Unauthorized);
            }

            // Also double check that we obtained a valid connection.
            if parent == -1 {
                return Err(SquadOvError::Unauthorized);
            }
        }

        Ok(parent_connection_id)
    }
}

pub async fn create_new_share_connection_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<ShareConnectionNewData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    // If the user wants to share with a squad need to ensure that they're actually in the squad.
    if let Some(squad_id) = &data.conn.dest_squad_id {
        let role = app.get_squad_user_role(*squad_id, session.user.id).await?;
        if role.is_none() {
            return Err(SquadOvError::Unauthorized);
        }
    }
    
    if data.conn.dest_user_id.is_some() {
        return Err(SquadOvError::InternalError(String::from("Not implemented just yet.")));
    }

    let associated_video_uuids: Vec<Uuid> = if let Some(match_uuid) = &data.conn.match_uuid {
        // If we're creating a share connection for a match, then we actually need to create a connection for every VOD
        // the user has *SHARE* access to. The only way we can do this is to first get a list of all the acccesible VODs in
        // the match and then filter that based on the permissions. Note that each VOD will require a separate DB query
        // to obtain whether or not we can share that particular VOD since I don't believe the SQL query to do a proper
        // permission check can be created easily.
        let mut filtered_video_uuids: Vec<Uuid> = vec![];
        let raw_vods = app.find_accessible_vods_in_match_for_user(match_uuid, session.user.id, false).await?;

        for vod in raw_vods {
            if vod.user_uuid.unwrap_or(Uuid::nil()) != session.user.uuid {
                let perms = share::get_match_vod_share_permissions_for_user(&*app.pool, data.conn.match_uuid.as_ref(), Some(&vod.video_uuid), session.user.id).await?;
                if !perms.can_share {
                    continue;
                }
            }

            filtered_video_uuids.push(vod.video_uuid.clone());
        }

        filtered_video_uuids
    } else if let Some(video_uuid) = &data.conn.video_uuid {
        vec![video_uuid.clone()]
    } else {
        return Err(SquadOvError::BadRequest);
    };

    let pending_conns: Vec<MatchVideoShareConnection> = if associated_video_uuids.is_empty() {
        vec![MatchVideoShareConnection{
            video_uuid: None,
            ..data.conn
        }]
    } else {
        associated_video_uuids.into_iter().map(|x| {
            MatchVideoShareConnection{
                video_uuid: Some(x),
                ..data.conn
            }
        }).collect()
    };

    let mut ret_conns: Vec<MatchVideoShareConnection> = vec![];
    let mut tx = app.pool.begin().await?;

    for new_conn in pending_conns {
        ret_conns.push(share::create_new_share_connection(&mut tx, &new_conn, session.user.id, app.find_shareable_parent_connection_for_match_video_for_user(
            new_conn.match_uuid.as_ref(),
            new_conn.video_uuid.as_ref(),
            session.user.id,
            session.user.uuid.clone(),
            data.game.as_ref(),
        ).await?).await?);
    }
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(ret_conns))
}

pub async fn delete_share_connection_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<ShareConnectionPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    share::delete_share_connection(&*app.pool, path.connection_id, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn edit_share_connection_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<ShareConnectionPath>, data: web::Json<ShareConnectionEditData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    share::edit_share_connection(&*app.pool, path.connection_id, session.user.id, data.can_share, data.can_clip).await?;
    Ok(HttpResponse::NoContent().finish())
}