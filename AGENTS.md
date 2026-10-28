# Repository instructions

- Keep changes within the user's requested scope and preserve Ukrainian and English UI text together.
- The app has two viewing layouts, **Overview** and **Focus**. Use these names in UI, documentation and screenshots.
- Before changing a version number, adding a release tag or publishing a release, use the exact number explicitly agreed with the user. If no number has been agreed for that release, ask the user for it. Do not independently choose the size of a version bump or rewrite historical versions/tags.
- The user approved 0.2 (Cargo version 0.2.0) for scoped refresh, syntax highlighting, per-category live/read-only policies, configurable Home and the compact Focus toolbar.
- Verify relevant Rust tests, Python installation/translation checks and `git diff --check` before committing. Do not commit local binaries, configuration, scan caches or personal test fixtures.
