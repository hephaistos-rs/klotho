//! Who is making an API or git request, from its `Authorization` header.
//!
//! - The API takes `Authorization: Bearer <token>` only (FR-AUTH-022). It never
//!   looks at the session cookie, so a cookie can't be abused for cross-site
//!   requests against it.
//! - Git takes HTTP Basic with the token as the password (FR-AUTH-020). The
//!   username is ignored. Account passwords are refused (FR-AUTH-021).
//!
//! Missing credentials make an anonymous actor; wrong ones are an error, never
//! silently anonymous.

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use klotho_core::{Actor, Core, Scope, User};

use crate::error::ApiError;

/// The API's actor. A malformed or unknown token gets `401`.
pub async fn api_actor(core: &Core, headers: &HeaderMap) -> Result<Actor, ApiError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(Actor::Anonymous);
    };
    let token = value
        .to_str()
        .ok()
        .and_then(|value| value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")))
        .map(str::trim)
        .ok_or_else(|| ApiError::unauthenticated("use Authorization: Bearer <token>"))?;
    core.token_actor(token).await?.ok_or_else(|| ApiError::unauthenticated("invalid or expired token"))
}

/// The signed-in user behind an API actor, if their token has `scope`.
pub fn require_user(actor: &Actor, scope: Scope) -> Result<&User, ApiError> {
    let user = actor.user().ok_or_else(|| ApiError::unauthenticated("this needs a token"))?;
    if !actor.has_scope(scope) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            format!("this needs a token with the {scope} scope"),
        ));
    }
    Ok(user)
}

/// An instance administrator with an `admin`-scoped credential.
pub fn require_admin(actor: &Actor) -> Result<(), ApiError> {
    match actor {
        Actor::Anonymous => Err(ApiError::unauthenticated("this needs a token")),
        _ if actor.is_admin() => Ok(()),
        _ => Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "this needs an administrator's admin-scoped token",
        )),
    }
}

/// Git's actor. Wrong credentials get the challenge again, which git reports
/// as "Authentication failed".
pub async fn git_actor(core: &Core, headers: &HeaderMap) -> Result<Actor, Box<Response>> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(Actor::Anonymous);
    };
    let password = value.to_str().ok().and_then(basic_password);
    let actor = match password {
        Some(password) => {
            core.token_actor(&password).await.map_err(|err| Box::new(ApiError::from(err).into_response()))?
        }
        None => None,
    };
    actor.ok_or_else(|| Box::new(challenge()))
}

/// The `401` that makes git ask for credentials (FR-AUTH-023).
pub fn challenge() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"Klotho\"")],
        "Authentication required. Use a personal access token as the password.\n",
    )
        .into_response()
}

/// The password from a `Basic` header value.
fn basic_password(value: &str) -> Option<String> {
    let encoded = value.strip_prefix("Basic ").or_else(|| value.strip_prefix("basic "))?;
    let decoded = String::from_utf8(base64_decode(encoded.trim())?).ok()?;
    decoded.split_once(':').map(|(_, password)| password.to_owned())
}

/// Standard base64, as HTTP Basic uses. Small enough not to need a crate.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_password_from_basic_credentials() {
        // "alice:klotho_pat_x" and "user:pass:with:colons"
        assert_eq!(basic_password("Basic YWxpY2U6a2xvdGhvX3BhdF94").as_deref(), Some("klotho_pat_x"));
        assert_eq!(basic_password("Basic dXNlcjpwYXNzOndpdGg6Y29sb25z").as_deref(), Some("pass:with:colons"));
        assert_eq!(basic_password("Bearer abc"), None);
        assert_eq!(basic_password("Basic !!!"), None);
    }
}
