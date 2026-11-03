# Changelog

Release history for disco, including earlier development checkpoints.

## 0.2.3-unstable — in development

- **Per-root scan cache:** independent snapshots preserve broad scans after narrower/unrelated scans. Category/Home navigation loads a matching workspace asynchronously. The legacy last snapshot is archived before replacement; profile writers are serialized and cancelled scans never replace complete entries. No cross-profile imports or automatic merging of independent scan dates.

- **Language servers:** opt-in settings for system clangd and managed Rust/Python/JavaScript/TypeScript servers. Automatic/manual completion, hover, diagnostics, shared background sessions, explicit install/update/rollback/remove and resource information. See the [illustrated setup guide](docs/language-servers.md).
- **Function help:** clangd groups overloads into one completion. Accept the name and type `(` to inspect signatures with ‹/› or Alt+Left/Right; Ctrl+Shift+Space opens parameter help manually. Descriptions and current-argument highlighting float over the document. Stable list scrolling and readable Doxygen descriptions replace jumping rows and raw tags.
- **Direct paths:** `disco file.txt` opens a standalone document without scanning; `disco /path/to/project` opens and scans that folder as a workspace. Ctrl+L edits the path with asynchronous file/folder suggestions. Direct-file windows retain the category rail; Home and breadcrumbs reuse cached folders or list a single directory without recursively scanning.
- **File search:** selected folder/file parent → current directory/category roots → optional Home, advancing only without matches. The origin remains fixed while selecting results. Search includes unexpanded/unscanned directories; clearing an outside result reveals its folder without scanning.
- **Find in file:** Ctrl+F in Focus opens a floating, literal Unicode-aware search over current text, including drafts. Case sensitivity, match counts, highlighting, wrapping next/previous and F3 navigation. Closing clears the query by default; an Interface option retains it per open tab for the session. Overview and terminal key routing remain separate.
- **Tabs:** Ctrl+Tab / Ctrl+Shift+Tab follows visible right/left order with wrapping. Tree selections reuse one preview; clicking document/image content, editing, or Enter/Right in the tree keeps it. Drafts and per-tab state survive switching.
- **New documents:** Ctrl+T creates an in-memory tab. First Ctrl+S chooses a relative/absolute path, creates missing parents and rejects existing files. Cancel retains the draft; unsaved-close protection applies.
- **Editing:** action-based Undo/Redo gives paste, Enter, cut and selection replacement distinct steps. Ordinary typing/deletion can group; IME is one action. Save, tab changes and Markdown modes retain history.
- **External changes:** active documents follow terminal/agent edits and atomic saves independently of category refresh. Background reads retain visible text and unaffected syntax colors until ready; cursor/selection track changes where possible. Dirty drafts keep conflict protection. Loading terminal fonts no longer hides editor text.
- **Markdown:** native Read/Edit with one source buffer and undo history; headings, tables, lists, quotes, anchors, local images and lazy details/summary. Full-height wrapped code blocks have copy controls. Reading typography and colors follow the theme; remote images are not fetched.
- **Document controls:** unified toolbar and bottom zoom/status controls. Ctrl+Plus/Minus, Ctrl+wheel and reset with Ctrl+0; additive 10-point default step, configurable step and remappable reset. Each tab retains session zoom separately from the UI/sidebar.
- **Themes and icons:** five dark/four light presets, including pink-surfaced Soft Rose. Accent swatches, five visible theme rows, pinned Create, separate TOML color files and a robot prompt with schema/paths. Independent AI prompt language is optional. Theme-aware line branding, rounded taskbar backgrounds and optional bundled file-type icons for 59 families.
- **Interface:** compact 8 px tree indentation, independent 80–120% sidebar scale, and a new UI 100% baseline matching the former 85%. Compact title controls have optional labels. Shortcut recording waits for a main key, supports modifier combinations and rejects conflicts without losing valid bindings.
- **Cached scans:** categories above a narrow snapshot show known descendants and missing coverage. CLI-file category changes restore saved data asynchronously with loading indicators and an in-tree Scan action. Snapshot diagnostics explain the active profile/root; the [investigation](docs/investigations/cache-categories.md) documents one-last-scan replacement and separate profiles.
- **Packaging:** development installs are named disco and replace the old duplicate launcher. Docker/BuildKit builds native Linux bundles without host Rust/dev packages, with caches, checksums, dependency reports and CI artifacts.
- **Documentation:** reorganized English/Ukrainian guides, 34 current native screenshots and a ready-to-open C++ example. See [English](docs/guide.md) / [Українська](docs/guide.uk.md).

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
