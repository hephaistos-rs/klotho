use std::fmt;
use std::process::Command;

/// A `git` program version, as printed by `git --version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    /// The oldest git Klotho supports (NFR-OPS-003). 2.39 is the floor the roadmap
    /// plans around, e.g. `git merge-tree --write-tree` for pull requests.
    pub const MIN: Self = Self { major: 2, minor: 39, patch: 0 };

    /// Parses `git --version` output such as `git version 2.55.0.windows.5` or
    /// `git version 2.39.5 (Apple Git-154)`.
    pub fn parse(output: &str) -> Option<Self> {
        let rest = output.trim().strip_prefix("git version ")?;
        let mut parts = rest.split(['.', ' ', '-']).map(leading_number);
        Some(Self {
            major: parts.next()??,
            minor: parts.next()??,
            patch: parts.next().flatten().unwrap_or(0),
        })
    }
}

fn leading_number(part: &str) -> Option<u32> {
    let end = part.find(|c: char| !c.is_ascii_digit()).unwrap_or(part.len());
    part[..end].parse().ok()
}

impl fmt::Display for GitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GitCheckError {
    #[error("could not run `git --version` ({0}). Klotho needs git {min} or newer on PATH", min = GitVersion::MIN)]
    NotRunnable(std::io::Error),
    #[error("could not read the git version from {0:?}")]
    Unrecognised(String),
    #[error("git {found} is too old. Klotho needs git {min} or newer", min = GitVersion::MIN)]
    TooOld { found: GitVersion },
}

/// Runs `git --version` and checks it against [`GitVersion::MIN`], so a missing or
/// outdated git fails at startup instead of on the first clone.
pub fn check_git() -> Result<GitVersion, GitCheckError> {
    let output = Command::new("git").arg("--version").output().map_err(GitCheckError::NotRunnable)?;
    let text = String::from_utf8_lossy(&output.stdout);
    let found =
        GitVersion::parse(&text).ok_or_else(|| GitCheckError::Unrecognised(text.trim().to_owned()))?;
    if found < GitVersion::MIN {
        return Err(GitCheckError::TooOld { found });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> GitVersion {
        GitVersion { major, minor, patch }
    }

    #[test]
    fn parses_platform_variants() {
        assert_eq!(GitVersion::parse("git version 2.55.0.windows.5\n"), Some(v(2, 55, 0)));
        assert_eq!(GitVersion::parse("git version 2.39.5 (Apple Git-154)"), Some(v(2, 39, 5)));
        assert_eq!(GitVersion::parse("git version 2.45.0-rc1"), Some(v(2, 45, 0)));
        assert_eq!(GitVersion::parse("git version 2.40"), Some(v(2, 40, 0)));
    }

    #[test]
    fn rejects_other_output() {
        assert_eq!(GitVersion::parse("hub version 2.14.2"), None);
        assert_eq!(GitVersion::parse(""), None);
    }

    #[test]
    fn compares_numerically() {
        assert!(v(2, 38, 9) < GitVersion::MIN);
        assert!(v(2, 100, 0) > GitVersion::MIN);
    }

    #[test]
    fn installed_git_is_supported() {
        // The test suite runs real git elsewhere too, so this doubles as a clear
        // message when the machine's git is too old.
        check_git().unwrap();
    }
}
