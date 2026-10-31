//! Bundled Material Icon Theme symbols; see licenses/material-icon-theme-MIT.txt.
use std::path::Path;
pub const SOURCES: &[(&str, &[u8])] = &[
    ("c", include_bytes!("../assets/file-icons/c.svg")),
    ("cpp", include_bytes!("../assets/file-icons/cpp.svg")),
    ("rust", include_bytes!("../assets/file-icons/rust.svg")),
    ("json", include_bytes!("../assets/file-icons/json.svg")),
    ("yaml", include_bytes!("../assets/file-icons/yaml.svg")),
    ("toml", include_bytes!("../assets/file-icons/toml.svg")),
    (
        "typescript",
        include_bytes!("../assets/file-icons/typescript.svg"),
    ),
    (
        "javascript",
        include_bytes!("../assets/file-icons/javascript.svg"),
    ),
    ("java", include_bytes!("../assets/file-icons/java.svg")),
    ("python", include_bytes!("../assets/file-icons/python.svg")),
    ("go", include_bytes!("../assets/file-icons/go.svg")),
    ("csharp", include_bytes!("../assets/file-icons/csharp.svg")),
    ("kotlin", include_bytes!("../assets/file-icons/kotlin.svg")),
    ("swift", include_bytes!("../assets/file-icons/swift.svg")),
    ("ruby", include_bytes!("../assets/file-icons/ruby.svg")),
    ("php", include_bytes!("../assets/file-icons/php.svg")),
    ("html", include_bytes!("../assets/file-icons/html.svg")),
    ("css", include_bytes!("../assets/file-icons/css.svg")),
    ("sass", include_bytes!("../assets/file-icons/sass.svg")),
    (
        "console",
        include_bytes!("../assets/file-icons/console.svg"),
    ),
    (
        "powershell",
        include_bytes!("../assets/file-icons/powershell.svg"),
    ),
    (
        "database",
        include_bytes!("../assets/file-icons/database.svg"),
    ),
    (
        "markdown",
        include_bytes!("../assets/file-icons/markdown.svg"),
    ),
    ("docker", include_bytes!("../assets/file-icons/docker.svg")),
    (
        "makefile",
        include_bytes!("../assets/file-icons/makefile.svg"),
    ),
    ("xml", include_bytes!("../assets/file-icons/xml.svg")),
    ("vue", include_bytes!("../assets/file-icons/vue.svg")),
    ("react", include_bytes!("../assets/file-icons/react.svg")),
    (
        "react_ts",
        include_bytes!("../assets/file-icons/react_ts.svg"),
    ),
    ("svelte", include_bytes!("../assets/file-icons/svelte.svg")),
    ("zig", include_bytes!("../assets/file-icons/zig.svg")),
    ("lua", include_bytes!("../assets/file-icons/lua.svg")),
    ("perl", include_bytes!("../assets/file-icons/perl.svg")),
    ("r", include_bytes!("../assets/file-icons/r.svg")),
    ("dart", include_bytes!("../assets/file-icons/dart.svg")),
    ("elixir", include_bytes!("../assets/file-icons/elixir.svg")),
    (
        "clojure",
        include_bytes!("../assets/file-icons/clojure.svg"),
    ),
    (
        "haskell",
        include_bytes!("../assets/file-icons/haskell.svg"),
    ),
    ("scala", include_bytes!("../assets/file-icons/scala.svg")),
    ("fsharp", include_bytes!("../assets/file-icons/fsharp.svg")),
    (
        "objective-c",
        include_bytes!("../assets/file-icons/objective-c.svg"),
    ),
    ("cmake", include_bytes!("../assets/file-icons/cmake.svg")),
    ("git", include_bytes!("../assets/file-icons/git.svg")),
    ("less", include_bytes!("../assets/file-icons/less.svg")),
    (
        "graphql",
        include_bytes!("../assets/file-icons/graphql.svg"),
    ),
    (
        "settings",
        include_bytes!("../assets/file-icons/settings.svg"),
    ),
    ("julia", include_bytes!("../assets/file-icons/julia.svg")),
    ("erlang", include_bytes!("../assets/file-icons/erlang.svg")),
    ("ocaml", include_bytes!("../assets/file-icons/ocaml.svg")),
    ("groovy", include_bytes!("../assets/file-icons/groovy.svg")),
    (
        "assembly",
        include_bytes!("../assets/file-icons/assembly.svg"),
    ),
    ("typst", include_bytes!("../assets/file-icons/typst.svg")),
    ("nix", include_bytes!("../assets/file-icons/nix.svg")),
    (
        "terraform",
        include_bytes!("../assets/file-icons/terraform.svg"),
    ),
    (
        "solidity",
        include_bytes!("../assets/file-icons/solidity.svg"),
    ),
    (
        "verilog",
        include_bytes!("../assets/file-icons/verilog.svg"),
    ),
    (
        "fortran",
        include_bytes!("../assets/file-icons/fortran.svg"),
    ),
    ("pascal", include_bytes!("../assets/file-icons/pascal.svg")),
    ("lisp", include_bytes!("../assets/file-icons/lisp.svg")),
];

fn kind(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" | "containerfile" => return Some("docker"),
        "makefile" | "gnumakefile" => return Some("makefile"),
        "cmakelists.txt" => return Some("cmake"),
        "cargo.lock" | "cargo.toml" => return Some("toml"),
        ".gitignore" | ".gitattributes" | ".gitmodules" => return Some("git"),
        ".bashrc" | ".zshrc" | ".bash_profile" | ".profile" => return Some("console"),
        _ => {}
    }
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "c++" => "cpp",
            "rs" => "rust",
            "json" | "jsonc" | "json5" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "ts" | "mts" | "cts" => "typescript",
            "tsx" => "react_ts",
            "js" | "mjs" | "cjs" => "javascript",
            "jsx" => "react",
            "java" | "class" | "jar" => "java",
            "py" | "pyi" | "pyw" => "python",
            "go" => "go",
            "cs" | "csx" => "csharp",
            "kt" | "kts" => "kotlin",
            "swift" => "swift",
            "rb" | "rake" | "gemspec" => "ruby",
            "php" | "phtml" => "php",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" | "sass" => "sass",
            "less" => "less",
            "sh" | "bash" | "zsh" | "fish" => "console",
            "ps1" | "psm1" | "psd1" => "powershell",
            "sql" | "sqlite" | "db" => "database",
            "md" | "markdown" | "mdx" => "markdown",
            "dockerfile" => "docker",
            "mk" | "mak" => "makefile",
            "cmake" => "cmake",
            "xml" | "xsl" | "xsd" => "xml",
            "vue" => "vue",
            "svelte" => "svelte",
            "zig" => "zig",
            "lua" => "lua",
            "pl" | "pm" => "perl",
            "r" | "rmd" => "r",
            "dart" => "dart",
            "ex" | "exs" => "elixir",
            "clj" | "cljs" | "cljc" | "edn" => "clojure",
            "hs" | "lhs" => "haskell",
            "scala" | "sc" => "scala",
            "fs" | "fsx" | "fsi" => "fsharp",
            "m" | "mm" => "objective-c",
            "graphql" | "gql" => "graphql",
            "ini" | "cfg" | "conf" | "config" | "editorconfig" => "settings",
            "jl" => "julia",
            "erl" | "hrl" => "erlang",
            "ml" | "mli" => "ocaml",
            "groovy" | "gradle" => "groovy",
            "asm" | "s" => "assembly",
            "typ" => "typst",
            "nix" => "nix",
            "tf" | "tfvars" => "terraform",
            "sol" => "solidity",
            "v" | "sv" | "svh" | "vh" => "verilog",
            "f" | "f90" | "f95" | "for" => "fortran",
            "pas" | "pp" => "pascal",
            "lisp" | "lsp" | "cl" | "el" => "lisp",
            _ => return None,
        },
    )
}
pub fn icon(path: &Path) -> Option<usize> {
    let kind = kind(path)?;
    SOURCES
        .iter()
        .position(|(name, _)| *name == kind)
        .map(|index| crate::art::UI_ICONS + index)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn programming_and_config_types_map_case_insensitively_with_safe_fallback() {
        for (path, name) in [
            ("hello.C", "c"),
            ("hello.hpp", "cpp"),
            ("main.rs", "rust"),
            ("settings.JSON", "json"),
            ("config.yml", "yaml"),
            ("Cargo.toml", "toml"),
            ("app.tsx", "react_ts"),
            ("index.ts", "typescript"),
            ("index.js", "javascript"),
            ("main.java", "java"),
            ("Dockerfile", "docker"),
            ("Makefile", "makefile"),
            (".gitignore", "git"),
        ] {
            let index = icon(Path::new(path)).unwrap() - crate::art::UI_ICONS;
            assert_eq!(SOURCES[index].0, name);
        }
        assert!(icon(Path::new("unknown.disco-unknown")).is_none());
        assert!(icon(Path::new("photo.PNG")).is_none());
    }
}
