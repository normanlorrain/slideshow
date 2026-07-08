//! Configuration: merge CLI flags with TOML, validate albums.
//!
//! Behaviour mirrors Python `config.py`, with one intentional extension:
//! a global `weight` key is accepted (as documented in README / example.toml)
//! and used as the default album weight. Python currently rejects it because
//! `weight` is missing from its global `defaults` dict.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

/// Album ordering modes (TOML / config values are lowercase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Order {
    Sequence,
    Atomic,
    Random,
}

impl Default for Order {
    fn default() -> Self {
        Self::Sequence
    }
}

impl Order {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sequence => "sequence",
            Self::Atomic => "atomic",
            Self::Random => "random",
        }
    }
}

/// Raw TOML shape (global section + albums array).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlConfig {
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    fullscreen: Option<bool>,
    #[serde(default)]
    shuffle: Option<bool>,
    #[serde(default)]
    interval: Option<u64>,
    /// Global default album weight (README / docs/example.toml).
    #[serde(default)]
    weight: Option<u32>,
    #[serde(default)]
    albums: Vec<TomlAlbum>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlAlbum {
    folder: String,
    #[serde(default)]
    order: Option<Order>,
    #[serde(default)]
    weight: Option<u32>,
    #[serde(default)]
    interval: Option<u64>,
}

/// CLI overrides that win over TOML when present.
///
/// Falsy CLI flags must not clobber TOML (matches Click/Python merge):
/// - `fullscreen` / `shuffle` only override when `true`
/// - `exclude` only when non-empty
/// - `interval` / `dry_run` only when `Some`
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub config_file: Option<PathBuf>,
    pub directory: Option<PathBuf>,
    pub fullscreen: bool,
    pub shuffle: bool,
    pub interval: Option<u64>,
    pub exclude: Vec<String>,
    pub dry_run: Option<usize>,
}

/// Fully resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub config_file: Option<PathBuf>,
    pub directory: Option<PathBuf>,
    pub fullscreen: bool,
    pub shuffle: bool,
    pub interval: u64,
    pub exclude: Vec<String>,
    pub dry_run: Option<usize>,
    /// Default weight applied when an album omits `weight`.
    pub weight: u32,
    pub albums: Vec<AlbumConfig>,
}

#[derive(Debug, Clone)]
pub struct AlbumConfig {
    pub folder: PathBuf,
    pub order: Order,
    pub weight: u32,
    pub interval: u64,
}

impl Config {
    /// Build configuration from CLI overrides (and optional TOML / directory).
    pub fn from_cli(cli: &CliOverrides) -> Result<Self> {
        if cli.config_file.is_none() && cli.directory.is_none() {
            return Err(Error::Config(
                "Must specify a DIRECTORY or a config file.".into(),
            ));
        }
        if cli.config_file.is_some() && cli.directory.is_some() {
            return Err(Error::Config(
                "Must specify a DIRECTORY or a config file.".into(),
            ));
        }

        if let Some(ref dir) = cli.directory {
            return Self::from_directory(cli, dir);
        }

        let config_file = cli
            .config_file
            .as_ref()
            .expect("config_file checked above");
        Self::from_config_file(cli, config_file)
    }

    fn from_directory(cli: &CliOverrides, directory: &Path) -> Result<Self> {
        let directory = directory
            .canonicalize()
            .map_err(|_| Error::InvalidPath(directory.to_path_buf()))?;
        if !directory.is_dir() {
            return Err(Error::InvalidPath(directory));
        }

        let order = if cli.shuffle {
            Order::Random
        } else {
            Order::Sequence
        };
        let interval = cli.interval.unwrap_or(5);
        let weight = 1;

        Ok(Config {
            config_file: None,
            directory: Some(directory.clone()),
            fullscreen: cli.fullscreen,
            shuffle: cli.shuffle,
            interval,
            exclude: cli.exclude.clone(),
            dry_run: cli.dry_run,
            weight,
            albums: vec![AlbumConfig {
                folder: directory,
                order,
                weight,
                interval,
            }],
        })
    }

    fn from_config_file(cli: &CliOverrides, config_file: &Path) -> Result<Self> {
        let config_file = config_file
            .canonicalize()
            .map_err(|_| Error::InvalidPath(config_file.to_path_buf()))?;
        let raw = load_toml(&config_file)?;

        // Merge: CLI true/non-empty wins; otherwise TOML; otherwise defaults.
        let fullscreen = if cli.fullscreen {
            true
        } else {
            raw.fullscreen.unwrap_or(false)
        };
        let shuffle = if cli.shuffle {
            true
        } else {
            raw.shuffle.unwrap_or(false)
        };
        let interval = cli.interval.or(raw.interval).unwrap_or(5);
        let exclude = if !cli.exclude.is_empty() {
            cli.exclude.clone()
        } else {
            raw.exclude
        };
        let weight = raw.weight.unwrap_or(1);

        let parent = config_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut albums = Vec::new();
        for album in raw.albums {
            match resolve_album(&album, &parent, interval, weight) {
                Ok(a) => albums.push(a),
                Err(e) => {
                    // Python logs and continues on per-album errors.
                    tracing::error!("{e}");
                }
            }
        }

        Ok(Config {
            config_file: Some(config_file),
            directory: None,
            fullscreen,
            shuffle,
            interval,
            exclude,
            dry_run: cli.dry_run,
            weight,
            albums,
        })
    }
}

fn load_toml(path: &Path) -> Result<TomlConfig> {
    let text = fs::read_to_string(path)?;
    // Strip full-line `#` comments is not needed; toml crate handles them.
    let cfg: TomlConfig = toml::from_str(&text).map_err(|e| {
        // Unknown fields / bad types → configuration error (deny_unknown_fields).
        Error::Config(format!("Configuration file error: {}: {e}", path.display()))
    })?;
    Ok(cfg)
}

fn resolve_album(
    album: &TomlAlbum,
    config_parent: &Path,
    global_interval: u64,
    global_weight: u32,
) -> Result<AlbumConfig> {
    let mut path = PathBuf::from(&album.folder);
    if !path.is_absolute() {
        path = config_parent.join(&path);
    }
    // Prefer canonical path when it exists; otherwise keep joined path for error.
    if path.exists() {
        path = path.canonicalize().unwrap_or(path);
    } else {
        return Err(Error::Config(format!(
            "Configuration error. Invalid path: {}",
            path.display()
        )));
    }

    Ok(AlbumConfig {
        folder: path,
        order: album.order.unwrap_or(Order::Sequence),
        weight: album.weight.unwrap_or(global_weight),
        interval: album.interval.unwrap_or(global_interval),
    })
}

/// Load and parse a TOML config file, rejecting unknown top-level keys.
/// Used by tests for bad.toml without full CLI merge.
pub fn parse_toml_file(path: &Path) -> Result<Config> {
    let cli = CliOverrides {
        config_file: Some(path.to_path_buf()),
        ..CliOverrides::default()
    };
    Config::from_cli(&cli)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn rejects_bad_toml_unknown_keys() {
        let path = repo_root().join("tests/bad.toml");
        let err = parse_toml_file(&path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Configuration") || msg.contains("unknown"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn loads_example_1_toml() {
        let path = repo_root().join("tests/example 1.toml");
        let cfg = parse_toml_file(&path).unwrap();
        assert!(!cfg.fullscreen);
        assert!(!cfg.shuffle);
        assert_eq!(cfg.interval, 5); // default
        assert_eq!(cfg.albums.len(), 4);
        assert_eq!(cfg.albums[0].order, Order::Sequence);
        assert_eq!(cfg.albums[1].order, Order::Atomic);
        assert_eq!(cfg.albums[2].order, Order::Random);
        assert_eq!(cfg.albums[0].weight, 1);
        // folders resolve under tests/
        assert!(cfg.albums[0].folder.ends_with("images/numbers"));
        assert!(cfg.albums[3].folder.ends_with("pdfs"));
    }

    #[test]
    fn loads_docs_example_toml_with_global_weight() {
        // docs/example.toml uses paths relative to docs/ that may not exist
        // on disk; we only assert globals + that missing albums are skipped.
        let path = repo_root().join("docs/example.toml");
        let cfg = parse_toml_file(&path).unwrap();
        assert!(cfg.fullscreen);
        assert_eq!(cfg.interval, 3);
        assert_eq!(cfg.weight, 1);
        assert_eq!(
            cfg.exclude,
            vec![
                "_archive".to_string(),
                "archive".to_string(),
                "old".to_string(),
                "_old".to_string()
            ]
        );
        // folders point at docs/images/* which are absent → albums skipped
        assert!(cfg.albums.is_empty());
    }

    #[test]
    fn cli_overrides_toml() {
        let path = repo_root().join("tests/example 1.toml");
        let cli = CliOverrides {
            config_file: Some(path),
            fullscreen: true, // TOML says false
            shuffle: true,
            interval: Some(9),
            exclude: vec!["_skip".into()],
            dry_run: Some(3),
            directory: None,
        };
        let cfg = Config::from_cli(&cli).unwrap();
        assert!(cfg.fullscreen);
        assert!(cfg.shuffle);
        assert_eq!(cfg.interval, 9);
        assert_eq!(cfg.exclude, vec!["_skip".to_string()]);
        assert_eq!(cfg.dry_run, Some(3));
        // album intervals inherit global after merge
        assert!(cfg.albums.iter().all(|a| a.interval == 9));
    }

    #[test]
    fn falsy_cli_flags_do_not_clobber_toml() {
        // docs/example.toml has fullscreen=true; a false CLI flag must not clobber it.
        let path = repo_root().join("docs/example.toml");
        let cli = CliOverrides {
            config_file: Some(path),
            fullscreen: false, // must NOT force false over TOML true
            shuffle: false,
            interval: None,
            exclude: vec![],
            dry_run: None,
            directory: None,
        };
        let cfg = Config::from_cli(&cli).unwrap();
        assert!(cfg.fullscreen, "false CLI flag must not clobber TOML true");
        assert_eq!(cfg.interval, 3);
    }

    #[test]
    fn single_directory_mode() {
        let dir = repo_root().join("tests/images/numbers");
        let cli = CliOverrides {
            directory: Some(dir.clone()),
            shuffle: true,
            interval: Some(2),
            dry_run: Some(5),
            ..CliOverrides::default()
        };
        let cfg = Config::from_cli(&cli).unwrap();
        assert_eq!(cfg.albums.len(), 1);
        assert_eq!(cfg.albums[0].order, Order::Random);
        assert_eq!(cfg.albums[0].interval, 2);
        assert!(cfg.albums[0].folder.ends_with("numbers"));
    }

    #[test]
    fn requires_config_or_directory() {
        let err = Config::from_cli(&CliOverrides::default()).unwrap_err();
        assert!(err.to_string().contains("DIRECTORY") || err.to_string().contains("config"));
    }

    #[test]
    fn mutually_exclusive_config_and_directory() {
        let cli = CliOverrides {
            config_file: Some(repo_root().join("tests/example 1.toml")),
            directory: Some(repo_root().join("tests/images/numbers")),
            ..CliOverrides::default()
        };
        let err = Config::from_cli(&cli).unwrap_err();
        assert!(err.to_string().contains("DIRECTORY") || err.to_string().contains("config"));
    }
}
