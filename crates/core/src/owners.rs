use serde::Serialize;

use crate::names::OwnerName;
use crate::{Core, Error, Result};

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
    use crate::{Error, NewUser, OwnerKind};

    #[tokio::test]
    async fn create_and_find_users_case_insensitively() {
        let (_dir, core) = core().await;
        let alice = core.create_user(NewUser::named("Alice")).await.unwrap();
        assert_eq!(alice.username, "Alice");
        let owner = core.find_owner("ALICE").await.unwrap();
        assert_eq!((owner.id, owner.name.as_str(), owner.kind), (alice.id, "Alice", OwnerKind::User));
        assert!(matches!(core.find_owner("bob").await, Err(Error::OwnerNotFound(_))));
    }

    #[tokio::test]
    async fn names_are_unique_by_key() {
        let (_dir, core) = core().await;
        core.create_user(NewUser::named("alice")).await.unwrap();
        assert!(matches!(core.create_user(NewUser::named("ALICE")).await, Err(Error::OwnerExists(_))));
    }

    #[tokio::test]
    async fn reserved_names_are_rejected() {
        let (_dir, core) = core().await;
        assert!(matches!(core.create_user(NewUser::named("api")).await, Err(Error::InvalidName(_))));
    }
}
