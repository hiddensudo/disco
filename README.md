# disco

**English** · [Українська](README.uk.md)

A native disk space analyzer for Linux. Built with Rust and egui, with a dark interface and no always-running indexing service.

![Folder tree and properties](docs/screenshots/folders.png)

## Features

- Browse a folder tree, sort by size, search, and filter by category.
- Inspect file and folder properties, find large files, and compare duplicate file contents.
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

Inter font: [SIL Open Font License](assets/fonts/LICENSE.txt). Local Trash library patch: [notes and license](vendor/trash/DISCO-PATCH.md).
