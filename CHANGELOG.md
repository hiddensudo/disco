# Changelog

The versions below follow existing development checkpoints. The next release number and any historical regrouping will be agreed with the maintainer before publishing.

## Unreleased — current development build 0.11.1

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

**Поточні зміни (збірка 0.11.1):** режими «Огляд» і «Фокус», власні категорії та іконки, окремі TOML для категорій, локалізовані промпти, клікабельний шлях, пошук у вибраній папці, збереження фокусу редактора, системний буфер обміну та створення файлів разом із вкладеними папками. Помилки вводу залишають вікно створення відкритим і повертають фокус у поле.

**Раніше:** у 0.11.0 додали редагування тексту й клавіатурну навігацію; у 0.10.0 — прев’ю текстів і картинок; у 0.9.2 розділили код на модулі; у 0.9.0 додали англійську мову та відображення кошика на диску; у 0.8.0 опублікували початкову реалізацію аналізатора зі знімками сканування, категоріями, очищенням і кошиком.

Номер наступного релізу та перегрупування попередніх версій спершу узгоджуємо з автором.
