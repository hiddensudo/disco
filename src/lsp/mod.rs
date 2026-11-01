pub mod catalog;
pub mod client;
pub mod manager;
pub mod protocol;
pub mod state;

/// Opt-in protocol/UI breadcrumbs; never records document text or key contents.
pub(crate) fn trace(message: std::fmt::Arguments<'_>) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ENABLED.get_or_init(|| std::env::var_os("DISCO_LSP_TRACE").is_some()) {
        eprintln!("LSP[{}] {message}", std::process::id());
    }
}
