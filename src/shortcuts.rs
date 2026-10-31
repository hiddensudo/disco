//! Local application bindings. No compositor/global shortcuts are registered.
use eframe::egui::{self, Key, Modifiers};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    NewTab,
    Terminal,
    Focus,
    Overview,
    ResetZoom,
}
impl Command {
    pub const ALL: [Self; 5] = [
        Self::NewTab,
        Self::Terminal,
        Self::Focus,
        Self::Overview,
        Self::ResetZoom,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::NewTab => crate::tr!("Нова вкладка", "New tab"),
            Self::Terminal => crate::tr!("Термінал", "Terminal"),
            Self::Focus => crate::tr!("Фокус", "Focus"),
            Self::Overview => crate::tr!("Огляд", "Overview"),
            Self::ResetZoom => crate::tr!("Скинути масштаб файла", "Reset file zoom"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}
impl Binding {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key: key.name().into(),
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
        }
    }
    pub fn modifiers(&self) -> Modifiers {
        Modifiers {
            ctrl: self.ctrl,
            alt: self.alt,
            shift: self.shift,
            ..Modifiers::NONE
        }
    }
    pub fn label(&self) -> String {
        format!(
            "{}{}{}{}",
            if self.ctrl { "Ctrl+" } else { "" },
            if self.alt { "Alt+" } else { "" },
            if self.shift { "Shift+" } else { "" },
            self.key
        )
    }
    pub fn consume(&self, ctx: &egui::Context) -> bool {
        let Some(key) = Key::from_name(&self.key) else {
            return false;
        };
        ctx.input_mut(|i| {
            let mut matched = false;
            i.events.retain(|event| {
                let consume = matches!(event, egui::Event::Key { key: actual, modifiers, pressed: true, .. }
                    if *actual == key && modifiers.matches_exact(self.modifiers()));
                matched |= consume;
                !consume
            });
            matched
        })
    }
    fn validate(&self) -> Result<(), String> {
        let key = Key::from_name(&self.key)
            .ok_or_else(|| crate::tr!("Невідома клавіша", "Unknown key").to_string())?;
        let function = self
            .key
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=35).contains(&n));
        if !(self.ctrl || self.alt || function) || key == Key::Escape {
            return Err(crate::tr!(
                "Додайте Ctrl або Alt; також доступні функціональні клавіші",
                "Use Ctrl or Alt, or a function key"
            )
            .into());
        }
        let reserved = (self.ctrl
            && !self.alt
            && matches!(
                key,
                Key::S | Key::N | Key::W | Key::L | Key::Tab | Key::Comma
            ))
            || (self.ctrl
                && self.alt
                && matches!(
                    key,
                    Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
                ))
            || (!self.ctrl && !self.alt && matches!(key, Key::F5 | Key::F6))
            || (!self.ctrl && self.alt && key == Key::Home);
        if reserved {
            return Err(crate::tr!(
                "Комбінація зайнята навігацією або редагуванням",
                "This shortcut is reserved for navigation or editing"
            )
            .into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Shortcuts {
    pub new_tab: Binding,
    pub terminal: Binding,
    pub focus: Binding,
    pub overview: Binding,
    pub reset_zoom: Binding,
}
// Missing reset binding in older configs must not invalidate an existing Ctrl+0.
impl<'de> Deserialize<'de> for Shortcuts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Stored {
            new_tab: Binding,
            terminal: Binding,
            focus: Binding,
            overview: Binding,
            reset_zoom: Option<Binding>,
        }
        impl Default for Stored {
            fn default() -> Self {
                let s = Shortcuts::default();
                Self {
                    new_tab: s.new_tab,
                    terminal: s.terminal,
                    focus: s.focus,
                    overview: s.overview,
                    reset_zoom: None,
                }
            }
        }
        let s = Stored::deserialize(d)?;
        let reset = s.reset_zoom.unwrap_or_else(|| {
            [
                Binding::new(Key::Num0, Modifiers::CTRL),
                Binding::new(Key::Num0, Modifiers::CTRL | Modifiers::ALT),
                Binding::new(Key::F12, Modifiers::CTRL | Modifiers::ALT),
                Binding::new(Key::F11, Modifiers::CTRL | Modifiers::ALT),
                Binding::new(Key::F10, Modifiers::CTRL | Modifiers::ALT),
            ]
            .into_iter()
            .find(|b| ![&s.new_tab, &s.terminal, &s.focus, &s.overview].contains(&b))
            .unwrap()
        });
        Ok(Self {
            new_tab: s.new_tab,
            terminal: s.terminal,
            focus: s.focus,
            overview: s.overview,
            reset_zoom: reset,
        })
    }
}
impl Default for Shortcuts {
    fn default() -> Self {
        let mods = Modifiers::CTRL | Modifiers::ALT;
        Self {
            new_tab: Binding::new(Key::T, Modifiers::CTRL),
            terminal: Binding::new(Key::T, mods),
            focus: Binding::new(Key::F, mods),
            overview: Binding::new(Key::V, mods),
            reset_zoom: Binding::new(Key::Num0, Modifiers::CTRL),
        }
    }
}
impl Shortcuts {
    pub fn get(&self, command: Command) -> &Binding {
        match command {
            Command::NewTab => &self.new_tab,
            Command::Terminal => &self.terminal,
            Command::Focus => &self.focus,
            Command::Overview => &self.overview,
            Command::ResetZoom => &self.reset_zoom,
        }
    }
    pub fn assign(&mut self, command: Command, binding: Binding) -> Result<(), String> {
        binding.validate()?;
        if command == Command::ResetZoom
            && binding.ctrl
            && !binding.alt
            && matches!(
                Key::from_name(&binding.key),
                Some(Key::Plus | Key::Equals | Key::Minus)
            )
        {
            return Err(crate::tr!(
                "Комбінація зайнята масштабуванням",
                "This shortcut is reserved for zoom"
            )
            .into());
        }
        if let Some(other) = Command::ALL
            .into_iter()
            .find(|other| *other != command && self.get(*other) == &binding)
        {
            return Err(crate::tr!(
                "Уже призначено: {action}",
                "Already assigned to: {action}",
                action = other.label()
            ));
        }
        match command {
            Command::NewTab => self.new_tab = binding,
            Command::Terminal => self.terminal = binding,
            Command::Focus => self.focus = binding,
            Command::Overview => self.overview = binding,
            Command::ResetZoom => self.reset_zoom = binding,
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut copy = self.clone();
        for command in Command::ALL {
            copy.assign(command, self.get(command).clone())?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bindings_validate_reservations_conflicts_and_roundtrip() {
        let old = "[focus]\nkey='0'\nctrl=true\nalt=false\nshift=false\n";
        let migrated: Shortcuts = toml::from_str(old).unwrap();
        migrated.validate().unwrap();
        assert_eq!(migrated.focus, Binding::new(Key::Num0, Modifiers::CTRL));
        assert_ne!(migrated.reset_zoom, migrated.focus);
        let mut keys = Shortcuts::default();
        keys.validate().unwrap();
        assert!(keys.assign(Command::Focus, keys.terminal.clone()).is_err());
        assert!(
            keys.assign(Command::Focus, Binding::new(Key::S, Modifiers::CTRL))
                .is_err()
        );
        assert!(
            keys.assign(Command::Focus, Binding::new(Key::A, Modifiers::NONE))
                .is_err()
        );
        keys.assign(Command::Focus, Binding::new(Key::F9, Modifiers::NONE))
            .unwrap();
        assert_eq!(
            toml::from_str::<Shortcuts>(&toml::to_string(&keys).unwrap()).unwrap(),
            keys
        );
    }
}
