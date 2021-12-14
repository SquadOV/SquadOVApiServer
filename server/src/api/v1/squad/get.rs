use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use crate::api::auth::{SquadOVSession};
use crate::api::v1::UserResourcePath;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    SquadOvSquad,
    SquadRole,
    SquadOvSquadMembership,
    SquadSharingSettings,
    share,
};
use sqlx::{Transaction, Postgres};

impl api::ApiApplication {
    pub async fn get_squad(&self, squad_id: i64) -> Result<SquadOvSquad, SquadOvError> {
        let squad = sqlx::query_as!(
            SquadOvSquad,
            r#"
            SELECT
                sq.id AS "id!",
                sq.squad_name AS "squad_name!",
                sq.creation_time AS "creation_time!",
                sq.member_count AS "member_count!",
                sq.pending_invite_count AS "pending_invite_count!",
                sq.is_public AS "is_public!",
                sq.is_discoverable AS "is_discoverable!"
            FROM squadov.squad_overview AS sq
            WHERE id = $1
            "#,
            squad_id,
        )
            .fetch_optional(&*self.pool)
            .await?;

        if squad.is_none() {
            Err(SquadOvError::NotFound)
        } else {
            Ok(squad.unwrap())
        }
    }

    pub async fn get_squad_user_role(&self, squad_id: i64, user_id: i64) -> Result<Option<SquadRole>, SquadOvError> {
        Ok(sqlx::query_scalar(
            "
            SELECT squad_role
            FROM squadov.squad_role_assignments
            WHERE squad_id = $1 AND user_id = $2
            "
        )
            .bind(squad_id)
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await?)
    }

    pub async fn get_user_squads(&self, user_id: i64) -> Result<Vec<SquadOvSquadMembership>, SquadOvError> {
        let raw = sqlx::query!(
            r#"
            SELECT
                sq.id AS "id!",
                sq.squad_name AS "squad_name!",
                sq.creation_time AS "creation_time!",
                sq.member_count AS "member_count!",
                sq.pending_invite_count AS "pending_invite_count!",
                sq.is_public AS "is_public!",
                sq.is_discoverable AS "is_discoverable!",
                sra.squad_role AS "squad_role: SquadRole",
                us.username AS "username",
                us.id AS "user_id",
                sub.squad_id IS NULL AS "can_share!"
            FROM squadov.squad_overview AS sq
            INNER JOIN squadov.squad_role_assignments AS sra
                ON sra.squad_id = sq.id
            INNER JOIN squadov.users AS us
                ON us.id = sra.user_id
            INNER JOIN squadov.squads AS s
                ON s.id = sq.id
            LEFT JOIN squadov.squad_user_share_blacklist AS sub
                ON sub.squad_id = s.id
                    AND sub.user_id = us.id
            WHERE sra.user_id = $1
                AND (NOT us.is_admin OR NOT s.is_default)
            ORDER BY sq.squad_name
            "#,
            user_id
        )
            .fetch_all(&*self.pool)
            .await?;
        Ok(raw.into_iter().map(|x| {
            SquadOvSquadMembership{
                squad: SquadOvSquad{
                    id: x.id,
                    squad_name: x.squad_name,
                    creation_time: x.creation_time,
                    member_count: x.member_count,
                    pending_invite_count: x.pending_invite_count,
                    is_public: x.is_public,
                    is_discoverable: x.is_discoverable,
                },
                role: x.squad_role,
                username: x.username,
                user_id: x.user_id,
                can_share: x.can_share,
            }
        }).collect())
    }

    pub async fn get_squad_users(&self, squad_id: i64) -> Result<Vec<SquadOvSquadMembership>, SquadOvError> {
        let raw = sqlx::query!(
            r#"
            SELECT
                sq.id AS "id!",
                sq.squad_name AS "squad_name!",
                sq.creation_time AS "creation_time!",
                sq.member_count AS "member_count!",
                sq.pending_invite_count AS "pending_invite_count!",
                sq.is_public AS "is_public!",
                sq.is_discoverable AS "is_discoverable!",
                sra.squad_role AS "squad_role: SquadRole",
                us.username AS "username",
                us.id AS "user_id",
                sub.squad_id IS NULL AS "can_share!"
            FROM squadov.squad_overview AS sq
            INNER JOIN squadov.squad_role_assignments AS sra
                ON sra.squad_id = sq.id
            INNER JOIN squadov.users AS us
                ON us.id = sra.user_id
            LEFT JOIN squadov.squad_user_share_blacklist AS sub
                ON sub.squad_id = sq.id
                    AND sub.user_id = us.id
            WHERE sra.squad_id = $1
            ORDER BY sq.squad_name
            "#,
            squad_id
        )
            .fetch_all(&*self.pool)
            .await?;
        Ok(raw.into_iter().map(|x| {
            SquadOvSquadMembership{
                squad: SquadOvSquad{
                    id: x.id,
                    squad_name: x.squad_name,
                    creation_time: x.creation_time,
                    member_count: x.member_count,
                    pending_invite_count: x.pending_invite_count,
                    is_public: x.is_public,
                    is_discoverable: x.is_discoverable,
                },
                role: x.squad_role,
                username: x.username,
                user_id: x.user_id,
                can_share: x.can_share,
            }
        }).collect())
    }

    pub async fn get_user_squad_membership(&self, squad_id: i64, user_id: i64) -> Result<SquadOvSquadMembership, SquadOvError> {
        let raw = sqlx::query!(
            r#"
            SELECT
                sq.id AS "id!",
                sq.squad_name AS "squad_name!",
                sq.creation_time AS "creation_time!",
                sq.member_count AS "member_count!",
                sq.pending_invite_count AS "pending_invite_count!",
                sq.is_public AS "is_public!",
                sq.is_discoverable AS "is_discoverable!",
                sra.squad_role AS "squad_role: SquadRole",
                us.username AS "username",
                us.id AS "user_id",
                sub.squad_id IS NULL AS "can_share!"
            FROM squadov.squad_overview AS sq
            INNER JOIN squadov.squad_role_assignments AS sra
                ON sra.squad_id = sq.id
            INNER JOIN squadov.users AS us
                ON us.id = sra.user_id
            LEFT JOIN squadov.squad_user_share_blacklist AS sub
                ON sub.squad_id = sq.id
                    AND sub.user_id = us.id
            WHERE sra.user_id = $1 AND sra.squad_id = $2
            ORDER BY sq.squad_name
            "#,
            user_id,
            squad_id
        )
            .fetch_optional(&*self.pool)
            .await?;

        if raw.is_none() {
            return Err(SquadOvError::NotFound)
        }

        let x = raw.unwrap();
        Ok(SquadOvSquadMembership{
            squad: SquadOvSquad{
                id: x.id,
                squad_name: x.squad_name,
                creation_time: x.creation_time,
                member_count: x.member_count,
                pending_invite_count: x.pending_invite_count,
                is_public: x.is_public,
                is_discoverable: x.is_discoverable,
            },
            role: x.squad_role,
            username: x.username,
            user_id: x.user_id,
            can_share: x.can_share,
        })
    }

    pub async fn get_user_ids_in_same_squad_as_users(&self, user_ids: &[i64], squad_filter: Option<&Vec<i64>>) -> Result<Vec<i64>, SquadOvError> {
        Ok(
            if let Some(squads) = squad_filter {
                sqlx::query!(
                    r#"
                    WITH user_squads AS (
                        SELECT squad_id
                        FROM squadov.squad_role_assignments
                        WHERE user_id = any($1)
                    )
                    SELECT DISTINCT sra.user_id AS "user_id"
                    FROM squadov.squad_role_assignments AS sra
                    INNER JOIN user_squads AS us
                        ON us.squad_id = sra.squad_id
                    WHERE us.squad_id = ANY($2)
                    "#,
                    user_ids,
                    squads
                )
                    .fetch_all(&*self.pool)
                    .await?
                    .into_iter()
                    .map(|x| {
                        x.user_id
                    })
                    .collect()
            } else {
                sqlx::query!(
                    r#"
                    WITH user_squads AS (
                        SELECT squad_id
                        FROM squadov.squad_role_assignments
                        WHERE user_id = any($1)
                    )
                    SELECT DISTINCT sra.user_id AS "user_id"
                    FROM squadov.squad_role_assignments AS sra
                    INNER JOIN user_squads AS us
                        ON us.squad_id = sra.squad_id
                    "#,
                    user_ids,
                )
                    .fetch_all(&*self.pool)
                    .await?
                    .into_iter()
                    .map(|x| {
                        x.user_id
                    })
                    .collect()
            }
        )
    }

    pub async fn get_discover_squads(&self, user_id: i64) -> Result<Vec<SquadOvSquad>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                SquadOvSquad,
                r#"
                SELECT
                    sq.id AS "id!",
                    sq.squad_name AS "squad_name!",
                    sq.creation_time AS "creation_time!",
                    sq.member_count AS "member_count!",
                    sq.pending_invite_count AS "pending_invite_count!",
                    sq.is_public AS "is_public!",
                    sq.is_discoverable AS "is_discoverable!"
                FROM squadov.squad_overview AS sq
                LEFT JOIN squadov.squad_role_assignments AS sra
                    ON sra.squad_id = sq.id
                        AND sra.user_id = $1
                WHERE sq.is_public AND sq.is_discoverable AND sra.squad_id IS NULL
                "#,
                user_id,
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

    pub async fn update_squad_sharing_settings(&self, tx : &mut Transaction<'_, Postgres>, squad_id: i64, settings: &SquadSharingSettings) -> Result<(), SquadOvError> {
        // Need to delete everything from squad_sharing_games_filter and then insert again
        // so we get make sure that the input disabled_games vec is authoritative.
        sqlx::query!(
            "
            DELETE FROM squadov.squad_sharing_games_filter
            WHERE squad_id = $1
            ",
            squad_id
        )
            .execute(&mut *tx)
            .await?;

        sqlx::query!(
            "
            INSERT INTO squadov.squad_sharing_games_filter (
                squad_id,
                disabled_game
            )
            SELECT $1, t.id
            FROM UNNEST($2::INTEGER[]) AS t(id)
            ON CONFLICT DO NOTHING
            ",
            squad_id,
            &settings.disabled_games.iter().map(|x| { *x as i32 }).collect::<Vec<i32>>(),
        )
            .execute(&mut *tx)
            .await?;

        // Need to create the entry for the wow filters in the database if it doesn't already exist.
        sqlx::query!(
            "
            INSERT INTO squadov.squad_sharing_wow_filters (
                squad_id,
                disable_arenas,
                disable_bgs,
                disable_dungeons,
                disable_encounters,
                disable_keystones,
                disabled_releases
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7
            )
            ON CONFLICT (squad_id) DO UPDATE SET
                disable_arenas = EXCLUDED.disable_arenas,
                disable_bgs = EXCLUDED.disable_bgs,
                disable_dungeons = EXCLUDED.disable_dungeons,
                disable_encounters = EXCLUDED.disable_encounters,
                disable_keystones = EXCLUDED.disable_keystones,
                disabled_releases = EXCLUDED.disabled_releases
            ",
            squad_id,
            settings.wow.disable_arenas,
            settings.wow.disable_bgs,
            settings.wow.disable_dungeons,
            settings.wow.disable_encounters,
            settings.wow.disable_keystones,
            &settings.wow.disabled_releases.iter().map(|x| { *x as i32 }).collect::<Vec<i32>>(),
        )
            .execute(&mut *tx)
            .await?;
        
        Ok(())
    }
}

pub async fn get_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    let squad = app.get_squad(path.squad_id).await?;
    Ok(HttpResponse::Ok().json(&squad))
}

pub async fn get_user_squads_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<UserResourcePath>) -> Result<HttpResponse, SquadOvError> {
    let squads = app.get_user_squads(path.user_id).await?;
    Ok(HttpResponse::Ok().json(&squads))
}

pub async fn get_squad_user_membership_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadMembershipPathInput>) -> Result<HttpResponse, SquadOvError> {
    let membership = app.get_user_squad_membership(path.squad_id, path.user_id).await?;
    Ok(HttpResponse::Ok().json(&membership))
}

pub async fn get_all_squad_user_memberships_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    let memberships = app.get_squad_users(path.squad_id).await?;
    Ok(HttpResponse::Ok().json(&memberships))
}

pub async fn get_user_discover_squads_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(&app.get_discover_squads(session.user.id).await?))
}

pub async fn get_squad_share_settings_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(
        &share::get_squad_sharing_settings(&*app.pool, path.squad_id).await?
    ))
}

pub async fn update_squad_share_settings_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadSelectionInput>, data: web::Json<SquadSharingSettings>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.update_squad_sharing_settings(&mut tx, path.squad_id, &data).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}