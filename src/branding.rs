use crate::settings;
use eframe::egui::Color32;
use serde::Deserialize;
use std::{fs, path::Path};

pub const DARK_ICON: &str = "app-icon-dark.svg";
pub const LIGHT_ICON: &str = "app-icon-light.svg";
const DARK_SVG: &[u8] = include_bytes!("../assets/disco-dark.svg");
const LIGHT_SVG: &[u8] = include_bytes!("../assets/disco-light.svg");

pub fn dark(background: Color32) -> bool {
    settings::contrast(Color32::WHITE, background) >= settings::contrast(Color32::BLACK, background)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub ink: Color32,
    pub background: Option<Color32>,
}

impl Appearance {
    pub fn for_surface(surface: Color32) -> Self {
        // Keep the in-app mark transparent on every surface, including colors.
        Self {
            ink: if dark(surface) {
                Color32::WHITE
            } else {
                Color32::BLACK
            },
            background: None,
        }
    }

    pub fn for_icon(theme_background: Color32) -> Self {
        // A launcher/window icon can be drawn over a panel or wallpaper whose
        // color the app cannot inspect. An opaque theme tile keeps it legible.
        if dark(theme_background) {
            Self {
                ink: Color32::WHITE,
                background: Some(Color32::BLACK),
            }
        } else {
            Self {
                ink: Color32::BLACK,
                background: Some(Color32::WHITE),
            }
        }
    }
}

#[derive(Deserialize)]
struct Installation {
    entry: std::path::PathBuf,
}

pub fn desktop_app_id(config: &Path) -> String {
    fs::read_to_string(config.join("desktop-integration.toml"))
        .ok()
        .and_then(|text| toml::from_str::<Installation>(&text).ok())
        .and_then(|installation| {
            installation
                .entry
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "ua.disco".into())
}

fn desktop_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn write_changed(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if fs::read(path).is_ok_and(|old| old == contents) {
        return Ok(());
    }
    let temporary = path.with_file_name(format!(
        ".{}.disco-next",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&temporary, contents)?;
    fs::rename(temporary, path)
}

/// Update only the desktop entry registered by the installer for this config.
/// Demo/screenshot callers never invoke this. User-selected custom icons stay put.
pub fn sync_installed_icon(config: &Path, background: Color32) -> Result<(), String> {
    let registration = match fs::read_to_string(config.join("desktop-integration.toml")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let installation: Installation = toml::from_str(&registration).map_err(|e| e.to_string())?;
    if !installation.entry.is_absolute() {
        return Err("Desktop entry path must be absolute".into());
    }
    let original = fs::read_to_string(&installation.entry).map_err(|e| e.to_string())?;
    let config = config.canonicalize().map_err(|e| e.to_string())?;
    let dark_path = config.join(DARK_ICON);
    let light_path = config.join(LIGHT_ICON);
    let current = original.lines().find_map(|line| line.strip_prefix("Icon="));
    if !current.is_some_and(|value| {
        value == desktop_path(&dark_path) || value == desktop_path(&light_path)
    }) {
        return Ok(());
    }
    write_changed(&dark_path, DARK_SVG).map_err(|e| e.to_string())?;
    write_changed(&light_path, LIGHT_SVG).map_err(|e| e.to_string())?;
    let selected = if dark(background) {
        dark_path
    } else {
        light_path
    };
    let mut updated = String::new();
    for line in original.split_inclusive('\n') {
        if line.starts_with("Icon=") {
            updated.push_str(&format!("Icon={}", desktop_path(&selected)));
            if line.ends_with("\r\n") {
                updated.push_str("\r\n");
            } else if line.ends_with('\n') {
                updated.push('\n');
            }
        } else {
            updated.push_str(line);
        }
    }
    write_changed(&installation.entry, updated.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_and_colored_surfaces_use_transparent_contrasting_lines() {
        for surface in [
            Color32::BLACK,
            Color32::from_rgb(16, 20, 23),
            Color32::from_rgb(14, 56, 92),
        ] {
            assert_eq!(
                Appearance::for_surface(surface),
                Appearance {
                    ink: Color32::WHITE,
                    background: None
                }
            );
        }
        for surface in [
            Color32::WHITE,
            Color32::from_rgb(242, 244, 247),
            Color32::from_rgb(240, 174, 54),
            Color32::from_rgb(128, 128, 128),
        ] {
            assert_eq!(
                Appearance::for_surface(surface),
                Appearance {
                    ink: Color32::BLACK,
                    background: None
                }
            );
        }
        assert_eq!(
            Appearance::for_icon(Color32::BLACK).background,
            Some(Color32::BLACK)
        );
        assert_eq!(
            Appearance::for_icon(Color32::WHITE).background,
            Some(Color32::WHITE)
        );
    }

    #[test]
    fn desktop_switch_preserves_entry_fields_and_respects_custom_icons() {
        let root = std::env::temp_dir().join(format!("disco-branding-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let entry = root.join("disco.desktop");
        fs::write(
            root.join("desktop-integration.toml"),
            format!("entry = {:?}\n", entry.to_str().unwrap()),
        )
        .unwrap();
        fs::write(
            &entry,
            format!(
                "[Desktop Entry]\nName=disco\nExec=keep-me\nIcon={}\n",
                root.join(DARK_ICON).display()
            ),
        )
        .unwrap();
        sync_installed_icon(&root, Color32::WHITE).unwrap();
        let changed = fs::read_to_string(&entry).unwrap();
        assert!(changed.contains("Exec=keep-me\n"));
        assert!(changed.contains(&format!("Icon={}\n", root.join(LIGHT_ICON).display())));
        assert_eq!(fs::read(root.join(LIGHT_ICON)).unwrap(), LIGHT_SVG);
        sync_installed_icon(&root, Color32::BLACK).unwrap();
        assert!(
            fs::read_to_string(&entry)
                .unwrap()
                .contains(&root.join(DARK_ICON).to_string_lossy().to_string())
        );
        fs::write(&entry, "[Desktop Entry]\nIcon=my-custom-icon\n").unwrap();
        sync_installed_icon(&root, Color32::WHITE).unwrap();
        assert_eq!(
            fs::read_to_string(&entry).unwrap(),
            "[Desktop Entry]\nIcon=my-custom-icon\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
