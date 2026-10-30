//! A user-owned TOML theme catalog and independent color files.
use crate::{
    i18n::Language,
    settings::{self, Theme},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const PRESETS: [(&str, &str); 8] = [
    ("carbon", include_str!("../assets/themes/carbon.toml")),
    ("ember", include_str!("../assets/themes/ember.toml")),
    ("noir", include_str!("../assets/themes/noir.toml")),
    ("midnight", include_str!("../assets/themes/midnight.toml")),
    ("forest", include_str!("../assets/themes/forest.toml")),
    ("paper", include_str!("../assets/themes/paper.toml")),
    ("cairo", include_str!("../assets/themes/cairo.toml")),
    ("glacier", include_str!("../assets/themes/glacier.toml")),
];

pub fn preset(name: &str) -> Option<Theme> {
    PRESETS.iter().find_map(|(id, text)| {
        let theme: Theme = toml::from_str(text).expect("bundled theme TOML");
        (name.eq_ignore_ascii_case(id) || name == theme.name).then_some(theme)
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub name: String,
    pub file: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u8,
    pub active: String,
    pub themes: BTreeMap<String, Descriptor>,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub theme: Option<Theme>,
    pub error: Option<String>,
}
#[derive(Clone)]
struct State {
    catalog: Catalog,
    entries: Vec<Entry>,
    selected: String,
    dirty: BTreeSet<String>,
    baseline: BTreeMap<PathBuf, Option<blake3::Hash>>,
}
pub struct Manager {
    root: PathBuf,
    state: State,
    snapshot: Option<State>,
    catalog_error: Option<String>,
    pub creating: bool,
    pub new_name: String,
    pub error: String,
    pub prompt: Option<(Language, String)>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn theme_path(root: &Path, id: &str, entry: &Descriptor) -> Result<PathBuf, String> {
    if !valid_id(id) || entry.file != PathBuf::from(format!("themes/{id}.toml")) {
        return Err(crate::tr!(
            "Тема потребує ID та шляху themes/<id>.toml",
            "A theme needs an ID and a themes/<id>.toml path"
        )
        .into());
    }
    if entry.name.trim().is_empty()
        || entry.name.chars().count() > 80
        || entry.name.chars().any(char::is_control)
    {
        return Err(crate::tr!(
            "Назва теми: 1–80 символів без керувальних знаків",
            "Theme name: 1–80 characters without control characters"
        )
        .into());
    }
    Ok(root.join(&entry.file))
}
fn read_catalog(root: &Path) -> Result<Catalog, String> {
    let path = root.join("themes.toml");
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 256 * 1024 {
        return Err("Theme catalog is too large".into());
    }
    let catalog: Catalog = toml::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if catalog.version != 1 || catalog.themes.len() > 256 || !valid_id(&catalog.active) {
        return Err(
            "Expected theme catalog version = 1, a valid active ID and at most 256 themes".into(),
        );
    }
    Ok(catalog)
}
fn read_entry(root: &Path, id: &str, entry: &Descriptor) -> Result<Theme, String> {
    let path = theme_path(root, id, entry)?;
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 {
        return Err("Theme file is too large".into());
    }
    let mut theme = settings::read_theme(&path)?;
    theme.name = entry.name.clone();
    Ok(theme)
}
pub fn load_active(root: &Path) -> Result<Theme, String> {
    let catalog = read_catalog(root)?;
    if let Some(entry) = catalog.themes.get(&catalog.active) {
        if let Ok(theme) = read_entry(root, &catalog.active, entry) {
            return Ok(theme);
        }
    }
    for (id, entry) in &catalog.themes {
        if let Ok(theme) = read_entry(root, id, entry) {
            return Ok(theme);
        }
    }
    Err(crate::tr!(
        "У каталозі немає придатної теми",
        "The catalog has no usable theme"
    )
    .into())
}
fn fingerprint(path: &Path) -> Option<blake3::Hash> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 256 * 1024 {
        return Some(blake3::hash(
            format!("oversize:{}:{:?}", meta.len(), meta.modified().ok()).as_bytes(),
        ));
    }
    fs::read(path).ok().map(|data| blake3::hash(&data))
}
fn fingerprints(root: &Path, catalog: &Catalog) -> BTreeMap<PathBuf, Option<blake3::Hash>> {
    let mut paths = vec![root.join("themes.toml")];
    paths.extend(
        catalog
            .themes
            .iter()
            .filter_map(|(id, e)| theme_path(root, id, e).ok()),
    );
    paths
        .into_iter()
        .map(|p| {
            let value = fingerprint(&p);
            (p, value)
        })
        .collect()
}
fn state_from(root: &Path, catalog: Catalog) -> State {
    let entries: Vec<_> = catalog
        .themes
        .iter()
        .map(|(id, e)| {
            let value = read_entry(root, id, e);
            Entry {
                id: id.clone(),
                name: e.name.clone(),
                theme: value.as_ref().ok().cloned(),
                error: value.err(),
            }
        })
        .collect();
    let selected = entries
        .iter()
        .find(|e| e.id == catalog.active && e.theme.is_some())
        .or_else(|| entries.iter().find(|e| e.theme.is_some()))
        .map(|e| e.id.clone())
        .unwrap_or_default();
    let baseline = fingerprints(root, &catalog);
    State {
        catalog,
        entries,
        selected,
        dirty: BTreeSet::new(),
        baseline,
    }
}
fn bootstrap(root: &Path, current: &Theme) -> Result<(), String> {
    if root.join("themes.toml").exists() {
        return Ok(());
    }
    fs::create_dir_all(root.join("themes")).map_err(|e| e.to_string())?;
    let mut catalog: Catalog =
        toml::from_str(include_str!("../assets/themes.toml")).expect("bundled theme catalog");
    for (id, text) in PRESETS {
        let file = root.join(format!("themes/{id}.toml"));
        if !file.exists() {
            let theme: Theme = toml::from_str(text).expect("bundled theme");
            settings::write(&file, &theme)?;
        }
    }
    if let Some((id, _)) = catalog.themes.iter().find(|(id, e)| {
        read_entry(root, id, e)
            .as_ref()
            .is_ok_and(|theme| theme == current)
    }) {
        catalog.active = id.clone();
    } else {
        let mut id = "legacy-theme".to_string();
        let mut suffix = 2;
        while root.join(format!("themes/{id}.toml")).exists() {
            if settings::read_theme(&root.join(format!("themes/{id}.toml")))
                .as_ref()
                .is_ok_and(|t| t == current)
            {
                break;
            }
            id = format!("legacy-theme-{suffix}");
            suffix += 1;
        }
        settings::write(&root.join(format!("themes/{id}.toml")), current)?;
        catalog.themes.insert(
            id.clone(),
            Descriptor {
                name: current.name.clone(),
                file: format!("themes/{id}.toml").into(),
            },
        );
        catalog.active = id;
    }
    // The old theme.toml stays intact as a migration backup. Publish index last.
    if !root.join("themes.toml").exists() {
        settings::write(&root.join("themes.toml"), &catalog)?;
    }
    Ok(())
}

impl Manager {
    pub fn load(root: PathBuf, current: &Theme) -> Self {
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir().unwrap_or_default().join(root)
        };
        let result = bootstrap(&root, current).and_then(|_| read_catalog(&root));
        let error = result.as_ref().err().cloned();
        let catalog = result
            .unwrap_or_else(|_| toml::from_str(include_str!("../assets/themes.toml")).unwrap());
        let mut state = state_from(&root, catalog);
        if error.is_some() {
            for entry in &mut state.entries {
                entry.theme = preset(&entry.id);
            }
            state.selected = "carbon".into();
        }
        Self {
            root,
            state,
            snapshot: None,
            catalog_error: error.clone(),
            creating: false,
            new_name: String::new(),
            error: error.unwrap_or_default(),
            prompt: None,
        }
    }
    pub fn entries(&self) -> &[Entry] {
        &self.state.entries
    }
    pub fn selected(&self) -> &str {
        &self.state.selected
    }
    pub fn begin_edit(&mut self) {
        if self.snapshot.is_none() {
            self.snapshot = Some(self.state.clone());
        }
    }
    pub fn cancel(&mut self) {
        if let Some(old) = self.snapshot.take() {
            self.state = old;
        }
        self.creating = false;
        self.new_name.clear();
        self.prompt = None;
        self.error = self.catalog_error.clone().unwrap_or_default();
    }
    pub fn remember(&mut self, theme: &Theme) {
        if theme.validate().is_err() {
            return;
        }
        if let Some(entry) = self
            .state
            .entries
            .iter_mut()
            .find(|e| e.id == self.state.selected)
        {
            if entry.theme.as_ref() != Some(theme) {
                entry.theme = Some(theme.clone());
                entry.name = theme.name.clone();
                if let Some(descriptor) = self.state.catalog.themes.get_mut(&entry.id) {
                    descriptor.name = theme.name.clone();
                }
                self.state.dirty.insert(entry.id.clone());
            }
        }
    }
    pub fn select(&mut self, id: &str, current: &Theme) -> Result<Theme, String> {
        self.remember(current);
        let entry = self
            .state
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or("Theme is not in the catalog")?;
        let theme = entry
            .theme
            .clone()
            .ok_or_else(|| entry.error.clone().unwrap_or_default())?;
        self.state.selected = id.into();
        self.error = self.catalog_error.clone().unwrap_or_default();
        Ok(theme)
    }
    fn fresh_id(&self, name: &str) -> String {
        let base = name
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let base = base.trim_matches('-').chars().take(48).collect::<String>();
        let base = if base.is_empty() { "theme" } else { &base };
        (1..)
            .map(|n| {
                if n == 1 {
                    base.to_string()
                } else {
                    format!("{base}-{n}")
                }
            })
            .find(|id| {
                !self.state.catalog.themes.contains_key(id)
                    && !self.root.join(format!("themes/{id}.toml")).exists()
            })
            .unwrap()
    }
    pub fn create(&mut self, name: &str, template: &Theme) -> Result<Theme, String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err(crate::tr!(
                "Введи назву теми: 1–80 символів",
                "Enter a theme name: 1–80 characters"
            )
            .into());
        }
        if self
            .state
            .entries
            .iter()
            .any(|e| e.name.to_lowercase() == name.to_lowercase())
        {
            return Err(crate::tr!(
                "Тема з такою назвою вже існує",
                "A theme with that name already exists"
            )
            .into());
        }
        if self.state.entries.len() >= 256 {
            return Err("At most 256 themes are supported".into());
        }
        let mut theme = template.clone();
        theme.name = name.into();
        theme.validate()?;
        let id = self.fresh_id(name);
        let file = PathBuf::from(format!("themes/{id}.toml"));
        self.state.baseline.insert(self.root.join(&file), None);
        self.state.catalog.themes.insert(
            id.clone(),
            Descriptor {
                name: name.into(),
                file,
            },
        );
        self.state.entries.push(Entry {
            id: id.clone(),
            name: name.into(),
            theme: Some(theme.clone()),
            error: None,
        });
        self.state.dirty.insert(id.clone());
        self.state.selected = id;
        self.creating = false;
        self.new_name.clear();
        self.error.clear();
        Ok(theme)
    }
    pub fn import(&mut self, theme: &Theme, current: &Theme) -> Result<Theme, String> {
        if let Some(id) = self
            .state
            .entries
            .iter()
            .find(|e| e.theme.as_ref() == Some(theme))
            .map(|e| e.id.clone())
        {
            return self.select(&id, current);
        }
        self.remember(current);
        let base = theme.name.clone();
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base} {n}")
                }
            })
            .find(|name| {
                !self
                    .state
                    .entries
                    .iter()
                    .any(|e| e.name.to_lowercase() == name.to_lowercase())
            })
            .unwrap();
        self.create(&name, theme)
    }
    pub fn reload(&mut self) -> Result<Theme, String> {
        let catalog = read_catalog(&self.root)?;
        self.state = state_from(&self.root, catalog);
        self.catalog_error = None;
        self.error.clear();
        self.state
            .entries
            .iter()
            .find(|e| e.id == self.state.selected)
            .and_then(|e| e.theme.clone())
            .ok_or_else(|| {
                crate::tr!(
                    "Немає придатної теми. Створи нову або виправ файл.",
                    "No usable theme. Create one or repair the file."
                )
                .into()
            })
    }
    pub fn save(&mut self, current: &Theme) -> Result<(), String> {
        current.validate()?;
        if let Some(error) = &self.catalog_error {
            return Err(error.clone());
        }
        for (path, expected) in &self.state.baseline {
            if fingerprint(path) != *expected {
                return Err(crate::tr!(
                    "Файли тем змінено зовні. Онови список перед збереженням.",
                    "Theme files changed outside disco. Reload the list before saving."
                )
                .into());
            }
        }
        if self.state.selected.is_empty() {
            self.create(&current.name, current)?;
        }
        self.remember(current);
        for id in &self.state.dirty {
            let entry = self.state.entries.iter().find(|e| &e.id == id).unwrap();
            let theme = entry.theme.as_ref().ok_or("Invalid pending theme")?;
            theme.validate()?;
            theme_path(&self.root, id, &self.state.catalog.themes[id])?;
        }
        for id in &self.state.dirty {
            let entry = self.state.entries.iter().find(|e| &e.id == id).unwrap();
            settings::write(
                &theme_path(&self.root, id, &self.state.catalog.themes[id])?,
                entry.theme.as_ref().unwrap(),
            )?;
        }
        self.state.catalog.active = self.state.selected.clone();
        settings::write(&self.root.join("themes.toml"), &self.state.catalog)?;
        self.state.baseline = fingerprints(&self.root, &self.state.catalog);
        self.state.dirty.clear();
        self.snapshot = None;
        self.prompt = None;
        Ok(())
    }
    pub fn generation_prompt(&self, language: Language, current: &Theme) -> String {
        let id = self.fresh_id("generated-theme");
        let file = self.root.join(format!("themes/{id}.toml"));
        let path = file.display();
        let manifest = self.root.join("themes.toml");
        let manifest = manifest.display();
        let mut sample = current.clone();
        sample.name = "Generated theme".into();
        let schema = toml::to_string_pretty(&sample).unwrap();
        let existing = toml::to_string_pretty(
            &read_catalog(&self.root).unwrap_or_else(|_| self.state.catalog.clone()),
        )
        .unwrap();
        let catalog = Catalog {
            version: 1,
            active: id.clone(),
            themes: BTreeMap::from([(
                id.clone(),
                Descriptor {
                    name: sample.name.clone(),
                    file: format!("themes/{id}.toml").into(),
                },
            )]),
        };
        let registration = toml::to_string_pretty(&catalog).unwrap();
        crate::tr!(
            "Створи кольорову тему для нативного Linux-застосунку disco за моїм описом наприкінці повідомлення.\nФормат: TOML; кольори #RRGGBB; radius від 0 до 12. Не додавай полів поза схемою. Основний текст має мати контраст не менше 4,5:1 на background, panel і raised.\n\nАбсолютний шлях файлу теми: {path}\nКаталог тем: {manifest}\nID: {id}\n\nПовна схема файлу теми (заміни назву та кольори):\n{schema}\nЧинний каталог (збережи всі його записи):\n{existing}\nФрагмент реєстрації (об’єднай з чинним каталогом, збережи всі інші теми й узгодь назву у двох файлах):\n{registration}\nЯкщо маєш доступ до мого ПК, створи файл за вказаним шляхом, акуратно додай запис у каталог і обери його active. Не перезаписуй іншу тему; при конфлікті ID/шляху вибери новий ID і узгоджено зміни посилання. Перевір TOML і контраст.\nЯкщо доступу до ПК немає, створи файл для завантаження (або надай повний вміст), точні команди створення директорії та розміщення файлу за наведеним шляхом і точну зміну каталогу без втрати інших записів. Після додавання файлів натисни «Оновити теми» в disco та «Зберегти».\n\nНижче я опишу бажану тему, світлий чи темний фон, акцент і кольори:\n",
            "Create a color theme for the native Linux app disco based on the description I append at the end.\nFormat: TOML; colors #RRGGBB; radius from 0 to 12. Do not add fields outside the schema. Main text contrast must be at least 4.5:1 on background, panel and raised.\n\nAbsolute theme file path: {path}\nTheme catalog: {manifest}\nID: {id}\n\nComplete theme file schema (replace the name and colors):\n{schema}\nExisting catalog (preserve every entry):\n{existing}\nRegistration fragment (merge with the existing catalog, preserve all other themes and keep the name consistent in both files):\n{registration}\nIf you have access to my PC, create the file at that path, carefully register it and select it as active. Do not overwrite another theme; if an ID or path is taken, choose a new ID and update the references consistently. Validate TOML and contrast.\nIf you do not have access to my PC, create a downloadable file (or provide its full contents), exact commands to create the directory and place the file at the path above, and the precise catalog change without losing other entries. After adding the files, click Reload themes in disco and Save.\n\nBelow I will describe the desired theme, light or dark background, accent and colors:\n",
            @language language,path=path,manifest=manifest,id=id,schema=schema,existing=existing,registration=registration
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("disco-themes-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn bootstrap_preserves_legacy_colors_and_every_preset_is_valid() {
        let root = root("migration");
        let mut legacy = Theme::default();
        legacy.accent = "#e16e55".into();
        settings::write(&root.join("theme.toml"), &legacy).unwrap();
        let bytes = fs::read(root.join("theme.toml")).unwrap();
        let manager = Manager::load(root.clone(), &legacy);
        assert_eq!(manager.entries().len(), 9);
        assert_eq!(load_active(&root).unwrap(), legacy);
        assert_eq!(fs::read(root.join("theme.toml")).unwrap(), bytes);
        for (_, text) in PRESETS {
            let theme: Theme = toml::from_str(text).unwrap();
            theme.validate().unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn create_cancel_save_and_external_conflicts_are_safe() {
        let root = root("drafts");
        let original = Theme::default();
        let mut manager = Manager::load(root.clone(), &original);
        manager.begin_edit();
        manager.create("My theme", &original).unwrap();
        let id = manager.selected().to_string();
        assert!(!root.join(format!("themes/{id}.toml")).exists());
        manager.cancel();
        assert_eq!(manager.entries().len(), 8);
        manager.begin_edit();
        let mut current = original.clone();
        for i in 0..20 {
            current = manager.create(&format!("Custom {i}"), &current).unwrap();
        }
        manager.save(&current).unwrap();
        assert_eq!(read_catalog(&root).unwrap().themes.len(), 28);
        assert_eq!(load_active(&root).unwrap().name, "Custom 19");
        let path = root.join(format!("themes/{}.toml", manager.selected()));
        fs::write(&path, "external = true\n").unwrap();
        assert!(manager.save(&current).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "external = true\n");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn broken_theme_isolated_and_prompts_have_real_paths_in_both_languages() {
        let root = root("invalid");
        let mut manager = Manager::load(root.clone(), &Theme::default());
        fs::write(root.join("themes/carbon.toml"), "bad theme").unwrap();
        let active = manager.reload().unwrap();
        active.validate().unwrap();
        assert!(
            manager
                .entries()
                .iter()
                .find(|e| e.id == "carbon")
                .unwrap()
                .error
                .is_some()
        );
        for language in [Language::Uk, Language::En] {
            let prompt = manager.generation_prompt(language, &active);
            assert!(prompt.contains(root.to_str().unwrap()));
            assert!(prompt.contains("themes.toml"));
            assert!(prompt.contains("#RRGGBB"));
            assert!(!prompt.contains("{{"));
            assert_eq!(
                prompt.contains("If you do not have access to my PC"),
                language == Language::En
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
