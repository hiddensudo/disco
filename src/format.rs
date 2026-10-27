//! Locale-aware presentation helpers shared by the views.
use crate::{i18n, scan::human};
use std::path::Path;
pub(crate) fn label(p: &Path) -> String {
    p.file_name()
        .unwrap_or(p.as_os_str())
        .to_string_lossy()
        .into_owned()
}
pub(crate) fn plural<'a>(n: u64, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    if i18n::current() == i18n::Language::En {
        return if n == 1 { one } else { many };
    }
    if (11..=14).contains(&(n % 100)) {
        many
    } else {
        match n % 10 {
            1 => one,
            2..=4 => few,
            _ => many,
        }
    }
}
pub(crate) fn display_size(n: u64) -> String {
    human(n).replace(".0 ", " ")
}
pub(crate) fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}
pub(crate) fn shorten(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(limit.saturating_sub(1)).collect::<String>()
        )
    }
}
