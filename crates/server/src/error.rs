use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Error type for handlers: known `klotho_git::Error`s map to 4xx, everything else is a
/// logged 500 whose details are not sent to the client.
#[derive(Debug)]
pub enum AppError {
    Git(klotho_git::Error),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Git(err) => match err {
                klotho_git::Error::InvalidName(_) => StatusCode::BAD_REQUEST,
                klotho_git::Error::RepoNotFound(_)
                | klotho_git::Error::RevisionNotFound(_)
                | klotho_git::Error::PathNotFound(_) => StatusCode::NOT_FOUND,
                klotho_git::Error::RepoExists(_) => StatusCode::CONFLICT,
                klotho_git::Error::Io(_) | klotho_git::Error::Git(_) => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self, "request failed");
            "internal server error".to_owned()
        } else {
            match self {
                Self::Git(err) => err.to_string(),
                Self::Internal(err) => err.to_string(),
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<klotho_git::Error> for AppError {
    fn from(err: klotho_git::Error) -> Self {
        Self::Git(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Internal(err.into())
    }
}
