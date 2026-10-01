use std::fmt;
use std::str::FromStr;

use crate::Error;

/// A validated repository name, safe to use as a directory name.
///
/// Allows ASCII letters, digits, `-`, `_` and `.`, must not start with `.` or `-`,
/// and has any trailing `.git` stripped so `demo` and `demo.git` are the same repo.
///
/// Names are lowercased, so `Demo` and `demo` are the same repo on every platform.
/// Without this, case-insensitive filesystems (Windows, macOS) would treat them as
/// one repo and Linux as two.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoName(String);

impl RepoName {
    const MAX_LEN: usize = 100;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RepoName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        let lower = s.to_ascii_lowercase();
        let name = lower.strip_suffix(".git").unwrap_or(&lower);
        let valid = !name.is_empty()
            && name.len() <= Self::MAX_LEN
            && !name.starts_with(['.', '-'])
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        if valid { Ok(Self(name.to_owned())) } else { Err(Error::InvalidName(s.to_owned())) }
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names_and_strips_git_suffix() {
        assert_eq!("demo".parse::<RepoName>().unwrap().as_str(), "demo");
        assert_eq!("demo.git".parse::<RepoName>().unwrap().as_str(), "demo");
        assert_eq!("my_repo-2.0".parse::<RepoName>().unwrap().as_str(), "my_repo-2.0");
    }

    #[test]
    fn lowercases_names() {
        assert_eq!("Demo".parse::<RepoName>().unwrap().as_str(), "demo");
        assert_eq!("DEMO.GIT".parse::<RepoName>().unwrap().as_str(), "demo");
        assert_eq!("Demo".parse::<RepoName>().unwrap(), "demo".parse().unwrap());
    }

    #[test]
    fn rejects_unsafe_names() {
        for bad in ["", ".git", "..", ".hidden", "-flag", "a/b", "a\\b", "ü", "a b"] {
            assert!(bad.parse::<RepoName>().is_err(), "{bad:?} should be rejected");
        }
    }
}
