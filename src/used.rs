use anyhow::{Context, Result};
use std::{collections::BTreeSet, io::Write, path::PathBuf};

pub struct Used {
    ids: BTreeSet<String>,
    path: PathBuf,
}
impl Used {
    pub fn open() -> Result<Self> {
        Self::load(
            dirs::config_dir()
                .context("找不到配置目录")?
                .join("WallpaperDesktop/used.json"),
        )
    }
    pub(crate) fn load(path: PathBuf) -> Result<Self> {
        let ids = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("使用记录损坏，未覆盖原文件")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeSet::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Self { ids, path })
    }
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
    pub fn record(&mut self, id: String) -> Result<()> {
        // Keep the ID in memory even if persistence fails, so this session still skips it.
        self.ids.insert(id);
        let parent = self.path.parent().context("无效的记录目录")?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec(&self.ids)?)?;
        temp.as_file().sync_all()?;
        temp.persist(&self.path)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_survive_restart_and_are_unique() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("used.json");
        let mut used = Used::load(path.clone()).unwrap();
        assert!(!used.contains("a"));
        used.record("a".into()).unwrap();
        used.record("a".into()).unwrap();
        let restored = Used::load(path).unwrap();
        assert!(restored.contains("a"));
        assert_eq!(restored.ids.len(), 1);
    }
    #[test]
    fn corrupt_records_are_not_silently_reset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("used.json");
        std::fs::write(&path, b"invalid").unwrap();
        assert!(Used::load(path.clone()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"invalid");
    }
}
