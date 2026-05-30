/// `placeholders` テーブルの行に対応する。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Placeholder {
    pub id: i64,
    pub name: String,
    pub widget_type_id: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// プレースホルダー作成・更新時にリポジトリへ渡す入力値。
#[derive(Debug, Clone)]
pub struct PlaceholderInput {
    pub name: String,
    pub widget_type_id: i64,
}

const RESERVED_NAMES: &[&str] = &["blogname", "blogdescription"];

/// プレースホルダー名の形式と予約語を検証する。
pub fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("プレースホルダー名は必須です".to_string());
    }
    let Some(first) = name.chars().next() else {
        return Err("プレースホルダー名は必須です".to_string());
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err("英字またはアンダースコアで始めてください".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err("英数字とアンダースコアのみ使用できます".to_string());
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(format!("「{name}」は予約語のため使用できません"));
    }
    if name.starts_with("has_") {
        return Err("has_ で始まる名前は使用できません".to_string());
    }
    Ok(())
}
