use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::{
    api,
    api::{
        v1::{
            CommunityPathInput,
            CommunitySlugInput,
        },
        auth::SquadOVSession,
    },
};
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    community::{
        db,
        CommunityListQuery,
    },
    subscriptions,
};

pub async fn get_community_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CommunityPathInput>) -> Result<HttpResponse, SquadOvError> {
    Ok(
        HttpResponse::Ok().json(
            db::get_community_from_id(&*app.pool, path.community_id).await?
        )
    )
}

pub async fn get_community_role_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CommunitySlugInput>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    let community = db::get_community_from_slug(&*app.pool, &path.community_slug).await?;
    Ok(
        HttpResponse::Ok().json(
            db::get_user_community_roles(&*app.pool, community.id, session.user.id).await?
        )
    )
}

pub async fn get_community_sub_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CommunitySlugInput>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    let community = db::get_community_from_slug(&*app.pool, &path.community_slug).await?;

    Ok(
        HttpResponse::Ok().json(
            subscriptions::get_u2u_subscription_from_user_ids(&*app.pool, session.user.id, community.creator_user_id).await?
        )
    )
}

pub async fn get_community_slug_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CommunitySlugInput>) -> Result<HttpResponse, SquadOvError> {
    Ok(
        HttpResponse::Ok().json(
            db::get_community_from_slug(&*app.pool, &path.community_slug).await?
        )
    )
}

pub async fn list_communities_handler(app : web::Data<Arc<api::ApiApplication>>, filter: web::Query<CommunityListQuery>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let communities = if filter.only_me {
        let extensions = request.extensions();
        let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
        db::find_communities_for_user(&*app.pool, session.user.id).await?
    } else {
        vec![]
    };

    Ok(HttpResponse::Ok().json(communities))
}