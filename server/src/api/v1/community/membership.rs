use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::{
    api::{
        v1::{
            CommunityPathInput,
            CommunityInvitePathInput,
            CommunityUserPathInput,
        },
        auth::SquadOVSession,
        ApiApplication,
    },
};
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    community::{
        db,
        CommunitySecurityLevel,
        CommunityInvite,
        invites,
        roles,
    },
    subscriptions,
};
use serde::Deserialize;
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct JoinCommunityInput {
    invite_code: Option<Uuid>,
    subscription_id: Option<i64>,
}

pub async fn delete_community_invite_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityInvitePathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;

    let invite = invites::get_community_invite(&*app.pool, &path.code).await?;
    if invite.inviter_user_id != session.user.id {
        return Err(SquadOvError::Unauthorized);
    }

    if invite.community_id != path.community_id {
        return Err(SquadOvError::BadRequest);
    }
  
    invites::delete_community_invite(&*app.pool, &path.code).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_community_invites_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;
    Ok(HttpResponse::Ok().json(invites::get_user_community_invites(&*app.pool, path.community_id, session.user.id).await?))
}

pub async fn create_community_invite_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<CommunityInvite>, path: web::Path<CommunityPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;

    let mut invite = data.into_inner();
    invite.inviter_user_id = session.user.id;
    invite.community_id = path.community_id;
    Ok(HttpResponse::Ok().json(
        invites::create_community_invite(&*app.pool, &invite).await?
    ))
}

pub async fn leave_community_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;

    db::kick_user_from_community(&*app.pool, session.user.id, path.community_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn list_users_in_community_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityPathInput>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(
        db::get_users_with_roles_in_community(&*app.pool, path.community_id).await?
    ))
}

pub async fn remove_user_from_community_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityUserPathInput>) -> Result<HttpResponse, SquadOvError> {
    db::kick_user_from_community(&*app.pool, path.user_id, path.community_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct EditCommunityUserRolesInput {
    add_roles: Vec<i64>,
    delete_roles: Vec<i64>,
}

pub async fn edit_user_in_community_handler(app : web::Data<Arc<ApiApplication>>, path: web::Path<CommunityUserPathInput>, data: web::Json<EditCommunityUserRolesInput>) -> Result<HttpResponse, SquadOvError> {
    // Do a sanity check that all these roles are actually part of the given community.
    let mut all_roles: HashSet<i64> = HashSet::new();
    data.delete_roles.iter().for_each(|x| { all_roles.insert(*x); });
    data.add_roles.iter().for_each(|x| { all_roles.insert(*x); });

    if !roles::bulk_verify_community_roles(&*app.pool, path.community_id, &all_roles.into_iter().collect::<Vec<i64>>()).await? {
        return Err(SquadOvError::BadRequest);
    }

    let mut tx = app.pool.begin().await?;

    for dr in &data.delete_roles {
        db::remove_user_role(&mut tx, path.user_id, *dr).await?;
    }

    for ar in &data.add_roles {
        db::assign_user_role(&mut tx, path.user_id, *ar).await?;
    }

    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn join_community_handler(app : web::Data<Arc<ApiApplication>>, data: web::Json<JoinCommunityInput>, path: web::Path<CommunityPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::BadRequest)?;

    let community = db::get_community_from_id(&*app.pool, path.community_id).await?;

    // If the user already is part of the community then this should fail.
    let membership = db::get_user_community_membership(&*app.pool, path.community_id, session.user.id).await?;
    if membership.is_some() {
        return Err(SquadOvError::BadRequest);
    }

    let mut sub_id: Option<i64> = None;
    // If the community is public, then anyone can join the community no question.
    // If the community is private/unlisted, then the user must either have a sub or an invite to join.
    // If the community is unlisted, then the user must have an invite to join.
    if let Some(invite_code) = &data.invite_code {
        // An invite code can be used for anyone so no need to check security level here.
        // Only need to make sure it's a valid invite for the community.
        let mut tx = app.pool.begin().await?;
        let invite = invites::get_community_invite(&mut tx, invite_code).await?;
        if invite.community_id != community.id {
            return Err(SquadOvError::Unauthorized);
        }

        if let Some(max_uses) = invite.max_uses {
            if invite.num_uses >= max_uses {
                return Err(SquadOvError::Unauthorized);
            }
        }

        if let Some(expiration) = invite.expiration {
            if Utc::now() >= expiration {
                return Err(SquadOvError::Unauthorized);
            }
        }

        invites::increment_community_invite_usage(&mut tx, invite_code).await?;
        invites::record_community_invite_usage(&mut tx, invite_code, session.user.id).await?;
        tx.commit().await?;
    } else if let Some(subscription_id) = data.subscription_id {
        // A subscription ID is only valid for communities that need it. Otherwise we can just ignore it.
        if community.requires_subscription {
            let sub = subscriptions::get_u2u_subscription(&*app.pool, subscription_id).await?;

            // In this case we'd require a subscription from the user doing the joining to the creator.
            if sub.source_user_id != session.user.id || sub.dest_user_id != community.creator_user_id {
                return Err(SquadOvError::Unauthorized);
            }
            
            if sub.is_twitch && !community.allow_twitch_sub {
                return Err(SquadOvError::Unauthorized);
            }
            sub_id = data.subscription_id;
        }
    } else if community.security_level != CommunitySecurityLevel::Public {
        // Private/unlisted communities need either an invitation or a subscription to pass.
        return Err(SquadOvError::Unauthorized);
    }

    // If we made it this for, we can let the user join as the community's default role.
    let default_role = roles::get_community_default_role(&*app.pool, community.id).await?;

    let mut tx = app.pool.begin().await?;
    db::user_join_community(&mut tx, community.id, session.user.id, sub_id).await?;
    db::assign_user_role(&mut tx, session.user.id, default_role.id).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}