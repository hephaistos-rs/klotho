use axum::Json;
use axum::http::{StatusCode, header};
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

    /// `401` with a `WWW-Authenticate: Bearer` challenge.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthenticated", message)
    }

    fn internal(err: &dyn std::fmt::Debug) -> Self {
        tracing::error!(error = ?err, "request failed");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal server error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({ "code": self.code, "message": self.message }));
        if self.status == StatusCode::UNAUTHORIZED {
            return (self.status, [(header::WWW_AUTHENTICATE, "Bearer realm=\"Klotho\"")], body)
                .into_response();
        }
        (self.status, body).into_response()
    }
}

impl From<Error> for ApiError {
    fn from(err: Error) -> Self {
        use StatusCode as S;
        let (status, code) = match &err {
            Error::InvalidName(_) => (S::BAD_REQUEST, "name_invalid"),
            Error::OwnerNotFound(_) => (S::NOT_FOUND, "owner_not_found"),
            // The same for a private repository the caller can't see (FR-ACL-013).
            Error::RepoNotFound(_) => (S::NOT_FOUND, "repo_not_found"),
            Error::NotUnadopted(_) => (S::NOT_FOUND, "unadopted_not_found"),
            Error::TokenNotFound => (S::NOT_FOUND, "token_not_found"),
            Error::OwnerExists(_) => (S::CONFLICT, "owner_exists"),
            Error::RepoExists(_) => (S::CONFLICT, "repo_exists"),
            Error::EmailExists => (S::CONFLICT, "email_exists"),
            Error::InvalidInput(_) => (S::UNPROCESSABLE_ENTITY, "invalid"),
            Error::InvalidCredentials => (S::UNAUTHORIZED, "unauthenticated"),
            Error::Forbidden => (S::FORBIDDEN, "forbidden"),
            Error::RegistrationClosed => (S::FORBIDDEN, "registration_closed"),
            Error::InvalidInvite => (S::BAD_REQUEST, "invite_invalid"),
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

impl From<klotho_git::Error> for ApiError {
    fn from(err: klotho_git::Error) -> Self {
        Error::from(err).into()
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(err: tokio::task::JoinError) -> Self {
        Error::from(err).into()
    }
}
