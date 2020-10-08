use actix_web::{error, HttpResponse, http::StatusCode, dev::HttpResponseBuilder};
use derive_more::{Display};

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
            SquadOvError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            
        }
    }
}