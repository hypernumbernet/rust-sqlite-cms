use std::path::PathBuf;

use minijinja::{Environment, path_loader};
use minijinja_autoreload::AutoReloader;
use serde::Serialize;

/// 公開サイト用のランタイムテンプレートエンジン。
///
/// `themes_dir` をルートに `path_loader` を張り、テンプレートを
/// `{theme}/templates/{name}` の形でアドレスする。これによりテーマ切替は
/// プレフィックスの変更だけで完結し、環境を作り直す必要がない。
/// `AutoReloader` が `themes_dir` 配下のファイル変更を監視するため、
/// テンプレートを編集すると再起動なしで反映される。
pub struct Templates {
    reloader: AutoReloader,
}

impl Templates {
    pub fn new(themes_dir: PathBuf) -> Self {
        let reloader = AutoReloader::new(move |notifier| {
            let mut env = Environment::new();
            notifier.watch_path(&themes_dir, true);
            env.set_loader(path_loader(&themes_dir));
            Ok(env)
        });
        Self { reloader }
    }

    /// `{theme}/templates/{name}` を描画する。`.html` は MiniJinja の
    /// 既定で自動 HTML エスケープが有効になる。
    pub fn render<S: Serialize>(
        &self,
        theme: &str,
        name: &str,
        ctx: S,
    ) -> Result<String, minijinja::Error> {
        let env = self.reloader.acquire_env()?;
        let template = env.get_template(&format!("{theme}/templates/{name}"))?;
        template.render(ctx)
    }

    /// DB などに保存された任意のソース文字列を描画する。
    /// `name` の拡張子で自動エスケープが決まるため、HTML には `.html` を渡す。
    pub fn render_str<S: Serialize>(
        &self,
        name: &str,
        source: &str,
        ctx: S,
    ) -> Result<String, minijinja::Error> {
        let env = self.reloader.acquire_env()?;
        env.render_named_str(name, source, ctx)
    }
}
