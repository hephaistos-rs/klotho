//! Who may do what (docs/requirements/access-control.md). Every interface
//! (web, API, git over HTTP, later SSH) asks [`authorize`], so the answer is
//! the same everywhere (FR-ACL-050).

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::Serialize;

use crate::auth::User;

/// What a token may be used for (auth-flows.md, "Personal access tokens"). A
/// token never grants more than its owner has (FR-ACL-030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    RepoRead,
    RepoWrite,
    RepoAdmin,
    UserRead,
    UserWrite,
    /// Instance administration, for administrators' tokens.
    Admin,
}

impl Scope {
    pub const ALL: [Scope; 6] =
        [Self::RepoRead, Self::RepoWrite, Self::RepoAdmin, Self::UserRead, Self::UserWrite, Self::Admin];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RepoRead => "repo:read",
            Self::RepoWrite => "repo:write",
            Self::RepoAdmin => "repo:admin",
            Self::UserRead => "user:read",
            Self::UserWrite => "user:write",
            Self::Admin => "admin",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::RepoRead => "Clone and read repositories",
            Self::RepoWrite => "Push to repositories",
            Self::RepoAdmin => "Create repositories and change their settings",
            Self::UserRead => "Read your profile and tokens",
            Self::UserWrite => "Change your profile and revoke tokens",
            Self::Admin => "Administer the instance (administrators only)",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|scope| scope.as_str() == s).ok_or_else(|| format!("unknown scope {s:?}"))
    }
}

impl Serialize for Scope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// A set of scopes, stored space-separated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Scopes(BTreeSet<Scope>);

impl Scopes {
    pub fn new(scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self(scopes.into_iter().collect())
    }

    pub fn contains(&self, scope: Scope) -> bool {
        self.0.contains(&scope)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Scope> + '_ {
        self.0.iter().copied()
    }
}

impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.0.iter().map(|scope| scope.as_str()).collect();
        f.write_str(&names.join(" "))
    }
}

impl FromStr for Scopes {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.split_ascii_whitespace().map(str::parse).collect::<Result<BTreeSet<_>, _>>().map(Self)
    }
}

/// Who is making a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    Anonymous,
    /// Signed in to the web UI: everything the user may do.
    Session(User),
    /// A personal access token: what both the token and its user allow.
    Token(User, Scopes),
}

impl Actor {
    pub fn user(&self) -> Option<&User> {
        match self {
            Self::Anonymous => None,
            Self::Session(user) | Self::Token(user, _) => Some(user),
        }
    }

    /// Whether the credential allows `scope`. Sessions allow everything.
    pub fn has_scope(&self, scope: Scope) -> bool {
        match self {
            Self::Anonymous => false,
            Self::Session(_) => true,
            Self::Token(_, scopes) => scopes.contains(scope),
        }
    }

    /// An administrator whose credential allows administration.
    pub fn is_admin(&self) -> bool {
        self.user().is_some_and(|user| user.is_admin) && self.has_scope(Scope::Admin)
    }
}

/// Something to do with a repository, in increasing order of power (FR-ACL-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    /// Clone, fetch, browse.
    Read,
    /// Push.
    Write,
    /// Settings, visibility, delete.
    Admin,
}

impl Action {
    fn scope(self) -> Scope {
        match self {
            Self::Read => Scope::RepoRead,
            Self::Write => Scope::RepoWrite,
            Self::Admin => Scope::RepoAdmin,
        }
    }
}

/// The facts about a repository that permissions depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoFacts {
    pub owner_id: i64,
    pub private: bool,
}

/// The highest action `user` may take on the repository, regardless of how
/// they authenticated (FR-ACL-003, 004, 007, 010). `None` means not even read.
fn user_level(user: Option<&User>, repo: RepoFacts) -> Option<Action> {
    match user {
        Some(user) if user.suspended => None,
        Some(user) if user.is_admin || user.id == repo.owner_id => Some(Action::Admin),
        // Collaborators and teams add levels here (Phase 6, 7).
        _ if !repo.private => Some(Action::Read),
        _ => None,
    }
}

/// Whether `actor` may do `action` on the repository. The single permission
/// decision for every interface.
///
/// A token's scopes cap what its user may do (FR-ACL-030), except that a
/// public repository can always be read: anyone can read it without a token.
pub fn authorize(actor: &Actor, repo: RepoFacts, action: Action) -> bool {
    if action == Action::Read && !repo.private && !actor.user().is_some_and(|user| user.suspended) {
        return true;
    }
    let Some(level) = user_level(actor.user(), repo) else { return false };
    level >= action && actor.has_scope(action.scope())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: i64 = 1;
    const OTHER: i64 = 2;
    const ADMIN: i64 = 3;

    fn user(id: i64) -> User {
        User { id, username: format!("user{id}"), is_admin: id == ADMIN, suspended: false }
    }

    fn token(id: i64, scopes: &[Scope]) -> Actor {
        Actor::Token(user(id), Scopes::new(scopes.iter().copied()))
    }

    #[test]
    fn the_permission_table() {
        use Action::{Admin, Read, Write};
        use Scope::{RepoRead, RepoWrite, UserRead};
        let public = RepoFacts { owner_id: OWNER, private: false };
        let private = RepoFacts { owner_id: OWNER, private: true };
        let suspended_owner = Actor::Session(User { suspended: true, ..user(OWNER) });

        #[rustfmt::skip]
        let cases: Vec<(&str, Actor, RepoFacts, Action, bool)> = vec![
            ("anonymous reads public",            Actor::Anonymous,                 public,  Read,  true),
            ("anonymous can't push to public",    Actor::Anonymous,                 public,  Write, false),
            ("anonymous can't see private",       Actor::Anonymous,                 private, Read,  false),
            ("owner session does anything",       Actor::Session(user(OWNER)),      private, Admin, true),
            ("other user reads public",           Actor::Session(user(OTHER)),      public,  Read,  true),
            ("other user can't push",             Actor::Session(user(OTHER)),      public,  Write, false),
            ("other user can't see private",      Actor::Session(user(OTHER)),      private, Read,  false),
            ("instance admin does anything",      Actor::Session(user(ADMIN)),      private, Admin, true),
            ("token with repo:read clones",       token(OWNER, &[RepoRead]),        private, Read,  true),
            ("token with repo:read can't push",   token(OWNER, &[RepoRead]),        private, Write, false),
            ("token with repo:write pushes",      token(OWNER, &[RepoWrite]),       private, Write, true),
            ("repo:write isn't repo:admin",       token(OWNER, &[RepoWrite]),       private, Admin, false),
            ("scopes don't add permissions",      token(OTHER, &[RepoWrite]),       public,  Write, false),
            ("scopes don't reveal private repos", token(OTHER, &[RepoRead]),        private, Read,  false),
            ("public needs no repo scope to read",token(OTHER, &[UserRead]),        public,  Read,  true),
            ("private needs repo:read",           token(OWNER, &[UserRead]),        private, Read,  false),
            ("admin token is capped by scopes",   token(ADMIN, &[RepoRead]),        private, Write, false),
            ("suspended owner can do nothing",    suspended_owner.clone(),          private, Read,  false),
            ("suspended users can't read public", suspended_owner,                  public,  Read,  false),
        ];
        for (name, actor, repo, action, expected) in cases {
            assert_eq!(authorize(&actor, repo, action), expected, "{name}");
        }
    }

    #[test]
    fn instance_administration_needs_the_flag_and_the_scope() {
        assert!(Actor::Session(user(ADMIN)).is_admin());
        assert!(token(ADMIN, &[Scope::Admin]).is_admin());
        assert!(!token(ADMIN, &[Scope::RepoAdmin]).is_admin());
        assert!(!token(OWNER, &[Scope::Admin]).is_admin());
        assert!(!Actor::Anonymous.is_admin());
    }

    #[test]
    fn scopes_round_trip_through_text() {
        let scopes: Scopes = "repo:write repo:read".parse().unwrap();
        assert_eq!(scopes.to_string(), "repo:read repo:write");
        assert!("repo:read nope".parse::<Scopes>().is_err());
    }
}
