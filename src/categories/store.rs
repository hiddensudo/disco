//! Manifest + one editable TOML document per category. Legacy lists migrate once.
use super::{Collection, Config, read_bounded};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    files: Vec<PathBuf>,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    id: String,
    #[serde(default)]
    name: String,
    icon: String,
    #[serde(default)]
    folders: Vec<PathBuf>,
    #[serde(default)]
    auto_refresh: bool,
    #[serde(default)]
    read_only: bool,
    #[serde(default)]
    auto_save: bool,
}
impl From<&Collection> for Document {
    fn from(c: &Collection) -> Self {
        Self {
            version: 1,
            id: c.id.clone(),
            name: c.name.clone(),
            icon: c.icon.clone(),
            folders: c.folders.clone(),
            auto_refresh: c.auto_refresh,
            auto_save: c.auto_save,
            read_only: c.read_only,
        }
    }
}
impl From<Document> for Collection {
    fn from(c: Document) -> Self {
        Self {
            id: c.id,
            name: c.name,
            icon: c.icon,
            folders: c.folders,
            auto_refresh: c.auto_refresh,
            auto_save: c.auto_save,
            read_only: c.read_only,
        }
    }
}
pub(super) struct Loaded {
    pub config: Config,
    pub stamp: blake3::Hash,
    pub locations: HashMap<String, PathBuf>,
    pub legacy: bool,
}
pub(super) fn default_file(id: &str) -> PathBuf {
    PathBuf::from("categories").join(format!("{id}.toml"))
}
fn valid_reference(path: &Path) -> bool {
    !path.is_absolute()
        && path.extension().is_some_and(|x| x == "toml")
        && path.as_os_str().len() <= 256
        && path.components().all(|c| matches!(c, Component::Normal(_)))
        && path.components().count() >= 2
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    toml::from_str(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub(super) fn read(path: &Path) -> Result<Loaded, String> {
    let bytes = read_bounded(path, 1024 * 1024)?;
    let value: toml::Value = parse(&bytes)?;
    if value.get("version").and_then(toml::Value::as_integer) == Some(1) {
        let config = Config::parse(&bytes)?;
        return Ok(Loaded {
            config,
            stamp: blake3::hash(&bytes),
            locations: HashMap::new(),
            legacy: true,
        });
    }
    let manifest: Manifest = parse(&bytes)?;
    if manifest.version != 2 || manifest.files.len() > 64 {
        return Err("Expected version = 2 and at most 64 category files".into());
    }
    let base = path.parent().ok_or("Missing configuration directory")?;
    let mut unique = HashSet::new();
    let mut items = Vec::new();
    let mut locations = HashMap::new();
    let mut hash = blake3::Hasher::new();
    hash.update(&bytes);
    for reference in manifest.files {
        if !valid_reference(&reference) || !unique.insert(reference.clone()) {
            return Err(
                "Category files must be unique relative TOML paths in subdirectories, without .."
                    .into(),
            );
        }
        let data = read_bounded(&base.join(&reference), 128 * 1024)
            .map_err(|e| format!("{}: {e}", reference.display()))?;
        let doc: Document = parse(&data).map_err(|e| format!("{}: {e}", reference.display()))?;
        if doc.version != 1 {
            return Err(format!(
                "{}: expected category version = 1",
                reference.display()
            ));
        }
        let name = reference.to_string_lossy();
        hash.update(&(name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(&(data.len() as u64).to_le_bytes());
        hash.update(&data);
        locations.insert(doc.id.clone(), reference);
        items.push(doc.into());
    }
    let config = Config { version: 1, items };
    config.validate()?;
    Ok(Loaded {
        config,
        stamp: hash.finalize(),
        locations,
        legacy: false,
    })
}
pub(super) fn fingerprint(path: &Path) -> Option<blake3::Hash> {
    read(path).ok().map(|s| s.stamp)
}
/// Write only changed records so editing one collection preserves unrelated
/// documents (including comments). Publish new references after the files exist.
pub(super) fn save(
    path: &Path,
    config: &Config,
    previous: &HashMap<String, PathBuf>,
) -> Result<Loaded, String> {
    config.validate()?;
    let base = path.parent().ok_or("Missing configuration directory")?;
    let old = read(path).ok();
    let mut files = Vec::new();
    // Validate every newly referenced target before the first write. An orphan
    // file may be an interrupted migration, but may also contain user edits.
    for c in &config.items {
        if !previous.contains_key(&c.id) {
            let target = base_file(path, &c.id);
            if target.exists()
                && parse::<Document>(&read_bounded(&target, 128 * 1024)?)
                    .ok()
                    .as_ref()
                    != Some(&Document::from(c))
            {
                return Err(format!(
                    "A different category document already exists at {}",
                    target.display()
                ));
            }
        }
    }
    for c in &config.items {
        let relative = previous
            .get(&c.id)
            .cloned()
            .unwrap_or_else(|| default_file(&c.id));
        if !valid_reference(&relative) {
            return Err("Invalid category file path".into());
        }
        let unchanged = old.as_ref().is_some_and(|s| {
            !s.legacy
                && s.locations.get(&c.id) == Some(&relative)
                && s.config.items.iter().find(|other| other.id == c.id) == Some(c)
        });
        if !unchanged {
            settings::write(&base.join(&relative), &Document::from(c))?;
        }
        files.push(relative);
    }
    settings::write(path, &Manifest { version: 2, files })?;
    read(path)
}
pub(super) fn open(path: &Path) -> Result<Loaded, String> {
    if !path.exists() {
        return save(path, &Config::default(), &HashMap::new());
    }
    let loaded = read(path)?;
    if !loaded.legacy {
        return Ok(loaded);
    }
    // Keep the exact old bytes before replacing the single-file format.
    let bytes = read_bounded(path, 1024 * 1024)?;
    if blake3::hash(&bytes) != loaded.stamp {
        return Err("categories.toml changed during migration; retry loading it".into());
    }
    let backup = path.with_file_name(format!("categories.v1-{}.toml.bak", loaded.stamp.to_hex()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
    {
        Ok(mut f) => {
            f.write_all(&bytes).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_bounded(&backup, 1024 * 1024)? != bytes {
                return Err("Legacy category backup conflict".into());
            }
        }
        Err(e) => return Err(e.to_string()),
    }
    if read_bounded(path, 1024 * 1024)? != bytes {
        return Err("categories.toml changed during migration; retry loading it".into());
    }
    save(path, &loaded.config, &HashMap::new())
}
pub(super) fn base_file(index: &Path, id: &str) -> PathBuf {
    index
        .parent()
        .unwrap_or(Path::new("."))
        .join(default_file(id))
}
pub(super) fn entry_text(c: &Collection) -> String {
    toml::to_string_pretty(&Document::from(c)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("disco-category-{name}-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn legacy_migration_preserves_exact_backup_and_uses_one_file_per_category() {
        let root = fixture("migration");
        let path = root.join("categories.toml");
        let mut config = Config::default();
        config.items.push(Collection {
            auto_refresh: false,
            auto_save: false,
            read_only: false,
            id: "photos".into(),
            name: "Photos".into(),
            icon: "camera".into(),
            folders: vec!["/data/My Photos".into()],
        });
        let bytes = toml::to_string_pretty(&config).unwrap();
        fs::write(&path, &bytes).unwrap();
        let loaded = open(&path).unwrap();
        assert_eq!(loaded.config, config);
        assert!(!loaded.legacy);
        let manifest: Manifest = parse(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(manifest.version, 2);
        assert_eq!(manifest.files.len(), 6);
        assert_eq!(
            fs::read_to_string(path.with_file_name(format!(
                "categories.v1-{}.toml.bak",
                blake3::hash(bytes.as_bytes()).to_hex()
            )))
            .unwrap(),
            bytes
        );
        assert!(root.join("categories/photos.toml").exists());
        assert_eq!(open(&path).unwrap().stamp, loaded.stamp);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_migrations_resume_but_never_overwrite_different_orphan_files() {
        let root = fixture("migration-resume");
        let path = root.join("categories.toml");
        let config = Config::default();
        settings::write(&path, &config).unwrap();
        let target = root.join(default_file("games"));
        settings::write(&target, &Document::from(&config.items[0])).unwrap();
        assert_eq!(open(&path).unwrap().config, config);
        fs::remove_dir_all(&root).unwrap();
        fs::create_dir_all(&root).unwrap();
        settings::write(&path, &config).unwrap();
        let mut orphan = Document::from(&config.items[0]);
        orphan.folders = vec!["/my/Steam".into()];
        settings::write(&target, &orphan).unwrap();
        let before = fs::read(&target).unwrap();
        assert!(open(&path).is_err());
        assert_eq!(fs::read(&target).unwrap(), before);
        assert_eq!(read(&path).unwrap().config, config);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn child_edits_are_detected_and_unrelated_document_comments_are_preserved() {
        let root = fixture("reload");
        let path = root.join("categories.toml");
        let mut loaded = open(&path).unwrap();
        let games = root.join(&loaded.locations["games"]);
        let docs = root.join(&loaded.locations["documents"]);
        let docs_before = format!(
            "# User comment stays\n{}",
            fs::read_to_string(&docs).unwrap()
        );
        fs::write(&docs, &docs_before).unwrap();
        let mut external: Document = parse(&fs::read(&games).unwrap()).unwrap();
        external.folders = vec!["/data/Steam".into()];
        settings::write(&games, &external).unwrap();
        let reloaded = read(&path).unwrap();
        assert_ne!(reloaded.stamp, loaded.stamp);
        assert_eq!(reloaded.config.items[0].folders, external.folders);
        loaded = reloaded;
        loaded.config.items[0].name = "Steam".into();
        save(&path, &loaded.config, &loaded.locations).unwrap();
        assert_eq!(fs::read_to_string(docs).unwrap(), docs_before);
        fs::write(&path, "version=2\nfiles=['../outside.toml']\n").unwrap();
        assert!(read(&path).is_err());
        fs::write(
            &path,
            "version=2\nfiles=['categories/games.toml','categories/games.toml']\n",
        )
        .unwrap();
        assert!(read(&path).is_err());
        fs::write(&path, "version=2\nfiles=['categories/missing.toml']\n").unwrap();
        assert!(read(&path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
