use squadov_common::SquadOvError;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use serde::Serialize;

#[derive(Serialize)]
pub struct NotificationSummaryOutput {
    #[serde(rename="numSquadInvites")]
    num_squad_invites: i64
}

impl api::ApiApplication {
    async fn get_notification_summary_for_user(&self, user_id: i64) -> Result<NotificationSummaryOutput, SquadOvError> {
        Ok(
            sqlx::query_as!(
                NotificationSummaryOutput,
                r#"
                SELECT
                    (
                        SELECT COUNT(squad_id)
                        FROM squadov.squad_membership_invites
                        WHERE user_id = $1 AND response_time IS NULL
                    ) AS "num_squad_invites!"
                "#,
                user_id,
            )
                .fetch_one(&*self.pool)
                .await?
        )
    }
}


pub async fn get_current_user_notifications_handler(app : web::Data<Arc<api::ApiApplication>>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    let summary = app.get_notification_summary_for_user(session.user.id).await?;
    Ok(HttpResponse::Ok().json(&summary))
}