pub mod auto;
pub mod settings;

pub use auto::*;
pub use settings::*;

use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    share::{
        self,
        MatchVideoShareConnection,
        MatchVideoSharePermissions,
    },
    VodAssociation,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use sqlx::{Transaction, Postgres};

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
    // This function takes care of the things the old fn_trigger_auto_share database trigger used to do.
    // We had to move it out into code since it was getting a built unwieldy and we need to start doing more
    // complex checks for sharing.
    pub async fn handle_vod_share(&self, tx : &mut Transaction<'_, Postgres>, user_id: i64, vod: &VodAssociation) -> Result<(), SquadOvError> {
        if let Some(match_uuid) = vod.match_uuid.as_ref() {
            // Now we need grab some details about the match, the user's sharing settings, and the user's squads sharing settings so we know how to share things properly.
            // 1) Get all the auto-sharing settings the user has that matches the game being played.
            //    This will get us information on which users and/or squads to continue to share to.
            let auto_connections = share::auto::get_auto_share_connections_for_user(&mut *tx, user_id).await?;
            let game = self.get_game_for_match(match_uuid).await?;

            // Doing the safe thing here and explicitly checking for unknown.
            // I'm a little worried that somewhere contains a thing that'll demonstrate
            // the mismatch between the client SquadOvGames enum and the server one...
            if game == SquadOvGames::Unknown {
                return Ok(());
            }

            // Doing a loop here may be slower than bulk but for now I'm assuming that 
            //      a) This isn't a core inner loop so the slowness should be OK.
            //      b) A user won't be in a bajillion squads that'd make this significantly slow.
            // Chances are this is going to change in the future. :)
            for conn in &auto_connections {
                if !conn.games.contains(&game) {
                    continue;
                }

                // 3) Share with the user/squad in question.
                // Should this actually error out if something failed?
                if conn.dest_squad_id.is_none() && conn.dest_user_id.is_none() {
                    self.sharing_itf.handle_vod_share_to_profile(&mut *tx, user_id, vod).await?;
                } else if let Some(squad_id) = conn.dest_squad_id {
                    self.sharing_itf.handle_vod_share_to_squad(&mut *tx, user_id, match_uuid, game, squad_id, &MatchVideoShareConnection{
                        can_share: conn.can_share,
                        can_clip: conn.can_clip,
                        id: -1,
                        match_uuid: if vod.is_clip {
                            None
                        } else {
                            vod.match_uuid.clone()
                        },
                        video_uuid: Some(vod.video_uuid.clone()),
                        dest_user_id: None,
                        dest_squad_id: Some(squad_id),
                    }, None).await?;
                }
            }
        }

        Ok(())
    }

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
        let raw_vods = app.find_accessible_vods_in_match_for_user(match_uuid, session.user.id).await?;

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
        // We currently only support squad sharing.
        if let Some(squad_id) = &new_conn.dest_squad_id {
            if let Some(added) = app.sharing_itf.handle_vod_share_to_squad(
                &mut tx,
                session.user.id,
                // The incoming connection will probably have a video uuid but won't necessarily have a match uuid (e.g. for clips).
                // Thus we have to query the database for more information about the VOD and get the match UUID properly so that
                // we can do proper game data filtering when doing the sharing. In the case where no video uuid exists, then the
                // match uuid must exist since in that case we're just sharing a match with no VODs.
                &if let Some(video_uuid) = new_conn.video_uuid.as_ref() {
                    app.get_vod_match_uuid(video_uuid).await?.ok_or(SquadOvError::BadRequest)?
                } else {
                    new_conn.match_uuid.ok_or(SquadOvError::BadRequest)?
                },
                data.game.clone().unwrap_or(SquadOvGames::Unknown),
                *squad_id,
                &new_conn,
                app.find_shareable_parent_connection_for_match_video_for_user(
                    new_conn.match_uuid.as_ref(),
                    new_conn.video_uuid.as_ref(),
                    session.user.id,
                    session.user.uuid.clone(),
                    data.game.as_ref(),
                ).await?
            ).await? {
                ret_conns.push(added);
            }
        }
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

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct SharePermissionsQueryData {
    match_uuid: Option<Uuid>,
    video_uuid: Option<Uuid>,
    game: Option<SquadOvGames>,
}

pub async fn get_share_permissions_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<SharePermissionsQueryData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    // Early exists for when we know for sure that the user doesn't need a permission check (aka they're the "owner").
    // For matches, this means that the user themself is in the match. For VODs/clips, this means that they're the owner of the VOD.
    let is_owner = if let Some(match_uuid) = data.match_uuid {
        if let Some(game) = data.game {
            squadov_common::matches::is_user_in_match(&*app.pool, session.user.id, &match_uuid, game).await?
        } else {
            false
        }
    } else if let Some(video_uuid) = data.video_uuid {
        let assocs = app.find_vod_associations(&[video_uuid.clone()]).await?;
        if let Some(vod) = assocs.get(&video_uuid) {
            if let Some(user_uuid) = vod.user_uuid {
                user_uuid == session.user.uuid
            } else {
                false
            }
        } else {
            false
        }
    } else {
        return Err(SquadOvError::BadRequest);
    };

    Ok(
        HttpResponse::Ok().json(
            if is_owner {
                MatchVideoSharePermissions{
                    id: -1,
                    can_share: true,
                    can_clip: true,
                }
            } else {
                share::get_match_vod_share_permissions_for_user(&*app.pool, data.match_uuid.as_ref(), data.video_uuid.as_ref(), session.user.id).await?
            }
        )
    )
}
