use anyhow::Context;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const RETAINED_BACKUP_MANIFESTS: usize = 10;
const BACKUP_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct BackupStore {
    root: PathBuf,
}

impl BackupStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn write_backup(
        &self,
        session_id: &str,
        source_db: &Path,
        tables: serde_json::Value,
    ) -> anyhow::Result<String> {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let token = format!("{epoch}-{}", Uuid::new_v4().simple());
        fs::create_dir_all(&self.root).with_context(|| {
            format!(
                "failed to create backup directory {}",
                self.root.to_string_lossy()
            )
        })?;
        let payload = json!({
            "token": token,
            "session_id": session_id,
            "source_db": source_db.to_string_lossy(),
            "tables": tables,
        });
        swivox_core::settings::atomic_write(
            &self.path_for(&token),
            serde_json::to_string_pretty(&payload)?.as_bytes(),
        )?;
        self.prune_expired_backups(epoch);
        Ok(token)
    }

    /// 至少保留最新 10 份，且 7 天内批量删除产生的备份全部保留（#2544）。
    fn prune_expired_backups(&self, now_epoch: u64) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let mut manifests: Vec<(u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return None;
                }
                let stem = path.file_stem()?.to_str()?;
                let (epoch_str, hex_part) = stem.split_once('-')?;
                if hex_part.len() != 32 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
                    return None;
                }
                let epoch = epoch_str.parse::<u64>().ok()?;
                Some((epoch, path))
            })
            .collect();
        manifests.sort_by(|a, b| b.0.cmp(&a.0));
        for (epoch, path) in manifests.into_iter().skip(RETAINED_BACKUP_MANIFESTS) {
            if now_epoch.saturating_sub(epoch) >= BACKUP_RETENTION_SECS {
                let _ = fs::remove_file(&path);
            }
        }
    }

    pub fn read_backup(&self, token: &str) -> anyhow::Result<serde_json::Value> {
        let path = self.path_for(token);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("Backup token not found: {token}"))?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn path_for(&self, token: &str) -> PathBuf {
        let safe: String = token
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .collect();
        self.root.join(format!("{safe}.json"))
    }
}
