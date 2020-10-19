use actix_web::{error, HttpResponse, http::StatusCode, dev::HttpResponseBuilder};
use derive_more::{Display};
use sqlx;
use url;
use std::str;

#[macro_export]
macro_rules! logged_error {
    ($x:expr) => {{
        warn!("{}", $x); Err($x)
    }};
}

#[derive(Debug, Display)]
pub enum SquadOvError {
    #[display(fmt = "[SquadovError] Invalid credentials.")]
    Credentials,
    #[display(fmt = "[SquadovError] Unauthorized Access")]
    Unauthorized,
    #[display(fmt = "[SquadovError] Invalid Request")]
    BadRequest,
    #[display(fmt = "[SquadovError] Not found")]
    NotFound,
    #[display(fmt = "[SquadovError] Internal Error: {}", _0)]
    InternalError(String),
}

impl error::ResponseError for SquadOvError {
    fn error_response(&self) -> HttpResponse {
        HttpResponseBuilder::new(self.status_code()).finish()
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            SquadOvError::Credentials => StatusCode::UNAUTHORIZED,
            SquadOvError::Unauthorized => StatusCode::UNAUTHORIZED,
            SquadOvError::BadRequest => StatusCode::BAD_REQUEST,
            SquadOvError::NotFound => StatusCode::NOT_FOUND,
            SquadOvError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<sqlx::Error> for SquadOvError {
    fn from(err: sqlx::Error) -> Self {
        return Self::InternalError(format!("Database Error {}", err))
    }
}

impl From<serde_json::Error> for SquadOvError {
    fn from(err: serde_json::Error) -> Self {
        return Self::InternalError(format!("Parse JSON Error {}", err))
    }
}

impl From<url::ParseError> for SquadOvError {
    fn from(err: url::ParseError) -> Self {
        return Self::InternalError(format!("Parse URL Error {}", err))
    }
}
impl From<str::Utf8Error> for SquadOvError {
    fn from(err: str::Utf8Error) -> Self {
        return Self::InternalError(format!("String from UTF-8 Bytes Error {}", err))
    }
}

impl From<uuid::Error> for SquadOvError {
    fn from(err: uuid::Error) -> Self {
        return Self::InternalError(format!("Parse UUID Error {}", err))
    }
}

impl From<base64::DecodeError> for SquadOvError {
    fn from(err: base64::DecodeError) -> Self {
        return Self::InternalError(format!("Base64 Decode Error {}", err))
    }
}