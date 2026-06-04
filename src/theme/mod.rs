use std::path::{Path, PathBuf};

use minijinja::{Environment, path_loader};
use minijinja_autoreload::AutoReloader;
use serde::Serialize;

use crate::presets;

/// 公開サイト用のランタイムテンプレートエンジン。
///
/// `work/layouts` をルートに `path_loader` を張り、テンプレートを
/// `{layout_key}/shell.html` や `{layout_key}/pages/...` でアドレスする。
pub struct Templates {
    reloader: AutoReloader,
}

impl Templates {
    pub fn new(layouts_dir: PathBuf) -> Self {
        let reloader = AutoReloader::new(move |notifier| {
            let mut env = Environment::new();
            notifier.watch_path(&layouts_dir, true);
            env.set_loader(path_loader(&layouts_dir));
            Ok(env)
        });
        Self { reloader }
    }

    /// `work/layouts/{name}` を描画する。
    pub fn render<S: Serialize>(&self, name: &str, ctx: S) -> Result<String, minijinja::Error> {
        let env = self.reloader.acquire_env()?;
        let template = env.get_template(name)?;
        template.render(ctx)
    }
}

/// `work/layouts` ディレクトリのパス。
pub fn layouts_dir(work_dir: &str) -> PathBuf {
    Path::new(work_dir).join("layouts")
}

/// 1 レイアウトのルート（`work/layouts/{key}/`）。
pub fn layout_dir(work_dir: &str, layout_key: &str) -> PathBuf {
    layouts_dir(work_dir).join(layout_key)
}

/// レイアウト配下の static ディレクトリ。
pub fn layout_static_dir(work_dir: &str, layout_key: &str) -> PathBuf {
    layout_dir(work_dir, layout_key).join("static")
}

/// `/static/{layout_key}/...` → `work/layouts/{layout_key}/static/...` を解決する。
pub fn resolve_static_path(work_dir: &str, path: &str) -> Option<PathBuf> {
    let (layout_key, file_path) = path.split_once('/')?;
    if layout_key.is_empty() || file_path.is_empty() {
        return None;
    }
    if !is_safe_static_relative_path(file_path) {
        return None;
    }

    let candidate = layout_static_dir(work_dir, layout_key).join(file_path);
    candidate.is_file().then_some(candidate)
}

/// 拡張子から Content-Type を推定する。
pub fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

fn is_safe_static_relative_path(path: &str) -> bool {
    !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Phase 1 の `work/templates` / `work/pages` を `work/layouts/default/` へベストエフォートで移す。
pub fn migrate_legacy_work_dir(work_dir: &str) -> std::io::Result<()> {
    let legacy_templates = Path::new(work_dir).join("templates");
    let legacy_pages = Path::new(work_dir).join("pages");
    let target_pages = layout_dir(work_dir, "default").join("pages");

    if legacy_templates.join("index.html").exists()
        && !target_pages.join("index.html").exists()
    {
        ensure_layout_dirs(work_dir, "default")?;
        std::fs::create_dir_all(&target_pages)?;
        for entry in std::fs::read_dir(&legacy_templates)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".html") {
                let dest = target_pages.join(name.as_ref());
                if !dest.exists() {
                    std::fs::copy(entry.path(), dest)?;
                }
            }
        }
        let legacy_static = legacy_templates.join("static");
        if legacy_static.is_dir() {
            let dest_static = layout_static_dir(work_dir, "default");
            std::fs::create_dir_all(&dest_static)?;
            for entry in std::fs::read_dir(legacy_static)? {
                let entry = entry?;
                let dest = dest_static.join(entry.file_name());
                if !dest.exists() {
                    std::fs::copy(entry.path(), dest)?;
                }
            }
        }
    }

    if legacy_pages.is_dir() {
        ensure_layout_dirs(work_dir, "default")?;
        std::fs::create_dir_all(&target_pages)?;
        for entry in std::fs::read_dir(legacy_pages)? {
            let entry = entry?;
            let dest = target_pages.join(entry.file_name());
            if !dest.exists() {
                std::fs::copy(entry.path(), dest)?;
            }
        }
    }

    Ok(())
}

/// 作業ディレクトリを初期化する。既定レイアウト `default` を seed する。
pub fn ensure_seeded(work_dir: &str) -> std::io::Result<()> {
    migrate_legacy_work_dir(work_dir)?;
    ensure_layout_dirs(work_dir, "default")?;

    let shell = layout_dir(work_dir, "default").join("shell.html");
    if !shell.exists() {
        std::fs::write(&shell, presets::DEFAULT_SHELL)?;
    }

    let index_page = layout_dir(work_dir, "default").join("pages/index.html");
    if !index_page.exists() {
        std::fs::create_dir_all(index_page.parent().unwrap())?;
        std::fs::write(&index_page, presets::DEFAULT_HOME_PAGE)?;
    }

    let site_css = layout_static_dir(work_dir, "default").join("site.css");
    if !site_css.exists() {
        std::fs::write(&site_css, presets::DEFAULT_SITE_CSS)?;
    }

    Ok(())
}

/// レイアウト用ディレクトリ（shell / pages / static）を作成する。
pub fn ensure_layout_dirs(work_dir: &str, layout_key: &str) -> std::io::Result<()> {
    let root = layout_dir(work_dir, layout_key);
    std::fs::create_dir_all(root.join("pages"))?;
    std::fs::create_dir_all(root.join("static"))?;
    Ok(())
}

/// shell.html を読み込む。
pub fn read_shell(work_dir: &str, layout_key: &str) -> std::io::Result<String> {
    std::fs::read_to_string(layout_dir(work_dir, layout_key).join("shell.html"))
}

/// shell.html を書き込む。
pub fn write_shell(work_dir: &str, layout_key: &str, content: &str) -> std::io::Result<()> {
    ensure_layout_dirs(work_dir, layout_key)?;
    std::fs::write(layout_dir(work_dir, layout_key).join("shell.html"), content)
}

/// ページ本文テンプレートを読み込む。
pub fn read_page_body(work_dir: &str, layout_key: &str, file_name: &str) -> std::io::Result<String> {
    std::fs::read_to_string(layout_dir(work_dir, layout_key).join(file_name))
}

/// ページ本文テンプレートを書き込む。
pub fn write_page_body(
    work_dir: &str,
    layout_key: &str,
    file_name: &str,
    content: &str,
) -> std::io::Result<()> {
    let path = layout_dir(work_dir, layout_key).join(file_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// ページ本文テンプレートを削除する。
pub fn remove_page_body(work_dir: &str, layout_key: &str, file_name: &str) -> std::io::Result<()> {
    match std::fs::remove_file(layout_dir(work_dir, layout_key).join(file_name)) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// レイアウトディレクトリを削除する。
pub fn remove_layout_dir(work_dir: &str, layout_key: &str) -> std::io::Result<()> {
    let path = layout_dir(work_dir, layout_key);
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// レイアウト key の変更に伴いディレクトリをリネームする。
pub fn rename_layout_dir(work_dir: &str, old_key: &str, new_key: &str) -> std::io::Result<()> {
    let from = layout_dir(work_dir, old_key);
    let to = layout_dir(work_dir, new_key);
    if from.exists() {
        std::fs::rename(from, to)?;
    } else {
        ensure_layout_dirs(work_dir, new_key)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_static_path_rejects_unsafe_paths() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work = tmp.path().to_str().expect("utf8 path");
        ensure_layout_dirs(work, "default").expect("dirs");
        assert!(resolve_static_path(work, "default/../shell.html").is_none());
        assert!(resolve_static_path(work, "default/site.css").is_none());
    }

    #[test]
    fn resolve_static_path_uses_layout_static_subdirectory() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work = tmp.path().to_str().expect("utf8 path");
        ensure_layout_dirs(work, "default").expect("dirs");
        std::fs::write(
            layout_static_dir(work, "default").join("site.css"),
            "body { margin: 0; }",
        )
        .expect("write css");

        let resolved = resolve_static_path(work, "default/site.css").expect("resolved");
        assert!(resolved.ends_with("default/static/site.css"));
    }
}
