//! Repository and owner names (docs/requirements/naming.md).
//!
//! A name keeps the display form its owner typed (`MyRepo`, FR-NAME-020). Its
//! key, the display form with ASCII letters lowercased (`myrepo`), is what
//! uniqueness and lookup use (FR-NAME-021, 022), so equality and hashing go by
//! the key.
//!
//! Each name has two parsers. `parse_new` is for names entering the system
//! (create, rename, import) and applies every rule. `parse_lookup` is for URL
//! segments: it strips one trailing `.git` (FR-NAME-041) and checks syntax only,
//! since a name that breaks a reserved-name rule simply won't be found.

use std::fmt;
use std::hash::{Hash, Hasher};

/// Why a name was rejected, worded for the person who typed it (FR-NAME-007).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid {what} name {name:?}: {reason}")]
pub struct InvalidName {
    pub what: &'static str,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct RepoName {
    display: String,
    key: String,
}

impl RepoName {
    pub const MAX_LEN: usize = 100;
    /// Keys ending in these are reserved for wikis and feeds, and `.git` would be
    /// stripped again on lookup (FR-NAME-030, 031).
    const RESERVED_SUFFIXES: [&str; 4] = [".git", ".wiki", ".atom", ".rss"];

    /// For a name entering the system: create, rename, import, adopt.
    pub fn parse_new(name: &str) -> Result<Self, InvalidName> {
        let invalid = |reason: String| InvalidName { what: "repository", name: name.to_owned(), reason };
        check_syntax(name, Self::MAX_LEN).map_err(invalid)?;
        let key = name.to_ascii_lowercase();
        if let Some(suffix) = Self::RESERVED_SUFFIXES.iter().find(|s| key.ends_with(*s)) {
            return Err(invalid(format!("names can't end in {suffix}")));
        }
        Ok(Self { display: name.to_owned(), key })
    }

    /// For a repository segment in a URL, with or without `.git` (FR-NAME-040).
    pub fn parse_lookup(segment: &str) -> Result<Self, InvalidName> {
        let name = strip_git_suffix(segment);
        check_syntax(name, Self::MAX_LEN).map_err(|reason| InvalidName {
            what: "repository",
            name: segment.to_owned(),
            reason,
        })?;
        Ok(Self { display: name.to_owned(), key: name.to_ascii_lowercase() })
    }

    pub fn as_str(&self) -> &str {
        &self.display
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Debug, Clone)]
pub struct OwnerName {
    display: String,
    key: String,
}

impl OwnerName {
    pub const MAX_LEN: usize = 39;
    /// Keys that are, or will be, top-level routes, so an owner can't hide them
    /// (FR-NAME-032). `_topcoat` is Topcoat's runtime and asset prefix.
    const RESERVED: [&str; 12] = [
        "-",
        ".well-known",
        "_topcoat",
        "admin",
        "api",
        "assets",
        "explore",
        "favicon.ico",
        "login",
        "logout",
        "robots.txt",
        "sitemap.xml",
    ];
    /// Reserved for routes like `/<user>.keys` (FR-NAME-033).
    const RESERVED_SUFFIXES: [&str; 5] = [".keys", ".gpg", ".rss", ".atom", ".png"];

    /// For a user or organisation name entering the system.
    pub fn parse_new(name: &str) -> Result<Self, InvalidName> {
        let invalid = |reason: String| InvalidName { what: "owner", name: name.to_owned(), reason };
        check_syntax(name, Self::MAX_LEN).map_err(invalid)?;
        if name.starts_with('.') {
            return Err(invalid("names can't start with '.'".to_owned()));
        }
        let key = name.to_ascii_lowercase();
        if Self::RESERVED.contains(&key.as_str()) {
            return Err(invalid("this name is reserved".to_owned()));
        }
        if let Some(suffix) = Self::RESERVED_SUFFIXES.iter().find(|s| key.ends_with(*s)) {
            return Err(invalid(format!("names can't end in {suffix}")));
        }
        Ok(Self { display: name.to_owned(), key })
    }

    /// For an owner segment in a URL.
    pub fn parse_lookup(segment: &str) -> Result<Self, InvalidName> {
        check_syntax(segment, Self::MAX_LEN).map_err(|reason| InvalidName {
            what: "owner",
            name: segment.to_owned(),
            reason,
        })?;
        Ok(Self { display: segment.to_owned(), key: segment.to_ascii_lowercase() })
    }

    pub fn as_str(&self) -> &str {
        &self.display
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

macro_rules! by_key {
    ($name:ident) => {
        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                self.key == other.key
            }
        }
        impl Eq for $name {}
        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.key.hash(state);
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.display)
            }
        }
    };
}
by_key!(RepoName);
by_key!(OwnerName);

/// Strips at most one trailing `.git`, compared ASCII case-insensitively (FR-NAME-041).
fn strip_git_suffix(segment: &str) -> &str {
    let len = segment.len();
    if len > 4 && segment.is_char_boundary(len - 4) && segment[len - 4..].eq_ignore_ascii_case(".git") {
        &segment[..len - 4]
    } else {
        segment
    }
}

/// The rules shared by every name (FR-NAME-001 to 005).
fn check_syntax(name: &str, max_len: usize) -> Result<(), String> {
    if name.is_empty() {
        return Err("names can't be empty".to_owned());
    }
    if let Some(c) = name.chars().find(|&c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))) {
        return Err(format!("{c:?} isn't allowed; use ASCII letters, digits, '-', '_' and '.'"));
    }
    if name.len() > max_len {
        return Err(format!("names can be at most {max_len} characters long"));
    }
    if name.starts_with('-') {
        return Err("names can't start with '-'".to_owned());
    }
    if name.contains("..") {
        return Err("names can't contain '..'".to_owned());
    }
    if name == "." {
        return Err("names can't be '.'".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_repo(name: &str) -> Result<RepoName, InvalidName> {
        RepoName::parse_new(name)
    }

    #[test]
    fn display_form_is_kept_and_key_is_lowercased() {
        let name = new_repo("MyRepo").unwrap();
        assert_eq!(name.as_str(), "MyRepo");
        assert_eq!(name.key(), "myrepo");
        assert_eq!(name.to_string(), "MyRepo");
        assert_eq!(name, new_repo("myrepo").unwrap());
    }

    #[test]
    fn accepts_valid_repository_names() {
        for good in ["a", "my_repo-2.0", ".github", ".profile", &"x".repeat(100)] {
            assert!(new_repo(good).is_ok(), "{good:?}");
        }
    }

    #[test]
    fn rejects_invalid_repository_names() {
        for bad in ["", ".", "..", "a..b", "-flag", "a/b", "a\\b", "ü", "a b", &"x".repeat(101)] {
            assert!(new_repo(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rejects_reserved_suffixes_on_new_names_only() {
        for bad in ["demo.git", "Demo.GIT", "demo.wiki", "demo.atom", "demo.rss"] {
            assert!(new_repo(bad).is_err(), "{bad:?}");
        }
        assert_eq!(RepoName::parse_lookup("demo.git").unwrap().key(), "demo");
    }

    #[test]
    fn lookup_strips_exactly_one_git_suffix() {
        assert_eq!(RepoName::parse_lookup("Demo.GIT").unwrap().as_str(), "Demo");
        assert_eq!(RepoName::parse_lookup("demo").unwrap().key(), "demo");
        // `foo.git.git` looks up `foo.git`, which can never be created.
        assert_eq!(RepoName::parse_lookup("foo.git.git").unwrap().key(), "foo.git");
        // `.git` alone isn't stripped to nothing; it's looked up as itself and,
        // since no new name can end in `.git`, never found.
        assert_eq!(RepoName::parse_lookup(".git").unwrap().key(), ".git");
        assert!(new_repo(".git").is_err());
    }

    #[test]
    fn error_names_the_offending_character() {
        let err = new_repo("a b").unwrap_err();
        assert!(err.to_string().contains("' '"), "{err}");
    }

    #[test]
    fn owner_names() {
        assert_eq!(OwnerName::parse_new("Alice").unwrap().key(), "alice");
        assert!(OwnerName::parse_new(&"a".repeat(39)).is_ok());
        for bad in [&"a".repeat(40), ".hidden", "api", "API", "_topcoat", "-", "bob.keys", "x.png", "a b"] {
            assert!(OwnerName::parse_new(bad).is_err(), "{bad:?}");
        }
    }
}
