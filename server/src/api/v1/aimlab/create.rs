use squadov_common;
use squadov_common::SquadOvGames;
use squadov_common::AimlabTask;
use crate::api;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use uuid::Uuid;
use sqlx::{Transaction, Executor, Postgres};
use serde::Serialize;
use crate::api::auth::SquadOVSession;
use std::vec::Vec;
use std::sync::Arc;

#[derive(Serialize)]
struct CreateAimlabTaskResponse<'a> {
    #[serde(rename = "matchUuid")]
    match_uuid: &'a Uuid
}

impl api::ApiApplication {
    pub async fn bulk_create_aimlab_task(&self, tx : &mut Transaction<'_, Postgres>, all_matches : Vec<AimlabTask>) -> Result<(), squadov_common::SquadOvError> {
        let mut sql : Vec<String> = Vec::new();
        sql.push(String::from("
            INSERT INTO squadov.aimlab_tasks (
                id,
                user_id,
                klutch_id,
                match_uuid,
                task_name,
                mode,
                score,
                version,
                create_date,
                raw_data
            )
            VALUES
        "));
        for (idx, m) in all_matches.iter().enumerate() {
            sql.push(format!("(
                {id},
                {user_id},
                '{klutch_id}',
                '{match_uuid}',
                '{task_name}',
                {mode},
                {score},
                '{version}',
                '{create_date}',
                '{raw_data}'
            )",
                id=m.id,
                user_id=m.user_id,
                klutch_id=&m.klutch_id,
                match_uuid=&m.match_uuid,
                task_name=&m.task_name,
                mode=m.mode,
                score=m.score,
                version=&m.version,
                create_date=&m.create_date,
                raw_data=&m.raw_data
            ));

            if idx != all_matches.len() - 1 {
                sql.push(String::from(","))
            }
        }
        sql.push(String::from(" ON CONFLICT DO NOTHING"));
        sqlx::query(&sql.join("")).execute(tx).await?;
        return Ok(());
    }

    pub async fn create_new_aimlab_task(&self, tx : &mut Transaction<'_, Postgres>, uuid: &Uuid, raw_match : AimlabTask) -> Result<(), squadov_common::SquadOvError> {
        tx.execute(
            sqlx::query!(
                "
                INSERT INTO squadov.aimlab_tasks (
                    id,
                    user_id,
                    klutch_id,
                    match_uuid,
                    task_name,
                    mode,
                    score,
                    version,
                    create_date,
                    raw_data
                )
                VALUES (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    $7,
                    $8,
                    $9,
                    $10
                )
                ON CONFLICT DO NOTHING
                ",
                raw_match.id,
                raw_match.user_id,
                &raw_match.klutch_id,
                uuid,
                &raw_match.task_name,
                raw_match.mode,
                raw_match.score,
                &raw_match.version,
                &raw_match.create_date,
                &raw_match.raw_data
            )
        ).await?;
        return Ok(());
    }
}

pub async fn create_new_aimlab_task_handler(data : web::Json<AimlabTask>, app : web::Data<Arc<api::ApiApplication>>, request : HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let mut raw_data = data.into_inner();

    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    raw_data.user_id = session.user.id;

    let mut tx = app.pool.begin().await?;
    // Create a new match ID and then create the match.
    let internal_match = app.create_new_match(&mut tx, SquadOvGames::AimLab).await?;
    app.create_new_aimlab_task(&mut tx, &internal_match.uuid, raw_data).await?;
    tx.commit().await?;

    return Ok(HttpResponse::Ok().json(
        &CreateAimlabTaskResponse{
            match_uuid: &internal_match.uuid
        }
    ))
}

pub async fn bulk_create_aimlab_task_handler(data : web::Json<Vec<AimlabTask>>, app : web::Data<Arc<api::ApiApplication>>, request : HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let mut raw_data = data.into_inner();

    // First bulk create a bunch of matches for each of the input Aim Lab tasks. 
    // Then make sure we associate the Aim Lab task with the current user.
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let internal_matches = app.bullk_create_matches(&mut tx, raw_data.len(), SquadOvGames::AimLab).await?;
    for (idx, data) in raw_data.iter_mut().enumerate() {
        data.user_id = session.user.id;
        data.match_uuid = internal_matches[idx].uuid.clone();
    }
    app.bulk_create_aimlab_task(&mut tx, raw_data).await?;
    tx.commit().await?;
    return Ok(HttpResponse::Ok().finish());
}