# disco terminal prototype

[Українською](README.uk.md)

A separate native experiment, built with the same egui 0.37 UI framework as disco. It does not change the installed disco application, settings, scan cache, or release version. The package version matches the parent project; this is not a release.

```sh
./run.sh --folder /path/to/project
```

`run.sh` uses the local prototype executable when available, otherwise builds with Cargo. Build explicitly with `cargo build --manifest-path Cargo.toml --locked`.

- Right and bottom panels are independent real PTY sessions. The bottom session starts only when shown. Hiding a panel keeps its shell alive; **New** ends that session and starts a replacement.
- Uses `$SHELL`, then the account's login shell, then `/bin/sh`. Override using `--shell /bin/bash`.
- New sessions start in the folder shown in the toolbar. Existing sessions retain their working directory when browsing files.
- Drag panel separators to resize. A−/A+ change terminal text size.
- Click a terminal to focus it; keyboard input remains there when moving the pointer away. Ctrl+C interrupts; Ctrl+Shift+C/V copy/paste; Ctrl+backtick toggles the bottom panel. Header buttons also expose copy/paste.
- `--en` selects English; Ukrainian is the default.
- The file browser and text preview are a small read-only test harness, not the full disco workspace.

## Performance and compatibility

PTY events and user input wake the window. There is no perpetual repaint/poll loop when idle. The UI p95 counter measures CPU time constructing recent UI frames, not GPU time, display refresh rate, or full input latency. PTY startup measures spawn to the first visible nonblank output, including shell configuration startup.

The included real-PTY tests exercise keyboard input while the pointer is outside the terminal, Unicode, paste requests, alternate screen, independent sessions, resize, Ctrl+C, child shutdown, and a 5,000-line scrollback workload. Run:

```sh
cargo test --workspace --locked -- --nocapture
```

This prototype must still be checked interactively with the particular TUI applications you use. It does not claim complete terminal compatibility. Moving a panel does not move a session between positions yet; right and bottom are separate sessions. Closing the window or choosing New ends its sessions. OSC 52 clipboard integration and persistent terminal sessions across app restarts are outside this experiment.

## Dependencies

Native rendering through eframe/egui, terminal parser/state/PTY through Alacritty, and a locally adapted MIT-licensed [egui_term](https://github.com/kemokempo/egui_term) widget. Attribution, exact upstream revision and local changes are in [../../vendor/egui_term/UPSTREAM.md](../../vendor/egui_term/UPSTREAM.md). No WebView or JavaScript runtime is embedded.

[Measured results](MEASUREMENTS.md).

Color/font handling: launcher-inherited NO_COLOR is cleared before workers start; system monospace and supplementary glyph fonts are loaded through Fontconfig (fc-match), with bundled fallback fonts. This affects only this prototype process and its children.
