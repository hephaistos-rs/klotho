//! Accounts and credentials: users, passwords, web sessions, personal access
//! tokens and invites (docs/design/auth-flows.md).

use std::sync::OnceLock;
use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use serde::Serialize;

use crate::access::{Actor, Scope, Scopes};
use crate::config::Registration;
use crate::error::is_unique_violation;
use crate::names::OwnerName;
use crate::secret::{base62, crc32, random_base62, sha256};
use crate::{Core, Error, Result, db};

/// Shortest and longest password accepted. The upper bound keeps hashing cheap.
const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 8..=1024;
/// `last_seen_at` and `last_used_at` are written at most this often, to keep
/// writes off the hot path of every request.
const TOUCH_INTERVAL: Duration = Duration::from_secs(60);
const TOKEN_TOUCH_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Personal access tokens: the prefix secret scanners look for, the random
/// part, and a CRC-32 checksum of it (FR-AUTH-013).
const TOKEN_PREFIX: &str = "klotho_pat_";
const TOKEN_RANDOM_LEN: usize = 30;
const TOKEN_CHECKSUM_LEN: usize = 6;
/// Invite links are single-use and short-lived.
pub const INVITE_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// A user account, as permission checks need it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub suspended: bool,
}

/// What a new account starts with.
#[derive(Debug, Clone, Copy, Default)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub email: Option<&'a str>,
    /// Without one, the account can't sign in with a password.
    pub password: Option<&'a str>,
    pub admin: bool,
}

impl<'a> NewUser<'a> {
    /// A user with just a name: enough to own repositories.
    pub fn named(username: &'a str) -> Self {
        Self { username, ..Self::default() }
    }
}

/// A personal access token as listed; the secret itself is never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenInfo {
    pub id: i64,
    pub name: String,
    pub scopes: Scopes,
    pub created_at: jiff::Timestamp,
    pub expires_at: Option<jiff::Timestamp>,
    pub last_used_at: Option<jiff::Timestamp>,
}

/// A token just created. `secret` is shown once and then only its hash exists
/// (FR-AUTH-012).
#[derive(Debug, Clone)]
pub struct NewToken {
    pub info: TokenInfo,
    pub secret: String,
}

impl Core {
    /// Creates an account. For the CLI and administrators; sign-ups go through
    /// [`Self::register`], which checks the registration mode.
    pub async fn create_user(&self, new: NewUser<'_>) -> Result<User> {
        let name = OwnerName::parse_new(new.username)?;
        let email = new.email.map(check_email).transpose()?;
        let password_hash = match new.password {
            Some(password) => Some(hash_password(password).await?),
            None => None,
        };
        let mut tx = db::begin_write(&self.db).await?;
        let user = insert_user(&mut tx, &name, email, password_hash.as_deref(), new.admin).await?;
        tx.commit().await?;
        Ok(user)
    }

    /// Signs someone up, if the registration mode allows it (FR-AUTH-002). With
    /// invite-only registration, `invite` must be an unused, unexpired invite;
    /// it's used up by this call.
    pub async fn register(&self, new: NewUser<'_>, invite: Option<&str>) -> Result<User> {
        let mode = self.auth.registration;
        if mode == Registration::Disabled {
            return Err(Error::RegistrationClosed);
        }
        let name = OwnerName::parse_new(new.username)?;
        let email = check_email(
            new.email.ok_or_else(|| Error::InvalidInput("an email address is required".into()))?,
        )?;
        let password = new.password.ok_or_else(|| Error::InvalidInput("a password is required".into()))?;
        let password_hash = hash_password(password).await?;

        let mut tx = db::begin_write(&self.db).await?;
        if mode == Registration::Invite {
            let hash = sha256(invite.ok_or(Error::InvalidInvite)?.as_bytes());
            let hash = &hash[..];
            // Used up in the same statement that checks it, so two sign-ups
            // can't share one invite.
            let now = db::now();
            let used = sqlx::query!(
                "UPDATE one_time_tokens SET used_at = ?1
                 WHERE token_hash = ?2 AND purpose = 'invite' AND used_at IS NULL AND expires_at > ?1",
                now,
                hash
            )
            .execute(&mut *tx)
            .await?;
            if used.rows_affected() != 1 {
                return Err(Error::InvalidInvite);
            }
        }
        let user = insert_user(&mut tx, &name, Some(email), Some(&password_hash), false).await?;
        tx.commit().await?;
        tracing::info!(user = %user.username, "registered");
        Ok(user)
    }

    pub fn registration(&self) -> Registration {
        self.auth.registration
    }

    /// Makes an invite link's secret (FR-AUTH-002).
    pub async fn create_invite(&self) -> Result<String> {
        let secret = random_base62(32);
        let hash = sha256(secret.as_bytes());
        let hash = &hash[..];
        let now = db::now();
        let expires = now + INVITE_LIFETIME.as_secs() as i64;
        sqlx::query!(
            "INSERT INTO one_time_tokens (token_hash, purpose, created_at, expires_at) VALUES (?, 'invite', ?, ?)",
            hash,
            now,
            expires
        )
        .execute(&self.db)
        .await?;
        Ok(secret)
    }

    /// Looks a user up by username, case-insensitively.
    pub async fn find_user(&self, username: &str) -> Result<User> {
        let name = OwnerName::parse_lookup(username)?;
        let key = name.key();
        let row = sqlx::query!(
            r#"SELECT u.id AS "id!", o.name, u.is_admin AS "is_admin: bool", u.suspended_at
               FROM users u JOIN owners o ON o.id = u.id WHERE o.name_key = ?"#,
            key
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| Error::OwnerNotFound(name.to_string()))?;
        Ok(User {
            id: row.id,
            username: row.name,
            is_admin: row.is_admin,
            suspended: row.suspended_at.is_some(),
        })
    }

    /// Checks a username and password (auth-flows.md, "Password sign-in").
    /// Unknown users, wrong passwords, accounts without a password and
    /// suspended accounts all give the same error, and the password is always
    /// hashed, so neither the answer nor its timing tells which.
    pub async fn sign_in(&self, username: &str, password: &str) -> Result<User> {
        let found = match OwnerName::parse_lookup(username) {
            Ok(name) => {
                let key = name.key();
                sqlx::query!(
                    r#"SELECT u.id AS "id!", o.name, u.is_admin AS "is_admin: bool", u.suspended_at, u.password_hash
                       FROM users u JOIN owners o ON o.id = u.id WHERE o.name_key = ?"#,
                    key
                )
                .fetch_optional(&self.db)
                .await?
            }
            Err(_) => None,
        };
        let stored = found.as_ref().and_then(|row| row.password_hash.clone());
        let matches = verify_password(password, stored).await?;
        match found {
            Some(row) if matches && row.suspended_at.is_none() => {
                Ok(User { id: row.id, username: row.name, is_admin: row.is_admin, suspended: false })
            }
            _ => Err(Error::InvalidCredentials),
        }
    }

    /// Records a web session for `user_id`. `id_hash` is the SHA-256 of the
    /// cookie's token; the token itself is never stored.
    pub async fn start_session(&self, user_id: i64, id_hash: &[u8; 32]) -> Result<()> {
        let id_hash = &id_hash[..];
        let now = db::now();
        sqlx::query!(
            "INSERT INTO sessions (id_hash, user_id, created_at, last_seen_at) VALUES (?, ?, ?, ?)",
            id_hash,
            user_id,
            now,
            now
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// The user a session belongs to, if the session exists and hasn't expired
    /// (FR-AUTH-041). Expired sessions are deleted.
    pub async fn session_user(&self, id_hash: &[u8; 32]) -> Result<Option<User>> {
        let hash = &id_hash[..];
        let Some(row) = sqlx::query!(
            r#"SELECT s.created_at, s.last_seen_at, u.id AS "id!", o.name, u.is_admin AS "is_admin: bool", u.suspended_at
               FROM sessions s JOIN users u ON u.id = s.user_id JOIN owners o ON o.id = u.id
               WHERE s.id_hash = ?"#,
            hash
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        let now = db::now();
        let expired = now - row.created_at > seconds(self.auth.session_max())
            || now - row.last_seen_at > seconds(self.auth.session_idle());
        if expired || row.suspended_at.is_some() {
            self.end_session(id_hash).await?;
            return Ok(None);
        }
        if now - row.last_seen_at > seconds(TOUCH_INTERVAL) {
            sqlx::query!("UPDATE sessions SET last_seen_at = ? WHERE id_hash = ?", now, hash)
                .execute(&self.db)
                .await?;
        }
        Ok(Some(User { id: row.id, username: row.name, is_admin: row.is_admin, suspended: false }))
    }

    pub async fn end_session(&self, id_hash: &[u8; 32]) -> Result<()> {
        let hash = &id_hash[..];
        sqlx::query!("DELETE FROM sessions WHERE id_hash = ?", hash).execute(&self.db).await?;
        Ok(())
    }

    /// Creates a personal access token (FR-AUTH-011). Only administrators can
    /// give a token the `admin` scope.
    pub async fn create_token(
        &self,
        user: &User,
        name: &str,
        scopes: Scopes,
        expires_at: Option<jiff::Timestamp>,
    ) -> Result<NewToken> {
        let name = name.trim();
        if name.is_empty() || name.len() > 100 {
            return Err(Error::InvalidInput("a token needs a name of at most 100 characters".into()));
        }
        if scopes.is_empty() {
            return Err(Error::InvalidInput("a token needs at least one scope".into()));
        }
        if scopes.contains(Scope::Admin) && !user.is_admin {
            return Err(Error::InvalidInput(
                "only administrators can create tokens with the admin scope".into(),
            ));
        }
        if expires_at.is_some_and(|at| at <= jiff::Timestamp::now()) {
            return Err(Error::InvalidInput("the expiry date must be in the future".into()));
        }

        let random = random_base62(TOKEN_RANDOM_LEN);
        let secret = format!(
            "{TOKEN_PREFIX}{random}{}",
            base62(u64::from(crc32(random.as_bytes())), TOKEN_CHECKSUM_LEN)
        );
        let hash = sha256(secret.as_bytes());
        let hash = &hash[..];
        let (scope_text, now) = (scopes.to_string(), db::now());
        let expires = expires_at.map(|at| at.as_second());
        let id = sqlx::query_scalar!(
            r#"INSERT INTO access_tokens (user_id, name, token_hash, scopes, created_at, expires_at)
               VALUES (?, ?, ?, ?, ?, ?) RETURNING id AS "id!""#,
            user.id,
            name,
            hash,
            scope_text,
            now,
            expires
        )
        .fetch_one(&self.db)
        .await?;
        let info = TokenInfo {
            id,
            name: name.to_owned(),
            scopes,
            created_at: db::time(now),
            // As stored, in whole seconds.
            expires_at: expires.map(db::time),
            last_used_at: None,
        };
        Ok(NewToken { info, secret })
    }

    pub async fn list_tokens(&self, user_id: i64) -> Result<Vec<TokenInfo>> {
        let rows = sqlx::query!(
            r#"SELECT id AS "id!", name, scopes, created_at, expires_at, last_used_at
               FROM access_tokens WHERE user_id = ? ORDER BY id"#,
            user_id
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| TokenInfo {
                id: row.id,
                name: row.name,
                scopes: parse_scopes(&row.scopes),
                created_at: db::time(row.created_at),
                expires_at: row.expires_at.map(db::time),
                last_used_at: row.last_used_at.map(db::time),
            })
            .collect())
    }

    /// Revokes one of `user_id`'s tokens. Someone else's token is reported as
    /// not found.
    pub async fn revoke_token(&self, user_id: i64, token_id: i64) -> Result<()> {
        let deleted =
            sqlx::query!("DELETE FROM access_tokens WHERE id = ? AND user_id = ?", token_id, user_id)
                .execute(&self.db)
                .await?;
        if deleted.rows_affected() == 0 {
            return Err(Error::TokenNotFound);
        }
        Ok(())
    }

    /// The actor behind a personal access token, or `None` if it isn't a valid,
    /// unexpired token of an active account.
    pub async fn token_actor(&self, secret: &str) -> Result<Option<Actor>> {
        if !has_valid_checksum(secret) {
            return Ok(None);
        }
        let hash = sha256(secret.as_bytes());
        let hash = &hash[..];
        let now = db::now();
        let Some(row) = sqlx::query!(
            r#"SELECT t.id AS "token_id!", t.scopes, t.last_used_at,
                      u.id AS "id!", o.name, u.is_admin AS "is_admin: bool"
               FROM access_tokens t JOIN users u ON u.id = t.user_id JOIN owners o ON o.id = u.id
               WHERE t.token_hash = ?1 AND (t.expires_at IS NULL OR t.expires_at > ?2)
                 AND u.suspended_at IS NULL"#,
            hash,
            now
        )
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(None);
        };
        if row.last_used_at.is_none_or(|at| now - at > seconds(TOKEN_TOUCH_INTERVAL)) {
            sqlx::query!("UPDATE access_tokens SET last_used_at = ? WHERE id = ?", now, row.token_id)
                .execute(&self.db)
                .await?;
        }
        let user = User { id: row.id, username: row.name, is_admin: row.is_admin, suspended: false };
        Ok(Some(Actor::Token(user, parse_scopes(&row.scopes))))
    }
}

async fn insert_user(
    tx: &mut sqlx::SqliteConnection,
    name: &OwnerName,
    email: Option<&str>,
    password_hash: Option<&str>,
    admin: bool,
) -> Result<User> {
    let (display, key, now) = (name.as_str(), name.key(), db::now());
    let id = sqlx::query_scalar!(
        r#"INSERT INTO owners (kind, name, name_key, created_at) VALUES ('user', ?, ?, ?) RETURNING id AS "id!""#,
        display,
        key,
        now,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|err| if is_unique_violation(&err) { Error::OwnerExists(name.to_string()) } else { err.into() })?;
    sqlx::query!(
        "INSERT INTO users (id, password_hash, is_admin) VALUES (?, ?, ?)",
        id,
        password_hash,
        admin
    )
    .execute(&mut *tx)
    .await?;
    if let Some(email) = email {
        let email_key = email.to_lowercase();
        sqlx::query!(
            "INSERT INTO emails (user_id, address, address_key, is_primary) VALUES (?, ?, ?, 1)",
            id,
            email,
            email_key
        )
        .execute(&mut *tx)
        .await
        .map_err(|err| if is_unique_violation(&err) { Error::EmailExists } else { err.into() })?;
    }
    Ok(User { id, username: name.to_string(), is_admin: admin, suspended: false })
}

/// A minimal sanity check. Whether the address works is for email
/// verification (Phase 5) to find out.
fn check_email(email: &str) -> Result<&str> {
    let email = email.trim();
    let valid = email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if valid { Ok(email) } else { Err(Error::InvalidInput("that doesn't look like an email address".into())) }
}

fn has_valid_checksum(secret: &str) -> bool {
    let Some(rest) = secret.strip_prefix(TOKEN_PREFIX) else { return false };
    if rest.len() != TOKEN_RANDOM_LEN + TOKEN_CHECKSUM_LEN || !rest.is_ascii() {
        return false;
    }
    let (random, checksum) = rest.split_at(TOKEN_RANDOM_LEN);
    base62(u64::from(crc32(random.as_bytes())), TOKEN_CHECKSUM_LEN) == checksum
}

fn parse_scopes(text: &str) -> Scopes {
    text.parse().unwrap_or_else(|err| {
        tracing::error!(%err, text, "unreadable token scopes in the database");
        Scopes::default()
    })
}

/// A duration in whole seconds, to compare with stored times.
fn seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

/// Argon2id with the crate's defaults (at least OWASP's minimum), as a PHC
/// string so the parameters can be raised later. Runs on the blocking pool:
/// it's deliberately slow.
async fn hash_password(password: &str) -> Result<String> {
    if !PASSWORD_LEN.contains(&password.len()) {
        return Err(Error::InvalidInput(format!(
            "passwords must be {} to {} characters long",
            PASSWORD_LEN.start(),
            PASSWORD_LEN.end()
        )));
    }
    let password = password.to_owned();
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|err| Error::Internal(format!("hashing a password failed: {err}")))
    })
    .await?
}

/// Checks `password` against `stored`. Without a stored hash it checks against
/// a dummy one, so the time taken doesn't reveal whether the account exists.
async fn verify_password(password: &str, stored: Option<String>) -> Result<bool> {
    if password.len() > *PASSWORD_LEN.end() {
        return Ok(false);
    }
    let password = password.to_owned();
    Ok(tokio::task::spawn_blocking(move || {
        let (hash, real) = match &stored {
            Some(stored) => (stored.as_str(), true),
            None => (dummy_hash(), false),
        };
        let Ok(parsed) = PasswordHash::new(hash) else {
            tracing::error!("unreadable password hash in the database");
            return false;
        };
        Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok() && real
    })
    .await?)
}

fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| {
        Argon2::default()
            .hash_password(random_base62(32).as_bytes())
            .expect("hashing a random password works")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthConfig;
    use crate::testing::{core, core_with};

    #[tokio::test]
    async fn sign_in_checks_the_password_and_hides_why_it_failed() {
        let (_dir, core) = core().await;
        let alice = NewUser { password: Some("correct horse"), ..NewUser::named("alice") };
        core.create_user(alice).await.unwrap();
        core.create_user(NewUser::named("nopass")).await.unwrap();

        assert_eq!(core.sign_in("ALICE", "correct horse").await.unwrap().username, "alice");
        for (user, password) in [("alice", "wrong"), ("nobody", "correct horse"), ("nopass", "anything")] {
            assert!(matches!(core.sign_in(user, password).await, Err(Error::InvalidCredentials)), "{user}");
        }
    }

    #[tokio::test]
    async fn passwords_are_stored_as_argon2id() {
        let (_dir, core) = core().await;
        core.create_user(NewUser { password: Some("correct horse"), ..NewUser::named("alice") })
            .await
            .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT password_hash FROM users").fetch_one(&core.db).await.unwrap();
        assert!(stored.starts_with("$argon2id$"), "{stored}");
        assert!(matches!(
            core.create_user(NewUser { password: Some("short"), ..NewUser::named("bob") }).await,
            Err(Error::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn registration_follows_the_mode() {
        let new = NewUser {
            email: Some("a@example.com"),
            password: Some("correct horse"),
            ..NewUser::named("alice")
        };

        let (_dir, closed) = core().await;
        assert!(matches!(closed.register(new, None).await, Err(Error::RegistrationClosed)));

        let (_dir, open) =
            core_with(AuthConfig { registration: Registration::Open, ..AuthConfig::default() }).await;
        open.register(new, None).await.unwrap();
        let again = NewUser { username: "alice2", ..new };
        assert!(matches!(open.register(again, None).await, Err(Error::EmailExists)), "emails are unique");

        let (_dir, invite) =
            core_with(AuthConfig { registration: Registration::Invite, ..AuthConfig::default() }).await;
        assert!(matches!(invite.register(new, None).await, Err(Error::InvalidInvite)));
        assert!(matches!(invite.register(new, Some("made-up")).await, Err(Error::InvalidInvite)));
        let link = invite.create_invite().await.unwrap();
        invite.register(new, Some(&link)).await.unwrap();
        let bob = NewUser { username: "bob", email: Some("b@example.com"), ..new };
        assert!(matches!(invite.register(bob, Some(&link)).await, Err(Error::InvalidInvite)), "single use");
    }

    #[tokio::test]
    async fn sessions_expire_when_idle_or_too_old() {
        let (_dir, core) = core().await;
        let alice = core.create_user(NewUser::named("alice")).await.unwrap();
        let hash = sha256(b"session token");
        core.start_session(alice.id, &hash).await.unwrap();
        assert_eq!(core.session_user(&hash).await.unwrap(), Some(alice.clone()));

        let long_ago = db::now() - 15 * 24 * 60 * 60;
        sqlx::query!("UPDATE sessions SET last_seen_at = ?", long_ago).execute(&core.db).await.unwrap();
        assert_eq!(core.session_user(&hash).await.unwrap(), None, "idle for 15 days");

        core.start_session(alice.id, &hash).await.unwrap();
        let ancient = db::now() - 91 * 24 * 60 * 60;
        sqlx::query!("UPDATE sessions SET created_at = ?", ancient).execute(&core.db).await.unwrap();
        assert_eq!(core.session_user(&hash).await.unwrap(), None, "older than 90 days");
    }

    #[tokio::test]
    async fn tokens_authenticate_until_revoked_or_expired() {
        let (_dir, core) = core().await;
        let alice = core.create_user(NewUser::named("alice")).await.unwrap();
        let token = core.create_token(&alice, "ci", Scopes::new([Scope::RepoRead]), None).await.unwrap();
        assert!(token.secret.starts_with("klotho_pat_") && token.secret.len() == 47, "{}", token.secret);

        let actor = core.token_actor(&token.secret).await.unwrap();
        assert_eq!(actor, Some(Actor::Token(alice.clone(), Scopes::new([Scope::RepoRead]))));
        let mut typo = token.secret.clone();
        typo.replace_range(20..21, if &typo[20..21] == "a" { "b" } else { "a" });
        assert_eq!(core.token_actor(&typo).await.unwrap(), None);

        assert_eq!(core.list_tokens(alice.id).await.unwrap().len(), 1);
        core.revoke_token(alice.id, token.info.id).await.unwrap();
        assert_eq!(core.token_actor(&token.secret).await.unwrap(), None);
        assert!(matches!(core.revoke_token(alice.id, token.info.id).await, Err(Error::TokenNotFound)));

        let soon = jiff::Timestamp::now() + jiff::SignedDuration::from_secs(3600);
        let expiring =
            core.create_token(&alice, "x", Scopes::new([Scope::RepoRead]), Some(soon)).await.unwrap();
        let past = db::now() - 1;
        sqlx::query!("UPDATE access_tokens SET expires_at = ?", past).execute(&core.db).await.unwrap();
        assert_eq!(core.token_actor(&expiring.secret).await.unwrap(), None);
    }

    #[tokio::test]
    async fn only_admins_get_admin_tokens() {
        let (_dir, core) = core().await;
        let alice = core.create_user(NewUser::named("alice")).await.unwrap();
        let root = core.create_user(NewUser { admin: true, ..NewUser::named("root") }).await.unwrap();
        let admin = Scopes::new([Scope::Admin]);
        assert!(matches!(
            core.create_token(&alice, "x", admin.clone(), None).await,
            Err(Error::InvalidInput(_))
        ));
        core.create_token(&root, "x", admin, None).await.unwrap();
    }
}
