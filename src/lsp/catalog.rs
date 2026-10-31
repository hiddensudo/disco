use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub servers: BTreeMap<String, Server>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub name: String,
    pub extensions: Vec<String>,
    pub languages: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub root_markers: Vec<String>,
    pub provider: String,
    pub enabled: bool,
    pub repository: String,
    pub version: String,
    pub asset: String,
    pub sha256: String,
    pub platform: String,
    pub package: String,
    pub integrity: String,
    pub extra_packages: Vec<String>,
}
impl Default for Server {
    fn default() -> Self {
        Self {
            name: String::new(),
            extensions: vec![],
            languages: vec![],
            command: String::new(),
            args: vec![],
            root_markers: vec![],
            provider: "system".into(),
            enabled: false,
            repository: String::new(),
            version: String::new(),
            asset: String::new(),
            sha256: String::new(),
            platform: String::new(),
            package: String::new(),
            integrity: String::new(),
            extra_packages: vec![],
        }
    }
}
pub fn segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && !matches!(s, "." | "..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
impl Catalog {
    pub fn builtin() -> Self {
        toml::from_str(include_str!("../../assets/lsp.toml")).expect("bundled LSP catalog")
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.servers.len() > 32 {
            return Err("Unsupported LSP catalog".into());
        }
        for (id, s) in &self.servers {
            if !segment(id)
                || s.command.is_empty()
                || s.command.len() > 1024
                || s.command.contains('\0')
                || s.extensions.is_empty()
                || s.extensions.len() != s.languages.len()
                || s.args.len() > 32
                || s.args.iter().any(|a| a.len() > 4096 || a.contains('\0'))
                || s.root_markers.iter().any(|m| !segment(m))
                || s.extensions.iter().any(|e| !segment(e))
                || !["system", "rustup", "github_release", "npm"].contains(&s.provider.as_str())
            {
                return Err(format!("Invalid LSP server: {id}"));
            }
            if matches!(s.provider.as_str(), "github_release" | "npm")
                && (!segment(&s.version) || !segment(&s.command))
            {
                return Err(format!("Invalid managed version/command: {id}"));
            }
            if s.provider == "github_release"
                && (s.repository.split('/').count() != 2
                    || !s.repository.split('/').all(segment)
                    || !segment(&s.asset)
                    || !valid_sha(&s.sha256))
            {
                return Err(format!("Invalid GitHub artifact: {id}"));
            }
            if s.provider == "npm"
                && (!npm_name(&s.package)
                    || !s.integrity.starts_with("sha512-")
                    || s.extra_packages.iter().any(|p| {
                        p.rsplit_once('@')
                            .is_none_or(|(n, v)| !npm_name(n) || !segment(v))
                    }))
            {
                return Err(format!("Invalid npm package: {id}"));
            }
        }
        Ok(())
    }
}
pub fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn npm_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 160
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@/._-".contains(&b))
        && !s.contains("..")
        && !s.starts_with('-')
}
pub fn executable(command: &Path) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let valid = |path: &Path| {
        path.metadata()
            .ok()
            .is_some_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if command.is_absolute() {
        return valid(command).then(|| command.into());
    }
    if command.components().count() != 1 {
        return None;
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| p.is_absolute())
        .map(|p| p.join(command))
        .find(|p| valid(p))
}
pub fn project_root(path: &Path, markers: &[String]) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("/"));
    for dir in parent.ancestors() {
        if markers.iter().any(|m| dir.join(m).exists()) {
            return dir.into();
        }
    }
    parent.into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_defaults_are_opt_in_and_managed_paths_are_validated() {
        let mut c = Catalog::builtin();
        c.validate().unwrap();
        assert!(c.servers.values().all(|s| !s.enabled));
        c.servers.get_mut("rust-analyzer").unwrap().version = "../../bad".into();
        assert!(c.validate().is_err());
    }
}
