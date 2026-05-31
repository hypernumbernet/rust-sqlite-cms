use std::path::{Path, PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

const CONFIG_EXAMPLE: &str = "config.example.toml";
const LEGACY_CONFIG: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub uploads_dir: String,
    /// テンプレート・静的アセットを置くステートフルな作業ディレクトリ。
    pub work_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub title: String,
    pub tagline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub cookie_name: String,
    pub max_age_secs: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// 本番では環境変数 `CMS_SESSION_SECRET` で必ず上書きする。
    #[serde(default)]
    pub session_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub paths: PathsConfig,
    pub site: SiteConfig,
    pub session: SessionConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_addr: "127.0.0.1:3000".to_string(),
            },
            database: DatabaseConfig {
                path: "data/cms.db".to_string(),
            },
            paths: PathsConfig {
                uploads_dir: "work/uploads".to_string(),
                work_dir: "work".to_string(),
            },
            site: SiteConfig {
                title: "My Site".to_string(),
                tagline: "Just another rust-sqlite-cms site".to_string(),
            },
            session: SessionConfig {
                cookie_name: "cms_session".to_string(),
                max_age_secs: 604_800,
            },
            security: SecurityConfig::default(),
        }
    }
}

/// 作業ディレクトリ配下の設定ファイルパス（`paths.work_dir` とは独立）。
pub fn config_path() -> PathBuf {
    PathBuf::from("work/config.toml")
}

impl AppConfig {
    /// `work/config.toml` が無ければ用意する（例: `config.example.toml` からコピー）。
    pub fn ensure_default_file() -> std::io::Result<()> {
        let path = config_path();
        if path.exists() {
            return Ok(());
        }

        std::fs::create_dir_all("work")?;

        let source = if Path::new(LEGACY_CONFIG).exists() {
            tracing::info!(
                legacy = LEGACY_CONFIG,
                target = %path.display(),
                "ルートの config.toml を work/config.toml へ移行します"
            );
            LEGACY_CONFIG
        } else {
            CONFIG_EXAMPLE
        };

        std::fs::copy(source, &path)?;
        Ok(())
    }

    /// 設定を `work/config.toml`（存在すれば）と環境変数（`CMS_` プレフィックス）から読み込む。
    /// いずれも無ければ [`AppConfig::default`] の値で動作する。
    pub fn load() -> Result<Self, figment::Error> {
        Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file(config_path()))
            .merge(Env::prefixed("CMS_").map(|key| {
                match key.as_str().to_ascii_uppercase().as_str() {
                    "BIND_ADDR" => "server.bind_addr".into(),
                    "DATABASE_PATH" => "database.path".into(),
                    "SESSION_SECRET" => "security.session_secret".into(),
                    _ => key.as_str().replace("__", ".").to_ascii_lowercase().into(),
                }
            }))
            .extract()
    }

    /// ファイル内容のみを読み込む（環境変数はマージしない）。
    pub fn load_from_file() -> Result<Self, figment::Error> {
        Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file(config_path()))
            .extract()
    }

    /// `[site]` の `title` / `tagline` のみ更新して書き戻す。
    pub fn save_site_section(title: &str, tagline: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut config = Self::load_from_file()?;
        config.site.title = title.to_string();
        config.site.tagline = tagline.to_string();
        let contents = toml::to_string_pretty(&config)?;
        std::fs::write(config_path(), contents)?;
        Ok(())
    }
}
