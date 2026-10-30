# Development

[← README](../README.md) · [User guide](guide.md)

## Build and check

Use Rust 1.95+ and a Linux desktop with Wayland/X11 and OpenGL.

```sh
cargo build --release --locked
cargo fmt --all --check
cargo test --workspace --locked
python3 -m unittest discover -s tests -v
git diff --check
```

`cargo test --workspace` includes the native terminal widget and its prototype. One large synthetic benchmark is ignored by default. GUI interaction tests use temporary configuration and real PTYs, without writing to the user's settings.

The installer links the selected binary into `~/.local/bin/disco` and creates its desktop entry. Use `python3 install.py --binary /absolute/path/to/disco` for a custom build directory. Keep that binary available after installation.

The installer also registers that desktop entry in `desktop-integration.toml` inside the app's configuration directory and installs `app-icon-dark.svg` and `app-icon-light.svg` there. On startup and after saving a theme, disco selects the matching launcher icon. It preserves unrelated desktop fields and user-selected custom icons. Demo and screenshot runs never modify desktop integration. Separate `DISCO_CONFIG_DIR` installations keep their icon registration separate; the window app ID matches the registered desktop entry (including the unstable launcher).

## Useful commands

Layout text scaling is versioned with `text_scale_version = 1`. The new 100% baseline is the former 85%; legacy files are converted once by dividing `text_scale` by 0.85 and preserve their actual font size. New/reset configurations use 100%. `sidebar_text_scale` (0.8–1.2) affects only the Focus tree; it does not resize the document editor or terminal. Tree indentation is 8 logical pixels and capped by available name width; names are measured and elided before the size column.

```sh
./target/release/disco --version
./target/release/disco --new-tab       # in-memory text tab
./target/release/disco --demo          # sample storage data
./target/release/disco --trash         # open Trash
./target/release/disco --scan /path    # CLI scan
./target/release/disco --scan /path --save-scan
./target/release/disco --snapshot-info
./target/release/disco --syntax-licenses
```

For isolated QA, `DISCO_CONFIG_DIR=/tmp/disco-qa` stores settings and its scan cache there. `--folder /path --preview-file /path/file --screenshot /tmp/disco.png` loads the specified content, captures the native window, and closes it. `--terminal` starts the terminal on launch. Use sample files and an isolated config for public captures; the screenshot option does not hide personal paths or commands.

`DISCO_QA_SCALE=1.25` with `--screenshot` exercises fractional DPI without changing the desktop scale. `--settings appearance`, `--settings layout`, or `--settings categories` captures the corresponding settings page using the embedded QA fallback; normal settings remain a separate native window.

## Source map

| Area | Files |
| --- | --- |
| Startup and lifecycle | `src/main.rs`, `src/app.rs`, `src/app/{state,jobs}.rs` |
| Panels and controls | `src/app/ui/` |
| Navigation and file tabs | `src/app/{navigation,tabs,workspace_keyboard}.rs` |
| Scan and persistence | `src/{scan,snapshot,index,refresh}.rs` |
| Live updates and search | `src/{live_files,file_search}.rs`, `src/app/scope.rs` |
| Preview and editing | `src/{file_preview,text_editor,syntax,directory_files,associations}.rs` |
| Categories | `src/categories.rs`, `src/categories/store.rs` |
| Trash and removal | `src/{recycle,deletion}.rs` |
| Terminal | `src/terminal.rs`, `src/app/terminal.rs`, `vendor/egui_term/` |

## Terminal

One lazily opened PTY uses Alacritty 0.26 and a local MIT-licensed adaptation of egui_term. Rendering is native egui; there is no webview. The same session moves between layouts. Hidden terminal wakeups do not drive redraws; visible notifications are coalesced. Closing destroys the PTY session tree and releases its backend asynchronously; app exit uses synchronous teardown.

System monospace/symbol fonts are loaded through Fontconfig on first open. `NO_COLOR` is removed only in the PTY subprocess, preserving the app's environment. The host answers terminal color and size queries; upstream compatibility is not exhaustive.

- [Widget provenance and local adaptations](../vendor/egui_term/UPSTREAM.md)
- [Widget MIT license](../vendor/egui_term/LICENSE)
- [Prototype and validation](../prototypes/terminal/README.md)
- [Paired memory measurements](../prototypes/terminal/MEASUREMENTS.md) — prototype measurements, not the total app with a large scan or a running agent.

## Rendering regression in 0.2.2

The old native capture had five rows below the custom title bar with alpha 180. Panel frames and the whole-window background now paint opaquely, the renderer clear alpha is 255, and native transparency is explicitly disabled. The new native capture has alpha 255 throughout.

A window-level regression covers Overview/Focus transitions, resizing and 100%, 125%, 150%, 200% scale. PTY interaction tests cover panel resizing, session continuity, teardown, keyboard isolation and terminal focus routes.

## Component credits

- App artwork: `assets/disco.svg` is a monochrome line mirrorball projected from a tilted sphere. `disco-black.svg`, `disco-dark.svg` and `disco-light.svg` provide its other monochrome variants. Every in-app logo is transparent; its white/black lines maximize contrast against the actual surface color. Launcher/window icons have a rounded dark/light theme tile with transparent corners because the surrounding desktop color is unknown; window icons are rendered directly from the same rounded SVG variants. Both READMEs select a transparent line variant for GitHub's light/dark color scheme. All artwork is vector; no bitmap is embedded. The app SVG is parsed once and its physical-size textures are cached per UI context.
- The logo uses tiny-skia edge antialiasing directly at the physical target size, without the extra supersampling/downsampling used for other UI artwork. Its texture is aligned to physical pixels and sampled without interpolation to avoid softening the mirrors again.

- Inter: [SIL Open Font License](../assets/fonts/LICENSE.txt).
- Syntax highlighting: syntect with bat/two-face grammars; [grammar licenses](../licenses/syntaxes.md), also in About and `--syntax-licenses`.
- Terminal widget: [MIT](../vendor/egui_term/LICENSE); engine and dependencies retain their upstream licenses.
- Local Trash library patch: [notes and license](../vendor/trash/DISCO-PATCH.md).

Only source, documentation and sample screenshots belong in Git. Local binaries, config, scan caches and private fixtures stay outside release commits. Ask the author for the exact version number before bumping or tagging; preserve existing tags.

## Testing the 0.2.3 preview

Keep the stable launcher separate. Build this branch and run `DISCO_CONFIG_DIR=/tmp/disco-unstable ./target/release/disco` to try it with isolated settings. The preview version is `0.2.3-unstable`; main and stable tags remain unchanged during preparation.
