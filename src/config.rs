use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

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
                uploads_dir: "uploads".to_string(),
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

impl AppConfig {
    /// 設定を `config.toml`（存在すれば）と環境変数（`CMS_` プレフィックス）から読み込む。
    /// いずれも無ければ [`AppConfig::default`] の値で動作する。
    pub fn load() -> Result<Self, figment::Error> {
        Figment::from(Serialized::defaults(AppConfig::default()))
            .merge(Toml::file("config.toml"))
            .merge(Env::prefixed("CMS_").map(|key| {
                // README で文書化したフラットなエイリアスをネストキーへ写像する。
                match key.as_str().to_ascii_uppercase().as_str() {
                    "BIND_ADDR" => "server.bind_addr".into(),
                    "DATABASE_PATH" => "database.path".into(),
                    "SESSION_SECRET" => "security.session_secret".into(),
                    // それ以外は `CMS_SERVER__BIND_ADDR` のように `__` をネスト区切りとして扱う。
                    _ => key.as_str().replace("__", ".").to_ascii_lowercase().into(),
                }
            }))
            .extract()
    }
}
