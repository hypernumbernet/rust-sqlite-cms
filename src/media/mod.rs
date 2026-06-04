use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Utc;

use crate::error::{AppError, AppResult};

/// アップロード可能な最大ファイルサイズ（5 MiB）。
pub const MAX_FILE_SIZE: usize = 5 * 1024 * 1024;

const BLOCKED_EXTENSIONS: &[&str] = &[
    "exe", "sh", "bash", "php", "html", "htm", "js", "bat", "cmd", "com", "msi", "dll", "so",
    "dylib",
];

/// `uploads_dir` 設定値からアップロード先ディレクトリのパスを返す。
pub fn uploads_dir(uploads_dir: &str) -> PathBuf {
    Path::new(uploads_dir).to_path_buf()
}

/// アップロード先ディレクトリを作成する。
pub fn ensure_uploads_dir(path: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(uploads_dir(path))
}

/// アップロードファイルを検証し、年月サブディレクトリへ保存する。
/// 戻り値: (相対パス, MIME タイプ)
pub fn save_upload(
    uploads_root: &str,
    original_name: &str,
    data: &[u8],
) -> AppResult<(String, String)> {
    validate_upload(original_name, data)?;

    let ext = extension_from_name(original_name)?;
    let mime_type = mime_from_extension(&ext)
        .ok_or_else(|| AppError::Conflict("許可されていないファイル形式です".to_string()))?
        .to_string();

    let now = Utc::now();
    let relative_dir = format!("{}/{}", now.format("%Y"), now.format("%m"));
    let stored_name = format!("{}.{}", unique_name(), ext);
    let relative_path = format!("{relative_dir}/{stored_name}");

    let dest_dir = uploads_dir(uploads_root).join(&relative_dir);
    std::fs::create_dir_all(&dest_dir)?;
    std::fs::write(dest_dir.join(&stored_name), data)?;

    Ok((relative_path, mime_type))
}

/// 相対パスで指定されたファイルを削除する。`uploads_root` 外へのパスは拒否する。
pub fn delete_file(uploads_root: &str, relative_path: &str) -> AppResult<()> {
    let root = uploads_dir(uploads_root);
    let target = root.join(relative_path);

    let canonical_root = root.canonicalize().unwrap_or(root.clone());
    let canonical_target = target
        .canonicalize()
        .map_err(|_| AppError::NotFound)?;

    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::Conflict(
            "不正なファイルパスです".to_string(),
        ));
    }

    match std::fs::remove_file(&canonical_target) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// バイト数を人間が読みやすい文字列に変換する。
pub fn format_file_size(size: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;

    if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}

/// MIME タイプが画像かどうか。
pub fn is_image_mime(mime_type: &str) -> bool {
    mime_type.starts_with("image/")
}

fn validate_upload(original_name: &str, data: &[u8]) -> AppResult<()> {
    if data.is_empty() {
        return Err(AppError::Conflict("ファイルが空です".to_string()));
    }
    if data.len() > MAX_FILE_SIZE {
        return Err(AppError::Conflict(format!(
            "ファイルサイズは {} 以下にしてください",
            format_file_size(MAX_FILE_SIZE as i64)
        )));
    }

    let ext = extension_from_name(original_name)?;
    if BLOCKED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AppError::Conflict(
            "この拡張子のファイルはアップロードできません".to_string(),
        ));
    }
    if mime_from_extension(&ext).is_none() {
        return Err(AppError::Conflict(
            "許可されていないファイル形式です".to_string(),
        ));
    }

    Ok(())
}

fn extension_from_name(name: &str) -> AppResult<String> {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name);
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .ok_or_else(|| AppError::Conflict("拡張子のないファイルはアップロードできません".to_string()))?;
    Ok(ext)
}

fn mime_from_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "pdf" => Some("application/pdf"),
        _ => None,
    }
}

fn unique_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}")
}
