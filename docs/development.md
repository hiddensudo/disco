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

## Useful commands

```sh
./target/release/disco --version
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

- Inter: [SIL Open Font License](../assets/fonts/LICENSE.txt).
- Syntax highlighting: syntect with bat/two-face grammars; [grammar licenses](../licenses/syntaxes.md), also in About and `--syntax-licenses`.
- Terminal widget: [MIT](../vendor/egui_term/LICENSE); engine and dependencies retain their upstream licenses.
- Local Trash library patch: [notes and license](../vendor/trash/DISCO-PATCH.md).

Only source, documentation and sample screenshots belong in Git. Local binaries, config, scan caches and private fixtures stay outside release commits. Ask the author for the exact version number before bumping or tagging; preserve existing tags.
