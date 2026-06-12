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

/// static ファイル 1 件あたりのアップロード上限（10MB）。
pub const MAX_STATIC_UPLOAD_SIZE: usize = 10 * 1024 * 1024;

/// `static/` 配下ファイルの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticFileKind {
    Text,
    Binary,
}

/// `static/` 配下のファイル一覧エントリ。
#[derive(Debug, Clone)]
pub struct StaticFileEntry {
    pub relative_path: String,
    pub kind: StaticFileKind,
    pub size_bytes: u64,
    pub public_url: String,
}

impl StaticFileEntry {
    /// 管理画面向けの種別ラベル。
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            StaticFileKind::Text => "テキスト",
            StaticFileKind::Binary => binary_kind_label(&self.relative_path),
        }
    }
}

fn binary_kind_label(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "svg") => "画像",
        Some("woff" | "woff2") => "フォント",
        _ => "バイナリ",
    }
}

/// shell.html のバイト数。存在しなければ 0。
pub fn shell_file_size_bytes(work_dir: &str, layout_key: &str) -> u64 {
    let path = layout_dir(work_dir, layout_key).join("shell.html");
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn is_safe_static_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.split('/').any(|part| part.starts_with('_'))
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn extension_matches(path: &str, allowed: &[&str]) -> bool {
    let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) else {
        return false;
    };
    allowed.iter().any(|allowed_ext| ext.eq_ignore_ascii_case(allowed_ext))
}

/// タブエディタで編集可能なテキスト static か。
pub fn is_editable_text_static(path: &str) -> bool {
    extension_matches(path, &["css", "js", "svg", "json", "txt", "map"])
}

/// アップロード可能な static ファイルか（テキスト + バイナリ）。
pub fn is_allowed_static_upload(path: &str) -> bool {
    is_editable_text_static(path)
        || extension_matches(
            path,
            &[
                "png", "jpg", "jpeg", "gif", "webp", "ico", "woff", "woff2",
            ],
        )
}

fn classify_static_kind(path: &str) -> StaticFileKind {
    if is_editable_text_static(path) {
        StaticFileKind::Text
    } else {
        StaticFileKind::Binary
    }
}

/// `static/` からの相対パスを検証する。
pub fn validate_static_relative_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("ファイルパスは必須です".to_string());
    }
    if !is_safe_static_relative_path(trimmed) {
        return Err("ファイルパスが不正です".to_string());
    }
    if !is_allowed_static_upload(trimmed) {
        return Err("この拡張子のファイルは許可されていません".to_string());
    }
    Ok(())
}

fn static_absolute_path(work_dir: &str, layout_key: &str, relative_path: &str) -> PathBuf {
    layout_static_dir(work_dir, layout_key).join(relative_path)
}

/// `static/` 配下のファイルを再帰的に一覧する。
pub fn list_static_files(work_dir: &str, layout_key: &str) -> std::io::Result<Vec<StaticFileEntry>> {
    let root = layout_static_dir(work_dir, layout_key);
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    collect_static_files(&root, &root, layout_key, &mut entries)?;
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(entries)
}

fn collect_static_files(
    root: &Path,
    dir: &Path,
    layout_key: &str,
    out: &mut Vec<StaticFileEntry>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_static_files(root, &path, layout_key, out)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("static file under root")
                .to_string_lossy()
                .replace('\\', "/");
            if !is_safe_static_relative_path(&relative) {
                continue;
            }
            let metadata = entry.metadata()?;
            out.push(StaticFileEntry {
                relative_path: relative.clone(),
                kind: classify_static_kind(&relative),
                size_bytes: metadata.len(),
                public_url: format!("/static/{layout_key}/{relative}"),
            });
        }
    }
    Ok(())
}

/// テキスト static を読み込む。
pub fn read_static_text(
    work_dir: &str,
    layout_key: &str,
    relative_path: &str,
) -> std::io::Result<String> {
    std::fs::read_to_string(static_absolute_path(
        work_dir,
        layout_key,
        relative_path,
    ))
}

/// テキスト static を書き込む。
pub fn write_static_text(
    work_dir: &str,
    layout_key: &str,
    relative_path: &str,
    content: &str,
) -> std::io::Result<()> {
    let path = static_absolute_path(work_dir, layout_key, relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// バイナリ static を書き込む。
pub fn write_static_bytes(
    work_dir: &str,
    layout_key: &str,
    relative_path: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    let path = static_absolute_path(work_dir, layout_key, relative_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

/// static ファイルを削除する。
pub fn remove_static_file(
    work_dir: &str,
    layout_key: &str,
    relative_path: &str,
) -> std::io::Result<()> {
    match std::fs::remove_file(static_absolute_path(
        work_dir,
        layout_key,
        relative_path,
    )) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// 人間向けのファイルサイズ表示。
pub fn format_static_file_size(size_bytes: u64) -> String {
    if size_bytes < 1024 {
        format!("{size_bytes} B")
    } else if size_bytes < 1024 * 1024 {
        format!("{:.1} KB", size_bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size_bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Phase 1 の `work/templates` / `work/pages` を `work/layouts/example/` へベストエフォートで移す。
pub fn migrate_legacy_work_dir(work_dir: &str) -> std::io::Result<()> {
    let legacy_templates = Path::new(work_dir).join("templates");
    let legacy_pages = Path::new(work_dir).join("pages");
    let target_pages = layout_dir(work_dir, "example").join("pages");

    if legacy_templates.join("index.html").exists()
        && !target_pages.join("index.html").exists()
    {
        ensure_layout_dirs(work_dir, "example")?;
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
            let dest_static = layout_static_dir(work_dir, "example");
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
        ensure_layout_dirs(work_dir, "example")?;
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

/// 作業ディレクトリを初期化する。例示レイアウト `example` を seed する。
pub fn ensure_seeded(work_dir: &str) -> std::io::Result<()> {
    migrate_legacy_work_dir(work_dir)?;
    ensure_layout_dirs(work_dir, "example")?;

    let shell = layout_dir(work_dir, "example").join("shell.html");
    if !shell.exists() {
        std::fs::write(&shell, presets::DEFAULT_SHELL)?;
    }

    let index_page = layout_dir(work_dir, "example").join("pages/index.html");
    if !index_page.exists() {
        std::fs::create_dir_all(index_page.parent().unwrap())?;
        std::fs::write(&index_page, presets::DEFAULT_HOME_PAGE)?;
    }

    let site_css = layout_static_dir(work_dir, "example").join("site.css");
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
        ensure_layout_dirs(work, "example").expect("dirs");
        assert!(resolve_static_path(work, "example/../shell.html").is_none());
        assert!(resolve_static_path(work, "example/site.css").is_none());
    }

    #[test]
    fn resolve_static_path_uses_layout_static_subdirectory() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work = tmp.path().to_str().expect("utf8 path");
        ensure_layout_dirs(work, "example").expect("dirs");
        std::fs::write(
            layout_static_dir(work, "example").join("site.css"),
            "body { margin: 0; }",
        )
        .expect("write css");

        let resolved = resolve_static_path(work, "example/site.css").expect("resolved");
        assert!(resolved.ends_with("example/static/site.css"));
    }

    #[test]
    fn validate_static_relative_path_rejects_unsafe_paths() {
        assert!(validate_static_relative_path("").is_err());
        assert!(validate_static_relative_path("../site.css").is_err());
        assert!(validate_static_relative_path("_hidden.css").is_err());
        assert!(validate_static_relative_path("img/../site.css").is_err());
    }

    #[test]
    fn static_file_entry_kind_label() {
        let text = StaticFileEntry {
            relative_path: "site.css".to_string(),
            kind: StaticFileKind::Text,
            size_bytes: 10,
            public_url: "/static/example/site.css".to_string(),
        };
        assert_eq!(text.kind_label(), "テキスト");

        let image = StaticFileEntry {
            relative_path: "logo.png".to_string(),
            kind: StaticFileKind::Binary,
            size_bytes: 10,
            public_url: "/static/example/logo.png".to_string(),
        };
        assert_eq!(image.kind_label(), "画像");
    }

    #[test]
    fn is_editable_text_static_and_upload_allowed() {
        assert!(is_editable_text_static("site.css"));
        assert!(is_editable_text_static("js/app.js"));
        assert!(!is_editable_text_static("logo.png"));
        assert!(is_allowed_static_upload("logo.png"));
        assert!(!is_allowed_static_upload("evil.exe"));
    }

    #[test]
    fn list_and_read_write_static_files() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let work = tmp.path().to_str().expect("utf8 path");
        ensure_layout_dirs(work, "corp").expect("dirs");

        write_static_text(work, "corp", "site.css", "body {}").expect("write");
        write_static_bytes(work, "corp", "img/logo.png", &[0x89, 0x50]).expect("bytes");

        let files = list_static_files(work, "corp").expect("list");
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.relative_path == "site.css" && f.kind == StaticFileKind::Text));
        assert!(files.iter().any(|f| f.relative_path == "img/logo.png" && f.kind == StaticFileKind::Binary));

        let css = read_static_text(work, "corp", "site.css").expect("read");
        assert_eq!(css, "body {}");

        remove_static_file(work, "corp", "site.css").expect("remove");
        let after = list_static_files(work, "corp").expect("list after");
        assert_eq!(after.len(), 1);
    }
}
