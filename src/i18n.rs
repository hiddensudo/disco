//! Two compiled-in languages. No runtime catalogs or background translation work.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    // Missing language in an older layout keeps its original Ukrainian UI.
    #[default]
    Uk,
    En,
}
impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Self::Uk => "Українська",
            Self::En => "English",
        }
    }
}
static CURRENT: AtomicU8 = AtomicU8::new(0);
pub fn current() -> Language {
    #[cfg(test)]
    if let Some(language) = TEST_LANGUAGE.with(|l| l.get()) {
        return language;
    }
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Language::En
    } else {
        Language::Uk
    }
}
pub fn set_current(language: Language) {
    CURRENT.store(
        if language == Language::En { 1 } else { 0 },
        Ordering::Relaxed,
    );
}

/// Keep both texts together so the Rust compiler checks formatting in both languages.
/// Named formatting values are explicit arguments rather than macro-captured locals.
#[macro_export]
macro_rules! tr {
    ($uk:literal, $en:literal $(,)?) => {{
        match $crate::i18n::current() {
            $crate::i18n::Language::Uk => $uk,
            $crate::i18n::Language::En => $en,
        }
    }};
    ($uk:literal, $en:literal, $($args:tt)+) => {{
        match $crate::i18n::current() {
            $crate::i18n::Language::Uk => format!($uk, $($args)+),
            $crate::i18n::Language::En => format!($en, $($args)+),
        }
    }};
}
#[cfg(test)]
thread_local! {static TEST_LANGUAGE: std::cell::Cell<Option<Language>> = const {std::cell::Cell::new(None)};}
#[cfg(test)]
pub fn with_language<T>(language: Language, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<Language>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_LANGUAGE.with(|l| l.set(self.0));
        }
    }
    let previous = TEST_LANGUAGE.with(|l| l.replace(Some(language)));
    let _reset = Reset(previous);
    f()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_units_plurals_and_named_formats_switch_together() {
        with_language(Language::En, || {
            assert_eq!(crate::tr!("Кошик", "Trash"), "Trash");
            assert_eq!(crate::scan::human(1024u64.pow(3)), "1.0 GiB");
            assert_eq!(crate::scan::human(31), "31 B");
            assert_eq!(crate::plural(21, "file", "files", "files"), "files");
            assert_eq!(crate::plural(1, "file", "files", "files"), "file");
            let name = "layout.toml";
            assert_eq!(
                crate::tr!("Експортувати {name}", "Export {name}", name = name),
                "Export layout.toml"
            );
            assert_eq!(crate::tr!("За {:.1} с", "In {:.1} s", 1.25), "In 1.2 s");
        });
        with_language(Language::Uk, || {
            assert_eq!(crate::tr!("Кошик", "Trash"), "Кошик");
            assert_eq!(crate::scan::human(1024u64.pow(3)), "1.0 ГіБ");
            assert_eq!(crate::plural(21, "файл", "файли", "файлів"), "файл");
            assert_eq!(crate::plural(11, "файл", "файли", "файлів"), "файлів");
        });
    }
    #[test]
    fn formatting_evaluates_arguments_once() {
        let mut calls = 0;
        with_language(Language::En, || {
            let text = crate::tr!("Значення: {}", "Value: {}", {
                calls += 1;
                42
            });
            assert_eq!(text, "Value: 42");
        });
        assert_eq!(calls, 1);
    }
    #[test]
    fn test_language_is_scoped_and_restores_after_a_panic() {
        with_language(Language::En, || {
            let _ = std::panic::catch_unwind(|| with_language(Language::Uk, || panic!("fixture")));
            assert_eq!(current(), Language::En);
        });
    }
}
