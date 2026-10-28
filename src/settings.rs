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
    pub fn preset(name: &str) -> Self {
        match name {
            "Ember" => Self {
                name: name.into(),
                accent: "#cf6156".into(),
                trash: "#d7a05c".into(),
                background: "#12100f".into(),
                panel: "#1b1817".into(),
                raised: "#25211f".into(),
                border: "#37302c".into(),
                text: "#eee5dd".into(),
                muted: "#afa49c".into(),
                radius: 3,
            },
            "Noir" => Self {
                name: name.into(),
                accent: "#f07167".into(),
                trash: "#d0b378".into(),
                background: "#090c0e".into(),
                panel: "#12181b".into(),
                raised: "#1c2529".into(),
                border: "#2b363b".into(),
                text: "#e8edef".into(),
                muted: "#9eafb6".into(),
                radius: 3,
            },
            _ => Self::default(),
        }
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
    #[serde(default)]
    pub language: crate::i18n::Language,
    pub categories: bool,
    pub category_labels: bool,
    pub properties: bool,
    pub category_width: f32,
    pub property_width: f32,
    pub compact: bool,
    pub text_scale: f32,
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
impl Default for Layout {
    fn default() -> Self {
        Self {
            language: crate::i18n::Language::En,
            categories: true,
            category_labels: false,
            properties: true,
            category_width: 220.,
            property_width: 300.,
            compact: false,
            text_scale: 1.,
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
            auto_refresh: true,
            animations: true,
            motion_speed: 1.,
            preview_mode: PreviewMode::Overview,
            open_files_on_double_click: true,
        }
    }
}
impl Layout {
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
        if !self.category_width.is_finite()
            || !(155.0..=280.0).contains(&self.category_width)
            || !self.property_width.is_finite()
            || !(Self::MIN_PROPERTY_WIDTH..=Self::MAX_PROPERTY_WIDTH).contains(&self.property_width)
            || !self.text_scale.is_finite()
            || !(0.85..=1.4).contains(&self.text_scale)
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
    let x: Layout = toml::from_str(&fs::read_to_string(p).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    x.validate()?;
    Ok(x)
}
pub fn load() -> (Theme, Layout, String) {
    let p = config_dir();
    let mut errors = vec![];
    let t = if p.join("theme.toml").exists() {
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
    fn presets_contrast_and_roundtrip() {
        for name in ["Carbon", "Ember", "Noir"] {
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
