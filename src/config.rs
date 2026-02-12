use crate::error::{Result, TlError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub log_path: String,
    pub date_format: String,
    pub note_indent: usize,
    pub scan_window_lines: usize,
}

impl Config {
    pub fn with_log_path(log_path: &str) -> Self {
        Self {
            log_path: log_path.to_string(),
            date_format: "DD/MM/YYYY".to_string(),
            note_indent: 6,
            scan_window_lines: 5000,
        }
    }

    pub fn base_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(".config")
            .join("tl")
    }

    pub fn config_path() -> PathBuf {
        Self::base_dir().join("config.toml")
    }

    /// Resolve the log path from config. Expands ~ to home dir.
    pub fn resolved_log_path(&self) -> PathBuf {
        expand_tilde(&self.log_path)
    }

    pub fn state_path() -> PathBuf {
        Self::base_dir().join("state.json")
    }

    pub fn lock_path() -> PathBuf {
        Self::base_dir().join("lock")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Err(TlError::NotInitialized);
        }
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content).map_err(|e| TlError::Config(e.to_string()))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self).map_err(|e| TlError::Config(e.to_string()))?;
        atomic_write(&path, content.as_bytes())
    }

    pub fn ensure_dir() -> Result<()> {
        let dir = Self::base_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_path: "~/.config/tl/log.md".to_string(),
            date_format: "DD/MM/YYYY".to_string(),
            note_indent: 6,
            scan_window_lines: 5000,
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;

    let dir = path.parent().ok_or_else(|| {
        TlError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no parent directory",
        ))
    })?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| TlError::Io(e.error))?;
    Ok(())
}
