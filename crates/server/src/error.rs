use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use klotho_core::Error;
use serde_json::json;

/// A handler error, sent as `{"code": …, "message": …}` (FR-API-030). `code` is
/// stable for clients to branch on; `message` is for people and may change.
/// Internal failures are logged and answered with a generic 500 that reveals
/// nothing (NFR-SEC-030).
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }

    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "not found")
    }

    fn internal(err: &dyn std::fmt::Debug) -> Self {
        tracing::error!(error = ?err, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal server error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "code": self.code, "message": self.message }))).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        use StatusCode as S;
        let (status, code) = match &err {
            Error::InvalidName(_) => (S::BAD_REQUEST, "name_invalid"),
            Error::OwnerNotFound(_) => (S::NOT_FOUND, "owner_not_found"),
            Error::RepoNotFound(_) => (S::NOT_FOUND, "repo_not_found"),
            Error::NotUnadopted(_) => (S::NOT_FOUND, "unadopted_not_found"),
            Error::OwnerExists(_) => (S::CONFLICT, "owner_exists"),
            Error::RepoExists(_) => (S::CONFLICT, "repo_exists"),
            Error::Git(klotho_git::Error::RevisionNotFound(_)) => (S::NOT_FOUND, "revision_not_found"),
            Error::Git(klotho_git::Error::PathNotFound(_)) => (S::NOT_FOUND, "path_not_found"),
            _ => return Self::internal(&err),
        };
        Self::new(status, code, err.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        Self::internal(&err)
    }
}
