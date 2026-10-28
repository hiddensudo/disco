//! User-owned folder collections. The TOML file is data, never executable instructions.
use crate::{index::Category, settings};
use serde::{Deserialize, Serialize};
mod store;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub const BUILTINS: [(&str, Category, &str); 5] = [
    ("games", Category::Games, "game"),
    ("development", Category::Development, "code"),
    ("caches", Category::Cache, "cache"),
    ("documents", Category::Documents, "document"),
    ("other", Category::Other, "other"),
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Collection {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub icon: String,
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    #[serde(default)]
    pub auto_refresh: bool,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub auto_save: bool,
}
impl Collection {
    pub fn category(&self) -> Category {
        BUILTINS
            .iter()
            .find(|b| b.0 == self.id)
            .map(|b| b.1)
            .unwrap_or_else(|| {
                let hash = blake3::hash(self.id.as_bytes());
                Category::Custom(u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap()))
            })
    }
    pub fn title(&self) -> &str {
        if self.name.is_empty() {
            self.category().name()
        } else {
            &self.name
        }
    }
    pub fn builtin(&self) -> bool {
        BUILTINS.iter().any(|b| b.0 == self.id)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default, rename = "category")]
    pub items: Vec<Collection>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            items: BUILTINS
                .iter()
                .map(|(id, _, icon)| Collection {
                    auto_refresh: false,
                    auto_save: false,
                    read_only: false,
                    id: id.to_string(),
                    name: String::new(),
                    icon: icon.to_string(),
                    folders: vec![],
                })
                .collect(),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.items.len() > 64 {
            return Err("Expected version = 1 and at most 64 categories".into());
        }
        let mut ids = HashSet::new();
        let mut keys = HashSet::new();
        for c in &self.items {
            if c.id.is_empty()
                || c.id.len() > 64
                || !c
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
                || !ids.insert(&c.id)
                || !keys.insert(c.category())
            {
                return Err("Category IDs must be unique: lowercase letters, digits, - or _ (1–64 characters)".into());
            }
            if c.name.chars().count() > 80
                || c.name.chars().any(char::is_control)
                || (!c.builtin() && c.name.trim().is_empty())
            {
                return Err(format!("{}: provide a name of 1–80 characters", c.id));
            }
            if !crate::art::category_icons()
                .iter()
                .any(|(name, _)| *name == c.icon)
                && !valid_icon_path(&c.icon)
            {
                return Err(format!(
                    "{}: unknown icon; use a built-in name or icons/<hash>.svg/.png",
                    c.id
                ));
            }
            if c.folders.len() > 128
                || c.folders.iter().any(|p| {
                    !p.is_absolute()
                        || p.as_os_str().len() > 4096
                        || p.components()
                            .any(|x| matches!(x, Component::ParentDir | Component::CurDir))
                        || p.to_string_lossy().chars().any(char::is_control)
                })
            {
                return Err(format!(
                    "{}: use at most 128 absolute folder paths without ..",
                    c.id
                ));
            }
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn read(path: &Path) -> Result<Self, String> {
        store::read(path).map(|s| s.config)
    }
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let config: Self = toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }
    pub fn get(&self, key: Category) -> Option<&Collection> {
        self.items.iter().find(|c| c.category() == key)
    }
    /// Insert at a row boundary; IDs stay stable and the current category is
    /// unaffected. The fixed Trash shortcut is not part of this list.
    pub fn reorder(&mut self, source: &str, target: &str, after: bool) -> bool {
        let Some(from) = self.items.iter().position(|c| c.id == source) else {
            return false;
        };
        let Some(to) = self.items.iter().position(|c| c.id == target) else {
            return false;
        };
        let boundary = to + usize::from(after);
        let destination = boundary - usize::from(from < boundary);
        if from == destination {
            return false;
        }
        let item = self.items.remove(from);
        self.items.insert(destination, item);
        true
    }
}
pub fn path() -> PathBuf {
    settings::config_dir().join("categories.toml")
}
pub fn valid_icon_path(s: &str) -> bool {
    let Some(file) = s.strip_prefix("icons/") else {
        return false;
    };
    let Some((stem, ext)) = file.rsplit_once('.') else {
        return false;
    };
    stem.len() == 64 && stem.bytes().all(|b| b.is_ascii_hexdigit()) && matches!(ext, "svg" | "png")
}
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| e.to_string())?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Expected a regular file".into());
    }
    let mut bytes = Vec::new();
    f.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("File is too large".into());
    }
    Ok(bytes)
}
pub fn fingerprint() -> Option<blake3::Hash> {
    store::fingerprint(&path())
}
#[cfg(test)]
pub fn category_path(c: &Collection) -> PathBuf {
    store::base_file(&path(), &c.id)
}
pub fn prompt(c: &Collection, file: &Path, language: crate::i18n::Language) -> String {
    let entry = store::entry_text(c);
    let name = c.title();
    let path = file.display();
    let manifest = self::path();
    let manifest = manifest.display();
    let id = &c.id;
    crate::tr!(
        "Допоможи заповнити категорію disco «{name}».\nФайл цієї категорії: {path}\nСпільний список категорій: {manifest}\nПрочитай обидва файли. Знайди на цьому ПК папки, які відповідають категорії, та онови лише масив folders у файлі категорії з id = {id}. Врахуй назву: наприклад, фотографії, усі ігри, лише Steam або лише Epic Games. Якщо критерії незрозумілі — уточни їх у мене. Не вигадуй шляхи. Використовуй абсолютні шляхи до наявних папок, без дублікатів і вкладених повторів; не додавай / або весь домашній каталог. Якщо файла категорії ще немає, створи його за записом нижче й додай відносний шлях до files у спільному categories.toml (version = 2). Збережи назву, id та іконку, інші категорії й налаштування. Не видаляй і не переміщуй файли користувача. Перевір TOML та коротко покажи додані папки.\nФормат окремого файла категорії:\n{entry}",
        "Help populate the disco category ‘{name}’.\nCategory file: {path}\nShared category index: {manifest}\nRead both files. Find existing folders on this PC that match the category and update only folders in the category file with id = {id}. Use the name as the criteria: for example photos, all games, Steam games only or Epic Games only. Ask me if the criteria are unclear. Do not invent paths. Use absolute paths to existing directories, without duplicates or redundant nested roots; do not include / or the entire home directory. If the category file does not exist, create it using the entry below and add its relative path to files in the shared categories.toml (version = 2). Preserve the name, id, icon, other categories and settings. Do not delete or move user files. Validate the TOML and briefly show the folders added.\nIndividual category file format:\n{entry}",
        @language language, name=name,path=path,manifest=manifest,id=id,entry=entry
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toml_round_trip_and_stable_keys() {
        let mut c = Config::default();
        c.items[0].auto_refresh = true;
        c.items[0].read_only = true;
        c.items.push(Collection {
            auto_refresh: false,
            auto_save: false,
            read_only: false,
            id: "photos".into(),
            name: "Photos".into(),
            icon: "camera".into(),
            folders: vec!["/data/Photos".into()],
        });
        c.validate().unwrap();
        let parsed: Config = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(parsed, c);
        assert_eq!(parsed.items[5].category(), c.items[5].category());
        assert!(
            prompt(
                &c.items[5],
                &category_path(&c.items[5]),
                crate::i18n::Language::En
            )
            .contains("id = photos")
        );
        assert!(
            prompt(
                &c.items[5],
                &category_path(&c.items[5]),
                crate::i18n::Language::En
            )
            .contains("version = 1")
        );
    }
    #[test]
    fn prompt_language_is_explicit_and_points_to_the_individual_category_file() {
        let c = Collection {
            auto_refresh: false,
            auto_save: false,
            read_only: false,
            id: "photos".into(),
            name: "Photos".into(),
            icon: "camera".into(),
            folders: vec![],
        };
        let file = category_path(&c);
        crate::i18n::with_language(crate::i18n::Language::En, || {
            let ua = prompt(&c, &file, crate::i18n::Language::Uk);
            let en = prompt(&c, &file, crate::i18n::Language::En);
            assert!(ua.starts_with('Д'));
            assert!(en.starts_with("Help populate"));
            assert!(
                ua.contains(&file.display().to_string())
                    && en.contains(&file.display().to_string())
            );
            assert!(en.contains("Steam games only") && en.contains("Epic Games only"));
            assert!(en.contains("version = 2") && en.contains("version = 1"));
        });
    }
    #[test]
    fn invalid_and_duplicate_config_is_rejected() {
        let mut c = Config::default();
        c.items.push(c.items[0].clone());
        assert!(c.validate().is_err());
        c.items.pop();
        c.items[0].folders = vec!["relative/path".into()];
        assert!(c.validate().is_err());
        c.items[0].folders = vec!["/data/../private".into()];
        assert!(c.validate().is_err());
        c.items[0].folders.clear();
        c.items[0].icon = "icons/../../outside.svg".into();
        assert!(c.validate().is_err());
    }
}

pub struct Manager {
    pub config: Config,
    pub draft: Option<Config>,
    pub editing: usize,
    pub folder_input: String,
    pub error: String,
    pub revision: u64,
    pub indexed: Option<(u64, u64)>,
    pub missing: std::collections::HashMap<Category, Vec<PathBuf>>,
    stamp: Option<blake3::Hash>,
    locations: std::collections::HashMap<String, PathBuf>,
    checked: std::time::Instant,
}
impl Manager {
    pub fn reorder(&mut self, source: &str, target: &str, after: bool) -> Result<bool, String> {
        // Never publish over a settings draft or externally edited category.
        if self.draft.is_some() {
            return Err(crate::tr!(
                "Спочатку збережи або скасуй редагування категорій.",
                "Save or cancel category edits first."
            )
            .into());
        }
        let mut config = self.config.clone();
        if !config.reorder(source, target, after) {
            return Ok(false);
        }
        if fingerprint() != self.stamp {
            return Err(crate::tr!(
                "Категорії змінено ззовні. Перезавантаж їх перед зміною порядку.",
                "Categories changed externally. Reload them before reordering."
            )
            .into());
        }
        // Unchanged category documents keep their comments and exact bytes.
        let saved = store::save(&path(), &config, &self.locations)?;
        self.config = saved.config;
        self.locations = saved.locations;
        self.stamp = Some(saved.stamp);
        self.revision = self.revision.wrapping_add(1);
        Ok(true)
    }
    pub fn load() -> Self {
        let path = path();
        let result = store::open(&path);
        let (config, stamp, locations, error) = match result {
            Ok(s) => (s.config, Some(s.stamp), s.locations, String::new()),
            Err(e) => (Config::default(), fingerprint(), Default::default(), e),
        };
        Self {
            config,
            error,
            draft: None,
            editing: 0,
            folder_input: String::new(),
            revision: 0,
            indexed: None,
            missing: Default::default(),
            stamp,
            locations,
            checked: std::time::Instant::now(),
        }
    }
    pub fn reload(&mut self) -> Result<(), String> {
        let loaded = store::read(&path())?;
        self.config = loaded.config;
        self.stamp = Some(loaded.stamp);
        self.locations = loaded.locations;
        self.revision = self.revision.wrapping_add(1);
        self.error.clear();
        Ok(())
    }
    pub fn maintain(&mut self) {
        if self.checked.elapsed().as_secs() < 3 {
            return;
        }
        self.checked = std::time::Instant::now();
        if fingerprint() != self.stamp {
            if let Err(error) = self.reload() {
                self.error = error;
            }
        }
    }
    pub fn file_for(&self, c: &Collection) -> PathBuf {
        settings::config_dir().join(
            self.locations
                .get(&c.id)
                .cloned()
                .unwrap_or_else(|| store::default_file(&c.id)),
        )
    }
    pub fn save(&mut self) -> Result<(), String> {
        let Some(config) = &self.draft else {
            return Ok(());
        };
        config.validate()?;
        if fingerprint() != self.stamp {
            return Err(crate::tr!(
                "categories.toml змінено ззовні. Перезавантаж його перед збереженням.",
                "categories.toml changed externally. Reload it before saving."
            )
            .into());
        }
        let saved = store::save(&path(), config, &self.locations)?;
        self.config = saved.config;
        self.locations = saved.locations;
        self.stamp = Some(saved.stamp);
        self.revision = self.revision.wrapping_add(1);
        self.draft = None;
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    #[test]
    fn external_edits_reload_and_conflicts_preserve_the_file_and_draft() {
        let Ok(root) = std::env::var("DISCO_CATEGORY_FIXTURE") else {
            let root =
                std::env::temp_dir().join(format!("disco-category-test-{}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            let out=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","categories::integration_tests::external_edits_reload_and_conflicts_preserve_the_file_and_draft","--nocapture"])
                .env("DISCO_CATEGORY_FIXTURE",&root).env("DISCO_CONFIG_DIR",root.join("config")).output().unwrap();
            fs::remove_dir_all(root).unwrap();
            assert!(
                out.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            return;
        };
        let mut manager = Manager::load();
        assert!(manager.error.is_empty());
        manager.draft = Some(manager.config.clone());
        manager.draft.as_mut().unwrap().items[0].name = "Local draft".into();
        let mut external = manager.config.clone();
        external.items[0].name = "External edit".into();
        let saved_manifest = fs::read(path()).unwrap();
        let child_file = manager.file_for(&manager.config.items[0]);
        let mut child: toml::Value =
            toml::from_str(&fs::read_to_string(&child_file).unwrap()).unwrap();
        child["name"] = toml::Value::String("External edit".into());
        settings::write(&child_file, &child).unwrap();
        assert!(manager.save().is_err());
        assert_eq!(Config::read(&path()).unwrap(), external);
        assert_eq!(manager.draft.as_ref().unwrap().items[0].name, "Local draft");
        manager.reload().unwrap();
        assert_eq!(manager.config, external);
        fs::write(path(), "broken = [").unwrap();
        assert!(manager.reload().is_err());
        assert_eq!(manager.config, external);
        fs::write(path(), saved_manifest).unwrap();
        manager.draft = Some(manager.config.clone());
        manager.draft.as_mut().unwrap().items.push(Collection {
            auto_refresh: false,
            auto_save: false,
            read_only: false,
            id: "photos".into(),
            name: "Photos".into(),
            icon: "camera".into(),
            folders: vec!["/data/Pictures".into()],
        });
        manager.save().unwrap();
        let restored = Manager::load();
        assert_eq!(restored.config, manager.config);
        assert_eq!(
            restored.config.items.last().unwrap().folders,
            vec![PathBuf::from("/data/Pictures")]
        );
        let icon = PathBuf::from(root).join("custom.svg");
        fs::write(&icon,r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="#fff"/></svg>"##).unwrap();
        let imported = crate::art::import_icon(&icon).unwrap();
        assert!(valid_icon_path(&imported));
        let mut art = crate::art::Art::load(&eframe::egui::Context::default());
        let index = art.category_icon(&imported);
        assert!(index >= 59);
        assert_eq!(art.category_icon(&imported), index);
        assert_eq!(crate::art::import_icon(&icon).unwrap(), imported);
    }
}
