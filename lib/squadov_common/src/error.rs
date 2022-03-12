use actix_web::{error, HttpResponse, http::StatusCode, HttpResponseBuilder};
use derive_more::{Display};
use sqlx;
use url;
use std::str;
use std::io;
use jsonwebtoken;
use reqwest;
use openssl;
use serde::Serialize;
use rusoto_core::RusotoError;
use rusoto_credential::CredentialsError;

#[derive(Serialize)]
struct ErrorBody {
    #[serde(rename="duplicateFlag")]
    duplicate_flag: bool
}

#[derive(Debug, Display, PartialEq)]
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
    #[display(fmt = "[SquadovError] Duplicate Error")]
    Duplicate,
    #[display(fmt = "[SquadovError] Defer: {}", _0)]
    Defer(i64),
    #[display(fmt = "[SquadovError] Rate Limit")]
    RateLimit,
    #[display(fmt = "[SquadovError] Two Factor Request")]
    TwoFactor(String),
    #[display(fmt = "[SquadovError] Forbidden")]
    Forbidden,
    #[display(fmt = "[SquadovError] Failover")]
    Failover,
    #[display(fmt = "[SquadovError] Switch Queue")]
    SwitchQueue(String),
}

impl std::error::Error for SquadOvError {}

impl error::ResponseError for SquadOvError {
    fn error_response(&self) -> HttpResponse {
        let body = ErrorBody{
            duplicate_flag: *self == SquadOvError::Duplicate
        };

        HttpResponseBuilder::new(self.status_code()).json(&body)
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            SquadOvError::Credentials => StatusCode::UNAUTHORIZED,
            SquadOvError::Unauthorized => StatusCode::UNAUTHORIZED,
            SquadOvError::BadRequest => StatusCode::BAD_REQUEST,
            SquadOvError::NotFound => StatusCode::NOT_FOUND,
            SquadOvError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SquadOvError::Duplicate => StatusCode::BAD_REQUEST,
            SquadOvError::Defer(_) | SquadOvError::Failover | SquadOvError::SwitchQueue(_) => StatusCode::SERVICE_UNAVAILABLE,
            SquadOvError::RateLimit => StatusCode::TOO_MANY_REQUESTS,
            SquadOvError::TwoFactor(_) => StatusCode::ACCEPTED,
            SquadOvError::Forbidden => StatusCode::FORBIDDEN,
        }
    }
}

impl From<sqlx::Error> for SquadOvError {
    fn from(err: sqlx::Error) -> Self {
        let generic_error = Self::InternalError(format!("Database Error {}", err));
        match err {
            sqlx::Error::Database(db_err) => {
                let code = db_err.code();
                // Check for the Postgres duplicate key violates unique constraint error
                // so that we can return a "Duplicate" error instead so that we can give
                // more context to the user.
                if code.is_some() {
                    let code = code.unwrap();
                    if code == "23505" || code == "23P01" {
                        Self::Duplicate
                    } else {
                        generic_error
                    }
                } else {
                    generic_error
                }
            },
            _ => generic_error
        }
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

impl From<io::Error> for SquadOvError {
    fn from(err: io::Error) -> Self {
        return Self::InternalError(format!("IO Error {}", err))
    }
}

impl From<jsonwebtoken::errors::Error> for SquadOvError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        return Self::InternalError(format!("JWT Error {}", err))
    }
}

impl From<reqwest::Error> for SquadOvError {
    fn from(err: reqwest::Error) -> Self {
        return Self::InternalError(format!("HTTP Request Error {}", err))
    }
}

impl From<reqwest::header::InvalidHeaderValue> for SquadOvError {
    fn from(err: reqwest::header::InvalidHeaderValue) -> Self {
        return Self::InternalError(format!("HTTP Header Value Error {}", err))
    }
}

impl From<openssl::error::ErrorStack> for SquadOvError {
    fn from(err: openssl::error::ErrorStack) -> Self {
        return Self::InternalError(format!("OpenSSL Error {:?}", err))
    }
}

impl From<std::num::ParseIntError> for SquadOvError {
    fn from(err: std::num::ParseIntError) -> Self {
        return Self::InternalError(format!("Parse Int {}", err))
    }
}

impl From<std::num::ParseFloatError> for SquadOvError {
    fn from(err: std::num::ParseFloatError) -> Self {
        return Self::InternalError(format!("Parse Float {}", err))
    }
}

impl<T : num_enum::TryFromPrimitive> From<num_enum::TryFromPrimitiveError<T>> for SquadOvError {
    fn from(err: num_enum::TryFromPrimitiveError<T>) -> Self {
        return Self::InternalError(format!("TryFromPrimitive {}", err))
    }
}

impl<T> From<std::sync::PoisonError<T>> for SquadOvError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        return Self::InternalError(format!("Sync PoisonError {}", err))
    }
}

impl From<actix_web::error::PayloadError> for SquadOvError {
    fn from(err: actix_web::error::PayloadError) -> Self {
        return Self::InternalError(format!("Payload {}", err))
    }
}

impl From<prost::EncodeError> for SquadOvError {
    fn from(err: prost::EncodeError) -> Self {
        return Self::InternalError(format!("Proto Encode {}", err))
    }
}

impl From<std::env::VarError> for SquadOvError {
    fn from(err: std::env::VarError) -> Self {
        return Self::InternalError(format!("Env Var {}", err))
    }
}

impl<T> From<std::sync::mpsc::SendError<T>> for SquadOvError {
    fn from(err: std::sync::mpsc::SendError<T>) -> Self {
        return Self::InternalError(format!("MPSC Send Error {}", err))
    }
}

impl From<actix_multipart::MultipartError> for SquadOvError {
    fn from(err: actix_multipart::MultipartError) -> Self {
        return Self::InternalError(format!("Multipart Error {}", err))
    }
}

impl From<reqwest::header::ToStrError> for SquadOvError {
    fn from(err: reqwest::header::ToStrError) -> Self {
        return Self::InternalError(format!("To Str {}", err))
    }
}

impl From<std::array::TryFromSliceError> for SquadOvError {
    fn from(err: std::array::TryFromSliceError) -> Self {
        return Self::InternalError(format!("Slice into Array {}", err))
    }
}

impl From<lapin::Error> for SquadOvError {
    fn from(err: lapin::Error) -> Self {
        return Self::InternalError(format!("AMQP {}", err))
    }
}

impl From<actix_web::Error> for SquadOvError {
    fn from(err: actix_web::Error) -> Self {
        return Self::InternalError(format!("Actix Web {}", err))
    }
}

impl From<hex::FromHexError> for SquadOvError {
    fn from(err: hex::FromHexError) -> Self {
        return Self::InternalError(format!("Hex Decode {}", err))
    }
}

impl<T : std::fmt::Debug> From<nom::Err<nom::error::Error<T>>> for SquadOvError {
    fn from(err: nom::Err<nom::error::Error<T>>) -> Self {
        return Self::InternalError(format!("Nom {:?}", err))
    }
}

impl From<prost::DecodeError> for SquadOvError {
    fn from(err: prost::DecodeError) -> Self {
        return Self::InternalError(format!("Prost Decode {:?}", err))
    }
}

impl From<std::string::FromUtf8Error> for SquadOvError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        return Self::InternalError(format!("From utf8 {:?}", err))
    }
}

impl From<std::str::ParseBoolError> for SquadOvError {
    fn from(err: std::str::ParseBoolError) -> Self {
        return Self::InternalError(format!("Parse Bool {:?}", err))
    }
}

impl<T: std::fmt::Debug> From<RusotoError<T>> for SquadOvError {
    fn from(err: RusotoError<T>) -> Self {
        return Self::InternalError(format!("Rusoto (AWS) {:?}", err))
    }
}

impl From<CredentialsError> for SquadOvError {
    fn from(err: CredentialsError) -> Self {
        return Self::InternalError(format!("Rusoto (AWS, Creds) {:?}", err))
    }
}

impl From<image::ImageError> for SquadOvError {
    fn from(err: image::ImageError) -> Self {
        return Self::InternalError(format!("Image Error {:?}", err))
    }
}

impl From<harsh::Error> for SquadOvError {
    fn from(err: harsh::Error) -> Self {
        return Self::InternalError(format!("HashId Error {:?}", err))
    }
}

impl From<std::net::AddrParseError> for SquadOvError {
    fn from(err: std::net::AddrParseError) -> Self {
        return Self::InternalError(format!("IP Parse Error {:?}", err))
    }
}

impl From<ipnetwork::IpNetworkError> for SquadOvError {
    fn from(err: ipnetwork::IpNetworkError) -> Self {
        return Self::InternalError(format!("IP Network Error {:?}", err))
    }
}

impl From<rsa::pkcs1::Error> for SquadOvError {
    fn from(err: rsa::pkcs1::Error) -> Self {
        return Self::InternalError(format!("PKCS1 Error {:?}", err))
    }
}

impl From<rsa::errors::Error> for SquadOvError {
    fn from(err: rsa::errors::Error) -> Self {
        return Self::InternalError(format!("RSA Error {:?}", err))
    }
}

impl From<hmac::crypto_mac::InvalidKeyLength> for SquadOvError {
    fn from(err: hmac::crypto_mac::InvalidKeyLength) -> Self {
        return Self::InternalError(format!("HMAC Error {:?}", err))
    }
}

impl From<deadpool_redis::PoolError> for SquadOvError {
    fn from(err: deadpool_redis::PoolError) -> Self {
        return Self::InternalError(format!("Deadpool Redis Error {:?}", err))
    }
}

impl From<redis::RedisError> for SquadOvError {
    fn from(err: redis::RedisError) -> Self {
        return Self::InternalError(format!("Redis Error {:?}", err))
    }
}

impl From<Box<dyn std::error::Error + Send + Sync + 'static>> for SquadOvError {
    fn from(err: Box<dyn std::error::Error + Send + Sync + 'static>) -> Self {
        return Self::InternalError(format!("Generic Error {:?}", err))
    }
}

impl From<rusoto_core::region::ParseRegionError> for SquadOvError {
    fn from(err: rusoto_core::region::ParseRegionError) -> Self {
        return Self::InternalError(format!("Rusoto Parse Region Error {:?}", err))
    }
}

impl From<tokio::sync::AcquireError> for SquadOvError {
    fn from(err: tokio::sync::AcquireError) -> Self {
        return Self::InternalError(format!("Tokio Sync AcquireError {:?}", err))
    }
}

impl From<chrono::ParseError> for SquadOvError {
    fn from(err: chrono::ParseError) -> Self {
        return Self::InternalError(format!("DateTime Parse Error {:?}", err))
    }
}

impl From<tokio::task::JoinError> for SquadOvError {
    fn from(err: tokio::task::JoinError) -> Self {
        return Self::InternalError(format!("Tokio JoinError {:?}", err))
    }
}

impl From<avro_rs::DeError> for SquadOvError {
    fn from(err: avro_rs::DeError) -> Self {
        return Self::InternalError(format!("Avro Deserialize Error {:?}", err))
    }
}