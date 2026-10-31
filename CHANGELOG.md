# Changelog

Release history for disco, including earlier development checkpoints.

## 0.2.3-unstable — in development

- External file edits update the visible document without a blank/loading frame. The previous buffer stays on screen until the background read is ready, unchanged syntax colors remain stable while parsing catches up, and cursor selections track insertions/deletions above them. File sizes update in the existing tree instead of clearing its directory listing; edits made during loading remain protected.

- Opening the terminal no longer makes editor text disappear. Syntax formatting stays cached, while glyph layouts follow egui’s current font atlas after terminal fonts load or the atlas resets; document contents, selection and undo remain intact.

- The active file now follows external edits independently of category auto-refresh, scan snapshots or terminal focus. A bounded background metadata monitor catches normal writes, atomic replacements and file removal/recreation. Clean documents reload with cursor, focus and zoom preserved; local drafts remain intact with conflict protection, and changes arriving during Save are processed after completion.

- Choosing a category after opening a file from the CLI now restores the saved scan asynchronously, without rescanning or replacing the current draft. Later category choices reuse that snapshot. Empty file trees have a single scan action below wrapped explanatory text, and small loading indicators distinguish snapshot restoration, directory reads and scans. Late cache results cannot replace a newly opened path or an explicit scan.

- Markdown reading has its own typography: real bold headings, comfortable line spacing, theme-aware links/tables and a true monospace code face at physical pixel-aligned sizes. Code blocks expand to their full height with wrapped long lines and a vector copy/check control. Details/summary sections use lazy native disclosures; missing local images show their alt text. Read/Edit retains the source buffer, cursor and undo state.

- Document actions and the Markdown Read/Edit segment share a single toolbar, with compact controls in narrow inspectors. File zoom, Fit/reset and zoom buttons now sit in the bottom status bar beside truncated document metadata.

- Direct-file windows retain the category rail and existing scan snapshot. Home and breadcrumbs reuse known folders or browse an asynchronous, bounded single-directory listing without recursively scanning. Unmeasured folder sizes show a dash; Scan explicitly calculates sizes. Open tabs and drafts remain intact.

- File zoom uses additive 10 percentage-point steps, configurable in Interface settings. Ctrl+0 resets the active document to 100%; Reset file zoom is remappable in Shortcuts. Existing shortcut bindings survive migration, including an occupied Ctrl+0.

- Optional SVG language/file-type icons for 59 families, including C/C++, Rust, JSON/YAML/TOML, JS/TS, Java and Python. The setting defaults off, supports Save/Cancel, and retains generic fallbacks. Material Icon Theme assets and the MIT notice ship locally.

- Open a file or workspace from the command line. Ctrl+L edits the address with asynchronous file/folder suggestions; Ctrl+F focuses filename search. Direct files open without a category or disk scan, with unsaved-document protection.

- File zoom through Ctrl+Plus/Minus and Ctrl+wheel, independent of UI/sidebar scale. Each Focus tab retains its session zoom; text/Markdown support 50–300%, images 25–800% of Fit, with a percentage reset.

- Bundled category icons follow the theme text color in the sidebar and icon picker. Folders, images and documents use distinct open line symbols that stay legible on light, pink and dark surfaces; imported artwork keeps its original colors.

- Soft Rose: a gentle pink preset with distinctly pink backgrounds and panels, a dusty rose accent and warm dark text. New bundled presets are added to existing catalogs without changing the current selection or replacing custom colors.

- Docker/BuildKit native Linux bundle build with a pinned Rust/Debian builder, Cargo caches, user-owned export, checksums, runtime report and CI artifacts. Host Rust/dev packages are unnecessary.

- Markdown Read/Edit modes share one document, draft and undo history. Native cached rendering supports tables, lists, code highlighting, heading anchors and local images without automatic network fetches.
- AI prompts follow the UI language unless Separate AI prompt language is checked; category and theme prompts then use the independent Ukrainian/English selection.
- Tree clicks reuse one preview tab; clicking document/image content keeps it open. Development builds install under the disco name, replacing the old duplicate launcher.

- Theme catalog with five dark and four light presets, accent swatches, a five-row scrolling picker and a fixed Create new action. Custom themes stay in draft until Save. A robot button shows a UA/EN prompt with the full TOML schema, actual paths, catalog context and instructions for assistants without PC access. Legacy colors migrate without losing their original file.
- Compact 8 px tree indentation, width-aware ellipsis and a narrower size column keep nested folders readable in Focus. The sidebar has its own 80–120% text scale. New UI 100% matches the old 85%; existing scales migrate once without changing their physical size.
- A minimal line disco-ball logo stays transparent throughout the app, with white or black lines selected for contrast against each surface. Window/taskbar icons keep a rounded dark/light background with transparent corners matching the theme; the installed launcher follows the saved theme. Both READMEs use the line logo and adapt to GitHub's light/dark color scheme.
- Sharp in-app logo rendering at physical pixel size.

- Ctrl+T opens an in-memory text tab. Its first Ctrl+S asks for a path relative to the folder selected at creation, or accepts a literal absolute path. Pasting, nested directories, conflict-safe first writes, tab switching and unsaved-close protection are supported. New tab is also available in the shortcut recorder.

- Added a Shortcuts settings section with key recording, collision checks, reset and KDE conflict hints. Local defaults: Ctrl+Alt+T terminal, Ctrl+Alt+F Focus, Ctrl+Alt+V Overview.

- Title-bar Overview, Focus and Terminal controls use compact icons by default. Interface settings can restore labels; spacing is shared with the window controls.

## 0.2.2 — 2026-10-29

- Trash operations now refresh only the Trash listing and disk-space sample, without rescanning the folder tree. Successful items and totals reconcile immediately; unaffected Trash directory sizes are reused. F5 refreshes Trash, and listing failures no longer report a known-empty bin or confuse restore errors with unreadable contents.

- Reorganized English and Ukrainian READMEs into quick starts and a native screenshot gallery; moved detailed controls and configuration to bilingual user guides, with a separate development guide.

- Fixed uncovered strips between the custom title bar and disk header: the root window, panel frames and renderer clear color are now opaque, preventing desktop bleed-through during repaints and layout changes.

- Overview terminal keyboard navigation follows its bottom docking: Ctrl+Alt+Down from categories or files enters the terminal, Up returns to files and Right enters properties. Left/Down at terminal edges stay in place and do not send escape sequences to the shell.

- Overview docks the terminal below the category rail and file list while keeping properties at full height. The lower panel has a draggable height divider; switching layouts preserves the same PTY session, and hide/close restores the browsing area.

- Focus includes a lazy, resizable terminal column with a title-bar toggle. Hiding retains the PTY; closing kills its shell and session jobs, frees its buffers and returns the column to the document. Keyboard input is isolated from file actions; colors and system symbol fonts follow the tested native prototype.

## 0.2.1 — 2026-10-28

- Keyboard file navigation reuses a single italic preview tab. Enter or Right first retains it; subsequent Enter launches an existing file association and subsequent Right focuses the document. Editing retains the draft automatically; clean replaced previews release their undo state.

- Keyboard workspace navigation uses Ctrl+Alt+arrows and F6/Shift+F6 with visible focus; categories and tabs have arrow/Enter navigation, category reordering respects the lock, and Ctrl+L/Ctrl+comma/Ctrl+W expose search, Settings and safe tab close. Standard editor Ctrl+arrows remain intact.

- Focus file tabs retain separate drafts, cursor and scroll positions across categories; inactive documents keep their original policies and background auto-save. Tab/app closing checks all unsaved documents.

- Settings use one separate native window with left-hand sections, resizing, Save/Cancel and safe restoration of the main editor context.

- Optional per-category auto-save after one second without typing; background writes preserve focus, read-only rules and conflicting drafts. Saved/unsaved/saving states are visible in the editor.

## 0.2.0 — 2026-10-28

- Drag categories to reorder them with a visible insertion target; persist the order without rewriting individual category documents. Optional category-order lock in Interface settings; Trash stays fixed.
- Ctrl+Tab / Ctrl+Shift+Tab preview adjacent visible files, skipping folders and preserving unsaved-change confirmation; editor input no longer treats these shortcuts as indentation.
- Clicking anywhere below a short document, including its empty gutter, focuses the editor and places the cursor at EOF.

- Scoped refresh merges only the current folder or category roots into the existing snapshot; selection, navigation and unsaved text remain intact.
- Per-category live file updates use native notifications, bounded queues and targeted directory reads. Monitoring is active only for the selected scope.
- Per-category read-only mode blocks editing, saving, creation and deletion inside disco.
- Cached background syntax highlighting with syntect and bat/two-face grammars, including TOML and TypeScript.
- Configurable Home (`/home`, with `/` fallback), real file/folder search in unopened directories, and optional Home fallback after an empty scope search.
- Compact Focus layout: full-width breadcrumbs/search instead of the SSD header, plus a small status bar without storage selection counts.
- Replaced the former global SSD usage refresh timer with category-level file events.

Consolidates the changes developed during the preview.

## 0.11.1 — earlier development checkpoint

- Two consistent layouts: **Overview** for sizes, properties and previews, and **Focus** for a document with a resizable folder tree.
- Compact category icons by default, custom categories, a bundled vector icon picker and imported icons.
- A shared category TOML index with a separate file for each category; migration backups, external-edit detection and localized agent prompts in category settings.
- Clickable breadcrumbs, responsive navigation controls and search scoped to the folder entered from a result. Clearing a query reveals the selected item.
- Sorting arrows for both Name and Size, consistent preview buttons and a single preview status line.
- Resizable properties pane and contained document scrolling, with panel size saved to settings.
- Text-editor focus survives saving; typing can continue during a background save, with newer changes retained as an unsaved draft.
- Native clipboard shortcuts on Wayland and X11.
- New-file creation through **+** or **Ctrl+N**, arbitrary extensions and nested paths such as `/new_folder/file.txt` relative to the selected folder. Existing files are preserved and invalid input retains dialog focus.

## 0.11.0 — text editing and navigation

- Edit complete UTF-8 and UTF-16 text files directly in previews.
- Explicit atomic saves preserving encoding, line endings, permissions and extended attributes, with conflict detection and unsaved-change confirmations.
- Keyboard navigation, opening with the default application, confirmed deletion and mouse side-button navigation.
- Larger portrait-image previews and improved preview controls.

## 0.10.0 — file previews

- Image and text previews with switchable layouts.
- Image fitting and zooming, bounded background decoding and explanatory states for unsupported or oversized files.
- Optional opening of files in their associated application on double click.

## 0.9.2 — internal structure

- Split application state, models and interface code into focused modules to make ongoing changes easier to maintain.

## 0.9.0 — localization and storage summary

- Ukrainian and English interfaces with language selection and bilingual documentation.
- A separate Trash segment and item count in the drive usage summary.

## 0.8.0 — initial published source

- Native Linux disk scanning, folder and category sizes, large files and duplicate analysis.
- Saved scan snapshots and conservative reuse when changes can be tracked.
- Confirmed cleanup, a Trash view, restoration and confirmed permanent removal.
- Dark themes, configurable accent colors and layout preferences, with vector interface icons.

---

## Українською

**0.2.2:** кошик оновлює лише свій скоуп після видалення чи відновлення, без пересканування дерева; F5 працює в кошику, успішні видалення одразу прибираються зі списку, а помилка читання не позначає його порожнім. Також вбудований кольоровий термінал праворуч у «Фокусі» та знизу в «Огляді» зі збереженням сесії між режимами. Згортання залишає процеси, хрестик завершує їх; розміри панелей змінюються. Виправлено маршрути Ctrl+Alt+стрілок і введення escape-послідовностей замість переходу. Верхня панель і фон вікна непрозорі, щоб робочий стіл не просвічувався через смужку. README скорочено, додано двомовні посібники й нативні скріншоти.

**0.2.1:** тимчасова вкладка при переборі файлів, закріплення через Enter/→, окремі чернетки й курсори вкладок, автозбереження, окреме вікно налаштувань і повна клавіатурна навігація.

**0.2.0:** перетягування категорій зі збереженням порядку, опція закріплення, Ctrl+Tab / Ctrl+Shift+Tab між видимими файлами та встановлення курсора в кінець файла при кліку нижче тексту. Також оновлення поточного scope, файлові події й режим лише читання для категорій, фонове підсвічування коду, налаштовуваний Home та пошук файлів у нерозгорнутих папках. У «Фокусі» шлях і пошук замінили смугу SSD.

**Попередні напрацювання (0.11.1):** режими «Огляд» і «Фокус», власні категорії та іконки, окремі TOML для категорій, локалізовані промпти, клікабельний шлях, пошук у вибраній папці, збереження фокусу редактора, системний буфер обміну та створення файлів разом із вкладеними папками. Помилки вводу залишають вікно створення відкритим і повертають фокус у поле.

**Раніше:** у 0.11.0 додали редагування тексту й клавіатурну навігацію; у 0.10.0 — прев’ю текстів і картинок; у 0.9.2 розділили код на модулі; у 0.9.0 додали англійську мову та відображення кошика на диску; у 0.8.0 опублікували початкову реалізацію аналізатора зі знімками сканування, категоріями, очищенням і кошиком.

Номер наступного релізу та перегрупування попередніх версій спершу узгоджуємо з автором.
