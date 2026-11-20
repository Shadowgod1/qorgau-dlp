use std::path::{Path, PathBuf};

use qorgau_common::validate_agent_id;

use crate::error::{AppError, AppResult};

/// Safely builds a storage file path inside `base_dir/<agent_id>/<safe_filename>`
/// and verifies that the resolved path cannot escape `base_dir` (eliminates Path Traversal).
pub async fn prepare_safe_storage_path(
    base_dir: &Path,
    agent_id: &str,
    filename: &str,
) -> AppResult<PathBuf> {
    validate_agent_id(agent_id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let safe_name = sanitize_filename(filename);
    if safe_name.is_empty() {
        return Err(AppError::BadRequest("Invalid filename".to_string()));
    }

    tokio::fs::create_dir_all(base_dir).await?;
    let base_canon = tokio::fs::canonicalize(base_dir).await?;

    let agent_dir = base_canon.join(agent_id);
    tokio::fs::create_dir_all(&agent_dir).await?;
    let agent_canon = tokio::fs::canonicalize(&agent_dir).await?;

    if !agent_canon.starts_with(&base_canon) {
        return Err(AppError::Forbidden("Path traversal denied".to_string()));
    }

    Ok(agent_canon.join(safe_name))
}

/// Validates that an existing stored path is strictly inside `base_dir` before serving/deleting.
pub async fn verify_path_within(base_dir: &Path, stored_path: &str) -> AppResult<PathBuf> {
    let base_canon = tokio::fs::canonicalize(base_dir)
        .await
        .map_err(|_| AppError::NotFound("Storage directory missing".to_string()))?;
    let target_canon = tokio::fs::canonicalize(Path::new(stored_path))
        .await
        .map_err(|_| AppError::NotFound("File not found on disk".to_string()))?;

    if !target_canon.starts_with(&base_canon) {
        return Err(AppError::Forbidden("Access denied".to_string()));
    }
    Ok(target_canon)
}

pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.bin");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    if cleaned == "." || cleaned == ".." || cleaned.is_empty() {
        "file.bin".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blocks_traversal_in_agent_id_and_filename() {
        let tmp = std::env::temp_dir().join(format!("qorgau_test_{}", uuid::Uuid::new_v4()));
        assert!(prepare_safe_storage_path(&tmp, "../etc", "a.jpg")
            .await
            .is_err());
        assert!(prepare_safe_storage_path(&tmp, "C:\\Windows", "a.jpg")
            .await
            .is_err());
        let ok = prepare_safe_storage_path(&tmp, "agent_12345678", "../../evil.jpg")
            .await
            .unwrap();
        assert!(ok.ends_with("evil.jpg"));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
