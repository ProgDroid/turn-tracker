//! Application-level error type for HTTP responses.

use actix_web::{HttpResponse, ResponseError, http::StatusCode};

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Invalid request")]
    InvalidRequest,
    #[error("Name must be between 1 and 40 characters")]
    NameTooLong,
    #[error("Room not found")]
    RoomNotFound,
    #[error("Could not allocate a unique room code")]
    CodeExhausted,
    #[error("Server is at capacity; try again later")]
    RoomCapacityReached,
    #[error("Hosting is invite-only during the beta")]
    CreateForbidden,
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .json(serde_json::json!({ "error": self.to_string() }))
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest | Self::NameTooLong => StatusCode::BAD_REQUEST,
            Self::RoomNotFound => StatusCode::NOT_FOUND,
            Self::CodeExhausted | Self::RoomCapacityReached => StatusCode::SERVICE_UNAVAILABLE,
            Self::CreateForbidden => StatusCode::FORBIDDEN,
        }
    }
}
