//! The shape of Klotho's config file. Loading it (file + `KLOTHO_…` environment
//! overrides) happens in the binary; this module only defines the keys, their
//! defaults and how paths in them resolve.
//!
//! Every key here must also be documented in `klotho.example.toml` (NFR-OPS-013).
//! Unknown keys are rejected so a typo can't silently fall back to a default
//! (NFR-OPS-011).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub log: LogConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to listen on.
    pub addr: SocketAddr,
    /// How long shutdown waits for running requests and git processes (NFR-OPS-030).
    pub shutdown_grace_secs: u64,
    /// Timeout for every request except git transport, which can run for as long as
    /// a clone or push takes.
    pub request_timeout_secs: u64,
    /// Largest request body `/api` accepts, in bytes (NFR-SEC-020).
    pub api_body_limit: usize,
}

impl ServerConfig {
    pub fn shutdown_grace(&self) -> Duration {
        Duration::from_secs(self.shutdown_grace_secs)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            shutdown_grace_secs: 30,
            request_timeout_secs: 30,
            api_body_limit: 1024 * 1024,
        }
    }
}

/// Where Klotho keeps its data (FR-STOR-012). Everything lives under `data_dir`
/// unless a location is overridden on its own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
    /// Bare git repositories. Default: `<data_dir>/repositories`.
    pub repositories: Option<PathBuf>,
    /// The SQLite database. Default: `<data_dir>/klotho.db`.
    pub database: Option<PathBuf>,
    /// The local file store (ADR 0003). Default: `<data_dir>/files`.
    pub files: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { data_dir: PathBuf::from("data"), repositories: None, database: None, files: None }
    }
}

impl StorageConfig {
    /// Resolves every location to an absolute path. Relative paths resolve against
    /// `base`, which is the config file's folder, so starting the server from
    /// another directory still finds the same data.
    pub fn resolve(&self, base: &Path) -> DataPaths {
        let data_dir = base.join(&self.data_dir);
        let pick = |set: &Option<PathBuf>, default: &str| match set {
            Some(path) => base.join(path),
            None => data_dir.join(default),
        };
        DataPaths {
            repositories: pick(&self.repositories, "repositories"),
            database: pick(&self.database, "klotho.db"),
            files: pick(&self.files, "files"),
            data_dir,
        }
    }
}

/// The resolved, absolute locations from [`StorageConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    pub data_dir: PathBuf,
    pub repositories: PathBuf,
    pub database: PathBuf,
    pub files: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// A `tracing` filter such as `klotho=debug`. `RUST_LOG` takes precedence.
    pub filter: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self { filter: "klotho=info,tower_http=info".to_owned() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        if cfg!(windows) { PathBuf::from(r"C:\srv\klotho") } else { PathBuf::from("/srv/klotho") }
    }

    #[test]
    fn everything_defaults_to_under_data_dir() {
        let paths = StorageConfig::default().resolve(&base());
        assert_eq!(paths.data_dir, base().join("data"));
        assert_eq!(paths.repositories, base().join("data").join("repositories"));
        assert_eq!(paths.database, base().join("data").join("klotho.db"));
        assert_eq!(paths.files, base().join("data").join("files"));
    }

    #[test]
    fn overrides_resolve_against_the_base_not_data_dir() {
        let config = StorageConfig { repositories: Some(PathBuf::from("git")), ..StorageConfig::default() };
        assert_eq!(config.resolve(&base()).repositories, base().join("git"));
    }

    #[test]
    fn absolute_paths_are_kept() {
        let elsewhere = base().join("elsewhere");
        let config = StorageConfig { data_dir: elsewhere.clone(), ..StorageConfig::default() };
        assert_eq!(config.resolve(Path::new("ignored")).data_dir, elsewhere);
    }
}
