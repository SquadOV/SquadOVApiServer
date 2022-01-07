use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::SquadOvError;
use crate::api::{
    ApiApplication,
    auth::SquadOVSession,
};
use std::sync::Arc;
use serde::Serialize;
use rusoto_cognito_identity::{
    CognitoIdentity,
    GetOpenIdTokenForDeveloperIdentityInput,
};
use std::collections::HashMap;

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct AwsCognitoCredentials {
    token: String,
    idp: String,
    identity_id: String,
    pool_id: String,
    region: String,
    account_id: String,
}

pub async fn get_aws_credentials_handler(app : web::Data<Arc<ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    if let Some(aws) = app.aws.as_ref() {
        let mut logins: HashMap<String, String> = HashMap::new();
        logins.insert(app.config.aws.cognito.provider.clone(), String::from(session.user.uuid.to_hyphenated().to_string()));

        let result = aws.cognito.get_open_id_token_for_developer_identity(GetOpenIdTokenForDeveloperIdentityInput{
            identity_pool_id: app.config.aws.cognito.pool_id.clone(),
            logins,
            // Giving us a LARGE buffer around how long a session is valid for.
            token_duration: Some(7200i64),
            ..GetOpenIdTokenForDeveloperIdentityInput::default()
        }).await?;

        let creds = AwsCognitoCredentials{
            token: result.token.ok_or(SquadOvError::BadRequest)?,
            idp: app.config.aws.cognito.provider.clone(),
            identity_id: result.identity_id.ok_or(SquadOvError::BadRequest)?,
            pool_id: app.config.aws.cognito.pool_id.clone(),
            region: String::from(aws.region.name()),
            account_id: app.config.aws.account_id.clone(),
        };
        Ok(HttpResponse::Ok().json(&creds))
    } else {
        Err(SquadOvError::InternalError(String::from("AWS not enabled.")))
    }
}