pub mod auto;
pub mod settings;

pub use auto::*;
pub use settings::*;

use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    share::{
        self,
        MatchVideoShareConnection,
        MatchVideoSharePermissions,
    },
    user::SquadOVUser,
    VodAssociation,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use std::convert::TryFrom;
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
    pub async fn handle_vod_share(&self, tx : &mut Transaction<'_, Postgres>, user: &SquadOVUser, vod: &VodAssociation) -> Result<(), SquadOvError> {
        if let Some(match_uuid) = vod.match_uuid.as_ref() {
            let db_match_uuid = if vod.is_clip {
                None
            } else {
                Some(match_uuid.clone())
            };

            // Now we need grab some details about the match, the user's sharing settings, and the user's squads sharing settings so we know how to share things properly.
            // 1) Get all the auto-sharing settings the user has that matches the game being played.
            //    This will get us information on which users and/or squads to continue to share to.
            let auto_connections = share::auto::get_auto_share_connections_for_user(&mut *tx, user.id).await?;
            let game = sqlx::query!(
                "
                SELECT game
                FROM squadov.matches
                WHERE uuid = $1
                ",
                match_uuid
            )
                .fetch_one(&mut *tx)
                .await?
                .game
                .map(|x| {
                    SquadOvGames::try_from(x).unwrap_or(SquadOvGames::Unknown)
                })
                .unwrap_or(SquadOvGames::Unknown);

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

                // 2) Check that user/squad being shared with will accept this user's VOD.
                if let Some(squad_id) = conn.dest_squad_id {
                    let settings = self.get_squad_sharing_settings(squad_id).await?;

                    if settings.disabled_games.contains(&game) {
                        continue;
                    }

                    if game == SquadOvGames::WorldOfWarcraft {
                        // Easiest to do a database check here using the parameters we found in the squad sharing settings rather than pulling in a
                        // bunch of information about the different possible types of wow match views and doing the check here on the server.
                        let prevent_sharing = sqlx::query!(
                            r#"
                            SELECT (($3::BOOLEAN AND wev.view_id IS NOT NULL) 
                                OR ($4::BOOLEAN AND (wiv.view_id IS NOT NULL AND wiv.instance_type = 1))
                                OR ($5::BOOLEAN AND wcv.view_id IS NOT NULL)
                                OR ($6::BOOLEAN AND 
                                    (
                                        wav.view_id IS NOT NULL
                                            OR (
                                                wiv.view_id IS NOT NULL AND wiv.instance_type = 4
                                            )
                                    )
                                )
                                OR ($7::BOOLEAN AND (wiv.view_id IS NOT NULL AND wiv.instance_type = 3))
                            ) AS "value!"
                            FROM squadov.wow_match_view AS wmv
                            LEFT JOIN squadov.wow_encounter_view AS wev
                                ON wev.view_id = wmv.id
                            LEFT JOIN squadov.wow_challenge_view AS wcv
                                ON wcv.view_id = wmv.id
                            LEFT JOIN squadov.wow_arena_view AS wav
                                ON wav.view_id = wmv.id
                            LEFT JOIN squadov.wow_instance_view AS wiv
                                ON wiv.view_id = wmv.id
                            WHERE wmv.match_uuid = $1
                                AND wmv.user_id = $2
                            "#,
                            &match_uuid,
                            user.id,
                            settings.wow.disable_encounters,
                            settings.wow.disable_dungeons,
                            settings.wow.disable_keystones,
                            settings.wow.disable_arenas,
                            settings.wow.disable_bgs,
                        )
                            .fetch_one(&mut *tx)
                            .await?
                            .value;
                        
                        if prevent_sharing {
                            continue;
                        }
                    }

                    // At this point we also need to check the blacklist. If the user is blacklisted they are not allowed to
                    // share VODs with the squad even if they leave and rejoin.
                    let is_on_blacklist = sqlx::query!(
                        r#"
                        SELECT EXISTS (
                            SELECT 1
                            FROM squadov.squad_user_share_blacklist
                            WHERE squad_id = $1 AND user_id = $2
                        ) AS "exists!"
                        "#,
                        squad_id,
                        user.id,
                    )
                        .fetch_one(&mut *tx)
                        .await?
                        .exists;

                    if is_on_blacklist {
                        continue;
                    }
                }

                // 3) Share with the user/squad in question.
                // Should this actually error out if something failed?
                if conn.dest_squad_id.is_none() && conn.dest_user_id.is_none() {
                    sqlx::query!(
                        "
                        INSERT INTO squadov.user_profile_vods (
                            user_id,
                            video_uuid
                        ) VALUES (
                            $1,
                            $2
                        )
                        ON CONFLICT DO NOTHING
                        ",
                        user.id,
                        &vod.video_uuid,
                    )
                        .execute(&mut *tx)
                        .await?;
                } else {
                    sqlx::query!(
                        "
                        INSERT INTO squadov.share_match_vod_connections (
                            match_uuid,
                            video_uuid,
                            source_user_id,
                            dest_user_id,
                            dest_squad_id,
                            can_share,
                            can_clip,
                            parent_connection_id,
                            share_depth
                        ) VALUES (
                            $1,
                            $2,
                            $3,
                            $4,
                            $5,
                            $6,
                            $7,
                            NULL,
                            0
                        )
                        ON CONFLICT DO NOTHING
                        ",
                        db_match_uuid,
                        &vod.video_uuid,
                        user.id,
                        conn.dest_user_id,
                        conn.dest_squad_id,
                        conn.can_share,
                        conn.can_clip,
                    )
                        .execute(&mut *tx)
                        .await?;
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
