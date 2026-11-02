# Language servers / Мовні сервери

## English

Open **Settings → Language servers**, enable a server, and **Save**. All servers are disabled initially. `clangd` uses the installed system binary; Rust, Python and JavaScript/TypeScript also have explicit managed installation buttons. Installation does not enable a language. Node.js and npm are required for npm servers; `curl` and `sha256sum` are required for managed downloads. Missing tools, incompatible engines, unsupported platforms and network errors are reported without replacing the current installation.

In a saved text document, **Ctrl+Space** requests completion; typing at least two identifier characters or `.`, `::` and `->` triggers it automatically after a short pause. Use ↑/↓, Enter/Tab and Escape in the floating list. Hover over code for plain-text documentation. Diagnostics mark the affected lines and appear on hover; their count is in the bottom document status. Accepting a completion, including additional text edits, is one undoable action. It changes the draft, never writes the file itself. Existing optional auto-save remains applicable.

Servers start lazily for an enabled extension, not for a category or the entire disk. The nearest ancestor with a configured marker determines the project root; the file's parent is the fallback. Documents in one `(server, root)` share a process. Up to four projects run concurrently. Close their tabs to release a session after 30 seconds, or use **Stop / Restart**. Stop remains in effect until Restart, disabling/re-enabling the server, or restarting disco. Unsaved tabs connect only after their first save with a supported extension. Initialization and protocol IO run on background threads.

For C/C++, provide `compile_commands.json` or `compile_flags.txt` at the project root. Without either, clangd uses fallback flags; Settings and the document status indicate this. The default disables clangd's background index and clang-tidy. Rust's default disables build scripts, proc macros and check-on-save. These reduce automatic project work; compiler-quality Rust diagnostics and every third-party server feature are not promised by this initial client.

### Catalog and storage

The active profile's `lsp.toml` is the **only** disco LSP command catalog. For the installed development launcher this is normally `~/.config/disco-unstable/lsp.toml`; the normal profile uses `~/.config/disco/lsp.toml`. The path is shown in Settings. Edit this file with disco closed and restart to load changes. Project-local `lsp.toml` files are not read. A normal user-controlled server can still interpret its own project configuration.

The bundled [`assets/lsp.toml`](../assets/lsp.toml) is a complete example. `version = 1`; every `[servers.ID]` contains:

| Fields | Meaning |
| --- | --- |
| `name`, `enabled` | Display name and explicit opt-in |
| `extensions`, `languages` | Equally sized arrays mapping extensions to LSP language IDs |
| `command`, `args`, `root_markers` | Executable, arguments and nearest-root markers |
| `provider = "system"` | Existing executable, resolved through absolute PATH entries or an absolute executable path |
| `provider = "rustup"` | External `rustup run VERSION COMMAND`; `version` defaults to `stable` |
| `provider = "github_release"` | A standalone gzip binary: `repository`, pinned `version`, exact `asset`, `sha256`, `platform` (e.g. `linux-x86_64`) |
| `provider = "npm"` | Pinned `package`, `version`, `integrity` (SHA-512), optional pinned `extra_packages` |

Managed data lives at `$XDG_DATA_HOME/disco/lsp` (normally `~/.local/share/disco/lsp`), separate from scan caches. `state.toml` records the selected and previous versions, source, verified digest and size. Per-version `install.toml` retains the matching metadata for rollback. Settings shows the full managed installation size including Node dependencies; an external probe reports only the executable size. `system` and `rustup` installations are never updated or removed by disco.

**Check updates** explicitly contacts the official GitHub release API or npm registry and stages a pinned candidate in memory; **Update** installs that candidate. There is no automatic update check. Installation downloads into a temporary directory, checks the artifact digest (or npm lock integrity), uses npm `--ignore-scripts --engine-strict`, then atomically selects the new version. A failed or cancelled operation preserves the current version. The previous version remains available for rollback; older managed versions are cleaned up after a successful update. Removing a managed server waits for its sessions to stop and removes only its marked installation directory. It reports the freed size. Package actions take effect immediately; Save/Cancel applies to the enable checkboxes.

### Protocol boundary and verification

The client uses bounded stdio JSON-RPC, UTF-16 positions and full-document changes. Completion and hover responses are checked against path, document version and request generation; superseded requests are cancelled. Versioned diagnostics are rejected when stale. The LSP protocol permits unversioned diagnostic notifications: these are associated with the current open document, so a server that omits versions cannot provide the same strict stale-diagnostic guarantee. Drafts remain intact on server errors or Stop.

This first client supports plain-text completion edits (including additional edits), plain-text hover, completion documentation (including lazy `completionItem/resolve`), signature help and push diagnostics. It does not advertise snippets, arbitrary command execution, workspace edits, rename, formatting, code actions, a debugger or semantic tokens. Large documents retain disco's existing 128 KiB editor bound. `async-lsp` was evaluated; the current bounded stdio worker avoids adding an async runtime for this scope. See the [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/) and [async-lsp](https://github.com/oxalica/async-lsp).

Headless protocol/editor tests use real clangd without opening desktop windows. Optional explicit network tests install official Rust/Python/TypeScript artifacts in a disposable profile, exercise failed verification and rollback, and then initialize the installed servers offline. Resource measurements and their limits are in [the investigation](investigations/lsp-resources.md).

### Function overloads and parameter documentation

The default `clangd` entry adds `--completion-style=bundled` unless its arguments already explicitly select a completion style. clangd combines related overloads into one completion row; explicit `detailed` remains available in the user catalog. Other servers control their own grouping; disco does not merge unrelated symbols just because their names match.

The selected completion shows its detail and documentation below the list, in the same floating panel. If the server supports `completionItem/resolve`, missing documentation is requested after a short selection debounce. Stale replies are checked against document version and selection request; resolve never replaces the original insertion edits. Documentation comes from the server, often source/header comments, and may be absent. The initial display is bounded plain text, not fetched web documentation or generated AI text.

After `(` or `,`, parameter help shows available signatures, the current argument and any signature/parameter description. `Ctrl+Shift+Space` requests it manually. Use the `‹` / `›` buttons or `Alt+Left` / `Alt+Right` to inspect another overload. These shortcuts take priority over folder history only while the signature panel is active in the editor; ordinary Left/Right still move the cursor. Switching the displayed signature does not edit code or force the compiler's overload selection. The server may narrow the list as arguments are typed. Escape dismisses the panel. Both panels float over the editor without changing its viewport size.

## Українська

Відкрий **Налаштування → Мовні сервери**, увімкни потрібний сервер і натисни **Зберегти**. Типово всі вимкнені. `clangd` використовує системну установку. Для Rust, Python та JavaScript/TypeScript є окремі кнопки встановлення; встановлення саме по собі не вмикає мову. Для npm-серверів потрібні Node.js і npm, для завантажень — `curl` і `sha256sum`. Помилка мережі, відсутній runtime, несумісна версія Node або платформа залишають робочу установку.

У збереженому текстовому файлі **Ctrl+Space** відкриває підказки; набір щонайменше двох символів назви, `.`, `::` і `->` викликають їх після короткої паузи. ↑/↓ обирають, Enter/Tab приймають, Escape закриває. Список плаває над документом і не зсуває текст. Наведення показує довідку та повідомлення діагностики; кількість діагностик є внизу документа. Прийняття підказки разом із додатковими правками відкатується одним Undo. Зміни залишаються чернеткою; опційне автозбереження працює за наявними правилами.

Один процес обслуговує документи одного сервера й кореня проєкту. Корінь — найближчий предок із маркером із каталогу; інакше батьківська папка файла. Ліміт — чотири активні проєкти. Через 30 с без відкритих документів процес завершується. Є **Зупинити / Перезапустити**; після Stop сервер не стартує до Restart, повторного вимкнення/увімкнення чи перезапуску disco. Незбережені вкладки підключаються після першого збереження з відомим розширенням.

Для clangd бажаний `compile_commands.json` або `compile_flags.txt`. Їх відсутність показується в налаштуваннях і статусі документа. Типово фоновий індекс і clang-tidy вимкнені; для Rust вимкнені build scripts, proc macros і check-on-save. Цей початковий клієнт не обіцяє всі можливості повноцінного IDE або перевірку Rust компілятором.

Каталог — лише глобальний `lsp.toml` активного профілю; шлях видно в налаштуваннях. Для development-лаунчера зазвичай це `~/.config/disco-unstable/lsp.toml`. Редагуй із закритим disco та перезапусти. Проєктні каталоги disco не виконує, але сам мовний сервер може читати власні налаштування проєкту. Повний приклад — [`assets/lsp.toml`](../assets/lsp.toml), поля й провайдери описані в таблиці вище.

Власні пакети лежать у `$XDG_DATA_HOME/disco/lsp` (типово `~/.local/share/disco/lsp`), окремо від кешу сканувань. `state.toml` містить поточну/попередню версію, джерело, хеш і розмір; `install.toml` кожної версії зберігає її метадані для відкату. Системні й rustup-установки disco не видаляє й не оновлює.

Перевірка оновлень виконується лише кнопкою через офіційний GitHub API або npm. Нова версія спочатку завантажується й перевіряється, npm запускається без lifecycle scripts; перемикання відбувається після успіху. Збій чи скасування зберігає попередню робочу версію. Після оновлення доступний відкат; ще старіші власні версії прибираються. Видалення чекає зупинки сервера й прибирає тільки позначений власний каталог із показом звільненого обсягу. Дії з пакетами виконуються одразу, а Зберегти/Скасувати стосуються галочок увімкнення.

Completion і hover захищені перевіркою шляху, версії тексту та покоління запиту. Застарілі версійовані діагностики відкидаються. Якщо сервер не передає версію діагностики, протокол не дозволяє так само строго перевірити її актуальність. Підтримуються звичайні текстові підказки з додатковими правками, документація вибраної підказки (зокрема через `completionItem/resolve`), довідка про параметри й push-діагностика; snippets, команди, workspace edits, rename, formatting, code actions і semantic tokens не заявлені. Помилки сервера не змінюють чернетку.

Перевірки працюють без desktop-вікон. Реальні пакети тестуються в окремому профілі; [заміри ресурсів](investigations/lsp-resources.md) окремо пояснюють межі headless-вимірювання.

### Перевантаження й довідка про параметри

Типовий сервер `clangd` запускається з `--completion-style=bundled`, якщо в його аргументах ще не задано режим явно. Пов’язані перевантаження показуються однією підказкою; це групування сервера, а не злиття довільних однакових назв. Інші сервери визначають власне групування.

Під вибраною підказкою показуються тип/сигнатура й опис. Якщо сервер підтримує `completionItem/resolve`, відсутній опис довантажується після короткої паузи вибору. Застарілі відповіді відкидаються; результат resolve не підміняє правки вставлення. Джерело — сервер, зазвичай коментарі коду/заголовків; текст може бути відсутнім. Виведення обмежене звичайним текстом, без завантаження вебсторінок чи генерації опису AI.

Після `(` або `,` з’являються сигнатури, виділення поточного аргументу й документація функції/параметра. Ручний виклик — `Ctrl+Shift+Space`. Стрілки `‹` / `›` у панелі або `Alt+←` / `Alt+→` перемикають довідку між перевантаженнями. Ці комбінації мають пріоритет над історією папок лише в активній панелі підписів редактора; звичайні ←/→ рухають курсор. Вибір підпису не змінює код і не примушує компілятор вибрати певне перевантаження. Сервер уточнює варіанти за аргументами. Escape закриває панель; розмір області документа залишається незмінним.
