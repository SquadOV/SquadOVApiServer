use crate::common;
use crate::api;
use actix_web::{web, HttpResponse, HttpRequest};
use sqlx::{Executor};
use crate::api::auth::SquadOVSession;

impl api::ApiApplication {
    pub async fn associate_vod(&self, assoc : super::VodAssociation) -> Result<(), common::SquadOvError> {
        let mut tx = self.pool.begin().await?;

        tx.execute(
            sqlx::query!(
                "
                INSERT INTO squadov.vods (match_uuid, user_uuid, video_uuid, start_time, end_time)
                VALUES ($1, $2, $3, $4, $5)
                ",
                assoc.match_uuid,
                assoc.user_uuid,
                assoc.vod_uuid,
                assoc.start_time,
                assoc.end_time
            )
        ).await?;

        tx.commit().await?;
        Ok(())
    }
}

pub async fn associate_vod_handler(data : web::Json<super::VodAssociation>, app : web::Data<api::ApiApplication>, request : HttpRequest) -> Result<HttpResponse, common::SquadOvError> {
    let assoc = data.into_inner();

    // If the current user doesn't match the UUID passed in the association then reject the request.
    // We could potentially force the association to contain the correct user UUID but in reality
    // the user *should* know their own user UUID since they need to set it to properly stream it.
    // So this check acts as a sanity check on what the user streamed to.
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(common::SquadOvError::BadRequest)
    };
    if assoc.user_uuid != session.user.uuid {
        return Err(common::SquadOvError::Unauthorized);
    }

    app.associate_vod(assoc).await?;
    return Ok(HttpResponse::Ok().finish());
}