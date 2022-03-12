use sqlx;
use sqlx::postgres::PgPool;
use sqlx::{Executor, Postgres};
use actix_web::{ HttpRequest, FromRequest, dev, Error, HttpMessage};
use actix_web::error::ErrorUnauthorized;
use futures_util::future::{ok, err, Ready};
use squadov_common;
use squadov_common::{
    SquadOvError,
    encrypt::{AESEncryptToken, squadov_decrypt},
    access::{
        AccessTokenRequest,
        AccessToken,
    },
};
use crate::api::fusionauth;
use uuid::Uuid;
use crate::logged_error;
use serde::Serialize;
use std::clone::Clone;

#[derive(Debug, Serialize, Clone)]
pub struct SquadOVSession {
    pub session_id: String,
    pub user: super::SquadOVUser,
    pub access_token: String,
    pub refresh_token: String,
    pub is_temp: bool,
    pub share_token: Option<AccessTokenRequest>,
    pub sqv_access_token: Option<AccessToken>,
}

impl FromRequest for SquadOVSession {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req : &HttpRequest, _: &mut dev::Payload) -> Self::Future {
        let extensions = req.extensions();
        match extensions.get::<SquadOVSession>() {
            Some(x) => ok(x.clone()),
            None => err(ErrorUnauthorized("No session available."))
        }
    }
}

pub struct SessionManager {
}

const SESSION_ID_HEADER_KEY : &str = "x-squadov-session-id";
const SHARE_KEY_HEADER_KEY: &str = "x-squadov-share-id";
const ACCESS_KEY_HEADER_KEY: &str = "x-squadov-access-token";

impl SessionManager {
    pub fn new() -> SessionManager {
        return SessionManager{
        }
    }

    pub async fn delete_session<'a, T>(&self, id : &str, ex: T) -> Result<(), sqlx::Error>
    where
        T: Executor<'a, Database = Postgres>
    {
        sqlx::query!(
            "
            DELETE FROM squadov.user_sessions
            WHERE id = $1
            ",
            id,
        )
            .execute(ex)
            .await?;
        
        return Ok(())
    }

    pub async fn get_session_from_id(&self, id : &str, pool: &PgPool) -> Result<Option<SquadOVSession>, sqlx::Error> {
        let ret = sqlx::query!(
            "
            SELECT
                us.id AS \"session_id\",
                us.access_token,
                us.refresh_token,
                u.id AS \"user_id\",
                u.username AS \"user_username\",
                u.email AS \"user_email\",
                u.verified AS \"user_verified\",
                u.uuid AS \"user_uuid\",
                u.is_test AS \"is_test\",
                u.is_admin AS \"is_admin\",
                us.is_temp AS \"is_temp\",
                u.welcome_sent AS \"welcome_sent\",
                u.registration_time AS \"registration_time\"
            FROM squadov.user_sessions AS us
            INNER JOIN squadov.users AS u
                ON u.id = us.user_id
            WHERE us.id = $1
            ",
            id,
        ).fetch_optional(pool).await?;

        match ret {
            Some(x) => Ok(Some(SquadOVSession{
                session_id: x.session_id,
                user: super::SquadOVUser{
                    id: x.user_id,
                    username: x.user_username,
                    email: x.user_email,
                    verified: x.user_verified,
                    uuid: x.user_uuid,
                    is_test: x.is_test,
                    is_admin: x.is_admin,
                    welcome_sent: x.welcome_sent,
                    registration_time: x.registration_time,
                },
                access_token: x.access_token,
                refresh_token: x.refresh_token,
                is_temp: x.is_temp,
                share_token: None,
                sqv_access_token: None,
            })),
            None => Ok(None),
        }
    }

    pub async fn get_session_from_request(&self, req : &HttpRequest, pool: &PgPool) -> Result<Option<SquadOVSession>, sqlx::Error> {
        match req.headers().get(SESSION_ID_HEADER_KEY) {
            Some(id) => self.get_session_from_id(id.to_str().unwrap(), pool).await,
            None => Ok(None),
        }
    }

    pub async fn get_share_token_from_request(&self, req: &HttpRequest, encryption_key: &str) -> Result<Option<AccessTokenRequest>, SquadOvError> {
        let access_token = req.headers().get(SHARE_KEY_HEADER_KEY);
        if access_token.is_none() {
            return Ok(None);
        }
        let access_token = AESEncryptToken::from_string(&access_token.unwrap().to_str()?)?;
        let decrypted_token = squadov_decrypt(access_token, encryption_key)?;
        Ok(Some(serde_json::from_slice::<AccessTokenRequest>(&decrypted_token.data)?))
    }

    pub async fn get_access_token_from_request(&self, req: &HttpRequest, encryption_key: &str) -> Result<Option<AccessToken>, SquadOvError> {
        if let Some(access_token) = req.headers().get(ACCESS_KEY_HEADER_KEY) {
            let access_token = AESEncryptToken::from_string(&access_token.to_str()?)?;
            let decrypted_token = squadov_decrypt(access_token, encryption_key)?;
            Ok(Some(serde_json::from_slice::<AccessToken>(&decrypted_token.data)?))
        } else {
            Ok(None)
        }
    }

    pub async fn store_session<'a, T>(&self, ex: T, session: &SquadOVSession) -> Result<(), sqlx::Error>
    where
        T: Executor<'a, Database = Postgres>
    {
        // Store in database
        sqlx::query!(
            "
            INSERT INTO squadov.user_sessions (
                id,
                access_token,
                refresh_token,
                user_id,
                is_temp,
                issue_tm
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                NOW()
            )
            ",
            session.session_id,
            &session.access_token,
            &session.refresh_token,
            session.user.id,
            session.is_temp,
        )
            .execute(ex)
            .await?;
        
        Ok(())
    }

    pub async fn get_transition_session_id<'a, T>(&self, ex: T, id: &str) -> Result<Option<String>, SquadOvError>
    where
        T: Executor<'a, Database = Postgres>
    {
        Ok(
            sqlx::query!(
                "
                SELECT transition_id
                FROM squadov.user_sessions
                WHERE id = $1
                ",
                id
            )
                .fetch_optional(ex)
                .await?
                .map(|x| {
                    x.transition_id
                })
                .unwrap_or(None)
        )
    }

    pub async fn transition_session_to_new_id<'a, T>(&self, ex: T, old_id: &str, new_id: &str) -> Result<(), sqlx::Error>
    where
        T: Executor<'a, Database = Postgres>
    {
        sqlx::query!(
            "
            UPDATE squadov.user_sessions
            SET transition_id = $2,
                expiration_tm = NOW() + INTERVAL '3 hour'
            WHERE id = $1
            ",
            old_id,
            new_id,
        )
            .execute(ex)
            .await?;
        return Ok(())
    }

    pub async fn clean_expired_sessions_for_user<'a, T>(&self, ex: T, user_id: i64) -> Result<(), SquadOvError>
    where
        T: Executor<'a, Database = Postgres>
    {
        sqlx::query!(
            "
            DELETE FROM squadov.user_sessions
            WHERE user_id = $1
                AND is_temp = FALSE
                AND expiration_tm IS NOT NULL
                AND expiration_tm <= NOW()
            ",
            user_id
        )
            .execute(ex)
            .await?;
        Ok(())
    }
}

impl crate::api::ApiApplication {
    pub async fn is_session_valid(&self, session: &SquadOVSession) -> Result<bool, squadov_common::SquadOvError> {   
        // Temp sessions are generated by us and don't have a fusionauth access token to verify.
        if !session.is_temp {
            match self.clients.fusionauth.validate_jwt(&session.access_token).await {
                Ok(_) => Ok(true),
                Err(err) => match err {
                    fusionauth::FusionAuthValidateJwtError::Invalid => Ok(false),
                    _ => Err(squadov_common::SquadOvError::InternalError(format!("Validate JWT {}", err)))
                }
            }
        } else {
            Ok(true)
        }   
    }

    pub async fn refresh_session_if_necessary(&self, session: SquadOVSession, force: bool) -> Result<SquadOVSession, squadov_common::SquadOvError> {
        // Check if the session is expired (as determined by FusionAuth).
        // If it is expired (or close to it), generate a new session ID and use the refresh token to get a new access token.
        // If it isn't expired, return the session as is.
        let mut session = session;
        let expired = !self.is_session_valid(&session).await?;

        if expired || force {
            if let Some(transition_id) = self.session.get_transition_session_id(&*self.pool, &session.session_id).await? {
                return Ok(self.session.get_session_from_id(&transition_id, &*self.pool).await?.ok_or(SquadOvError::NotFound)?);
            }

            let new_token = match self.clients.fusionauth.refresh_jwt(&session.refresh_token).await {
                Ok(t) => t,
                Err(err) => {
                    log::warn!("Failed to Refresh client JWT: {}", err);
                    return Err(squadov_common::SquadOvError::Unauthorized);
                },
            };

            let old_id = session.session_id;
            session.session_id = Uuid::new_v4().to_string();
            session.access_token = new_token.token;
            session.refresh_token = new_token.refresh_token;

            let mut tx = self.pool.begin().await?;
            squadov_common::analytics::mark_active_user_session(&mut tx, session.user.id).await?;

            // We want to make sure a valid user has a valid session at all times; however, we also
            // want to make sure the user's session expires after a reasonable amount of time to prevent
            // a third-party from hijacking that session value. So what we need to do is
            //  1) Create a new session to pass back to the user.
            //  2) Set an expiration on the old session where it's still valid but any attempts to refresh that session
            //     will always return the session returned in #1.
            //  3) After the expiration, delete the old session and prevent it from being used.
            self.session.store_session(&mut tx, &session).await?;
            self.session.transition_session_to_new_id(&mut tx, &old_id, &session.session_id).await?;
            self.session.clean_expired_sessions_for_user(&mut tx, session.user.id).await?;

            tx.commit().await?;
        }

        Ok(session)
    }

    pub async fn refresh_and_obtain_valid_session_from_request(&self, req : &HttpRequest) -> Result<Option<SquadOVSession>, squadov_common::SquadOvError> {
        let session = match self.session.get_session_from_request(req, &self.pool).await {
            Ok(x) => match x {
                Some(y) => y,
                None => return Ok(None),
            },
            Err(err) => return logged_error!(squadov_common::SquadOvError::InternalError(format!("Refresh And Obtain Session {}", err))),
        };

        let session = self.refresh_session_if_necessary(session, false).await?;
        
        return Ok(Some(session));
    }

    pub async fn is_logged_in(&self, req : &HttpRequest) -> Result<bool, squadov_common::SquadOvError> {
        match self.refresh_and_obtain_valid_session_from_request(req).await {
            Ok(x) => match x {
                Some(_) => Ok(true),
                None => Ok(false),
            },
            Err(err) => return Err(squadov_common::SquadOvError::InternalError(format!("Is Logged In Error {}", err))),
        }
    }

    pub async fn logout(&self, session: &SquadOVSession) -> Result<(),  squadov_common::SquadOvError> {
        // Logout from FusionAuth AND delete the session from our database.
        // Both operations should be done regardless of whether the other one is successful.
        let fa_result = self.clients.fusionauth.logout(&session.refresh_token).await;
        let db_result = self.session.delete_session(&session.session_id, &*self.pool).await;

        match fa_result {
            Ok(_) => (),
            Err(err) => return Err(squadov_common::SquadOvError::InternalError(format!("Failed to logout (FA): {}", err)))
        };

        match db_result {
            Ok(_) => (),
            Err(err) => return Err(squadov_common::SquadOvError::InternalError(format!("Failed to logout (DB): {}", err)))
        }

        return Ok(());
    }
}