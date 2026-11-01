# DSC-017: headless LSP verification and resource sample

Measured on the development machine with the complete disco egui UI in an isolated profile and clangd 23.1.4. No native desktop window, xdotool, input injection or user project/config/cache was used. These are development-build **headless** measurements, not GPU/native-window memory or a universal performance guarantee.

The small project contains one C++ source with `<vector>`, a UTF-16-sensitive emoji before `std::vec`, and `compile_flags.txt`. The large project adds 500 translation units (100,000 functions) and an included header with 20,000 struct definitions. Default `--background-index=false --clang-tidy=false` flags remain in effect: the large fixture measures a heavy active translation unit, not indexing every source file.

For each scale the real application opens the file, starts clangd, receives diagnostics, accepts a completion and undoes/redoes it. After header indexing has been quiet for 3 s (up to 15 s warm-up), another 3 s idle sample runs. The harness still renders frames every 25 ms, so its disco CPU is more than an event-driven idle window would necessarily use. CPU time is cumulative parent / server time from `/proc`; percentages use one CPU as 100%. The machine reports 100 ticks/s. RSS sum may count shared pages twice; PSS and graphics allocations were not measured. Startup/initial indexing time is included in cumulative CPU, and server work can overlap disk/toolchain caches.

| Fixture | disco RSS MiB | clangd RSS MiB | RSS sum MiB | Cumulative CPU s, disco / clangd | Idle CPU, disco / clangd | Last tab close → server reaped |
| --- | ---: | ---: | ---: | --- | --- | --- |
| small | 39.8 | 239.5 | 279.3 | 0.14 / 1.00 | 1.32% / 0.00% | 30.20 s |
| large | 39.6 | 345.8 | 385.4 | 0.19 / 2.16 | 1.00% / 0.00% | 30.08 s |

Both server PIDs disappear under the production 30-second idle policy after the actual last-tab close action. The UI process remains around 40 MiB; it has no active LSP child afterward. Stop is also tested separately while preserving a modified draft.

## Reproducible checks

- `cargo test --locked` includes bounded framing, UTF-16/URI/text-edit handling, stale response/versioned diagnostic rejection, owned-only rollback/removal, cancellation, real clangd protocol, and the complete headless editor completion/Undo scenario.
- `lsp_real_editor_completion_is_one_undo_and_settings_are_opt_in` uses an isolated child process to avoid changing process-global profile variables. With an explicitly prepared `DISCO_LSP_UI_FIXTURE` profile, optional `DISCO_LSP_METRICS_FILE` writes the resource sample and additionally waits for production idle exit. Omit these variables for the normal self-contained regression.
- Explicit ignored `managed_official_artifacts_install_update_rollback_and_remove` requires `DISCO_LSP_INSTALL_FIXTURE` pointing to an absolute disposable directory containing `.diagnostic-copy`. It downloads the pinned official Rust/Python/TypeScript artifacts, verifies installation metadata, checks release metadata, installs the previous Rust release, rolls back, and rejects a deliberately incorrect hash while preserving the selected version. It leaves owned test installations for the next check.
- Explicit ignored `managed_language_servers_initialize_and_stop_offline` reuses those test artifacts, with `CARGO_NET_OFFLINE=true`. Rust, Pyright and TypeScript initialize and shut down without downloading packages; Pyright/TypeScript push diagnostics in the fixture. Rust's default check-on-save is disabled; the original semantic-error fixture did not produce compiler diagnostics. The normal completion/hover/diagnostic protocol acceptance is proven separately with real clangd.
- npm lock integrity and extra TypeScript installation were checked against the official pinned versions. Missing tools/platform, external ownership, failed digest, cancellation and rollback paths have independent checks. Cancellation reaps the owned command process group, and a stopped server acknowledges exit before managed rollback/removal begins.

The optional fixture roots and generated benchmark files are excluded from the repository. Protocol limits, capabilities and the unversioned-diagnostic limitation are documented in [the user guide](../language-servers.md).

## Українською

Заміри зроблені повним headless UI disco без desktop-вікон: малий C++ проєкт і великий із 500 файлами та заголовком із 20 000 структур. Таблиця показує окремо RAM disco/clangd, суму RSS, накопичений CPU, CPU після завершення індексації та час завершення сервера після закриття останньої вкладки. В обох випадках процес сервера зник приблизно через 30 с.

Це не замір OpenGL/GPU чи native-вікна; сума RSS може двічі рахувати спільні сторінки. Тест примусово рендерить близько 40 кадрів/с навіть у простої. Фоновий індекс clangd вимкнений, тому великі числа проєкту не означають індексацію всіх файлів. Для інших мов і проєктів витрати відрізнятимуться. Власні установки й перевірки виконувалися лише в одноразових тестових профілях.

## Follow-up: event-driven completion regression

The first automatic-completion test rendered extra frames between keys. That hid a race: a completion request could wake the stdio worker after its snapshot check, before the preceding `didChange` was applied, and be discarded as a future document version. The regression now types individual characters with 240 ms pauses **without intermediate UI frames**, including `std::` before a following `std::cout` statement. It checks an onscreen completion overlay, retained editor focus and Escape dismissal for ordinary C/C++ text and IME commit input. Before the correction, both the idle-loop case and IME case failed; both pass after applying queued document snapshots before requests and accepting committed IME text as a trigger. The mailbox lock is released before root/process work. Tests still open no native desktop windows.

## Follow-up: empty native IME events erased the popup

An instrumented Wayland session established a separate failure: clangd returned 100 items for the current document/request, but no completion overlay was painted. The editor received empty IME preedit/lifecycle events in the same UI pass. The pre-widget invalidation treated every `Ime` event as a text edit and erased the accepted result even when the text stayed unchanged. This was absent from both ordinary headless typing and the native X11 reproduction.

The input gate now ignores empty IME notifications, and post-widget invalidation checks actual buffer changes. Nonempty composition/commit and real edits still invalidate stale suggestions. The regression injects empty preedit events into the response-delivery frames; it failed before the fix and passes afterward. The user also confirmed the popup in the instrumented Wayland window. Popup rows now use a compact single-line layout instead of inheriting the main application's large button padding.

Native UI validation additionally uses a separate Xvfb display with software rendering and an isolated profile: actual key events type `std::` in the full C++ example before `return`; screenshots show the automatic menu and the Ctrl+Space menu. These checks do not send input to the user's desktop. The virtual server was extracted into disposable workspace storage from the signed official [Arch Xvfb package](https://archlinux.org/packages/extra/x86_64/xorg-server-xvfb/), without system installation.

For future investigations, `DISCO_LSP_TRACE=1 disco example.cpp 2>lsp-trace.log` enables opt-in breadcrumbs for input-event kinds, document versions, request IDs, result counts and overlay state. The trace does not record document contents or key contents; it is disabled by default.
