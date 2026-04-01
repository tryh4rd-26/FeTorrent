use serde::{Deserialize, Serialize};
use crate::error::Result;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeConfig {
    pub server: ServerConfig,
    pub downloads: DownloadsConfig,
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub port: u16,
    pub bind: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadsConfig {
    pub directory: String,
    pub max_peers: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LimitsConfig {
    pub download_kbps: u32,
    pub upload_kbps: u32,
}

impl Default for FeConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                port: 6977,
                bind: "127.0.0.1".to_string(),
            },
            downloads: DownloadsConfig {
                directory: "~/Downloads/FeTorrent".to_string(),
                max_peers: 200,
            },
            limits: LimitsConfig {
                download_kbps: 0,
                upload_kbps: 0,
            },
        }
    }
}

impl FeConfig {
    pub fn save(&self) -> Result<()> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("fetorrent").join("config.toml");
            let _ = std::fs::create_dir_all(config_dir.join("fetorrent"));
            let content = toml::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            std::fs::write(config_path, content)?;
        }
        Ok(())
    }

    pub fn load_or_default() -> Self {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("fetorrent").join("config.toml");
            if config_path.exists() {
                if let Ok(content) = std::fs::read_to_string(config_path) {
                    if let Ok(config) = toml::from_str(&content) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn resolve_log_path() -> std::path::PathBuf {
        let base = dirs::data_local_dir()
            .or_else(|| dirs::data_dir())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let log_dir = base.join("fetorrent");
        let _ = std::fs::create_dir_all(&log_dir);
        log_dir.join("daemon.log")
    }

    pub fn resolve_ui_dir(override_path: Option<std::path::PathBuf>) -> std::path::PathBuf {
        if let Some(p) = override_path {
            if p.exists() {
                return p;
            }
        }

        // 1. Check ./ui/dist (running from cargo workspace root)
        if std::path::Path::new("./ui/dist/index.html").exists() {
            return std::path::PathBuf::from("./ui/dist");
        }
        // 2. Check ./ui (running from dist output folder)
        if std::path::Path::new("./ui/index.html").exists() {
            return std::path::PathBuf::from("./ui");
        }
        // 3. Look in user's home directory local share
        if let Some(mut home) = dirs::data_local_dir() {
            home.push("fetorrent");
            home.push("ui");
            if home.join("index.html").exists() {
                return home;
            }
        }
        // Default fallback
        std::path::PathBuf::from("./ui")
    }
}
