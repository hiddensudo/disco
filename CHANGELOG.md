# Changelog

Release history for disco, including earlier development checkpoints.

## Unreleased

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

**0.2.0:** перетягування категорій зі збереженням порядку, опція закріплення, Ctrl+Tab / Ctrl+Shift+Tab між видимими файлами та встановлення курсора в кінець файла при кліку нижче тексту. Також оновлення поточного scope, файлові події й режим лише читання для категорій, фонове підсвічування коду, налаштовуваний Home та пошук файлів у нерозгорнутих папках. У «Фокусі» шлях і пошук замінили смугу SSD.

**Попередні напрацювання (0.11.1):** режими «Огляд» і «Фокус», власні категорії та іконки, окремі TOML для категорій, локалізовані промпти, клікабельний шлях, пошук у вибраній папці, збереження фокусу редактора, системний буфер обміну та створення файлів разом із вкладеними папками. Помилки вводу залишають вікно створення відкритим і повертають фокус у поле.

**Раніше:** у 0.11.0 додали редагування тексту й клавіатурну навігацію; у 0.10.0 — прев’ю текстів і картинок; у 0.9.2 розділили код на модулі; у 0.9.0 додали англійську мову та відображення кошика на диску; у 0.8.0 опублікували початкову реалізацію аналізатора зі знімками сканування, категоріями, очищенням і кошиком.

Номер наступного релізу та перегрупування попередніх версій спершу узгоджуємо з автором.
