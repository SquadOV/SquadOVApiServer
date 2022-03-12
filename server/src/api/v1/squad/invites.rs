use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::v1::UserResourcePath;
use crate::api::auth::SquadOVSession;
use crate::api::auth::SquadOVUserHandle;
use std::sync::Arc;
use squadov_common::{
    SquadOvError, SquadInvite,
    SquadOvSquad,
    EmailTemplate, EmailUser,
    squad::{
        links,
        links::{
            SquadInviteLink,
            PublicSquadInviteLink,
        }
    }
};
use sqlx::{Transaction, Postgres, Row};
use serde::{Serialize, Deserialize};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use openssl::hash::MessageDigest;

#[derive(Deserialize)]
pub struct CreateSquadInviteInput {
    usernames: Vec<String>,
    emails: Vec<String>,
}

struct SquadOvInviteCreationHandle {
    invite_uuid: Uuid,
    username: Option<String>,
}

impl api::ApiApplication {
    pub async fn get_squad_invite_user(&self, invite_uuid: &Uuid) -> Result<i64, SquadOvError> {
        Ok(sqlx::query_scalar(
            "
            SELECT user_id
            FROM squadov.squad_membership_invites
            WHERE invite_uuid = $1
            "
        )
            .bind(invite_uuid)
            .fetch_one(&*self.pool)
            .await?)
    }

    async fn create_squad_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, inviter_user_id: i64, usernames: &[String], emails: &[String]) -> Result<HashMap<String, SquadOvInviteCreationHandle>, SquadOvError> {
        if usernames.is_empty() && emails.is_empty() {
            return Err(SquadOvError::BadRequest);
        }

        // Remove all usernames and emails already in the squad in question.
        // Note that in the case of usernames we force the existence of the username
        // via the inner join on the users table; however, in the case of emails
        // we allows sending invites to non-registered users. The query returns of
        // tuple of (user id, email) where user id can be an option if the user
        // is not yet registered.
        let mut filtered_users_emails: Vec<(Option<i64>, String)> = sqlx::query!(
            r#"
            SELECT u.id, u.email AS "email!"
            FROM UNNEST($2::VARCHAR[]) AS a(username)
            INNER JOIN squadov.users AS u
                ON LOWER(u.username) = LOWER(a.username)
            LEFT JOIN squadov.squad_role_assignments AS sra
                ON sra.user_id = u.id
            WHERE sra.squad_id != $1 OR sra.squad_id IS NULL
            UNION
            SELECT u.id, b.email AS "email!"
            FROM UNNEST($3::VARCHAR[]) AS b(email)
            LEFT JOIN squadov.users AS u
                ON u.email = b.email
            LEFT JOIN squadov.squad_role_assignments AS sra
                ON sra.user_id = u.id
            WHERE sra.squad_id != $1 OR sra.squad_id IS NULL
            "#,
            squad_id,
            usernames,
            emails
        )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|x| { (x.id, x.email) })
            .collect();

        let filtered_users: Vec<Option<i64>> = filtered_users_emails.iter().map(|x| { x.0 }).collect();
        let filtered_emails: Vec<String> = filtered_users_emails.iter().map(|x| { x.1.clone() }).collect();
        filtered_users_emails.clear();

        // Create a squad invite for every user. Return a mapping of the email address to the invite uuid
        // so we can send invite emails as well.
        Ok(
            sqlx::query(
                r#"
                WITH inserted (email, user_id, invite_uuid) AS (
                    INSERT INTO squadov.squad_membership_invites (
                        squad_id,
                        inviter_user_id,
                        invite_time,
                        user_id,
                        email
                    )
                    SELECT $1, $2, NOW(), i.user_id, i.email
                    FROM UNNEST($3::BIGINT[], $4::VARCHAR[]) AS i(user_id, email)
                    RETURNING email, user_id, invite_uuid
                )
                SELECT i.email, i.invite_uuid, u.username
                FROM inserted AS i
                LEFT JOIN squadov.users AS u
                    ON u.id = i.user_id
                "#,
            )
                .bind(squad_id)
                .bind(inviter_user_id)
                .bind(filtered_users.as_slice())
                .bind(&filtered_emails)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|x| {
                    (x.get(0), SquadOvInviteCreationHandle{
                        invite_uuid: x.get(1),
                        username: x.get(2),
                    })
                })
                .collect()
        )
    }

    pub async fn delete_squad_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.squad_membership_invites
            WHERE squad_id = $1 AND invite_uuid = $2 AND response_time IS NULL
            ",
            squad_id,
            invite_uuid
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn accept_reject_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid, accepted: bool) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.squad_membership_invites
            SET joined = $3,
                response_time = $4
            WHERE squad_id = $1 AND invite_uuid = $2 AND response_time IS NULL
            RETURNING invite_uuid
            ",
            squad_id,
            invite_uuid,
            accepted,
            Utc::now(),
        )
            // Do a fetch one here to error if we try to accept/reject an already used invite.
            .fetch_one(tx)
            .await?;
        Ok(())
    }

    pub async fn force_add_user_to_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.squad_role_assignments (
                squad_id,
                user_id,
                squad_role
            )
            VALUES (
                $1,
                $2,
                'Member'
            )
            ",
            squad_id,
            user_id,
        )
            .execute(&mut *tx)
            .await?;

        self.autoshare_to_squad_for_user_on_join(&mut *tx, user_id, squad_id).await?;
        Ok(())
    }

    pub async fn add_user_to_squad_from_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid) -> Result<(), SquadOvError> {
        let invite_user_id = sqlx::query!(
            "
            INSERT INTO squadov.squad_role_assignments (
                squad_id,
                user_id,
                squad_role
            )
            SELECT $1, u.id, 'Member'
            FROM squadov.squad_membership_invites AS smi
            INNER JOIN squadov.users AS u
                ON smi.email = u.email
            WHERE squad_id = $1 AND invite_uuid = $2
            RETURNING user_id
            ",
            squad_id,
            invite_uuid
        )
            .fetch_optional(&mut *tx)
            .await?
            .map(|x| { x.user_id });

        if let Some(user_id) = invite_user_id {
            self.autoshare_to_squad_for_user_on_join(&mut *tx, user_id, squad_id).await?;
        }
        Ok(())
    }

    pub async fn get_user_squad_invites(&self, user_id: i64) -> Result<Vec<SquadInvite>, SquadOvError> {
        Ok(sqlx::query_as!(
            SquadInvite,
            r#"
            SELECT
                smi.squad_id,
                smi.user_id,
                smi.joined,
                smi.response_time,
                smi.invite_time,
                smi.invite_uuid,
                smi.email,
                ur.username AS "username?",
                us.username AS "inviter_username"
            FROM squadov.squad_membership_invites AS smi
            INNER JOIN squadov.users AS us
                ON us.id = smi.inviter_user_id
            LEFT JOIN squadov.users AS ur
                ON ur.id = smi.user_id
            WHERE user_id = $1 AND response_time IS NULL
            "#,
            user_id
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| { x.hide_email() })
            .collect()
        )
    }

    pub async fn get_squad_invites(&self, squad_id: i64) -> Result<Vec<SquadInvite>, SquadOvError> {
        Ok(sqlx::query_as!(
            SquadInvite,
            r#"
            SELECT
                smi.squad_id,
                smi.user_id,
                smi.joined,
                smi.response_time,
                smi.invite_time,
                smi.invite_uuid,
                smi.email,
                ur.username AS "username?",
                us.username AS "inviter_username"
            FROM squadov.squad_membership_invites AS smi
            INNER JOIN squadov.users AS us
                ON us.id = smi.inviter_user_id
            LEFT JOIN squadov.users AS ur
                ON ur.id = smi.user_id
            WHERE smi.squad_id = $1 AND response_time IS NULL
            "#,
            squad_id
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| { x.hide_email() })
            .collect()
        )
    }

    pub fn generate_invite_hmac_signature(&self, squad_id: i64, invite_uuid: &Uuid) -> Result<String, SquadOvError> {
        let request = format!("{}+{}", squad_id, invite_uuid);

        let key = PKey::hmac(&hex::decode(self.config.squadov.invite_key.as_bytes())?)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &key)?;
        signer.update(request.as_bytes())?;
        let hmac = signer.sign_to_vec()?;

        Ok(base64::encode_config(&hmac, base64::URL_SAFE_NO_PAD))
    }

    pub fn generate_invite_accept_reject_url(&self, squad_id: i64, invite_uuid: &Uuid, is_user: bool) -> Result<(String, String), SquadOvError> {
        let base = format!(
            "{}/invite/{}",
            &self.config.squadov.app_url,
            invite_uuid
        );

        let query = format!(
            "?isUser={}&squadId={}&sig={}",
            is_user,
            squad_id,
            self.generate_invite_hmac_signature(squad_id, invite_uuid)?
        );

        Ok((
            format!(
                "{}/accept{}",
                &base,
                &query
            ),
            format!(
                "{}/reject{}",
                &base,
                &query
            ),
        ))
    }

    pub async fn accept_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid) -> Result<(), SquadOvError> {
        self.accept_reject_invite(&mut *tx, squad_id, invite_uuid, true).await?;
        self.add_user_to_squad_from_invite(&mut *tx, squad_id, invite_uuid).await?;
        Ok(())
    }

    pub async fn reassociate_invite_email(&self, tx: &mut Transaction<'_, Postgres>, invite_uuid: &Uuid, email: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.squad_membership_invites
            SET email = $1
            WHERE invite_uuid = $2
                AND user_id IS NULL
            ",
            email,
            invite_uuid,
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn set_invite_pending(&self, tx: &mut Transaction<'_, Postgres>, invite_uuid: &Uuid, pending: bool) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.squad_membership_invites
            SET pending = $1
            WHERE invite_uuid = $2
                AND user_id IS NULL
            ",
            pending,
            invite_uuid,
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn associate_pending_invites_to_user(&self, email: &str, user_id: i64) -> Result<(), SquadOvError> {
        let mut tx = self.pool.begin().await?;

        let invites = sqlx::query!(
            "
            UPDATE squadov.squad_membership_invites
            SET pending = FALSE,
                user_id = $2
            WHERE pending = TRUE
                AND email = $1
            RETURNING invite_uuid, squad_id
            ",
            email,
            user_id,
        )
            .fetch_all(&mut tx)
            .await?;

        // I'm assuming this should really only ever be an array of 1 invite so it won't be expensive to iterate.
        for inv in &invites {
            self.add_user_to_squad_from_invite(&mut tx, inv.squad_id, &inv.invite_uuid).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub fn private_squad_link_to_public(&self, x: SquadInviteLink) -> PublicSquadInviteLink {
        let id = self.hashid.encode(&[x.id as u64, x.squad_id as u64, x.user_id as u64]);
        let link = format!(
            "{}/link/{}",
            &self.config.squadov.app_url,
            id
        );

        PublicSquadInviteLink {
            id,
            squad_id: x.squad_id,
            user_id: x.user_id,
            create_time: x.create_time,
            expire_time: x.expire_time,
            use_count: x.use_count,
            max_uses: x.max_uses,
            link,
        }
    }

    pub fn public_squad_link_to_private(&self, x: PublicSquadInviteLink) -> Result<SquadInviteLink, SquadOvError> {
        // [id, squad id, user id]
        let data = self.hashid.decode(&x.id)?;

        Ok(SquadInviteLink {
            id: data[0] as i64,
            squad_id: data[1] as i64,
            user_id: data[2] as i64,
            create_time: x.create_time,
            expire_time: x.expire_time,
            use_count: x.use_count,
            max_uses: x.max_uses,
        })
    }
}

pub async fn create_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>, data: web::Json<CreateSquadInviteInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let invites = app.create_squad_invite(&mut tx, path.squad_id, session.user.id, &data.usernames, &data.emails).await?;
    tx.commit().await?;

    // Now that we've tracked all the invites in the database, we can go about sending email invites for all the
    // users in question.
    match app.email.send_bulk_templated_email(&app.config.email.invite_template, invites.into_iter().map(|(email, invite)| {
        let (accept, reject) = match app.generate_invite_accept_reject_url(path.squad_id, &invite.invite_uuid, invite.username.is_some()) {
            Ok(x) => x,
            Err(err) => {
                log::warn!("Failed to generate invite accept/reject URL: {:?}", err);
                return None
            }
        };

        Some(EmailTemplate{
            to: EmailUser{
                email: email,
                name: invite.username,
            },
            params: vec![
                (String::from("product_url"), String::from("https://www.squadov.gg")),
                (String::from("product_name"), String::from("SquadOV")),
                (String::from("invite_sender_name"), session.user.username.clone()),
                (String::from("accept_url"), accept),
                (String::from("decline_url"), reject),
            ].into_iter().collect()
        })
    })
        .filter(|x| {
            x.is_some()
        })
        .map(|x| {
            x.unwrap()
        })
        .collect::<Vec<EmailTemplate>>()).await {
            Ok(_) => (),
            Err(err) => log::warn!("Failed to send squad invite emails: {:?}", err),
        };

        Ok(HttpResponse::NoContent().finish())
}

pub async fn accept_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.accept_invite(&mut tx, path.squad_id, &path.invite_uuid).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn reject_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.accept_reject_invite(&mut tx, path.squad_id, &path.invite_uuid, false).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
pub struct PublicInviteQuery {
    sig: String
}

pub async fn public_accept_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>, query: web::Query<PublicInviteQuery>) -> Result<HttpResponse, SquadOvError> {
    let sig = app.generate_invite_hmac_signature(path.squad_id, &path.invite_uuid)?;
    if sig != query.sig {
        return Err(SquadOvError::Unauthorized);
    }
    accept_squad_invite_handler(app, path).await
}

pub async fn public_reject_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>, query: web::Query<PublicInviteQuery>) -> Result<HttpResponse, SquadOvError> {
    let sig = app.generate_invite_hmac_signature(path.squad_id, &path.invite_uuid)?;
    if sig != query.sig {
        return Err(SquadOvError::Unauthorized);
    }
    reject_squad_invite_handler(app, path).await
}

pub async fn get_user_squad_invites_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<UserResourcePath>) -> Result<HttpResponse, SquadOvError> {
    let invites = app.get_user_squad_invites(path.user_id).await?;
    Ok(HttpResponse::Ok().json(&invites))
}

pub async fn  get_all_squad_invites_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    let invites = app.get_squad_invites(path.squad_id).await?;
    Ok(HttpResponse::Ok().json(&invites))
}

pub async fn revoke_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.delete_squad_invite(&mut tx, path.squad_id, &path.invite_uuid).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn get_user_squad_invite_links_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadMembershipPathInput>) -> Result<HttpResponse, SquadOvError> {
    let squad_links = links::get_squad_invite_links_for_user(&*app.pool, path.squad_id, path.user_id).await?;
    Ok(HttpResponse::Ok().json(
        squad_links
            .into_iter()
            .filter(|x| {
                !x.is_invalid()
            })
            .map(|x| {
                app.private_squad_link_to_public(x)
            })
            .collect::<Vec<PublicSquadInviteLink>>()
    ))
}

pub async fn create_user_squad_invite_link_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadMembershipPathInput>) -> Result<HttpResponse, SquadOvError> {
    let squad_link = links::create_default_squad_invite_link_for_user(&*app.pool, path.squad_id, path.user_id).await?;
    Ok(HttpResponse::Ok().json(
        app.private_squad_link_to_public(squad_link)
    ))
}

pub async fn edit_user_squad_invite_link_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadLinkPathInput>, data: web::Json<PublicSquadInviteLink>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    if path.link_id != data.id {
        return Err(SquadOvError::BadRequest);
    }

    let squad_link = app.public_squad_link_to_private(data.into_inner())?;

    // Need an error check that the current user is modifying an invite that they own.
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    if squad_link.user_id != session.user.id || 
        path.squad_id != squad_link.squad_id ||
        path.user_id != squad_link.user_id {
        return Err(SquadOvError::Unauthorized);
    }

    let mut tx = app.pool.begin().await?;
    links::modify_squad_invite(&mut tx, squad_link).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn delete_user_squad_invite_link_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadLinkPathInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    links::delete_squad_invite(&mut tx, app.hashid.decode(&path.link_id)?[0] as i64, path.squad_id, path.user_id).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Serialize)]
pub struct SquadInviteLinkData {
    squad: SquadOvSquad,
    inviter: SquadOVUserHandle,
    referral: Option<String>,
}

pub async fn get_public_invite_link_data_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadPublicLinkPathInput>) -> Result<HttpResponse, SquadOvError> {
    let id = app.hashid.decode(&path.link_id)?[0] as i64;
    let invite = links::get_squad_invite_link_from_id(&*app.pool, id).await?;
    let squad = app.get_squad(invite.squad_id).await?;
    let inviter = app.get_user_handles(&[invite.user_id]).await?.pop().ok_or(SquadOvError::BadRequest)?;

    Ok(HttpResponse::Ok().json(SquadInviteLinkData{
        squad,
        inviter,
        referral: app.get_user_referral_code(invite.user_id).await?,
    }))
}

pub async fn use_link_to_join_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadPublicLinkPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    let id = app.hashid.decode(&path.link_id)?[0] as i64;
    let invite = links::get_squad_invite_link_from_id(&*app.pool, id).await?;
    if invite.is_invalid() {
        return Err(SquadOvError::BadRequest);
    }

    let mut tx = app.pool.begin().await?;
    app.force_add_user_to_squad(&mut tx, invite.squad_id, session.user.id).await?;
    links::mark_squad_invite_link_used(&mut tx, id, session.user.id).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}

pub async fn join_public_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let squad = app.get_squad(path.squad_id).await?;
    Ok(
        if squad.is_public && squad.is_discoverable {
            let mut tx = app.pool.begin().await?;
            app.force_add_user_to_squad(&mut tx, path.squad_id, session.user.id).await?;
            tx.commit().await?;
            HttpResponse::NoContent().finish()
        } else {
            return Err(SquadOvError::Unauthorized);
        }
    )

}