use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    pub name: String,
    pub accent: String,
    pub trash: String,
    pub background: String,
    pub panel: String,
    pub raised: String,
    pub border: String,
    pub text: String,
    pub muted: String,
    pub radius: u8,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Carbon".into(),
            accent: "#e45a52".into(),
            trash: "#e3b368".into(),
            background: "#101417".into(),
            panel: "#111518".into(),
            raised: "#1f252a".into(),
            border: "#30373d".into(),
            text: "#eef0f3".into(),
            muted: "#bdc5ce".into(),
            radius: 3,
        }
    }
}
impl Theme {
    #[cfg(test)]
    pub fn preset(name: &str) -> Self {
        crate::themes::preset(name).unwrap_or_default()
    }
    pub fn validate(&self) -> Result<(), String> {
        for c in [
            &self.accent,
            &self.trash,
            &self.background,
            &self.panel,
            &self.raised,
            &self.border,
            &self.text,
            &self.muted,
        ] {
            parse_color(c)?;
        }
        if self.radius > 12 {
            return Err(crate::tr!(
                "Радіус має бути від 0 до 12",
                "Corner radius must be between 0 and 12"
            )
            .into());
        }
        if contrast(color(&self.text), color(&self.background)) < 4.5
            || contrast(color(&self.text), color(&self.panel)) < 4.5
            || contrast(color(&self.text), color(&self.raised)) < 4.5
        {
            return Err(crate::tr!(
                "Контраст основного тексту має бути не менше 4,5:1",
                "Main text contrast must be at least 4.5:1"
            )
            .into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub shortcuts: crate::shortcuts::Shortcuts,
    #[serde(default)]
    pub language: crate::i18n::Language,
    /// None follows the interface language; Some enables the separate selector.
    pub prompt_language: Option<crate::i18n::Language>,
    pub categories: bool,
    pub category_labels: bool,
    pub title_button_labels: bool,
    pub categories_locked: bool,
    pub properties: bool,
    pub category_width: f32,
    pub property_width: f32,
    pub compact: bool,
    pub text_scale: f32,
    #[serde(default)]
    pub text_scale_version: u8,
    pub sidebar_text_scale: f32,
    pub file_zoom_step: f32,
    pub file_type_icons: bool,
    pub show_count: bool,
    pub drive_name: String,
    pub icon_scale: f32,
    pub category_sizes: bool,
    pub row_separators: bool,
    pub striped_rows: bool,
    pub drive_model: bool,
    pub single_click_expand: bool,
    pub reuse_scan: bool,
    pub gentle_scan: bool,
    pub save_scan: bool,
    pub auto_refresh: bool,
    pub animations: bool,
    pub motion_speed: f32,
    pub preview_mode: PreviewMode,
    pub open_files_on_double_click: bool,
    pub home_path: String,
    pub search_home_fallback: bool,
    pub all_files_auto_refresh: bool,
    pub all_files_read_only: bool,
    pub all_files_auto_save: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewMode {
    Off,
    #[default]
    #[serde(rename = "overview", alias = "inspector")]
    Overview,
    #[serde(rename = "focus", alias = "workspace")]
    Focus,
}
pub fn default_home() -> PathBuf {
    if Path::new("/home").is_dir() {
        "/home".into()
    } else {
        "/".into()
    }
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            shortcuts: Default::default(),
            language: crate::i18n::Language::En,
            prompt_language: None,
            categories: true,
            category_labels: false,
            title_button_labels: false,
            categories_locked: false,
            properties: true,
            category_width: 220.,
            property_width: 300.,
            compact: false,
            text_scale: 1.,
            text_scale_version: 1,
            sidebar_text_scale: 1.,
            file_zoom_step: 10.,
            file_type_icons: false,
            show_count: false,
            drive_name: String::new(),
            icon_scale: 1.,
            category_sizes: true,
            row_separators: true,
            striped_rows: false,
            drive_model: true,
            single_click_expand: true,
            reuse_scan: true,
            gentle_scan: false,
            save_scan: true,
            auto_refresh: false,
            animations: true,
            motion_speed: 1.,
            preview_mode: PreviewMode::Overview,
            open_files_on_double_click: true,
            home_path: default_home().display().to_string(),
            search_home_fallback: true,
            all_files_auto_refresh: false,
            all_files_read_only: false,
            all_files_auto_save: false,
        }
    }
}
impl Layout {
    pub fn effective_prompt_language(&self) -> crate::i18n::Language {
        self.prompt_language.unwrap_or(self.language)
    }

    pub fn migrate_text_scale(&mut self) {
        if self.text_scale_version == 0 {
            self.text_scale /= 0.85;
            self.text_scale_version = 1;
        }
    }
    pub fn effective_text_scale(&self) -> f32 {
        self.text_scale
            * if self.text_scale_version == 0 {
                1.
            } else {
                0.85
            }
    }
    pub fn tree_text_scale(&self) -> f32 {
        self.effective_text_scale()
            * if self.preview_mode == PreviewMode::Focus {
                self.sidebar_text_scale
            } else {
                1.
            }
    }
    pub fn tree_icon_scale(&self) -> f32 {
        self.icon_scale
            * if self.preview_mode == PreviewMode::Focus {
                self.sidebar_text_scale
            } else {
                1.
            }
    }
    pub fn tree_indent(depth: usize, name_width: f32) -> f32 {
        (depth as f32 * 8.).min((name_width - 90.).clamp(0., 80.))
    }
    pub fn tree_row_height(&self) -> f32 {
        if self.preview_mode == PreviewMode::Focus {
            ((if self.compact { 26. } else { 32. }) * self.tree_text_scale())
                .max(18. * self.tree_icon_scale() + 6.)
                .max(22.)
        } else {
            ((if self.compact { 28. } else { 34. }) * self.effective_text_scale())
                .max(23. * self.icon_scale + 6.)
        }
    }
    pub fn home_directory(&self) -> PathBuf {
        let path = PathBuf::from(&self.home_path);
        if path.is_absolute() && path.is_dir() {
            path
        } else {
            default_home()
        }
    }
    pub fn normalize_file_mode(&mut self) {
        // "off" remains readable in old TOML files, but is no longer a third mode.
        if self.preview_mode == PreviewMode::Off {
            self.preview_mode = PreviewMode::Overview;
        }
        self.properties = self.preview_mode == PreviewMode::Overview;
    }
    pub const MIN_PROPERTY_WIDTH: f32 = 240.;
    pub const MAX_PROPERTY_WIDTH: f32 = 760.;
    pub fn validate(&self) -> Result<(), String> {
        self.shortcuts.validate()?;
        let home = Path::new(&self.home_path);
        if !home.is_absolute()
            || self.home_path.len() > 4096
            || self.home_path.chars().any(char::is_control)
            || home
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(crate::tr!(
                "Для Home потрібен абсолютний шлях без ..",
                "Home requires an absolute path without .."
            )
            .into());
        }
        if !self.category_width.is_finite()
            || !(155.0..=280.0).contains(&self.category_width)
            || !self.property_width.is_finite()
            || !(Self::MIN_PROPERTY_WIDTH..=Self::MAX_PROPERTY_WIDTH).contains(&self.property_width)
            || !self.text_scale.is_finite()
            || self.text_scale_version > 1
            || !(if self.text_scale_version == 0 {
                0.85..=1.4
            } else {
                1.0..=1.65
            })
            .contains(&self.text_scale)
            || !self.sidebar_text_scale.is_finite()
            || !(0.8..=1.2).contains(&self.sidebar_text_scale)
            || !self.file_zoom_step.is_finite()
            || !(0.1..=1000.).contains(&self.file_zoom_step)
            || !self.icon_scale.is_finite()
            || !(0.8..=1.4).contains(&self.icon_scale)
            || !self.motion_speed.is_finite()
            || !(0.5..=2.).contains(&self.motion_speed)
            || self.drive_name.chars().count() > 64
        {
            return Err(crate::tr!(
                "Неприпустимий розмір панелей, масштаб, швидкість анімацій або назва диска",
                "Invalid panel size, scale, animation speed, or drive name"
            )
            .into());
        }
        Ok(())
    }
}
pub fn parse_color(s: &str) -> Result<Color32, String> {
    if s.len() != 7 || !s.starts_with('#') || !s[1..].bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(crate::tr!(
            "Неправильний колір {s}: потрібен #RRGGBB",
            "Invalid color {s}: expected #RRGGBB",
            s = s
        ));
    }
    let x = u32::from_str_radix(&s[1..], 16).map_err(|e| e.to_string())?;
    Ok(Color32::from_rgb((x >> 16) as u8, (x >> 8) as u8, x as u8))
}
pub fn color(s: &str) -> Color32 {
    parse_color(s).unwrap_or(Color32::WHITE)
}
pub fn contrast(a: Color32, b: Color32) -> f32 {
    fn lum(c: Color32) -> f32 {
        let linear = |x: u8| {
            let x = x as f32 / 255.;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }
    let a = lum(a);
    let b = lum(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
pub fn config_dir() -> PathBuf {
    std::env::var_os("DISCO_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("disco")))
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config/disco")
        })
}
pub fn read_theme(p: &Path) -> Result<Theme, String> {
    let x: Theme = toml::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    x.validate()?;
    Ok(x)
}
pub fn read_layout(p: &Path) -> Result<Layout, String> {
    let mut x: Layout = toml::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    x.validate()?;
    x.migrate_text_scale();
    x.validate()?;
    Ok(x)
}
pub fn load() -> (Theme, Layout, String) {
    let p = config_dir();
    let mut errors = vec![];
    let t = if p.join("themes.toml").exists() {
        crate::themes::load_active(&p).unwrap_or_else(|error| {
            errors.push(format!("themes.toml: {error}"));
            read_theme(&p.join("theme.toml")).unwrap_or_default()
        })
    } else if p.join("theme.toml").exists() {
        read_theme(&p.join("theme.toml")).unwrap_or_else(|e| {
            errors.push(format!("theme.toml: {e}"));
            Theme::default()
        })
    } else {
        Theme::default()
    };
    let l = if p.join("layout.toml").exists() {
        read_layout(&p.join("layout.toml")).unwrap_or_else(|e| {
            errors.push(format!("layout.toml: {e}"));
            Layout::default()
        })
    } else {
        Layout::default()
    };
    (t, l, errors.join(" · "))
}
pub fn write<T: Serialize>(p: &Path, value: &T) -> Result<(), String> {
    use std::io::Write;
    let text = toml::to_string_pretty(value).map_err(|e| e.to_string())?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension(format!("{}.tmp", std::process::id()));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, p)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e: std::io::Error| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prompt_language_defaults_to_ui_and_roundtrips_independently() {
        use crate::i18n::Language::{En, Uk};
        for ui in [Uk, En] {
            let mut layout: Layout = toml::from_str(&format!(
                "language='{}'",
                if ui == Uk { "uk" } else { "en" }
            ))
            .unwrap();
            assert!(layout.prompt_language.is_none());
            assert_eq!(layout.effective_prompt_language(), ui);
            for prompt in [Uk, En] {
                layout.prompt_language = Some(prompt);
                let saved = toml::to_string(&layout).unwrap();
                let mut restored: Layout = toml::from_str(&saved).unwrap();
                assert_eq!(restored.language, ui);
                assert_eq!(restored.effective_prompt_language(), prompt);
                restored.language = if ui == Uk { En } else { Uk };
                assert_eq!(restored.effective_prompt_language(), prompt);
                restored.prompt_language = None;
                assert_eq!(restored.effective_prompt_language(), restored.language);
            }
        }
    }

    #[test]
    fn text_scale_migration_preserves_physical_size_once_and_sidebar_is_independent() {
        for scale in [0.85, 1., 1.15, 1.4] {
            let mut old: Layout = toml::from_str(&format!("text_scale = {scale}\n")).unwrap();
            assert_eq!(old.text_scale_version, 0);
            old.migrate_text_scale();
            old.validate().unwrap();
            assert!((old.effective_text_scale() - scale).abs() < 0.00001);
            let mut again: Layout = toml::from_str(&toml::to_string(&old).unwrap()).unwrap();
            again.migrate_text_scale();
            assert_eq!(again, old);
        }
        let mut layout = Layout::default();
        assert_eq!(layout.effective_text_scale(), 0.85);
        layout.preview_mode = PreviewMode::Focus;
        layout.sidebar_text_scale = 0.8;
        assert!((layout.tree_text_scale() - 0.68).abs() < 0.00001);
        assert_eq!(layout.effective_text_scale(), 0.85);
        assert_eq!(Layout::tree_indent(3, 158.), 24.);
        assert!(Layout::tree_indent(100, 158.) < 70.);
        assert!(layout.tree_row_height() >= 22.);
    }
    #[test]
    fn presets_contrast_and_roundtrip() {
        for name in [
            "Carbon", "Ember", "Noir", "Midnight", "Forest", "Paper", "Cairo", "Glacier",
        ] {
            let t = Theme::preset(name);
            t.validate().unwrap();
            let text = toml::to_string(&t).unwrap();
            assert_eq!(toml::from_str::<Theme>(&text).unwrap(), t);
            assert!(contrast(color(&t.text), color(&t.panel)) >= 4.5);
        }
    }
    #[test]
    fn older_themes_gain_a_valid_trash_color() {
        let serialized = toml::to_string(&Theme::default()).unwrap();
        let old = serialized
            .lines()
            .filter(|line| !line.starts_with("trash ="))
            .collect::<Vec<_>>()
            .join("\n");
        let mut loaded: Theme = toml::from_str(&old).unwrap();
        assert_eq!(loaded.trash, Theme::default().trash);
        assert!(loaded.validate().is_ok());
        loaded.trash = "not-a-color".into();
        assert!(loaded.validate().is_err());
    }
    #[test]
    fn invalid_settings_rejected() {
        assert!(parse_color("#gg0000").is_err());
        let t = Theme {
            text: "#0c0d0f".into(),
            ..Theme::default()
        };
        assert!(t.validate().is_err());
        assert!(
            Layout {
                text_scale: f32::NAN,
                ..Layout::default()
            }
            .validate()
            .is_err()
        );
        assert!(toml::from_str::<Layout>("unexpected=true").is_err());
    }
    #[test]
    fn layout_customization_preserves_older_configs() {
        let old = r#"categories = true
properties = false
category_width = 205.0
property_width = 290.0
compact = true
text_scale = 1.15
show_count = true
drive_name = "My SSD"
"#;
        let mut layout: Layout = toml::from_str(old).unwrap();
        layout.validate().unwrap();
        assert_eq!(layout.language, crate::i18n::Language::Uk);
        assert_eq!(layout.icon_scale, 1.);
        assert!(layout.animations && layout.motion_speed == 1.);
        assert!(layout.single_click_expand && layout.reuse_scan && !layout.gentle_scan);
        assert!(layout.row_separators && layout.category_sizes && layout.drive_model);
        assert!(!layout.striped_rows);
        assert!(layout.compact && !layout.properties);
        layout.icon_scale = 1.4;
        layout.striped_rows = true;
        layout.row_separators = false;
        assert_eq!(
            toml::from_str::<Layout>(&toml::to_string(&layout).unwrap()).unwrap(),
            layout
        );
        for invalid in [f32::NAN, f32::INFINITY, 0., 20.] {
            layout.icon_scale = invalid;
            assert!(layout.validate().is_err());
        }
    }
    #[test]
    fn preview_preferences_roundtrip_and_older_configs_still_load() {
        let old: Layout = toml::from_str("categories=true").unwrap();
        assert_eq!(old.preview_mode, PreviewMode::Overview);
        assert!(!old.category_labels && old.properties);
        assert!(old.open_files_on_double_click);
        for (old_name, mode, current_name) in [
            ("inspector", PreviewMode::Overview, "overview"),
            ("workspace", PreviewMode::Focus, "focus"),
        ] {
            let restored: Layout = toml::from_str(&format!("preview_mode='{old_name}'")).unwrap();
            assert_eq!(restored.preview_mode, mode);
            assert!(
                toml::to_string(&restored)
                    .unwrap()
                    .contains(&format!("preview_mode = \"{current_name}\""))
            );
        }
        for mode in [PreviewMode::Off, PreviewMode::Overview, PreviewMode::Focus] {
            let layout = Layout {
                preview_mode: mode,
                open_files_on_double_click: false,
                ..old.clone()
            };
            let restored: Layout = toml::from_str(&toml::to_string(&layout).unwrap()).unwrap();
            assert_eq!(restored, layout);
        }
        assert!(toml::from_str::<Layout>("preview_mode='invalid'").is_err());
    }
    #[test]
    fn language_roundtrips_without_losing_other_preferences() {
        let mut layout = Layout::default();
        layout.language = crate::i18n::Language::Uk;
        layout.drive_name = "My SSD".into();
        layout.compact = true;
        let dir = std::env::temp_dir().join(format!("disco-language-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("layout.toml");
        write(&file, &layout).unwrap();
        assert_eq!(read_layout(&file).unwrap(), layout);
        layout.language = crate::i18n::Language::En;
        write(&file, &layout).unwrap();
        assert_eq!(read_layout(&file).unwrap(), layout);
        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("language = \"en\"")
        );
        assert!(toml::from_str::<Layout>("language = \"unsupported\"").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn configuration_is_atomic_and_validated() {
        let p = std::env::temp_dir().join(format!("disco-config-{}", std::process::id()));
        fs::create_dir_all(&p).unwrap();
        let path = p.join("theme.toml");
        write(&path, &Theme::default()).unwrap();
        assert_eq!(read_theme(&path).unwrap(), Theme::default());
        write(&path, &Theme::preset("Noir")).unwrap();
        assert_eq!(read_theme(&path).unwrap(), Theme::preset("Noir"));
        fs::remove_dir_all(p).unwrap();
    }
}
#[test]
fn home_defaults_and_missing_paths_fall_back_without_accepting_relative_paths() {
    let mut layout = Layout::default();
    assert!(layout.home_directory().is_dir());
    assert_eq!(layout.home_directory(), default_home());
    layout.home_path = "/this/disco-home-does-not-exist".into();
    assert_eq!(layout.home_directory(), default_home());
    layout.home_path = "relative/path".into();
    assert!(layout.validate().is_err());
    layout.home_path = "/home/../etc".into();
    assert!(layout.validate().is_err());
    layout.home_path = "/".into();
    assert!(layout.validate().is_ok());
}
