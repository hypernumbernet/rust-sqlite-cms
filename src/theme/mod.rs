use std::path::{Path, PathBuf};

use minijinja::{Environment, path_loader};
use minijinja_autoreload::AutoReloader;
use serde::Serialize;

use crate::presets;

/// 公開サイト用のランタイムテンプレートエンジン。
///
/// `work/templates` をルートに `path_loader` を張り、テンプレートを
/// ファイル名（`index.html` や `page-3.html`）でアドレスする。
/// `AutoReloader` が配下のファイル変更を監視するため、テンプレートを
/// 編集すると再起動なしで反映される。
pub struct Templates {
    reloader: AutoReloader,
}

impl Templates {
    pub fn new(templates_dir: PathBuf) -> Self {
        let reloader = AutoReloader::new(move |notifier| {
            let mut env = Environment::new();
            notifier.watch_path(&templates_dir, true);
            env.set_loader(path_loader(&templates_dir));
            Ok(env)
        });
        Self { reloader }
    }

    /// `work/templates/{name}` を描画する。`.html` は MiniJinja の
    /// 既定で自動 HTML エスケープが有効になる。
    pub fn render<S: Serialize>(&self, name: &str, ctx: S) -> Result<String, minijinja::Error> {
        let env = self.reloader.acquire_env()?;
        let template = env.get_template(name)?;
        template.render(ctx)
    }
}

/// `work/templates` ディレクトリのパス。
pub fn templates_dir(work_dir: &str) -> PathBuf {
    Path::new(work_dir).join("templates")
}

/// `work/templates/static` ディレクトリのパス。
pub fn static_dir(work_dir: &str) -> PathBuf {
    templates_dir(work_dir).join("static")
}

/// 作業ディレクトリを初期化する。`work/templates/` と `work/templates/static/`
/// を用意し、`index.html` が無ければ同梱プリセットから生成する。
pub fn ensure_seeded(work_dir: &str) -> std::io::Result<()> {
    let templates = templates_dir(work_dir);
    std::fs::create_dir_all(&templates)?;
    std::fs::create_dir_all(static_dir(work_dir))?;

    let index = templates.join("index.html");
    if !index.exists() {
        std::fs::write(&index, presets::HOME_INDEX)?;
    }

    Ok(())
}

/// 編集フォーム向けにテンプレートファイルの生ソースを読み込む。
pub fn read_source(work_dir: &str, file_name: &str) -> std::io::Result<String> {
    std::fs::read_to_string(templates_dir(work_dir).join(file_name))
}

/// テンプレートファイルへ生ソースを書き込む。
pub fn write_source(work_dir: &str, file_name: &str, content: &str) -> std::io::Result<()> {
    std::fs::write(templates_dir(work_dir).join(file_name), content)
}

/// テンプレートファイルを削除する。既に無い場合は無視する。
pub fn remove_source(work_dir: &str, file_name: &str) -> std::io::Result<()> {
    match std::fs::remove_file(templates_dir(work_dir).join(file_name)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}
