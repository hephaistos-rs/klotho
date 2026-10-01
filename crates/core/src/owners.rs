use serde::Serialize;

use crate::error::is_unique_violation;
use crate::names::OwnerName;
use crate::{Core, Error, Result, db};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    User,
    Org,
}

impl OwnerKind {
    pub(crate) fn from_db(kind: &str) -> Self {
        if kind == "org" { Self::Org } else { Self::User }
    }
}

/// A user or organisation: anything that can own repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Owner {
    pub id: i64,
    /// The display form, as typed.
    #[serde(rename = "username")]
    pub name: String,
    pub kind: OwnerKind,
}

impl Core {
    /// Creates a user. Accounts (passwords, emails) arrive in Phase 2; until then a
    /// user is just a name that can own repositories.
    pub async fn create_user(&self, name: &str) -> Result<Owner> {
        let name = OwnerName::parse_new(name)?;
        let (display, key, now) = (name.as_str(), name.key(), db::now());
        let mut tx = self.db.begin().await?;
        let id = sqlx::query_scalar!(
            r#"INSERT INTO owners (kind, name, name_key, created_at) VALUES ('user', ?, ?, ?) RETURNING id AS "id!""#,
            display,
            key,
            now,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| if is_unique_violation(&err) { Error::OwnerExists(name.to_string()) } else { err.into() })?;
        sqlx::query!("INSERT INTO users (id) VALUES (?)", id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Owner { id, name: name.to_string(), kind: OwnerKind::User })
    }

    /// Looks an owner up by name, case-insensitively (FR-NAME-021).
    pub async fn find_owner(&self, name: &str) -> Result<Owner> {
        let name = OwnerName::parse_lookup(name)?;
        let key = name.key();
        let row = sqlx::query!(r#"SELECT id AS "id!", name, kind FROM owners WHERE name_key = ?"#, key)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| Error::OwnerNotFound(name.to_string()))?;
        Ok(Owner { id: row.id, name: row.name, kind: OwnerKind::from_db(&row.kind) })
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::core;
    use crate::{Error, OwnerKind};

    #[tokio::test]
    async fn create_and_find_users_case_insensitively() {
        let (_dir, core) = core().await;
        let alice = core.create_user("Alice").await.unwrap();
        assert_eq!(alice.name, "Alice");
        assert_eq!(alice.kind, OwnerKind::User);
        assert_eq!(core.find_owner("ALICE").await.unwrap(), alice);
        assert!(matches!(core.find_owner("bob").await, Err(Error::OwnerNotFound(_))));
    }

    #[tokio::test]
    async fn names_are_unique_by_key() {
        let (_dir, core) = core().await;
        core.create_user("alice").await.unwrap();
        assert!(matches!(core.create_user("ALICE").await, Err(Error::OwnerExists(_))));
    }

    #[tokio::test]
    async fn reserved_names_are_rejected() {
        let (_dir, core) = core().await;
        assert!(matches!(core.create_user("api").await, Err(Error::InvalidName(_))));
    }
}
