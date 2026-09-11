use crate::service::Category;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub category: Category,
    pub categories: Vec<Category>,
    pub initialized: bool,
    pub rotation_enabled: bool,
    pub interval_minutes: u32,
    pub custom_interval: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            category: Category::Landscape,
            categories: Vec::new(),
            initialized: false,
            rotation_enabled: false,
            interval_minutes: 5,
            custom_interval: false,
        }
    }
}
impl Settings {
    pub fn selected_categories(&self) -> Vec<Category> {
        let mut selected = Vec::new();
        for category in &self.categories {
            if !selected.contains(category) {
                selected.push(*category);
            }
        }
        if selected.is_empty() {
            selected.push(self.category);
        }
        selected
    }
    pub fn toggle_category(&mut self, category: Category) -> bool {
        let mut selected = self.selected_categories();
        if selected.contains(&category) {
            if selected.len() == 1 {
                return false;
            }
            selected.retain(|c| *c != category);
        } else {
            selected.push(category);
        }
        self.category = selected[0];
        self.categories = selected;
        true
    }

    fn path() -> Result<std::path::PathBuf> {
        Ok(dirs::config_dir()
            .context("找不到配置目录")?
            .join("WallpaperDesktop/settings.json"))
    }
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }
    fn save_to(&self, path: &std::path::Path) -> Result<()> {
        let parent = path.parent().context("无效的配置目录")?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(())
    }
}
pub fn launcher() -> Result<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe()?;
    Ok(auto_launch::AutoLaunchBuilder::new()
        .set_app_name("WallpaperDesktop")
        .set_app_path(exe.to_str().context("程序路径无法编码")?)
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .set_args(&["--minimized"])
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_categories_survive_disk_roundtrip() {
        let mut settings = Settings::default();
        assert!(settings.toggle_category(Category::Beauty));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        settings.save_to(&path).unwrap();
        let restored: Settings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            restored.selected_categories(),
            vec![Category::Landscape, Category::Beauty]
        );
    }
    #[test]
    fn last_category_cannot_be_deselected() {
        let mut settings = Settings::default();
        assert!(!settings.toggle_category(Category::Landscape));
        assert!(settings.toggle_category(Category::Beauty));
        assert!(settings.toggle_category(Category::Landscape));
        assert_eq!(settings.selected_categories(), vec![Category::Beauty]);
        assert!(!settings.toggle_category(Category::Beauty));
    }
    #[test]
    fn repeated_saves_replace_existing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.save_to(&path).unwrap();
        settings.category = Category::Beauty;
        settings.rotation_enabled = true;
        settings.interval_minutes = 37;
        settings.custom_interval = true;
        settings.save_to(&path).unwrap();
        let restored: Settings = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(restored.category, Category::Beauty);
        assert!(restored.rotation_enabled && restored.custom_interval);
        assert_eq!(restored.interval_minutes, 37);
    }
    #[test]
    fn old_settings_keep_working() {
        let s: Settings =
            serde_json::from_str(r#"{"category":"Beauty","initialized":true}"#).unwrap();
        assert_eq!(s.category, Category::Beauty);
        assert_eq!(s.selected_categories(), vec![Category::Beauty]);
        assert_eq!(s.interval_minutes, 5);
        assert!(!s.rotation_enabled);
    }
    #[test]
    fn custom_interval_survives_restart() {
        let s = Settings {
            rotation_enabled: true,
            interval_minutes: 37,
            custom_interval: true,
            ..Default::default()
        };
        let restored: Settings = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(restored.rotation_enabled && restored.custom_interval);
        assert_eq!(restored.interval_minutes, 37);
    }
}
