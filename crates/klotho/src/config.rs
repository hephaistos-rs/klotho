//! Loads the config file plus `KLOTHO_…` environment overrides (NFR-OPS-010).
//!
//! Environment variables use `__` between sections and keys, so `server.addr` is
//! `KLOTHO_SERVER__ADDR` and `storage.data_dir` is `KLOTHO_STORAGE__DATA_DIR`.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use figment::Figment;
use figment::providers::{Env, Format, Toml};
use klotho_core::config::Config;

pub struct Loaded {
    pub config: Config,
    /// The config file, if one was given.
    pub file: Option<PathBuf>,
    /// The folder relative paths resolve against: the config file's folder, or the
    /// current directory when there is no config file.
    pub base_dir: PathBuf,
}

pub fn load(file: Option<&Path>) -> anyhow::Result<Loaded> {
    let mut figment = Figment::new();
    let (file, base_dir) = match file {
        Some(file) => {
            let file = std::path::absolute(file)?;
            if !file.is_file() {
                bail!("config file {} not found", file.display());
            }
            figment = figment.merge(Toml::file_exact(&file));
            let base_dir = file.parent().expect("an absolute file path has a parent").to_owned();
            (Some(file), base_dir)
        }
        None => (None, std::env::current_dir()?),
    };
    // KLOTHO_CONFIG names the file itself (see main.rs), it isn't a setting.
    figment = figment.merge(Env::prefixed("KLOTHO_").split("__").ignore(&["config"]));
    Ok(Loaded { config: extract(figment)?, file, base_dir })
}

fn extract(figment: Figment) -> anyhow::Result<Config> {
    figment.extract().context("invalid configuration")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_toml(toml: &str) -> anyhow::Result<Config> {
        extract(Figment::from(Toml::string(toml)))
    }

    /// The error with its causes, as `main` prints it.
    fn message(err: anyhow::Error) -> String {
        format!("{err:#}")
    }

    #[test]
    fn empty_file_gives_defaults() {
        let config = from_toml("").unwrap();
        assert_eq!(config.server.addr.port(), 3000);
        assert_eq!(config.storage.data_dir, PathBuf::from("data"));
    }

    #[test]
    fn typo_in_a_key_is_rejected_and_named() {
        let err = message(from_toml("[server]\nadr = \"127.0.0.1:3000\"").unwrap_err());
        assert!(err.contains("adr"), "{err}");
    }

    #[test]
    fn unknown_section_is_rejected_and_named() {
        let err = message(from_toml("[strage]\ndata_dir = \"x\"").unwrap_err());
        assert!(err.contains("strage"), "{err}");
    }

    #[test]
    fn invalid_value_names_the_key() {
        let err = message(from_toml("[server]\naddr = \"not an address\"").unwrap_err());
        assert!(err.contains("addr"), "{err}");
    }

    #[test]
    fn relative_paths_resolve_against_the_config_file_not_the_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("klotho.toml");
        std::fs::write(&file, "[storage]\ndata_dir = \"mydata\"").unwrap();

        let loaded = load(Some(&file)).unwrap();
        let paths = loaded.config.storage.resolve(&loaded.base_dir);
        assert_eq!(paths.data_dir, std::path::absolute(dir.path()).unwrap().join("mydata"));
    }

    #[test]
    fn missing_config_file_is_an_error() {
        let err = message(load(Some(Path::new("does/not/exist.toml"))).err().unwrap());
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn example_config_is_valid_and_complete() {
        // klotho.example.toml documents every key with its default (NFR-OPS-013),
        // so it must parse, and parsing it must give exactly the defaults.
        let example = include_str!("../../../klotho.example.toml");
        let config = from_toml(example).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn dev_config_is_valid() {
        from_toml(include_str!("../../../klotho.dev.toml")).unwrap();
    }
}
