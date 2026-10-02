//! Web sessions (docs/design/auth-flows.md, "Sessions and CSRF").
//!
//! Topcoat mints the token and carries it in a cookie; the `sessions` table
//! (in `klotho-core`) holds its hash and the user. Pages call [`current_user`]
//! or [`require_user`] themselves ("functions, not middlewares").

use std::time::Duration;

use klotho_core::{Core, User};
use topcoat::Result;
use topcoat::context::{Cx, app_context, memoize};
use topcoat::cookie::{Cookie, Cookies, SameSite, cookies};
use topcoat::router::error::{RedirectError, redirect};
use topcoat::router::request::uri;
use topcoat::session::{self, Token, TokenStore, TokenStoreFuture};

/// The session cookie. Over HTTPS it's `__Host-klotho_session` with `Secure`
/// (FR-AUTH-040, 045). Over plain HTTP, which only makes sense on
/// `http://localhost` during development, browsers would drop a `Secure` or
/// `__Host-` cookie, so both are left off.
pub struct SessionCookie {
    pub secure: bool,
}

const NAME: &str = "klotho_session";

impl SessionCookie {
    fn jar<'a>(&self, cx: &'a Cx) -> Box<dyn CookieJarLike + 'a> {
        let base = cookies(cx).override_same_site(SameSite::Lax).override_http_only(true).override_path("/");
        if self.secure {
            Box::new(base.override_secure(true).override_prefix_host())
        } else {
            Box::new(base.override_secure(false))
        }
    }
}

/// The cookie operations the store needs, over both jar configurations.
trait CookieJarLike {
    fn get(&self, name: &str) -> Option<Cookie<'static>>;
    fn add_with_max_age(&self, cookie: Cookie<'static>, max_age: Duration) -> Result<()>;
    fn remove(&self, cookie: Cookie<'static>);
}

impl<C: Cookies> CookieJarLike for C {
    fn get(&self, name: &str) -> Option<Cookie<'static>> {
        Cookies::get(self, name)
    }

    fn add_with_max_age(&self, mut cookie: Cookie<'static>, max_age: Duration) -> Result<()> {
        cookie.set_max_age(topcoat::cookie::time::Duration::try_from(max_age)?);
        Cookies::add(self, cookie);
        Ok(())
    }

    fn remove(&self, cookie: Cookie<'static>) {
        Cookies::remove(self, cookie);
    }
}

impl TokenStore for SessionCookie {
    fn read<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, Option<Token>> {
        Box::pin(async move {
            Ok(self.jar(cx).get(NAME).and_then(|cookie| Token::decode(cookie.value_trimmed()).ok()))
        })
    }

    fn write<'a>(&'a self, cx: &'a Cx, token: Token, max_age: Duration) -> TokenStoreFuture<'a, ()> {
        Box::pin(async move { self.jar(cx).add_with_max_age(Cookie::new(NAME, token.encode()), max_age) })
    }

    fn delete<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, ()> {
        Box::pin(async move {
            self.jar(cx).remove(Cookie::new(NAME, ""));
            Ok(())
        })
    }
}

pub fn core(cx: &Cx) -> &Core {
    app_context::<Core>(cx)
}

/// The signed-in user, if any. Looked up once per request.
#[memoize(as_ref)]
pub async fn current_user(cx: &Cx) -> Option<User> {
    let hash = session::token_hash(cx).await.ok()??;
    match core(cx).session_user(&hash).await {
        Ok(user) => user,
        Err(err) => {
            tracing::error!(%err, "looking up a session failed");
            None
        }
    }
}

/// The signed-in user, or a redirect to the login page that comes back here.
pub async fn require_user(cx: &Cx) -> Result<&User, RedirectError> {
    match current_user(cx).await {
        Some(user) => Ok(user),
        None => {
            let here = uri(cx).path_and_query().map_or("/", |path| path.as_str());
            Err(redirect(format!("/-/login?return_to={}", encode(here))))
        }
    }
}

/// Signs `user` in: a fresh token (so a planted one is useless, FR-AUTH-045)
/// and its hash recorded.
pub async fn sign_in(cx: &Cx, user: &User) -> Result<()> {
    let session = session::start(cx).await?;
    core(cx).start_session(user.id, &session.token_hash).await?;
    Ok(())
}

pub async fn sign_out(cx: &Cx) -> Result<()> {
    if let Some(hash) = session::stop(cx).await? {
        core(cx).end_session(&hash).await?;
    }
    Ok(())
}

/// Where to go after signing in: a path on this site, or the home page. Never
/// `//host` or an absolute URL, which would make this an open redirect.
pub fn safe_return_to(return_to: Option<&str>) -> &str {
    match return_to {
        Some(path) if path.starts_with('/') && !path.starts_with("//") && !path.starts_with("/\\") => path,
        _ => "/",
    }
}

/// Percent-encodes a value for a query string.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_to_stays_on_this_site() {
        assert_eq!(safe_return_to(Some("/alice/demo")), "/alice/demo");
        for bad in ["//evil.example", "https://evil.example", "/\\evil.example", "relative"] {
            assert_eq!(safe_return_to(Some(bad)), "/", "{bad}");
        }
        assert_eq!(safe_return_to(None), "/");
    }
}
