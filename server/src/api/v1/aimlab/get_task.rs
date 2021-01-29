use squadov_common;
use squadov_common::AimlabTask;
use crate::api;
use actix_web::{web, HttpResponse};
use uuid::Uuid;
use std::sync::Arc;

impl api::ApiApplication {
    pub async fn get_aimlab_task_data(&self, match_id : Uuid) -> Result<Option<AimlabTask>, squadov_common::SquadOvError> {
        let task = sqlx::query_as!(
            AimlabTask,
            "
            SELECT *
            FROM squadov.aimlab_tasks
            WHERE match_uuid = $1
            ",
            match_id,
        )
            .fetch_optional(&*self.pool)
            .await?;
        Ok(task)
    }
}

pub async fn get_aimlab_task_data_handler(data : web::Path<super::AimlabTaskGetInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let task = app.get_aimlab_task_data(data.match_uuid).await?;
    match task {
        Some(x) => Ok(HttpResponse::Ok().json(&x)),
        None => Err(squadov_common::SquadOvError::NotFound)
    }
}