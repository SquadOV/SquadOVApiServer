use squadov_common::{
    SquadOvError,
    vod::{
        self,
        RawVodTag,
        VodTag,
    }
};
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use std::sync::Arc;
use uuid::Uuid;
use sqlx::{Transaction, Postgres};
use serde::Deserialize;

impl api::ApiApplication {
    pub async fn get_vod_tags(&self, video_uuid: &Uuid, user_id: i64) -> Result<Vec<VodTag>, SquadOvError> {
        let tags = sqlx::query_as!(
            RawVodTag,
            r#"
            SELECT
                vvt.video_uuid AS "video_uuid!",
                vvt.user_id AS "user_id!",
                vvt.tm AS "tm!",
                vvt.tag AS "tag!",
                vvt.tag_id AS "tag_id!"
            FROM squadov.view_vod_tags AS vvt
            WHERE video_uuid = $1
            "#,
            video_uuid,
        )
            .fetch_all(&*self.pool)
            .await?;
        Ok(vod::condense_raw_vod_tags(tags, user_id)?)
    }

    pub async fn create_tags(&self, tx: &mut Transaction<'_, Postgres>, tags: &[String]) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.tags ( tag )
            SELECT LOWER(t.tag)
            FROM UNNEST($1::VARCHAR[]) AS t(tag)
            ON CONFLICT DO NOTHING
            ",
            tags,
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn add_tags_to_video(&self, tx: &mut Transaction<'_, Postgres>, video_uuid: &Uuid, tags: &[String], user_id: i64) -> Result<Vec<VodTag>, SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.user_vod_tags (
                video_uuid,
                tag_id,
                user_id,
                tm
            )
            SELECT $1, t.tag_id, $3, NOW()
            FROM UNNEST($2::VARCHAR[]) AS inp(tag)
            INNER JOIN squadov.tags AS t
                ON t.tag = LOWER(inp.tag)
            ON CONFLICT DO NOTHING
            ",
            video_uuid,
            tags,
            user_id
        )
            .execute(&mut *tx)
            .await?;

        let tags = sqlx::query_as!(
            RawVodTag,
            r#"
            SELECT
                vvt.video_uuid AS "video_uuid!",
                vvt.user_id AS "user_id!",
                vvt.tm AS "tm!",
                vvt.tag AS "tag!",
                vvt.tag_id AS "tag_id!"
            FROM UNNEST($2::VARCHAR[]) AS inp(tag)
            INNER JOIN squadov.view_vod_tags AS vvt
                ON vvt.video_uuid = $1
                    AND vvt.tag = LOWER(inp.tag)
            "#,
            video_uuid,
            tags,
        )
            .fetch_all(tx)
            .await?;
        Ok(vod::condense_raw_vod_tags(tags, user_id)?)
    }

    pub async fn remove_tag_from_video_for_user(&self, tag_id: i64, video_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.user_vod_tags
            WHERE video_uuid = $1
                AND tag_id = $2
                AND user_id = $3
            ",
            video_uuid,
            tag_id,
            user_id
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}

pub async fn get_tags_for_vod_handler(data : web::Path<super::GenericVodPathInput>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(
        app.get_vod_tags(&data.video_uuid, session.user.id).await?
    ))
}

pub async fn add_tags_for_vod_handler(data : web::Path<super::GenericVodPathInput>, app : web::Data<Arc<api::ApiApplication>>, tags: web::Json<Vec<String>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let mut tx = app.pool.begin().await?;
    app.create_tags(&mut tx, &tags).await?;
    let ret_tags = app.add_tags_to_video(&mut tx, &data.video_uuid, &tags, session.user.id).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(&ret_tags))
}

#[derive(Deserialize)]
pub struct VodTagInput {
    pub video_uuid: Uuid,
    pub tag_id: i64,
}

pub async fn delete_tag_for_vod_handler(data : web::Path<VodTagInput>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.remove_tag_from_video_for_user(data.tag_id, &data.video_uuid, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}