use crate::api;
use crate::api::auth::SquadOVSession;
use crate::api::v1::{
    RecentMatchQuery,
    UserProfilePath,
};
use actix_web::{web, HttpResponse, HttpRequest};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use squadov_common::{
    SquadOvError,
    VodDestination,
    SquadOvGames,
    VodClip,
    ClipReact,
    ClipComment,
    access::{
        self,
        AccessToken,
    },
    vod::{
        self,
        RawVodTag,
    }
};
use std::sync::Arc;
use std::convert::TryFrom;
use chrono::{Utc, TimeZone, Duration};

#[derive(Deserialize)]
pub struct CreateClipPathInput {
    video_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct ClipPathInput {
    clip_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct ClipBodyInput {
    title: String,
    description: String,
    game: SquadOvGames
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ClipResponse {
    uuid: Uuid,
    destination: VodDestination,
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ClipQuery {
    match_uuid: Option<Uuid>
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ClipCommentInput {
    comment: String
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct ClipCommentPathInput {
    comment_id: i64,
}

impl api::ApiApplication {
    async fn create_clip_for_vod(&self, vod_uuid: &Uuid, user_id: i64, title: &str, description: &str, game: SquadOvGames) -> Result<ClipResponse, SquadOvError> {
        let clip_uuid = Uuid::new_v4();

        let mut tx = self.pool.begin().await?;
        self.reserve_vod_uuid(&mut tx, &clip_uuid, "mp4", user_id, true).await?;

        sqlx::query!(
            "
            INSERT INTO squadov.vod_clips (
                clip_uuid,
                parent_vod_uuid,
                clip_user_id,
                title,
                description,
                game,
                tm
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                NOW()
            )
            ",
            clip_uuid,
            vod_uuid,
            user_id,
            title,
            description,
            game as i32,
        )
            .execute(&mut tx)
            .await?;
        tx.commit().await?;

        Ok(ClipResponse{
            uuid: clip_uuid.clone(),
            destination: self.create_vod_destination(&clip_uuid, "mp4").await?,
        })
    }

    pub async fn get_vod_clip_from_clip_uuids(&self, uuids: &[Uuid], user_id: i64) -> Result<Vec<VodClip>, SquadOvError> {
        // This is a multi-pass solution to get all the data we need in the most efficient way.
        // First we get all the basic VOD clip data stored in the database. After that, the only thing
        // we need to take care of is grabbing the VodAssociation and VodManifest for each clip.
        let base_data = sqlx::query!(
            r#"
            SELECT
                vc.*,
                u.username AS "clipper",
                COALESCE(rc.count, 0) AS "reacts!",
                COALESCE(cc.count, 0) AS "comments!",
                COALESCE(cv.count, 0) AS "views!",
                ufv.reason AS "favorite_reason?",
                uwv.video_uuid IS NOT NULL AS "is_watchlist!",
                COALESCE(JSONB_AGG(vvt.*) FILTER(WHERE vvt.video_uuid IS NOT NULL), '[]'::JSONB)  AS "tags!"
            FROM squadov.vod_clips AS vc
            INNER JOIN squadov.users AS u
                ON u.id = vc.clip_user_id
            LEFT JOIN squadov.view_clip_react_count AS rc
                ON rc.clip_uuid = vc.clip_uuid
            LEFT JOIN squadov.view_clip_comment_count AS cc
                ON cc.clip_uuid = vc.clip_uuid
            LEFT JOIN squadov.view_clip_view_count AS cv
                ON cv.clip_uuid = vc.clip_uuid
            LEFT JOIN squadov.user_favorite_vods AS ufv
                ON ufv.video_uuid = vc.clip_uuid
                    AND ufv.user_id = $2
            LEFT JOIN squadov.user_watchlist_vods AS uwv
                ON uwv.video_uuid = vc.clip_uuid
                    AND uwv.user_id = $2
            LEFT JOIN squadov.view_vod_tags AS vvt
                ON vvt.video_uuid = vc.clip_uuid
            WHERE vc.clip_uuid = ANY($1)
            GROUP BY vc.clip_uuid, vc.parent_vod_uuid, vc.clip_user_id, vc.title, vc.description, vc.game, vc.tm, u.username, rc.count, cc.count, cv.count, ufv.reason, uwv.video_uuid
            ORDER BY vc.tm DESC
            "#,
            uuids,
            user_id,
        )
            .fetch_all(&*self.pool)
            .await?;

        // Don't use the input uuids but rather use the more accurate UUIDs that we collected in base_data
        // as the input may (or may not) have invalid UUIDs.
        let clip_uuids: Vec<Uuid> = base_data.iter().map(|x| { x.clip_uuid.clone() }).collect();
        let mut clip_vod_associations = self.find_vod_associations(&clip_uuids).await?;
        let mut clip_vod_manifests = self.get_vod(&clip_uuids).await?;

        Ok(base_data.into_iter().map(|x| {
            Ok(VodClip {
                clip: clip_vod_associations.remove(&x.clip_uuid).ok_or(SquadOvError::NotFound)?,
                manifest: clip_vod_manifests.remove(&x.clip_uuid).ok_or(SquadOvError::NotFound)?,
                title: x.title,
                description: x.description,
                clipper: x.clipper,
                game: SquadOvGames::try_from(x.game)?,
                tm: x.tm,
                views: x.views,
                reacts: x.reacts,
                comments: x.comments,
                favorite_reason: x.favorite_reason,
                is_watchlist: x.is_watchlist,
                access_token: None,
                tags: vod::condense_raw_vod_tags(serde_json::from_value::<Vec<RawVodTag>>(x.tags)?, user_id)?,
            })
        }).collect::<Result<Vec<VodClip>, SquadOvError>>()?)
    }

    async fn list_user_accessible_clips(&self, user_id: i64, start: i64, end: i64, match_uuid: Option<Uuid>, filter: &RecentMatchQuery, needs_profile: bool) -> Result<Vec<VodClip>, SquadOvError> {
        let clips = sqlx::query!(
            "
            SELECT DISTINCT vc.clip_uuid, vc.tm
            FROM squadov.vod_clips AS vc
            INNER JOIN squadov.users AS u
                ON u.id = vc.clip_user_id
            INNER JOIN squadov.vods AS v
                ON v.video_uuid = vc.clip_uuid
            LEFT JOIN squadov.share_match_vod_connections AS svc
                ON svc.video_uuid = v.video_uuid
            INNER JOIN squadov.matches AS m
                ON m.uuid = v.match_uuid
            LEFT JOIN squadov.squad_role_assignments AS sra
                ON sra.user_id = vc.clip_user_id
            LEFT JOIN squadov.user_favorite_vods AS ufv
                ON ufv.video_uuid = v.video_uuid
                    AND ufv.user_id = $1
            LEFT JOIN squadov.user_watchlist_vods AS uwv
                ON uwv.video_uuid = v.video_uuid
                    AND uwv.user_id = $1
            LEFT JOIN squadov.view_share_connections_access_users AS vau
                ON vau.video_uuid = v.video_uuid
                    AND vau.user_id = $1
            LEFT JOIN squadov.user_profile_vods AS upv
                ON upv.video_uuid = vc.clip_uuid
                    AND upv.user_id = vc.clip_user_id
            LEFT JOIN squadov.wow_match_view AS wmv
                ON wmv.match_uuid = v.match_uuid
                    AND wmv.user_id = u.id
            LEFT JOIN squadov.wow_encounter_view AS wev
                ON wev.view_id = wmv.id
            LEFT JOIN squadov.wow_challenge_view AS wcv
                ON wcv.view_id = wmv.id
            LEFT JOIN squadov.wow_arena_view AS wav
                ON wav.view_id = wmv.id
            LEFT JOIN squadov.wow_instance_view AS wiv
                ON wiv.view_id = wmv.id
            LEFT JOIN squadov.view_vod_tags AS vvt
                ON v.video_uuid = vvt.video_uuid
            WHERE (
                (
                    vc.clip_user_id = $1 
                    AND
                    (
                        CARDINALITY($6::BIGINT[]) = 0 OR svc.dest_squad_id = ANY($6)
                    )
                )
                OR
                vau.video_uuid IS NOT NULL
            )
                AND ($4::UUID IS NULL OR m.uuid = $4)
                AND (CARDINALITY($5::INTEGER[]) = 0 OR m.game = ANY($5))
                AND (CARDINALITY($6::BIGINT[]) = 0 OR sra.squad_id = ANY($6))
                AND (CARDINALITY($7::BIGINT[]) = 0 OR vc.clip_user_id = ANY($7))
                AND COALESCE(v.end_time >= $8, TRUE)
                AND COALESCE(v.end_time <= $9, TRUE)
                AND (NOT $10::BOOLEAN OR ufv.video_uuid IS NOT NULL)
                AND (NOT $11::BOOLEAN OR uwv.video_uuid IS NOT NULL)
                AND (NOT $12::BOOLEAN OR upv.video_uuid IS NOT NULL)
                AND (CARDINALITY($13::VARCHAR[]) = 0 OR wmv.build_version LIKE ANY ($13))
                AND (wmv.id IS NULL OR wmv.build_version NOT LIKE '9.%' OR (
                    wmv.build_version LIKE '9.%'
                        AND ((
                                wev.view_id IS NOT NULL
                                    AND (CARDINALITY($14::INTEGER[]) = 0 OR wev.instance_id = ANY($14))
                                    AND (CARDINALITY($15::INTEGER[]) = 0 OR wev.encounter_id = ANY($15))
                                    AND ($16::BOOLEAN IS NULL OR wev.success = $16)
                                    AND (CARDINALITY($17::INTEGER[]) = 0 OR wev.difficulty = ANY($17))
                                    AND (CARDINALITY($18::INTEGER[]) = 0 OR wmv.player_spec = ANY($18))
                                    AND (COALESCE(wmv.t0_specs, '') ~ $19 OR COALESCE(wmv.t1_specs, '') ~ $19)
                                    AND $34
                            )
                            OR (
                                wcv.view_id IS NOT NULL
                                    AND (CARDINALITY($20::INTEGER[]) = 0 OR wcv.instance_id = ANY($20)) 
                                    AND ($21::BOOLEAN IS NULL OR wcv.success = $21)
                                    AND ($22::INTEGER IS NULL OR wcv.keystone_level >= $22)
                                    AND ($23::INTEGER IS NULL OR wcv.keystone_level <= $23)
                                    AND (CARDINALITY($24::INTEGER[]) = 0 OR wmv.player_spec = ANY($24))
                                    AND (COALESCE(wmv.t0_specs, '') ~ $25 OR COALESCE(wmv.t1_specs, '') ~ $25)
                                    AND $35
                            )
                            OR (
                                wav.view_id IS NOT NULL
                                    AND (CARDINALITY($26::INTEGER[]) = 0 OR wav.instance_id = ANY($26))
                                    AND (CARDINALITY($27::VARCHAR[]) = 0 OR wav.arena_type = ANY($27))
                                    AND ($28::BOOLEAN IS NULL OR ((wav.winning_team_id = wmv.player_team) = $28))
                                    AND (CARDINALITY($29::INTEGER[]) = 0 OR wmv.player_spec = ANY($29))
                                    AND ($30::INTEGER IS NULL OR wmv.player_rating >= $30)
                                    AND ($31::INTEGER IS NULL OR wmv.player_rating <= $31)
                                    AND (
                                        (COALESCE(wmv.t0_specs, '') ~ $32 AND COALESCE(wmv.t1_specs, '') ~ $33)
                                        OR
                                        (COALESCE(wmv.t0_specs, '') ~ $33 AND COALESCE(wmv.t1_specs, '') ~ $32)
                                    )
                                    AND $36
                            )
                            OR (
                                wiv.view_id IS NOT NULL
                            )
                        )
                ))
            GROUP BY vc.clip_uuid, vc.tm
            HAVING CARDINALITY($37::VARCHAR[]) = 0 OR ARRAY_AGG(vvt.tag) @> $37::VARCHAR[]
            ORDER BY vc.tm DESC
            LIMIT $2 OFFSET $3
            ",
            user_id,
            end - start,
            start,
            match_uuid,
            &filter.games.as_ref().unwrap_or(&vec![]).iter().map(|x| {
                *x as i32
            }).collect::<Vec<i32>>(),
            &filter.squads.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i64>>(),
            &filter.users.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i64>>(),
            filter.time_start.map(|x| {
                Utc.timestamp_millis(x)
            }),
            filter.time_end.map(|x| {
                Utc.timestamp_millis(x)
            }),
            filter.only_favorite,
            filter.only_watchlist,
            needs_profile,
            &filter.get_wow_release_db_filter(),
            // Wow retail encounter filters
            &filter.filters.wow.encounters.raids.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.encounters.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.encounters.is_winner,
            &filter.filters.wow.encounters.encounter_difficulties.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.build_friendly_composition_filter()?,
            // Wow retail keystone filters
            &filter.filters.wow.keystones.dungeons.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.keystones.is_winner,
            filter.filters.wow.keystones.keystone_low,
            filter.filters.wow.keystones.keystone_high,
            &filter.filters.wow.keystones.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.keystones.build_friendly_composition_filter()?,
            // Wow retail arena filters
            &filter.filters.wow.arenas.arenas.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.arenas.brackets.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            filter.filters.wow.arenas.is_winner,
            &filter.filters.wow.arenas.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.arenas.rating_low,
            filter.filters.wow.arenas.rating_high,
            &filter.filters.wow.arenas.build_friendly_composition_filter()?,
            &filter.filters.wow.arenas.build_enemy_composition_filter()?,
            // Wow game mode filter
            &filter.filters.wow.encounters.enabled,
            &filter.filters.wow.keystones.enabled,
            &filter.filters.wow.arenas.enabled,
            // Tags
            &filter.tags.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone().to_lowercase() }).collect::<Vec<String>>(),
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| { x.clip_uuid })
            .collect::<Vec<Uuid>>();
        Ok(self.get_vod_clip_from_clip_uuids(&clips, user_id).await?)
    }

    async fn mark_clip_view(&self, clip_uuid: &Uuid, user_id: Option<i64>) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.clip_views (
                clip_uuid,
                user_id,
                tm
            )
            VALUES (
                $1,
                $2,
                NOW()
            )
            ",
            clip_uuid,
            user_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    async fn get_clip_reacts_for_user(&self, clip_uuid: &Uuid, user_id: i64) -> Result<Vec<ClipReact>, SquadOvError> {
        let num_reacts = sqlx::query!(
            r#"
            SELECT COALESCE(
                (
                    SELECT 1
                    FROM squadov.clip_reacts AS cr
                    WHERE cr.clip_uuid = $1
                        AND cr.user_id = $2
                ),
                0
            ) AS "num!"
            "#,
            clip_uuid,
            user_id,
        )
            .fetch_one(&*self.pool)
            .await?
            .num;
        Ok(vec![ClipReact{}; num_reacts as usize])
    }

    async fn add_clip_react_for_user(&self, clip_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.clip_reacts (
                clip_uuid,
                user_id,
                tm
            )
            VALUES (
                $1,
                $2,
                NOW()
            )
            ON CONFLICT DO NOTHING
            ",
            clip_uuid,
            user_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    async fn remove_clip_react_for_user(&self, clip_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.clip_reacts
            WHERE clip_uuid = $1 AND user_id = $2
            ",
            clip_uuid,
            user_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    async fn check_user_has_access_to_clip(&self, clip_uuid: &Uuid, user_id: i64) -> Result<bool, SquadOvError> {
        let owner_id = self.get_vod_owner_user_id(clip_uuid).await?;
        if owner_id == user_id {
            return Ok(true);
        } else {
            return Ok(access::check_user_has_access_to_match_vod_from_user(&*self.pool, user_id, None, None, Some(clip_uuid.clone())).await?);
        }
    }

    async fn get_clip_comments(&self, clip_uuid: &Uuid, start: i64, end: i64) -> Result<Vec<ClipComment>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                ClipComment,
                "
                SELECT
                    cc.id,
                    cc.clip_uuid,
                    u.username,
                    cc.comment,
                    cc.tm
                FROM squadov.clip_comments AS cc
                INNER JOIN squadov.users AS u
                    ON u.id = cc.user_id
                WHERE cc.clip_uuid = $1
                ORDER BY cc.tm DESC
                LIMIT $2 OFFSET $3
                ",
                clip_uuid,
                end - start,
                start
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

    async fn create_clip_comment(&self, clip_uuid: &Uuid, user_id: i64, comment: &str) -> Result<ClipComment, SquadOvError> {
        Ok(
            sqlx::query_as!(
                ClipComment,
                r#"
                WITH new_comment AS (
                    INSERT INTO squadov.clip_comments (
                        clip_uuid,
                        user_id,
                        comment,
                        tm
                    )
                    VALUES (
                        $1,
                        $2,
                        $3,
                        NOW()
                    )
                    RETURNING *
                )
                SELECT
                    cc.id AS "id!",
                    cc.clip_uuid AS "clip_uuid!",
                    u.username AS "username!",
                    cc.comment AS "comment!",
                    cc.tm AS "tm!"
                FROM new_comment AS cc
                INNER JOIN squadov.users AS u
                    ON u.id = cc.user_id
                "#,
                clip_uuid,
                user_id,
                comment,
            )
                .fetch_one(&*self.pool)
                .await?
        )
    }

    async fn delete_clip_comment(&self, comment_id: i64, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.clip_comments
            WHERE id = $1 AND user_id = $2
            ",
            comment_id,
            user_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    fn generate_access_token_for_vod_clip(&self, user_id: Option<i64>, id: &Uuid) -> Result<String, SquadOvError> {
        Ok(
            AccessToken{
                // Ideally we'd refresh this somehow instead of just granting access for such a large chunk of time.
                expires: Some(Utc::now() + Duration::hours(6)),
                methods: Some(vec![String::from("GET")]),
                paths: Some(vec![
                    format!("/v1/vod/{}", id),
                    format!("/v1/clip/{}", id),
                ]),
                user_id,
            }.encrypt(&self.config.squadov.access_key)?
        )
    }
}

pub async fn create_clip_for_vod_handler(pth: web::Path<CreateClipPathInput>, data : web::Json<ClipBodyInput>, app : web::Data<Arc<api::ApiApplication>>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let resp = app.create_clip_for_vod(&pth.video_uuid, session.user.id, &data.title, &data.description, data.game).await?;
    Ok(HttpResponse::Ok().json(&resp))
}

async fn get_recent_clips_for_user(user_id: i64, app : web::Data<Arc<api::ApiApplication>>, req: &HttpRequest, page: web::Query<api::PaginationParameters>, query: web::Query<ClipQuery>, mut filter: web::Json<RecentMatchQuery>, needs_profile: bool) -> Result<HttpResponse, SquadOvError> {
    if needs_profile {
        filter.users = Some(vec![user_id]);
    }
    
    let mut clips = app.list_user_accessible_clips(user_id, page.start, page.end, query.match_uuid.clone(), &filter, needs_profile).await?;

    if needs_profile {
        for c in &mut clips {
            c.access_token = Some(app.generate_access_token_for_vod_clip(if let Some(user_uuid) = &c.clip.user_uuid {
                if let Some(u) = app.users.get_stored_user_from_uuid(user_uuid, &*app.pool).await? {
                    Some(u.id)
                } else {
                    None
                }
            } else {
                None
            }, &c.clip.video_uuid)?);
        }
    }

    let expected_total = page.end - page.start;
    let got_total = clips.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(clips, req, &page, expected_total == got_total)?)) 
}

pub async fn list_clips_for_user_handler(app : web::Data<Arc<api::ApiApplication>>, page: web::Query<api::PaginationParameters>,  query: web::Query<ClipQuery>, filter: web::Json<RecentMatchQuery>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    get_recent_clips_for_user(session.user.id, app, &request, page, query, filter, false).await
}

pub async fn get_profile_clips_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<UserProfilePath>, page: web::Query<api::PaginationParameters>,  query: web::Query<ClipQuery>, filter: web::Json<RecentMatchQuery>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    get_recent_clips_for_user(path.profile_id, app, &request, page, query, filter, true).await
}

pub async fn get_clip_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let clips = app.get_vod_clip_from_clip_uuids(&[pth.clip_uuid.clone()], session.user.id).await?;

    if clips.is_empty() {
        Err(SquadOvError::NotFound)
    } else {
        Ok(HttpResponse::Ok().json(&clips[0]))
    }
}

// REACTS
pub async fn get_clip_reacts_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    if !app.check_user_has_access_to_clip(&pth.clip_uuid, session.user.id).await? {
        return Err(SquadOvError::Unauthorized);
    }

    let reacts = app.get_clip_reacts_for_user(&pth.clip_uuid, session.user.id).await?;
    Ok(HttpResponse::Ok().json(&reacts))
}

pub async fn add_react_to_clip_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    if !app.check_user_has_access_to_clip(&pth.clip_uuid, session.user.id).await? {
        return Err(SquadOvError::Unauthorized);
    }

    app.add_clip_react_for_user(&pth.clip_uuid, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn delete_react_from_clip_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    if !app.check_user_has_access_to_clip(&pth.clip_uuid, session.user.id).await? {
        return Err(SquadOvError::Unauthorized);
    }

    app.remove_clip_react_for_user(&pth.clip_uuid, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

// VIEWS
pub async fn mark_clip_view_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>,  request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.mark_clip_view(&pth.clip_uuid, if session.share_token.is_some() { None } else { Some(session.user.id) }).await?;
    Ok(HttpResponse::NoContent().finish())
}

// COMMENTS
pub async fn get_clip_comments_handler(app : web::Data<Arc<api::ApiApplication>>, page: web::Query<api::PaginationParameters>, pth: web::Path<ClipPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    if !app.check_user_has_access_to_clip(&pth.clip_uuid, session.user.id).await? {
        return Err(SquadOvError::Unauthorized);
    }

    let comments = app.get_clip_comments(&pth.clip_uuid, page.start, page.end).await?;
    let expected_total = page.end - page.start;
    let got_total = comments.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(comments, &request, &page, expected_total == got_total)?)) 
}

pub async fn create_clip_comment_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipPathInput>, comment: web::Json<ClipCommentInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    if !app.check_user_has_access_to_clip(&pth.clip_uuid, session.user.id).await? {
        return Err(SquadOvError::Unauthorized);
    }

    Ok(HttpResponse::Ok().json(
        &app.create_clip_comment(&pth.clip_uuid, session.user.id, &comment.comment).await?
    ))
}

pub async fn delete_clip_comment_handler(app : web::Data<Arc<api::ApiApplication>>, pth: web::Path<ClipCommentPathInput>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };
    
    app.delete_clip_comment(pth.comment_id, session.user.id).await?;    
    Ok(HttpResponse::NoContent().finish())
}