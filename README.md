# disco

**English** · [Українська](README.uk.md)

A native disk space analyzer for Linux. Built with Rust and egui, with a dark interface and no always-running indexing service.

![Folder tree and properties](docs/screenshots/folders.png)

## Features

- Browse a folder tree, sort by size, search, and filter by category.
- Inspect file and folder properties, find large files, and compare duplicate file contents.
- Preview images and text in the right inspector or in a resizable area beside the list.
- Double-click files to open their default application; this can be disabled.
- Automatically save the last scan and restore it on startup.
- Refresh in the background when disk usage changes.
- Move folders to Trash with confirmation; restore items or empty Trash.
- See Trash usage and file count separately in the drive's storage bar.
- Customize themes, colors, text size, layout, and animations through settings or TOML.
- Switch between English and Ukrainian without restarting.

## Install

Requires **Linux**, **Rust 1.95+** with Cargo, a C compiler (GCC or Clang), Git, and Python 3. The desktop UI needs Wayland or X11 and an OpenGL driver.

```sh
git clone https://github.com/hiddensudo/disco.git
cd disco
cargo build --release --locked
python3 install.py
```

Open **disco** from your application menu or run `~/.local/bin/disco`. To run without installing: `./target/release/disco`.

The installer creates a launcher pointing to your build, so keep the project directory in place. To update, run `git pull` and rebuild. For a custom Cargo output directory, use `python3 install.py --binary /path/to/disco`.

## Use

1. Click the drive name at the top left, choose a folder, and click **Scan**. You can pause or cancel the scan.
2. Click folders to expand the tree; properties appear on the right. Navigation arrows and mouse side buttons go back and forward.
3. Open **Large files**, **Cleanup**, or **Duplicates** from the **···** menu at the bottom left. **Delete folder** is in the properties panel.
4. In **Trash**, select items to restore or permanently delete. Emptying Trash requires a separate confirmation.
5. Open the gear icon to customize the interface. The **Language** selector is at the top of the settings window: choose **English** or **Українська**, then click **Save** to remember your choice. **Cancel** restores your previous preferences.

![Trash: restore items or delete them permanently](docs/screenshots/trash.png)

*Screenshots use demonstration data.*

Trash remains part of used space until it is emptied. Its amber segment is included in used storage, rather than added again; click the Trash label to open it. Change this color under **Appearance → Additional colors**.

Sizes use allocated disk blocks. Symbolic links are not followed, and scans stay on one filesystem. Automatic refresh responds to changes of at least 64 MiB; use **Refresh** for changes that leave total usage unchanged.

Preferences: `~/.config/disco/{theme,layout}.toml`. Last scan: `~/.cache/disco/last-scan.bin.gz`. `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` are supported. In `layout.toml`, `language = "en"` selects English and `language = "uk"` selects Ukrainian. Existing configurations without a language setting keep Ukrainian.

```sh
./target/release/disco --demo          # sample data; no scanning or deletion
./target/release/disco --trash         # open Trash directly
./target/release/disco --scan /path    # scan in the terminal
cargo test --locked
python3 -m unittest discover -s tests
```

## File previews

Expand a folder to load its files, then select a file. Above the list, **Preview → Right panel / Beside list / Off** switches modes and remembers your choice. The larger preview area can be resized by dragging its divider. Images support Fit and zoom; text is read-only, scrollable, and can be copied.

**Settings → Interface → File previews** also controls the mode and **Double click opens files in the default app** (enabled by default). A single click selects and previews. Double-clicking launches only an installed system association; without one, nothing opens. This integration uses `xdg-mime` and `gio` (typically provided by `xdg-utils` and GLib).

Supported images: PNG, JPEG, WebP, BMP, TIFF. Text supports UTF-8 and UTF-16 with a byte-order mark. Only the selected file is read, in the background. To limit resource usage, image previews accept files up to 32 MiB and 12 megapixels and retain a thumbnail up to 2048 px; text shows up to 128 KiB / 2,000 lines. Unsupported or larger content keeps its properties and an explanatory preview message. Expanded-directory listings are cached separately from the saved scan and are bounded; a shortened listing is labeled explicitly. Search includes folder names and already loaded file listings.

In `layout.toml`: `preview_mode = "inspector"` (or `"workspace"`, `"off"`) and `open_files_on_double_click = true` (or `false`).

## Source layout

- `src/main.rs`: command-line entry points and native window startup.
- `src/app.rs` and `src/app/{state,jobs}.rs`: application lifecycle, grouped state, and background job coordination.
- `src/app/ui/`: window chrome, folder browser, inspector, Trash, dialogs, and settings pages.
- `src/index.rs`, `src/filesystem.rs`, `src/format.rs`: scan navigation index, filesystem metadata, and display formatting.
- `src/{file_preview,directory_files,associations}.rs`: bounded content loading, lazy file listings, and default-app integration.
- `src/{scan,snapshot,recycle,deletion}.rs`: scanning, persistence, and Trash operations; independent of UI state.

Inter font: [SIL Open Font License](assets/fonts/LICENSE.txt). Local Trash library patch: [notes and license](vendor/trash/DISCO-PATCH.md).
