use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    community::{
        db,
        SquadOvCommunity,
        CommunityRole,
        roles,
    },
};
use serde::Deserialize;
use sqlx::{Transaction, Postgres};

#[derive(Deserialize)]
pub struct CreateCommunityInput {
    community: SquadOvCommunity
}

async fn create_default_community_roles(ex: &mut Transaction<'_, Postgres>, community_id: i64) -> Result<CommunityRole, SquadOvError> {
    let owner_role = roles::create_community_role(&mut *ex, &CommunityRole{
        id: -1,
        community_id,
        name: String::from("Admin"),
        can_manage: true,
        can_moderate: true,
        can_invite: true,
        can_share: true,
        is_default: false,
    }).await?;

    roles::create_community_role(&mut *ex, &CommunityRole{
        id: -1,
        community_id,
        name: String::from("Member"),
        can_manage: false,
        can_moderate: false,
        can_invite: false,
        can_share: false,
        is_default: true,
    }).await?;

    Ok(owner_role)
}

pub async fn create_community_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<CreateCommunityInput>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    
    let mut community = data.into_inner().community;
    community.creator_user_id = session.user.id;

    let mut tx = app.pool.begin().await?;
    let community = db::create_commmunity(&mut tx, &community).await?;
    let owner_role = create_default_community_roles(&mut tx, community.id).await?;
    db::user_join_community(&mut tx, community.id, session.user.id, None).await?;
    db::assign_user_role(&mut tx, session.user.id, owner_role.id).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(community))
}